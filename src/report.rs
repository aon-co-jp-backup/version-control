//! The status model written to `status.json` (read by the dashboard) and the text sent to Claude.
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Item {
    /// "crates.io" | "npm" | "packagist" | "clone" | "manifest"
    pub source: String,
    pub name: String,
    pub req: String,
    pub latest: String,
    /// "patch" | "minor" | "major" | "behind" | "diverged" | "error"
    pub gap: String,
    /// "maintainer" (aruaru-llm / open-cuda side) or "claude"
    pub route: String,
    pub file: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct RepoStatus {
    pub name: String,
    pub deps_checked: usize,
    pub unchecked: usize,
    pub items: Vec<Item>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MaintainerStatus {
    pub name: String,
    pub alive: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Status {
    pub generated_at: u64,
    pub demo: bool,
    pub maintainers: Vec<MaintainerStatus>,
    pub repos: Vec<RepoStatus>,
    /// Number of private repositories left out of `repos`.
    pub hidden_private: usize,
}

impl RepoStatus {
    pub fn claude_items(&self) -> Vec<&Item> {
        let mut v: Vec<&Item> = self.items.iter().filter(|i| i.route == "claude").collect();
        v.sort_by_key(|i| (rank(&i.gap), i.name.clone()));
        v
    }
}

fn rank(gap: &str) -> u8 {
    match gap {
        "diverged" => 0,
        "error" => 1,
        "major" => 2,
        "behind" => 3,
        "minor" => 4,
        _ => 5,
    }
}

fn line(i: &Item) -> String {
    match i.gap.as_str() {
        "behind" => format!("- クローンがoriginより{}コミット遅れ ({})", i.latest, i.name),
        "diverged" => format!("- クローンとoriginが分岐 ({}) — 取り込む前にユーザーへ確認", i.latest),
        "error" => format!("- {}: {} ({})", i.source, i.latest, i.file),
        _ => format!("- {} {}: {} → {} [{}, {}]", i.source, i.name, i.req, i.latest, i.gap, i.file),
    }
}

/// The message to paste into Claude Code for one repository.
pub fn claude_prompt(root: &str, repo: &RepoStatus, max_items: usize) -> String {
    let items = repo.claude_items();
    let lines: Vec<String> = items.iter().take(max_items).map(|i| line(i)).collect();
    let more = items.len().saturating_sub(max_items);
    let mut s = format!("【{}】{}\\{} で次の更新が必要です(自動保守では対応しきれませんでした)。\n", repo.name, root.trim_end_matches(['/', '\\']), repo.name);
    for l in &lines {
        s.push_str(l);
        s.push('\n');
    }
    if more > 0 {
        s.push_str(&format!("…ほか{more}件(version-controlのstatus.jsonを参照)\n"));
    }
    s.push_str(
        "\nClaudeへの依頼: 上記を最新にして。手順=①git statusで作業ツリーを確認(未コミットがあれば触らない) \
         ②依存を更新しビルド・テスト ③破壊的変更はリリースノートを日英で調べて対応 ④README/CLAUDE/PORTINGを更新してpush。\
         クローンが遅れている/分岐している場合は`git pull`せず、`git fetch`のあと内容を確認(書き換え済みリポジトリで`git pull`すると古い履歴が戻る)。\
         秘密(トークン・パスワード)は表示・コミットしない。",
    );
    s
}

/// FNV-1a: stable across runs and Rust versions, so an unchanged problem is not announced twice.
pub fn fingerprint(text: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in text.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(gap: &str, route: &str) -> Item {
        Item { source: "crates.io".into(), name: "poem".into(), req: "3".into(), latest: "4.0.0".into(), gap: gap.into(), route: route.into(), file: "Cargo.toml".into() }
    }

    #[test]
    fn only_claude_items_are_in_prompt() {
        let r = RepoStatus { name: "x".into(), deps_checked: 2, unchecked: 0, items: vec![item("major", "claude"), item("patch", "maintainer")] };
        let p = claude_prompt("F:/", &r, 12);
        assert!(p.contains("poem") && p.contains("F:\\x") && p.matches("- crates.io").count() == 1);
    }

    #[test]
    fn fingerprint_is_stable() {
        assert_eq!(fingerprint("a"), fingerprint("a"));
        assert_ne!(fingerprint("a"), fingerprint("b"));
    }
}
