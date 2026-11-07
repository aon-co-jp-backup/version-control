//! version-control: checks every repository under a root for outdated dependencies and stale clones,
//! leaves routine bumps to the automatic maintainers (aruaru-llm / open-cuda side), and sends the message
//! to paste into Claude by e-mail and LINE for everything they cannot handle.
mod advisory;
mod apiex;
mod config;
mod fix;
mod hw;
mod latest;
mod autoloop;
mod notify;
mod report;
mod scan;

use anyhow::{Context, Result};
use config::Config;
use report::{Item, MaintainerStatus, RepoStatus, Status};
use scan::{classify, parse_req, parse_ver, Gap};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Default, Serialize, Deserialize)]
struct State {
    #[serde(default)]
    cache: latest::Cache,
    /// repository -> fingerprint of the last announced message
    #[serde(default)]
    notified: HashMap<String, String>,
    #[serde(default)]
    apiex: apiex::State,
}

struct Args {
    loop_run: bool,
    goal: Option<String>,
    config: PathBuf,
    dry_run: bool,
    force: bool,
    test_notify: bool,
    no_fetch: bool,
    demo: bool,
    no_api: bool,
    api_only: bool,
    fix: bool,
    apply: bool,
    push: bool,
    pr: bool,
    hw: bool,
    only: Option<String>,
}

