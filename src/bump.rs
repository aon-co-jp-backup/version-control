//! Manifest edits for automatic upgrades (`--edit-dep`, used by `scripts/upgrade.sh`).
//!
//! Cargo.toml is edited with `toml_edit` (comments and layout stay), package.json with a targeted text replacement.
//! The new requirement keeps the old one's operator and precision (`^1` -> `^3`, `0.60` -> `0.61`, `1.2.3` -> `3.5.2`).
use anyhow::{anyhow, bail, Result};
use toml_edit::{DocumentMut, Item, TableLike, Value};

/// The requirement text to write for `latest`, in the style of `old`. Comparison ranges / wildcards are not rewritten.
pub fn new_req(old: &str, latest: &str) -> Result<String> {
    let old = old.trim();
    let (op, body) = ["^", "~", "="].iter().find(|o| old.starts_with(**o)).map_or(("", old), |o| (*o, &old[o.len()..]));
    if body.is_empty() || !body.chars().next().is_some_and(|c| c.is_ascii_digit()) || body.contains([' ', ',', '|', '*', 'x', '<', '>']) {
        bail!("cannot rewrite requirement `{old}`");
    }
    let lat: Vec<&str> = latest.trim_start_matches('v').split(['.', '-', '+']).take(3).collect();
    if lat.len() < 3 || lat.iter().any(|p| p.is_empty() || !p.chars().all(|c| c.is_ascii_digit())) {
        bail!("cannot read version `{latest}`");
    }
    let mut n = body.split('.').count().clamp(1, 3);
    if lat[0] == "0" {
        n = n.max(2); // `0` would also allow 0.x breaking releases
    }
    Ok(format!("{op}{}", lat[..n].join(".")))
}

fn set_req(item: &mut Item, latest: &str) -> Result<bool> {
    let Some(v) = item.as_value_mut() else { return Ok(false) };
    let Some(old) = v.as_str().map(str::to_string) else { return Ok(false) };
    let new = new_req(&old, latest)?;
    if new == old {
        return Ok(false);
    }
    let decor = v.decor().clone();
    let mut nv = Value::from(new);
    *nv.decor_mut() = decor;
    *v = nv;
    Ok(true)
}

/// Edits one dependency entry; returns whether a requirement was changed.
fn edit_entry(entry: &mut Item, latest: &str) -> Result<bool> {
    if entry.is_str() {
        return set_req(entry, latest);
    }
    let Some(t) = entry.as_table_like_mut() else { return Ok(false) };
    if t.get("workspace").is_some() {
        return Ok(false); // the version lives in [workspace.dependencies]
    }
    match t.get_mut("version") {
        Some(v) => set_req(v, latest),
        None => Ok(false), // path / git dependency
    }
}

fn edit_dep_table(deps: &mut dyn TableLike, krate: &str, latest: &str) -> Result<usize> {
    let keys: Vec<String> = deps.iter().map(|(k, _)| k.to_string()).collect();
    let mut n = 0;
    for k in keys {
        let Some(entry) = deps.get_mut(&k) else { continue };
        let renamed = entry.as_table_like().and_then(|t| t.get("package")).and_then(|p| p.as_str()).map(str::to_string);
        if renamed.as_deref().unwrap_or(&k) == krate && edit_entry(entry, latest)? {
            n += 1;
        }
    }
    Ok(n)
}

const KINDS: [&str; 3] = ["dependencies", "dev-dependencies", "build-dependencies"];

/// Sets every registry requirement for `krate` in a Cargo.toml (all dependency tables, `[workspace.dependencies]`,
/// `[target.*]`). Returns the new text and the number of requirements changed.
pub fn edit_cargo(text: &str, krate: &str, latest: &str) -> Result<(String, usize)> {
    let mut doc: DocumentMut = text.parse().map_err(|e| anyhow!("invalid Cargo.toml: {e}"))?;
    let mut n = 0;
    for kind in KINDS {
        if let Some(t) = doc.get_mut(kind).and_then(Item::as_table_like_mut) {
            n += edit_dep_table(t, krate, latest)?;
        }
    }
    if let Some(t) = doc.get_mut("workspace").and_then(Item::as_table_like_mut).and_then(|w| w.get_mut("dependencies")).and_then(Item::as_table_like_mut) {
        n += edit_dep_table(t, krate, latest)?;
    }
    if let Some(targets) = doc.get_mut("target").and_then(Item::as_table_like_mut) {
        let cfgs: Vec<String> = targets.iter().map(|(k, _)| k.to_string()).collect();
        for c in cfgs {
            let Some(tt) = targets.get_mut(&c).and_then(Item::as_table_like_mut) else { continue };
            for kind in KINDS {
                if let Some(t) = tt.get_mut(kind).and_then(Item::as_table_like_mut) {
                    n += edit_dep_table(t, krate, latest)?;
                }
            }
        }
    }
    Ok((doc.to_string(), n))
}

