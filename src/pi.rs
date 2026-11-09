//! The one place Tinkery starts a model host, Pi or Claude Code: a bounded, cancellable process
//! per request, and the call budget every request is counted against.
//!
//! A request runs its host with no session, extensions, plugins, MCP servers, skills or context
//! files, so the model sees only the prompt, the input and, when a caller asks for them, read-only
//! tools (Pi's `read`, `grep`, `find`, `ls`; Claude Code's `Read`, `Grep`, `Glob`): it can read
//! files inside the folder the host starts in, never outside it, and runs no commands. Without
//! tools, Claude Code starts in an empty folder. Claude Code gets a clean environment, so a
//! parent session's settings never leak in, and Tinkery reads no credentials of either host.
//! Input is capped at 128 KiB and replies at 32 KiB. Every completed or cancelled request stops its process
//! group and reaps the direct child; descendants that leave that group are outside this
//! ownership. Nothing retries on its own.
//! Callers get text back, or one line saying why not. Which phase asks is the caller's business; this module names none.

use std::{
    io::{Read, Write},
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};

const FOLDER_GUARD: &str = include_str!("../pi/folder.mjs");

/// Maximum encoded request bytes sent to a host.
pub const REQUEST_LIMIT: usize = 128 * 1024;
/// Maximum reply or diagnostic bytes accepted from a host.
pub const LIMIT: usize = 32 * 1024;

/// The thinking levels Pi names, in the order a panel cycles them.
const PI_THINKING: [&str; 7] = ["off", "minimal", "low", "medium", "high", "xhigh", "max"];
/// The effort levels Claude Code names. It only warns about others, so Tinkery refuses them.
const CLAUDE_EFFORT: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];
/// The Claude Code models a panel offers. A flag may name any `claude-` id.
const CLAUDE_MODELS: [&str; 3] = ["claude-sonnet-5-5", "claude-opus-5-5", "claude-haiku-5-5"];
/// What Claude Code inherits from Tinkery's environment; nothing else, so a parent session's
/// settings, endpoints and keys never reach it.
const CLAUDE_ENV: [&str; 6] = ["HOME", "USER", "PATH", "TERM", "LANG", "TMPDIR"];

/// Which program answers a request.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Harness {
    /// Pi, with an explicit `provider/model-id`.
    #[default]
    Pi,
    /// Claude Code (`claude -p`) on its own login.
    ClaudeCode,
}

impl Harness {
    /// Both harnesses, in the order a panel cycles them.
    pub const ALL: [Harness; 2] = [Harness::Pi, Harness::ClaudeCode];

    /// `pi` or `claude-code`, as flags and the saved setup spell it.
    pub fn parse(name: &str) -> Result<Self, String> {
        match name {
            "pi" => Ok(Self::Pi),
            "claude-code" => Ok(Self::ClaudeCode),
            _ => Err("Use harness pi|claude-code.".into()),
        }
    }

    /// The name people read: `Pi` or `Claude Code`.
    pub fn label(self) -> &'static str {
        match self {
            Self::Pi => "Pi",
            Self::ClaudeCode => "Claude Code",
        }
    }

    /// The models a panel offers to pick from; empty when the model is typed.
    pub fn models(self) -> &'static [&'static str] {
        match self {
            Self::Pi => &[],
            Self::ClaudeCode => &CLAUDE_MODELS,
        }
    }

    /// The thinking levels this harness accepts, in cycling order.
    pub fn thinking(self) -> &'static [&'static str] {
        match self {
            Self::Pi => &PI_THINKING,
            Self::ClaudeCode => &CLAUDE_EFFORT,
        }
    }

    /// Refuses a thinking level this harness doesn't name.
    pub fn check_thinking(self, level: &str) -> Result<(), String> {
        if self.thinking().contains(&level) {
            Ok(())
        } else {
            Err(format!(
                "Use thinking {} with {}.",
                self.thinking().join("|"),
                self.label()
            ))
        }
    }

    /// What a model must look like here, in the words refusals use.
    pub fn model_shape(self) -> &'static str {
        match self {
            Self::Pi => "provider/model-id",
            Self::ClaudeCode => "a Claude Code model id such as claude-sonnet-5-5",
        }
    }

    /// Refuses a model this harness can't be sure to run as named: Pi needs a provider and an id,
    /// Claude Code a full `claude-` id, never an alias that changes meaning. Neither takes spaces.
    pub fn check_model(self, model: &str) -> Result<(), String> {
        let fits = match self {
            Self::Pi => explicit_model(model),
            Self::ClaudeCode => {
                model.len() > "claude-".len()
                    && model.starts_with("claude-")
                    && !model.chars().any(char::is_whitespace)
            }
        };
        if fits {
            Ok(())
        } else {
            Err(format!(
                "Use an explicit model: {}, with no spaces.",
                self.model_shape()
            ))
        }
    }

    /// The executable started when no other is named.
    pub fn program(self) -> &'static str {
        match self {
            Self::Pi => "pi",
            Self::ClaudeCode => "claude",
        }
    }
}

/// Which kind of request a call is, for a [`CallBudget`]'s record.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Request {
    /// The shaping call that reads the person's words.
    Shaping,
    /// The review that names decisions the reading settled silently.
    DecisionReview,
    /// One step of the seed interview: investigate, then draft the next question or the seed.
    SeedStep,
}