fn parse_args() -> Args {
    let mut a = Args { loop_run: false, goal: None, config: PathBuf::from("config.toml"), dry_run: false, force: false, test_notify: false, no_fetch: false, demo: false, no_api: false, api_only: false, fix: false, apply: false, push: false, pr: false, hw: false, only: None };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--config" => a.config = it.next().map(PathBuf::from).unwrap_or(a.config),
            "--dry-run" => a.dry_run = true,
            "--force" => a.force = true,
            "--test-notify" => a.test_notify = true,
            "--no-fetch" => a.no_fetch = true,
            "--demo" => a.demo = true,
            "--no-api" => a.no_api = true,
            "--api-only" => a.api_only = true,
            "--fix" => a.fix = true,
            "--apply" => a.apply = true,
            "--push" => a.push = true,
            "--pr" => a.pr = true,
            "--hw" => a.hw = true,
            "--only" => a.only = it.next(),
            "--loop" => a.loop_run = true,
            "--goal" => a.goal = it.next(),
            "-h" | "--help" => {
                println!("version-control [--config FILE] [--dry-run] [--force] [--no-fetch] [--test-notify] [--demo]\n  --dry-run      scan and write the dashboard, send nothing\n  --force        announce even if the message was already sent\n  --test-notify  send a short test message by e-mail and LINE\n  --no-api       skip the API exception patrol
  --api-only     run only the API exception patrol (ledger, route counts, notice)
  --demo         write the sample status.json for the public demo (no scan)");
                std::process::exit(0);
            }
            other => eprintln!("ignoring unknown argument {other}"),
        }
    }
    a
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn main() -> Result<()> {
    let args = parse_args();
    if args.demo {
        let cfg_site = Config::load(&args.config).map(|c| c.site_dir).unwrap_or_else(|_| PathBuf::from("site"));
        return write_demo(&cfg_site.join("demo"));
    }
    let cfg = Config::load(&args.config)?;

    if args.test_notify {
        let o = notify::deliver(&cfg.notify, "[version-control] 通知の試験", "version-control の通知試験です。このメールが届けば経路は正常です。", &["version-control の通知試験です。このメッセージが届けばLINEの経路は正常です。".to_string()]);
        println!("e-mail: {}", o.email.as_ref().map(|_| "OK".to_string()).unwrap_or_else(|e| format!("NG ({e})")));
        println!("LINE  : {}", o.line.as_ref().map(|_| "OK".to_string()).unwrap_or_else(|e| format!("NG ({e})")));
        return Ok(());
    }

    let state_path = PathBuf::from("state.json");
    let mut state: State = std::fs::read_to_string(&state_path).ok().and_then(|t| sonic_rs::from_str(&t).ok()).unwrap_or_default();
    if args.api_only {
        run_apiex(&cfg, &mut state, &args);
        std::fs::write(&state_path, serde_json::to_string_pretty(&state)?).context("cannot write state.json")?;
        return Ok(());
    }
    let mut registry = latest::Registry::new(std::mem::take(&mut state.cache), cfg.policy.registry_cache_hours)?;

    let maintainers = check_maintainers(&cfg);
    let maintainer_alive = maintainers.iter().any(|m| m.alive);
    if args.hw {
        print!("{}", hw::render(&hw::detect(&cfg, &maintainers), &cfg.fix));
        return Ok(());
    }
    println!("maintainers: {}", maintainers.iter().map(|m| format!("{}={}", m.name, if m.alive { "up" } else { "DOWN" })).collect::<Vec<_>>().join(", "));

    let advisories = if cfg.advisory.enabled {
        if let Err(e) = advisory::refresh(&cfg.advisory.dir, &cfg.advisory.url) {
            eprintln!("advisory-db: {e}");
        }
        let db = advisory::load(&cfg.advisory.dir);
        println!("advisory-db: {} package(s) with advisories", db.len());
        Some(db)
    } else {
        None
    };

    let mut repos = Vec::new();
    for dir in list_repos(&cfg) {
        // Read-only: fetch, then look at the upstream branch's manifests / lock files (not the possibly stale working tree).
        let fetched = !args.no_fetch && cfg.policy.fetch_clones && git(&dir, &["fetch", "--quiet"], Duration::from_secs(60)).is_some();
        let snap = OriginSnap::new(&dir);
        if snap.path.is_none() || (!args.no_fetch && cfg.policy.fetch_clones && !fetched) {
            println!("  note: {} reads the clone as it is (no upstream, or fetch failed)", dir.file_name().unwrap_or_default().to_string_lossy());
        }
        let scan_dir = snap.dir(&dir);
        let mut rs = scan_repo(&dir, &scan_dir, &cfg, &mut registry, maintainer_alive);
        // A worktree copy whose `.git` points to a vanished directory has no upstream and can never be refreshed: say so.
        if git(&dir, &["rev-parse", "--git-dir"], Duration::from_secs(20)).is_none() {
            println!("  warn: {} has a broken .git (worktree source missing); its files may be stale", dir.file_name().unwrap_or_default().to_string_lossy());
            rs.items.push(report::Item {
                source: "clone".into(),
                name: "broken-git".into(),
                req: "gitdir missing".into(),
                latest: "worktree の元リポジトリが無く更新できない。持ち主が削除するか `git worktree repair` / 再 clone する(古い lock を脆弱と誤報しうる)".into(),
                gap: "error".into(),
                route: "claude".into(),
                file: ".git".into(),
            });
        }
        if let Some(db) = &advisories {
            add_advisories(&mut rs, &scan_dir, db, &cfg);
        }
        println!("{:<34} deps={:<4} open={:<3} to-claude={}", rs.name, rs.deps_checked, rs.items.len(), rs.items.iter().filter(|i| i.route == "claude").count());
        repos.push(rs);
    }
    state.cache = std::mem::take(&mut registry.cache);
    let mut fix_failures: Vec<String> = Vec::new();
    if args.fix {
        fix_failures = run_fix(&repos, &cfg, &args, &maintainers);
    }
    if args.loop_run {
        run_loop(&repos, &cfg, &args);
    }

    // Dashboard data: private repositories are left out entirely.
    let hidden = repos.iter().filter(|r| cfg.private.contains(&r.name)).count();
    let public: Vec<RepoStatus> = repos.iter().filter(|r| !cfg.private.contains(&r.name)).cloned().collect();
    let status = Status { generated_at: now(), demo: false, maintainers, repos: public, hidden_private: hidden };
    write_site(&cfg.site_dir, &status)?;

    // Notification: one block per repository that needs Claude and whose problem changed since the last announcement.
    let root = cfg.root.to_string_lossy().to_string();
    let mut blocks = Vec::new();
    let mut fresh: Vec<(String, String)> = Vec::new();
    for r in &repos {
        if r.claude_items().is_empty() {
            state.notified.remove(&r.name);
            continue;
        }
        let msg = report::claude_prompt(&root, r, cfg.policy.max_items_per_message);
        let fp = report::fingerprint(&msg);
        if args.force || state.notified.get(&r.name) != Some(&fp) {
            blocks.push(msg);
            fresh.push((r.name.clone(), fp));
        }
    }

    if !fix_failures.is_empty() {
        blocks.push(format!("{root} の自動修正(--fix)が失敗または未対応でした。原因を調べて直してください:
{}", fix_failures.iter().map(|l| format!("- {l}")).collect::<Vec<_>>().join("
")));
    }
    if blocks.is_empty() {
        println!("nothing new to announce");
    } else if args.dry_run {
        println!("--dry-run: {} message(s) would be sent:\n", blocks.len());
        for b in &blocks {
            println!("{b}\n");
        }
    } else {
        let subject = format!("[version-control] Claudeでの更新が必要: {}件のリポジトリ", blocks.len());
        let email_body = format!("次のメッセージをそのままClaude Codeに貼り付けてください。\n\n{}\n", blocks.join("\n\n----------------------------------------\n\n"));
        let mut line_blocks = vec![format!("【version-control】Claudeでの更新が必要: {}件", blocks.len())];
        line_blocks.extend(blocks.iter().cloned());
        let mut line_texts = notify::pack_for_line(&line_blocks, 4500);
        if line_texts.len() > 5 {
            line_texts.truncate(5);
            line_texts[4].push_str("\n\n(続きはメールを参照)");
        }
        let o = notify::deliver(&cfg.notify, &subject, &email_body, &line_texts);
        println!("e-mail: {}", o.email.as_ref().map(|_| "sent".to_string()).unwrap_or_else(|e| format!("FAILED ({e})")));
        println!("LINE  : {}", o.line.as_ref().map(|_| "sent".to_string()).unwrap_or_else(|e| format!("FAILED ({e})")));
        if o.any_ok() {
            // A failure of one channel is retried next run only if neither channel got through.
            for (n, fp) in fresh {
                state.notified.insert(n, fp);
            }
        } else {
            eprintln!("no channel delivered; the message will be retried next run");
        }
    }

    if !args.no_api {
        run_apiex(&cfg, &mut state, &args);
    }

    std::fs::write(&state_path, serde_json::to_string_pretty(&state)?).context("cannot write state.json")?;
    Ok(())
}

/// API exception ledger and patrol. Never fails the run: a problem here is printed and the dependency scan result stands.
fn run_apiex(cfg: &Config, state: &mut State, args: &Args) {
    if !cfg.apiex.enabled {
        return;
    }
    // --dry-run works on a scratch copy so that nothing (baseline, seen links, history) is recorded.
    let mut work = state.apiex.clone();
    let out = match apiex::patrol(cfg, &mut work, now(), true) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("api exceptions: {e:#}");
            return;
        }
    };
    if let Err(e) = std::fs::create_dir_all(&cfg.site_dir).and_then(|_| std::fs::write(cfg.site_dir.join("api-exceptions.json"), out.public.to_string())) {
        eprintln!("api exceptions: cannot write api-exceptions.json: {e}");
    }
    let http = reqwest::blocking::Client::builder().timeout(Duration::from_secs(30)).build().ok();
    for (key, kind, subject, body) in &out.messages {
        let fingerprint = report::fingerprint(&format!("{subject}
{body}"));
        if !args.force && work.notified.get(key) == Some(&fingerprint) {
            continue;
        }
        if args.dry_run {
            println!("--dry-run: notice [{kind}] {subject}
{body}
");
            continue;
        }
        let Some(h) = http.as_ref() else { continue };
        match apiex::send_notice(h, &cfg.apiex, kind, subject, body) {
            Ok(()) => {
                println!("api notice sent: {subject}");
                work.notified.insert(key.clone(), fingerprint);
                if key == "rest-increase" {
                    work.baseline.extend(out.held.clone());
                }
            }
            Err(e) => eprintln!("api notice NOT sent ({subject}): {e:#}"),
        }
    }
    println!("api exceptions: {} entries, REST-like routes now {}", out.public["exceptions"].as_array().map_or(0, |a| a.len()), out.public["routes"]["total_rest_like"]);
    if !args.dry_run {
        state.apiex = work;
    }
}

fn check_maintainers(cfg: &Config) -> Vec<MaintainerStatus> {
    let http = reqwest::blocking::Client::builder().timeout(Duration::from_secs(5)).build().ok();
    cfg.maintainers
        .iter()
        .map(|m| {
            let alive = !m.health_url.is_empty() && http.as_ref().is_some_and(|h| probe(h, &m.health_url));
            MaintainerStatus { name: m.name.clone(), alive }
        })
        .collect()
}

pub fn list_repos(cfg: &Config) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(&cfg.root)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.join(".git").exists() && !cfg.exclude.contains(&p.file_name().unwrap_or_default().to_string_lossy().to_string()))
        .collect();
    v.sort();
    v
}

/// The manifests and lock files of the upstream branch (`@{u}`), written to a temp folder with `git ls-tree` + `git show`
/// (read-only: nothing is pulled or merged). Removed on drop. `path` is None when there is no upstream.
struct OriginSnap {
    path: Option<PathBuf>,
}

impl OriginSnap {
    fn new(dir: &Path) -> Self {
        Self { path: Self::build(dir) }
    }

    fn dir(&self, fallback: &Path) -> PathBuf {
        self.path.clone().unwrap_or_else(|| fallback.to_path_buf())
    }

    fn build(dir: &Path) -> Option<PathBuf> {
        let list = git(dir, &["ls-tree", "-r", "--name-only", "-z", "@{u}"], Duration::from_secs(30))?;
        let root = std::env::temp_dir().join(format!("vc-origin-{}-{}", std::process::id(), dir.file_name()?.to_string_lossy()));
        let _ = std::fs::remove_dir_all(&root);
        for rel in list.split(' ').filter(|r| !r.is_empty()) {
            let comps: Vec<&str> = rel.split('/').collect();
            let name = comps.last().copied().unwrap_or("");
            if !["Cargo.toml", "Cargo.lock", "package.json", "composer.json"].contains(&name) || comps.len() > 6 || comps[..comps.len() - 1].iter().any(|c| scan::SKIP_DIRS.contains(c)) {
                continue;
            }
            let Some(text) = git(dir, &["show", &format!("@{{u}}:{rel}")], Duration::from_secs(20)) else { continue };
            let dest = root.join(rel);
            if let Some(parent) = dest.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(dest, text);
        }
        root.exists().then_some(root)
    }
}

impl Drop for OriginSnap {
    fn drop(&mut self) {
        if let Some(p) = &self.path {
            let _ = std::fs::remove_dir_all(p);
        }
    }
}

/// Runs git with a hard time limit so one unreachable remote cannot stall the whole scan.
pub(crate) fn git(dir: &Path, args: &[&str], limit: Duration) -> Option<String> {
    let mut child = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    // Drain stdout on a thread: a child that fills the pipe (a big Cargo.lock) would otherwise never exit.
    let mut pipe = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut out = String::new();
        std::io::Read::read_to_string(&mut pipe, &mut out).ok().map(|_| out)
    });
    let start = Instant::now();
    loop {
        match child.try_wait().ok()? {
            Some(st) => {
                let out = reader.join().ok()??;
                return if st.success() { Some(out) } else { None };
            }
            None if start.elapsed() > limit => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    }
}

