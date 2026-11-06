//! Session publication: one cooperating writer, atomic private ledgers and read-back save outcomes.
//! The browser snapshot is not part of a ledger publication.

use super::{Value, load, transition};
use std::{
    fs::{File, OpenOptions},
    io::{self, Write},
    os::fd::AsRawFd,
    path::Path,
};

/// Create a private session with a unique folder identity and an empty validated ledger.
/// Never replaces a session; no browser, provider or external helper is involved.
pub fn initialize(root: &Path, timestamp: &str) -> Result<std::path::PathBuf, String> {
    initialize_for(root, timestamp, crate::think_record::Actor::Operator)
}

/// Create a session for the same authorized actor that will publish its goal and answers.
/// Refuse retired storage before creating any folder or ledger.
pub(crate) fn initialize_for(
    root: &Path,
    timestamp: &str,
    actor: crate::think_record::Actor<'_>,
) -> Result<std::path::PathBuf, String> {
    crate::seed_me::migration::refuse_old_root(root)?;
    std::fs::create_dir_all(root)
        .map_err(|_| "Cannot create the sessions folder. Check its permissions.")?;
    let directory = tempfile::Builder::new()
        .prefix("session-")
        .tempdir_in(root)
        .map_err(|_| "Cannot create a private session. Check the sessions folder.")?;
    let path = directory
        .path()
        .canonicalize()
        .map_err(|_| "Cannot locate the new session.")?;
    let id = path
        .file_name()
        .and_then(|v| v.to_str())
        .ok_or("The session has no usable identity.")?;
    let value = Value::from(serde_json::json!({
        "schema_version":1,"session_id":id,"version":0,"revision":0,
        "created_at":timestamp,"updated_at":timestamp,"status":"active",
        "draft":{"goal":"","outcome":"","options":[]},"goal":null,"origin":null,
        "current_question":null,"frontier":[],"nodes":[],"assumed":[],"operator":actor.session_operator()
    }));
    atomic_write(&path, &value.pretty()?)
        .map_err(|_| "Cannot save the new session. Check its folder permissions.")?;
    let _ = directory.keep();
    Ok(path)
}

/// What a refused publication established about the durable ledger.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveState {
    /// Checks refused or read-back found the unchanged ledger.
    NotSaved,
    /// Read-back could not establish either the old or proposed ledger.
    Unknown,
}

/// A publication refusal, including whether retrying requires reconciliation first.
#[derive(Debug)]
pub struct PublishError {
    /// The helper-compatible first reason line, except platform IO diagnostics.
    pub reason: String,
    /// The established durable outcome; an unreadable ledger never counts as saved.
    pub save_state: SaveState,
}

impl std::fmt::Display for PublishError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.reason)
    }
}
impl std::error::Error for PublishError {}
impl From<String> for PublishError {
    fn from(reason: String) -> Self {
        Self {
            reason,
            save_state: SaveState::NotSaved,
        }
    }
}

struct Writer(File);
impl Writer {
    fn acquire(directory: &Path) -> Result<Self, PublishError> {
        let file = OpenOptions::new()
            .append(true)
            .create(true)
            .open(directory.join(".writer.lock"))
            .map_err(|e| e.to_string())?;
        // SAFETY: the descriptor remains owned by `file` through this call.
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            let error = io::Error::last_os_error();
            return Err(if error.kind() == io::ErrorKind::WouldBlock {
                "session already has a writer".to_owned()
            } else {
                error.to_string()
            }
            .into());
        }
        Ok(Self(file))
    }
}
impl Drop for Writer {
    fn drop(&mut self) {
        // SAFETY: `self` still owns this descriptor. Closing it also releases the lock.
        unsafe {
            libc::flock(self.0.as_raw_fd(), libc::LOCK_UN);
        }
    }
}

