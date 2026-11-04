//! `--fix`: turns an open RustSec finding into a concrete, verified change.
//!
//! Default is a plan only. `--apply` additionally needs `[fix] enabled = true` in the config (so the production
//! server, which must not build, never does it by accident). Work happens in a fresh clone of the repository's
//! `origin` (never in the working tree); it is committed locally, and pushed only with `--push` and a green test run.
use crate::config::Config;
use crate::report::RepoStatus;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    /// Replace the crate through a configured mitigation (e.g. rsa -> rsa-rs).
    Mitigate,
    /// A fixed release exists: `cargo update -p crate --precise <ver>`.
    Bump(String),
    /// The lock belongs to a pinned copy of another repository: update that pin, not this lock.
    Pin,
    /// Nothing safe can be done automatically.
    Manual(String),
}

#[derive(Debug, Clone)]
pub struct Plan {
    pub repo: String,
    pub origin: String,
    pub sub: String,
    pub krate: String,
    pub advisory: String,
    pub kind: Kind,
}

/// First lower bound of a patched requirement text such as `>=0.4.2` or `^1.2.3 / >=2.0.0` (conservative: first one).
fn first_version(patched: &str) -> Option<String> {
    let t = patched.trim_start_matches("修正:").trim();
    let first = t.split('/').next()?.trim();
    let v = first.trim_start_matches(|c: char| !c.is_ascii_digit());
    let ok = !v.is_empty() && v.chars().all(|c| c.is_ascii_digit() || c == '.' || c == '-' || c.is_ascii_alphabetic());
    ok.then(|| v.to_string())
}

pub fn plan(dir: &Path, rs: &RepoStatus, cfg: &Config) -> Vec<Plan> {
    let origin = Command::new("git").arg("-C").arg(dir).args(["remote", "get-url", "origin"]).output().ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    let mut out = Vec::new();
    for it in rs.items.iter().filter(|i| i.gap == "vuln") {
        let advisory = it.latest.split_whitespace().next().unwrap_or("").to_string();
        let sub = match it.file.rsplit_once('/') { Some((d, _)) => d.to_string(), None => ".".to_string() };
        let kind = if it.file.starts_with(".deps/") {
            Kind::Pin
        } else if origin.is_empty() {
            Kind::Manual("origin が無い".into())
        } else if it.latest.contains("修正版なし") {
            if cfg.advisory.mitigations.iter().any(|m| m.advisory == advisory && m.krate == it.name) {
                Kind::Mitigate
            } else {
                Kind::Manual("修正版も登録済みの緩和策も無い".into())
            }
        } else if let Some(v) = it.latest.split("修正:").nth(1).and_then(|s| s.split(" — ").next()).and_then(first_version) {
            Kind::Bump(v)
        } else {
            Kind::Manual("修正版の指定を解釈できない".into())
        };
        out.push(Plan { repo: rs.name.clone(), origin: origin.clone(), sub, krate: it.name.clone(), advisory, kind });
    }
    out.dedup_by(|a, b| a.sub == b.sub && a.krate == b.krate && a.advisory == b.advisory);
    out
}

pub fn describe(p: &Plan) -> String {
    let what = match &p.kind {
        Kind::Mitigate => "緩和策で差し替え(rsa-rs の [patch] を追加)".to_string(),
        Kind::Bump(v) => format!("{} を {} に更新(cargo update --precise)", p.krate, v),
        Kind::Pin => "固定(.deps)の写し: 依存元リポジトリを直してから `scripts/update-deps.sh <名前>` で固定を最新にする".to_string(),
        Kind::Manual(why) => format!("自動対応不可({why}) — 手動で判断"),
    };
    format!("{} [{}] {} {}: {}", p.repo, p.sub, p.krate, p.advisory, what)
}

/// Runs the plan. Returns the one-line RESULT of each applied item.
pub fn apply(plans: &[Plan], push: bool, script: &Path) -> Vec<String> {
    let mut results = Vec::new();
    for p in plans {
        let (kind, ver) = match &p.kind {
            Kind::Mitigate => ("mitigate", String::new()),
            Kind::Bump(v) => ("bump", v.clone()),
            _ => continue,
        };
        let mut c = Command::new("bash");
        c.arg(script).args([&p.repo, &p.origin, &p.sub, &p.krate, kind]);
        if !ver.is_empty() {
            c.arg(&ver);
        }
        if push {
            c.arg("--push");
        }
        match c.output() {
            Ok(o) => {
                let text = String::from_utf8_lossy(&o.stdout);
                let line = text.lines().rev().find(|l| l.starts_with("RESULT")).unwrap_or("RESULT: 出力なし").to_string();
                results.push(line);
            }
            Err(e) => results.push(format!("RESULT {}: 実行できない ({e})", p.repo)),
        }
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_first_patched_bound() {
        assert_eq!(first_version(">=0.4.2").as_deref(), Some("0.4.2"));
        assert_eq!(first_version("修正: ^1.2.3 / >=2.0.0").as_deref(), Some("1.2.3"));
        assert_eq!(first_version("").as_deref(), None);
    }
}
