//! Configuration (`config.toml`, git-ignored). Secrets are only referenced by file path or environment variable.
use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
pub struct Config {
    pub root: PathBuf,
    #[serde(default)]
    pub exclude: Vec<String>,
    /// Repositories whose names must not appear on the published dashboard.
    #[serde(default)]
    pub private: Vec<String>,
    #[serde(default)]
    pub maintainers: Vec<Maintainer>,
    #[serde(default)]
    pub policy: Policy,
    #[serde(default)]
    pub notify: Notify,
    #[serde(default)]
    pub advisory: Advisory,
    #[serde(default)]
    pub apiex: ApiEx,
    /// Automatic remediation (`--fix`). Off by default; turn on only on a machine that may build.
    #[serde(default)]
    pub fix: Fix,
    /// Unattended investigate -> develop -> test -> debug cycles (`--loop`). Off by default.
    #[serde(default)]
    pub autoloop: AutoLoop,
    /// Updates this program itself from its own repository (`--self-update`, and at the start of every run).
    #[serde(default)]
    pub selfupdate: SelfUpdate,
    /// Triage of the pull requests this program opened (`--review`).
    #[serde(default)]
    pub review: Review,
    /// Folder the dashboard (`index.html` + `status.json`) is written to.
    #[serde(default = "default_site")]
    pub site_dir: PathBuf,
}

#[derive(Debug, Deserialize)]
pub struct Maintainer {
    pub name: String,
    #[serde(default)]
    pub health_url: String,
}

#[derive(Debug, Deserialize)]
pub struct Policy {
    #[serde(default = "yes")]
    pub fetch_clones: bool,
    #[serde(default = "default_cache_hours")]
    pub registry_cache_hours: u64,
    #[serde(default = "default_max_items")]
    pub max_items_per_message: usize,
}

impl Default for Policy {
    fn default() -> Self {
        Self { fetch_clones: true, registry_cache_hours: 20, max_items_per_message: 12 }
    }
}

/// RustSec advisory scan of every `Cargo.lock`.
#[derive(Debug, Deserialize)]
pub struct Advisory {
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default = "default_adv_url")]
    pub url: String,
    /// Where the advisory-db checkout lives (created on first run).
    #[serde(default = "default_adv_dir")]
    pub dir: PathBuf,
    /// Include "unmaintained"/"unsound"/"notice" advisories (they are never vulnerabilities, so they go to the maintainers).
    #[serde(default = "yes")]
    pub include_informational: bool,
    /// Known mitigations. A lock entry whose source starts with `source_prefix` is a deliberate replacement for
    /// the vulnerable registry crate, so it is listed as "mitigated" instead of silently disappearing.
    #[serde(default = "default_mitigations")]
    pub mitigations: Vec<Mitigation>,
}

impl Default for Advisory {
    fn default() -> Self {
        Self { enabled: true, url: default_adv_url(), dir: default_adv_dir(), include_informational: true, mitigations: default_mitigations() }
    }
}

/// One entry of `[[advisory.mitigations]]`.
#[derive(Debug, Clone, Deserialize)]
pub struct Mitigation {
    pub advisory: String,
    #[serde(rename = "crate")]
    pub krate: String,
    /// Prefix of the lock `source`, e.g. `git+https://github.com/aon-co-jp/rsa-rs`.
    pub source_prefix: String,
    /// Concrete remedy shown next to open findings, so the fix is one step away.
    pub fix: String,
    /// Honest caveat shown next to mitigated entries.
    #[serde(default)]
    pub caveat: String,
}

fn default_mitigations() -> Vec<Mitigation> {
    vec![Mitigation {
        advisory: "RUSTSEC-2023-0071".into(),
        krate: "rsa".into(),
        source_prefix: "git+https://github.com/aon-co-jp/rsa-rs".into(),
        fix: "Cargo.toml に [patch.crates-io] rsa = { git = \"https://github.com/aon-co-jp/rsa-rs\", tag = \"v0.1.0\", version = \"=<ロックのrsa版>\" } を追加し、cargo update -p crypto-bigint --precise 0.7.10(手順: rsa-rs の PORTING.md)".into(),
        caveat: "rsa-rs は実験的・第三者の暗号監査なし".into(),
    }]
}

fn default_adv_url() -> String {
    "https://github.com/RustSec/advisory-db".into()
}
fn default_adv_dir() -> PathBuf {
    PathBuf::from("advisory-db")
}

#[derive(Debug, Default, Deserialize)]
pub struct Notify {
    #[serde(default)]
    pub email_to: Vec<String>,
    pub line_token_file: Option<PathBuf>,
    pub line_to_file: Option<PathBuf>,
}

/// API exception ledger and patrol (see `apiex.rs`). No secret here: the admin token is read from the environment variable named by `token_env`.
#[derive(Debug, Deserialize, Clone)]
pub struct ApiEx {
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default = "default_ledger")]
    pub ledger: PathBuf,
    #[serde(default = "default_search_url")]
    pub search_url: String,
    #[serde(default = "default_llm_url")]
    pub llm_url: String,
    #[serde(default = "default_llm_timeout")]
    pub llm_timeout_secs: u64,
    #[serde(default = "default_search_n")]
    pub search_n: usize,
    #[serde(default = "default_notify_url")]
    pub notify_url: String,
    #[serde(default = "default_token_env")]
    pub token_env: String,
    /// "repo/relative/dir" entries the route counter must not enter (e.g. a nested mirror of other repositories).
    #[serde(default = "default_skip_paths")]
    pub skip_paths: Vec<String>,
}