/// Publish from a displayed version with the supplied UTC timestamp, refusing completion here.
/// Hold the shared writer lock through validation and read-back; never refresh the browser view.
pub fn publish(
    directory: &Path,
    state: &Value,
    expected_version: &Value,
    reason: &Value,
    revalidated: Option<&Value>,
    timestamp: &str,
) -> Result<Value, PublishError> {
    let _writer = Writer::acquire(directory)?;
    let ledger = load(directory)?;
    publication(
        directory,
        &ledger,
        state,
        expected_version,
        reason,
        revalidated,
        timestamp,
    )
}

/// Revalidate one node with the operator's reason, leaving its answer unchanged. Unlike an
/// ordinary matching publication, a stale review is refused even when the editable state matches.
/// Holds the writer lock while checking the displayed version and publishing the review.
pub fn revalidate(
    directory: &Path,
    expected: u64,
    id: &str,
    reason: &str,
    timestamp: &str,
    actor: Option<&crate::think_record::GoalConfirmation>,
) -> Result<(), PublishError> {
    let actor = crate::think_record::Actor::authorize(actor)?;
    let _writer = Writer::acquire(directory)?;
    let ledger = load(directory)?;
    let expected = super::value::int(expected);
    super::require(ledger["version"] == expected, "stale publication version")?;
    let state = super::editable(&ledger)?;
    super::validate_revalidation(&super::value::object(&[(id, reason.into())]))?;
    let review_reason = actor.context().map_or_else(
        || reason.to_owned(),
        |_| actor.reason(&format!("kept this answer with reason:\n{reason}")),
    );
    let revalidated = super::value::object(&[(id, review_reason.as_str().into())]);
    publication(
        directory,
        &ledger,
        &state,
        &expected,
        &actor
            .reason("kept a flagged answer with a reason.")
            .as_str()
            .into(),
        Some(&revalidated),
        timestamp,
    )
    .map(|_| ())
}

/// Publish an explicit operator answer action from the displayed version. The ledger alone
/// owns eligibility, required fields and reopening; stale versions never mutate anything.
pub fn respond(
    directory: &Path,
    expected: u64,
    id: &str,
    action: &super::AnswerAction,
    timestamp: &str,
    actor: Option<&crate::think_record::GoalConfirmation>,
) -> Result<Value, PublishError> {
    let actor = crate::think_record::Actor::authorize(actor)?;
    let _writer = Writer::acquire(directory).map_err(|error| {
        if error.reason == "session already has a writer" {
            error
        } else {
            "The session cannot be locked for saving. Check its folder permissions, then try again."
                .to_owned()
                .into()
        }
    })?;
    let ledger = load(directory).map_err(|_| {
        "The session could not be read. Reopen it and check its saved ledger before trying again.".to_owned()
    })?;
    let expected = super::value::int(expected);
    super::require(ledger["version"] == expected, "stale publication version")?;
    if matches!(action, super::AnswerAction::Revisit(_)) {
        super::require(
            !directory.join(super::seed::FILE).exists(),
            "A seed is already recorded. Reopen Shape before answering a deferred prerequisite.",
        )?;
    }
    let (mut state, mut reviewed) = super::answers::change(&ledger, id, action)?;
    if let Some(context) = actor.context()
        && let Some(node) = state["nodes"]
            .list_mut()
            .iter_mut()
            .find(|n| n["id"].is(id))
        && node["status"].is("settled")
    {
        node.set("authority", "delegated".into());
        node.set(
            "authority_source",
            actor
                .reason("recorded this answer action in Tinkery Shape.")
                .as_str()
                .into(),
        );
        node["evidence"].list_mut().push(super::value::object(&[
            (
                "checked",
                "Caller-declared authorization, not authenticated identity".into(),
            ),
            ("at", timestamp.into()),
            ("observed", context.details().as_str().into()),
        ]));
    }
    let verb = match action {
        super::AnswerAction::Defer { .. } => "deferred",
        super::AnswerAction::Delegate => "delegated",
        super::AnswerAction::Revise(_) => "revised",
        super::AnswerAction::Revisit(_) => "answered deferred prerequisite",
    };
    let reason: Value = actor
        .reason(&format!("{verb} {id} in Tinkery Shape."))
        .as_str()
        .into();
    if reviewed.get(id).is_some() {
        // A changed answer can itself retire a direct contradiction flag. Only revalidate
        // flags the ordinary transition still holds, using that same rule owner.
        let proposed = transition(&ledger, &state, &expected, &reason, None, timestamp)?;
        if super::review_flags(&proposed["nodes"]).get(id).is_none() {
            reviewed = super::value::object(&[]);
        }
    }
    publication(directory, &ledger, &state, &expected, &reason, Some(&reviewed), timestamp)
    .map_err(|mut error| {
        if error.reason.starts_with("could not save the ledger:") {
            error.reason = "The ledger save could not be verified. Check the session folder before trying again.".into();
        }
        error
    })
}