/// One request a [`CallBudget`] saw: started (then `reply` or `failure`) or refused unstarted.
#[derive(Clone, Debug, Serialize)]
pub struct Attempt {
    /// Which request it was.
    pub kind: Request,
    /// `started`, `reply`, `failure` or `refused`.
    pub outcome: &'static str,
    /// Why it failed or was refused.
    pub error: Option<String>,
}

/// A hard limit on drafting attempts, shared by every host that holds it. An attempt is counted
/// before preparing or starting the harness, whatever happens after; once the limit is reached the next request is
/// refused unstarted. The count never resets, so a new session on the same budget can't spend
/// it twice. Only an explicit [`CallBudget::add`] raises the limit, for this launch alone.
#[derive(Debug)]
pub struct CallBudget {
    limit: AtomicUsize,
    attempts: std::sync::Mutex<Vec<Attempt>>,
}

impl CallBudget {
    /// A budget allowing at most `limit` requests. Zero refuses every request.
    pub fn new(limit: usize) -> Arc<Self> {
        Arc::new(Self {
            limit: AtomicUsize::new(limit),
            attempts: Default::default(),
        })
    }
    /// The limit now.
    pub fn limit(&self) -> usize {
        self.limit.load(Ordering::Relaxed)
    }
    /// Allows `calls` more requests this launch. Starts nothing and saves nothing.
    pub fn add(&self, calls: usize) {
        self.limit.fetch_add(calls, Ordering::Relaxed);
    }
    /// Every request seen so far, in order, refusals included.
    pub fn attempts(&self) -> Vec<Attempt> {
        self.attempts.lock().unwrap().clone()
    }
    /// Requests that counted against the limit.
    pub fn used(&self) -> usize {
        self.attempts()
            .iter()
            .filter(|a| a.outcome != "refused")
            .count()
    }
    /// Requests still allowed.
    pub fn remaining(&self) -> usize {
        self.limit().saturating_sub(self.used())
    }
    /// The allowance in the words both phases show: requests left out of the limit.
    pub fn left(&self) -> String {
        format!(
            "Drafting runs left this launch: {} of {}.",
            self.remaining(),
            self.limit()
        )
    }
    /// Counts one `kind` request, or refuses it when the limit is reached. Returns the slot that
    /// [`CallBudget::finish`] fills in.
    pub fn reserve(&self, kind: Request) -> Result<usize, String> {
        let mut attempts = self.attempts.lock().unwrap();
        let used = attempts.iter().filter(|a| a.outcome != "refused").count();
        let limit = self.limit();
        if used >= limit {
            let error = format!(
                "Drafting run limit of {limit} reached; the next drafting run was refused before it started. Add runs to keep going."
            );
            attempts.push(Attempt {
                kind,
                outcome: "refused",
                error: Some(error.clone()),
            });
            return Err(error);
        }
        attempts.push(Attempt {
            kind,
            outcome: "started",
            error: None,
        });
        Ok(attempts.len() - 1)
    }
    /// Records how a reserved request ended.
    pub fn finish<T>(&self, slot: usize, result: &Result<T, String>) {
        if let Some(attempt) = self.attempts.lock().unwrap().get_mut(slot) {
            attempt.outcome = if result.is_ok() { "reply" } else { "failure" };
            attempt.error = result.as_ref().err().cloned();
        }
    }
}

/// How to start a host: which one, the executable, an explicit model, a thinking level (empty
/// leaves the host's default), whether the model may read files, where it starts, and how long it
/// may take.
#[derive(Clone, Debug)]
pub struct Call {
    /// Which host the executable is.
    pub harness: Harness,
    /// The executable.
    pub program: PathBuf,
    /// A model the harness accepts; never defaulted.
    pub model: String,
    /// One of the harness's thinking levels, or empty for its own default.
    pub thinking: String,
    /// `true` gives the model read-only tools; `false` gives it none.
    pub read_only_tools: bool,
    /// The folder the host starts in, the only one its tools can read; `None` keeps Tinkery's.
    /// Claude Code without tools always starts in an empty folder instead.
    pub folder: Option<PathBuf>,
    /// The request stops after this long.
    pub timeout: Duration,
}

/// A prompt file as the model reads it. Notes (lines starting with `#`) are dropped, lines in a
/// paragraph are joined with one space, and each blank line becomes one newline.
pub fn assemble(file: &str) -> String {
    let mut prompt = String::new();
    let mut in_paragraph = false;
    for line in file.lines().filter(|l| !l.starts_with('#')) {
        if line.is_empty() {
            if !prompt.is_empty() {
                prompt.push('\n');
            }
            in_paragraph = false;
        } else {
            if in_paragraph {
                prompt.push(' ');
            }
            prompt.push_str(line);
            in_paragraph = true;
        }
    }
    prompt
}

/// Whether `model` names a provider and a model id with no spaces, as Pi needs.
fn explicit_model(model: &str) -> bool {
    model.split_once('/').is_some_and(|(provider, id)| {
        !provider.is_empty() && !id.is_empty() && !model.chars().any(char::is_whitespace)
    })
}

