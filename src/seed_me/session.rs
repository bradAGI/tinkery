//! Reading one saved session straight from session files. Read-only: nothing here writes.
//! Files are read directly rather than through the helper, so a scan of thousands of
//! sessions stays fast (see menu.md's measured scope).

use std::path::{Path, PathBuf};

use serde_json::Value;

/// The stage declared by session files, not verification of a seed's confirmation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// Active, with a confirmed goal and no seed contract yet.
    GoalSaved,
    /// Active, with a seed contract still in draft.
    SeedDrafted,
    /// Active, with a seed declaring confirmation; only the helper can verify it.
    SeedUnchecked,
    /// Finished: the only stage that satisfies a need.
    Completed,
    /// Abandoned before finishing; nothing to continue, and no need is met by it.
    Stopped,
    /// The files say something this module doesn't recognise. Never guessed.
    Unknown,
    /// A practice session's seed, not a real one.
    Simulated,
}
impl Stage {
    /// The stage a ledger `status` and the seed contract's text (if one exists) mean. Anything
    /// it doesn't recognise is `Unknown`, never a guess.
    pub fn from_files(status: &str, seed: Option<&str>) -> Self {
        match (status, seed.and_then(|s| s.lines().next())) {
            ("completed", _) => Self::Completed,
            ("stopped", _) => Self::Stopped,
            ("active", None) => Self::GoalSaved,
            ("active", Some("status: draft")) => Self::SeedDrafted,
            ("active", Some("status: confirmed for intake")) => Self::SeedUnchecked,
            ("active", Some("status: simulated")) => Self::Simulated,
            _ => Self::Unknown,
        }
    }
    /// The stage in plain words, for a list row.
    pub fn label(self) -> &'static str {
        match self {
            Self::GoalSaved => "goal saved",
            Self::SeedDrafted => "seed drafted",
            Self::SeedUnchecked => "seed needs checking",
            Self::Completed => "completed",
            Self::Stopped => "stopped",
            Self::Unknown => "status unknown",
            Self::Simulated => "practice seed",
        }
    }
    /// Over, whether completed or stopped: nothing to continue.
    pub fn ended(self) -> bool {
        matches!(self, Self::Completed | Self::Stopped)
    }
    /// Whether Shape can open it: only a live session with a real goal.
    pub fn can_shape(self) -> bool {
        matches!(
            self,
            Self::GoalSaved | Self::SeedDrafted | Self::SeedUnchecked
        )
    }
    /// What to do next at this stage; none when there is nothing to continue.
    pub fn next(self) -> Option<&'static str> {
        match self {
            Self::GoalSaved => Some("Next: shape it into a seed in Shape."),
            Self::SeedDrafted => Some("Next: confirm the seed draft in Shape."),
            Self::SeedUnchecked => Some("Next: check the seed in Shape."),
            _ => None,
        }
    }
}

/// One readable session.
#[derive(Debug, Clone, PartialEq)]
pub struct Session {
    /// The ledger's nonempty, control-free session_id; never inferred from a path or title.
    pub id: Option<String>,
    /// The session folder, canonicalised.
    pub path: PathBuf,
    /// The confirmed goal as recorded, or a plain placeholder when none is.
    pub goal: String,
    /// Its declared stage; seed confirmation has not been verified by this file read.
    pub stage: Stage,
    /// The ledger's last update time as recorded; empty when absent.
    pub updated: String,
}

/// Maximum session failures retained by one discovery result; further failures are counted.
pub const MAX_READ_FAILURES: usize = 8;

/// One failed session read, with its actual source and reason; never a guessed session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadFailure {
    /// The ledger or folder operation that failed, without canonicalisation that could also fail.
    pub path: PathBuf,
    /// The read/parse failure, bounded to 512 characters; truncated text ends with an ellipsis.
    pub reason: String,
}
impl ReadFailure {
    /// Retain a bounded reason without reading or changing the failed source.
    pub fn new(path: PathBuf, reason: impl std::fmt::Display) -> Self {
        let reason = reason.to_string();
        let mut bounded: String = reason.chars().take(512).collect();
        if reason.chars().count() > 512 {
            bounded.push('…');
        }
        Self {
            path,
            reason: bounded,
        }
    }
}

/// What one folder holds.
pub enum Record {
    /// A session that could be read.
    Session(Session),
    /// No ledger: some other folder, not counted.
    NotASession,
    /// A ledger exists but can't be read or has no status. Never guessed or counted.
    Unreadable(ReadFailure),
}

