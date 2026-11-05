//! Explicit repo declarations and one cancellable exact-head preview. No command inference,
//! public URL, operator checkout, agent or durable preview record is supported.

use crate::sdlc::{self, DEADLINE, Read};
use serde::Deserialize;
use serde_json::Value;
use std::{
    io::{Read as _, Write as _},
    net::{SocketAddr, TcpStream},
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

/// Exact-head 404: help explains how to add the absent declaration.
pub(super) const NO_DECLARATION: &str = "No preview for this project.";

/// A source-owned local result, bound to its run, checked commit, receipt and original goal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Local {
    root: PathBuf,
    checkout: PathBuf,
    slug: String,
    id: String,
    head: String,
    receipt: Value,
    seed: Value,
    goal: sdlc::Goal,
}

impl Local {
    /// Require a current checked commit whose source files are committed; run evidence is separate.
    pub(super) fn read(root: &Path, run: &Value, stop: &AtomicBool) -> Result<Self, String> {
        if run.get("pr") != Some(&Value::Null) || !run["closure"].is_null() {
            return Err("This run is not an open local result; press r.".into());
        }
        let receipt = &run["verification"];
        if run["verification_status"] != "current"
            || receipt["passed"] != true
            || receipt["result"] != "passed"
        {
            return Err("Local preview needs a current passed verification. Commit and verify, then press r.".into());
        }
        let head = receipt["candidate"]["head"]
            .as_str()
            .filter(|head| head.len() == 40 && head.bytes().all(|b| b.is_ascii_hexdigit()))
            .ok_or("Local result commit is unknown; commit and verify, then press r.")?;
        let source = match run.get("checkout") {
            None => root,
            Some(value) => Path::new(
                value
                    .as_str()
                    .filter(|s| !s.contains('\0'))
                    .ok_or("Local result checkout is invalid; inspect the run and press r.")?,
            ),
        };
        if !source.is_absolute() {
            return Err(
                "Local result checkout must be absolute; inspect the run and press r.".into(),
            );
        }
        let checkout = source
            .canonicalize()
            .map_err(|_| "Local result checkout is unavailable.")?;
        let actual = git(&checkout, &["rev-parse", "--show-toplevel"], false, stop)?;
        if Path::new(actual.trim()) != checkout {
            return Err(
                "Local result checkout is not its Git root; inspect the run and press r.".into(),
            );
        }
        let paths = [".", ":(exclude)runs"];
        let diff = [
            "diff",
            "--quiet",
            "--no-ext-diff",
            "--no-textconv",
            head,
            "--",
        ];
        let differs =
            "Local source differs from the checked commit. Commit and verify, then press r.";
        git(
            &checkout,
            &[&diff[..], &paths, &[":(exclude)AGENTS.md"]].concat(),
            false,
            stop,
        )
        .map_err(|_| differs)?;
        if git(
            &checkout,
            &[&diff[..], &["AGENTS.md"]].concat(),
            false,
            stop,
        )
        .is_err()
            && !generated_rules_only(&checkout, head, stop)
        {
            return Err(differs.into());
        }
        let mut untracked = vec!["ls-files", "--others", "--exclude-standard", "-z", "--"];
        untracked.extend(paths);
        if !git(&checkout, &untracked, false, stop)?.is_empty() {
            return Err(
                "Local source has uncommitted files. Commit and verify, then press r.".into(),
            );
        }
        Ok(Self {
            root: root.to_path_buf(),
            checkout,
            slug: run["slug"]
                .as_str()
                .ok_or("Local run locator is unknown.")?
                .into(),
            id: run["run_id"]
                .as_str()
                .ok_or("Local run identity is unknown.")?
                .into(),
            head: head.into(),
            receipt: receipt.clone(),
            seed: run["seed_contract"].clone(),
            goal: sdlc::goal(run),
        })
    }

    /// The exact checked commit, never a newer checkout HEAD or inferred branch.
    pub(super) fn head(&self) -> &str {
        &self.head
    }

    /// The source-owned checkout, shown as local result provenance.
    pub(super) fn checkout(&self) -> &Path {
        &self.checkout
    }