/// The models Pi lists for its configured providers, as `provider/model-id`, in Pi's order.
/// Runs `pi --list-models --offline`, which reads Pi's registry and makes no model request;
/// stops it after 10 seconds. Models Pi's extensions add may be missing; type those.
pub fn pi_models(program: &Path) -> Result<Vec<String>, String> {
    let mut command = Command::new(program);
    command
        .args(["--list-models", "--offline"])
        .env_remove("PI_SESSION_ID")
        .env_remove("PI_SESSION_FILE")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0);
    let mut child = start(&mut command).map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => format!(
            "Pi isn't installed: no `{}` on PATH. Install Pi and log in.",
            program.display()
        ),
        _ => format!("Cannot list Pi's models: {error}"),
    })?;
    let overflow = Arc::new(AtomicBool::new(false));
    let stdout = read_pipe(child.stdout.take().unwrap(), LIMIT, "Pi", overflow);
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) if stdout.is_finished() => break Ok(status),
            Err(error) => break Err(format!("Cannot list Pi's models: {error}")),
            _ if started.elapsed() >= Duration::from_secs(10) => {
                break Err("Pi took too long to list its models.".into());
            }
            _ => thread::sleep(Duration::from_millis(20)),
        }
    };
    stop(&mut child);
    let status = status?;
    let output = stdout
        .join()
        .map_err(|_| "Pi output reader failed.".to_owned())??;
    if !status.success() {
        return Err(format!("Pi couldn't list its models ({status})."));
    }
    let models = listed_models(&String::from_utf8_lossy(&output));
    if models.is_empty() {
        return Err("Pi listed no models.".into());
    }
    Ok(models)
}

/// The `provider/model-id` of each row under Pi's `provider  model  …` header.
fn listed_models(table: &str) -> Vec<String> {
    table
        .lines()
        .skip_while(|line| {
            let mut fields = line.split_whitespace();
            (fields.next(), fields.next()) != (Some("provider"), Some("model"))
        })
        .skip(1)
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            Some(format!("{}/{}", fields.next()?, fields.next()?))
        })
        .filter(|model| explicit_model(model))
        .collect()
}

impl Call {
    /// Run the host once with `prompt` as its system prompt and `input` on stdin. Refuses oversized
    /// input before starting; counts against the required `budget` before anything starts. Blocks
    /// until the host answers, fails, times out or `cancelled` is set; run it off the paint path.
    ///
    /// A call cannot omit its budget:
    /// ```compile_fail,E0308
    /// use std::sync::atomic::AtomicBool;
    /// use tinkery::pi::{Call, Request};
    /// fn unbudgeted(call: Call) {
    ///     let _ = call.complete(None, Request::Shaping, vec![], "TEST".into(), &AtomicBool::new(false));
    /// }
    /// ```
    pub fn complete(
        &self,
        budget: &CallBudget,
        kind: Request,
        input: Vec<u8>,
        prompt: String,
        cancelled: &AtomicBool,
    ) -> Result<String, String> {
        if input.len() > REQUEST_LIMIT {
            return Err(
                "This is too much to send at once. Nothing was sent; your words are kept. Start a smaller goal, keeping this one as a reference.".into(),
            );
        }
        if cancelled.load(Ordering::Relaxed) {
            return Err("Request cancelled.".into());
        }
        let slot = budget.reserve(kind)?;
        let result = self.run(input, prompt, cancelled);
        budget.finish(slot, &result);
        result
    }