fn publication(
    directory: &Path,
    ledger: &Value,
    state: &Value,
    expected_version: &Value,
    reason: &Value,
    revalidated: Option<&Value>,
    timestamp: &str,
) -> Result<Value, PublishError> {
    let result = transition(
        ledger,
        state,
        expected_version,
        reason,
        revalidated,
        timestamp,
    )?;
    if result != *ledger && result["status"].string() == Some("completed") {
        return Err("publish cannot complete a session: use `end --status completed`, which checks the confirmed seed".to_owned().into());
    }
    // JSON refusals precede the IO attempt, as in the helper's atomic_write.
    let bytes = if result != *ledger {
        result.pretty()?
    } else {
        String::new()
    };
    commit(
        directory,
        ledger,
        result,
        |directory, _| atomic_write(directory, &bytes),
        load,
    )
}

fn atomic_write(directory: &Path, bytes: &str) -> io::Result<()> {
    atomic_text(&directory.join("ledger.json"), bytes, None)
}

fn atomic_text(path: &Path, bytes: &str, mode: Option<std::fs::Permissions>) -> io::Result<()> {
    let mut temporary = tempfile::Builder::new()
        .prefix(".ledger-")
        .suffix(".tmp")
        .tempfile_in(path.parent().expect("session file has a parent"))?;
    temporary.write_all(bytes.as_bytes())?;
    temporary.as_file().sync_all()?;
    if let Some(mode) = mode {
        temporary.as_file().set_permissions(mode)?;
    }
    temporary.persist(path).map_err(|e| e.error)?;
    Ok(())
}

fn commit(
    directory: &Path,
    ledger: &Value,
    result: Value,
    write: impl FnOnce(&Path, &Value) -> io::Result<()>,
    read: impl FnOnce(&Path) -> Result<Value, String>,
) -> Result<Value, PublishError> {
    if result == *ledger {
        return Ok(result);
    }
    let written = write(directory, &result);
    let current = read(directory);
    if current.as_ref().is_ok_and(|v| *v == result) {
        return Ok(result);
    }
    let unchanged = current.as_ref().is_ok_and(|v| v == ledger);
    let cause = match written {
        Err(error) => error.to_string(),
        Ok(()) => current
            .err()
            .unwrap_or_else(|| "ledger read-back did not match the publication".into()),
    };
    Err(PublishError {
        reason: format!("could not save the ledger: {cause}"),
        save_state: if unchanged {
            SaveState::NotSaved
        } else {
            SaveState::Unknown
        },
    })
}

