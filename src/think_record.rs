//! The think record: one plain JSON file per confirmed think session, saying what happened in
//! the order it happened.
//!
//! It is written once, by the same typed `confirm` that saves the goal, and never before: a
//! session that ends without confirming leaves no record. The person's words are kept verbatim
//! and the goal exactly as confirmed. Reading verifies the saved session and goal; it never
//! writes or supplies context to a model. The format is this module's alone; [`SCHEMA`] changes
//! when it does.

mod confirmation;
mod context;
pub use context::{ReadingContext, RecordedReading};
#[cfg(test)]
mod confirmation_tests;
mod read;

pub(crate) use confirmation::Actor;
pub use confirmation::{BlastRadius, ConfirmationClass, Door, GoalConfirmation};

#[cfg(test)]
pub(crate) use read::count_reads;
pub use read::{History, MadeIn, RecordedTurn, Unavailable, read};
pub(crate) use read::{read_for_shape, read_origins};

use crate::goal::Part;
use serde::{Deserialize, Serialize};
use std::{
    ffi::OsStr,
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

/// The record format's version. Renaming or removing a field changes it.
pub const SCHEMA: u8 = 3;

// The reader and writer share one complete-record limit; neither truncates originals.
const MAX_RECORD_BYTES: u64 = 16 * 1024 * 1024;

/// Where records go for a user whose home is `home` (the `HOME` variable). Only a default: the
/// operator may point Tinkery elsewhere with `--record-root`, and nothing here creates the
/// folder. Refuses, saying so, when `home` is unset, empty or relative, rather than writing
/// records under whatever folder Tinkery was started in.
pub fn default_root(home: Option<&OsStr>) -> Result<PathBuf, String> {
    match home.map(Path::new) {
        Some(home) if home.is_absolute() => Ok(home.join(".local/share/tinkery/think")),
        _ => Err(
            "HOME is unset or not an absolute path, so think records have no default folder; pass --record-root PATH"
                .into(),
        ),
    }
}

/// One thing that happened in a think session, and when.
#[derive(Clone, Debug)]
pub struct Turn {
    /// When it happened, by the wall clock.
    pub at: SystemTime,
    /// What happened.
    pub event: Event,
}

/// An offered draft and its separate source attribution, never submitted words or model context.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct DraftSuggestion {
    /// Exact suggestion offered before editing.
    pub(crate) text: String,
    /// Snapshot relationship and deterministic basis, not an acceptance receipt.
    pub(crate) provenance: String,
}

/// What happened in one turn. Words and answers carry the person's text verbatim.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Event {
    /// A deterministic next-dump proposal; separate from any subsequently submitted words.
    DraftSuggested {
        /// Exact proposed draft before operator edits.
        text: String,
        /// Source snapshot and basis, not an operator verdict or model context.
        provenance: String,
    },
    /// A model suggestion shown with a question; never an answer submitted by the operator.
    Suggested {
        /// The question this suggestion belongs to.
        id: String,
        /// The proposed answer as shown.
        answer: String,
        /// The model's stated basis, not verified evidence.
        reason: String,
    },
    /// Words sent that answer no question: a dump or an addition.
    Words {
        /// The original's number in the conversation, from 1.
        source: usize,
        /// The words exactly as sent.
        text: String,
    },
    /// A question came into focus.
    Asked {
        /// The question's id.
        id: String,
        /// The goal part an answer would change, if any.
        part: Option<Part>,
        /// The question as shown.
        question: String,
    },
    /// Words sent as the answer to a question.
    Answered {
        /// The question's id.
        id: String,
        /// The question as shown.
        question: String,
        /// The answer's number in the conversation.
        source: usize,
        /// The answer exactly as sent.
        answer: String,
    },
    /// The question in focus was skipped.
    Skipped {
        /// The question's id.
        id: String,
        /// The goal part it was about, if any.
        part: Option<Part>,
        /// The question as shown.
        question: String,
    },
    /// The last skip was brought back.
    Restored {
        /// The question's id.
        id: String,
        /// The goal part it was about, if any.
        part: Option<Part>,
        /// The question as shown.
        question: String,
    },
    /// Added words came in, and think asked whether they belong to this goal.
    ScopeAsked {
        /// The question's id.
        id: String,
        /// The question as shown.
        question: String,
        /// The added words' number in the conversation.
        source: usize,
    },
    /// The scope of added words was answered.
    ScopeAnswered {
        /// The question's id.
        id: String,
        /// The question as shown.
        question: String,
        /// The answer's number in the conversation.
        source: usize,
        /// The answer exactly as sent.
        answer: String,
    },
}

