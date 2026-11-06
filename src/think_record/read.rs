//! Read one saved conversation without changing its files. Attribution requires a complete scan,
//! one record matching a unique live ledger session_id and the unchanged goal. Missing-folder
//! path-only records may use their final folder name as that ID; ambiguous identities refuse.

mod origins;
pub(crate) use origins::read_origins;

use super::{Event, GoalConfirmation, MAX_RECORD_BYTES, SCHEMA, context, session_id};
use crate::seed_me;
use serde::Deserialize;
use std::{
    fs::{self, File},
    io::Read,
    path::{Component, Path, PathBuf},
};

/// One turn as recorded, with the person's words unchanged.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct RecordedTurn {
    /// The timestamp exactly as recorded, without estimating time spent working.
    pub at: String,
    /// The recorded words or question action.
    #[serde(flatten)]
    pub event: Event,
}

impl Event {
    /// The source heading shared by the history sequence and conversation; non-submissions have none.
    pub fn source_label(&self) -> Option<String> {
        match self {
            Self::Words { source, .. } => Some(format!("Original words {source}")),
            Self::Answered { source, .. } | Self::ScopeAnswered { source, .. } => {
                Some(format!("Submission {source}"))
            }
            _ => None,
        }
    }
}

/// A verified connection to one saved goal; reading supplies no model context and writes nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct History {
    /// Every recorded turn, in order and with original words intact.
    pub turns: Vec<RecordedTurn>,
    /// The exact confirmed goal, checked against the current saved session.
    pub goal: String,
    /// Where the goal was made, as recorded.
    pub made_in: MadeIn,
    /// Source, model and check information for technical help, kept apart from the conversation.
    pub details: String,
    /// Reader-owned sequence heading and rows from the same validated context as details, or a missing/invalid notice.
    pub sequence: String,
    /// One saved text projection per presentation, in the same order as the compact sequence.
    pub readings: Vec<super::RecordedReading>,
}

/// The repository a confirmed goal was made in, as its record says. Where it was made, never
/// what it is about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MadeIn {
    /// Started inside the repository rooted here.
    Repo(PathBuf),
    /// Started outside any repository.
    NoRepo,
    /// Recorded before Tinkery kept this (schema 2).
    Unknown,
}

/// Why no history can be attributed safely; this does not invalidate the saved goal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unavailable {
    /// A short explanation for the history view.
    pub message: String,
    /// Source paths and the precise reason, for technical help.
    pub details: String,
}
impl Unavailable {
    fn new(message: &str, details: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            details: details.into(),
        }
    }

    fn unreadable(path: &Path, reason: impl std::fmt::Display) -> Self {
        Self::new(
            "Think history could not be read.",
            format!("{}: {reason}", path.display()),
        )
    }
}

// Limit each file before parsing or formatting its checks. A larger record is unavailable,
// never partially shown. History holds one selected record plus one file; batch attribution
// keeps only paths, identities, goals and origins after each file is validated.

#[derive(Deserialize)]
struct SavedRecord {
    schema: u8,
    tinkery: String,
    #[serde(default)]
    tinkery_source: serde_json::Value,
    #[serde(default)]
    reading_context: serde_json::Value,
    #[serde(default)]
    reading_context_error: serde_json::Value,
    origin: String,
    #[serde(default)]
    harness: Option<String>,
    model: Option<String>,
    thinking: Option<String>,
    prompt_hash: Option<String>,
    started: String,
    confirmed: String,
    seed_me_session: String,
    seed_me_session_path: Option<PathBuf>,
    seed_me_session_id: Option<String>,
    #[serde(default)]
    repo: Option<PathBuf>,
    turns: Vec<RecordedTurn>,
    checks: Vec<serde_json::Value>,
    goal: String,
    #[serde(default)]
    goal_confirmation: GoalConfirmation,
}

