//! pt-bridge-matrix — Tuwunel Matrix bot for PalOnTour.
//!
//! Morning digest: new events, deadline alerts, diffs on watched events.
//! Commands: !feed, !shortlist, !applied, !ghost, !why, !verify, !map.
//!
//! Scout and her manager are just Matrix users in a room with the bot.
//! Auth solved by the homeserver — zero-config for the artist.

use chrono::{Datelike, Utc};
use serde::{Deserialize, Serialize};

/// A single item in the morning digest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DigestItem {
    pub rank: u32,
    pub event_name: String,
    pub location: Option<String>,
    pub genres: Vec<String>,
    pub deadline: Option<String>,
    pub days_remaining: Option<i64>,
    pub confidence: String,
    pub url: String,
    pub application_state: String,
    pub explanation: String,
}

/// The morning digest message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MorningDigest {
    pub date: String,
    pub artist_name: String,
    pub header: String,
    pub items: Vec<DigestItem>,
    pub footer: String,
    pub alerts: Vec<AlertItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertItem {
    pub level: AlertLevel,
    pub message: String,
    pub event_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AlertLevel {
    Critical,
    Warning,
    Info,
}

impl MorningDigest {
    /// Build a morning digest from ranked events, tracker state, and alerts.
    pub fn build(
        artist_name: &str,
        ranked: &[crate::RankedDigestInput],
        alerts: Vec<AlertItem>,
    ) -> Self {
        let today = Utc::now();
        let date = today.format("%A, %B %d, %Y").to_string();

        let items: Vec<_> = ranked
            .iter()
            .enumerate()
            .map(|(i, r)| DigestItem {
                rank: (i + 1) as u32,
                event_name: r.event_name.clone(),
                location: r.location.clone(),
                genres: r.genres.clone(),
                deadline: r.deadline.clone(),
                days_remaining: r.days_remaining,
                confidence: r.confidence.clone(),
                url: r.url.clone(),
                application_state: r.application_state.clone(),
                explanation: r.explanation.clone(),
            })
            .collect();

        let item_count = items.len();
        let alert_count = alerts.len();

        MorningDigest {
            date,
            artist_name: artist_name.to_string(),
            header: format!("☀️ Morning Digest — {item_count} opportunities"),
            items,
            footer: if alert_count > 0 {
                format!("⚠️ {alert_count} alerts | !help for commands")
            } else {
                "!help for commands".to_string()
            },
            alerts,
        }
    }

    /// Render the digest as a plain-text Matrix message.
    pub fn to_markdown(&self) -> String {
        let mut msg = format!(
            "## {} — {}\n\n{}\n\n",
            self.artist_name, self.date, self.header
        );

        for item in &self.items {
            let deadline_str = if let Some(ref d) = item.deadline {
                if let Some(days) = item.days_remaining {
                    if days <= 3 {
                        format!("🔴 **{d}** ({days} days!)")
                    } else if days <= 7 {
                        format!("🟠 **{d}** ({days} days)")
                    } else {
                        format!("🟢 {d} ({days} days)")
                    }
                } else {
                    format!("📅 {d}")
                }
            } else {
                "📅 rolling / unknown".to_string()
            };

            let genres = if item.genres.is_empty() {
                String::new()
            } else {
                format!(" | `{}`", item.genres.join("`, `"))
            };

            msg.push_str(&format!(
                "**#{rank} {name}**{genres}\n  {deadline} | {conf} | {state}\n  {url}\n  _{explain}_\n\n",
                rank = item.rank,
                name = item.event_name,
                deadline = deadline_str,
                conf = item.confidence,
                state = item.application_state,
                url = item.url,
                explain = item.explanation,
            ));
        }

        // Alerts section
        if !self.alerts.is_empty() {
            msg.push_str("---\n### ⚠️ Alerts\n\n");
            for alert in &self.alerts {
                let emoji = match alert.level {
                    AlertLevel::Critical => "🔴",
                    AlertLevel::Warning => "🟠",
                    AlertLevel::Info => "ℹ️",
                };
                msg.push_str(&format!("{emoji} **{name}**: {msg}\n", name = alert.event_name, msg = alert.message));
            }
        }

        msg.push_str(&format!("\n---\n{}", self.footer));
        msg
    }
}

/// Input type for building the digest — bridges pt-rank output.
#[derive(Debug, Clone)]
pub struct RankedDigestInput {
    pub event_name: String,
    pub location: Option<String>,
    pub genres: Vec<String>,
    pub deadline: Option<String>,
    pub days_remaining: Option<i64>,
    pub confidence: String,
    pub url: String,
    pub application_state: String,
    pub explanation: String,
}

/// Matrix bot commands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Feed { limit: u32 },
    Shortlist { event_id: String },
    Applied { event_id: String, via: String },
    Ghost { event_id: String },
    Why { event_id: String },
    Verify { event_id: String },
    Map,
    Help,
}

