//! A rejection belongs to the checked commit named by its verdict. Review reads only.
//! A separate save creates a detached checkout and a fresh session, never a worker or commit.

use super::preview::{Local, git};
use crate::{
    sdlc::Goal,
    seed_me::{
        self,
        retry::{Base, Prior},
    },
};
use serde_json::Value;
use std::{
    fs,
    io::Read,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

/// Words of the latest local rejection and whether that verdict recorded this commit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Verdict {
    pub(super) words: String,
    pub(super) pinned: bool,
}

/// An operator's recorded rejection and the local reading it applies to.
#[derive(Clone)]
pub(crate) struct Rejected {
    pub(super) local: Local,
    pub(super) verdict: Verdict,
}

/// The exact prior choices and checkout destination shown before saving.
#[derive(Clone, PartialEq)]
pub(crate) struct Reviewed {
    pub(crate) prior: Prior,
    pub(crate) worktrees: PathBuf,
}

/// Only the latest verdict counts. SDLC validates its label chain in the source status read.
/// Old records without a candidate cannot establish which commit the operator rejected.
pub(super) fn words(local: &Local, run: &Value) -> Result<Option<Verdict>, String> {
    if run["labels"]["kinds"]["rejected"].as_u64().unwrap_or(0) == 0 {
        return Ok(None);
    }
    let count = run["labels"]["count"]
        .as_u64()
        .ok_or("Unreadable label count.")?;
    let path = local
        .checkout()
        .join("runs")
        .join(run["slug"].as_str().ok_or("Run name missing.")?)
        .join("labels.jsonl");
    if path.canonicalize().ok().as_ref() != Some(&path) {
        return Err("Cannot read this run's verdict words in its checkout.".into());
    }
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW)
        .open(&path)
        .map_err(|error| error.to_string())?;
    if !file.metadata().is_ok_and(|m| m.is_file()) {
        return Err("The verdict words are not in a regular file.".into());
    }
    const LIMIT: u64 = 4 * 1024 * 1024;
    let mut bytes = Vec::new();
    file.take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > LIMIT || !bytes.ends_with(b"\n") {
        return Err("The verdict record is too large or incomplete.".into());
    }
    let expected = format!("taste@{}", local.head());
    let mut latest = None;
    let mut total = 0;
    for line in bytes[..bytes.len() - 1].split(|&byte| byte == b'\n') {
        let record: Value =
            serde_json::from_slice(line).map_err(|_| "Unreadable verdict record.")?;
        match record["kind"].as_str() {
            Some("accepted") => latest = None,
            Some("rejected") => {
                latest = record["words"]
                    .as_str()
                    .filter(|s| {
                        !s.trim().is_empty()
                            && (record["source"] == expected || record["source"] == "taste")
                    })
                    .map(|words| Verdict {
                        words: words.to_owned(),
                        pinned: record["source"] == expected,
                    })
            }
            Some("correction" | "none") => {}
            _ => return Err("Unrecognized verdict record.".into()),
        }
        total += 1;
    }
    if total != count {
        return Err("The verdict changed while reading. Press r.".into());
    }
    Ok(latest)
}

impl Rejected {
    /// The checked commit shown to the operator.
    pub(crate) fn head(&self) -> &str {
        self.local.head()
    }

    /// The rejection words as recorded.
    pub(crate) fn words(&self) -> &str {
        &self.verdict.words
    }

    /// Older labels require the operator to pair these words with the currently checked commit.
    pub(crate) fn pinned(&self) -> bool {
        self.verdict.pinned
    }

    /// Recheck the run, verified original and latest verdict. Reads no unrelated checkout.
    pub(crate) fn review(&self, stop: &AtomicBool) -> Result<Reviewed, String> {
        let run = self.local.recheck(stop)?;
        if words(&self.local, &run)?.as_ref() != Some(&self.verdict) {
            return Err("The rejection changed. Return to Taste and reread.".into());
        }
        let Goal::Matched(goal) = crate::sdlc::goal(&run) else {
            return Err("The original goal is no longer verified.".into());
        };
        let seed = run["seed_contract"]["source"]
            .as_str()
            .ok_or("The prior seed is missing.")?;
        let prior = seed_me::retry::prior(Path::new(seed), &goal)?;
        let common = git(
            self.local.checkout(),
            &["rev-parse", "--path-format=absolute", "--git-common-dir"],
            false,
            stop,
        )?;
        let repo = Path::new(common.trim_end())
            .parent()
            .and_then(Path::file_name)
            .ok_or("Cannot name the source repository.")?;
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .ok_or("An absolute HOME is required for the new checkout.")?;
        Ok(Reviewed {
            prior,
            worktrees: home.join("dev/worktrees").join(repo),
        })
    }

    /// Recheck the displayed reading before creating a fresh, detached checkout. An interrupted
    /// save names its own artifacts for recovery and never overwrites another attempt.
    pub(crate) fn save(
        &self,
        reviewed: &Reviewed,
        sessions: &Path,
        stop: &AtomicBool,
    ) -> Result<PathBuf, String> {
        if self.review(stop)? != *reviewed {
            return Err("The source changed. Return to Taste and reread before saving.".into());
        }
        seed_me::migration::refuse_old_root(sessions)?;
        fs::create_dir_all(sessions).map_err(|e| e.to_string())?;
        let pending = tempfile::Builder::new()
            .prefix(".pending-retry-")
            .tempdir_in(sessions)
            .map_err(|e| e.to_string())?;
        let staged = seed_me::ledger::initialize(pending.path(), &seed_me::answering::now())?;
        let id = staged
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or("The new session has no valid name.")?;
        let session = sessions.join(id);
        fs::create_dir_all(&reviewed.worktrees).map_err(|e| e.to_string())?;
        let checkout = reviewed
            .worktrees
            .canonicalize()
            .map_err(|e| e.to_string())?
            .join(format!("retry-{id}"));
        if session.try_exists().map_err(|e| e.to_string())? || checkout.symlink_metadata().is_ok() {
            return Err(
                "The new session or checkout location already exists. Reopen the attempt.".into(),
            );
        }
        let path = checkout
            .to_str()
            .filter(|s| !s.chars().any(char::is_control))
            .ok_or("Invalid checkout path.")?;
        let base = Base {
            checkout: checkout.clone(),
            head: self.head().to_owned(),
        };
        reviewed
            .prior
            .save(&staged, &base, self.words(), self.local.id())?;
        git(
            self.local.checkout(),
            &["worktree", "add", "--detach", "--", path, self.head()],
            false,
            stop,
        )
        .map_err(|why| format!("Could not create checkout {path}: {why}"))?;
        if let Err(error) = fs::rename(&staged, &session) {
            // Git refuses to remove a checkout that acquired edits during this save.
            return match git(
                self.local.checkout(),
                &["worktree", "remove", "--", path],
                false,
                stop,
            ) {
                Ok(_) => Err(format!("Could not save the new session: {error}")),
                Err(why) => Err(format!(
                    "Could not save the new session: {error}. Inspect checkout {path}: {why}"
                )),
            };
        }
        Ok(session)
    }
}