/// Save an unconfirmed host draft and its records under one writer lock, rechecking the displayed
/// ledger and prior seed. IO failures can leave records saved, but never grant confirmation.
pub(crate) fn save_draft(
    directory: &Path,
    expected: u64,
    previous: Option<&str>,
    records: &Value,
    seed: &str,
) -> Result<(), String> {
    super::checks::validate_checks(records)?;
    super::require(
        seed.lines().next() == Some(super::seed::DRAFT),
        "a seed draft must start with status: draft",
    )?;
    let _writer = Writer::acquire(directory).map_err(|e| e.reason)?;
    let ledger = load(directory)?;
    super::require(
        ledger["version"] == super::value::int(expected),
        "stale publication version",
    )?;
    super::require(ledger["status"].is("active"), "ended session is read-only")?;
    let path = directory.join(super::seed::FILE);
    let current = match std::fs::read_to_string(&path) {
        Ok(text) => Some(text),
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(e) => return Err(e.to_string()),
    };
    super::require(
        current
            .as_ref()
            .is_none_or(|s| s.lines().next() == Some(super::seed::DRAFT)),
        "a seed that is not a draft already exists; never replace it",
    )?;
    super::require(
        current.as_deref() == previous,
        "the seed changed while the draft was prepared; reopen this session before trying again",
    )?;
    atomic_text(
        &directory.join(super::checks::FILE),
        &records.pretty()?,
        None,
    )
    .map_err(|e| format!("could not save check records: {e}"))?;
    atomic_text(&path, seed, None)
        .map_err(|e| format!("check records saved, but the seed draft was not saved: {e}"))
}

/// Record a drafting review or teach-back against the displayed ledger and exact draft bytes.
/// A stale draft refuses without saving; model text never becomes a confirmation gate.
pub fn record_teach_back(
    directory: &Path,
    expected: u64,
    revision: &str,
    seed: &str,
    record: &super::TeachBackRecord<'_>,
    timestamp: &str,
    actor: Option<&crate::think_record::GoalConfirmation>,
) -> Result<u64, PublishError> {
    let actor = crate::think_record::Actor::authorize(actor)?;
    let _writer = Writer::acquire(directory).map_err(|_| {
        "The session cannot be locked. Reopen it before recording this review again.".to_owned()
    })?;
    let ledger = load(directory).map_err(|_| {
        "The session could not be read. Reopen it before recording this review again.".to_owned()
    })?;
    super::require(
        ledger["version"] == super::value::int(expected),
        "stale publication version",
    )?;
    let current = std::fs::read_to_string(directory.join(super::seed::FILE)).map_err(|_| {
        "The draft could not be read. Reopen Shape before recording this review again.".to_owned()
    })?;
    super::require(
        current == seed,
        "The draft changed. Reopen Shape before recording this review again.",
    )?;
    let state = super::teach_back::append(&ledger, revision, seed, record, timestamp)?;
    let saved = publication(
        directory,
        &ledger,
        &state,
        &ledger["version"],
        &actor
            .reason("requested a draft review or teach-back in Tinkery Shape.")
            .as_str()
            .into(),
        None,
        timestamp,
    )
    .map_err(|mut error| {
        if error.reason.starts_with("could not save the ledger:") {
            error.reason =
                "The review save could not be verified. Reopen the session before trying again."
                    .into();
        }
        error
    })?;
    saved["version"]
        .pretty()
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .ok_or_else(|| PublishError {
            reason: "The review may have saved. Reopen the session before trying again.".into(),
            save_state: SaveState::Unknown,
        })
}

/// Store strict records while holding the session writer lock; never execute a check.
pub fn set_checks(directory: &Path, records: &Value) -> Result<Value, String> {
    super::checks::validate_checks(records)?;
    let _writer = Writer::acquire(directory).map_err(|e| e.reason)?;
    super::require(
        load(directory)?["status"].is("active"),
        "ended session is read-only",
    )?;
    let path = directory.join(super::checks::FILE);
    atomic_text(&path, &records.pretty()?, None).map_err(|e| e.to_string())?;
    Ok(super::value::object(&[
        ("checks", path.to_string_lossy().as_ref().into()),
        ("count", super::value::int(records.items().len() as u64)),
        ("sha256", super::checks::digest(records)?.as_str().into()),
    ]))
}