fn scan_repo(dir: &Path, scan_dir: &Path, cfg: &Config, reg: &mut latest::Registry, maintainer_alive: bool) -> RepoStatus {
    let name = dir.file_name().unwrap_or_default().to_string_lossy().to_string();
    let mut rs = RepoStatus { name, ..Default::default() };

    // Stale or diverged clone. Never pulled automatically.
    if let Some(out) = git(dir, &["rev-list", "--left-right", "--count", "HEAD...@{u}"], Duration::from_secs(20)) {
        let mut it = out.split_whitespace().filter_map(|x| x.parse::<u32>().ok());
        let (ahead, behind) = (it.next().unwrap_or(0), it.next().unwrap_or(0));
        if behind > 0 {
            let diverged = ahead > 0;
            rs.items.push(Item {
                source: "clone".into(),
                name: "origin".into(),
                req: format!("ahead {ahead}"),
                latest: if diverged { format!("behind {behind}, ahead {ahead}") } else { behind.to_string() },
                gap: if diverged { "diverged" } else { "behind" }.into(),
                route: "claude".into(),
                file: String::new(),
            });
        }
    }

    // Dependencies.
    let mut seen: HashMap<(String, String, String), ()> = HashMap::new();
    for m in scan::find_manifests(scan_dir) {
        let deps = match scan::parse_manifest(scan_dir, &m) {
            Ok(d) => d,
            Err(e) => {
                rs.items.push(Item { source: "manifest".into(), name: "parse".into(), req: String::new(), latest: format!("読めません: {e}").chars().take(120).collect(), gap: "error".into(), route: "claude".into(), file: m.strip_prefix(scan_dir).unwrap_or(&m).to_string_lossy().replace('\\', "/") });
                continue;
            }
        };
        let has_windows = deps.iter().any(|x| x.name == "windows");
        for d in deps {
            // windows-core is versioned on its own line (0.100) but must match the `windows` crate's line (0.62) used in the same manifest.
            if scan::follows_windows(&d.name, has_windows) {
                continue;
            }
            if seen.insert((d.eco.label().to_string(), d.name.clone(), d.req.clone()), ()).is_some() {
                continue;
            }
            let Some(cur) = parse_req(&d.req) else { continue };
            rs.deps_checked += 1;
            let latest = match reg.latest(d.eco, &d.name) {
                Ok(Some(v)) => v,
                Ok(None) => continue, // not on the public registry: nothing to compare
                Err(e) => {
                    rs.unchecked += 1;
                    eprintln!("  lookup failed: {e:#}");
                    continue;
                }
            };
            let Some(lv) = parse_ver(&latest) else { continue };
            let (gap, to_claude) = match classify(&d.req, &cur, &lv) {
                Gap::Current => continue,
                Gap::Major => ("major", true),
                Gap::Minor => ("minor", !maintainer_alive),
                Gap::Patch => ("patch", !maintainer_alive),
            };
            rs.items.push(Item { source: d.eco.label().into(), name: d.name, req: d.req, latest, gap: gap.into(), route: if to_claude { "claude" } else { "maintainer" }.into(), file: d.file });
        }
    }
    let _ = cfg;
    rs
}