    fn run(
        &self,
        input: Vec<u8>,
        prompt: String,
        cancelled: &AtomicBool,
    ) -> Result<String, String> {
        let host = self.harness.label();
        let (mut command, _held) = match self.harness {
            Harness::Pi => self.pi()?,
            Harness::ClaudeCode => self.claude()?,
        };
        command
            .arg("--model")
            .arg(&self.model)
            .arg("--system-prompt")
            .arg(prompt)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0);
        let mut child = start(&mut command).map_err(|error| match self.harness {
            Harness::ClaudeCode if error.kind() == std::io::ErrorKind::NotFound => {
                "Cannot start Claude Code: `claude` is not installed or not on PATH.".to_owned()
            }
            _ => format!("Cannot start {host}: {error}"),
        })?;
        let mut stdin = child.stdin.take().unwrap();
        let writer = thread::spawn(move || stdin.write_all(&input));
        let overflow = Arc::new(AtomicBool::new(false));
        // Claude Code wraps the reply in JSON, escapes and all; its text is checked after.
        let cap = match self.harness {
            Harness::Pi => LIMIT,
            Harness::ClaudeCode => 4 * LIMIT,
        };
        let stdout = read_pipe(child.stdout.take().unwrap(), cap, host, overflow.clone());
        let stderr = read_pipe(child.stderr.take().unwrap(), LIMIT, host, overflow.clone());
        let started = Instant::now();
        let mut status = None;
        let result = loop {
            let failure = if cancelled.load(Ordering::Relaxed) {
                Some("Request cancelled.".to_owned())
            } else if overflow.load(Ordering::Relaxed) {
                Some(format!("{host} output exceeded 32 KiB."))
            } else if started.elapsed() >= self.timeout {
                Some(format!(
                    "{host} timed out after {} seconds.",
                    self.timeout.as_secs()
                ))
            } else {
                None
            };
            if let Some(error) = failure {
                break Err(error);
            }
            match child.try_wait() {
                Ok(Some(exited)) => status = Some(exited),
                Ok(None) => {}
                Err(error) => {
                    break Err(format!("Cannot wait for {host}: {error}"));
                }
            }
            if let Some(status) = status
                && writer.is_finished()
                && stdout.is_finished()
                && stderr.is_finished()
            {
                break Ok(status);
            }
            thread::sleep(Duration::from_millis(20));
        };
        stop(&mut child);
        // Unfinished pipe workers must not extend a failed call's deadline.
        let status = result?;
        let input_result = writer
            .join()
            .map_err(|_| format!("{host} input writer failed."))?;
        let output = stdout
            .join()
            .map_err(|_| format!("{host} output reader failed."))??;
        let diagnostics = stderr
            .join()
            .map_err(|_| format!("{host} error reader failed."))??;
        let diagnostics = String::from_utf8_lossy(&diagnostics);
        let text = match self.harness {
            Harness::Pi => pi_reply(status, &output, &diagnostics)?,
            Harness::ClaudeCode => claude_reply(status, &output, &diagnostics, &self.model)?,
        };
        input_result.map_err(|error| format!("Cannot send the request to {host}: {error}"))?;
        if cancelled.load(Ordering::Relaxed) {
            return Err("Request cancelled.".into());
        }
        Ok(text)
    }

    /// Pi's command line, and the folder guard it must keep until Pi exits.
    fn pi(&self) -> Result<(Command, Held), String> {
        let mut command = Command::new(&self.program);
        command.args(["--print", "--no-session"]);
        let mut held: Held = None;
        if self.read_only_tools {
            let guard = tempfile::Builder::new()
                .prefix("tinkery-pi-folder-")
                .suffix(".mjs")
                .tempfile()
                .and_then(|mut file| file.write_all(FOLDER_GUARD.as_bytes()).map(|()| file))
                .map_err(|error| format!("Cannot prepare Pi's folder guard: {error}"))?;
            command.args(["--tools", "read,grep,find,ls", "--extension"]);
            command.arg(guard.path());
            held = Some(Box::new(guard));
        } else {
            command.arg("--no-tools");
        }
        command.args([
            "--no-extensions",
            "--no-mcp",
            "--no-skills",
            "--no-prompt-templates",
            "--no-themes",
            "--no-context-files",
            "--no-approve",
            "--offline",
        ]);
        if !self.thinking.is_empty() {
            command.arg("--thinking").arg(&self.thinking);
        }
        if let Some(folder) = &self.folder {
            command.current_dir(folder);
        }
        command
            .env_remove("PI_SESSION_ID")
            .env_remove("PI_SESSION_FILE");
        Ok((command, held))
    }

    /// Claude Code's command line, in a clean environment. Without tools it starts in an empty
    /// folder it must keep until Claude Code exits, so no project file can reach it. With tools,
    /// `dontAsk` refuses every read outside the folder it starts in.
    fn claude(&self) -> Result<(Command, Held), String> {
        let mut command = Command::new(&self.program);
        command.env_clear();
        for name in CLAUDE_ENV {
            if let Some(value) = std::env::var_os(name) {
                command.env(name, value);
            }
        }
        command.env("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", "1");
        command.args([
            "-p",
            "--output-format",
            "json",
            "--no-session-persistence",
            "--setting-sources",
            "",
            "--strict-mcp-config",
            "--mcp-config",
            r#"{"mcpServers":{}}"#,
            "--disable-slash-commands",
            "--permission-mode",
            "dontAsk",
            "--permission-prompts",
            "none",
            "--tools",
        ]);
        let mut held: Held = None;
        if self.read_only_tools {
            command.arg("Read,Grep,Glob");
            if let Some(folder) = &self.folder {
                command.current_dir(folder);
            }
        } else {
            command.arg("");
            let empty = tempfile::Builder::new()
                .prefix("tinkery-claude-empty-")
                .tempdir()
                .map_err(|error| format!("Cannot prepare an empty folder: {error}"))?;
            command.current_dir(empty.path());
            held = Some(Box::new(empty));
        }
        if !self.thinking.is_empty() {
            command.arg("--effort").arg(&self.thinking);
        }
        Ok((command, held))
    }
}

/// What a running host needs kept until it exits: Pi's folder guard or Claude Code's empty folder.
type Held = Option<Box<dyn std::any::Any>>;

/// Pi's reply text, or why there isn't one.
fn pi_reply(
    status: std::process::ExitStatus,
    output: &[u8],
    diagnostics: &str,
) -> Result<String, String> {
    if !status.success() {
        if diagnostics.contains("429") && diagnostics.contains("rate_limit") {
            return Err("Pi provider rate limit (429); retry later. No automatic retry.".into());
        }
        let lower = diagnostics.to_ascii_lowercase();
        if [
            "context_length_exceeded",
            "context window",
            "input too long",
            "context rejected",
            "context length",
            "input/context rejected",
        ]
        .iter()
        .any(|s| lower.contains(s))
        {
            return Err("Pi provider rejected the input/context size. No automatic retry.".into());
        }
        return Err(format!(
            "Pi exited with {status}; check the configured model and authentication."
        ));
    }
    let text = std::str::from_utf8(output).map_err(|_| "Pi returned invalid UTF-8.")?;
    Ok(text.to_owned())
}

