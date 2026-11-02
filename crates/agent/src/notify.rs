//! Alerts. They are sent from the server, so they arrive whether or not the desktop app is open.

use crate::restic::anyhow_lite::{Error, Result};
use hmac::{Hmac, Mac};
use orluma_core::model::{Event, Notifier, NotifierKind};
use serde_json::json;

#[derive(Clone, Debug)]
pub struct Message {
    pub event: Event,
    pub server: String,
    pub job_id: Option<String>,
    pub job_name: Option<String>,
    pub title: String,
    pub body: String,
    /// Extra facts as label/value pairs (size, duration, snapshot).
    pub fields: Vec<(String, String)>,
}

impl Message {
    fn level(&self) -> &'static str {
        match self.event {
            Event::RunSucceeded | Event::AgentUpdated => "success",
            Event::RunWarning | Event::DiskLow => "warning",
            Event::RunFailed | Event::VerifyFailed => "failure",
        }
    }
    fn color(&self) -> u32 {
        match self.level() {
            "success" => 0x3DDC97,
            "warning" => 0xFFC94D,
            _ => 0xFF5C5C,
        }
    }
    fn icon(&self) -> &'static str {
        match self.level() {
            "success" => "✅",
            "warning" => "⚠️",
            _ => "❌",
        }
    }
    fn plain(&self) -> String {
        let mut s = format!("{} {} · {}\n{}", self.icon(), self.title, self.server, self.body);
        for (k, v) in &self.fields {
            s.push_str(&format!("\n{k}: {v}"));
        }
        s
    }
}

pub fn wants(n: &Notifier, m: &Message) -> bool {
    if !n.enabled {
        return false;
    }
    let events: &[Event] = if n.events.is_empty() {
        &Event::DEFAULT
    } else {
        &n.events
    };
    let job_ok = n.jobs.is_empty() || m.job_id.as_ref().is_none_or(|j| n.jobs.contains(j));
    events.contains(&m.event) && job_ok
}

pub async fn send(http: &reqwest::Client, n: &Notifier, m: &Message) -> Result<()> {
    let check = |r: reqwest::Response| async move {
        if r.status().is_success() {
            Ok(())
        } else {
            let code = r.status();
            let text = r.text().await.unwrap_or_default();
            Err(Error(format!(
                "HTTP {code}: {}",
                text.chars().take(200).collect::<String>()
            )))
        }
    };
    match &n.kind {
        NotifierKind::Webhook { url, secret } => {
            let body = serde_json::to_vec(&json!({
                "event": m.event, "level": m.level(), "server": m.server, "job_id": m.job_id, "job": m.job_name,
                "title": m.title, "message": m.body,
                "fields": m.fields.iter().map(|(k, v)| json!({"name": k, "value": v})).collect::<Vec<_>>(),
                "at": chrono::Utc::now(),
            }))?;
            let mut req = http
                .post(url)
                .header("Content-Type", "application/json")
                .header("User-Agent", "Orluma");
            if !secret.is_empty() {
                let mut mac =
                    Hmac::<sha2::Sha256>::new_from_slice(secret.as_bytes()).map_err(|e| Error(e.to_string()))?;
                mac.update(&body);
                req = req.header(
                    "X-Orluma-Signature",
                    format!("sha256={}", hex::encode(mac.finalize().into_bytes())),
                );
            }
            check(req.body(body).send().await?).await
        }
        NotifierKind::Discord { webhook_url } => {
            let fields: Vec<_> = m
                .fields
                .iter()
                .map(|(k, v)| json!({"name": k, "value": v, "inline": true}))
                .collect();
            let body = json!({"username": "Orluma", "embeds": [{
                "title": format!("{} {}", m.icon(), m.title), "description": m.body, "color": m.color(),
                "fields": fields, "footer": {"text": m.server}, "timestamp": chrono::Utc::now(),
            }]});
            check(http.post(webhook_url).json(&body).send().await?).await
        }
        NotifierKind::Slack { webhook_url } => {
            let fields = m
                .fields
                .iter()
                .map(|(k, v)| format!("*{k}:* {v}"))
                .collect::<Vec<_>>()
                .join("\n");
            let body = json!({"text": m.plain(), "blocks": [
                {"type": "section", "text": {"type": "mrkdwn", "text": format!("{} *{}* · {}\n{}", m.icon(), m.title, m.server, m.body)}},
                {"type": "section", "text": {"type": "mrkdwn", "text": if fields.is_empty() { " ".into() } else { fields }}},
            ]});
            check(http.post(webhook_url).json(&body).send().await?).await
        }
        NotifierKind::Teams { webhook_url } => {
            let facts: Vec<_> = m.fields.iter().map(|(k, v)| json!({"title": k, "value": v})).collect();
            let body = json!({"type": "message", "attachments": [{"contentType": "application/vnd.microsoft.card.adaptive", "content": {
            "type": "AdaptiveCard", "version": "1.4", "$schema": "http://adaptivecards.io/schemas/adaptive-card.json",
            "body": [
                {"type": "TextBlock", "size": "Medium", "weight": "Bolder", "text": format!("{} {}", m.icon(), m.title), "wrap": true},
                {"type": "TextBlock", "text": format!("{} · {}", m.server, m.body), "wrap": true},
                {"type": "FactSet", "facts": facts},
            ]}}]});
            check(http.post(webhook_url).json(&body).send().await?).await
        }
        NotifierKind::Telegram { bot_token, chat_id } => {
            let url = format!("https://api.telegram.org/bot{bot_token}/sendMessage");
            check(
                http.post(url)
                    .json(&json!({"chat_id": chat_id, "text": m.plain(), "disable_web_page_preview": true}))
                    .send()
                    .await?,
            )
            .await
        }
        NotifierKind::Ntfy { server, topic, token } => {
            let url = format!("{}/{}", server.trim_end_matches('/'), topic);
            let prio = match m.level() {
                "failure" => "high",
                "warning" => "default",
                _ => "low",
            };
            let tag = match m.level() {
                "success" => "white_check_mark",
                "warning" => "warning",
                _ => "x",
            };
            let mut req = http
                .post(url)
                .header("Title", format!("{} · {}", m.title, m.server))
                .header("Priority", prio)
                .header("Tags", tag);
            if !token.is_empty() {
                req = req.bearer_auth(token);
            }
            let mut body = m.body.clone();
            for (k, v) in &m.fields {
                body.push_str(&format!("\n{k}: {v}"));
            }
            check(req.body(body).send().await?).await
        }
        NotifierKind::Gotify { server, token } => {
            let url = format!("{}/message?token={token}", server.trim_end_matches('/'));
            let prio = if m.level() == "failure" { 8 } else { 4 };
            check(http.post(url).json(&json!({"title": format!("{} · {}", m.title, m.server), "message": m.plain(), "priority": prio})).send().await?).await
        }
        NotifierKind::Pushover { user_key, app_token } => {
            let prio = if m.level() == "failure" { "1" } else { "0" };
            let form = [
                ("token", app_token.as_str()),
                ("user", user_key.as_str()),
                ("title", &format!("{} · {}", m.title, m.server)),
                ("message", &m.plain()),
                ("priority", prio),
            ];
            check(
                http.post("https://api.pushover.net/1/messages.json")
                    .form(&form)
                    .send()
                    .await?,
            )
            .await
        }
        NotifierKind::Email {
            smtp_host,
            smtp_port,
            security,
            username,
            password,
            from,
            to,
        } => email(smtp_host, *smtp_port, security, username, password, from, to, m).await,
    }
}