/// Repair the first commandless provisional in the exact displayed draft, without a model.
/// Preserve the statement and typed words; stale inputs and non-drafts refuse before writes.
pub fn repair_check(
    directory: &Path,
    expected: u64,
    seed: &str,
    words: &str,
    judgment: bool,
    actor: Option<&crate::think_record::GoalConfirmation>,
) -> Result<(), String> {
    crate::think_record::Actor::authorize(actor)?;
    super::require(
        !words.trim().is_empty() && !words.contains('\0'),
        "Type the check command or how-to-judge words first.",
    )?;
    let _writer = Writer::acquire(directory).map_err(|e| e.reason)?;
    let ledger = load(directory)?;
    super::require(
        ledger["version"] == super::value::int(expected),
        "stale publication version",
    )?;
    super::require(ledger["status"].is("active"), "ended session is read-only")?;
    let path = directory.join(super::seed::FILE);
    super::require(
        std::fs::read_to_string(&path).map_err(|e| e.to_string())? == seed,
        "The draft changed. Read it again before changing a check.",
    )?;
    super::require(
        seed.lines().next() == Some(super::seed::DRAFT),
        "Only a draft check can change.",
    )?;
    let records = super::checks::require_seed_checks(directory, &ledger, false)?;
    let mut items = records.items().to_vec();
    let record = items
        .iter_mut()
        .find(|r| r["kind"].is("provisional") && !r.has("cmd"))
        .ok_or("No provisional check needs a command or judgment.")?;
    if judgment {
        *record = super::value::object(&[
            ("statement", record["statement"].clone()),
            ("kind", "judge".into()),
            ("source", record["source"].clone()),
            ("how_to_judge", words.into()),
        ]);
    } else {
        record.set("cmd", words.into());
    }
    let repaired = Value::Array(items);
    super::checks::validate_checks(&repaired)?;
    let old = super::checks::checks_block(&records)?;
    let new = super::checks::checks_block(&repaired)?;
    super::require(
        seed.matches(&old).count() == 1,
        "The draft acceptance section cannot be replaced safely.",
    )?;
    let draft = seed.replacen(&old, &new, 1);
    atomic_text(
        &directory.join(super::checks::FILE),
        &repaired.pretty()?,
        None,
    )
    .map_err(|e| e.to_string())?;
    atomic_text(&path, &draft, None)
        .map_err(|e| format!("Check records saved, but the draft was not saved: {e}"))
}

/// Confirm only a displayed draft revision, after all refusals, under the shared writer lock.
/// The receipt is written first, so an interrupted seed replacement cannot grant handoff.
pub fn confirm_seed(
    directory: &Path,
    revision: &str,
    source: &str,
    timestamp: &str,
) -> Result<Value, String> {
    confirm(directory, revision, source, timestamp, false, None, None)
}

/// Confirm after recording a missing teach-back as skipped. A completed comparison, including
/// mismatches, never blocks confirmation; all ordinary seed refusals still apply first.
pub fn confirm_with_teach_back(
    directory: &Path,
    revision: &str,
    source: &str,
    timestamp: &str,
) -> Result<Value, String> {
    confirm(directory, revision, source, timestamp, true, None, None)
}

/// Confirm a seed for the explicitly named agent; permission is checked before any write.
/// Human confirmations keep their existing receipt format.
pub fn confirm_as(
    directory: &Path,
    revision: &str,
    source: &str,
    timestamp: &str,
    actor: Option<&crate::think_record::GoalConfirmation>,
) -> Result<Value, String> {
    confirm(directory, revision, source, timestamp, true, actor, None)
}

/// Confirm the exact displayed draft and ledger version under the session writer lock.
/// Stale inputs refuse before recording a teach-back skip or confirmation.
pub fn confirm_displayed_as(
    directory: &Path,
    expected: u64,
    revision: &str,
    seed: &str,
    source: &str,
    timestamp: &str,
    actor: Option<&crate::think_record::GoalConfirmation>,
) -> Result<Value, String> {
    confirm(
        directory,
        revision,
        source,
        timestamp,
        true,
        actor,
        Some((expected, seed)),
    )
}

