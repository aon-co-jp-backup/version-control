//! `--fix`: turns an open RustSec finding into a concrete, verified change.
//!
//! Default is a plan only. `--apply` additionally needs `[fix] enabled = true` in the config (so the production
//! server, which must not build, never does it by accident). Work happens in a fresh clone of the repository's
//! `origin` (never in the working tree); it is committed locally, and pushed only with `--push` and a green test run.
use crate::config::{Config, Fix};
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
    /// `cargo update` of the lock file inside the existing semver requirements (security and bug-fix releases).
    LockRefresh,
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
        Kind::LockRefresh => "Cargo.lock を互換範囲で更新(cargo update)→ テスト".to_string(),
        Kind::Pin => "固定(.deps)の写し: 依存元リポジトリを直してから `scripts/update-deps.sh <名前>` で固定を最新にする".to_string(),
        Kind::Manual(why) => format!("自動対応不可({why}) — 手動で判断"),
    };
    format!("{} [{}] {} {}: {}", p.repo, p.sub, p.krate, p.advisory, what)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Mode {
    /// Commit in the work copy only.
    Local,
    /// Push the commit to the default branch (needs `allow_push`).
    Main,
    /// Push a new `vc/` branch and open a pull request (needs `allow_pr`).
    Pr,
}

impl Mode {
    fn env(self) -> &'static str {
        match self {
            Mode::Local => "none",
            Mode::Main => "main",
            Mode::Pr => "pr",
        }
    }
}

/// Decides the effective mode: a request is honoured only when the config allows it (never the other way round).
pub fn resolve_mode(push: bool, pr: bool, fix: &Fix) -> Mode {
    if pr && fix.allow_pr {
        Mode::Pr
    } else if push && fix.allow_push {
        Mode::Main
    } else {
        Mode::Local
    }
}

/// Repositories that get a lock refresh: those named in `lock_refresh` (`*` = all) whose origin tracks a root `Cargo.lock`.
pub fn plan_lock_refresh(dir: &Path, name: &str, fix: &Fix) -> Option<Plan> {
    if !fix.lock_refresh.iter().any(|n| n == "*" || n == name) {
        return None;
    }
    let git = |args: &[&str]| Command::new("git").arg("-C").arg(dir).args(args).output().ok().filter(|o| o.status.success());
    let origin = String::from_utf8_lossy(&git(&["remote", "get-url", "origin"])?.stdout).trim().to_string();
    let tracked = git(&["cat-file", "-e", "@{u}:Cargo.lock"]).is_some() || git(&["cat-file", "-e", "HEAD:Cargo.lock"]).is_some();
    tracked.then(|| Plan { repo: name.to_string(), origin, sub: ".".into(), krate: "*".into(), advisory: String::new(), kind: Kind::LockRefresh })
}

/// Runs the plan with up to `workers` repositories at the same time (each repository's items stay in order).
/// Returns the one-line RESULT of each applied item, in plan order.
pub fn apply(plans: &[Plan], mode: Mode, script: &Path, workers: usize, jobs: usize) -> Vec<String> {
    let mut groups: Vec<(String, Vec<&Plan>)> = Vec::new();
    for p in plans.iter().filter(|p| matches!(p.kind, Kind::Mitigate | Kind::Bump(_) | Kind::LockRefresh)) {
        match groups.iter_mut().find(|(r, _)| *r == p.repo) {
            Some((_, v)) => v.push(p),
            None => groups.push((p.repo.clone(), vec![p])),
        }
    }
    let queue = std::sync::Mutex::new(groups.iter().enumerate().collect::<Vec<_>>().into_iter());
    let results = std::sync::Mutex::new(vec![Vec::new(); groups.len()]);
    std::thread::scope(|s| {
        for _ in 0..workers.max(1) {
            s.spawn(|| loop {
                let next = queue.lock().unwrap().next();
                let Some((i, (_, items))) = next else { break };
                let lines: Vec<String> = items.iter().map(|p| run_one(p, mode, script, jobs)).collect();
                results.lock().unwrap()[i] = lines;
            });
        }
    });
    results.into_inner().unwrap().into_iter().flatten().collect()
}

