//! Explicit, bounded worker actions through the named checkout's SDLC runtime. SDLC owns gates,
//! sessions, caps and detached workers. Recent launch evidence can inform the operator’s cap choice.
//! It records relayed start attribution, never SDLC approvals, and never retries or calls a provider.

mod authorization;
pub(crate) use authorization::{Gate, Relay};
mod orientation;
pub(crate) use orientation::cap_advice;
#[cfg(test)]
mod tests;

use crate::sdlc::git;
use crate::{repo::Repo, sdlc, seed_me::ready, think_record};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, mpsc},
    thread::{self, JoinHandle},
    time::Duration,
};

const STEP: &str = "build";
// Official resolution may take 15 s, followed by admission taking 60 s.
const ADMISSION: Duration = Duration::from_secs(90);
const STAGE: Duration = Duration::from_secs(180);

/// A separate worker allowance; neither drafting settings nor seed confirmation supply it.
#[derive(Clone, Debug, Default)]
pub(crate) struct Config {
    /// Exact Pi provider/model:thinking pin, never a saved default or alias.
    pub(crate) model: String,
    /// Maximum recorded responses across launch and resume; zero grants no allowance.
    pub(crate) responses: usize,
}

impl Config {
    /// Refuse missing allowance before any command or durable record is created.
    pub(crate) fn validate(&self) -> Result<(), String> {
        let valid = regex::Regex::new(
            r"^[a-zA-Z0-9][a-zA-Z0-9._-]*/[a-zA-Z0-9][a-zA-Z0-9._:/-]*:(off|minimal|low|medium|high|xhigh|max)$",
        )
        .expect("worker pin pattern");
        if self.responses == 0 || !valid.is_match(&self.model) {
            return Err("Starting a worker needs an exact provider/model:thinking and a positive response allowance.".into());
        }
        Ok(())
    }
}

/// An exact seed and canonical checkout selected for an explicit Start worker action.
#[derive(Clone, Debug)]
pub(crate) struct Start {
    /// Verified identity selected in Approve; execution rechecks its bytes and confirmation.
    pub(crate) target: ready::SeedTarget,
    /// Canonical checkout shown before the action; a substituted path is refused.
    pub(crate) root: PathBuf,
    /// Think records used to refuse a known different repository origin.
    pub(crate) record_root: Result<PathBuf, String>,
    /// The separately disclosed worker allowance.
    pub(crate) config: Config,
    /// The reviewed start authority; an absent or mismatched agent relay refuses before creation.
    pub(crate) authorization: Gate,
}

/// Source-owned run identity pinned across later actions, not a new workflow store.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Run {
    /// The exact canonical checkout where the build lives.
    pub(crate) root: PathBuf,
    /// SDLC run name, never interpreted as shell text.
    pub(crate) slug: String,
    /// SDLC's stable run identity; a reused name cannot substitute a run.
    pub(crate) id: String,
}

/// The current turn as reported by SDLC's offline source read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Turn {
    /// The adapter observed the latest turn end normally; this proves no checks.
    Ended,
    /// The latest turn has not ended normally, including an interrupted turn.
    Running,
    /// The adapter could not establish the turn state.
    Unknown,
}

/// Current step state, without converting stopped or uncertain work into completion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum State {
    /// Intake exists but no worker step has been prepared.
    NotStarted,
    /// The immutable step exists and has not been admitted.
    Prepared,
    /// A session is recorded; its current turn is independently observed.
    Active { turn: Turn },
    /// Admission was interrupted or failed; no automatic retry is safe.
    Uncertain,
    /// SDLC recorded a finished turn and a report, not passing verification.
    Completed,
    /// The worker was stopped explicitly or at the recorded response cap.
    Stopped { cap: bool },
}

