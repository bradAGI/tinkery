//! Closed-run postmortem records, read from tink-sdlc status, the run's own files and Git.
//! Nothing is written, inferred or estimated: a record that is missing or doesn't agree with its
//! source reads as unknown, never as zero.

use crate::sdlc;
use serde_json::Value;
use std::{
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Read as _},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::AtomicBool,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

/// At most this many recently closed runs are read in one pass.
pub(super) const RUNS: usize = 20;
/// A merge younger than this many days is too recent to call held.
pub(super) const HOLD_DAYS: u64 = 14;
const LIMIT: u64 = 8 * 1024 * 1024;

/// One read: recently closed runs, newest first, and what the checkout couldn't report.
pub(super) struct Postmortem {
    /// Closed runs, newest closure first.
    pub(super) runs: Vec<Closed>,
    /// Closed runs beyond [`RUNS`] that this pass left unread.
    pub(super) more: usize,
    /// Runs whose status tink-sdlc reported as unreadable.
    pub(super) unreadable: usize,
}

/// One closed run as its records state it.
pub(super) struct Closed {
    /// The run's folder name.
    pub(super) slug: String,
    /// The merge commit, or `None` when the run was abandoned.
    pub(super) merge: Option<String>,
    /// Closure time in nanoseconds since the Unix epoch.
    pub(super) closed_ns: u128,
    /// The recorded PR URL, if any.
    pub(super) pr: Option<String>,
    /// Every decision receipt, oldest first.
    pub(super) decisions: Vec<Decision>,
    /// Operator labels, oldest first, or why their words can't be shown.
    pub(super) labels: Result<Vec<Label>, String>,
    /// Pi responses per step, in step-name order. `None` per step is unknown.
    pub(super) steps: Vec<(String, Option<usize>)>,
    /// Whether the merge held on this checkout's `HEAD`.
    pub(super) held: Hold,
}

/// One `decide` receipt. Only the operator's count as gates.
pub(super) struct Decision {
    /// The gated stage.
    pub(super) stage: u64,
    /// `approved` or `changes-requested`, verbatim.
    pub(super) decision: String,
    /// The recorded reviewer, verbatim.
    pub(super) reviewer: String,
    /// Who recorded it.
    pub(super) by: By,
}

/// Who recorded a decision receipt.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum By {
    /// A person's gate decision.
    Operator,
    /// An agent recorded it; it is not the operator's gate.
    Agent,
    /// tink-sdlc bound the confirmed seed itself.
    Automatic,
}

/// One operator label, with its words exactly as written.
pub(super) struct Label {
    /// `correction`, `none`, `accepted` or `rejected`.
    pub(super) kind: String,
    /// The operator's words.
    pub(super) words: String,
}

/// Whether a merged result held. Unknown keeps its reason.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(super) enum Hold {
    /// Abandoned runs have no merge to hold.
    Abandoned,
    /// A commit on `HEAD` reverts the merge.
    Reverted(String),
    /// Not reverted, and closed at least [`HOLD_DAYS`] ago.
    Held,
    /// Not reverted yet, but closed under [`HOLD_DAYS`] ago.
    TooRecent,
    /// Git couldn't say.
    Unknown(String),
}

impl Closed {
    /// Operator gate decisions. Agent and automatic receipts are not the operator's.
    pub(super) fn gates(&self) -> usize {
        self.decisions
            .iter()
            .filter(|d| d.by == By::Operator)
            .count()
    }

    /// Correction labels, or `None` when the labels are unreadable.
    pub(super) fn corrections(&self) -> Option<usize> {
        self.labels
            .as_ref()
            .ok()
            .map(|labels| labels.iter().filter(|l| l.kind == "correction").count())
    }

    /// All counted Pi responses, or `None` if any step is unknown or no step was recorded.
    pub(super) fn pi(&self) -> Option<usize> {
        if self.steps.is_empty() {
            return None;
        }
        self.steps.iter().map(|(_, n)| *n).sum()
    }
}