/// Reads the unique history for `session` and its exact `goal`, without writing or running
/// commands. Scans JSON files under `root`; missing, unreadable, unsupported, conflicting or
/// older matching records refuse attribution. A record over 16 MiB is unreadable. Ledger IDs
/// must be unique among readable live sessions before and after the read. A path-only record may use
/// its final folder name as the ID only when that folder is missing. Nothing is rewritten.
pub fn read(root: &Path, session: &Path, goal: &str) -> Result<History, Unavailable> {
    let VerifiedRecord {
        selected,
        path,
        record,
    } = verified_record(root, session, goal)?;
    let made_in = origin(record.schema, record.repo);
    let context = context::history(
        &record.reading_context,
        record
            .turns
            .iter()
            .map(|turn| (&turn.event, turn.at.clone())),
        &record.reading_context_error,
    );
    let source_details = context::source_details(&record.tinkery_source);
    let details = format!(
        concat!(
            "Think record: {}\nSession: {}\nSession ID: {}\nRepository: {}\nFormat: {}\nTinkery: {}\n",
            "Origin: {}\nHarness: {}\nModel: {}\nThinking: {}\nPrompt: {}\nStarted: {}\nConfirmed: {}\n",
            "Goal confirmer (caller-declared, not authenticated): {}\n\nChecks:\n{}"
        ),
        path.display(),
        selected.0.display(),
        selected.1,
        match &made_in {
            MadeIn::Repo(root) => root.display().to_string(),
            MadeIn::NoRepo => "none".into(),
            MadeIn::Unknown => "not recorded".into(),
        },
        record.schema,
        record.tinkery,
        record.origin,
        record.harness.as_deref().unwrap_or("not recorded"),
        record.model.as_deref().unwrap_or("not recorded"),
        record.thinking.as_deref().unwrap_or("not recorded"),
        record.prompt_hash.as_deref().unwrap_or("not recorded"),
        record.started,
        record.confirmed,
        record.goal_confirmation.details(),
        serde_json::to_string_pretty(&record.checks).expect("JSON values always serialize"),
    );
    let details = format!("{details}\n\n{source_details}\n\n{}", context.details);
    Ok(History {
        turns: record.turns,
        goal: record.goal,
        made_in,
        details,
        sequence: context.sequence,
        readings: context.readings,
    })
}

/// A submitted question and answer, with the recorded words unchanged.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AnsweredQuestion {
    /// The question the person answered.
    pub question: String,
    /// Their exact submitted answer.
    pub answer: String,
}

/// Only the verified origin and submissions Shape needs; no history presentation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ShapeContext {
    /// Where this exact saved goal was made.
    pub made_in: MadeIn,
    /// Answered questions in recorded order, without suggestions or skipped questions.
    pub answered: Vec<AnsweredQuestion>,
}

/// Uses the same attribution and refusal contract as [`read`]; Shape owns its prompt format.
pub(crate) fn read_for_shape(
    root: &Path,
    session: &Path,
    goal: &str,
) -> Result<ShapeContext, Unavailable> {
    let record = verified_record(root, session, goal)?.record;
    Ok(ShapeContext {
        made_in: origin(record.schema, record.repo),
        answered: record
            .turns
            .into_iter()
            .filter_map(|turn| match turn.event {
                Event::Answered {
                    question, answer, ..
                }
                | Event::ScopeAnswered {
                    question, answer, ..
                } => Some(AnsweredQuestion { question, answer }),
                _ => None,
            })
            .collect(),
    })
}

struct VerifiedRecord {
    selected: (PathBuf, String),
    path: PathBuf,
    record: SavedRecord,
}

fn verified_record(root: &Path, session: &Path, goal: &str) -> Result<VerifiedRecord, Unavailable> {
    let selected = current_session(session, goal)?;
    require_unique(&identity_index([session])?, &selected)?;
    let mut found = None;
    for entry in record_files(root)? {
        let (path, record) = entry?;
        if record.schema == 1 {
            if record.seed_me_session == session_id(&selected.0) {
                return Err(Unavailable::new(
                    "This older Think history cannot be matched safely.",
                    format!(
                        "{}: schema 1 stores only the session folder name; its root is unknown. No file was changed.",
                        path.display()
                    ),
                ));
            }
            continue;
        }
        let identity = record_identity(&path, &record)?;
        if identity != selected.1 {
            continue;
        }
        if found.is_some() {
            return Err(Unavailable::new(
                "More than one Think record matches this saved session.",
                format!(
                    "{}: another matching record is {}. No record was chosen.",
                    root.display(),
                    path.display()
                ),
            ));
        }
        found = Some((path, record));
    }
    let (path, record) = found.ok_or_else(|| missing(root))?;
    if record.goal != goal {
        return Err(Unavailable::new(
            "Think history disagrees with the saved goal.",
            format!(
                "{}: the recorded goal does not exactly match the saved goal.",
                path.display()
            ),
        ));
    }
    if current_session(session, goal)? != selected {
        return Err(Unavailable::new(
            "The saved session changed while history was read.",
            "Open the saved goal again before reading its history.",
        ));
    }
    require_unique(&identity_index([session])?, &selected)?;
    Ok(VerifiedRecord {
        selected,
        path,
        record,
    })
}