fn run_one(p: &Plan, mode: Mode, script: &Path, jobs: usize) -> String {
    let (kind, ver) = match &p.kind {
        Kind::Mitigate => ("mitigate", String::new()),
        Kind::Bump(v) => ("bump", v.clone()),
        Kind::LockRefresh => ("lockrefresh", String::new()),
        _ => return String::new(),
    };
    let mut c = Command::new("bash");
    c.arg(script).args([&p.repo, &p.origin, &p.sub, &p.krate, kind]);
    if !ver.is_empty() {
        c.arg(&ver);
    }
    c.env("VC_MODE", mode.env()).env("CARGO_BUILD_JOBS", jobs.to_string());
    // The work copy gets its own folder per repository, so parallel runs never share a build directory.
    match c.output() {
        Ok(o) => {
            let text = String::from_utf8_lossy(&o.stdout);
            text.lines().rev().find(|l| l.starts_with("RESULT")).unwrap_or("RESULT: 出力なし").to_string()
        }
        Err(e) => format!("RESULT {}: 実行できない ({e})", p.repo),
    }
}

/// A RESULT line that means "this needs a human/Claude": failed tests, build errors, failed clone/update.
pub fn is_failure(line: &str) -> bool {
    ["test=FAIL", "test=BUILD-ERROR", "テスト不合格", "失敗", "実行できない", "未対応"].iter().any(|k| line.contains(k))
}

/// The path of the work copy named in a RESULT line (for the advisory change summary).
pub fn work_copy(line: &str) -> Option<&str> {
    line.split("作業コピー: ").nth(1).map(|s| s.trim())
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

#[cfg(test)]
mod mode_tests {
    use super::*;

    #[test]
    fn mode_needs_config_permission() {
        let off = Fix::default();
        assert_eq!(resolve_mode(true, true, &off), Mode::Local);
        let push = Fix { allow_push: true, ..Default::default() };
        assert_eq!(resolve_mode(true, false, &push), Mode::Main);
        assert_eq!(resolve_mode(false, true, &push), Mode::Local);
        let pr = Fix { allow_pr: true, allow_push: true, ..Default::default() };
        assert_eq!(resolve_mode(true, true, &pr), Mode::Pr); // PR is the safer of the two
        assert_eq!(resolve_mode(false, false, &pr), Mode::Local);
    }

    #[test]
    fn failure_and_work_copy_detection() {
        assert!(is_failure("RESULT a: テスト不合格のため push しない test=FAIL 1 pass / 2 fail 作業コピー: /x"));
        assert!(is_failure("RESULT a: clone失敗"));
        assert!(!is_failure("RESULT a: 修正をコミット済み(未push) test=PASS 3 pass / 0 fail 作業コピー: /x"));
        assert_eq!(work_copy("RESULT a: ok 作業コピー: /home/u/.vc-fix/a"), Some("/home/u/.vc-fix/a"));
        assert_eq!(work_copy("RESULT a: ok"), None);
    }

    #[test]
    fn lock_refresh_only_for_listed_repos() {
        let f = Fix { lock_refresh: vec!["x".into()], ..Default::default() };
        assert!(plan_lock_refresh(Path::new("/nonexistent"), "y", &f).is_none());
        assert!(plan_lock_refresh(Path::new("/nonexistent"), "x", &f).is_none()); // no git there: nothing to do
    }
}

#[cfg(test)]
mod script_tests {
    use std::process::Command;

    /// End to end through the real script with a dependency-free local "origin": nothing to update -> "変更なし", nothing pushed.
    #[test]
    fn lockrefresh_without_changes_is_a_noop() {
        if Command::new("bash").arg("-c").arg("command -v cargo && command -v git").output().map_or(true, |o| !o.status.success()) {
            return; // no toolchain here
        }
        let d = std::env::temp_dir().join(format!("vc-fix-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let src = d.join("origin-src");
        std::fs::create_dir_all(src.join("src")).unwrap();
        std::fs::write(src.join("Cargo.toml"), "[package]\nname=\"t\"\nversion=\"0.1.0\"\nedition=\"2021\"\n").unwrap();
        std::fs::write(src.join("src/lib.rs"), "").unwrap();
        let sh = |cmd: &str, dir: &std::path::Path| {
            let o = Command::new("bash").arg("-c").arg(cmd).current_dir(dir).output().unwrap();
            assert!(o.status.success(), "{cmd}: {}", String::from_utf8_lossy(&o.stderr));
            String::from_utf8_lossy(&o.stdout).to_string()
        };
        sh("git init -q . && cargo generate-lockfile -q && git add -A && git -c user.name=t -c user.email=t@t commit -qm init", &src);
        let script = std::env::current_dir().unwrap().join("scripts/remediate.sh");
        let out = sh(&format!("VC_FIX_DIR={} bash {} t {} . '*' lockrefresh", d.join("work").display(), script.display(), src.display()), &d);
        assert!(out.contains("RESULT t: 変更なし"), "{out}");
        let _ = std::fs::remove_dir_all(&d);
    }
}