/// Read up to [`RUNS`] closed runs in `root`, all within `deadline`.
pub(super) fn read(
    root: &Path,
    cancel: &AtomicBool,
    deadline: Duration,
) -> Result<Postmortem, String> {
    let until = Instant::now() + deadline;
    let left = || until.saturating_duration_since(Instant::now());
    let list = sdlc::envelope(&sdlc::status(
        root,
        &["status".into(), "--json".into()],
        cancel,
        left(),
    )?)?;
    let listed = list["runs"]
        .as_array()
        .ok_or("The run list is unreadable. Results unknown; press p to retry.")?;
    let unreadable = listed.iter().filter(|r| r.get("error").is_some()).count();
    let mut closed: Vec<(u128, &str)> = listed
        .iter()
        .filter_map(|run| Some((closure_ns(&run["closure"])?, run["slug"].as_str()?)))
        .collect();
    closed.sort_unstable_by(|a, b| b.cmp(a));
    let more = closed.len().saturating_sub(RUNS);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let mut runs = Vec::new();
    for (closed_ns, slug) in closed.into_iter().take(RUNS) {
        let output = sdlc::status(
            root,
            &["status".into(), slug.into(), "--json".into()],
            cancel,
            left(),
        )?;
        let value = sdlc::envelope(&output)?;
        let run = &value["run"];
        if run["slug"].as_str() != Some(slug) || closure_ns(&run["closure"]) != Some(closed_ns) {
            return Err(format!(
                "Run {slug} changed while it was read. Press p to read again."
            ));
        }
        runs.push(closed_run(root, slug, run, now, cancel, left())?);
    }
    Ok(Postmortem {
        runs,
        more,
        unreadable,
    })
}

fn closure_ns(closure: &Value) -> Option<u128> {
    matches!(closure["outcome"].as_str(), Some("merged" | "abandoned"))
        .then(|| closure["time_ns"].as_u64().map(u128::from))
        .flatten()
}

fn closed_run(
    root: &Path,
    slug: &str,
    run: &Value,
    now: u128,
    cancel: &AtomicBool,
    deadline: Duration,
) -> Result<Closed, String> {
    let closure = &run["closure"];
    let merge = match closure["outcome"].as_str() {
        Some("merged") => Some(
            closure["merge"]
                .as_str()
                .filter(|c| c.len() == 40 && c.bytes().all(|b| b.is_ascii_hexdigit()))
                .ok_or(format!(
                    "Run {slug} is merged without a readable merge commit."
                ))?
                .to_owned(),
        ),
        _ => None,
    };
    let checkout = match run.get("checkout") {
        Some(path) => PathBuf::from(path.as_str().ok_or("A build checkout is unreadable.")?),
        None => root.to_path_buf(),
    };
    let folder = checkout.join("runs").join(slug);
    // Resolve the checkout once (macOS temp paths are links); files below it must not be links.
    let folder = folder.canonicalize().unwrap_or(folder);
    let pr = run["pr"]["url"].as_str().map(str::to_owned);
    let closed_ns = closure_ns(closure).unwrap_or_default();
    let held = match &merge {
        None => Hold::Abandoned,
        Some(commit) => held(
            root,
            commit,
            pr.as_deref(),
            now.saturating_sub(closed_ns),
            cancel,
            deadline,
        ),
    };
    Ok(Closed {
        slug: slug.into(),
        merge,
        closed_ns,
        pr,
        decisions: decisions(run),
        labels: labels(&folder, run["labels"]["count"].as_u64()),
        steps: steps(&folder, run),
        held,
    })
}

fn decisions(run: &Value) -> Vec<Decision> {
    let text = |v: &Value| v.as_str().unwrap_or("unknown").to_owned();
    run["decisions"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|d| Decision {
            stage: d["stage"].as_u64().unwrap_or_default(),
            decision: text(&d["decision"]),
            reviewer: text(&d["reviewer"]),
            by: if d.get("basis").is_some() {
                By::Automatic
            } else if d.get("attribution").is_some() {
                By::Agent
            } else {
                By::Operator
            },
        })
        .collect()
}

// tink-sdlc has validated the chain; this reads only kind and words, and refuses a count mismatch.
fn labels(folder: &Path, count: Option<u64>) -> Result<Vec<Label>, String> {
    let file =
        regular(&folder.join("labels.jsonl")).ok_or("Labels unreadable in this checkout.")?;
    let mut labels = Vec::new();
    for line in BufReader::new(file.take(LIMIT)).lines() {
        let record: Value = line
            .ok()
            .and_then(|l| serde_json::from_str(&l).ok())
            .ok_or("A label line is unreadable.")?;
        match (record["kind"].as_str(), record["words"].as_str()) {
            (Some(kind), Some(words)) => labels.push(Label {
                kind: kind.into(),
                words: words.into(),
            }),
            _ => return Err("A label has no kind or words.".into()),
        }
    }
    if Some(labels.len() as u64) != count {
        return Err("Labels don't match the run's recorded count.".into());
    }
    Ok(labels)
}