    /// Read the committed blob only; a working-tree edit or symlink cannot supply a command.
    pub(super) fn declaration(&self, stop: &AtomicBool) -> Result<Option<Spec>, String> {
        let found = git(
            &self.checkout,
            &[
                "ls-tree",
                "--name-only",
                &self.head,
                "--",
                "tinkery.preview",
            ],
            false,
            stop,
        )?;
        if found.trim().is_empty() {
            return Ok(None);
        }
        let blob = git(
            &self.checkout,
            &["show", &format!("{}:tinkery.preview", self.head)],
            false,
            stop,
        )?;
        Spec::parse(&blob).map(Some)
    }

    fn recheck(&self, stop: &AtomicBool) -> Result<(), String> {
        let output = sdlc::status(
            &self.root,
            &["status".into(), self.slug.clone(), "--json".into()],
            stop,
            DEADLINE,
        )?;
        let value = sdlc::envelope(&output)?;
        if Self::read(&self.root, &value["run"], stop)? != *self {
            return Err(
                "Local result changed since reading. Press r before running its preview.".into(),
            );
        }
        Ok(())
    }
}

/// AGENTS.md differs from the checked commit only by tink-sdlc's generated rules block, which the
/// worker launcher writes and tink-sdlc excludes from the verified candidate.
fn generated_rules_only(checkout: &Path, head: &str, stop: &AtomicBool) -> bool {
    let (Ok(committed), Ok(current)) = (
        git(
            checkout,
            &["show", &format!("{head}:AGENTS.md")],
            false,
            stop,
        ),
        std::fs::read(checkout.join("AGENTS.md")),
    ) else {
        return false;
    };
    sdlc::without_generated_rules(committed.as_bytes()) == sdlc::without_generated_rules(&current)
}

/// An explicit PR result or a current checked local result; never a guessed repository.
#[derive(Clone, Debug)]
pub(super) enum Source {
    GitHub { url: String, head: String },
    Local(Local),
}

impl Source {
    fn head(&self) -> &str {
        match self {
            Self::GitHub { head, .. } => head,
            Self::Local(local) => local.head(),
        }
    }
}

/// A declared shell line and loopback-only HTTP address from the exact result commit.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct Spec {
    command: String,
    url: String,
}

impl Spec {
    fn parse(source: &str) -> Result<Self, String> {
        let spec: Self = serde_json::from_str(source).map_err(|_| {
            "Invalid tinkery.preview; declare one JSON object with command and url.".to_owned()
        })?;
        if spec.command.trim().is_empty()
            || spec.command.len() > 4096
            || spec.command.chars().any(char::is_control)
        {
            return Err("Invalid preview command; use one nonempty shell line.".into());
        }
        spec.address()?;
        Ok(spec)
    }

    fn address(&self) -> Result<(SocketAddr, &str), String> {
        let invalid = || "Preview URL must be http://127.0.0.1:PORT/path (local only).".to_owned();
        let rest = self
            .url
            .strip_prefix("http://127.0.0.1:")
            .ok_or_else(invalid)?;
        if self
            .url
            .chars()
            .any(|c| c.is_control() || c.is_whitespace())
            || rest.contains(['#', '?', '@'])
        {
            return Err(invalid());
        }
        let (port, path) = rest
            .split_once('/')
            .map_or((rest, "/"), |(port, _)| (port, &rest[port.len()..]));
        let port: u16 = port.parse().map_err(|_| invalid())?;
        if port == 0 {
            return Err(invalid());
        }
        Ok((SocketAddr::from(([127, 0, 0, 1], port)), path))
    }

    /// Exact command and full commit disclosure, shown before v can execute it.
    pub(super) fn disclosure(&self, head: &str) -> String {
        format!(
            "v run {:?} at {head}\nLocal preview: {}",
            self.command, self.url
        )
    }

    /// The exact command and address on the review, before v can execute it; the commit is
    /// checked at launch and shown in details.
    pub(super) fn offer(&self) -> String {
        format!("v runs {:?}, then opens {}", self.command, self.url)
    }
}

/// Read only the declared file at this snapshot's commit; absence is not inferred from scripts.
pub(super) fn declaration(
    root: &Path,
    url: &str,
    head: &str,
    stop: &AtomicBool,
) -> Result<Option<Spec>, String> {
    let repo = repository(url)?;
    let mut command = Command::new("gh");
    command.args([
        "api",
        "-H",
        "Accept: application/vnd.github.raw+json",
        &format!("repos/{repo}/contents/tinkery.preview?ref={head}"),
    ]);
    match sdlc::execute(root, command, stop, DEADLINE) {
        Ok(source) => Spec::parse(&source).map(Some),
        Err(why) if why.contains("HTTP 404") => Ok(None),
        Err(_) => {
            Err("Couldn't read tinkery.preview at this commit. Check gh access and press r.".into())
        }
    }
}