/// Vulnerabilities always go to Claude (a lock-file bump needs a build and tests); informational advisories go to the maintainers.
fn add_advisories(rs: &mut RepoStatus, dir: &Path, db: &advisory::Db, cfg: &Config) {
    for h in advisory::check(db, dir) {
        let vuln = h.adv.informational.is_empty();
        if !vuln && !cfg.advisory.include_informational {
            continue;
        }
        let fix = if h.adv.patched_text.is_empty() { "修正版なし".to_string() } else { format!("修正: {}", h.adv.patched_text.join(" / ")) };
        let remedy = remedy_for(&h, cfg);
        let latest = format!("{} {} — {} → 対処: {}", h.adv.id, fix, h.adv.title.chars().take(80).collect::<String>(), remedy);
        rs.items.push(Item {
            source: if vuln { "RustSec".into() } else { format!("RustSec/{}", h.adv.informational) },
            name: h.krate,
            req: h.version,
            latest,
            gap: if vuln { "vuln" } else { "advisory" }.into(),
            route: if vuln { "claude" } else { "maintainer" }.into(),
            file: h.lock,
        });
    }
    // Findings that are already handled by swapping the crate for a vetted replacement stay visible (never routed to anyone).
    for m in advisory::mitigated(db, dir, &cfg.advisory.mitigations) {
        let src = m.source.split('#').next().unwrap_or(&m.source).to_string();
        rs.mitigated.push(Item {
            source: "RustSec/mitigated".into(),
            name: m.krate,
            req: m.version,
            latest: format!("{} 対策済み(差し替え: {}) — {}", m.advisory, src, m.caveat),
            gap: "mitigated".into(),
            route: "none".into(),
            file: m.lock,
        });
    }
}

