//! RustSec advisory scan: compares the versions locked in every `Cargo.lock` with the advisory-db.
//! The database is a shallow clone of https://github.com/RustSec/advisory-db that is refreshed on each run.
use semver::{Version, VersionReq};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, Clone)]
pub struct Advisory {
    pub id: String,
    pub title: String,
    /// "" for vulnerabilities, otherwise "unmaintained" | "unsound" | "notice".
    pub informational: String,
    patched: Vec<VersionReq>,
    unaffected: Vec<VersionReq>,
    pub patched_text: Vec<String>,
}

impl Advisory {
    pub fn affects(&self, v: &Version) -> bool {
        // semver's `matches` never matches a pre-release (5.0.0-rc.1) against `>= 4.1.3`, which made every
        // release candidate look vulnerable. Also try the version without its pre-release tag (RustSec treats it as newer).
        let core = Version::new(v.major, v.minor, v.patch);
        !self.patched.iter().chain(self.unaffected.iter()).any(|r| r.matches(v) || (!v.pre.is_empty() && r.matches(&core)))
    }
}

pub type Db = HashMap<String, Vec<Advisory>>;

/// Clones or fast-forwards the advisory-db checkout. Returns an error string if it is unusable.
pub fn refresh(dir: &Path, url: &str) -> Result<(), String> {
    let git = |args: &[&str], cwd: Option<&Path>| {
        let mut c = Command::new("git");
        if let Some(d) = cwd {
            c.arg("-C").arg(d);
        }
        c.args(args).env("GIT_TERMINAL_PROMPT", "0").stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped()).output()
    };
    let out = if dir.join(".git").exists() {
        git(&["pull", "--ff-only", "--quiet", "--depth", "1"], Some(dir))
    } else {
        git(&["clone", "--quiet", "--depth", "1", url, &dir.to_string_lossy()], None)
    };
    match out {
        Ok(o) if o.status.success() => Ok(()),
        // An older copy still gives an answer; only fail when there is nothing at all.
        Ok(o) if dir.join("crates").exists() => Err(format!("更新失敗(古い写しで続行): {}", String::from_utf8_lossy(&o.stderr).trim())),
        Ok(o) => Err(String::from_utf8_lossy(&o.stderr).trim().to_string()),
        Err(e) => Err(e.to_string()),
    }
}

pub fn load(dir: &Path) -> Db {
    let mut db = Db::new();
    let Ok(crates) = std::fs::read_dir(dir.join("crates")) else { return db };
    for c in crates.flatten() {
        let Ok(files) = std::fs::read_dir(c.path()) else { continue };
        for f in files.flatten() {
            if let Ok(text) = std::fs::read_to_string(f.path()) {
                if let Some((pkg, adv)) = parse_advisory(&text) {
                    db.entry(pkg).or_default().push(adv);
                }
            }
        }
    }
    db
}

fn parse_advisory(text: &str) -> Option<(String, Advisory)> {
    let start = text.find("```toml")? + 7;
    let end = text[start..].find("```")? + start;
    let doc: toml::Table = toml::from_str(&text[start..end]).ok()?;
    let a = doc.get("advisory")?.as_table()?;
    if a.contains_key("withdrawn") {
        return None;
    }
    let heading = text[end..].lines().find_map(|l| l.strip_prefix("# ")).unwrap_or("").to_string();
    let s = |k: &str| a.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
    let versions = doc.get("versions").and_then(|v| v.as_table());
    let reqs = |k: &str| -> (Vec<VersionReq>, Vec<String>) {
        let list: Vec<String> = versions.and_then(|t| t.get(k)).and_then(|v| v.as_array()).map(|x| x.iter().filter_map(|e| e.as_str().map(String::from)).collect()).unwrap_or_default();
        (list.iter().filter_map(|r| VersionReq::parse(r).ok()).collect(), list)
    };
    let (patched, patched_text) = reqs("patched");
    let (unaffected, _) = reqs("unaffected");
    // Advisory files list unaffected versions only through `versions.unaffected`; target-specific ones are ignored (conservative).
    let adv = Advisory { id: s("id"), title: if s("title").is_empty() { heading } else { s("title") }, informational: s("informational"), patched, unaffected, patched_text };
    Some((s("package"), adv))
}

/// `Cargo.lock` files below a repository (same depth/skip rules as manifests).
pub fn find_locks(repo: &Path) -> Vec<PathBuf> {
    crate::scan::find_files(repo, "Cargo.lock")
}