fn repository(url: &str) -> Result<&str, String> {
    url.strip_prefix("https://github.com/")
        .and_then(|s| s.split_once("/pull/").map(|(repo, _)| repo))
        .ok_or_else(|| "Result repository unknown; press r.".into())
}

/// Own the preview through readiness and browser navigation; dropping stops and reaps it.
pub(super) struct Preview {
    _worker: Read<()>,
    status: Arc<Mutex<String>>,
}

impl Preview {
    /// Start only after the explicit v action, using the already-disclosed declaration.
    pub(super) fn start(source: Source, spec: Spec) -> Self {
        let status = Arc::new(Mutex::new(
            "Preparing temporary exact-commit checkout… Esc stops.".into(),
        ));
        let progress = status.clone();
        let worker = Read::task(move |stop| {
            if let Err(why) = run(&source, &spec, stop, &progress) {
                *progress.lock().unwrap() =
                    format!("Preview stopped: {why} Press Esc, then v to retry.");
            }
            Ok(())
        });
        Self {
            _worker: worker,
            status,
        }
    }

    /// Current lifecycle state only; never claims the app was accepted or inspected.
    pub(super) fn status(&self) -> String {
        self.status.lock().unwrap().clone()
    }
}

struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        // SAFETY: this child owns a new process group; no operator process is targeted.
        unsafe {
            libc::kill(-(self.0.id() as libc::pid_t), libc::SIGKILL);
        }
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn git(root: &Path, args: &[&str], github: bool, stop: &AtomicBool) -> Result<String, String> {
    let mut command = Command::new("git");
    command.args([
        "-c",
        "core.hooksPath=/dev/null",
        "-c",
        "core.fileMode=true",
        "-c",
        "credential.helper=",
    ]);
    if github {
        command.args(["-c", "credential.helper=!gh auth git-credential"]);
    }
    command
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0");
    for key in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_COMMON_DIR",
        "GIT_CONFIG_COUNT",
    ] {
        command.env_remove(key);
    }
    sdlc::execute(root, command, stop, DEADLINE)
}

