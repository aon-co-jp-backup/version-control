//! Finds the manifests of one repository and extracts the registry dependencies they declare.
use serde_json::Value as Json;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Eco {
    Cargo,
    Npm,
    Composer,
}

impl Eco {
    pub fn label(self) -> &'static str {
        match self {
            Eco::Cargo => "crates.io",
            Eco::Npm => "npm",
            Eco::Composer => "packagist",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Dep {
    pub eco: Eco,
    pub name: String,
    pub req: String,
    /// Manifest path relative to the repository.
    pub file: String,
}

const SKIP_DIRS: &[&str] = &[".git", "node_modules", "target", "vendor", "dist", "build", ".venv", "pkg"];

pub fn find_manifests(repo: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    walk(repo, 0, &mut out);
    out
}

fn walk(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > 4 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        let name = e.file_name().to_string_lossy().to_string();
        if p.is_dir() {
            if !SKIP_DIRS.contains(&name.as_str()) {
                walk(&p, depth + 1, out);
            }
        } else if matches!(name.as_str(), "Cargo.toml" | "package.json" | "composer.json") {
            out.push(p);
        }
    }
}

pub fn parse_manifest(repo: &Path, path: &Path) -> Result<Vec<Dep>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let rel = path.strip_prefix(repo).unwrap_or(path).to_string_lossy().replace('\\', "/");
    match path.file_name().and_then(|n| n.to_str()) {
        Some("Cargo.toml") => parse_cargo(&text, &rel),
        Some("package.json") => parse_json(&text, &rel, Eco::Npm, &["dependencies", "devDependencies"]),
        Some("composer.json") => parse_json(&text, &rel, Eco::Composer, &["require", "require-dev"]),
        _ => Ok(vec![]),
    }
}

fn parse_cargo(text: &str, file: &str) -> Result<Vec<Dep>, String> {
    let doc: toml::Table = toml::from_str(text).map_err(|e| e.to_string())?;
    fn add<'a>(tables: &mut Vec<&'a toml::Table>, t: Option<&'a toml::Value>) {
        if let Some(t) = t.and_then(|v| v.as_table()) {
            tables.push(t);
        }
    }
    let mut tables: Vec<&toml::Table> = Vec::new();
    for k in ["dependencies", "dev-dependencies", "build-dependencies"] {
        add(&mut tables, doc.get(k));
    }
    add(&mut tables, doc.get("workspace").and_then(|w| w.get("dependencies")));
    if let Some(targets) = doc.get("target").and_then(|t| t.as_table()) {
        for t in targets.values() {
            for k in ["dependencies", "dev-dependencies", "build-dependencies"] {
                add(&mut tables, t.get(k));
            }
        }
    }
    let mut out = Vec::new();
    for t in tables {
        for (key, v) in t {
            let (req, name) = match v {
                toml::Value::String(s) => (s.clone(), key.clone()),
                toml::Value::Table(d) => {
                    if d.contains_key("path") || d.contains_key("git") {
                        continue; // sibling / git dependency: not a registry version
                    }
                    let Some(req) = d.get("version").and_then(|x| x.as_str()) else { continue };
                    let name = d.get("package").and_then(|x| x.as_str()).unwrap_or(key).to_string();
                    (req.to_string(), name)
                }
                _ => continue,
            };
            out.push(Dep { eco: Eco::Cargo, name, req, file: file.to_string() });
        }
    }
    Ok(out)
}

fn parse_json(text: &str, file: &str, eco: Eco, sections: &[&str]) -> Result<Vec<Dep>, String> {
    // Manifests can be large (lock-like package.json files): use the default limits, not `small`.
    let v: Json = rust_json::parse_secure(text, &rust_json::Limits::default()).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for s in sections {
        let Some(map) = v.get(*s).and_then(|m| m.as_object()) else { continue };
        for (name, req) in map {
            let Some(req) = req.as_str() else { continue };
            if eco == Eco::Composer && (name == "php" || name.starts_with("ext-") || name.starts_with("lib-")) {
                continue;
            }
            out.push(Dep { eco, name: name.clone(), req: req.to_string(), file: file.to_string() });
        }
    }
    Ok(out)
}

/// A parsed version: the numbers that were written, so `^1.2` only compares major and minor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ver(pub Vec<u64>);