/// Claude Code's reply text from its JSON result, or why there isn't one. A result whose only
/// model is `<synthetic>` is Claude Code's own error message, never a model's reply.
fn claude_reply(
    status: std::process::ExitStatus,
    output: &[u8],
    diagnostics: &str,
    model: &str,
) -> Result<String, String> {
    let json: Option<serde_json::Value> = serde_json::from_slice(output).ok();
    // `--verbose` would print every message as a list; the result is always the last one.
    let result = json.as_ref().and_then(|json| match json {
        serde_json::Value::Array(messages) => messages
            .iter()
            .rev()
            .find(|m| m["type"] == "result")
            .cloned(),
        other => Some(other.clone()),
    });
    let text = result
        .as_ref()
        .and_then(|r| r["result"].as_str())
        .unwrap_or_default();
    let synthetic = result
        .as_ref()
        .and_then(|r| r["modelUsage"].as_object())
        .is_some_and(|usage| !usage.is_empty() && usage.keys().all(|m| m == "<synthetic>"));
    let failed = !status.success()
        || result.is_none()
        || result.as_ref().is_some_and(|r| r["is_error"] != false)
        || synthetic;
    if !failed {
        if text.len() > LIMIT {
            return Err("Claude Code output exceeded 32 KiB.".into());
        }
        return Ok(text.to_owned());
    }
    // Without a JSON result, whatever Claude Code printed is all there is to show.
    let raw = String::from_utf8_lossy(output);
    let said = if result.is_some() { text } else { &raw };
    let words = format!("{said}\n{diagnostics}");
    let lower = words.to_ascii_lowercase();
    let why = if lower.contains("not logged in") || lower.contains("/login") {
        "Claude Code is not logged in; run `claude` and log in, then send again.".to_owned()
    } else if lower.contains("usage limit") || lower.contains("limit reached") {
        "Claude Code's usage limit is reached; send again after it resets.".to_owned()
    } else if lower.contains("model")
        && (lower.contains("not found") || lower.contains("invalid") || lower.contains("404"))
    {
        format!("Claude Code doesn't offer model {model}; choose another in the Model panel.")
    } else {
        let said = words.split_whitespace().collect::<Vec<_>>().join(" ");
        let said: String = said.chars().take(200).collect();
        if said.is_empty() {
            format!("Claude Code exited with {status} and said nothing.")
        } else {
            format!("Claude Code failed: {said}")
        }
    };
    Err(format!("{why} No automatic retry."))
}

/// Starts `command`, waiting up to a second while its executable is busy: one being replaced
/// (an update in progress, or a fake a test just wrote) can't start until its writer closes it.
fn start(command: &mut Command) -> std::io::Result<Child> {
    let until = Instant::now() + Duration::from_secs(1);
    loop {
        match command.spawn() {
            Err(error) if error.raw_os_error() == Some(libc::ETXTBSY) && Instant::now() < until => {
                thread::sleep(Duration::from_millis(10));
            }
            started => return started,
        }
    }
}

