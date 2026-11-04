//! API exception ledger and patrol ("API 例外の台帳と巡回").
//!
//! Policy (F:\runo\API-POLICY.md, 2026-11-04): REST is being abolished in favour of gRPC / Connect. A few things stay
//! because somebody else's standard fixes their shape (ACME, third-party webhooks, static files ...). This module keeps
//! a machine-readable ledger (`exceptions.toml`) of them and, on every run:
//!   (a) asks aruaru-search (and, for a reference note, aruaru-llm) whether something new makes an exception replaceable,
//!   (b) counts the REST-like routes that really remain in each repository's code and records the trend,
//!   (c) tells the owner through open-easy-web's notice endpoint when something changed.
//! Everything fetched from the outside (search results, LLM text) is data: it is only matched, truncated and displayed.
use crate::config::{ApiEx, Config};
use anyhow::{anyhow, Context, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::time::Duration;

const REPLACEABILITY: [&str; 4] = ["impossible", "replaceable", "partial", "unknown"];
const HISTORY_MAX: usize = 480; // 4 runs a day for ~120 days
const SEEN_MAX: usize = 300;

#[derive(Debug, Deserialize)]
pub struct Ledger {
    pub checked: String,
    #[serde(default, rename = "exception")]
    pub exceptions: Vec<Exception>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Exception {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub name_en: String,
    pub standard: String,
    #[serde(default)]
    pub standard_urls: Vec<String>,
    pub replaceability: String,
    pub replacement: String,
    pub next_step: String,
    #[serde(default)]
    pub search_queries: Vec<String>,
    #[serde(default)]
    pub signal_terms: Vec<String>,
    #[serde(default, rename = "use")]
    pub uses: Vec<Use>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Use {
    pub repo: String,
    #[serde(rename = "where")]
    pub place: String,
}

pub fn load_ledger(path: &Path) -> Result<Ledger> {
    let text = std::fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
    let l: Ledger = toml::from_str(&text).with_context(|| format!("invalid {}", path.display()))?;
    let mut ids = BTreeSet::new();
    for e in &l.exceptions {
        if !REPLACEABILITY.contains(&e.replaceability.as_str()) {
            return Err(anyhow!("{}: replaceability '{}' is not one of {:?}", e.id, e.replaceability, REPLACEABILITY));
        }
        if !ids.insert(e.id.clone()) {
            return Err(anyhow!("duplicate exception id {}", e.id));
        }
        if e.standard_urls.is_empty() {
            return Err(anyhow!("{}: standard_urls must not be empty (every entry needs a source)", e.id));
        }
    }
    parse_date(&l.checked).ok_or_else(|| anyhow!("checked '{}' is not YYYY-MM-DD", l.checked))?;
    Ok(l)
}

// ---------------------------------------------------------------- dates

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// `YYYY-MM-DD` in Japan time.
pub fn date_jst(unix: u64) -> String {
    let (y, m, d) = civil_from_days(((unix + 9 * 3600) / 86400) as i64);
    format!("{y:04}-{m:02}-{d:02}")
}

fn parse_date(s: &str) -> Option<i64> {
    let mut it = s.split('-');
    let (y, m, d): (i64, i64, i64) = (it.next()?.parse().ok()?, it.next()?.parse().ok()?, it.next()?.parse().ok()?);
    if it.next().is_some() || !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    Some(days_from_civil(y, m, d))
}

// ---------------------------------------------------------------- route counting

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct RouteCount {
    /// every distinct route path registered (Connect-style ones included)
    pub routes: usize,
    /// distinct paths that are neither Connect-style nor a known exception (health, ACME, webhook, callback, static): the REST-like ones
    pub rest_like: usize,
    /// of those, the ones containing /api/ or /admin/
    pub api_admin: usize,
}

const SKIP_DIRS: [&str; 13] = ["target", "node_modules", ".git", "vendor", "advisory-db", "dist", ".deps", ".vscode-test", ".vscode-test-web", "tests", "examples", "benches", "site"];

fn re(p: &str) -> Regex {
    Regex::new(p).expect("static regex")
}

/// Connect paths look like `/open_easy_web.admin.v1.Notify/Send`: a dotted package, then `Service/Method`.
pub fn is_connect_path(p: &str) -> bool {
    let first = p.trim_start_matches('/').split('/').next().unwrap_or("");
    first.contains('.') && first.rsplit('.').next().is_some_and(|s| s.chars().next().is_some_and(|c| c.is_ascii_uppercase())) && p.trim_start_matches('/').matches('/').count() >= 1
}

/// Paths whose shape somebody else's standard fixes (see exceptions.toml) or that are plain browser assets.
pub fn is_exception_path(p: &str) -> bool {
    let l = p.to_ascii_lowercase();
    l.starts_with("/.well-known/")
        || l.contains("health")
        || l.contains("webhook")
        || l.contains("callback")
        || l.contains("/static")
        || l.contains("/assets")
        || l.contains("favicon")
        || l.contains("robots.txt")
        || l == "/"
        || [".html", ".js", ".css", ".png", ".ico", ".svg", ".wasm", ".webp", ".json"].iter().any(|e| l.ends_with(e))
}

/// Removes `#[cfg(test)] mod x { ... }` blocks (brace matching; braces inside string literals can skew it a little).
fn strip_test_mods(text: &str, start: &Regex) -> String {
    let mut out = String::with_capacity(text.len());
    let mut pos = 0;
    while let Some(m) = start.find_at(text, pos) {
        out.push_str(&text[pos..m.start()]);
        let mut depth = 1usize;
        let mut end = text.len();
        for (i, c) in text[m.end()..].char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = m.end() + i + 1;
                        break;
                    }
                }
                _ => {}
            }
        }
        pos = end;
    }
    out.push_str(&text[pos..]);
    out
}

