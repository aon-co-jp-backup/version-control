//! Hardware / compute-resource detection and task routing for `--fix`.
//!
//! Honest scope: compiling and testing (`cargo`) is CPU/disk/RAM-bound. GPUs (open-cuda / open-directx) cannot
//! speed it up, so they are only *reported*; the machine's cores and memory decide how many repositories are
//! verified in parallel. aruaru-llm (when it answers) is used for a short, advisory change summary only; the
//! decision to commit/push is always the test result.
use crate::config::{Config, Fix};
use crate::report::MaintainerStatus;
use std::process::Command;
use std::time::Duration;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Profile {
    pub os: String,
    pub cores: usize,
    /// Available memory in MiB (None when it cannot be read).
    pub mem_mb: Option<u64>,
    pub gpus: Vec<String>,
    /// Maintainer name -> answering.
    pub tools: Vec<(String, bool)>,
    pub llm_up: bool,
    pub llm_runtime: String,
    pub llm_accelerators: String,
}

pub fn parse_meminfo(text: &str) -> Option<u64> {
    let line = text.lines().find(|l| l.starts_with("MemAvailable:"))?;
    let kb: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kb / 1024)
}

fn mem_available_mb() -> Option<u64> {
    parse_meminfo(&std::fs::read_to_string("/proc/meminfo").ok()?)
}

pub fn parse_gpus(csv: &str) -> Vec<String> {
    csv.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).map(|l| l.to_string()).collect()
}