type RecordEntry = Result<(PathBuf, SavedRecord), Unavailable>;

/// Stream decoded JSON files in directory order. Surface entry and record errors to both readers.
fn record_files(root: &Path) -> Result<impl Iterator<Item = RecordEntry>, Unavailable> {
    #[cfg(test)]
    counts::scan(root);
    let entries = fs::read_dir(root).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            missing(root)
        } else {
            Unavailable::unreadable(root, error)
        }
    })?;
    Ok(entries.filter_map(move |entry| {
        let path = match entry {
            Ok(entry) => entry.path(),
            Err(error) => return Some(Err(Unavailable::unreadable(root, error))),
        };
        (path.extension()? == "json").then(|| read_file(&path).map(|record| (path, record)))
    }))
}

fn record_identity(path: &Path, record: &SavedRecord) -> Result<String, Unavailable> {
    let stored = record.seed_me_session_path.as_ref();
    if let Some(stored) = stored {
        if !stored.is_absolute()
            || stored
                .components()
                .any(|part| matches!(part, Component::CurDir | Component::ParentDir))
            || record.seed_me_session != session_id(stored)
        {
            return Err(Unavailable::unreadable(
                path,
                "the saved session reference is invalid",
            ));
        }
        if let Ok(canonical) = stored.canonicalize()
            && canonical != *stored
        {
            return Err(Unavailable::unreadable(
                path,
                "the saved session path is no longer canonical",
            ));
        }
    }
    if let Some(id) = &record.seed_me_session_id {
        return Ok(id.clone());
    }
    let stored = stored.ok_or_else(|| {
        Unavailable::unreadable(
            path,
            "the recorded ledger identity and session path are missing",
        )
    })?;
    match fs::symlink_metadata(stored) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let id = stored
                .file_name()
                .and_then(|name| name.to_str())
                .filter(|id| !id.is_empty() && !id.chars().any(char::is_control))
                .ok_or_else(|| {
                    Unavailable::unreadable(
                        path,
                        "the missing session folder has no usable identity",
                    )
                })?;
            Ok(id.to_owned())
        }
        Ok(_) => Err(Unavailable::unreadable(
            path,
            "a path-only history needs a missing session folder before its final name can identify a moved session",
        )),
        Err(_) => Err(Unavailable::unreadable(
            path,
            "the recorded session location could not be inspected",
        )),
    }
}

type Identities = std::collections::HashMap<String, Option<PathBuf>>;

// Scan readable live identities in each current root, never an old recorded location.
// Discovery discloses unreadable siblings separately; they supply no matching identity.
fn identity_index<'a>(
    sessions: impl IntoIterator<Item = &'a Path>,
) -> Result<Identities, Unavailable> {
    let roots: std::collections::BTreeSet<_> =
        sessions.into_iter().filter_map(Path::parent).collect();
    let mut identities = Identities::new();
    for root in roots {
        let entries = fs::read_dir(root).map_err(|_| {
            Unavailable::new(
                "Saved session identities could not be checked.",
                "Reopen Tend after checking the sessions folder.",
            )
        })?;
        for entry in entries {
            let entry = entry.map_err(|_| {
                Unavailable::new(
                    "Saved session identities could not be checked.",
                    "The sessions folder could not be listed completely.",
                )
            })?;
            match seed_me::read(&entry.path()) {
                seed_me::Record::NotASession => {}
                seed_me::Record::Unreadable(_) => continue,
                seed_me::Record::Session(session) => {
                    let Some(id) = session.id else { continue };
                    identities
                        .entry(id)
                        .and_modify(|found| {
                            if found.as_ref() != Some(&session.path) {
                                *found = None;
                            }
                        })
                        .or_insert(Some(session.path));
                }
            }
        }
    }
    Ok(identities)
}