/// Read the session in folder `path`. Promises a `Session` only when the ledger parses and has
/// a status; refuses to guess a stage for an active session with no goal (`Stage::Unknown`).
pub fn read(path: &Path) -> Record {
    if let Err(reason) = super::migration::refuse_old_root(path) {
        return Record::Unreadable(ReadFailure::new(path.to_path_buf(), reason));
    }
    use std::io::ErrorKind::{NotADirectory, NotFound};
    let ledger_path = path.join("ledger.json");
    let text = match super::read_regular_text(&ledger_path, "ledger") {
        Ok(text) => text,
        Err(error) if matches!(error.kind(), NotFound | NotADirectory) => {
            return Record::NotASession;
        }
        Err(error) => return Record::Unreadable(ReadFailure::new(ledger_path, error)),
    };
    let ledger = match serde_json::from_str::<Value>(&text) {
        Ok(ledger) => ledger,
        Err(error) => {
            return Record::Unreadable(ReadFailure::new(
                ledger_path,
                format!("Invalid JSON: {error}"),
            ));
        }
    };
    let Some(status) = ledger.get("status").and_then(Value::as_str) else {
        return Record::Unreadable(ReadFailure::new(
            ledger_path,
            "The ledger status is missing or is not a string.",
        ));
    };
    let id = ledger
        .get("session_id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty() && !id.chars().any(char::is_control))
        .map(str::to_owned);
    let goal = ledger
        .get("goal")
        .and_then(Value::as_str)
        .unwrap_or("(no goal recorded)")
        .to_string();
    let updated = ledger
        .get("updated_at")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let stage = if ledger
        .get("goal")
        .and_then(Value::as_str)
        .is_none_or(|goal| goal.trim().is_empty())
        && status == "active"
    {
        Stage::Unknown
    } else {
        match super::read_seed_text(&path.join("seed-contract.md")) {
            Ok(seed) => Stage::from_files(status, Some(&seed)),
            Err(error) if error.kind() == NotFound => Stage::from_files(status, None),
            Err(_) => Stage::Unknown,
        }
    };
    match path.canonicalize() {
        Ok(path) => Record::Session(Session {
            id,
            path,
            goal,
            stage,
            updated,
        }),
        Err(error) => Record::Unreadable(ReadFailure::new(
            path.to_path_buf(),
            format!("Cannot resolve the session folder: {error}"),
        )),
    }
}

/// Read the ledger's existing session identifier only while its saved goal still equals `goal`.
/// Refuses missing identity, an unsupported ledger format or a changed goal; never invents an
/// identifier from the directory name and never changes the ledger.
pub fn session_id(path: &Path, goal: &str) -> Result<String, String> {
    super::migration::refuse_old_root(path)?;
    let ledger_path = path.join("ledger.json");
    let text = super::read_regular_text(&ledger_path, "ledger")
        .map_err(|error| format!("{}: {error}", ledger_path.display()))?;
    let ledger: Value = serde_json::from_str(&text)
        .map_err(|error| format!("{}: {error}", ledger_path.display()))?;
    let refuse = |reason: &str| format!("{}: {reason}", ledger_path.display());
    if ledger.get("schema_version").and_then(Value::as_u64) != Some(1) {
        return Err(refuse("unsupported session ledger schema"));
    }
    if ledger.get("goal").and_then(Value::as_str) != Some(goal) {
        return Err(refuse("the saved goal changed"));
    }
    ledger
        .get("session_id")
        .and_then(Value::as_str)
        .filter(|id| !id.trim().is_empty())
        .map(str::to_owned)
        .ok_or_else(|| refuse("the saved session has no identity"))
}

#[cfg(test)]
mod identity_tests {
    use super::*;
    use std::fs;

    #[test]
    fn completed_and_stopped_are_both_over_but_say_which() {
        for (status, stage, label) in [
            ("completed", Stage::Completed, "completed"),
            ("stopped", Stage::Stopped, "stopped"),
        ] {
            let read = Stage::from_files(status, Some("status: confirmed for intake"));
            assert_eq!((read, read.label()), (stage, label));
            assert!(read.ended() && !read.can_shape() && read.next().is_none());
        }
    }

    #[test]
    fn file_only_seed_status_never_claims_verified_readiness() {
        let root = tempfile::tempdir().unwrap();
        let ledger = root.path().join("ledger.json");
        fs::write(
            &ledger,
            r#"{"schema_version":1,"session_id":"TEST-id","status":"active","goal":"TEST goal"}"#,
        )
        .unwrap();
        let seed = root.path().join("seed-contract.md");
        fs::write(&seed, "status: confirmed for intake\n# TEST changed seed\n").unwrap();
        for confirmation in [None, Some("not JSON"), Some(r#"{"revision":"r1"}"#)] {
            if let Some(text) = confirmation {
                fs::write(root.path().join("seed-confirmation.json"), text).unwrap();
            }
            let before = fs::read(&seed).unwrap();
            let Record::Session(session) = read(root.path()) else {
                panic!("the declared stage must remain readable");
            };
            assert_eq!(session.stage.label(), "seed needs checking");
            assert_eq!(session.stage.next(), Some("Next: check the seed in Shape."));
            assert!(
                session.stage.can_shape(),
                "manual continuation stays available"
            );
            assert_eq!(fs::read(&seed).unwrap(), before);
        }
    }

    #[test]
    fn identity_comes_from_the_same_saved_goal_and_never_from_the_folder_name() {
        let root = tempfile::tempdir().unwrap();
        let session = root.path().join("TEST-folder-name");
        fs::create_dir(&session).unwrap();
        let ledger = session.join("ledger.json");
        let original = serde_json::json!({
            "schema_version": 1,
            "session_id": "TEST-existing-id",
            "goal": "TEST goal",
        });
        let original_bytes = serde_json::to_vec(&original).unwrap();
        fs::write(&ledger, &original_bytes).unwrap();
        assert_eq!(
            session_id(&session, "TEST goal").unwrap(),
            "TEST-existing-id"
        );
        assert_eq!(fs::read(&ledger).unwrap(), original_bytes);
        assert!(
            session_id(&session, "TEST different")
                .unwrap_err()
                .contains("goal changed")
        );
        for replacement in [Value::Null, "".into(), "  ".into(), 5.into()] {
            let mut value = original.clone();
            value["session_id"] = replacement;
            fs::write(&ledger, value.to_string()).unwrap();
            assert!(
                session_id(&session, "TEST goal")
                    .unwrap_err()
                    .contains("no identity")
            );
        }
        let mut value = original;
        value["schema_version"] = 2.into();
        fs::write(&ledger, value.to_string()).unwrap();
        assert!(
            session_id(&session, "TEST goal")
                .unwrap_err()
                .contains("unsupported")
        );
    }
}
