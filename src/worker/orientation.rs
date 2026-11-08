//! Estimate room for a first edit from recent, run-bound Pi launch logs. A missing log,
//! a resume, a different session or a failed tool call cannot establish a first edit.

use super::{Run, identity, read_status};
use crate::sdlc;
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Read},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

const NO_SAMPLE: &str = "Suggested minimum unknown: no usable recent Pi launch evidence.";
const MAX_ENTRIES: usize = 256;
const MAX_LOG_BYTES: u64 = 8 * 1024 * 1024;

struct Launch {
    time: u64,
    step: String,
    log: PathBuf,
    slug: String,
}

enum Observation {
    FirstEdit(usize),
    NoEditThrough(usize),
}

impl Observation {
    fn minimum(&self) -> usize {
        match self {
            Self::FirstEdit(n) => n + 1,
            Self::NoEditThrough(n) => n + 2,
        }
    }
}

/// Advisory only. Sample at most five recent Pi launches in the named repository.
/// Never inspect Pi's session store, send log contents or alter the allowance.
pub(crate) fn cap_advice(root: &Path, cap: usize, cancel: &AtomicBool) -> String {
    let folders = entries(&root.join("runs")).unwrap_or_default();
    if folders.is_empty() {
        return NO_SAMPLE.into();
    }
    let mut logs = Vec::new();
    let worktrees = build_worktrees(root, cancel);
    for entry in folders {
        if cancel.load(Ordering::SeqCst) {
            return NO_SAMPLE.into();
        }
        if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            continue;
        }
        let Ok(slug) = entry.file_name().into_string() else {
            continue;
        };
        collect_logs(root, &slug, &mut logs);
        if let Some(build) = worktrees.get(&slug)
            && build != root
        {
            collect_logs(build, &slug, &mut logs);
        }
    }
    logs.sort_unstable_by(|a, b| b.time.cmp(&a.time).then_with(|| a.log.cmp(&b.log)));
    let mut readings = HashMap::new();
    let mut samples = Vec::new();
    for launch in logs.iter().take(5) {
        if cancel.load(Ordering::SeqCst) {
            return NO_SAMPLE.into();
        }
        let source = readings.entry(&launch.slug).or_insert_with(|| {
            let value = read_status(root, &launch.slug, cancel, Duration::from_secs(1)).ok()?;
            Some((identity(root, &launch.slug, &value).ok()?, value))
        });
        if let Some((run, value)) = source
            && let Some(observation) = observe(launch, run, value, cancel)
        {
            samples.push((launch, observation));
        }
    }
    let Some((source, observed)) = samples.iter().max_by_key(|(_, o)| o.minimum()) else {
        return NO_SAMPLE.into();
    };
    let minimum = observed.minimum();
    let reason = match observed {
        Observation::FirstEdit(n) => {
            format!("first edit/write at response {n}. Allow one follow-up")
        }
        Observation::NoEditThrough(n) => {
            format!("no edit/write through response {n}. Allow one edit and one follow-up")
        }
    };
    let count = samples.len();
    let noun = if count == 1 { "log" } else { "logs" };
    let mut text = format!(
        "Suggested minimum: {minimum} responses. Based on {count} recent Pi launch {noun}.\n{}/{}: {reason}.",
        source.slug, source.step
    );
    if cap < minimum {
        text.push_str(&format!(
            "\n! Your cap of {cap} is below this minimum. F2 keeps your cap."
        ));
    }
    text
}

// Refuse an oversized or linked directory instead of sampling filesystem enumeration order.
fn entries(path: &Path) -> Option<Vec<fs::DirEntry>> {
    if path.canonicalize().ok()? != path {
        return None;
    }
    let files: Vec<_> = fs::read_dir(path)
        .ok()?
        .take(MAX_ENTRIES + 1)
        .collect::<Result<_, _>>()
        .ok()?;
    (files.len() <= MAX_ENTRIES).then_some(files)
}