/// Same for package.json: only entries whose value looks like a version requirement are touched.
pub fn edit_package_json(text: &str, name: &str, latest: &str) -> Result<(String, usize)> {
    let re = regex::Regex::new(&format!(r#"("{}"\s*:\s*")([^"]*)(")"#, regex::escape(name)))?;
    let mut n = 0;
    let mut err = None;
    let out = re.replace_all(text, |c: &regex::Captures| {
        let old = &c[2];
        if !old.starts_with(|ch: char| ch.is_ascii_digit() || ch == '^' || ch == '~') {
            return c[0].to_string();
        }
        match new_req(old, latest) {
            Ok(new) if new != old => {
                n += 1;
                format!("{}{}{}", &c[1], new, &c[3])
            }
            Ok(_) => c[0].to_string(),
            Err(e) => {
                err = Some(e);
                c[0].to_string()
            }
        }
    });
    match (n, err) {
        (0, Some(e)) => Err(e),
        _ => Ok((out.into_owned(), n)),
    }
}

/// `version-control --edit-dep FILE NAME LATEST`: edits FILE in place (Cargo.toml or package.json).
pub fn edit_file(path: &str, name: &str, latest: &str) -> Result<usize> {
    let text = std::fs::read_to_string(path)?;
    let (out, n) = if path.ends_with("package.json") { edit_package_json(&text, name, latest)? } else { edit_cargo(&text, name, latest)? };
    if n == 0 {
        bail!("{name} has no registry requirement to change in {path}");
    }
    std::fs::write(path, out)?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requirement_keeps_style() {
        assert_eq!(new_req("0.60", "0.61.2").unwrap(), "0.61");
        assert_eq!(new_req("^1.2", "3.5.2").unwrap(), "^3.5");
        assert_eq!(new_req("2", "3.5.2").unwrap(), "3");
        assert_eq!(new_req("~1.2.3", "1.9.0").unwrap(), "~1.9.0");
        assert_eq!(new_req("=0.3.1", "0.5.10").unwrap(), "=0.5.10");
        assert_eq!(new_req("0", "0.5.10").unwrap(), "0.5");
        assert!(new_req(">=1, <2", "3.0.0").is_err());
        assert!(new_req("*", "3.0.0").is_err());
    }

    #[test]
    fn cargo_edit_covers_every_table_and_keeps_layout() {
        let src = r#"[package]
name = "x"

[dependencies]
ureq = "2"          # http
sonic-rs = { version = "0.3", features = ["x"] }
local = { path = "../local" }
shared = { workspace = true }

[dev-dependencies]
ureq = { version = "2.1", default-features = false }

[target.'cfg(windows)'.dependencies]
ureq = "2"

[workspace.dependencies]
shared = "1"
"#;
        let (out, n) = edit_cargo(src, "ureq", "3.5.2").unwrap();
        assert_eq!(n, 3);
        assert!(out.contains("ureq = \"3\"          # http"), "{out}");
        assert!(out.contains("ureq = { version = \"3.5\", default-features = false }"), "{out}");
        assert!(out.contains("sonic-rs = { version = \"0.3\", features = [\"x\"] }"));
        let (out, n) = edit_cargo(src, "sonic-rs", "0.5.10").unwrap();
        assert_eq!(n, 1);
        assert!(out.contains("sonic-rs = { version = \"0.5\", features = [\"x\"] }"), "{out}");
        assert_eq!(edit_cargo(src, "local", "1.0.0").unwrap().1, 0);
        assert_eq!(edit_cargo(src, "shared", "2.0.0").unwrap().1, 1); // only the workspace table
    }

    #[test]
    fn renamed_dependency_is_found_by_package() {
        let src = "[dependencies]\nrsa-090 = { package = \"rsa\", version = \"0.9\" }\n";
        assert_eq!(edit_cargo(src, "rsa", "0.10.1").unwrap().1, 1);
    }

    #[test]
    fn package_json_only_touches_requirements() {
        let src = "{\n  \"name\": \"typescript\",\n  \"devDependencies\": {\n    \"typescript\": \"^5.4.0\",\n    \"jest\": \"^29.4.0\"\n  }\n}\n";
        let (out, n) = edit_package_json(src, "typescript", "7.0.2").unwrap();
        assert_eq!(n, 1);
        assert!(out.contains("\"typescript\": \"^7.0.2\"") && out.contains("\"name\": \"typescript\"") && out.contains("\"jest\": \"^29.4.0\""), "{out}");
    }
}