/// A concrete next step for an open advisory: a version bump when a fixed release exists, a configured mitigation
/// when one is known, and otherwise an honest "no fix" with the options that remain.
fn remedy_for(h: &advisory::Hit, cfg: &Config) -> String {
    if !h.adv.patched_text.is_empty() {
        return format!(
            "Cargo.toml の {0} を {1} 以上にして `cargo update -p {0}` → ビルドとテスト(固定が別リポジトリの場合は固定を最新に更新)",
            h.krate,
            h.adv.patched_text.join(" / ")
        );
    }
    if let Some(m) = cfg.advisory.mitigations.iter().find(|m| m.advisory == h.adv.id && m.krate == h.krate) {
        return m.fix.clone();
    }
    format!("修正版なし。{} の利用箇所と影響(ネットワークから時間を観測できる秘密鍵演算か)を確認し、使わない機能なら依存から外す/代替crateへの置き換えを検討", h.krate)
}

fn write_site(dir: &Path, status: &Status) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    std::fs::write(dir.join("status.json"), serde_json::to_string(status)?)?;
    std::fs::write(dir.join("index.html"), include_str!("../web/index.html"))?;
    Ok(())
}

/// Sample data for the public demo. The page labels it as sample data; the numbers are invented.
fn write_demo(dir: &Path) -> Result<()> {
    let it = |s: &str, n: &str, r: &str, l: &str, g: &str, route: &str, f: &str| Item { source: s.into(), name: n.into(), req: r.into(), latest: l.into(), gap: g.into(), route: route.into(), file: f.into() };
    let status = Status {
        generated_at: now(),
        demo: true,
        maintainers: vec![MaintainerStatus { name: "aruaru-llm".into(), alive: true }, MaintainerStatus { name: "open-cuda".into(), alive: false }],
        repos: vec![
            RepoStatus { name: "sample-web".into(), deps_checked: 24, unchecked: 0, items: vec![it("crates.io", "serde", "1.0.200", "1.0.228", "patch", "maintainer", "Cargo.toml"), it("crates.io", "poem", "3.1", "4.0.0", "major", "claude", "Cargo.toml")], ..Default::default() },
            RepoStatus { name: "sample-tool".into(), deps_checked: 9, unchecked: 0, items: vec![it("npm", "vite", "^5.2.0", "7.1.0", "major", "claude", "package.json"), it("clone", "origin", "ahead 0", "3", "behind", "claude", "")], ..Default::default() },
            RepoStatus { name: "sample-lib".into(), deps_checked: 12, unchecked: 1, items: vec![it("crates.io", "tokio", "1.30", "1.47.0", "minor", "maintainer", "Cargo.toml")], ..Default::default() },
            RepoStatus { name: "sample-site".into(), deps_checked: 6, unchecked: 0, items: vec![], ..Default::default() },
        ],
        hidden_private: 2,
    };
    write_site(dir, &status)?;
    if let Ok(cfg) = Config::load(&PathBuf::from("config.toml")).or_else(|_| Config::load(&PathBuf::from("config.example.toml"))) {
        if let Ok(v) = apiex::demo_json(&cfg, now()) {
            std::fs::write(dir.join("api-exceptions.json"), v.to_string())?;
        }
    }
    println!("demo status written to {}", dir.display());
    Ok(())
}

