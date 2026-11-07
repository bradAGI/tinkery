//! Bounded commands named by their callers, off the paint path, with a deadline, bounded output and
//! that every completed or cancelled read stops its process group and reaps the direct child.
//! Descendants that leave that group are outside this ownership; a late result is never delivered.

use std::{
    io::Read as _,
    os::unix::process::CommandExt,
    path::Path,
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

/// The script every read runs, relative to the named checkout.
pub(crate) const SCRIPT: &str = "_system/scripts/sdlc.py";
/// How long one read may take before it is stopped.
pub(crate) const DEADLINE: Duration = Duration::from_secs(30);
const STDOUT_LIMIT: u64 = 4 * 1024 * 1024;
const STDERR_LIMIT: u64 = 64 * 1024;

/// The caller's command identity and diagnostic format, independent of executable arguments.
#[derive(Clone, Copy)]
pub(crate) enum Name<'a> {
    /// An external program's displayed name and ordinary stderr diagnostics.
    Program(&'a str),
    /// An SDLC subcommand's displayed name and preferred `sdlc.py: error:` diagnostic.
    Sdlc(&'a str),
}

impl<'a> Name<'a> {
    fn text(self) -> &'a str {
        match self {
            Self::Program(name) | Self::Sdlc(name) => name,
        }
    }
}

enum Failure {
    Start,
    Finish,
    Message(String),
}

impl Failure {
    fn describe(self, name: Name<'_>) -> String {
        let name = name.text();
        match self {
            Self::Start => {
                format!("Couldn't start {name}. Check that it is installed and on PATH.")
            }
            Self::Finish => format!("Couldn't finish {name}. Reopen this view to try again."),
            Self::Message(message) => message,
        }
    }
}

impl From<String> for Failure {
    fn from(message: String) -> Self {
        Self::Message(message)
    }
}

/// A read in flight. Dropping it cancels, kills and reaps the process before returning.
pub(crate) struct Read<T> {
    cancel: Arc<AtomicBool>,
    result: mpsc::Receiver<Result<T, String>>,
    worker: Option<JoinHandle<()>>,
}

impl<T: Send + 'static> Read<T> {
    /// Start `python3 _system/scripts/sdlc.py <args>` in `root`, stopped after `deadline`.
    pub(crate) fn start(
        root: &Path,
        args: Vec<String>,
        deadline: Duration,
        prepare: fn(&str) -> Result<T, String>,
    ) -> Self {
        let root = root.to_path_buf();
        Self::task(move |stop| run(&root, &args, stop, deadline).and_then(|out| prepare(&out)))
    }

    /// Own a cancellable read transaction. Completion unparks the calling thread after publishing
    /// its result; dropping the read waits for its bounded helpers to stop.
    pub(crate) fn task(
        work: impl FnOnce(&AtomicBool) -> Result<T, String> + Send + 'static,
    ) -> Self {
        let cancel = Arc::new(AtomicBool::new(false));
        let (send, result) = mpsc::channel();
        let stop = cancel.clone();
        let owner = thread::current();
        let worker = thread::spawn(move || {
            let outcome = work(&stop);
            if !stop.load(Ordering::SeqCst) {
                let _ = send.send(outcome);
                owner.unpark();
            }
        });
        Self {
            cancel,
            result,
            worker: Some(worker),
        }
    }

    /// The prepared reading or why it failed; `None` while it is still running.
    pub(crate) fn poll(&self) -> Option<Result<T, String>> {
        match self.result.try_recv() {
            Ok(result) => Some(result),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => Some(Err(
                "Reading runs stopped unexpectedly. Press r to read again.".into(),
            )),
        }
    }
}

