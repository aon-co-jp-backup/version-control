//! `--self-update`: version-control brings itself up to date from its own repository.
//!
//! Safe by construction: only a clean clone that is strictly behind its upstream is fast-forwarded (never a merge,
//! never a reset of someone's work); the new build must succeed and pass a smoke test (`--version`), otherwise the
//! clone is put back on the commit it had. The program that is running keeps running; the next run (or the re-exec the
//! caller does) uses the new build.
use crate::config::SelfUpdate;
use crate::git;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

#[derive(Debug, PartialEq)]
pub enum Decision {
    UpToDate,
    Update,
    Skip(&'static str),
}

/// `ahead`/`behind` are the counts against the upstream; `dirty` = tracked files with uncommitted changes.
pub fn decide(ahead: u32, behind: u32, dirty: bool) -> Decision {
    if behind == 0 {
        Decision::UpToDate
    } else if ahead > 0 {
        Decision::Skip("ローカルのコミットがあり origin と分岐している(自動では取り込まない)")
    } else if dirty {
        Decision::Skip("作業ツリーに未コミットの変更がある(触らない)")
    } else {
        Decision::Update
    }
}

fn counts(dir: &Path) -> Option<(u32, u32)> {
    let out = git(dir, &["rev-list", "--left-right", "--count", "HEAD...@{u}"], Duration::from_secs(20))?;
    let mut it = out.split_whitespace().filter_map(|x| x.parse::<u32>().ok());
    Some((it.next()?, it.next()?))
}

/// One line describing what happened. `Err` text starts with "失敗".
pub fn run(cfg: &SelfUpdate) -> Result<String, String> {
    run_with(cfg, true)
}

pub fn run_with(cfg: &SelfUpdate, smoke: bool) -> Result<String, String> {
    let dir = cfg.dir.as_path();
    if git(dir, &["fetch", "--quiet"], Duration::from_secs(60)).is_none() {
        return Err("失敗: git fetch できない(ネットワークか upstream 未設定)".into());
    }
    let (ahead, behind) = counts(dir).ok_or("失敗: upstream との差を調べられない")?;
    let dirty = git(dir, &["status", "--porcelain", "--untracked-files=no"], Duration::from_secs(20)).is_none_or(|s| !s.trim().is_empty());
    match decide(ahead, behind, dirty) {
        Decision::UpToDate => return Ok("version-control は最新".into()),
        Decision::Skip(why) => return Ok(format!("自己更新しない: {why}(origin より {behind} コミット遅れ)")),
        Decision::Update => {}
    }
    let old = git(dir, &["rev-parse", "HEAD"], Duration::from_secs(10)).ok_or("失敗: HEAD を読めない")?.trim().to_string();
    if git(dir, &["merge", "--ff-only", "@{u}"], Duration::from_secs(60)).is_none() {
        return Err("失敗: fast-forward できない".into());
    }
    let new = git(dir, &["rev-parse", "--short", "HEAD"], Duration::from_secs(10)).unwrap_or_default().trim().to_string();
    let rollback = |why: String| -> String {
        // Only our own fast-forward is undone (the tree was clean and strictly behind before).
        let _ = git(dir, &["reset", "--hard", "--quiet", &old], Duration::from_secs(60));
        format!("失敗: {why} → {} に戻した", &old[..old.len().min(7)])
    };
    let Some((prog, args)) = cfg.build_command.split_first() else { return Err(rollback("[selfupdate] build_command が空".into())) };
    let build = Command::new(prog).args(args).current_dir(dir).stdin(Stdio::null()).output();
    match build {
        Ok(o) if o.status.success() => {}
        Ok(o) => {
            let err = String::from_utf8_lossy(&o.stderr);
            let tail: Vec<&str> = err.lines().rev().take(3).collect();
            return Err(rollback(format!("ビルド失敗 ({})", tail.into_iter().rev().collect::<Vec<_>>().join(" | "))));
        }
        Err(e) => return Err(rollback(format!("ビルドを起動できない: {e}"))),
    }
    if smoke {
        let bin = dir.join(&cfg.binary);
        match Command::new(&bin).arg("--version").stdin(Stdio::null()).output() {
            Ok(o) if o.status.success() && String::from_utf8_lossy(&o.stdout).starts_with("version-control") => {}
            _ => return Err(rollback(format!("新しい build が起動しない({})", bin.display()))),
        }
    }
    Ok(format!("version-control を {} から {new} へ更新({behind} コミット、ビルドと起動確認済み)", &old[..old.len().min(7)]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decisions() {
        assert_eq!(decide(0, 0, false), Decision::UpToDate);
        assert_eq!(decide(0, 3, false), Decision::Update);
        assert!(matches!(decide(2, 3, false), Decision::Skip(_)));
        assert!(matches!(decide(0, 3, true), Decision::Skip(_)));
        assert_eq!(decide(2, 0, true), Decision::UpToDate);
    }

    fn sh(dir: &Path, cmd: &str) -> String {
        let o = Command::new("git").args(cmd.split(' ')).current_dir(dir).output().unwrap();
        assert!(o.status.success(), "git {cmd}: {}", String::from_utf8_lossy(&o.stderr));
        String::from_utf8_lossy(&o.stdout).to_string()
    }

    /// A real clone behind a real origin: fast-forwarded when clean, left alone when it has local work, rolled back when the build fails.
    #[test]
    fn fast_forwards_only_a_clean_clone_and_rolls_back_a_failed_build() {
        if Command::new("git").arg("--version").output().map_or(true, |o| !o.status.success()) {
            return;
        }
        let base = std::env::temp_dir().join(format!("vc-self-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let origin = base.join("origin");
        std::fs::create_dir_all(&origin).unwrap();
        let cfgs = "-c user.name=t -c user.email=t@t";
        let commit = |d: &Path, f: &str| {
            std::fs::write(d.join(f), f).unwrap();
            sh(d, "add -A");
            let o = Command::new("git").args(cfgs.split(' ')).args(["commit", "-qm", f]).current_dir(d).output().unwrap();
            assert!(o.status.success());
        };
        sh(&origin, "init -q -b main .");
        commit(&origin, "a");
        let clone = base.join("clone");
        sh(&base, &format!("clone -q {} clone", origin.display()));
        commit(&origin, "b");
        let cfg = SelfUpdate { enabled: true, dir: clone.clone(), build_command: vec!["git".into(), "--version".into()], binary: "x".into() };

        // dirty tracked file -> untouched
        std::fs::write(clone.join("a"), "edited").unwrap();
        let r = run_with(&cfg, false).unwrap();
        assert!(r.contains("自己更新しない"), "{r}");
        sh(&clone, "checkout -- a");

        // clean -> fast-forwarded
        let r = run_with(&cfg, false).unwrap();
        assert!(r.contains("更新"), "{r}");
        assert!(clone.join("b").exists());
        assert_eq!(run_with(&cfg, false).unwrap(), "version-control は最新");

        // build failure -> back on the old commit
        commit(&origin, "c");
        let bad = SelfUpdate { build_command: vec!["git".into(), "no-such-subcommand".into()], ..cfg.clone() };
        let before = sh(&clone, "rev-parse HEAD");
        let r = run_with(&bad, false).unwrap_err();
        assert!(r.contains("ビルド失敗") && r.contains("戻した"), "{r}");
        assert_eq!(sh(&clone, "rev-parse HEAD"), before);
        assert!(!clone.join("c").exists());
        let _ = std::fs::remove_dir_all(&base);
    }
}