/// `--fix`: print the plan for every open vulnerability; with `--apply` (and `[fix] enabled`) carry it out in fresh clones.
fn run_fix(repos: &[RepoStatus], cfg: &Config, args: &Args, maintainers: &[MaintainerStatus]) -> Vec<String> {
    let mut plans = Vec::new();
    for rs in repos.iter().filter(|r| args.only.as_ref().map_or(true, |o| &r.name == o)) {
        let dir = cfg.root.join(&rs.name);
        plans.extend(fix::plan(&dir, rs, cfg));
        plans.extend(fix::plan_lock_refresh(&dir, &rs.name, &cfg.fix));
    }
    println!("--- fix plan ({} item(s)) ---", plans.len());
    for p in &plans {
        println!("{}", fix::describe(p));
    }
    if !args.apply {
        println!("(計画のみ。実行するには --fix --apply。[fix] enabled = true の機械でだけ動く)");
        return Vec::new();
    }
    if !cfg.fix.enabled {
        println!("[fix] enabled = false のため実行しません(この機械はビルドしない設定)。");
        return Vec::new();
    }
    let mode = fix::resolve_mode(args.push, args.pr, &cfg.fix);
    if (args.push || args.pr) && mode == fix::Mode::Local {
        println!("--push/--pr は [fix] allow_push / allow_pr が false のため無視(ローカルコミットまで)。");
    }
    let profile = hw::detect(cfg, maintainers);
    print!("{}", hw::render(&profile, &cfg.fix));
    let workers = hw::workers(&profile, &cfg.fix);
    let results = fix::apply(&plans, mode, Path::new("scripts/remediate.sh"), workers, hw::jobs_per_worker(&profile, workers));
    let mut failures = Vec::new();
    for line in &results {
        println!("{line}");
        if fix::is_failure(line) {
            failures.push(line.clone());
        } else if profile.llm_up {
            if let Some(note) = fix::work_copy(line).and_then(|w| hw::summarize(cfg, w)) {
                println!("  参考(aruaru-llm・判定には使わない): {note}");
            }
        }
    }
    failures
}

