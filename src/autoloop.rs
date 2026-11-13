//! `--loop`: a Claude Code Desktop-style repeat cycle run unattended in a fresh clone.
//!
//! Each cycle runs the phases investigate -> confirm -> prototype/develop -> improve -> test -> debug.
//! The agent (default `claude -p`, prompt on stdin) does the thinking; this module owns the loop: it runs the
//! test command itself, feeds failures back, and stops when the tests are green AND the agent's review says
//! `LOOP_DONE`, or after `max_cycles`. Work happens only in a clone of `origin`; it is committed locally and pushed
//! only with `--push` plus `[autoloop] allow_push = true`. Nothing in the working tree is touched.
use crate::config::AutoLoop;
use anyhow::{anyhow, Context, Result};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub const PHASES: [(&str, &str); 6] = [
    ("investigate", "Investigate: read the code and the goal, list facts you verified (file:line) and what is still unknown."),
    ("confirm", "Confirm: check each assumption against the code/tests/docs; do not guess. Say what you could not confirm."),
    ("develop", "Prototype and develop: make the smallest change that reaches the goal; scale up only after it works."),
    ("improve", "Improve: simplify, remove duplication, update README/CLAUDE/PORTING/POLICY in Japanese and English if behaviour changed."),
    ("test", "Test: add or fix tests for the change. Do not weaken or delete existing tests."),
    ("debug", "Debug: fix every failure reported below at its cause. If everything passes, review the diff once more."),
];

#[derive(Debug, PartialEq)]
pub enum Outcome {
    Done { cycles: usize },
    Exhausted { cycles: usize, last_failure: String },
}

/// Test command: configured one, else guessed from the files in the clone.
pub fn test_command(cfg: &AutoLoop, dir: &Path) -> Option<Vec<String>> {
    if !cfg.test_command.is_empty() {
        return Some(cfg.test_command.clone());
    }
    let has = |f: &str| dir.join(f).exists();
    if has("Cargo.toml") {
        Some(vec!["cargo".into(), "test".into(), "--no-fail-fast".into()])
    } else if has("package.json") {
        Some(vec!["npm".into(), "test".into(), "--silent".into()])
    } else if has("pyproject.toml") || has("pytest.ini") {
        Some(vec!["python".into(), "-m".into(), "pytest".into(), "-q".into()])
    } else {
        None
    }
}

pub fn phase_prompt(goal: &str, cycle: usize, max: usize, failure: &str) -> String {
    let mut p = format!("Goal: {goal}\n\nThis is cycle {cycle} of at most {max}. Work in the current directory only.\nDo these phases in order, briefly:\n");
    for (i, (_, text)) in PHASES.iter().enumerate() {
        p.push_str(&format!("{}. {}\n", i + 1, text));
    }
    if !failure.is_empty() {
        p.push_str("\nThe previous test run FAILED. Fix the cause first:\n----\n");
        p.push_str(failure);
        p.push_str("\n----\n");
    }
    p.push_str("\nRules: no stop-gap or half-wired code; never print secrets; never push.\nWhen the goal is fully reached and the review finds nothing more, end your reply with the single line LOOP_DONE.\n");
    p
}

fn tail(s: &str, n: usize) -> String {
    let lines: Vec<&str> = s.lines().collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}