/// Who produced the readings: a harness with a named model, or the practice simulation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Origin {
    /// `live` for a model host, `simulated` for practice or a stand-in host.
    pub origin: String,
    /// Which program drafted, when there was one.
    pub harness: Option<crate::pi::Harness>,
    /// The model, as the harness was given it, when there was one.
    pub model: Option<String>,
    /// The harness's thinking level, when set.
    pub thinking: Option<String>,
    /// [`hash`] of the system prompts the readings used, when known.
    pub prompt_hash: Option<String>,
}
impl Origin {
    /// No model: practice or a stand-in host.
    pub fn simulated() -> Self {
        Self {
            origin: "simulated".into(),
            harness: None,
            model: None,
            thinking: None,
            prompt_hash: None,
        }
    }
}

/// Everything a record holds, borrowed from the session that was just confirmed.
pub struct Session<'a> {
    /// Who produced the readings.
    pub origin: &'a Origin,
    /// Every turn, in order.
    pub turns: &'a [Turn],
    /// Each check's decision, as it serializes for F5 "why", keeping the model's reply only
    /// where the check rejected.
    pub checks: Vec<serde_json::Value>,
    /// The goal block exactly as confirmed.
    pub goal: &'a str,
    /// The saved session folder the goal was saved in.
    pub seed_me_session: &'a Path,
    /// The repository root Tinkery was started in, or `None` outside one. Where the goal was
    /// made, never what it is about.
    pub repo: Option<&'a Path>,
    /// When the save read back.
    pub confirmed: SystemTime,
}

/// A stable, short fingerprint of `text` (64-bit FNV-1a), so two records can say whether they
/// used the same prompt without carrying it.
pub fn hash(text: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.bytes() {
        h ^= u64::from(byte);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("fnv1a64:{h:016x}")
}

/// Writes one record for `session` into `root` (created if missing) and returns its path. The
/// file is named by confirm time and saved session, written whole, and never replaces another
/// file: a clash is an error, not an overwrite. The session must resolve to an existing
/// directory whose saved goal still matches; its canonical path and existing ledger identity
/// bind this local record to that session. Missing identity is an error, never a generated ID.
pub fn write(root: &Path, session: &Session<'_>) -> Result<PathBuf, String> {
    write_with_confirmation(root, session, &GoalConfirmation::default())
}

/// Writes the same record with explicit caller-declared goal confirmation provenance. Refuses
/// incomplete attribution or an agent outside the two-way/small class before filesystem access.
/// Valid metadata is not authentication or authority, and never confirms a seed or implementation.
/// The optional schema-3 extension leaves old records readable without guessing their confirmer.
pub fn write_with_confirmation(
    root: &Path,
    session: &Session<'_>,
    confirmation: &GoalConfirmation,
) -> Result<PathBuf, String> {
    write_with_context(root, session, confirmation, None).map(|record| record.path)
}

/// A successfully saved Think record, including any nonfatal loss of reading context.
#[derive(Debug)]
pub struct WrittenRecord {
    /// Where the complete record was written.
    pub path: PathBuf,
    /// Why optional reading context was omitted; the confirmed goal and originals still saved.
    pub reading_context_error: Option<String>,
}

impl WrittenRecord {
    /// The save's path and any context-loss reason, for technical details and exit receipts.
    pub fn message(&self) -> String {
        let path = format!("Think record: {}", self.path.display());
        match &self.reading_context_error {
            Some(reason) => format!("{path}\n{}", context::not_saved(reason)),
            None => path,
        }
    }

    /// A short nonfatal notice for the saved-goal screen; the full reason remains in details.
    pub fn notice(&self) -> Option<&'static str> {
        self.reading_context_error
            .as_ref()
            .map(|_| "Reading sequence wasn't saved; F1 then h shows why.")
    }
}