/// One current SDLC observation for presentation and explicitly requested follow-up actions.
#[derive(Clone, Debug)]
pub(crate) struct Snapshot {
    /// Stable source identity used to recheck the next action.
    pub(crate) run: Run,
    /// Effective immutable step pin, empty when there is no step.
    pub(crate) model: String,
    /// Effective cumulative response cap, zero when there is no step.
    pub(crate) responses: usize,
    /// The observed immutable step identity; resume must present this same observation.
    pub(crate) pin: Option<Pin>,
    /// Current source-owned worker state.
    pub(crate) state: State,
    /// SDLC verification state; a completed worker does not imply current verification.
    pub(crate) verification: sdlc::Word<sdlc::Verification>,
    /// Validated source-reported judgment IDs, not a completion or acceptance claim.
    pub(crate) pending_attestations: Vec<String>,
    /// SDLC's next action in its own words.
    pub(crate) next: String,
}

/// Opaque fingerprint of the source contract's fixed run, session, model, allowance and paths.
/// It permits comparison with what was displayed, never supplies authority on its own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Pin(String);

/// Each variant needs its own explicit interface action; none runs after a refresh.
#[derive(Clone, Debug)]
pub(crate) enum Action {
    /// Read current source records without admitting work or writing anything.
    Refresh,
    /// Explicitly retry a prepared step after inspecting its unchanged contract.
    Launch(Pin),
    /// Resume the recorded session under its original cumulative cap.
    Resume(Pin),
    /// Record a finished turn only after current passing checks and SDLC's report/session check.
    Finish,
    /// Run the checkout's verification commands, separately from worker completion.
    Verify,
    /// Stop the known step as an operator-directed stop, never normal completion.
    StopOperator,
    /// Record a known cap stop, never normal completion.
    StopCap,
}

/// The safe follow-up established by the transaction, never inferred from a missing identity.
#[derive(Clone, Debug)]
pub(crate) enum Recovery {
    /// No creation was attempted; the operator may fix the cause and explicitly retry.
    Retry,
    /// Creation may have happened; read SDLC in Watch before any further start.
    Inspect,
    /// A source-owned identity is available for explicit follow-up actions.
    Run(Run),
}

/// A refusal with its transaction-owned explanation and safe follow-up.
#[derive(Clone, Debug)]
pub(crate) struct Failure {
    /// The actual failed boundary and any next step, without an invented success.
    pub(crate) message: String,
    /// Whether retry is safe, inspection is needed, or a pinned run is available.
    pub(crate) recovery: Recovery,
}

/// One finite action owned by the menu across visits. Dropping waits for its bounded command
/// sequence rather than killing admission between its uncertain and active receipts.
pub(crate) struct Job {
    result: mpsc::Receiver<Result<Snapshot, Failure>>,
    worker: Option<JoinHandle<()>>,
}

impl Job {
    /// Start only after the interface explicitly grants the displayed worker action.
    pub(crate) fn start(request: Start) -> Self {
        Self::spawn(move || start(request))
    }

    /// Perform exactly one requested action, rechecking the pinned run before any write.
    pub(crate) fn act(gate: Gate, run: Run, action: Action) -> Self {
        Self::spawn(move || {
            act(&gate, &run, action).map_err(|message| Failure {
                message,
                recovery: Recovery::Run(run),
            })
        })
    }

    fn spawn(work: impl FnOnce() -> Result<Snapshot, Failure> + Send + 'static) -> Self {
        let (send, result) = mpsc::channel();
        let owner = thread::current();
        let worker = thread::spawn(move || {
            let _ = send.send(work());
            owner.unpark();
        });
        Self {
            result,
            worker: Some(worker),
        }
    }