fn is_test_file(name: &str) -> bool {
    let stem = name.trim_end_matches(".rs");
    stem == "test" || stem == "tests" || stem.ends_with("_test") || stem.ends_with("_tests")
}

fn collect_files(dir: &Path, rel: &str, skip: &[String], out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let path = e.path();
        let rel_here = if rel.is_empty() { name.clone() } else { format!("{rel}/{name}") };
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_dir() {
            if SKIP_DIRS.contains(&name.as_str()) || skip.iter().any(|s| s == &rel_here) {
                continue;
            }
            collect_files(&path, &rel_here, skip, out);
        } else if ft.is_file() && name.ends_with(".rs") && !is_test_file(&name) && e.metadata().map(|m| m.len() < 2_000_000).unwrap_or(false) {
            out.push(path);
        }
    }
}

/// Counts route registrations of axum (`.route("/x"`), poem (`.at("/x"`) and hand-written hyper dispatch
/// (`(&Method::GET, "/x")` / `("GET", "/x")`). A static heuristic: an upper bound for HTML page routes, blind to routes built at runtime.
pub fn count_routes(repo_dir: &Path, repo: &str, skip_paths: &[String]) -> (RouteCount, BTreeSet<String>) {
    // skip_paths are "repo/relative/dir"; turn the ones for this repo into repo-relative form
    let skip: Vec<String> = skip_paths.iter().filter_map(|s| s.strip_prefix(&format!("{repo}/")).map(str::to_string)).collect();
    let mut files = Vec::new();
    collect_files(repo_dir, "", &skip, &mut files);
    let r_route = re(r#"\.(?:route|at)\(\s*"(/[^"]*)""#);
    let r_hyper = re(r#"\(\s*&?(?:hyper::)?Method::(?:GET|POST|PUT|DELETE|PATCH)\s*,\s*"(/[^"]*)""#);
    let r_str = re(r#"\(\s*"(?:GET|POST|PUT|DELETE|PATCH)"\s*,\s*"(/[^"]*)""#);
    let r_test_mod = re(r"#\[cfg\(test\)\]\s*mod\s+\w+\s*\{");
    let mut paths = BTreeSet::new();
    for f in files {
        let Ok(text) = std::fs::read_to_string(&f) else { continue };
        let text = strip_test_mods(&text, &r_test_mod);
        let text = text.as_str();
        for r in [&r_route, &r_hyper, &r_str] {
            for c in r.captures_iter(text) {
                paths.insert(c[1].to_string());
            }
        }
    }
    let mut rc = RouteCount { routes: paths.len(), ..Default::default() };
    let mut rest = BTreeSet::new();
    for p in &paths {
        if is_connect_path(p) || is_exception_path(p) {
            continue;
        }
        rc.rest_like += 1;
        if p.contains("/api/") || p.contains("/admin/") {
            rc.api_admin += 1;
        }
        rest.insert(p.clone());
    }
    (rc, rest)
}

// ---------------------------------------------------------------- state

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct Snapshot {
    pub t: u64,
    pub date: String,
    pub repos: BTreeMap<String, RouteCount>,
    pub total_rest_like: usize,
    pub total_routes: usize,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct State {
    /// exception id -> links already seen in search results
    #[serde(default)]
    pub seen_links: HashMap<String, Vec<String>>,
    #[serde(default)]
    pub history: Vec<Snapshot>,
    /// repo -> REST-like paths at the last accepted baseline (state.json is git-ignored; private repo names stay here)
    #[serde(default)]
    pub baseline: HashMap<String, Vec<String>>,
    /// notice kind -> fingerprint of the last delivered notice
    #[serde(default)]
    pub notified: HashMap<String, String>,
}

// ---------------------------------------------------------------- research (aruaru-search / aruaru-llm)

#[derive(Serialize, Debug, Clone)]
pub struct Hit {
    pub title: String,
    pub link: String,
    #[serde(skip)]
    pub snippet: String,
}

#[derive(Debug, Clone, Default)]
pub struct Research {
    pub search_ok: bool,
    pub hits: Vec<Hit>,
    pub signals: Vec<Hit>,
    pub llm_note: Option<String>,
    pub detail: String,
}

fn clip(s: &str, n: usize) -> String {
    let t: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    t.chars().take(n).collect()
}

fn http_link(l: &str) -> bool {
    l.starts_with("https://") || l.starts_with("http://")
}

fn search(http: &reqwest::blocking::Client, cfg: &ApiEx, q: &str) -> Result<Vec<Hit>> {
    let url = format!("{}/v1/search", cfg.search_url.trim_end_matches('/'));
    let resp = http.get(&url).query(&[("q", q), ("n", &cfg.search_n.to_string())]).send()?;
    if !resp.status().is_success() {
        return Err(anyhow!("HTTP {}", resp.status()));
    }
    let v = rust_json::parse_secure(&resp.text()?, &rust_json::Limits::default()).map_err(|e| anyhow!("bad JSON from aruaru-search: {e}"))?;
    let mut out = Vec::new();
    for r in v.get("results").and_then(Value::as_array).into_iter().flatten() {
        let link = r.get("link").and_then(Value::as_str).unwrap_or("");
        if !http_link(link) {
            continue;
        }
        out.push(Hit { title: clip(r.get("title").and_then(Value::as_str).unwrap_or(""), 140), link: link.chars().take(300).collect(), snippet: clip(r.get("snippet").and_then(Value::as_str).unwrap_or(""), 300) });
    }
    Ok(out)
}

/// One short English note from aruaru-llm. The model is a small GPT-2: a reference at best, never a source; failures are only recorded.
fn llm_note(http: &reqwest::blocking::Client, cfg: &ApiEx, e: &Exception, signals: &[Hit]) -> Result<String> {
    let titles = signals.iter().map(|s| s.title.clone()).collect::<Vec<_>>().join("; ");
    let prompt = format!("Topic: {}. New pages: {}. One-sentence summary:", if e.name_en.is_empty() { &e.name } else { &e.name_en }, titles);
    let url = format!("{}/v1/generate", cfg.llm_url.trim_end_matches('/'));
    let resp = http.post(&url).timeout(Duration::from_secs(cfg.llm_timeout_secs)).json(&json!({"prompt": prompt, "max_new_tokens": 48})).send()?;
    if !resp.status().is_success() {
        return Err(anyhow!("HTTP {}", resp.status()));
    }
    let v = rust_json::parse_secure(&resp.text()?, &rust_json::Limits::default()).map_err(|e| anyhow!("bad JSON from aruaru-llm: {e}"))?;
    Ok(clip(v.get("completion").and_then(Value::as_str).unwrap_or(""), 240))
}

fn research(http: &reqwest::blocking::Client, cfg: &ApiEx, e: &Exception, st: &mut State, dry_log: bool) -> Research {
    let mut r = Research::default();
    let first_time = !st.seen_links.contains_key(&e.id);
    let seen = st.seen_links.entry(e.id.clone()).or_default();
    let mut errors = Vec::new();
    let mut all = Vec::new();
    for q in &e.search_queries {
        std::thread::sleep(Duration::from_millis(800));
        let got = search(http, cfg, q).or_else(|_| {
            std::thread::sleep(Duration::from_secs(3));
            search(http, cfg, q)
        });
        match got {
            Ok(h) => {
                r.search_ok = true;
                all.extend(h);
            }
            Err(err) => errors.push(format!("{err}")),
        }
    }
    let mut uniq = BTreeSet::new();
    all.retain(|h| uniq.insert(h.link.clone()));
    for h in &all {
        if seen.contains(&h.link) {
            continue;
        }
        seen.push(h.link.clone());
        let hay = format!("{} {}", h.title, h.snippet).to_lowercase();
        if !first_time && e.signal_terms.iter().any(|t| hay.contains(&t.to_lowercase())) && r.signals.len() < 3 {
            r.signals.push(h.clone());
        }
    }
    if seen.len() > SEEN_MAX {
        let drop = seen.len() - SEEN_MAX;
        seen.drain(0..drop);
    }
    r.hits = all.into_iter().take(5).collect();
    if !r.search_ok {
        r.detail = format!("aruaru-search unavailable: {}", errors.first().cloned().unwrap_or_else(|| "no query".into()));
    } else if first_time {
        r.detail = "baseline recorded (no signals on the first run)".into();
    }
    if !r.signals.is_empty() {
        match llm_note(http, cfg, e, &r.signals) {
            Ok(n) if !n.is_empty() => r.llm_note = Some(n),
            Ok(_) => r.detail.push_str(" aruaru-llm returned nothing;"),
            Err(err) => r.detail.push_str(&format!(" aruaru-llm unavailable ({err});")),
        }
    }
    if dry_log {
        println!("  api[{}]: search={} hits={} signals={}", e.id, r.search_ok, r.hits.len(), r.signals.len());
    }
    r
}

// ---------------------------------------------------------------- notice through open-easy-web

pub fn send_notice(http: &reqwest::blocking::Client, cfg: &ApiEx, kind: &str, subject: &str, body: &str) -> Result<()> {
    let token = std::env::var(&cfg.token_env).map_err(|_| anyhow!("environment variable {} is not set", cfg.token_env))?;
    let url = format!("{}/open_easy_web.admin.v1.Notify/Send", cfg.notify_url.trim_end_matches('/'));
    let resp = http
        .post(&url)
        .header("content-type", "application/json")
        .header("x-admin-token", token)
        .json(&json!({"kind": kind, "subject": subject, "body": body}))
        .send()
        .map_err(|e| anyhow!("notice request failed: {}", e.without_url()))?;
    if resp.status().is_success() {
        Ok(())
    } else {
        Err(anyhow!("notice endpoint answered HTTP {}", resp.status()))
    }
}

// ---------------------------------------------------------------- the patrol

pub struct Outcome {
    pub public: Value,
    pub messages: Vec<(String, String, String, String)>, // (key, kind, subject, body)
    /// repo -> REST-like paths to adopt as the new baseline once the "rest-increase" notice was delivered
    pub held: HashMap<String, Vec<String>>,
}

/// Runs the whole patrol and returns the public JSON plus the notices that should go out. `st` is updated in place
/// (the caller passes a scratch copy for `--dry-run`).
pub fn patrol(cfg: &Config, st: &mut State, now: u64, verbose: bool) -> Result<Outcome> {
    let ledger = load_ledger(&cfg.apiex.ledger)?;
    let http = reqwest::blocking::Client::builder().user_agent("version-control (https://github.com/aon-co-jp/version-control)").timeout(Duration::from_secs(25)).build()?;

    // (a) research
    let mut results: Vec<(Exception, Research)> = Vec::new();
    for e in &ledger.exceptions {
        let r = research(&http, &cfg.apiex, e, st, verbose);
        results.push((e.clone(), r));
    }
    let search_ok = results.iter().any(|(_, r)| r.search_ok);
    let llm_used = results.iter().any(|(_, r)| r.llm_note.is_some());

    // (b) route counts
    let mut snap = Snapshot { t: now, date: date_jst(now), ..Default::default() };
    let mut current: HashMap<String, BTreeSet<String>> = HashMap::new();
    for dir in crate::list_repos(cfg) {
        let name = dir.file_name().unwrap_or_default().to_string_lossy().to_string();
        let (rc, rest) = count_routes(&dir, &name, &cfg.apiex.skip_paths);
        if verbose {
            println!("  api routes {:<34} routes={:<4} rest-like={:<4} api/admin={}", name, rc.routes, rc.rest_like, rc.api_admin);
        }
        snap.total_rest_like += rc.rest_like;
        snap.total_routes += rc.routes;
        snap.repos.insert(name.clone(), rc);
        current.insert(name, rest);
    }

    // (c) what changed
    let mut messages = Vec::new();
    let mut increases: Vec<(String, Vec<String>)> = Vec::new();
    let mut new_baseline = st.baseline.clone();
    let have_baseline = !st.baseline.is_empty();
    for (repo, now_paths) in &current {
        let prev: BTreeSet<String> = st.baseline.get(repo).map(|v| v.iter().cloned().collect()).unwrap_or_default();
        let added: Vec<String> = now_paths.difference(&prev).cloned().collect();
        if have_baseline && !added.is_empty() {
            increases.push((repo.clone(), added));
        } else {
            new_baseline.insert(repo.clone(), now_paths.iter().cloned().collect());
        }
        // removals always lower the baseline (progress is never an alert)
        if let Some(b) = new_baseline.get_mut(repo) {
            b.retain(|p| now_paths.contains(p));
        }
    }
    increases.sort();
    if !increases.is_empty() {
        let mut body = String::from("API方針(F:\\runo\\API-POLICY.md、2026-11-04)に反して、REST風のエンドポイントが増えています。次のメッセージをそのままClaude Codeに貼り付けてください。\n\n");
        body.push_str("次のリポジトリで新しく追加された REST 風ルートを、Connect の単純呼び出し(POST /<パッケージ>.<Service>/<Method>、content-type: application/json、RS-JSON で読み書き)に作り直してください。旧ルートは残さないでください。例外(ACME・OAuth/OIDC コールバック・第三者Webhook・ヘルスチェック・静的ファイル・OTLP)は F:\\version-control\\exceptions.toml が根拠です。\n\n");
        for (repo, added) in &increases {
            body.push_str(&format!("■ {repo}(REST風 {} 本)\n", current[repo].len()));
            for p in added.iter().take(15) {
                body.push_str(&format!("  - {p}\n"));
            }
            if added.len() > 15 {
                body.push_str(&format!("  …ほか {} 本\n", added.len() - 15));
            }
        }
        body.push_str("\n(数え方は静的な目安です: axum の .route、poem の .at、hyper の (Method, \"/path\") を拾います。誤検出なら exceptions.toml / scan の対象外に追加してください。また、巡回先のクローンを git pull で更新しただけでも増えて見えます。古いコード側のルートなら、移行済みの側で消えているかを確認してください。)\n");
        let subject = format!("[version-control] API方針違反の疑い: REST風ルートが増加 ({}件のリポジトリ)", increases.len());
        messages.push(("rest-increase".to_string(), "needs-claude".to_string(), subject, body));
    }

    let mut signal_lines = String::new();
    let mut n_signals = 0;
    for (e, r) in &results {
        if r.signals.is_empty() {
            continue;
        }
        n_signals += r.signals.len();
        signal_lines.push_str(&format!("■ {}({}) 現在の判定: {}\n  次の手(台帳): {}\n", e.name, e.id, e.replaceability, e.next_step));
        for s in &r.signals {
            signal_lines.push_str(&format!("  - {}\n    出典: {}\n", s.title, s.link));
        }
        if let Some(n) = &r.llm_note {
            signal_lines.push_str(&format!("  参考(aruaru-llm の生成。小型モデルのため信頼せず、必ず出典を確認): {n}\n"));
        }
        signal_lines.push('\n');
    }
    if n_signals > 0 {
        let body = format!("API例外の台帳の巡回で、置き換えられる状況の兆候になりうる新しいページを見つけました。検索結果は『データ』であり、内容の真偽は未確認です。\n\n{signal_lines}確認して、置き換えられそうなら F:\\version-control の exceptions.toml(replaceability / 確認日)と API-EXCEPTIONS.md を更新し、該当リポジトリの置き換えをClaude Codeで進めてください。\n");
        messages.push(("signals".to_string(), "update-needed".to_string(), format!("[version-control] API例外: 置き換えの兆候 {n_signals}件"), body));
    }

    if let Some(days) = parse_date(&date_jst(now)).zip(parse_date(&ledger.checked)).map(|(a, b)| a - b) {
        if days > 30 {
            messages.push((
                format!("stale-{}", ledger.checked),
                "update-needed".to_string(),
                format!("[version-control] API例外の台帳の確認日が{days}日前です"),
                format!("exceptions.toml の確認日(checked)が {} のままです。各例外の根拠の規格・置き換え可否を見直し、checked を更新してください。", ledger.checked),
            ));
        }
    }

    // The baseline advances for repositories without an increase. Repositories with one stay held back until the notice
    // has gone out (`Outcome::held`, accepted by the caller), so a failed delivery is retried on the next run.
    let held: HashMap<String, Vec<String>> = increases.iter().map(|(r, _)| (r.clone(), current[r].iter().cloned().collect())).collect();
    for (r, _) in &increases {
        new_baseline.insert(r.clone(), st.baseline.get(r).cloned().unwrap_or_default());
    }
    st.baseline = new_baseline;
    st.history.push(snap.clone());
    if st.history.len() > HISTORY_MAX {
        let drop = st.history.len() - HISTORY_MAX;
        st.history.drain(0..drop);
    }

    let public = public_json(cfg, &ledger, &results, &snap, &st.history, now, search_ok, llm_used, false);
    Ok(Outcome { public, messages, held })
}

fn day_series(history: &[Snapshot]) -> Vec<Value> {
    // last snapshot of each day, at most 60 days
    let mut by_day: BTreeMap<&str, &Snapshot> = BTreeMap::new();
    for s in history {
        by_day.insert(s.date.as_str(), s);
    }
    let v: Vec<Value> = by_day.values().map(|s| json!({"date": s.date, "rest_like": s.total_rest_like, "routes": s.total_routes})).collect();
    let skip = v.len().saturating_sub(60);
    v.into_iter().skip(skip).collect()
}

#[allow(clippy::too_many_arguments)]
fn public_json(cfg: &Config, ledger: &Ledger, results: &[(Exception, Research)], snap: &Snapshot, history: &[Snapshot], now: u64, search_ok: bool, llm_used: bool, demo: bool) -> Value {
    let public = |repo: &str| !cfg.private.iter().any(|p| p == repo);
    let mut counts = json!({"impossible": 0, "replaceable": 0, "partial": 0, "unknown": 0});
    let mut ex = Vec::new();
    for (e, r) in results {
        counts[&e.replaceability] = json!(counts[&e.replaceability].as_u64().unwrap_or(0) + 1);
        let uses: Vec<Value> = e.uses.iter().filter(|u| public(&u.repo)).map(|u| json!({"repo": u.repo, "where": u.place})).collect();
        ex.push(json!({
            "id": e.id, "name": e.name, "name_en": e.name_en, "standard": e.standard, "standard_urls": e.standard_urls,
            "replaceability": e.replaceability, "replacement": e.replacement, "next_step": e.next_step,
            "uses": uses, "use_count": e.uses.len(),
            "sources": r.hits.iter().map(|h| json!({"title": h.title, "link": h.link})).collect::<Vec<_>>(),
            "signals": r.signals.len(), "patrol": if r.detail.is_empty() { "ok".to_string() } else { r.detail.clone() },
        }));
    }
    let repos: Vec<Value> = snap.repos.iter().filter(|(n, c)| public(n) && c.routes > 0).map(|(n, c)| json!({"name": n, "routes": c.routes, "rest_like": c.rest_like, "api_admin": c.api_admin})).collect();
    let total_public: usize = snap.repos.iter().filter(|(n, _)| public(n)).map(|(_, c)| c.rest_like).sum();
    json!({
        "generated_at": now, "demo": demo, "checked": ledger.checked,
        "search": {"ok": search_ok, "note": if search_ok { "aruaru-search" } else { "aruaru-search に接続できませんでした(記録して続行)" }},
        "llm": {"used": llm_used, "note": "aruaru-llm は小型モデルの参考要約。出典URLが正本"},
        "counts": counts, "exceptions": ex,
        "routes": {"repos": repos, "total_rest_like": total_public, "method": "axum .route / poem .at / hyper (Method, \"/path\") を静的に数えた目安(Connect形式と例外パスを除く)"},
        "history": day_series(history),
    })
}

/// Sample data for `/demo/`: the ledger is real, the numbers and history are invented.
pub fn demo_json(cfg: &Config, now: u64) -> Result<Value> {
    let ledger = load_ledger(&cfg.apiex.ledger)?;
    let results: Vec<(Exception, Research)> = ledger.exceptions.iter().cloned().map(|e| (e, Research { search_ok: true, ..Default::default() })).collect();
    let mut snap = Snapshot { t: now, date: date_jst(now), ..Default::default() };
    for (n, r, l) in [("sample-web", 14usize, 9usize), ("sample-tool", 6, 2), ("sample-lib", 3, 0)] {
        snap.repos.insert(n.to_string(), RouteCount { routes: r, rest_like: l, api_admin: l / 2 });
        snap.total_rest_like += l;
        snap.total_routes += r;
    }
    let history: Vec<Snapshot> = (0..6u64).map(|i| Snapshot { t: now, date: date_jst(now - (5 - i) * 86400), total_rest_like: 20 - (i as usize) * 2 + 1, total_routes: 40, ..Default::default() }).collect();
    Ok(public_json(cfg, &ledger, &results, &snap, &history, now, true, false, true))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connect_vs_rest_paths() {
        assert!(is_connect_path("/open_easy_web.admin.v1.Notify/Send"));
        assert!(is_connect_path("/grpc.health.v1.Health/Check"));
        assert!(!is_connect_path("/api/users"));
        assert!(!is_connect_path("/v1/search"));
        assert!(!is_connect_path("/index.html"));
    }

    #[test]
    fn exception_paths() {
        for p in ["/healthz", "/api/health", "/.well-known/acme-challenge/:token", "/webhook/line", "/api/auth/google/callback", "/", "/app.js"] {
            assert!(is_exception_path(p), "{p}");
        }
        assert!(!is_exception_path("/api/users"));
        assert!(!is_exception_path("/admin/stats"));
    }

    #[test]
    fn dates_round_trip() {
        assert_eq!(date_jst(0), "1970-01-01");
        let d = parse_date("2026-11-04").unwrap();
        assert_eq!(civil_from_days(d), (2026, 11, 4));
        assert_eq!(parse_date("2026-11-06").unwrap() - d, 2);
        assert!(parse_date("2026-13-01").is_none());
    }

    #[test]
    fn counts_routes_in_a_temp_repo() {
        let dir = std::env::temp_dir().join(format!("vc-apiex-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(
            dir.join("src/main.rs"),
            r#"fn r(){ app.route("/api/users", get(a)).route("/healthz", get(b)).at("/open_x.v1.S/Do", post(c)).at("/admin/stats", get(d));
               match (&Method::GET, "/v1/items") {} match ("POST", "/v1/items") {} }
               #[cfg(test)] mod t { fn x(){ app.route("/api/only-in-test", get(a)); } }"#,
        )
        .unwrap();
        let (rc, rest) = count_routes(&dir, "tmp", &[]);
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(rc.routes, 5); // /api/users /healthz /open_x.v1.S/Do /admin/stats /v1/items
        assert_eq!(rc.rest_like, 3); // users, admin/stats, v1/items
        assert_eq!(rc.api_admin, 2);
        assert!(!rest.contains("/api/only-in-test"));
    }

    #[test]
    fn ledger_in_repo_is_valid() {
        let l = load_ledger(Path::new("exceptions.toml")).unwrap();
        assert!(l.exceptions.len() >= 7);
        assert!(l.exceptions.iter().all(|e| !e.uses.is_empty()), "every entry must name where it is really used");
    }
}