// Git locates possible step directories. Only SDLC can identify which checkout owns a run.
fn build_worktrees(root: &Path, cancel: &AtomicBool) -> HashMap<String, PathBuf> {
    let mut git = Command::new("git");
    git.args([
        "--no-optional-locks",
        "worktree",
        "list",
        "--porcelain",
        "-z",
    ]);
    let listing =
        sdlc::execute(root, git, "git", cancel, Duration::from_secs(1)).unwrap_or_default();
    listing
        .split("\0\0")
        .filter_map(|block| {
            let mut fields = block.split('\0');
            let path = fields.next()?.strip_prefix("worktree ")?;
            let branch = fields.find_map(|f| f.strip_prefix("branch refs/heads/"))?;
            Some((branch.to_owned(), PathBuf::from(path)))
        })
        .collect()
}

fn collect_logs(root: &Path, slug: &str, logs: &mut Vec<Launch>) {
    let directory = root.join("runs").join(slug).join("steps");
    for entry in entries(&directory).unwrap_or_default() {
        let name = entry.file_name();
        if entry.file_type().is_ok_and(|kind| kind.is_file())
            && let Some((step, timestamp)) = name
                .to_str()
                .and_then(|n| n.strip_suffix(".pi.log"))
                .and_then(|n| n.rsplit_once(".launch-"))
            && let Ok(time) = timestamp.parse()
        {
            logs.push(Launch {
                time,
                step: step.into(),
                log: entry.path(),
                slug: slug.into(),
            });
        }
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

#[derive(Deserialize)]
#[serde(tag = "type")]
enum Event {
    #[serde(rename = "message_end")]
    End { message: Message },
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
#[serde(tag = "role")]
enum Message {
    #[serde(rename = "assistant")]
    Assistant,
    #[serde(rename = "toolResult", rename_all = "camelCase")]
    Tool { tool_name: String, is_error: bool },
    #[serde(other)]
    Other,
}

fn observe(
    launch: &Launch,
    run: &Run,
    value: &sdlc::Run,
    cancel: &AtomicBool,
) -> Option<Observation> {
    let steps_dir = run.root.join("runs").join(&run.slug).join("steps");
    if launch.log.parent()? != steps_dir {
        return None;
    }
    let step = value
        .steps
        .as_ref()?
        .iter()
        .find(|s| s.data["name"] == launch.step)?;
    if !matches!(
        step.state.known(),
        Some(sdlc::StepState::Active | sdlc::StepState::Completed | sdlc::StepState::Stopped)
    ) {
        return None;
    }
    let stopped_at_cap =
        step.state.is(sdlc::StepState::Stopped) && step.stopped_by.is(sdlc::StopReason::Cap);
    let contract = regular(&launch.log.with_file_name(format!("{}.json", launch.step)))?;
    if contract.metadata().ok()?.len() > 16 * 1024 {
        return None;
    }
    let contract: Value = serde_json::from_reader(contract.take(16 * 1024)).ok()?;
    if contract["schema"] != 1
        || contract["run_id"] != run.id
        || contract["checkout"].as_str() != run.root.to_str()
    {
        return None;
    }
    let session = contract["session"].as_str().filter(|s| !s.is_empty())?;
    let log = regular(&launch.log)?;
    let mut lines = BufReader::new(log.take(MAX_LOG_BYTES)).lines();
    let header: Value = serde_json::from_str(&lines.next()?.ok()?).ok()?;
    if header["type"] != "session"
        || header["version"] != 3
        || header["id"] != session
        || header["cwd"] != contract["checkout"]
    {
        return None;
    }

    let cap_stop = contract["max_responses"]
        .as_u64()
        .filter(|n| *n > 0)
        .map(|n| (n, format!("pi-launcher: response cap {n} reached;")));
    let mut responses = 0;
    for line in lines {
        if cancel.load(Ordering::SeqCst) {
            return None;
        }
        let line = line.ok()?;
        if stopped_at_cap
            && let Some((cap, ref marker)) = cap_stop
            && line.starts_with(marker)
        {
            return (responses as u64 >= cap).then_some(Observation::NoEditThrough(responses));
        }
        if let Event::End { message } = serde_json::from_str(&line).ok()? {
            match message {
                Message::Assistant => responses += 1,
                Message::Tool {
                    tool_name,
                    is_error: false,
                } if matches!(tool_name.as_str(), "edit" | "write") => {
                    return (responses > 0).then_some(Observation::FirstEdit(responses));
                }
                _ => {}
            }
        }
    }
    None
}
