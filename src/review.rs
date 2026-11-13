//! `--review`: triage of the pull requests `--auto` opened (`vc/upgrade-*`).
//!
//! The machine-readable list of changes is embedded in each pull request body by `scripts/upgrade.sh`
//! (`<!-- vc-upgrade: name | old requirement | new version -->`). A pull request whose every change stays inside the
//! declared semver range (no new major; for 0.x no new minor), that GitHub reports as mergeable, with no failed check
//! and with our own full test run marked as passed, is squash-merged when `[review] auto_merge` is on. Everything else
//! stays open and is listed with what a person has to check (release notes of each major).
use crate::config::Review;
use crate::scan::{classify, parse_req, parse_ver, Gap};
use std::process::Command;

#[derive(Debug, Clone, PartialEq)]
pub struct Change {
    pub name: String,
    pub from: String,
    pub to: String,
}

/// The `vc-upgrade` markers of a pull request body.
pub fn parse_markers(body: &str) -> Vec<Change> {
    body.lines()
        .filter_map(|l| {
            let inner = l.trim().strip_prefix("<!-- vc-upgrade:")?.strip_suffix("-->")?;
            let mut p = inner.split('|').map(|x| x.trim().to_string());
            Some(Change { name: p.next()?, from: p.next()?, to: p.next()? })
        })
        .collect()
}

/// A change is risky when the new version leaves the declared range, or when we cannot tell.
pub fn risky(c: &Change) -> bool {
    match (parse_req(&c.from), parse_ver(&c.to)) {
        (Some(cur), Some(latest)) => classify(&c.from, &cur, &latest) == Gap::Major,
        _ => true,
    }
}

#[derive(Debug, PartialEq)]
pub enum Verdict {
    Merge,
    /// Stays open, with the reason.
    Wait(String),
}

pub fn decide(changes: &[Change], mergeable: &str, check_failed: bool, tests_passed: bool, auto_merge: bool) -> Verdict {
    if changes.is_empty() {
        return Verdict::Wait("変更の一覧(vc-upgrade)が無い PR(人が確認)".into());
    }
    let majors: Vec<&str> = changes.iter().filter(|c| risky(c)).map(|c| c.name.as_str()).collect();
    if !majors.is_empty() {
        return Verdict::Wait(format!("破壊的変更の可能性: {}(リリースノートを確認)", majors.join(", ")));
    }
    if check_failed {
        return Verdict::Wait("CI のチェックが失敗している".into());
    }
    if !tests_passed {
        return Verdict::Wait("まとめてのテストが合格と記録されていない".into());
    }
    if mergeable != "MERGEABLE" {
        return Verdict::Wait(format!("GitHub がマージ可能と判定していない({mergeable})"));
    }
    if !auto_merge {
        return Verdict::Wait("互換範囲内で安全だが、[review] auto_merge が false".into());
    }
    Verdict::Merge
}

fn gh(args: &[&str]) -> Option<String> {
    let o = Command::new("gh").args(args).output().ok()?;
    o.status.success().then(|| String::from_utf8_lossy(&o.stdout).to_string())
}

/// Where a person looks up what changed in a major release.
fn notes_url(name: &str) -> String {
    format!("https://crates.io/crates/{name}/versions (npm: https://www.npmjs.com/package/{name}?activeTab=versions)")
}