/// Writes confirmed history with optional terminal presentation context. Context never changes
/// originals or model inputs; invalid context is omitted with a saved reason and nonfatal notice.
/// Oversized records refuse publication rather than truncate originals.
pub fn write_with_context(
    root: &Path,
    session: &Session<'_>,
    confirmation: &GoalConfirmation,
    context: Option<&ReadingContext>,
) -> Result<WrittenRecord, String> {
    confirmation.validate()?;
    let reading_context_error = context.and_then(|context| {
        context
            .check(
                session
                    .turns
                    .iter()
                    .map(|turn| (&turn.event, utc(turn.at, ':'))),
            )
            .err()
    });
    let context = context.filter(|_| reading_context_error.is_none());
    #[derive(Serialize)]
    struct TurnOut<'a> {
        at: String,
        #[serde(flatten)]
        event: &'a Event,
    }
    let session_path = session
        .seed_me_session
        .canonicalize()
        .map_err(|e| format!("{}: {e}", session.seed_me_session.display()))?;
    if !session_path.is_dir() {
        return Err(format!(
            "{}: the saved session is not a directory",
            session_path.display()
        ));
    }
    let session_path_text = session_path
        .to_str()
        .ok_or("The saved session path is not valid UTF-8.")?;
    let saved_id = crate::seed_me::session_id(&session_path, session.goal)?;
    let repo = session
        .repo
        .map(|root| {
            root.to_str()
                .filter(|_| root.is_absolute())
                .ok_or("The repository path must be absolute and valid UTF-8.")
        })
        .transpose()?;
    let id = session_id(&session_path);
    let started = session.turns.first().map_or(session.confirmed, |t| t.at);
    let record = serde_json::json!({
        "schema": SCHEMA,
        "tinkery": env!("CARGO_PKG_VERSION"),
        "tinkery_source": context::build_source(
            option_env!("TINKERY_BUILD_COMMIT"), option_env!("TINKERY_BUILD_DIRTY")
        ),
        "reading_context": context,
        "reading_context_error": reading_context_error,
        "origin": session.origin.origin,
        "harness": session.origin.harness,
        "model": session.origin.model,
        "thinking": session.origin.thinking,
        "prompt_hash": session.origin.prompt_hash,
        "started": utc(started, ':'),
        "confirmed": utc(session.confirmed, ':'),
        "seed_me_session": id,
        "seed_me_session_path": session_path_text,
        "seed_me_session_id": saved_id,
        "repo": repo,
        "turns": session.turns.iter().map(|t| TurnOut { at: utc(t.at, ':'), event: &t.event }).collect::<Vec<_>>(),
        "checks": session.checks,
        "goal": session.goal,
        "goal_confirmation": confirmation,
    });
    let mut text = serde_json::to_string_pretty(&record).map_err(|e| e.to_string())?;
    text.push('\n');
    if text.len() as u64 > MAX_RECORD_BYTES {
        return Err("Think record exceeds the 16 MiB history limit; no record was written.".into());
    }
    std::fs::create_dir_all(root).map_err(|e| format!("{}: {e}", root.display()))?;
    let path = root.join(format!("{}_{id}.json", utc(session.confirmed, '-')));
    let mut file =
        tempfile::NamedTempFile::new_in(root).map_err(|e| format!("{}: {e}", root.display()))?;
    file.write_all(text.as_bytes())
        .and_then(|()| file.as_file().sync_all())
        .map_err(|e| format!("{}: {e}", path.display()))?;
    file.persist_noclobber(&path)
        .map_err(|e| format!("{}: {}", path.display(), e.error))?;
    Ok(WrittenRecord {
        path,
        reading_context_error,
    })
}

/// The saved session's folder name, kept to characters that are safe in a file name.
fn session_id(session: &Path) -> String {
    let name = session
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let safe: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if safe.is_empty() {
        "unknown".into()
    } else {
        safe
    }
}

