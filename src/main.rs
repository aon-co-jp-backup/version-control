//! version-control: checks every repository under a root for outdated dependencies and stale clones,
//! leaves routine bumps to the automatic maintainers (aruaru-llm / open-cuda side), and sends the message
//! to paste into Claude by e-mail and LINE for everything they cannot handle.
mod config;
mod latest;
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
}

struct Args {
    config: PathBuf,
    dry_run: bool,
    force: bool,
    test_notify: bool,
    no_fetch: bool,
    demo: bool,
}

fn parse_args() -> Args {
    let mut a = Args { config: PathBuf::from("config.toml"), dry_run: false, force: false, test_notify: false, no_fetch: false, demo: false };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--config" => a.config = it.next().map(PathBuf::from).unwrap_or(a.config),
            "--dry-run" => a.dry_run = true,
            "--force" => a.force = true,
            "--test-notify" => a.test_notify = true,
            "--no-fetch" => a.no_fetch = true,
            "--demo" => a.demo = true,
            "-h" | "--help" => {
                println!("version-control [--config FILE] [--dry-run] [--force] [--no-fetch] [--test-notify] [--demo]\n  --dry-run      scan and write the dashboard, send nothing\n  --force        announce even if the message was already sent\n  --test-notify  send a short test message by e-mail and LINE\n  --demo         write the sample status.json for the public demo (no scan)");
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
    let mut state: State = std::fs::read_to_string(&state_path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    let mut registry = latest::Registry::new(std::mem::take(&mut state.cache), cfg.policy.registry_cache_hours)?;

    let maintainers = check_maintainers(&cfg);
    let maintainer_alive = maintainers.iter().any(|m| m.alive);
    println!("maintainers: {}", maintainers.iter().map(|m| format!("{}={}", m.name, if m.alive { "up" } else { "DOWN" })).collect::<Vec<_>>().join(", "));

    let mut repos = Vec::new();
    for dir in list_repos(&cfg) {
        let rs = scan_repo(&dir, &cfg, &mut registry, maintainer_alive, !args.no_fetch && cfg.policy.fetch_clones);
        println!("{:<34} deps={:<4} open={:<3} to-claude={}", rs.name, rs.deps_checked, rs.items.len(), rs.items.iter().filter(|i| i.route == "claude").count());
        repos.push(rs);
    }
    state.cache = std::mem::take(&mut registry.cache);

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

    std::fs::write(&state_path, serde_json::to_string_pretty(&state)?).context("cannot write state.json")?;
    Ok(())
}

fn check_maintainers(cfg: &Config) -> Vec<MaintainerStatus> {
    let http = reqwest::blocking::Client::builder().timeout(Duration::from_secs(5)).build().ok();
    cfg.maintainers
        .iter()
        .map(|m| {
            let alive = !m.health_url.is_empty() && http.as_ref().is_some_and(|h| h.get(&m.health_url).send().map(|r| r.status().is_success()).unwrap_or(false));
            MaintainerStatus { name: m.name.clone(), alive }
        })
        .collect()
}

fn list_repos(cfg: &Config) -> Vec<PathBuf> {
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

/// Runs git with a hard time limit so one unreachable remote cannot stall the whole scan.
fn git(dir: &Path, args: &[&str], limit: Duration) -> Option<String> {
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
    let start = Instant::now();
    loop {
        match child.try_wait().ok()? {
            Some(st) => {
                if !st.success() {
                    return None;
                }
                let mut out = String::new();
                std::io::Read::read_to_string(child.stdout.as_mut()?, &mut out).ok()?;
                return Some(out);
            }
            None if start.elapsed() > limit => {
                let _ = child.kill();
                return None;
            }
            None => std::thread::sleep(Duration::from_millis(50)),
        }
    }
}

fn scan_repo(dir: &Path, cfg: &Config, reg: &mut latest::Registry, maintainer_alive: bool, fetch: bool) -> RepoStatus {
    let name = dir.file_name().unwrap_or_default().to_string_lossy().to_string();
    let mut rs = RepoStatus { name, ..Default::default() };

    // Stale or diverged clone. Never pulled automatically.
    if fetch {
        git(dir, &["fetch", "--quiet"], Duration::from_secs(60));
    }
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
    for m in scan::find_manifests(dir) {
        let deps = match scan::parse_manifest(dir, &m) {
            Ok(d) => d,
            Err(e) => {
                rs.items.push(Item { source: "manifest".into(), name: "parse".into(), req: String::new(), latest: format!("読めません: {e}").chars().take(120).collect(), gap: "error".into(), route: "claude".into(), file: m.strip_prefix(dir).unwrap_or(&m).to_string_lossy().replace('\\', "/") });
                continue;
            }
        };
        for d in deps {
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
            RepoStatus { name: "sample-web".into(), deps_checked: 24, unchecked: 0, items: vec![it("crates.io", "serde", "1.0.200", "1.0.228", "patch", "maintainer", "Cargo.toml"), it("crates.io", "poem", "3.1", "4.0.0", "major", "claude", "Cargo.toml")] },
            RepoStatus { name: "sample-tool".into(), deps_checked: 9, unchecked: 0, items: vec![it("npm", "vite", "^5.2.0", "7.1.0", "major", "claude", "package.json"), it("clone", "origin", "ahead 0", "3", "behind", "claude", "")] },
            RepoStatus { name: "sample-lib".into(), deps_checked: 12, unchecked: 1, items: vec![it("crates.io", "tokio", "1.30", "1.47.0", "minor", "maintainer", "Cargo.toml")] },
            RepoStatus { name: "sample-site".into(), deps_checked: 6, unchecked: 0, items: vec![] },
        ],
        hidden_private: 2,
    };
    write_site(dir, &status)?;
    println!("demo status written to {}", dir.display());
    Ok(())
}