/// One line per open upgrade pull request: merged, or why it waits and what to check.
pub fn run(cfg: &Review) -> Vec<String> {
    use sonic_rs::{JsonContainerTrait, JsonValueTrait};
    let parse = |t: &str| rust_json::parse_secure_sonic(t, &rust_json::Limits::default()).ok();
    let owner = format!("--owner={}", cfg.owner);
    let Some(list) = gh(&["search", "prs", &owner, "--state=open", "--limit=100", "--json", "repository,number,title"]).and_then(|t| parse(&t)) else {
        return vec!["RESULT review: gh で PR の一覧を取得できない(gh のログインを確認)".into()];
    };
    let mut out = Vec::new();
    for pr in list.as_array().into_iter().flat_map(|a| a.iter()) {
        let title = pr.get("title").and_then(|x| x.as_str()).unwrap_or("");
        if !title.starts_with("deps: 依存を最新へ更新(version-control)") {
            continue;
        }
        let (Some(repo), Some(num)) = (pr.get("repository").and_then(|r| r.get("nameWithOwner")).and_then(|x| x.as_str()), pr.get("number").and_then(|x| x.as_i64())) else { continue };
        let n = num.to_string();
        let Some(v) = gh(&["pr", "view", &n, "--repo", repo, "--json", "headRefName,mergeable,statusCheckRollup,body"]).and_then(|t| parse(&t)) else {
            out.push(format!("RESULT {repo}#{n}: PR の詳細を取得できない"));
            continue;
        };
        if !v.get("headRefName").and_then(|x| x.as_str()).unwrap_or("").starts_with("vc/") {
            continue;
        }
        let body = v.get("body").and_then(|x| x.as_str()).unwrap_or("");
        let changes = parse_markers(body);
        let failed = v.get("statusCheckRollup").and_then(|x| x.as_array()).is_some_and(|a| {
            a.iter().any(|c| {
                let s = c.get("conclusion").and_then(|x| x.as_str()).or_else(|| c.get("state").and_then(|x| x.as_str())).unwrap_or("");
                ["FAILURE", "ERROR", "TIMED_OUT", "CANCELLED", "ACTION_REQUIRED"].contains(&s)
            })
        });
        let passed = body.contains("まとめてのテストは PASS");
        let mergeable = v.get("mergeable").and_then(|x| x.as_str()).unwrap_or("UNKNOWN");
        match decide(&changes, mergeable, failed, passed, cfg.auto_merge) {
            Verdict::Merge => {
                let merged = gh(&["pr", "merge", &n, "--repo", repo, "--squash", "--delete-branch"]).is_some();
                out.push(if merged { format!("RESULT {repo}#{n}: 互換範囲内のためマージした(更新: {})", names(&changes)) } else { format!("RESULT {repo}#{n}: マージに失敗(未対応: gh pr merge を確認)") });
            }
            Verdict::Wait(why) => {
                let check: Vec<String> = changes.iter().filter(|c| risky(c)).map(|c| format!("{} {}→{} {}", c.name, c.from, c.to, notes_url(&c.name))).collect();
                let tail = if check.is_empty() { String::new() } else { format!(" / 確認: {}", check.join("; ")) };
                out.push(format!("RESULT {repo}#{n}: 開いたまま — {why}{tail}"));
            }
        }
    }
    if out.is_empty() {
        out.push("RESULT review: 開いている更新 PR は無い".into());
    }
    out
}

fn names(c: &[Change]) -> String {
    c.iter().map(|x| x.name.as_str()).collect::<Vec<_>>().join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(name: &str, from: &str, to: &str) -> Change {
        Change { name: name.into(), from: from.into(), to: to.into() }
    }

    #[test]
    fn markers_are_read_from_the_body() {
        let body = "text\n<!-- vc-upgrade: ureq | 2 | 3.5.2 -->\n<!-- vc-upgrade: toml | 0.8 | 1.2.1 -->\n🤖";
        assert_eq!(parse_markers(body), vec![c("ureq", "2", "3.5.2"), c("toml", "0.8", "1.2.1")]);
        assert!(parse_markers("no markers").is_empty());
    }

    #[test]
    fn majors_and_unknowns_are_risky_compatible_ones_are_not() {
        assert!(risky(&c("a", "2", "3.5.2")));
        assert!(risky(&c("a", "0.60", "0.61.2"))); // 0.x minor is breaking
        assert!(risky(&c("a", "", "1.0.0")));
        assert!(!risky(&c("a", "1", "1.9.0")));
        assert!(!risky(&c("a", "^1.2", "1.9.3")));
        assert!(!risky(&c("a", "0.60", "0.60.5")));
    }

    #[test]
    fn only_safe_green_mergeable_prs_are_merged() {
        let safe = [c("a", "1", "1.9.0")];
        assert_eq!(decide(&safe, "MERGEABLE", false, true, true), Verdict::Merge);
        assert!(matches!(decide(&safe, "MERGEABLE", false, true, false), Verdict::Wait(_)));
        assert!(matches!(decide(&safe, "CONFLICTING", false, true, true), Verdict::Wait(_)));
        assert!(matches!(decide(&safe, "MERGEABLE", true, true, true), Verdict::Wait(_)));
        assert!(matches!(decide(&safe, "MERGEABLE", false, false, true), Verdict::Wait(_)));
        assert!(matches!(decide(&[c("a", "1", "1.9.0"), c("b", "2", "3.0.0")], "MERGEABLE", false, true, true), Verdict::Wait(_)));
        assert!(matches!(decide(&[], "MERGEABLE", false, true, true), Verdict::Wait(_)));
    }

    /// Against the real GitHub (needs a logged-in `gh`): `cargo test live_review -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn live_review() {
        let lines = run(&Review { auto_merge: true, owner: "aon-co-jp".into() });
        for l in &lines {
            println!("{l}");
        }
        assert!(!lines.is_empty());
    }
}
