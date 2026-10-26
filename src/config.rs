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

#[derive(Debug, Default, Deserialize)]
pub struct Notify {
    #[serde(default)]
    pub email_to: Vec<String>,
    pub line_token_file: Option<PathBuf>,
    pub line_to_file: Option<PathBuf>,
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