pub fn parse_req(req: &str) -> Option<Ver> {
    // Use the last alternative (`^7.4 || ^8.0`), skip non-registry forms.
    let alt = req.split('|').filter(|s| !s.trim().is_empty()).last()?.trim();
    let lower = alt.to_ascii_lowercase();
    let non_registry = ["workspace", "file:", "link:", "git", "http", "npm:", "dev-", "latest", "*"];
    if non_registry.iter().any(|p| lower.starts_with(p)) || alt.contains('/') {
        return None;
    }
    // Ranges such as ">=1.0 <2.0": take the last version token.
    let tok = alt.split_whitespace().last()?;
    parse_ver(tok)
}

pub fn parse_ver(s: &str) -> Option<Ver> {
    let s = s.trim_start_matches(|c: char| !c.is_ascii_digit());
    let head: String = s.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
    let nums: Vec<u64> = head.split('.').filter(|p| !p.is_empty()).map(|p| p.parse().ok()).collect::<Option<_>>()?;
    if nums.is_empty() {
        None
    } else {
        Some(Ver(nums))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gap {
    Current,
    Patch,
    Minor,
    /// A new major, or a new minor of a 0.x crate (which is breaking in semver).
    Major,
}

/// How far behind `latest` is, given how the requirement is written: a caret/bare requirement
/// (`1.2`, `^1.2.3`) already allows every compatible release, so only a new major (or a new minor
/// of a 0.x crate) is a gap; `~` also locks the minor; `=` pins everything.
pub fn classify(req: &str, cur: &Ver, latest: &Ver) -> Gap {
    let r = req.split('|').filter(|s| !s.trim().is_empty()).last().unwrap_or("").trim();
    let g = classify_all(cur, latest);
    if r.starts_with('=') {
        return g;
    }
    match g {
        Gap::Major => Gap::Major,
        Gap::Minor if r.starts_with('~') => Gap::Minor,
        _ => Gap::Current,
    }
}

fn classify_all(cur: &Ver, latest: &Ver) -> Gap {
    let c = |i: usize| cur.0.get(i).copied();
    let l = |i: usize| latest.0.get(i).copied().unwrap_or(0);
    // Only the components that were written count: `1.2` already allows 1.2.9.
    if c(0).is_some_and(|v| l(0) > v) {
        return Gap::Major;
    }
    if c(0) == Some(0) && l(0) == 0 && c(1).is_some_and(|v| l(1) > v) {
        return Gap::Major;
    }
    if c(1).is_some_and(|v| l(1) > v) {
        return Gap::Minor;
    }
    if c(2).is_some_and(|v| l(2) > v) {
        return Gap::Patch;
    }
    Gap::Current
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gap(req: &str, latest: &str) -> Gap {
        classify(req, &parse_req(req).unwrap(), &parse_ver(latest).unwrap())
    }

    #[test]
    fn classifies() {
        assert_eq!(gap("1", "1.9.3"), Gap::Current);
        assert_eq!(gap("1.2", "1.2.9"), Gap::Current);
        assert_eq!(gap("=1.2.3", "1.2.9"), Gap::Patch);
        assert_eq!(gap("=1.2.3", "1.4.0"), Gap::Minor);
        assert_eq!(gap("~1.2.3", "1.4.0"), Gap::Minor);
        assert_eq!(gap("^1.2.3", "1.4.0"), Gap::Current);
        assert_eq!(gap("^1.2", "2.0.0"), Gap::Major);
        assert_eq!(gap("0.12", "0.13.1"), Gap::Major);
        assert_eq!(gap("0.12", "0.12.5"), Gap::Current);
        assert_eq!(gap("^7.4 || ^8.0", "8.3.1"), Gap::Current);
        assert_eq!(gap(">=1.0 <2.0", "3.0.0"), Gap::Major);
    }

    #[test]
    fn skips_non_registry() {
        for r in ["workspace:*", "file:../x", "github:a/b", "*", "latest", "dev-main", "user/repo"] {
            assert!(parse_req(r).is_none(), "{r}");
        }
    }

    #[test]
    fn cargo_manifest() {
        let t = r#"
[dependencies]
serde = "1"
poem = { version = "3.1", features = ["x"] }
sib = { path = "../sib", version = "0.1" }
re = { package = "regex", version = "1.5" }
[dev-dependencies]
tokio = { version = "1", default-features = false }
[workspace.dependencies]
lettre = "0.11"
"#;
        let d = parse_cargo(t, "Cargo.toml").unwrap();
        let names: Vec<_> = d.iter().map(|x| x.name.as_str()).collect();
        assert!(names.contains(&"serde") && names.contains(&"poem") && names.contains(&"regex"));
        assert!(names.contains(&"tokio") && names.contains(&"lettre"));
        assert!(!names.contains(&"sib"));
    }
}