// Stop descendants even if the host already exited, then reap the immediate child.
fn stop(child: &mut Child) {
    // SAFETY: this group was created for the owned child with process_group(0).
    unsafe {
        libc::kill(-(child.id() as libc::pid_t), libc::SIGKILL);
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn read_pipe(
    reader: impl Read + Send + 'static,
    cap: usize,
    host: &'static str,
    overflow: Arc<AtomicBool>,
) -> JoinHandle<Result<Vec<u8>, String>> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        reader
            .take((cap + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|error| format!("Cannot read {host} output: {error}"))?;
        if bytes.len() > cap {
            overflow.store(true, Ordering::Relaxed);
            return Err(format!("{host} output exceeded 32 KiB."));
        }
        Ok(bytes)
    })
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::{os::unix::fs::PermissionsExt, path::Path};

    mod descendant {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/support/command_descendant.rs"
        ));
    }

    #[test]
    fn successful_requests_stop_descendants_and_preserve_results_and_budget() {
        completed_request(0);
    }

    #[test]
    fn failed_requests_stop_descendants_and_preserve_results_and_budget() {
        completed_request(7);
    }

    fn completed_request(exit: u8) {
        let fixture = descendant::Fixture::new(exit);
        let call = Call {
            harness: Harness::Pi,
            program: fixture.program(),
            model: "TEST/fake".into(),
            thinking: String::new(),
            read_only_tools: false,
            folder: Some(fixture.root().into()),
            timeout: Duration::from_secs(10),
        };
        let budget = CallBudget::new(1);
        let result = call.complete(
            &budget,
            Request::Shaping,
            b" TEST exact input\n ".to_vec(),
            "TEST instructions".into(),
            &AtomicBool::new(false),
        );
        fixture.assert_stopped();
        assert_eq!(fixture.input(), b" TEST exact input\n ");
        assert_eq!(budget.used(), 1);
        assert_eq!(budget.remaining(), 0);
        if exit == 0 {
            assert_eq!(result.unwrap(), " TEST reply café  \n");
            assert_eq!(budget.attempts()[0].outcome, "reply");
        } else {
            let error = result.unwrap_err();
            assert!(
                error.starts_with("Pi exited with exit status: 7"),
                "{error}"
            );
            assert!(
                error.ends_with("check the configured model and authentication."),
                "{error}"
            );
            assert_eq!(budget.attempts()[0].outcome, "failure");
            assert_eq!(budget.attempts()[0].error.as_deref(), Some(error.as_str()));
        }
    }

    #[test]
    fn oversized_input_keeps_words_and_offers_a_next_step_before_spending_budget() {
        let call = Call {
            harness: Harness::Pi,
            program: "/TEST-never-start-pi".into(),
            model: "TEST/fake".into(),
            thinking: String::new(),
            read_only_tools: true,
            folder: None,
            timeout: Duration::from_secs(1),
        };
        let budget = CallBudget::new(1);
        let words = vec![b'x'; REQUEST_LIMIT + 1];
        let error = call
            .complete(
                &budget,
                Request::SeedStep,
                words.clone(),
                "TEST instructions".into(),
                &AtomicBool::new(false),
            )
            .unwrap_err();
        assert_eq!(
            error,
            "This is too much to send at once. Nothing was sent; your words are kept. Start a smaller goal, keeping this one as a reference."
        );
        assert_eq!(words.len(), REQUEST_LIMIT + 1);
        assert!(words.iter().all(|b| *b == b'x'));
        assert!(budget.attempts().is_empty());
    }

    #[test]
    fn stopped_requests_kill_descendants_without_extending_the_deadline() {
        // Failure assertions must not leave this fixture's group running.
        struct Cleanup(std::path::PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                if let Ok(text) = std::fs::read_to_string(&self.0)
                    && let Ok(pid) = text.parse::<i32>()
                    && pid > 0
                {
                    // SAFETY: the fake atomically records its own group leader before forking.
                    unsafe { libc::kill(-pid, libc::SIGKILL) };
                }
            }
        }
        let mut failures = Vec::new();
        // Include the reproduced slow startup without doubling the deadline cases.
        let startup_ms = 400;
        for (harness, mode) in Harness::ALL.into_iter().flat_map(|harness| {
            ["timeout", "exited", "cancel", "overflow"].map(|mode| (harness, mode))
        }) {
            let directory = tempfile::Builder::new()
                .prefix("tinkery-TEST-pi-")
                .tempdir()
                .unwrap();
            let _cleanup = Cleanup(directory.path().join("TEST-group-pid"));
            let program = directory.path().join("pi");
            std::fs::write(
                &program,
                format!(
                    r#"#!/usr/bin/env python3
import os, pathlib, subprocess, sys, time
pathlib.Path('TEST-group-pid.tmp').write_text(str(os.getpid()))
pathlib.Path('TEST-group-pid.tmp').rename('TEST-group-pid')
time.sleep({startup_ms} / 1000)
child = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(10)'])
pathlib.Path('TEST-child-pid.tmp').write_text(str(child.pid))
pathlib.Path('TEST-child-pid.tmp').rename('TEST-child-pid')
if {mode:?} == 'exited':
    sys.exit(0)
if {mode:?} == 'overflow':
    print('x' * 140000, flush=True)
time.sleep(10)
"#
                ),
            )
            .unwrap();
            std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
            let call = Call {
                harness,
                program,
                model: "TEST/fake".into(),
                thinking: String::new(),
                // With tools, both start in the folder that the fake writes its PID file to.
                read_only_tools: true,
                folder: Some(directory.path().into()),
                timeout: Duration::from_secs(3),
            };
            let cancelled = Arc::new(AtomicBool::new(false));
            let ready = {
                let cancelled = cancelled.clone();
                let ready = directory.path().join("TEST-child-pid");
                thread::spawn(move || {
                    let until = Instant::now() + Duration::from_secs(2);
                    while !ready.exists() && Instant::now() < until {
                        thread::sleep(Duration::from_millis(10));
                    }
                    let observed = ready.exists().then(Instant::now);
                    if mode == "cancel" {
                        cancelled.store(true, Ordering::Relaxed);
                    }
                    observed
                })
            };
            let budget = CallBudget::new(1);
            let started = Instant::now();
            let result =
                call.complete(&budget, Request::Shaping, vec![], "TEST".into(), &cancelled);
            let returned = Instant::now();
            let elapsed = returned.duration_since(started);
            let ready = ready.join().unwrap().unwrap_or_else(|| {
                panic!("{harness:?} {mode} startup {startup_ms}ms: fake never published its descendant PID; result {result:?}; waited {elapsed:?}")
            });
            let pid: i32 = std::fs::read_to_string(directory.path().join("TEST-child-pid"))
                .unwrap_or_else(|error| panic!("{harness:?} {mode} startup {startup_ms}ms: cannot read ready descendant: {error}"))
                .parse()
                .unwrap();
            assert!(pid > 0);
            // An orphan may briefly remain a zombie until init reaps it. It cannot run.
            let running = || {
                let output = Command::new("ps")
                    .args(["-o", "stat=", "-p", &pid.to_string()])
                    .output()
                    .unwrap();
                let state = String::from_utf8_lossy(&output.stdout);
                !state.trim().is_empty() && !state.trim().starts_with('Z')
            };
            let until = Instant::now() + Duration::from_secs(1);
            while running() && Instant::now() < until {
                thread::sleep(Duration::from_millis(10));
            }
            let survived = running();
            if survived {
                // SAFETY: this PID belongs to the fake descendant, still alive above.
                unsafe {
                    libc::kill(pid, libc::SIGKILL);
                }
                failures.push(format!(
                    "{harness:?} {mode} startup {startup_ms}ms: descendant survived"
                ));
            }
            let response = returned.saturating_duration_since(ready);
            println!(
                "{harness:?} {mode} startup {startup_ms}ms: elapsed {elapsed:?}, ready response {response:?}, descendant {pid}, survived {survived}"
            );
            // Timeout/exited use observed fake readiness, not the start of process setup.
            let slack = Duration::from_millis(400);
            let (waited, origin) = if matches!(mode, "timeout" | "exited") {
                (response, "fake readiness")
            } else {
                (elapsed, "call start")
            };
            assert!(
                waited < call.timeout + slack,
                "{harness:?} {mode}: waited {waited:?} from {origin}"
            );
            if matches!(mode, "cancel" | "overflow") {
                assert!(
                    response < slack,
                    "{harness:?} {mode}: response waited {response:?}"
                );
            }
            let error = result.unwrap_err();
            let host = harness.label();
            let expected = match mode {
                "cancel" => "Request cancelled.".to_owned(),
                "overflow" => format!("{host} output exceeded 32 KiB."),
                _ => format!("{host} timed out"),
            };
            assert!(error.starts_with(&expected), "{harness:?} {mode}: {error}");
            assert_eq!(budget.used(), 1);
            assert_eq!(budget.attempts()[0].outcome, "failure");
        }
        assert!(failures.is_empty(), "{}", failures.join("; "));
    }

    /// A fake `claude` that records how it was started, then prints `reply` and exits `exit`.
    /// Its log path is baked in, because Claude Code gets a clean environment.
    struct FakeClaude {
        directory: tempfile::TempDir,
    }

    impl FakeClaude {
        fn new(reply: &str, exit: u8) -> Self {
            let directory = tempfile::Builder::new()
                .prefix("tinkery-TEST-claude-")
                .tempdir()
                .unwrap();
            let log = directory.path().join("TEST-started.json");
            let program = directory.path().join("claude");
            std::fs::write(
                &program,
                format!(
                    r#"#!/usr/bin/env python3
import json, os, sys
started = dict(argv=sys.argv[1:], cwd=os.getcwd(), files=sorted(os.listdir('.')),
               env=sorted(os.environ), stdin=sys.stdin.read())
open({log:?}, 'w').write(json.dumps(started))
sys.stdout.write({reply:?})
sys.exit({exit})
"#,
                    log = log.display().to_string(),
                ),
            )
            .unwrap();
            std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
            Self { directory }
        }

        fn call(&self, read_only_tools: bool) -> Call {
            Call {
                harness: Harness::ClaudeCode,
                program: self.directory.path().join("claude"),
                model: "claude-TEST".into(),
                thinking: "high".into(),
                read_only_tools,
                folder: Some(self.directory.path().into()),
                timeout: Duration::from_secs(10),
            }
        }

        fn complete(&self, read_only_tools: bool) -> Result<String, String> {
            self.call(read_only_tools).complete(
                &CallBudget::new(1),
                Request::Shaping,
                b"TEST words".to_vec(),
                "TEST prompt".into(),
                &AtomicBool::new(false),
            )
        }

        fn started(&self) -> serde_json::Value {
            let log = self.directory.path().join("TEST-started.json");
            serde_json::from_str(&std::fs::read_to_string(log).unwrap()).unwrap()
        }
    }

    fn reply(text: &str) -> String {
        serde_json::json!({"type": "result", "is_error": false, "result": text,
            "modelUsage": {"claude-TEST": {}}})
        .to_string()
    }

    fn argv(started: &serde_json::Value) -> Vec<String> {
        serde_json::from_value(started["argv"].clone()).unwrap()
    }

    fn after<'a>(argv: &'a [String], flag: &str) -> &'a str {
        let at = argv.iter().position(|a| a == flag).expect(flag);
        &argv[at + 1]
    }

    #[test]
    fn claude_code_without_tools_starts_empty_handed_in_an_empty_folder_and_a_clean_env() {
        let fake = FakeClaude::new(&reply(" TEST reply café \n"), 0);
        let budget = CallBudget::new(1);
        let result = fake.call(false).complete(
            &budget,
            Request::Shaping,
            b"TEST words".to_vec(),
            "TEST prompt".into(),
            &AtomicBool::new(false),
        );
        assert_eq!(result.unwrap(), " TEST reply café \n");
        assert_eq!(budget.used(), 1);
        let started = fake.started();
        let argv = argv(&started);
        assert_eq!(started["stdin"], "TEST words");
        assert_eq!(after(&argv, "--tools"), "", "no tools at all");
        assert_eq!(after(&argv, "--model"), "claude-TEST");
        assert_eq!(after(&argv, "--effort"), "high");
        assert_eq!(after(&argv, "--system-prompt"), "TEST prompt");
        assert_eq!(after(&argv, "--setting-sources"), "");
        assert_eq!(after(&argv, "--mcp-config"), r#"{"mcpServers":{}}"#);
        for flag in [
            "-p",
            "--no-session-persistence",
            "--strict-mcp-config",
            "--disable-slash-commands",
        ] {
            assert!(argv.contains(&flag.to_owned()), "{flag}: {argv:?}");
        }
        assert_eq!(after(&argv, "--output-format"), "json");
        let cwd = started["cwd"].as_str().unwrap();
        assert!(
            !cwd.starts_with(fake.directory.path().to_str().unwrap()),
            "the folder is never offered without tools: {cwd}"
        );
        assert_eq!(
            started["files"],
            serde_json::json!([]),
            "the folder is empty"
        );
        assert!(
            !Path::new(cwd).exists(),
            "the empty folder is removed after"
        );
        let env: Vec<String> = serde_json::from_value(started["env"].clone()).unwrap();
        for name in &env {
            assert!(
                CLAUDE_ENV.contains(&name.as_str())
                    || name == "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC"
                    // Python sets this one itself on some platforms.
                    || name == "LC_CTYPE"
                    || name == "__CF_USER_TEXT_ENCODING",
                "{name} leaked into Claude Code's environment"
            );
        }
        assert!(env.contains(&"CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC".to_owned()));
    }

    #[test]
    fn claude_code_with_tools_reads_only_where_it_starts_and_asks_nobody() {
        let fake = FakeClaude::new(&reply("TEST"), 0);
        fake.complete(true).unwrap();
        let started = fake.started();
        let argv = argv(&started);
        assert_eq!(after(&argv, "--tools"), "Read,Grep,Glob");
        assert_eq!(after(&argv, "--permission-mode"), "dontAsk");
        assert_eq!(after(&argv, "--permission-prompts"), "none");
        assert_eq!(
            Path::new(started["cwd"].as_str().unwrap())
                .canonicalize()
                .unwrap(),
            fake.directory.path().canonicalize().unwrap()
        );
    }

    #[test]
    fn claude_code_failures_read_plainly_and_never_pass_as_replies() {
        let error = |text: &str, exit| {
            let fake = FakeClaude::new(text, exit);
            fake.complete(false).unwrap_err()
        };
        let failed = |result: &str| {
            serde_json::json!({"type": "result", "is_error": true, "result": result}).to_string()
        };
        let cases = [
            (
                failed("Not logged in · Please run /login"),
                1,
                "is not logged in",
            ),
            (
                failed("Claude AI usage limit reached|1761530400"),
                1,
                "usage limit is reached",
            ),
            (
                failed("API Error: 404 model: claude-TEST not found"),
                1,
                "doesn't offer model claude-TEST",
            ),
            (
                failed("TEST something else"),
                1,
                "Claude Code failed: TEST something else",
            ),
            (String::new(), 3, "exited with exit status: 3"),
            ("not json".into(), 0, "Claude Code failed: not json"),
            (
                serde_json::json!({"type": "result", "is_error": false, "result": "TEST canned",
                    "modelUsage": {"<synthetic>": {}}})
                .to_string(),
                0,
                "Claude Code failed: TEST canned",
            ),
            (
                reply(&"x".repeat(LIMIT + 1)),
                0,
                "Claude Code output exceeded 32 KiB.",
            ),
        ];
        for (text, exit, expected) in cases {
            let error = error(&text, exit);
            assert!(error.contains(expected), "{expected}: {error}");
        }
        assert!(error(&failed("TEST"), 1).ends_with("No automatic retry."));
        let missing = Call {
            program: "/TEST-no-such-claude".into(),
            ..FakeClaude::new("", 0).call(false)
        };
        let budget = CallBudget::new(1);
        let error = missing
            .complete(
                &budget,
                Request::Shaping,
                vec![],
                "TEST".into(),
                &AtomicBool::new(false),
            )
            .unwrap_err();
        assert!(
            error.contains("`claude` is not installed or not on PATH"),
            "{error}"
        );
        assert_eq!(budget.used(), 1, "a request is counted before it starts");
    }

    #[test]
    fn each_harness_refuses_models_and_thinking_it_cannot_run_as_named() {
        assert!(
            Harness::Pi
                .check_model("anthropic/claude-sonnet-5-5")
                .is_ok()
        );
        assert!(Harness::ClaudeCode.check_model("claude-sonnet-5-5").is_ok());
        for model in [
            "sonnet",
            "claude-",
            "claude-x y",
            "anthropic/claude-sonnet-5-5",
        ] {
            assert!(Harness::ClaudeCode.check_model(model).is_err(), "{model}");
        }
        assert!(Harness::ClaudeCode.check_thinking("minimal").is_err());
        assert!(Harness::Pi.check_thinking("minimal").is_ok());
        for model in Harness::ClaudeCode.models() {
            assert!(Harness::ClaudeCode.check_model(model).is_ok());
        }
    }

    /// The head of `pi --list-models --offline` as Pi 1.4.0 prints it, trailing spaces and all.
    const PI_TABLE: &str = "Loading registry\n\
provider      model                       context  max-out  thinking  images\n\
anthropic     claude-haiku-5-5            1M       128K     yes       yes   \n\
anthropic     claude-opus-5-5             1M       128K     yes       yes   \n\
freellmapi    auto:fast                   128K     16.4K    no        yes   \n\
openai-codex  gpt-6.1-terra               500K     128K     yes       yes   \n";

    fn fake_pi(script: &str) -> (tempfile::TempDir, PathBuf) {
        let directory = tempfile::Builder::new()
            .prefix("tinkery-TEST-pi-list-")
            .tempdir()
            .unwrap();
        let program = directory.path().join("pi");
        std::fs::write(
            &program,
            format!("#!/usr/bin/env python3\nimport sys\n{script}\n"),
        )
        .unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        (directory, program)
    }

    #[test]
    fn pi_lists_its_models_as_provider_slash_id_without_a_model_request() {
        assert_eq!(
            listed_models(PI_TABLE),
            [
                "anthropic/claude-haiku-5-5",
                "anthropic/claude-opus-5-5",
                "freellmapi/auto:fast",
                "openai-codex/gpt-6.1-terra"
            ]
        );
        let (_directory, program) = fake_pi(&format!(
            "assert sys.argv[1:] == ['--list-models', '--offline'], sys.argv\n\
             assert sys.stdin.read() == ''\nsys.stdout.write({PI_TABLE:?})"
        ));
        assert_eq!(pi_models(&program).unwrap().len(), 4);
        let (_directory, failing) = fake_pi("sys.exit(2)");
        let (_directory, empty) = fake_pi("print('provider  model')");
        for (program, why) in [
            (failing, "couldn't list its models"),
            (empty, "listed no models"),
            (PathBuf::from("/TEST-no-such-pi"), "Pi isn't installed"),
        ] {
            let error = pi_models(&program).unwrap_err();
            assert!(error.contains(why), "{error}");
        }
    }
}