fn steps(folder: &Path, run: &Value) -> Vec<(String, Option<usize>)> {
    let mut steps: Vec<_> = run["steps"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|s| s["name"].as_str())
        .map(|name| (name.to_owned(), responses(&folder.join("steps"), name)))
        .collect();
    steps.sort();
    steps
}

/// Assistant answers across a step's launch and resume logs, bound to its contract's session.
/// No log, a foreign session or an unreadable event makes the step unknown.
fn responses(directory: &Path, step: &str) -> Option<usize> {
    let contract: Value =
        serde_json::from_reader(regular(&directory.join(format!("{step}.json")))?.take(64 * 1024))
            .ok()?;
    let session = contract["session"].as_str().filter(|s| !s.is_empty())?;
    let prefix = format!("{step}.");
    let logs: Vec<_> = fs::read_dir(directory)
        .ok()?
        .filter_map(Result::ok)
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|name| {
            name.strip_prefix(&prefix)
                .and_then(|rest| rest.strip_suffix(".pi.log"))
                .and_then(|rest| {
                    rest.strip_prefix("launch-")
                        .or(rest.strip_prefix("resume-"))
                })
                .is_some_and(|time| !time.is_empty() && time.bytes().all(|b| b.is_ascii_digit()))
        })
        .collect();
    if logs.is_empty() {
        return None;
    }
    let mut total = 0;
    for name in logs {
        total += log_responses(regular(&directory.join(name))?, session)?;
    }
    Some(total)
}

fn log_responses(file: File, session: &str) -> Option<usize> {
    let mut lines = BufReader::new(file.take(LIMIT)).lines();
    let header: Value = serde_json::from_str(&lines.next()?.ok()?).ok()?;
    if header["type"] != "session" || header["id"] != session {
        return None;
    }
    let mut count = 0;
    for line in lines {
        let line = line.ok()?;
        // The launcher's own notes share the log; only JSON events are Pi's.
        if !line.starts_with('{') {
            continue;
        }
        let event: Value = serde_json::from_str(&line).ok()?;
        if event["type"] == "message_end" && event["message"]["role"] == "assistant" {
            count += 1;
        }
    }
    Some(count)
}

fn held(
    root: &Path,
    merge: &str,
    pr: Option<&str>,
    age_ns: u128,
    cancel: &AtomicBool,
    deadline: Duration,
) -> Hold {
    let git = |args: &[&str]| {
        let mut command = Command::new("git");
        command.arg("--no-optional-locks").args(args);
        sdlc::execute(root, command, "git", cancel, deadline)
    };
    if git(&["merge-base", "--is-ancestor", merge, "HEAD"]).is_err() {
        return Hold::Unknown("the merge commit isn't in this checkout's HEAD history".into());
    }
    let mut args = vec![
        "log".to_owned(),
        "--format=%H".into(),
        "--extended-regexp".into(),
        format!("--grep=This reverts commit {merge}"),
    ];
    // GitHub's revert button writes "Reverts OWNER/REPO#N"; #1 must not match #12.
    if let Some((repo, number)) = pr
        .and_then(|url| url.strip_prefix("https://github.com/"))
        .and_then(|rest| rest.split_once("/pull/"))
    {
        let repo = repo.replace('.', "[.]");
        args.push(format!("--grep=Reverts {repo}#{number}([^0-9]|$)"));
    }
    args.push(format!("{merge}..HEAD"));
    match git(&args.iter().map(String::as_str).collect::<Vec<_>>()) {
        Err(error) => Hold::Unknown(error),
        Ok(out) => match out.lines().last() {
            Some(commit) => Hold::Reverted(commit.to_owned()),
            None if age_ns >= u128::from(HOLD_DAYS) * 86_400 * 1_000_000_000 => Hold::Held,
            None => Hold::TooRecent,
        },
    }
}

fn regular(path: &Path) -> Option<File> {
    if path.canonicalize().ok()? != path {
        return None;
    }
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .ok()?;
    file.metadata().ok()?.is_file().then_some(file)
}