/// (name, version) of every crates.io package locked in the file.
pub fn locked(path: &Path) -> Vec<(String, Version)> {
    let Ok(text) = std::fs::read_to_string(path) else { return vec![] };
    let Ok(doc) = toml::from_str::<toml::Table>(&text) else { return vec![] };
    doc.get("package")
        .and_then(|p| p.as_array())
        .into_iter()
        .flatten()
        .filter_map(|p| {
            let t = p.as_table()?;
            if !t.get("source")?.as_str()?.starts_with("registry+") {
                return None;
            }
            Some((t.get("name")?.as_str()?.to_string(), Version::parse(t.get("version")?.as_str()?).ok()?))
        })
        .collect()
}

pub struct Hit {
    pub krate: String,
    pub version: String,
    pub adv: Advisory,
    pub lock: String,
}

pub fn check(db: &Db, repo: &Path) -> Vec<Hit> {
    let mut hits = Vec::new();
    for lock in find_locks(repo) {
        let rel = lock.strip_prefix(repo).unwrap_or(&lock).to_string_lossy().replace('\\', "/");
        for (name, ver) in locked(&lock) {
            for adv in db.get(&name).into_iter().flatten() {
                if adv.affects(&ver) {
                    hits.push(Hit { krate: name.clone(), version: ver.to_string(), adv: adv.clone(), lock: rel.clone() });
                }
            }
        }
    }
    hits.sort_by(|a, b| (a.krate.as_str(), a.version.as_str(), a.adv.id.as_str()).cmp(&(b.krate.as_str(), b.version.as_str(), b.adv.id.as_str())));
    hits.dedup_by(|a, b| a.krate == b.krate && a.version == b.version && a.adv.id == b.adv.id);
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "```toml\n[advisory]\nid = \"RUSTSEC-2099-0001\"\npackage = \"demo\"\ntitle = \"t\"\n\n[versions]\npatched = [\">= 1.2.3\"]\nunaffected = [\"< 1.0.0\"]\n```\n# body";

    #[test]
    fn affects_only_the_vulnerable_range() {
        let (pkg, a) = parse_advisory(SAMPLE).unwrap();
        assert_eq!(pkg, "demo");
        assert!(a.affects(&Version::parse("1.1.0").unwrap()));
        assert!(!a.affects(&Version::parse("1.2.3").unwrap()));
        assert!(!a.affects(&Version::parse("0.9.0").unwrap()));
        // a release candidate of a newer major is not "vulnerable"
        assert!(!a.affects(&Version::parse("5.0.0-rc.1").unwrap()));
    }

    #[test]
    fn withdrawn_is_ignored() {
        let t = SAMPLE.replace("title = \"t\"", "title = \"t\"\nwithdrawn = \"2099-01-01\"");
        assert!(parse_advisory(&t).is_none());
    }
}

/// A locked package that replaces a vulnerable registry crate through a configured mitigation.
pub struct Mitigated {
    pub krate: String,
    pub version: String,
    pub advisory: String,
    pub source: String,
    pub caveat: String,
    pub lock: String,
}

/// (name, version, source) of every locked package that does *not* come from the registry.
fn locked_non_registry(path: &Path) -> Vec<(String, Version, String)> {
    let Ok(text) = std::fs::read_to_string(path) else { return vec![] };
    let Ok(doc) = toml::from_str::<toml::Table>(&text) else { return vec![] };
    doc.get("package")
        .and_then(|p| p.as_array())
        .into_iter()
        .flatten()
        .filter_map(|p| {
            let t = p.as_table()?;
            let src = t.get("source")?.as_str()?;
            if src.starts_with("registry+") {
                return None;
            }
            Some((t.get("name")?.as_str()?.to_string(), Version::parse(t.get("version")?.as_str()?).ok()?, src.to_string()))
        })
        .collect()
}

/// Lock entries that the advisory would hit if they came from the registry, but that come from a configured mitigation source.
pub fn mitigated(db: &Db, repo: &Path, mits: &[crate::config::Mitigation]) -> Vec<Mitigated> {
    let mut out = Vec::new();
    for lock in find_locks(repo) {
        let rel = lock.strip_prefix(repo).unwrap_or(&lock).to_string_lossy().replace('\\', "/");
        for (name, ver, src) in locked_non_registry(&lock) {
            for m in mits.iter().filter(|m| m.krate == name && src.starts_with(&m.source_prefix)) {
                if db.get(&name).into_iter().flatten().any(|a| a.id == m.advisory && a.affects(&ver)) {
                    out.push(Mitigated { krate: name.clone(), version: ver.to_string(), advisory: m.advisory.clone(), source: src.clone(), caveat: m.caveat.clone(), lock: rel.clone() });
                }
            }
        }
    }
    out
}