/// `t` in UTC as `2026-10-12T22:40:00.123Z`, with `sep` between hours, minutes and seconds
/// (`-` keeps it safe in a file name).
fn utc(t: SystemTime, sep: char) -> String {
    let d = t.duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = d.as_secs();
    let date = crate::calendar::date((secs / 86_400) as i64);
    let rem = secs % 86_400;
    format!(
        "{date}T{:02}{sep}{:02}{sep}{:02}.{:03}Z",
        rem / 3600,
        rem / 60 % 60,
        rem % 60,
        d.subsec_millis()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn at(secs: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_millis(secs * 1000 + 7)
    }

    #[test]
    fn utc_matches_known_dates() {
        assert_eq!(utc(at(0), ':'), "1970-01-01T00:00:00.007Z");
        assert_eq!(utc(at(951_782_400), ':'), "2000-02-29T00:00:00.007Z");
        assert_eq!(utc(at(1_791_844_800), '-'), "2026-10-12T22-40-00.007Z");
    }

    #[test]
    fn default_root_is_under_an_absolute_home_and_refuses_any_other() {
        assert_eq!(
            default_root(Some(OsStr::new("/TEST/home"))).unwrap(),
            Path::new("/TEST/home/.local/share/tinkery/think")
        );
        for home in [None, Some(""), Some("TEST-relative"), Some(".")] {
            let refusal = default_root(home.map(OsStr::new)).unwrap_err();
            assert!(
                refusal.contains("--record-root PATH"),
                "{home:?}: {refusal}"
            );
        }
    }

    #[test]
    fn hash_is_stable() {
        assert_eq!(hash(""), "fnv1a64:cbf29ce484222325");
        assert_eq!(hash("a"), "fnv1a64:af63dc4c8601ec8c");
    }

    #[test]
    fn writes_one_whole_file_with_originals_verbatim_and_never_overwrites() {
        let root = tempfile::tempdir().unwrap();
        let session_path = root.path().join("TEST-sessions/20261012-abc def");
        std::fs::create_dir_all(&session_path).unwrap();
        std::fs::write(
            session_path.join("ledger.json"),
            serde_json::json!({
                "schema_version": 1, "session_id": "TEST-existing-session-id",
                "goal": "I TEST.\nWhy: because.",
            })
            .to_string(),
        )
        .unwrap();
        let root = root.path().join("TEST-records");
        let words = "  Café 👩‍💻\n\"quoted\"\ttab  ";
        let turns = vec![
            Turn {
                at: at(1_791_844_000),
                event: Event::Words {
                    source: 1,
                    text: words.into(),
                },
            },
            Turn {
                at: at(1_791_844_010),
                event: Event::Asked {
                    id: "q".into(),
                    part: Some(Part::Why),
                    question: "Why?".into(),
                },
            },
        ];
        let origin = Origin::simulated();
        let session = Session {
            origin: &origin,
            turns: &turns,
            checks: vec![serde_json::json!({"check": "TEST"})],
            goal: "I TEST.\nWhy: because.",
            seed_me_session: &session_path,
            repo: None,
            confirmed: at(1_791_844_800),
        };
        let path = write(&root, &session).unwrap();
        assert_eq!(
            path.file_name().unwrap(),
            "2026-10-12T22-40-00.007Z_20261012-abc_def.json"
        );
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
        let record: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(record["schema"], 3);
        assert!(
            record["repo"].is_null(),
            "outside a repository the record says none"
        );
        assert_eq!(
            record["seed_me_session_path"],
            session_path.canonicalize().unwrap().to_str().unwrap()
        );
        assert_eq!(record["origin"], "simulated");
        assert_eq!(record["started"], "2026-10-12T22:26:40.007Z");
        assert_eq!(record["confirmed"], "2026-10-12T22:40:00.007Z");
        assert_eq!(record["seed_me_session"], "20261012-abc_def");
        assert_eq!(record["seed_me_session_id"], "TEST-existing-session-id");
        assert_eq!(record["goal"], "I TEST.\nWhy: because.");
        assert_eq!(record["turns"][0]["kind"], "words");
        assert_eq!(record["turns"][0]["text"], words);
        assert_eq!(record["turns"][1]["kind"], "asked");
        assert_eq!(record["turns"][1]["part"], "why");
        assert_eq!(record["checks"][0]["check"], "TEST");
        let clash = write(&root, &session).unwrap_err();
        assert!(clash.contains("_20261012-abc_def.json"), "{clash}");
        assert_eq!(
            std::fs::read_dir(&root).unwrap().count(),
            1,
            "no temp file left behind"
        );
    }

    #[test]
    fn records_the_repository_it_was_made_in_and_refuses_a_relative_one() {
        let dir = tempfile::tempdir().unwrap();
        let session_path = dir.path().join("TEST-sessions/one");
        std::fs::create_dir_all(&session_path).unwrap();
        std::fs::write(
            session_path.join("ledger.json"),
            serde_json::json!({"schema_version": 1, "session_id": "TEST-id", "goal": "I TEST."})
                .to_string(),
        )
        .unwrap();
        let repo = dir.path().join("TEST-repo");
        let origin = Origin::simulated();
        let session = |repo| Session {
            origin: &origin,
            turns: &[],
            checks: vec![],
            goal: "I TEST.",
            seed_me_session: &session_path,
            repo,
            confirmed: at(1_791_844_800),
        };
        let root = dir.path().join("TEST-records");
        let path = write(&root, &session(Some(&repo))).unwrap();
        let record: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(record["repo"], repo.to_str().unwrap());
        let relative = write(
            &dir.path().join("TEST-other"),
            &session(Some(Path::new("TEST-repo"))),
        );
        assert!(relative.unwrap_err().contains("absolute"));
    }
}