    /// Deliver the action result once; a pending job performs no additional actions.
    pub(crate) fn poll(&mut self) -> Option<Result<Snapshot, Failure>> {
        self.worker.as_ref()?;
        let result = match self.result.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return None,
            Err(mpsc::TryRecvError::Disconnected) => Err(Failure {
                message: "The worker request stopped unexpectedly. Inspect SDLC before retrying."
                    .into(),
                recovery: Recovery::Inspect,
            }),
        };
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        Some(result)
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn command(root: &Path, args: &[String], deadline: Duration) -> Result<String, String> {
    sdlc::status(root, args, &AtomicBool::new(false), deadline)
}

fn canonical_root(root: &Path) -> Result<(), String> {
    if !root.is_absolute()
        || root.canonicalize().ok().as_deref() != Some(root)
        || Repo::enclosing(root).as_ref().map(Repo::root) != Some(root)
    {
        return Err(
            "The selected checkout moved or is not a canonical repository root. Reopen it.".into(),
        );
    }
    if !root.join(sdlc::SCRIPT).is_file() {
        return Err("The named checkout has no SDLC runtime.".into());
    }
    Ok(())
}

fn status(root: &Path, slug: &str) -> Result<sdlc::Run, String> {
    read_status(root, slug, &AtomicBool::new(false), sdlc::DEADLINE)
}

fn read_status(
    root: &Path,
    slug: &str,
    cancel: &AtomicBool,
    deadline: Duration,
) -> Result<sdlc::Run, String> {
    canonical_root(root)?;
    if slug.is_empty()
        || slug.len() > 80
        || !slug
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        || slug.starts_with('-')
    {
        return Err("The run name is not supported; no worker action was taken.".into());
    }
    let output = sdlc::status(
        root,
        &["status".into(), slug.into(), "--json".into()],
        cancel,
        deadline,
    )?;
    let value = sdlc::envelope(&output)?;
    let run = &value["run"];
    if run["slug"].as_str() != Some(slug) {
        return Err("The checkout reported a different run; no worker action was taken.".into());
    }
    Ok(sdlc::Run::parse(run))
}

fn identity(root: &Path, slug: &str, report: &sdlc::Run) -> Result<Run, String> {
    let value = &report.data;
    let id = value["run_id"]
        .as_str()
        .filter(|id| sdlc::canonical_run_id(id))
        .ok_or("The run has no stable identity.")?;
    let root = match value.get("checkout") {
        Some(value) => PathBuf::from(value.as_str().ok_or("The build checkout is unreadable.")?),
        None => root.to_path_buf(),
    };
    canonical_root(&root)?;
    Ok(Run {
        root,
        slug: slug.into(),
        id: id.into(),
    })
}

fn current(run: &Run) -> Result<sdlc::Run, String> {
    let value = status(&run.root, &run.slug)?;
    if identity(&run.root, &run.slug, &value)? != *run {
        return Err("This run or its build checkout changed. No worker action was taken.".into());
    }
    Ok(value)
}

fn require_seed(value: &sdlc::Run) -> Result<ready::ReadySeed, String> {
    sdlc::confirmed(&value.data).map_err(|_| "The run's original confirmed seed is unavailable or changed. Recheck it before running work.".into())
}

// Check both the recorded ancestry and the files a newly launched worker will actually read.
fn check_retry_source(run: &Run, target: Option<&ready::SeedTarget>) -> Result<(), String> {
    if let Some(base) = target.and_then(|t| t.retry.as_ref()) {
        base.check_build(&run.root, &run.slug, &source_head(&run.root)?)
            .map_err(|_| "The new build did not start at the confirmed candidate. No worker was launched.")?;
        sdlc::source::check(&run.root, &base.head, |args| git(&run.root, args)).map_err(|why| {
            format!("{why} Inspect the build checkout before launching a worker.")
        })?;
    }
    Ok(())
}

fn source_head(root: &Path) -> Result<String, String> {
    Ok(git(root, &["rev-parse", "--verify", "HEAD"])?.trim().into())
}

fn committed_source(root: &Path) -> Result<String, String> {
    let head = source_head(root)?;
    let changes = git(
        root,
        &[
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=all",
            "--",
            ".",
            ":(exclude)runs",
        ],
    )?;
    if !changes.is_empty() {
        return Err("Commit source changes first. The worker receives the committed checkout; uncommitted source outside runs/ would be omitted.".into());
    }
    Ok(head)
}

fn start(request: Start) -> Result<Snapshot, Failure> {
    let (intake, source_commit) = (|| {
        request.authorization.check(Some(&request.target), &request.config)?;
        request.config.validate()?;
        canonical_root(&request.root)?;
        let (intake, goal) = ready::execution(&request.target, &request.root)?;
        if let Some(base) = &request.target.retry {
            base.check_checkout(&request.root)?;
            canonical_root(&intake.root)?;
        }
        let origins = request.record_root.as_ref().ok().and_then(|root| {
            think_record::read_origins(root, &[(&request.target.session, goal.as_str())]).ok()
        });
        if let Some(think_record::MadeIn::Repo(origin)) = origins.as_ref().and_then(|o| o.first())
            && origin != &intake.root
        {
            return Err(
                "The selected seed belongs to a different repository. No worker was started."
                    .into(),
            );
        }
        let source_commit = committed_source(&intake.root)?;
        if request
            .target
            .retry
            .as_ref()
            .is_some_and(|base| source_commit != base.head)
        {
            return Err("The retry checkout moved off its confirmed starting commit.".into());
        }
        let seeds = sdlc::seeds(&intake.root, &AtomicBool::new(false))?;
        if sdlc::started(&seeds, &intake.seed)? {
            return Err("This exact seed already has a run. Inspect that run instead of starting another worker.".into());
        }
        ready::execution(&request.target, &intake.root)?;
        Ok((intake, source_commit))
    })()
    .map_err(|message: String| Failure {
        message: format!("{message} Nothing was started; fix that, then F2 tries again."),
        recovery: Recovery::Retry,
    })?;
    let mut known = None;
    let result = (|| {
        command(
            &intake.root,
            &["new".into(), intake.slug.clone()],
            sdlc::DEADLINE,
        )?;
        let value = status(&intake.root, &intake.slug)?;
        let run = identity(&intake.root, &intake.slug, &value)?;
        known = Some(run.clone());
        ready::execution(&request.target, &intake.root)?;
        command(
            &intake.root,
            &[
                "stage".into(),
                intake.slug.clone(),
                "1".into(),
                "--seed-contract".into(),
                intake.seed.to_string_lossy().into_owned(),
                "--tinkery".into(),
                intake.executable.to_string_lossy().into_owned(),
            ],
            STAGE,
        )?;
        let value = current(&run)?;
        let seed =
            sdlc::Seed::parse(&value.data)?.ok_or("Intake did not record the selected seed.")?;
        if !seed.matches(&intake.seed)? {
            return Err("Intake recorded a different seed; no worker was started.".into());
        }
        ready::execution(&request.target, &intake.root)?;
        if let Some(base) = &request.target.retry {
            base.check_checkout(&request.root)?;
        }
        if source_head(&intake.root)? != source_commit {
            return Err("The selected source commit changed during intake. Inspect the run before starting a worker.".into());
        }
        command(
            &intake.root,
            &["stage".into(), intake.slug.clone(), "3".into()],
            STAGE,
        )?;
        let value = status(&intake.root, &intake.slug)?;
        let build = identity(&intake.root, &intake.slug, &value)?;
        if build.id != run.id || build.root == run.root {
            return Err(
                "Stage 3 did not report this run's separate build checkout; no worker was started."
                    .into(),
            );
        }
        known = Some(build.clone());
        check_retry_source(&build, Some(&request.target))?;
        let value = current(&build)?;
        require_seed(&value)?;
        ready::execution(&request.target, &intake.root)?;
        request
            .authorization
            .record(&build, Some(&request.target), &request.config, "start")?;
        let report = build
            .root
            .join("runs")
            .join(&build.slug)
            .join("worker-report.md");
        let launcher = build.root.join("_system/scripts/pi-launcher");
        command(
            &build.root,
            &[
                "step".into(),
                build.slug.clone(),
                STEP.into(),
                "prepare".into(),
                "--report".into(),
                report.to_string_lossy().into_owned(),
                "--model".into(),
                request.config.model.clone(),
                "--launcher".into(),
                launcher.to_string_lossy().into_owned(),
                "--max-responses".into(),
                request.config.responses.to_string(),
            ],
            sdlc::DEADLINE,
        )?;
        let prepared = snapshot(&build)?;
        if prepared.state != State::Prepared
            || prepared.model != request.config.model
            || prepared.responses != request.config.responses
        {
            return Err("The prepared worker differs from the displayed model or allowance. No worker was launched.".into());
        }
        let value = current(&build)?;
        require_seed(&value)?;
        ready::execution(&request.target, &intake.root)?;
        check_retry_source(&build, Some(&request.target))?;
        command(
            &build.root,
            &[
                "step".into(),
                build.slug.clone(),
                STEP.into(),
                "launch".into(),
            ],
            ADMISSION,
        )?;
        snapshot(&build)
    })();
    result.map_err(|message| match known {
        Some(run) => Failure {
            message,
            recovery: Recovery::Run(run),
        },
        None => Failure {
            message: format!(
                "{message} Run {} may have been created. Read it in Watch with --sdlc-root {} before starting more work.",
                intake.slug,
                intake.root.display()
            ),
            recovery: Recovery::Inspect,
        },
    })
}

fn act(gate: &Gate, run: &Run, action: Action) -> Result<Snapshot, String> {
    if matches!(action, Action::Refresh) {
        let value = status(&run.root, &run.slug)?;
        let observed = identity(&run.root, &run.slug, &value)?;
        if observed.id != run.id {
            return Err("This run changed. No worker action was taken.".into());
        }
        return snapshot(&observed);
    }
    let value = current(run)?;
    if matches!(action, Action::Resume(_) | Action::Verify) {
        require_seed(&value)?;
    }
    let step = |action: &str| vec!["step".into(), run.slug.clone(), STEP.into(), action.into()];
    match action {
        Action::Refresh => {}
        Action::Launch(pin) => {
            let seed = require_seed(&value)?;
            admit(gate, run, pin, true, seed.target.as_ref())?;
        }
        Action::Resume(pin) => {
            let seed = require_seed(&value)?;
            admit(gate, run, pin, false, seed.target.as_ref())?;
        }
        Action::Finish => {
            if !value.verification.is(sdlc::Verification::Current)
                || value.data["verification"]["passed"].as_bool() != Some(true)
            {
                return Err("Verify first. Finish needs current passing checks; keep this worker available to repair failures.".into());
            }
            command(&run.root, &step("complete"), sdlc::DEADLINE)?;
        }
        Action::StopOperator | Action::StopCap => {
            let mut args = step("complete");
            args.extend([
                "--stopped-by".into(),
                if matches!(action, Action::StopCap) {
                    "cap"
                } else {
                    "operator"
                }
                .into(),
            ]);
            command(&run.root, &args, sdlc::DEADLINE)?;
        }
        Action::Verify => {
            let verify = &value.data["actions"]["verify"];
            if verify["allowed"].as_bool() != Some(true) {
                return Err(verify["reason"]
                    .as_str()
                    .unwrap_or("This runtime does not permit verification.")
                    .into());
            }
            let timeout = verify["timeout_seconds"]
                .as_u64()
                .filter(|seconds| *seconds > 0)
                .ok_or("The runtime supplied no bounded verification timeout.")?;
            command(
                &run.root,
                &["verify".into(), run.slug.clone()],
                Duration::from_secs(timeout),
            )?;
        }
    }
    snapshot(run)
}

fn admit(
    gate: &Gate,
    run: &Run,
    pin: Pin,
    launch: bool,
    target: Option<&ready::SeedTarget>,
) -> Result<(), String> {
    let observed = snapshot(run)?;
    if observed.pin.as_ref() != Some(&pin) {
        return Err("The worker's model, allowance, session or contract changed. Admission refused; inspect it again.".into());
    }
    if launch && observed.state != State::Prepared {
        return Err(
            "Launch needs a prepared step. An uncertain or active step cannot be launched again."
                .into(),
        );
    }
    if !launch && observed.state != (State::Active { turn: Turn::Ended }) {
        return Err("Resume needs an observed ended turn. A running or unknown turn may still own this session.".into());
    }
    if launch {
        check_retry_source(run, target)?;
    }
    gate.record(
        run,
        target,
        &Config {
            model: observed.model,
            responses: observed.responses,
        },
        if launch { "launch" } else { "resume" },
    )?;
    command(
        &run.root,
        &[
            "step".into(),
            run.slug.clone(),
            STEP.into(),
            if launch { "launch" } else { "resume" }.into(),
        ],
        ADMISSION,
    )?;
    Ok(())
}

fn snapshot(run: &Run) -> Result<Snapshot, String> {
    let value = current(run)?;
    let steps = value
        .steps
        .as_ref()
        .ok_or("This runtime does not report worker steps.")?;
    let step = steps
        .iter()
        .find(|step| step.data["name"].as_str() == Some(STEP));
    let (state, model, responses, pin) = if let Some(step) = step {
        let output = command(
            &run.root,
            &["step".into(), run.slug.clone(), STEP.into(), "show".into()],
            sdlc::DEADLINE,
        )?;
        let record: Value = serde_json::from_str(&output).map_err(|error| error.to_string())?;
        if record["schema"].as_u64() != Some(1)
            || record["run_id"].as_str() != Some(&run.id)
            || record["checkout"].as_str() != run.root.to_str()
        {
            return Err(
                "The worker contract does not belong to this exact run and checkout.".into(),
            );
        }
        let model = record["model"]
            .as_str()
            .ok_or("The worker model pin is unavailable.")?
            .to_owned();
        let responses = record["max_responses"]
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .ok_or("The worker has no response cap; this interface cannot resume it.")?;
        Config {
            model: model.clone(),
            responses,
        }
        .validate()?;
        for field in ["report", "launcher"] {
            if record[field].as_str().is_none_or(|value| {
                !Path::new(value).is_absolute() || value.chars().any(char::is_control)
            }) {
                return Err(format!(
                    "The worker's {field} path is unavailable or malformed."
                ));
            }
        }
        if record["launcher_sha256"].as_str().is_none_or(|value| {
            value.len() != 64
                || !value
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        }) {
            return Err("The worker launcher fingerprint is unavailable or malformed.".into());
        }
        let parsed = sdlc::Run::parse(&record);
        if matches!(
            parsed.state.known(),
            Some(sdlc::StepState::Active | sdlc::StepState::Completed | sdlc::StepState::Stopped)
        ) && record["session"]
            .as_str()
            .is_none_or(|value| value.trim().is_empty())
        {
            return Err("The worker has no recorded session identity.".into());
        }
        let fixed = serde_json::json!({
            "schema": record["schema"], "run_id": record["run_id"], "checkout": record["checkout"],
            "model": record["model"], "max_responses": record["max_responses"], "session": record["session"],
            "report": record["report"], "launcher": record["launcher"], "launcher_sha256": record["launcher_sha256"],
            "prepared_ns": record["prepared_ns"],
        });
        let pin = Pin(format!(
            "{:x}",
            Sha256::digest(fixed.to_string().as_bytes())
        ));
        let state = match parsed.state.known() {
            Some(sdlc::StepState::Prepared) => State::Prepared,
            Some(sdlc::StepState::Active) => State::Active {
                turn: match step.turn.known() {
                    Some(sdlc::TurnStatus::Ended) => Turn::Ended,
                    Some(sdlc::TurnStatus::Running) => Turn::Running,
                    Some(sdlc::TurnStatus::Unknown) => Turn::Unknown,
                    _ => return Err("The worker turn is not reported in a supported form.".into()),
                },
            },
            Some(sdlc::StepState::Uncertain) => State::Uncertain,
            Some(sdlc::StepState::Completed) => State::Completed,
            Some(sdlc::StepState::Stopped) => State::Stopped {
                cap: match parsed.stopped_by.known() {
                    Some(sdlc::StopReason::Cap) => true,
                    Some(sdlc::StopReason::Operator) => false,
                    _ => return Err("The stopped worker has no known stop reason.".into()),
                },
            },
            _ => {
                return Err(
                    "The worker state is not supported; inspect SDLC before continuing.".into(),
                );
            }
        };
        (state, model, responses, Some(pin))
    } else {
        (State::NotStarted, String::new(), 0, None)
    };
    Ok(Snapshot {
        run: run.clone(),
        model,
        responses,
        pin,
        state,
        pending_attestations: value.pending_attestations?,
        verification: {
            value
                .verification
                .known()
                .ok_or("Verification state is unknown or missing.")?;
            value.verification
        },
        next: value.data["next_action"]
            .as_str()
            .ok_or("The run's next action is missing.")?
            .into(),
    })
}
