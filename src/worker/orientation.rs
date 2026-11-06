//! Estimate room for a first edit from recent, run-bound Pi launch logs. A missing log,
//! a resume, a different session or a failed tool call cannot establish a first edit.

use super::{Run, identity, read_status};
use serde::Deserialize;
use serde_json::Value;
use std::{
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Read},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
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
    run: Run,
    stopped_at_cap: bool,
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

/// Advisory only. Read up to five fresh Pi starts in the three most recently updated run
/// folders. Never inspect Pi's session store, send log contents or alter the worker allowance.
pub(crate) fn cap_advice(root: &Path, cap: usize, cancel: &AtomicBool) -> String {
    let mut folders = Vec::new();
    for entry in entries(&root.join("runs")).unwrap_or_default() {
        if let Ok(kind) = entry.file_type()
            && kind.is_dir()
            && let Ok(modified) = entry.metadata().and_then(|m| m.modified())
            && let Ok(slug) = entry.file_name().into_string()
        {
            folders.push((modified, slug));
        }
    }
    folders.sort_unstable_by(|a, b| b.cmp(a));
    let mut logs = Vec::new();
    for (_, slug) in folders.into_iter().take(3) {
        if cancel.load(Ordering::SeqCst) {
            return NO_SAMPLE.into();
        }
        if let Ok(value) = read_status(root, &slug, cancel, Duration::from_secs(2))
            && let Ok(run) = identity(root, &slug, &value)
            && let Some(steps) = value["steps"].as_array()
        {
            logs.extend(launches(run, steps));
        }
    }
    logs.sort_unstable_by(|a, b| b.time.cmp(&a.time).then_with(|| a.log.cmp(&b.log)));
    let mut samples = Vec::new();
    for launch in logs.iter().take(5) {
        if cancel.load(Ordering::SeqCst) {
            return NO_SAMPLE.into();
        }
        if let Some(observation) = observe(launch, cancel) {
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
        source.run.slug, source.step
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

fn launches(run: Run, steps: &[Value]) -> Vec<Launch> {
    let directory = run.root.join("runs").join(&run.slug).join("steps");
    let mut launches = Vec::new();
    for entry in entries(&directory).unwrap_or_default() {
        let name = entry.file_name();
        let Some((step, timestamp)) = name
            .to_str()
            .and_then(|n| n.strip_suffix(".pi.log"))
            .and_then(|n| n.rsplit_once(".launch-"))
        else {
            continue;
        };
        let Some(time) = timestamp.parse().ok() else {
            continue;
        };
        let Some(recorded) = steps.iter().find(|s| {
            s["name"] == step
                && matches!(
                    s["state"].as_str(),
                    Some("completed" | "stopped" | "active")
                )
        }) else {
            continue;
        };
        launches.push(Launch {
            time,
            step: step.into(),
            log: entry.path(),
            run: run.clone(),
            stopped_at_cap: recorded["state"] == "stopped" && recorded["stopped_by"] == "cap",
        });
    }
    launches
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

fn observe(launch: &Launch, cancel: &AtomicBool) -> Option<Observation> {
    let contract = regular(&launch.log.with_file_name(format!("{}.json", launch.step)))?;
    if contract.metadata().ok()?.len() > 16 * 1024 {
        return None;
    }
    let contract: Value = serde_json::from_reader(contract.take(16 * 1024)).ok()?;
    if contract["schema"] != 1
        || contract["run_id"] != launch.run.id
        || contract["checkout"].as_str() != launch.run.root.to_str()
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
        if launch.stopped_at_cap
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