fn require_unique(
    identities: &Identities,
    selected: &(PathBuf, String),
) -> Result<(), Unavailable> {
    match identities.get(&selected.1) {
        Some(Some(path)) if *path == selected.0 => Ok(()),
        Some(None) => Err(Unavailable::new(
            "More than one saved session has this identity.",
            "Think history is ambiguous; no session was chosen. Check the sessions folder before reopening.",
        )),
        _ => Err(Unavailable::new(
            "The saved session changed while history was read.",
            "Reopen the saved goal before reading its history.",
        )),
    }
}

fn origin(schema: u8, repo: Option<PathBuf>) -> MadeIn {
    match (schema, repo) {
        (2, _) => MadeIn::Unknown,
        (_, Some(root)) => MadeIn::Repo(root),
        (_, None) => MadeIn::NoRepo,
    }
}

fn missing(root: &Path) -> Unavailable {
    Unavailable::new(
        "No Think history was found for this saved goal.",
        format!(
            "{}: no record identifies this saved session. A saved goal may have no Think record.",
            root.display()
        ),
    )
}

fn current_session(session: &Path, goal: &str) -> Result<(PathBuf, String), Unavailable> {
    match seed_me::read(session) {
        seed_me::Record::Session(current) if current.goal == goal => {
            let id = seed_me::session_id(&current.path, goal).map_err(|reason| {
                Unavailable::new("The saved session identity could not be checked.", reason)
            })?;
            Ok((current.path, id))
        }
        seed_me::Record::Session(_) => Err(Unavailable::new(
            "The saved goal changed. Open it again.",
            format!(
                "{}: the current saved goal differs from the selected goal.",
                session.display()
            ),
        )),
        _ => Err(Unavailable::new(
            "The saved session could not be read.",
            format!(
                "{}: Tinkery could not read the current session.",
                session.display()
            ),
        )),
    }
}

fn read_file(path: &Path) -> Result<SavedRecord, Unavailable> {
    #[cfg(test)]
    counts::file(path);
    let metadata =
        fs::symlink_metadata(path).map_err(|error| Unavailable::unreadable(path, error))?;
    if !metadata.is_file() {
        return Err(Unavailable::unreadable(
            path,
            "expected a regular JSON file",
        ));
    }
    let mut bytes = Vec::new();
    File::open(path)
        .and_then(|file| file.take(MAX_RECORD_BYTES + 1).read_to_end(&mut bytes))
        .map_err(|error| Unavailable::unreadable(path, error))?;
    if bytes.len() as u64 > MAX_RECORD_BYTES {
        return Err(Unavailable::unreadable(
            path,
            "the record exceeds the 16 MiB reading limit",
        ));
    }
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|error| Unavailable::unreadable(path, error))?;
    match value.get("schema").and_then(serde_json::Value::as_u64) {
        Some(1 | 2) => {}
        Some(schema) if schema == u64::from(SCHEMA) => match value.get("repo") {
            Some(serde_json::Value::Null) => {}
            Some(serde_json::Value::String(root)) if Path::new(root).is_absolute() => {}
            _ => {
                return Err(Unavailable::unreadable(
                    path,
                    "the recorded repository is missing or not an absolute path",
                ));
            }
        },
        Some(schema) => {
            return Err(Unavailable::new(
                "Think history uses an unsupported format.",
                format!(
                    "{}: schema {schema} is unsupported; it could contain a matching record.",
                    path.display()
                ),
            ));
        }
        None => {
            return Err(Unavailable::unreadable(
                path,
                "the record schema is missing or invalid",
            ));
        }
    }
    let record: SavedRecord =
        serde_json::from_value(value).map_err(|error| Unavailable::unreadable(path, error))?;
    record
        .goal_confirmation
        .validate()
        .map_err(|error| Unavailable::unreadable(path, error))?;
    if record
        .seed_me_session_id
        .as_ref()
        .is_some_and(|id| id.trim().is_empty() || id.chars().any(char::is_control))
    {
        return Err(Unavailable::unreadable(
            path,
            "the recorded ledger identity is invalid",
        ));
    }
    if record.started.is_empty()
        || record.confirmed.is_empty()
        || record.turns.iter().any(|turn| turn.at.is_empty())
    {
        return Err(Unavailable::unreadable(path, "a recorded time is empty"));
    }
    Ok(record)
}

#[cfg(test)]
mod counts;
#[cfg(test)]
mod tests;
#[cfg(test)]
pub(crate) use counts::count_reads;