fn run_agent(cfg: &AutoLoop, dir: &Path, prompt: &str) -> Result<String> {
    let (prog, args) = cfg.agent_command.split_first().ok_or_else(|| anyhow!("[autoloop] agent_command is empty"))?;
    let mut child = Command::new(prog)
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("cannot start agent `{prog}`"))?;
    child.stdin.take().ok_or_else(|| anyhow!("no stdin"))?.write_all(prompt.as_bytes())?;
    let out = child.wait_with_output()?;
    if !out.status.success() {
        // Claude Code prints errors such as "Not logged in" on stdout, so show both.
        let msg = format!("{}
{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
        return Err(anyhow!("agent exited with {}: {}", out.status, tail(msg.trim(), 5)));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

/// Ok(None) = green; Ok(Some(tail of output)) = red.
fn run_tests(cmd: &[String], dir: &Path) -> Result<Option<String>> {
    let (prog, args) = cmd.split_first().ok_or_else(|| anyhow!("empty test command"))?;
    let o = Command::new(prog).args(args).current_dir(dir).output().with_context(|| format!("cannot run `{prog}`"))?;
    if o.status.success() {
        return Ok(None);
    }
    let text = format!("{}\n{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
    Ok(Some(tail(&text, 60)))
}

/// The loop itself, with the agent and the tests injected so it can be tested without either.
pub fn drive(
    max: usize,
    goal: &str,
    mut agent: impl FnMut(&str) -> Result<String>,
    mut tests: impl FnMut() -> Result<Option<String>>,
) -> Result<Outcome> {
    let mut failure = String::new();
    for cycle in 1..=max {
        let reply = agent(&phase_prompt(goal, cycle, max, &failure))?;
        let done = reply.lines().rev().find(|l| !l.trim().is_empty()).map(|l| l.trim() == "LOOP_DONE").unwrap_or(false);
        match tests()? {
            None if done => return Ok(Outcome::Done { cycles: cycle }),
            None => failure.clear(),
            Some(f) => failure = f,
        }
    }
    Ok(Outcome::Exhausted { cycles: max, last_failure: failure })
}

pub fn run(cfg: &AutoLoop, repo: &str, origin: &str, goal: &str, push: bool, pr: bool) -> Result<String> {
    let work: PathBuf = cfg.work_dir.join(repo);
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&cfg.work_dir)?;
    let st = Command::new("git").args(["clone", "-q", origin]).arg(&work).status()?;
    if !st.success() {
        return Err(anyhow!("clone of {origin} failed"));
    }
    let tcmd = test_command(cfg, &work).ok_or_else(|| anyhow!("no test command for {repo}: set [autoloop] test_command"))?;
    let outcome = drive(cfg.max_cycles, goal, |p| run_agent(cfg, &work, p), || run_tests(&tcmd, &work))?;
    let dirty = Command::new("git").arg("-C").arg(&work).args(["status", "--porcelain"]).output()?;
    let changed = !dirty.stdout.is_empty();
    let mut line = match &outcome {
        Outcome::Done { cycles } => format!("RESULT {repo}: LOOP_DONE after {cycles} cycle(s), tests green"),
        Outcome::Exhausted { cycles, last_failure } => {
            format!("RESULT {repo}: not finished after {cycles} cycle(s); last failure: {}", tail(last_failure, 3).replace('\n', " | "))
        }
    };
    // Only a finished, green loop is committed; an unfinished one stays uncommitted in the work folder for a human.
    if matches!(outcome, Outcome::Done { .. }) && changed {
        let git = |a: &[&str]| Command::new("git").arg("-C").arg(&work).args(a).status().map(|s| s.success()).unwrap_or(false);
        let msg = format!("auto-loop: {goal}\n\nCo-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>");
        if git(&["add", "-A"]) && git(&["commit", "-q", "-m", &msg]) {
            line.push_str("; committed locally");
            if pr {
                // Review path: a new branch and a pull request, never the default branch.
                let br = format!("vc/loop-{}", std::process::id());
                let slug = origin.trim_start_matches("https://github.com/").trim_end_matches(".git").to_string();
                if git(&["push", "-q", "origin", &format!("HEAD:refs/heads/{br}")]) {
                    let title = format!("auto-loop: {}", goal.lines().next().unwrap_or(""));
                    let body = "version-control --auto のエージェントによる更新です。マージ前に差分を確認してください。\n\n🤖 Generated with [Claude Code](https://claude.com/claude-code)";
                    let made = Command::new("gh").args(["pr", "create", "--repo", &slug, "--head", &br, "--title", &title, "--body", body]).output();
                    match made {
                        Ok(o) if o.status.success() => line.push_str(&format!("; PR {}", String::from_utf8_lossy(&o.stdout).trim())),
                        _ => line.push_str(&format!("; branch {br} pushed (PR creation failed)")),
                    }
                } else {
                    line.push_str("; branch push FAILED");
                }
            } else if push && cfg.allow_push {
                line.push_str(if git(&["push", "-q", "origin", "HEAD"]) { "; pushed" } else { "; push FAILED" });
            }
        }
    }
    Ok(line)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stops_when_green_and_done() {
        let mut n = 0;
        let r = drive(5, "g", |_| { n += 1; Ok(if n == 2 { "x\nLOOP_DONE\n".into() } else { "x".into() }) }, || Ok(None)).unwrap();
        assert_eq!(r, Outcome::Done { cycles: 2 });
    }

    #[test]
    fn red_tests_block_done_and_feed_back() {
        let mut seen = Vec::new();
        let mut t = 0;
        let r = drive(
            3,
            "g",
            |p| { seen.push(p.contains("FAILED")); Ok("LOOP_DONE".into()) },
            || { t += 1; Ok(if t < 3 { Some("boom".into()) } else { None }) },
        )
        .unwrap();
        assert_eq!(r, Outcome::Done { cycles: 3 });
        assert_eq!(seen, vec![false, true, true]);
    }

    #[test]
    fn gives_up_after_max_cycles() {
        let r = drive(2, "g", |_| Ok("hi".into()), || Ok(Some("red".into()))).unwrap();
        assert_eq!(r, Outcome::Exhausted { cycles: 2, last_failure: "red".into() });
    }
}