impl Default for ApiEx {
    fn default() -> Self {
        Self {
            enabled: true,
            ledger: default_ledger(),
            search_url: default_search_url(),
            llm_url: default_llm_url(),
            llm_timeout_secs: default_llm_timeout(),
            search_n: default_search_n(),
            notify_url: default_notify_url(),
            token_env: default_token_env(),
            skip_paths: default_skip_paths(),
        }
    }
}

fn default_ledger() -> PathBuf {
    PathBuf::from("exceptions.toml")
}
fn default_search_url() -> String {
    "http://127.0.0.1:4610".into()
}
fn default_llm_url() -> String {
    "http://127.0.0.1:4600".into()
}
fn default_llm_timeout() -> u64 {
    45
}
fn default_search_n() -> usize {
    6
}
fn default_notify_url() -> String {
    "https://easy-web.tokyo".into()
}
fn default_token_env() -> String {
    "OPEN_EASYWEB_DIST_SYNC_ADMIN_TOKEN".into()
}
fn default_skip_paths() -> Vec<String> {
    vec!["runo/repository".into()]
}

fn yes() -> bool {
    true
}
fn default_cache_hours() -> u64 {
    20
}
fn default_max_items() -> usize {
    12
}
fn default_site() -> PathBuf {
    PathBuf::from("site")
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
        toml::from_str(&text).with_context(|| format!("invalid {}", path.display()))
    }
}

/// `[fix]`: whether `--fix --apply` may run here at all.
#[derive(Debug, Default, Deserialize, Clone)]
pub struct Fix {
    #[serde(default)]
    pub enabled: bool,
    /// Allow `--push` (otherwise changes stay as local commits in the work folder).
    #[serde(default)]
    pub allow_push: bool,
    /// Allow `--pr`: push a new branch `vc/...` (never force, never the default branch) and open a pull request with `gh`.
    #[serde(default)]
    pub allow_pr: bool,
    /// Repositories (folder names) whose lock file gets a semver-compatible `cargo update`. `["*"]` = all.
    #[serde(default)]
    pub lock_refresh: Vec<String>,
    /// Upper bound of repositories verified in parallel (0 = decided from cores and memory).
    #[serde(default)]
    pub max_workers: usize,
    /// Fast-forward clones that are clean and only behind their upstream (no build needed, so the VPS may do it).
    #[serde(default)]
    pub sync_clones: bool,
    /// Also raise outdated registry requirements (Cargo.toml / package.json) in verified work copies (`--auto`).
    #[serde(default)]
    pub upgrade: bool,
    /// Dependencies never raised automatically (names), e.g. a major that needs a human decision.
    #[serde(default)]
    pub upgrade_skip: Vec<String>,
    /// At most this many dependencies per repository in one run (0 = 12).
    #[serde(default)]
    pub upgrade_max: usize,
    /// At most this many repositories get a new upgrade pull request per run (0 = 5).
    #[serde(default)]
    pub upgrade_max_repos: usize,
}

/// `[selfupdate]`: how version-control brings itself up to date.
#[derive(Debug, Deserialize, Clone)]
pub struct SelfUpdate {
    #[serde(default)]
    pub enabled: bool,
    /// Folder of this program's own clone.
    #[serde(default = "default_self_dir")]
    pub dir: PathBuf,
    /// Build command run in `dir` (the result replaces `binary` only when it succeeds and passes a smoke test).
    #[serde(default = "default_self_build")]
    pub build_command: Vec<String>,
    /// The built program (relative to `dir`).
    #[serde(default = "default_self_binary")]
    pub binary: PathBuf,
}

impl Default for SelfUpdate {
    fn default() -> Self {
        Self { enabled: false, dir: default_self_dir(), build_command: default_self_build(), binary: default_self_binary() }
    }
}

fn default_self_dir() -> PathBuf {
    PathBuf::from(".")
}
fn default_self_build() -> Vec<String> {
    vec!["cargo".into(), "build".into(), "--release".into(), "--locked".into()]
}
fn default_self_binary() -> PathBuf {
    PathBuf::from(if cfg!(windows) { "target/release/version-control.exe" } else { "target/release/version-control" })
}

/// `[autoloop]`: the repeat cycle behind `--loop`.
#[derive(Debug, Deserialize)]
pub struct AutoLoop {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_cycles")]
    pub max_cycles: usize,
    /// Program + args; the prompt is written to its stdin.
    #[serde(default = "default_agent")]
    pub agent_command: Vec<String>,
    /// Empty = guess (Cargo.toml -> cargo test, package.json -> npm test, pytest).
    #[serde(default)]
    pub test_command: Vec<String>,
    #[serde(default = "default_loop_dir")]
    pub work_dir: PathBuf,
    #[serde(default)]
    pub allow_push: bool,
}

impl Default for AutoLoop {
    fn default() -> Self {
        Self { enabled: false, max_cycles: default_cycles(), agent_command: default_agent(), test_command: Vec::new(), work_dir: default_loop_dir(), allow_push: false }
    }
}

fn default_cycles() -> usize {
    4
}
fn default_agent() -> Vec<String> {
    vec!["claude".into(), "-p".into(), "--permission-mode".into(), "acceptEdits".into()]
}
fn default_loop_dir() -> PathBuf {
    PathBuf::from(".vc-loop")
}

/// `[review]`: what `--review` may do with open `vc/` pull requests.
#[derive(Debug, Default, Deserialize, Clone)]
pub struct Review {
    /// Squash-merge a pull request when every change in it stays inside the declared semver range (no major),
    /// GitHub says it is mergeable, no check failed and our own full test run passed. Majors are never merged.
    #[serde(default)]
    pub auto_merge: bool,
    /// GitHub owner whose pull requests are looked at.
    #[serde(default = "default_owner")]
    pub owner: String,
}

fn default_owner() -> String {
    "aon-co-jp".into()
}