#[allow(clippy::too_many_arguments)]
async fn email(
    host: &str,
    port: u16,
    security: &str,
    user: &str,
    pass: &str,
    from: &str,
    to: &[String],
    m: &Message,
) -> Result<()> {
    use lettre::message::{Mailbox, header::ContentType};
    use lettre::transport::smtp::authentication::Credentials;
    use lettre::{AsyncSmtpTransport, AsyncTransport, Message as Mail, Tokio1Executor};
    let mut b = Mail::builder()
        .from(from.parse::<Mailbox>().map_err(|e| Error(format!("from: {e}")))?)
        .subject(format!("{} {} · {}", m.icon(), m.title, m.server));
    for t in to {
        b = b.to(t.parse::<Mailbox>().map_err(|e| Error(format!("to {t}: {e}")))?);
    }
    let mail = b
        .header(ContentType::TEXT_PLAIN)
        .body(m.plain())
        .map_err(|e| Error(e.to_string()))?;
    let mut tr = match security {
        "tls" => AsyncSmtpTransport::<Tokio1Executor>::relay(host)?,
        "none" => AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(host),
        _ => AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(host)?,
    }
    .port(port);
    if !user.is_empty() {
        tr = tr.credentials(Credentials::new(user.into(), pass.into()));
    }
    tr.build().send(mail).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routing() {
        let n = Notifier {
            id: "n".into(),
            name: "n".into(),
            enabled: true,
            kind: NotifierKind::Discord {
                webhook_url: String::new(),
            },
            events: vec![],
            jobs: vec!["j1".into()],
        };
        let m = |event, job: &str| Message {
            event,
            server: "s".into(),
            job_id: Some(job.into()),
            job_name: None,
            title: String::new(),
            body: String::new(),
            fields: vec![],
        };
        assert!(wants(&n, &m(Event::RunFailed, "j1")));
        assert!(!wants(&n, &m(Event::RunSucceeded, "j1")), "success is opt-in");
        assert!(!wants(&n, &m(Event::RunFailed, "j2")), "another job");
    }
}
