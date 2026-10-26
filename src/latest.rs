//! Latest-version lookups (crates.io / npm / packagist) with an on-disk cache.
//! Every response is untrusted external JSON and is read with `rust_json::parse_secure`.
use crate::scan::Eco;
use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Default, Serialize, Deserialize)]
pub struct Cache {
    /// "registry:name" -> (unix seconds, latest version, or "" when the registry has no such package)
    pub entries: HashMap<String, (u64, String)>,
}

pub struct Registry {
    http: reqwest::blocking::Client,
    pub cache: Cache,
    ttl: u64,
    last_crates_call: Option<std::time::Instant>,
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

impl Registry {
    pub fn new(cache: Cache, ttl_hours: u64) -> Result<Self> {
        let http = reqwest::blocking::Client::builder()
            .user_agent("version-control (https://github.com/aon-co-jp/version-control)")
            .timeout(Duration::from_secs(30))
            .build()?;
        Ok(Self { http, cache, ttl: ttl_hours * 3600, last_crates_call: None })
    }

    /// `Ok(None)`: the registry does not know the package (private / renamed / removed).
    pub fn latest(&mut self, eco: Eco, name: &str) -> Result<Option<String>> {
        let key = format!("{}:{}", eco.label(), name);
        if let Some((ts, v)) = self.cache.entries.get(&key) {
            if now().saturating_sub(*ts) < self.ttl {
                return Ok(if v.is_empty() { None } else { Some(v.clone()) });
            }
        }
        let found = self.fetch(eco, name).with_context(|| key.clone())?;
        self.cache.entries.insert(key, (now(), found.clone().unwrap_or_default()));
        Ok(found)
    }

    fn get(&self, url: &str) -> Result<Option<serde_json::Value>> {
        let resp = self.http.get(url).send()?;
        if resp.status().as_u16() == 404 {
            return Ok(None);
        }
        if !resp.status().is_success() {
            return Err(anyhow!("HTTP {} from {}", resp.status(), url));
        }
        let text = resp.text()?;
        let v = rust_json::parse_secure(&text, &rust_json::Limits::default()).map_err(|e| anyhow!("bad JSON from {url}: {e}"))?;
        Ok(Some(v))
    }

    fn fetch(&mut self, eco: Eco, name: &str) -> Result<Option<String>> {
        match eco {
            Eco::Cargo => {
                // crates.io asks for at most one request per second.
                if let Some(t) = self.last_crates_call {
                    let min = Duration::from_millis(1100);
                    if t.elapsed() < min {
                        std::thread::sleep(min - t.elapsed());
                    }
                }
                let r = self.get(&format!("https://crates.io/api/v1/crates/{name}"));
                self.last_crates_call = Some(std::time::Instant::now());
                Ok(r?.and_then(|v| {
                    let c = v.get("crate")?;
                    c.get("max_stable_version").or_else(|| c.get("max_version"))?.as_str().map(String::from)
                }))
            }
            Eco::Npm => {
                let enc = name.replace('/', "%2F");
                Ok(self.get(&format!("https://registry.npmjs.org/{enc}/latest"))?.and_then(|v| v.get("version")?.as_str().map(String::from)))
            }
            Eco::Composer => {
                let v = self.get(&format!("https://repo.packagist.org/p2/{name}.json"))?;
                Ok(v.and_then(|v| {
                    v.get("packages")?.get(name)?.as_array()?.iter().find_map(|p| {
                        let ver = p.get("version")?.as_str()?;
                        (!ver.starts_with("dev-") && !ver.contains('-')).then(|| ver.trim_start_matches('v').to_string())
                    })
                }))
            }
        }
    }
}
