//! E-mail (SMTP, same environment variables as open-easy-web) and LINE (Messaging API push) delivery.
//! Secrets are read from environment variables / txt files and never printed.
use crate::config::Notify;
use anyhow::{anyhow, Context, Result};
use lettre::message::Mailbox;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{Message, SmtpTransport, Transport};

pub struct Outcome {
    pub email: Result<(), String>,
    pub line: Result<(), String>,
}

impl Outcome {
    pub fn any_ok(&self) -> bool {
        self.email.is_ok() || self.line.is_ok()
    }
}

fn env(name: &str) -> Result<String> {
    std::env::var(name).map_err(|_| anyhow!("environment variable {name} is not set"))
}

fn send_email(to: &[String], subject: &str, body: &str) -> Result<()> {
    if to.is_empty() {
        return Err(anyhow!("notify.email_to is empty"));
    }
    let host = env("OPEN_EASYWEB_SMTP_HOST")?;
    let port: u16 = std::env::var("OPEN_EASYWEB_SMTP_PORT").ok().and_then(|v| v.parse().ok()).unwrap_or(587);
    let creds = Credentials::new(env("OPEN_EASYWEB_SMTP_USERNAME")?, env("OPEN_EASYWEB_SMTP_PASSWORD")?);
    let from: Mailbox = env("OPEN_EASYWEB_SMTP_FROM")?.parse().context("invalid OPEN_EASYWEB_SMTP_FROM")?;
    let mailer = SmtpTransport::starttls_relay(&host)?.port(port).credentials(creds).build();
    for addr in to {
        let msg = Message::builder()
            .from(from.clone())
            .to(addr.parse().with_context(|| "invalid recipient address")?)
            .subject(subject)
            .body(body.to_string())?;
        mailer.send(&msg).context("SMTP send failed")?;
    }
    Ok(())
}

fn read_secret(path: &std::path::Path) -> Result<String> {
    let t = std::fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
    let v = t.lines().next().unwrap_or("").trim().to_string();
    if v.is_empty() {
        return Err(anyhow!("{} is empty", path.display()));
    }
    Ok(v)
}

/// LINE Notify ended on 2025-03-31, so this uses the Messaging API push endpoint
/// (a channel access token + the recipient's user id). At most 5 messages per request.
fn send_line(cfg: &Notify, texts: &[String]) -> Result<()> {
    let token = read_secret(cfg.line_token_file.as_deref().ok_or_else(|| anyhow!("notify.line_token_file is not set"))?)?;
    let to = read_secret(cfg.line_to_file.as_deref().ok_or_else(|| anyhow!("notify.line_to_file is not set"))?)?;
    let messages: Vec<_> = texts.iter().take(5).map(|t| serde_json::json!({"type": "text", "text": t})).collect();
    let resp = reqwest::blocking::Client::new()
        .post("https://api.line.me/v2/bot/message/push")
        .bearer_auth(token)
        .json(&serde_json::json!({"to": to, "messages": messages}))
        .timeout(std::time::Duration::from_secs(30))
        .send()?;
    if !resp.status().is_success() {
        // The body explains the failure (e.g. invalid user id) and holds no secret.
        let status = resp.status();
        let body = resp.text().unwrap_or_default();
        return Err(anyhow!("LINE push failed: HTTP {status} {}", body.chars().take(200).collect::<String>()));
    }
    Ok(())
}

pub fn deliver(cfg: &Notify, subject: &str, email_body: &str, line_texts: &[String]) -> Outcome {
    Outcome {
        email: send_email(&cfg.email_to, subject, email_body).map_err(|e| format!("{e:#}")),
        line: send_line(cfg, line_texts).map_err(|e| format!("{e:#}")),
    }
}

/// Splits blocks into LINE messages of at most `limit` characters (LINE allows 5000).
pub fn pack_for_line(blocks: &[String], limit: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for b in blocks {
        let b: String = b.chars().take(limit).collect();
        match out.last_mut() {
            Some(last) if last.chars().count() + b.chars().count() + 2 <= limit => {
                last.push_str("\n\n");
                last.push_str(&b);
            }
            _ => out.push(b),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packs_within_limit() {
        let blocks: Vec<String> = (0..5).map(|i| format!("{i}{}", "あ".repeat(40))).collect();
        let m = pack_for_line(&blocks, 100);
        assert!(m.iter().all(|s| s.chars().count() <= 100));
        assert_eq!(m.join("\n\n").matches('あ').count(), 200);
    }
}