fn run(
    source: &Source,
    spec: &Spec,
    stop: &AtomicBool,
    progress: &Mutex<String>,
) -> Result<(), String> {
    if let Source::Local(local) = source {
        local.recheck(stop)?;
    }
    let temp = tempfile::Builder::new()
        .prefix("tinkery-TEST-preview-")
        .tempdir()
        .map_err(|e| e.to_string())?;
    let root = temp.path();
    let (repository, github) = match source {
        Source::GitHub { url, .. } => {
            (format!("https://github.com/{}.git", repository(url)?), true)
        }
        Source::Local(local) => (local.checkout.to_string_lossy().into_owned(), false),
    };
    let head = source.head();
    git(root, &["init", "--quiet"], github, stop)?;
    git(
        root,
        &["fetch", "--quiet", "--depth=1", &repository, head],
        github,
        stop,
    )?;
    git(
        root,
        &["checkout", "--quiet", "--detach", "FETCH_HEAD"],
        github,
        stop,
    )?;
    if git(root, &["rev-parse", "HEAD"], github, stop)?.trim() != head {
        return Err("Checkout commit disagrees with the displayed result.".into());
    }
    // Read the blob, not a working-tree symlink or checkout-filter output.
    let checked = Spec::parse(&git(root, &["show", "HEAD:tinkery.preview"], github, stop)?)?;
    if &checked != spec {
        return Err("Preview declaration changed; press r before running.".into());
    }
    if let Source::Local(local) = source {
        local.recheck(stop)?;
    }
    let (address, path) = spec.address()?;
    if TcpStream::connect_timeout(&address, Duration::from_millis(100)).is_ok() {
        return Err("Preview port is already in use; stop that server before retrying.".into());
    }
    let home = root.join("TEST-home");
    std::fs::create_dir(&home).map_err(|e| e.to_string())?;
    if stop.load(Ordering::SeqCst) {
        return Err("Cancelled.".into());
    }
    let child = Command::new("sh")
        .args(["-c", &spec.command])
        .current_dir(root)
        .env("HOME", &home)
        .env("XDG_CACHE_HOME", home.join("cache"))
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("XDG_DATA_HOME", home.join("data"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .map_err(|e| format!("Couldn't start declared command: {e}"))?;
    let mut process = Process(child);
    *progress.lock().unwrap() = format!("Waiting for {}… Esc stops.", spec.url);
    let until = Instant::now() + DEADLINE;
    while !stop.load(Ordering::SeqCst) {
        if let Some(status) = process.0.try_wait().map_err(|e| e.to_string())? {
            return Err(format!(
                "Declared command exited ({status}); check it externally."
            ));
        }
        if answers(address, path, stop, until) {
            break;
        }
        if Instant::now() >= until {
            return Err(format!(
                "{} did not answer within 30 seconds. The command must listen on 127.0.0.1; a server on localhost may listen only on ::1.",
                spec.url
            ));
        }
        thread::sleep(Duration::from_millis(50));
    }
    if stop.load(Ordering::SeqCst) {
        return Ok(());
    }
    let mut opener = Command::new(if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    });
    opener.arg(&spec.url);
    sdlc::execute(root, opener, stop, DEADLINE)
        .map_err(|_| "Couldn't open browser; check the default opener.".to_owned())?;
    *progress.lock().unwrap() = format!("Local URL answered; sent {} to browser.", spec.url);
    while !stop.load(Ordering::SeqCst) {
        if let Some(status) = process.0.try_wait().map_err(|e| e.to_string())? {
            return Err(format!("Declared command exited ({status})."));
        }
        thread::sleep(Duration::from_millis(20));
    }
    Ok(())
}

// Keep one slow response alive across short cancellation polls instead of abandoning it.
fn answers(address: SocketAddr, path: &str, stop: &AtomicBool, deadline: Instant) -> bool {
    let until = deadline.min(Instant::now() + Duration::from_secs(2));
    let Some(left) = until.checked_duration_since(Instant::now()) else {
        return false;
    };
    if stop.load(Ordering::SeqCst) || left.is_zero() {
        return false;
    }
    let Ok(mut stream) = TcpStream::connect_timeout(&address, left.min(Duration::from_millis(500)))
    else {
        return false;
    };
    let poll = Duration::from_millis(100);
    let left = until.saturating_duration_since(Instant::now());
    if left.is_zero() || stream.set_write_timeout(Some(left.min(poll))).is_err() {
        return false;
    }
    if write!(stream, "GET {path} HTTP/1.0\r\nHost: {address}\r\n\r\n").is_err() {
        return false;
    }
    let mut response = Vec::new();
    let mut bytes = [0; 128];
    while !stop.load(Ordering::SeqCst) && response.len() < 1024 {
        let left = until.saturating_duration_since(Instant::now());
        if left.is_zero() || stream.set_read_timeout(Some(left.min(poll))).is_err() {
            return false;
        }
        let capacity = bytes.len().min(1024 - response.len());
        match stream.read(&mut bytes[..capacity]) {
            Ok(0) => return false,
            Ok(n) => {
                response.extend_from_slice(&bytes[..n]);
                if let Some(end) = response.iter().position(|b| *b == b'\n') {
                    return Instant::now() < until
                        && !stop.load(Ordering::SeqCst)
                        && successful_status(&response[..=end]);
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::TimedOut
                        | std::io::ErrorKind::WouldBlock
                        | std::io::ErrorKind::Interrupted
                ) => {}
            Err(_) => return false,
        }
    }
    false
}

fn successful_status(line: &[u8]) -> bool {
    let Some(line) = line.strip_suffix(b"\r\n") else {
        return false;
    };
    let Ok(line) = std::str::from_utf8(line) else {
        return false;
    };
    let mut words = line.split_ascii_whitespace();
    matches!(words.next(), Some("HTTP/1.0" | "HTTP/1.1"))
        && words.next().is_some_and(|status| {
            status.len() == 3
                && status.starts_with('2')
                && status.bytes().all(|b| b.is_ascii_digit())
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    fn probe(address: SocketAddr) -> bool {
        answers(
            address,
            "/",
            &AtomicBool::new(false),
            Instant::now() + DEADLINE,
        )
    }

    fn receive_request(listener: TcpListener) -> Option<TcpStream> {
        listener.set_nonblocking(true).unwrap();
        let until = Instant::now() + Duration::from_secs(3);
        let (mut stream, _) = loop {
            match listener.accept() {
                Ok(connection) => break connection,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if Instant::now() >= until {
                        return None;
                    }
                    thread::sleep(Duration::from_millis(5));
                }
                Err(error) => panic!("test server couldn't accept: {error}"),
            }
        };
        stream.set_nonblocking(false).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let _ = stream.read(&mut [0; 2048]);
        Some(stream)
    }

    fn response(chunks: Vec<(u64, &'static [u8])>) -> (SocketAddr, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let worker = thread::spawn(move || {
            let Some(mut stream) = receive_request(listener) else {
                return;
            };
            for (delay, bytes) in chunks {
                thread::sleep(Duration::from_millis(delay));
                if stream.write_all(bytes).is_err() {
                    break;
                }
            }
        });
        (address, worker)
    }

    #[test]
    fn readiness_accepts_a_loaded_servers_delayed_status() {
        let (address, server) = response(vec![(250, b"HTTP/1.0 200 OK\r\n\r\n")]);
        let answered = probe(address);
        server.join().unwrap();
        assert!(answered, "a healthy server taking 250 ms was abandoned");
    }

    #[test]
    fn readiness_accepts_a_status_line_fragmented_across_read_timeouts() {
        let (address, server) = response(vec![
            (0, b"H"),
            (125, b"TTP/1.1 "),
            (125, b"20"),
            (125, b"0 OK\r\n\r\n"),
        ]);
        let answered = probe(address);
        server.join().unwrap();
        assert!(
            answered,
            "a partial TCP read was mistaken for a complete HTTP status"
        );
    }

    #[test]
    fn readiness_never_accepts_a_success_after_the_overall_deadline() {
        let (address, server) = response(vec![(250, b"HTTP/1.0 200 OK\r\n\r\n")]);
        let answered = answers(
            address,
            "/",
            &AtomicBool::new(false),
            Instant::now() + Duration::from_millis(50),
        );
        server.join().unwrap();
        assert!(
            !answered,
            "the overall deadline was replaced by a fresh attempt budget"
        );
    }

    #[test]
    fn readiness_cancels_an_inflight_silent_response_without_waiting_for_the_attempt_budget() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (request, requested) = std::sync::mpsc::channel();
        let (release, released) = std::sync::mpsc::channel();
        let server = thread::spawn(move || {
            let Some(_stream) = receive_request(listener) else {
                return;
            };
            request.send(()).unwrap();
            let _ = released.recv_timeout(Duration::from_secs(3));
        });
        let stop = Arc::new(AtomicBool::new(false));
        let cancelled = stop.clone();
        let (send, result) = std::sync::mpsc::channel();
        let worker = thread::spawn(move || {
            send.send(answers(address, "/", &cancelled, Instant::now() + DEADLINE))
                .unwrap();
        });
        requested.recv_timeout(Duration::from_secs(3)).unwrap();
        stop.store(true, Ordering::SeqCst);
        let outcome = result.recv_timeout(Duration::from_secs(1));
        let _ = release.send(());
        worker.join().unwrap();
        server.join().unwrap();
        assert!(!outcome.expect("cancellation waited for the two-second attempt budget"));
    }

    #[test]
    fn readiness_requires_a_complete_valid_success_status() {
        for line in [b"HTTP/1.0 200 OK\r\n".as_slice(), b"HTTP/1.1 299 Ready\r\n"] {
            assert!(successful_status(line));
        }
        for line in [
            b"HTTP/1.1 200".as_slice(),
            b"HTTP/1.1 2000 Bad\r\n",
            b"HTTP/1.1 503 Unavailable\r\n",
            b"HTTP/1.9 200 Bad\r\n",
            b"not HTTP 200\r\n",
        ] {
            assert!(!successful_status(line), "{line:?}");
        }
    }

    #[test]
    fn declarations_refuse_public_or_ambiguous_urls_and_empty_commands() {
        for url in [
            "https://example.com",
            "http://localhost:5173",
            "http://127.0.0.1:0/",
            "http://127.0.0.1:12@evil/",
            "http://127.0.0.1:80/\r\nx",
        ] {
            assert!(
                Spec::parse(&serde_json::json!({"command":"TEST preview", "url":url}).to_string())
                    .is_err(),
                "{url}"
            );
        }
        assert!(Spec::parse(r#"{"command":" ","url":"http://127.0.0.1:5173/"}"#).is_err());
        let spec =
            Spec::parse(r#"{"command":"TEST preview","url":"http://127.0.0.1:5173/"}"#).unwrap();
        assert!(spec.disclosure(&"a".repeat(40)).contains("TEST preview"));
    }
}