/// A Connect method URL (`/<package>.v1.<Service>/<Method>`) must be called with POST and a JSON body; anything else is a plain GET.
fn is_connect_url(url: &str) -> bool {
    url.rsplit('/').nth(1).is_some_and(|svc| svc.contains(".v") && svc.chars().next().is_some_and(|c| c.is_ascii_alphabetic()))
}

fn probe(h: &reqwest::blocking::Client, url: &str) -> bool {
    let r = if is_connect_url(url) { h.post(url).json(&serde_json::json!({})).send() } else { h.get(url).send() };
    r.map(|r| r.status().is_success()).unwrap_or(false)
}

#[cfg(test)]
mod probe_tests {
    use super::is_connect_url;

    #[test]
    fn connect_urls_are_recognised() {
        assert!(is_connect_url("http://127.0.0.1:4600/aruaru_llm.v1.Llm/Accelerators"));
        assert!(!is_connect_url("http://127.0.0.1:4600/v1/accelerators"));
        assert!(!is_connect_url("http://127.0.0.1:4600/health"));
    }
}

/// `--loop --only REPO [--goal TEXT] [--push]`: investigate/develop/test/debug cycles in a fresh clone.
fn run_loop(repos: &[RepoStatus], cfg: &Config, args: &Args) {
    let Some(only) = &args.only else {
        println!("--loop needs --only <repository> (one repository per run).");
        return;
    };
    // The scan result is optional: a repository with nothing to report still can be worked on, with an explicit --goal.
    let goal = match (&args.goal, repos.iter().find(|r| &r.name == only)) {
        (Some(g), _) => g.clone(),
        (None, Some(rs)) => report::claude_prompt(&cfg.root.to_string_lossy(), rs, cfg.policy.max_items_per_message),
        (None, None) => {
            println!("{only}: no scan result to build a goal from; pass --goal.");
            return;
        }
    };
    let origin = git(&cfg.root.join(only), &["remote", "get-url", "origin"], Duration::from_secs(10)).unwrap_or_default().trim().to_string();
    if origin.is_empty() {
        println!("{only} has no origin; nothing to clone.");
        return;
    }
    if !cfg.autoloop.enabled {
        println!("[autoloop] enabled = false: would run up to {} cycle(s) on {only} (goal: {} chars).", cfg.autoloop.max_cycles, goal.len());
        return;
    }
    match autoloop::run(&cfg.autoloop, only, &origin, &goal, args.push) {
        Ok(line) => println!("{line}"),
        Err(e) => println!("RESULT {only}: loop error: {e:#}"),
    }
}