fn confirm(
    directory: &Path,
    revision: &str,
    source: &str,
    timestamp: &str,
    record_skip: bool,
    actor: Option<&crate::think_record::GoalConfirmation>,
    displayed: Option<(u64, &str)>,
) -> Result<Value, String> {
    let actor = crate::think_record::Actor::authorize(actor)?;
    super::require(
        super::python::text(&revision.into()),
        "--revision is required: the displayed label",
    )?;
    super::require(
        super::python::text(&source.into()),
        "--source is required: quote the words that confirmed this revision",
    )?;
    super::require(!source.contains(['\n', '\r']), "--source must be one line")?;
    let _writer = Writer::acquire(directory).map_err(|e| e.reason)?;
    let ledger = load(directory)?;
    if let Some((expected, seed)) = displayed {
        super::require(
            ledger["version"] == super::value::int(expected),
            "stale publication version",
        )?;
        let current = std::fs::read_to_string(directory.join(super::seed::FILE)).map_err(|_| {
            "The draft could not be read. Reopen Shape before confirming.".to_owned()
        })?;
        super::require(
            current == seed,
            "The draft changed. Read it again before confirming.",
        )?;
    }
    let ledger = if record_skip {
        // Refusals precede the skip record: an invalid confirmation must write nothing.
        super::seed::confirmed_as(directory, &ledger, revision, source, timestamp, actor)?;
        let draft = std::fs::read_to_string(directory.join(super::seed::FILE)).map_err(|_| {
            "The draft could not be read. Reopen Shape before confirming.".to_owned()
        })?;
        let state = super::teach_back::skip(&ledger, revision, &draft, timestamp)?;
        publication(
        directory,
        &ledger,
        &state,
        &ledger["version"],
        &actor.reason("confirmed without a completed teach-back.").as_str().into(),
        None,
        timestamp,
    )
    .map_err(|_| {
        "The teach-back skip could not be verified. Reopen this session before confirming again."
            .to_owned()
    })?
    } else {
        ledger
    };
    let (confirmed, record, result) =
        super::seed::confirmed_as(directory, &ledger, revision, source, timestamp, actor)?;
    let path = directory.join(super::seed::FILE);
    atomic_text(
        &directory.join(super::seed::CONFIRMATION),
        &record.pretty()?,
        None,
    )
    .map_err(|e| e.to_string())?;
    let permissions = std::fs::metadata(&path)
        .map_err(|e| e.to_string())?
        .permissions();
    atomic_text(&path, &confirmed, Some(permissions)).map_err(|e| e.to_string())?;
    Ok(result)
}

/// End a session permanently, checking completion against the exact confirmed seed, never a viewer.
/// Uses publication's read-back outcome and lock; a matching repeat is a no-op.
pub fn end(
    directory: &Path,
    status: &str,
    reason: &str,
    timestamp: &str,
) -> Result<Value, PublishError> {
    super::require(
        ["stopped", "completed"].contains(&status),
        "invalid end status",
    )?;
    let _writer = Writer::acquire(directory)?;
    let ledger = load(directory)?;
    super::require(
        ledger["status"].is("active") || ledger["status"].is(status),
        "ended session is read-only",
    )?;
    let mut state = super::editable(&ledger)?;
    state.set("status", status.into());
    state.set("current_question", Value::Null);
    let result = transition(
        &ledger,
        &state,
        &ledger["version"],
        &reason.into(),
        None,
        timestamp,
    )?;
    if status == "completed" {
        let simulated = super::operator(&ledger).is("simulated");
        super::seed::require_seed_line(
            directory,
            if simulated {
                super::seed::SIMULATED
            } else {
                super::seed::CONFIRMED
            },
        )?;
        if !simulated {
            let handoff = super::seed::verify_handoff(directory, &ledger)?;
            super::require(
                handoff["verified"].truth(),
                format!(
                    "cannot complete: {}; save the current revision as a draft, show it, and confirm it again with `seed confirm`",
                    handoff["reason"].string().unwrap()
                ),
            )?;
        }
        super::seed::require_seed_decisions(directory, &ledger, "cannot complete")?;
        super::checks::require_seed_checks(directory, &ledger, true)?;
    }
    let bytes = result.pretty()?;
    commit(
        directory,
        &ledger,
        result,
        |directory, _| atomic_write(directory, &bytes),
        load,
    )
}

#[cfg(test)]
mod tests;