impl<T> Drop for Read<T> {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::SeqCst);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// One synchronous bounded status read for an existing worker; cancellation stops its process.
pub(crate) fn run(
    root: &Path,
    args: &[String],
    cancel: &AtomicBool,
    deadline: Duration,
) -> Result<String, String> {
    let mut command = Command::new("python3");
    command.arg(SCRIPT).args(args);
    let name = args.first().map_or("status", String::as_str);
    let identity = Name::Sdlc(name);
    output(root, command, identity, cancel, deadline).map_err(|failure| match (name, failure) {
        ("status", Failure::Start) => "Couldn't start reading runs. Check that Python 3 is installed and the tink-sdlc checkout is accessible, then reopen this view to read again.".into(),
        ("status", Failure::Finish) => "Couldn't finish reading runs. Reopen this view to read again.".into(),
        (_, failure) => failure.describe(identity),
    })
}

/// Execute one already-constructed read or explicit navigation command without a shell.
/// Output, cancellation and process lifetime have the same bounds as status reads; failures
/// use the caller's explicit name, never infer identity from an executable or its arguments.
pub(crate) fn execute(
    root: &Path,
    command: Command,
    name: Name<'_>,
    cancel: &AtomicBool,
    deadline: Duration,
) -> Result<String, String> {
    output(root, command, name, cancel, deadline).map_err(|failure| failure.describe(name))
}

fn output(
    root: &Path,
    mut command: Command,
    identity: Name<'_>,
    cancel: &AtomicBool,
    deadline: Duration,
) -> Result<String, Failure> {
    let name = identity.text();
    let mut child = command
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn()
        .map_err(|_| Failure::Start)?;
    let drain = |pipe: Box<dyn std::io::Read + Send>, limit: u64| {
        thread::spawn(move || {
            let mut bytes = Vec::new();
            let read = pipe.take(limit + 1).read_to_end(&mut bytes);
            (read.is_ok() && bytes.len() as u64 <= limit, bytes)
        })
    };
    let out = drain(Box::new(child.stdout.take().unwrap()), STDOUT_LIMIT);
    let err = drain(Box::new(child.stderr.take().unwrap()), STDERR_LIMIT);
    let until = Instant::now() + deadline;
    let result = loop {
        if cancel.load(Ordering::SeqCst) {
            break Err(Failure::Message("Stopped.".into()));
        }
        if Instant::now() >= until {
            break Err(format!(
                "{name} took longer than {} s and was stopped.",
                deadline.as_secs()
            )
            .into());
        }
        match child.try_wait() {
            Ok(Some(status)) if out.is_finished() && err.is_finished() => break Ok(status),
            Ok(_) => thread::sleep(Duration::from_millis(10)),
            Err(_) => {
                break Err(Failure::Finish);
            }
        }
    };
    stop(&mut child);
    let status = result?;
    let (out_ok, out) = out.join().unwrap_or((false, Vec::new()));
    let (_, err) = err.join().unwrap_or((false, Vec::new()));
    if !out_ok {
        return Err(format!("{name} printed more than 4 MiB; nothing is shown.").into());
    }
    let out =
        String::from_utf8(out).map_err(|_| format!("{name} printed text that isn't UTF-8."))?;
    if status.success() {
        return Ok(out);
    }
    // A refused read still prints the API 1 envelope with error.message on stdout.
    let message = serde_json::from_str::<serde_json::Value>(&out)
        .ok()
        .and_then(|value| value["error"]["message"].as_str().map(str::to_owned))
        .or_else(|| {
            let stderr = String::from_utf8_lossy(&err);
            matches!(identity, Name::Sdlc(_))
                .then(|| {
                    stderr
                        .lines()
                        .find(|line| line.starts_with("sdlc.py: error:"))
                })
                .flatten()
                .or_else(|| stderr.lines().rev().find(|line| !line.trim().is_empty()))
                .map(|line| line.trim().to_owned())
        })
        .unwrap_or_else(|| "no details".into());
    Err(format!("{name} exited with {status}: {message}").into())
}

/// Kill the owned process group, then reap the child; never leave a status command running.
fn stop(child: &mut Child) {
    // SAFETY: kill(2) on the group this process created with process_group(0); no memory is shared.
    unsafe {
        libc::kill(-(child.id() as libc::pid_t), libc::SIGKILL);
    }
    let _ = child.kill();
    let _ = child.wait();
}
