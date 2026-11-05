//! One bounded `sdlc.py status` read off the paint path. It promises a deadline, bounded output and
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
    execute(root, command, cancel, deadline)
}

/// Execute one already-constructed read or explicit navigation command without a shell.
/// Output, cancellation and process lifetime have the same bounds as status reads; failures
/// name the command that failed, so a `gh` or `git` failure never blames Python or tink-sdlc.
pub(crate) fn execute(
    root: &Path,
    mut command: Command,
    cancel: &AtomicBool,
    deadline: Duration,
) -> Result<String, String> {
    let program = command.get_program().to_string_lossy().into_owned();
    // An sdlc.py failure names its subcommand (status, label), never Python.
    let subcommand = (program == "python3").then(|| {
        command
            .get_args()
            .nth(1)
            .map_or("status".to_owned(), |arg| {
                arg.to_string_lossy().into_owned()
            })
    });
    let status_read = subcommand.as_deref() == Some("status");
    let name = subcommand.as_deref().unwrap_or(&program);
    let mut child = command
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn()
        .map_err(|_| if status_read {
            "Couldn't start reading runs. Check that Python 3 is installed and the tink-sdlc checkout is accessible, then reopen this view to read again.".to_owned()
        } else {
            format!("Couldn't start {name}. Check that it is installed and on PATH.")
        })?;
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
            break Err("Stopped.".into());
        }
        if Instant::now() >= until {
            break Err(format!(
                "{name} took longer than {} s and was stopped.",
                deadline.as_secs()
            ));
        }
        match child.try_wait() {
            Ok(Some(status)) if out.is_finished() && err.is_finished() => break Ok(status),
            Ok(_) => thread::sleep(Duration::from_millis(10)),
            Err(_) => {
                break Err(if status_read {
                    "Couldn't finish reading runs. Reopen this view to read again.".into()
                } else {
                    format!("Couldn't finish {name}. Reopen this view to try again.")
                });
            }
        }
    };
    stop(&mut child);
    let status = result?;
    let (out_ok, out) = out.join().unwrap_or((false, Vec::new()));
    let (_, err) = err.join().unwrap_or((false, Vec::new()));
    if !out_ok {
        return Err(format!("{name} printed more than 4 MiB; nothing is shown."));
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
            subcommand
                .as_ref()
                .and_then(|_| {
                    stderr
                        .lines()
                        .find(|line| line.starts_with("sdlc.py: error:"))
                })
                .or_else(|| stderr.lines().rev().find(|line| !line.trim().is_empty()))
                .map(|line| line.trim().to_owned())
        })
        .unwrap_or_else(|| "no details".into());
    Err(format!("{name} exited with {status}: {message}"))
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
