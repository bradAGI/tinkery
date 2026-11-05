//! One explicitly bound Codex observation. Reads no history, writes nothing and claims no
//! current activity or completion; invalid, future or mismatched observations are refused.

use std::{fs::OpenOptions, io::Read, path::PathBuf};

use serde::Deserialize;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

/// A launch-only run/chat/file binding. It discovers no chat or report and reads only on demand.
#[derive(Clone, Debug)]
pub struct CodexReport {
    run: String,
    chat: String,
    path: PathBuf,
}

impl CodexReport {
    /// Parse RUN_UUID:CHAT_ID:PATH, preserving colons in the path and refusing missing identities.
    pub fn parse(value: &str) -> Result<Self, String> {
        let mut parts = value.splitn(3, ':');
        let run = parts.next().unwrap_or("");
        let chat = parts.next().unwrap_or("");
        let path = parts.next().unwrap_or("");
        if !uuid(run) || run.as_bytes()[14] != b'4' || !uuid(chat) || path.is_empty() {
            return Err("--codex-report needs RUN_UUID:CHAT_ID:PATH with canonical UUIDs".into());
        }
        let path = std::env::current_dir()
            .map_err(|e| e.to_string())?
            .join(path);
        Ok(Self {
            run: run.into(),
            chat: chat.into(),
            path,
        })
    }

    pub(super) fn for_run(&self, id: &str) -> Option<Observation> {
        (self.run == id).then(|| Observation {
            chat: self.chat.clone(),
            report: self.read(),
        })
    }

    fn read(&self) -> Result<Report, String> {
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NONBLOCK);
        }
        let file = options
            .open(&self.path)
            .map_err(|_| "could not open the named file")?;
        let metadata = file
            .metadata()
            .map_err(|_| "could not inspect the named file")?;
        if !metadata.is_file() || metadata.len() > 8192 {
            return Err("expected a regular file of at most 8 KiB".into());
        }
        let mut bytes = Vec::new();
        file.take(8193)
            .read_to_end(&mut bytes)
            .map_err(|_| "could not read the named file")?;
        if bytes.len() > 8192 {
            return Err("report exceeds 8 KiB".into());
        }
        let mut raw: RawReport =
            serde_json::from_slice(&bytes).map_err(|_| "invalid report format or status")?;
        if raw.schema_version != 1 || raw.run_id != self.run || raw.thread_id != self.chat {
            return Err("report version, run or chat does not match the binding".into());
        }
        let at = OffsetDateTime::parse(&raw.observed_at, &Rfc3339)
            .map_err(|_| "invalid observation time")?;
        if at > OffsetDateTime::now_utc() {
            return Err("observation time is in the future".into());
        }
        if raw.next_step.trim().is_empty() || raw.next_step.len() > 512 {
            return Err("next step must contain 1 to 512 bytes of text".into());
        }
        raw.next_step = raw
            .next_step
            .chars()
            .flat_map(|c| {
                if c.is_control() || matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
                {
                    c.escape_default().collect::<Vec<_>>()
                } else {
                    vec![c]
                }
            })
            .collect();
        Ok(Report { raw, at })
    }
}

fn uuid(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(i, c)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                c == b'-'
            } else {
                c.is_ascii_digit() || (b'a'..=b'f').contains(&c)
            }
        })
        && matches!(value.as_bytes()[19], b'8'..=b'9' | b'a'..=b'b')
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct RawReport {
    schema_version: u8,
    run_id: String,
    thread_id: String,
    observed_at: String,
    status: Status,
    next_step: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "type", deny_unknown_fields)]
enum Status {
    #[serde(rename = "active")]
    Active {
        #[serde(rename = "activeFlags")]
        flags: Vec<Flag>,
    },
    #[serde(rename = "idle")]
    Idle,
    #[serde(rename = "notLoaded")]
    NotLoaded,
    #[serde(rename = "systemError")]
    SystemError,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
enum Flag {
    #[serde(rename = "waitingOnApproval")]
    Approval,
    #[serde(rename = "waitingOnUserInput")]
    Input,
}

#[derive(Clone, Debug, PartialEq)]
struct Report {
    raw: RawReport,
    at: OffsetDateTime,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Observation {
    chat: String,
    report: Result<Report, String>,
}

impl Observation {
    pub(super) fn needs_you(&self) -> bool {
        match &self.report {
            Ok(report) => match &report.raw.status {
                Status::Idle => false,
                Status::Active { flags } => !flags.is_empty(),
                _ => true,
            },
            Err(_) => true,
        }
    }

    pub(super) fn lines(&self, detail: bool) -> Vec<String> {
        let mut lines = match &self.report {
            Err(why) => vec![format!("Codex report unavailable: {why}")],
            Ok(report) => {
                let words = match &report.raw.status {
                    Status::Active { flags }
                        if flags.contains(&Flag::Approval) && flags.contains(&Flag::Input) =>
                    {
                        "waiting for approval and input"
                    }
                    Status::Active { flags } if flags.contains(&Flag::Approval) => {
                        "waiting for approval"
                    }
                    Status::Active { flags } if flags.contains(&Flag::Input) => "waiting for input",
                    Status::Active { .. } => "working",
                    Status::Idle => "idle (not proof of completion)",
                    Status::NotLoaded => "activity unknown (chat not loaded)",
                    Status::SystemError => "activity unknown (Codex error)",
                };
                let age = (OffsetDateTime::now_utc() - report.at)
                    .whole_minutes()
                    .max(0);
                vec![
                    format!("Codex last reported: {words}"),
                    format!("Observed {} · {age} min ago", report.raw.observed_at),
                    format!("Codex suggests: {}", report.raw.next_step),
                ]
            }
        };
        lines.push(if detail {
            format!("Check Codex chat {} for current status.", self.chat)
        } else {
            "Saved observation; check the Codex chat for current status.".into()
        });
        lines
    }
}