/// Parse a Matrix message into a command.
pub fn parse_command(msg: &str) -> Option<Command> {
    let msg = msg.trim();
    if !msg.starts_with('!') {
        return None;
    }

    let parts: Vec<&str> = msg.splitn(3, ' ').collect();
    match parts[0] {
        "!feed" => {
            let limit = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(10);
            Some(Command::Feed { limit })
        }
        "!shortlist" => parts.get(1).map(|id| Command::Shortlist { event_id: id.to_string() }),
        "!applied" => {
            let id = parts.get(1)?;
            let via = parts.get(2).unwrap_or(&"email");
            Some(Command::Applied { event_id: id.to_string(), via: via.to_string() })
        }
        "!ghost" => parts.get(1).map(|id| Command::Ghost { event_id: id.to_string() }),
        "!why" => parts.get(1).map(|id| Command::Why { event_id: id.to_string() }),
        "!verify" => parts.get(1).map(|id| Command::Verify { event_id: id.to_string() }),
        "!map" => Some(Command::Map),
        "!help" => Some(Command::Help),
        _ => None,
    }
}

/// Build a help message.
pub fn help_text() -> String {
    r#"**PalOnTour Commands**
`!feed [N]` — show top N ranked opportunities
`!shortlist <id>` — move event to shortlist
`!applied <id> [via]` — mark as submitted (default: email)
`!ghost <id>` — mark as ghosted (no response)
`!why <id>` — explain why this event is ranked here
`!verify <id>` — show source links + epistemic status
`!map` — get ODIN C2 GeoJSON export link
`!help` — this message"#
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_feed() {
        assert_eq!(parse_command("!feed"), Some(Command::Feed { limit: 10 }));
        assert_eq!(parse_command("!feed 5"), Some(Command::Feed { limit: 5 }));
    }

    #[test]
    fn test_parse_commands() {
        assert_eq!(
            parse_command("!shortlist abc123"),
            Some(Command::Shortlist { event_id: "abc123".into() })
        );
        assert_eq!(
            parse_command("!applied abc123 via website"),
            Some(Command::Applied { event_id: "abc123".into(), via: "via website".into() })
        );
        assert_eq!(parse_command("!map"), Some(Command::Map));
        assert_eq!(parse_command("!help"), Some(Command::Help));
    }

    #[test]
    fn test_parse_non_command() {
        assert_eq!(parse_command("hello"), None);
        assert_eq!(parse_command(""), None);
    }

    #[test]
    fn test_digest_rendering() {
        let ranked = vec![RankedDigestInput {
            event_name: "Robson Valley Music Festival".into(),
            location: Some("Dunster, BC".into()),
            genres: vec!["folk".into(), "bluegrass".into()],
            deadline: Some("2026-03-01".into()),
            days_remaining: Some(14),
            confidence: "verified".into(),
            url: "https://robsonvalleymusicfest.ca".into(),
            application_state: "discovered".into(),
            explanation: "strong genre match (100%); plenty of time to apply".into(),
        }];

        let alerts = vec![AlertItem {
            level: AlertLevel::Warning,
            message: "Deadline changed from Mar 1 to Mar 15".into(),
            event_name: "Vancouver Folk Fest".into(),
        }];

        let digest = MorningDigest::build("Scout Shorefox", &ranked, alerts);
        let md = digest.to_markdown();
        assert!(md.contains("Scout Shorefox"));
        assert!(md.contains("Robson Valley"));
        assert!(md.contains("14 days"));
        assert!(md.contains("Vancouver Folk Fest"));
    }
}