fn gpus() -> Vec<String> {
    Command::new("nvidia-smi")
        .args(["--query-gpu=name,memory.total", "--format=csv,noheader"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| parse_gpus(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or_default()
}

fn connect(http: &reqwest::blocking::Client, base: &str, method: &str) -> Option<String> {
    let url = format!("{}/aruaru_llm.v1.Llm/{}", base.trim_end_matches('/'), method);
    let r = http.post(url).json(&serde_json::json!({})).send().ok()?;
    if !r.status().is_success() {
        return None;
    }
    Some(r.text().ok()?.chars().take(300).collect())
}

pub fn detect(cfg: &Config, maintainers: &[MaintainerStatus]) -> Profile {
    let mut p = Profile {
        os: std::env::consts::OS.to_string(),
        cores: std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1),
        mem_mb: mem_available_mb(),
        gpus: gpus(),
        tools: maintainers.iter().map(|m| (m.name.clone(), m.alive)).collect(),
        ..Default::default()
    };
    if let Ok(http) = reqwest::blocking::Client::builder().timeout(Duration::from_secs(5)).build() {
        if let Some(r) = connect(&http, &cfg.apiex.llm_url, "Runtime") {
            p.llm_up = true;
            p.llm_runtime = r;
            p.llm_accelerators = connect(&http, &cfg.apiex.llm_url, "Accelerators").unwrap_or_default();
        }
    }
    p
}

/// How many repositories may be built/tested at the same time: about one per 2 cores and per 3 GiB of RAM,
/// at least 1, never more than `cap` (config `max_workers`, 0 = automatic with an upper bound of 8).
pub fn workers(p: &Profile, fix: &Fix) -> usize {
    let by_cpu = (p.cores / 2).max(1);
    let by_mem = p.mem_mb.map(|m| (m / 3072) as usize).unwrap_or(1).max(1);
    let auto = by_cpu.min(by_mem).clamp(1, 8);
    if fix.max_workers > 0 { fix.max_workers.min(auto.max(1)).max(1) } else { auto }
}

/// `CARGO_BUILD_JOBS` for each of `workers` parallel builds.
pub fn jobs_per_worker(p: &Profile, workers: usize) -> usize {
    (p.cores / workers.max(1)).max(1)
}

pub fn render(p: &Profile, fix: &Fix) -> String {
    let w = workers(p, fix);
    let mut s = format!(
        "hardware: os={} cores={} mem_available={} gpus={}\n",
        p.os,
        p.cores,
        p.mem_mb.map(|m| format!("{m} MiB")).unwrap_or_else(|| "unknown".into()),
        if p.gpus.is_empty() { "none detected".to_string() } else { p.gpus.join("; ") }
    );
    s += &format!("tools   : {}\n", p.tools.iter().map(|(n, a)| format!("{n}={}", if *a { "up" } else { "down" })).collect::<Vec<_>>().join(", "));
    s += &format!("aruaru-llm: {}\n", if p.llm_up { format!("up (Runtime: {})", p.llm_runtime.replace('\n', " ")) } else { "down (no change summaries)".into() });
    s += &format!(
        "routing : build/test -> CPU, {} repo(s) in parallel x {} job(s) each; change summary -> {}; GPU (open-cuda/open-directx) is not used for cargo\n",
        w,
        jobs_per_worker(p, w),
        if p.llm_up { "aruaru-llm (advisory only)" } else { "skipped" }
    );
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prof(cores: usize, mem: Option<u64>) -> Profile {
        Profile { cores, mem_mb: mem, ..Default::default() }
    }

    #[test]
    fn meminfo_parses() {
        assert_eq!(parse_meminfo("MemTotal: 1 kB\nMemAvailable:   8388608 kB\n"), Some(8192));
        assert_eq!(parse_meminfo("nothing"), None);
    }

    #[test]
    fn gpus_parse() {
        assert_eq!(parse_gpus("NVIDIA RTX, 8192 MiB\n\n"), vec!["NVIDIA RTX, 8192 MiB".to_string()]);
        assert!(parse_gpus("").is_empty());
    }

    #[test]
    fn workers_follow_cpu_and_memory() {
        let f = Fix::default();
        assert_eq!(workers(&prof(16, Some(64_000)), &f), 8);
        assert_eq!(workers(&prof(16, Some(6_200)), &f), 2); // memory bound
        assert_eq!(workers(&prof(1, Some(1_000)), &f), 1);
        assert_eq!(workers(&prof(16, None), &f), 1); // unknown memory: be careful
    }

    #[test]
    fn config_cap_never_exceeds_auto() {
        let f = Fix { max_workers: 3, ..Default::default() };
        assert_eq!(workers(&prof(16, Some(64_000)), &f), 3);
        assert_eq!(workers(&prof(4, Some(64_000)), &f), 2);
    }

    #[test]
    fn jobs_divide_cores() {
        assert_eq!(jobs_per_worker(&prof(16, None), 4), 4);
        assert_eq!(jobs_per_worker(&prof(2, None), 4), 1);
    }
}

/// One advisory sentence about the last commit of a work copy, from aruaru-llm. Never decides anything.
pub fn summarize(cfg: &Config, work_copy: &str) -> Option<String> {
    let stat = Command::new("git").arg("-C").arg(work_copy).args(["show", "--stat", "--format=%s", "HEAD"]).output().ok().filter(|o| o.status.success())?;
    let stat: String = String::from_utf8_lossy(&stat.stdout).chars().take(600).collect();
    let http = reqwest::blocking::Client::builder().timeout(Duration::from_secs(cfg.apiex.llm_timeout_secs)).build().ok()?;
    let url = format!("{}/aruaru_llm.v1.Llm/Generate", cfg.apiex.llm_url.trim_end_matches('/'));
    let r = http.post(url).json(&serde_json::json!({"prompt": format!("Dependency update commit: {stat}\nOne-sentence summary:"), "max_new_tokens": 48})).send().ok()?;
    if !r.status().is_success() {
        return None;
    }
    use sonic_rs::JsonValueTrait;
    let v = rust_json::parse_secure_sonic(&r.text().ok()?, &rust_json::Limits::default()).ok()?;
    let t = v.get("completion").and_then(|x| x.as_str())?.trim().chars().take(240).collect::<String>();
    (!t.is_empty()).then_some(t)
}
