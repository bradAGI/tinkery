//! Shape's agent step: one operator key, then (when it says so) one model call through the chosen
//! harness with read-only tools, and the shared ledger calls that record the result.
//!
//! The model only investigates and drafts words: what it found, with the next question or what the
//! seed confirms, plus separate check records validated only by Tinkery's ledger. It never formats or runs checks. It reads only the folder the operator was shown. Tinkery saves the operator's
//! answer, the question and the findings, builds the seed's evidence and decisions from
//! the ledger, and confirms a seed itself, each through the shared session boundary. The model is never asked to confirm
//! anything, and nothing it replies can confirm a seed. Each step counts against the call budget
//! before the model starts; with none left it refuses before writing anything.

use crate::{
    pi::{self, Call, CallBudget, Harness, Request},
    seed_me::{
        answering::{self, Alternative, Ask, Assumption, Found, Refusal},
        ledger,
    },
    think_record::{self, MadeIn},
};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

const PROMPT: &str = include_str!("prompt.txt");

#[cfg(all(test, unix))]
mod reply_contract_tests;
#[cfg(test)]
mod typed_tests;
#[cfg(test)]
mod unknown_tests;

mod review_reply;
mod teach_back;

/// One bounded host diagnostic for an explicit retry against the same saved context.
/// It is neither an operator answer nor ledger evidence, and is never persisted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RetryFeedback {
    session: PathBuf,
    version: u64,
    folder: PathBuf,
    reason: String,
    truncated: bool,
}

impl RetryFeedback {
    const LIMIT: usize = 2048;

    /// Whether this rejection still belongs to the session version and folder being read.
    pub(super) fn applies(&self, session: &Path, version: u64, folder: &Path) -> bool {
        self.session == session && self.version == version && self.folder == folder
    }

    fn new(session: &Path, version: u64, folder: &Path, reason: &str) -> Option<Self> {
        let line = reason
            .lines()
            .next()
            .filter(|line| !line.trim().is_empty())?;
        let truncated = line.len() > Self::LIMIT || reason.lines().nth(1).is_some();
        let mut reason = line.to_owned();
        if reason.len() > Self::LIMIT {
            let mut end = Self::LIMIT - '…'.len_utf8();
            while !reason.is_char_boundary(end) {
                end -= 1;
            }
            reason.truncate(end);
            reason.push('…');
        }
        Some(Self {
            session: session.into(),
            version,
            folder: folder.into(),
            reason,
            truncated,
        })
    }

    fn input(&self) -> Value {
        json!({"reason": self.reason, "truncated": self.truncated})
    }
}

/// Current reply fields decoded here or delegated unchanged to Tinkery's record validator.
/// The prompt's examples name the same fields.
#[cfg(test)]
pub(super) const FIELDS: &[&str] = &[
    "next",
    "id",
    "label",
    "question",
    "gate",
    "found",
    "resolves",
    "unknown",
    "resolved_by",
    "checked",
    "observed",
    "claim",
    "type",
    "scope",
    "limits",
    "supported_by",
    "prerequisites",
    "contradicts",
    "suggestion",
    "assumed",
    "text",
    "why",
    "options",
    "choice",
    "tradeoff",
    "undo",
    "against",
    "confidence",
    "confirming",
    "review",
    "contradictions",
    "decided",
    "done",
    "failure_case",
    "checks",
    "statement",
    "kind",
    "source",
    "how_to_judge",
    "cmd",
    "expect",
    "cwd",
    "missing",
];

/// How Shape reaches a model: Pi or Claude Code with an explicit model and read-only tools, a call budget shared
/// for the whole run (zero unless the operator sets one), and where Think's records are.
pub struct Agent {
    call: Call,
    budget: Arc<CallBudget>,
    records: Option<PathBuf>,
    pub(super) confirmation: Option<crate::think_record::GoalConfirmation>,
}

impl Agent {
    /// `harness` at `program` with `model` and `thinking` (empty for its default), starting in
    /// `folder` unless Think recorded where a goal was made; `budget` limits every step. Refuses a
    /// model or thinking level the harness doesn't accept. Starts nothing.
    pub fn new(
        harness: Harness,
        program: PathBuf,
        model: String,
        thinking: String,
        folder: Option<PathBuf>,
        budget: Arc<CallBudget>,
        records: Option<PathBuf>,
    ) -> Result<Self, String> {
        harness.check_model(&model)?;
        if !thinking.is_empty() {
            harness.check_thinking(&thinking)?;
        }
        Ok(Self {
            call: Call {
                harness,
                program,
                model,
                thinking,
                read_only_tools: true,
                folder,
                timeout: Duration::from_secs(300),
            },
            budget,
            records,
            confirmation: None,
        })
    }

    /// Reuse named caller context for this run; model replies can never supply or replace it.
    pub fn with_confirmation(
        mut self,
        confirmation: Option<crate::think_record::GoalConfirmation>,
    ) -> Self {
        self.confirmation = confirmation;
        self
    }

    /// Model calls this run may still make.
    pub fn remaining(&self) -> usize {
        self.budget.remaining()
    }

    /// The run's call limit.
    pub fn limit(&self) -> usize {
        self.budget.limit()
    }

    /// The allowance sentence Think shows too, so both phases count the same way.
    pub fn left(&self) -> String {
        self.budget.left()
    }

    /// The folder a step for `session` reads unless the operator picks another: where Think
    /// recorded `goal` was made, else the folder this agent was given, else where Tinkery started.
    /// Reads Think's records; blocks, so run it off the paint path.
    pub(super) fn folder(&self, session: &Path, goal: &str) -> PathBuf {
        let made_in = self
            .records
            .as_deref()
            .and_then(|root| think_record::read_for_shape(root, session, goal).ok())
            .and_then(|read| match read.made_in {
                MadeIn::Repo(path) => Some(path),
                _ => None,
            });
        made_in
            .or_else(|| self.call.folder.clone())
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."))
    }
}

/// What one key asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Work {
    /// Save the operator's answer to question `id`, then look into the next question in `folder`.
    Answer {
        id: String,
        words: String,
        expected: u64,
        folder: PathBuf,
    },
    /// Publish the operator's explicit action, then look into the next.
    Respond {
        id: String,
        action: crate::seed_me::ledger::AnswerAction,
        expected: u64,
        folder: PathBuf,
    },
    /// Look into the next question (or the seed) from version `expected`, reading `folder`.
    Look {
        expected: u64,
        folder: PathBuf,
        feedback: Option<RetryFeedback>,
    },
    /// Compare the operator's teach-back with the exact displayed draft, using one budgeted call.
    TeachBack {
        revision: String,
        words: String,
        expected: u64,
        seed: String,
    },
    /// Save the operator's change to seed draft `revision` from `expected`, then redraft by looking
    /// in `folder`. Never confirms.
    Change {
        revision: String,
        words: String,
        expected: u64,
        folder: PathBuf,
    },
    /// Confirm only the draft bytes and ledger version the operator read.
    Confirm {
        revision: String,
        words: String,
        expected: u64,
        seed: String,
    },
}

/// What a finished step did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Done {
    /// A new question is current, with the defaults its step assumed instead of asking.
    Asked(Asked),
    /// A seed draft was saved.
    Drafted,
    /// Teach-back words and the model's mismatches were saved; F3 remains available.
    Compared,
    /// The seed is confirmed.
    Confirmed,
}

/// What the step that asked a question leaves for the screen, beyond what the ledger records.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Asked {
    /// Defaults assumed instead of asking.
    pub(super) assumed: Vec<Assumption>,
    /// A hard-to-undo question's suggestion, not yet recorded, until the operator asks for it.
    pub(super) withheld: Option<String>,
}

/// Why a step stopped short, and whether the operator's answer was saved before it did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Failure {
    pub(super) answer_saved: bool,
    pub(super) why: Why,
    pub(super) feedback: Option<Box<RetryFeedback>>,
}

impl Failure {
    fn rejected(
        mut self,
        session: &Path,
        version: u64,
        folder: &Path,
        cancelled: &AtomicBool,
    ) -> Self {
        if !cancelled.load(Ordering::Relaxed)
            && let Why::Model(reason) | Why::Refused(reason) = &self.why
        {
            self.feedback = RetryFeedback::new(session, version, folder, reason).map(Box::new);
        }
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Why {
    /// Another writer changed the session first; nothing more was written.
    Stale,
    /// No drafting runs are left.
    Budget(usize),
    /// The model call failed or its reply couldn't be used.
    Model(String),
    /// Tinkery refused; the reason says whether anything saved.
    Refused(String),
    /// The operator stopped it.
    Stopped,
}

/// A running step, off the paint path. Dropping it asks the step to stop; a model call stops
/// promptly, and a helper call already started finishes.
pub(super) struct Running {
    cancelled: Arc<AtomicBool>,
    saved: Arc<AtomicBool>,
    started: Instant,
    result: mpsc::Receiver<Result<Done, Failure>>,
}

impl Running {
    /// Start `work` on `session`.
    pub(super) fn start(work: Work, session: PathBuf, agent: Arc<Agent>) -> Self {
        let cancelled = Arc::new(AtomicBool::new(false));
        let saved = Arc::new(AtomicBool::new(false));
        let (flag, answered) = (cancelled.clone(), saved.clone());
        let (sender, result) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send(run(&work, &session, &agent, &flag, &answered));
        });
        Self {
            cancelled,
            saved,
            started: Instant::now(),
            result,
        }
    }

    /// Whether the operator's answer is saved yet; only an answer step ever saves one.
    pub(super) fn saved(&self) -> bool {
        self.saved.load(Ordering::Relaxed)
    }

    /// How long the step has run.
    pub(super) fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    /// The result, once it lands. Never blocks.
    pub(super) fn poll(&self) -> Option<Result<Done, Failure>> {
        match self.result.try_recv() {
            Ok(mut result) => {
                if self.cancelled.load(Ordering::Relaxed)
                    && let Err(failure) = &mut result
                {
                    failure.feedback = None;
                }
                Some(result)
            }
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => Some(Err(Failure {
                answer_saved: false,
                why: Why::Model("The step stopped unexpectedly.".into()),
                feedback: None,
            })),
        }
    }

    /// Ask the step to stop.
    pub(super) fn stop(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        self.stop();
    }
}

fn refused(answer_saved: bool) -> impl Fn(Refusal) -> Failure {
    move |refusal| Failure {
        answer_saved,
        feedback: None,
        why: match refusal {
            Refusal::Stale => Why::Stale,
            Refusal::Refused(reason) | Refusal::Rejected(reason) => Why::Refused(reason),
        },
    }
}

fn run(
    work: &Work,
    session: &Path,
    agent: &Agent,
    cancelled: &AtomicBool,
    saved: &AtomicBool,
) -> Result<Done, Failure> {
    let actor = agent.confirmation.as_ref();
    crate::think_record::Actor::authorize(actor).map_err(|reason| Failure {
        answer_saved: false,
        why: Why::Refused(reason),
        feedback: None,
    })?;
    match work {
        Work::Answer {
            id,
            words,
            expected,
            folder,
        } => {
            if agent.remaining() == 0 {
                return Err(Failure {
                    answer_saved: false,
                    why: Why::Budget(agent.limit()),
                    feedback: None,
                });
            }
            let version =
                answering::answer(session, *expected, id, words, actor).map_err(refused(false))?;
            saved.store(true, Ordering::Relaxed);
            look(session, version, folder, agent, cancelled, true, None)
        }
        Work::Respond {
            id,
            action,
            expected,
            folder,
        } => {
            if agent.remaining() == 0 {
                return Err(Failure {
                    answer_saved: false,
                    why: Why::Budget(agent.limit()),
                    feedback: None,
                });
            }
            let version = answering::respond(session, *expected, id, action, actor)
                .map_err(refused(false))?;
            saved.store(true, Ordering::Relaxed);
            look(session, version, folder, agent, cancelled, true, None)
        }
        Work::Look {
            expected,
            folder,
            feedback,
        } => look(
            session,
            *expected,
            folder,
            agent,
            cancelled,
            false,
            feedback.as_ref(),
        ),
        Work::TeachBack { .. } => teach_back::run(work, session, agent, cancelled, saved),
        Work::Change {
            revision,
            words,
            expected,
            folder,
        } => {
            if agent.remaining() == 0 {
                return Err(Failure {
                    answer_saved: false,
                    why: Why::Budget(agent.limit()),
                    feedback: None,
                });
            }
            let version = answering::change_draft(session, *expected, revision, words, actor)
                .map_err(refused(false))?;
            saved.store(true, Ordering::Relaxed);
            look(session, version, folder, agent, cancelled, true, None)
        }
        Work::Confirm {
            revision,
            words,
            expected,
            seed,
        } => {
            answering::confirm_seed(session, *expected, revision, seed, words, actor)
                .map_err(refused(false))?;
            Ok(Done::Confirmed)
        }
    }
}

fn look(
    session: &Path,
    expected: u64,
    folder: &Path,
    agent: &Agent,
    cancelled: &AtomicBool,
    answer_saved: bool,
    feedback: Option<&RetryFeedback>,
) -> Result<Done, Failure> {
    let fail = |why| Failure {
        answer_saved,
        why,
        feedback: None,
    };
    let rejected = |failure: Failure| failure.rejected(session, expected, folder, cancelled);
    // A ledger rule the reply broke is retry feedback too; storage refusals never are (#432).
    let reply_refused = |refusal: Refusal| match refusal {
        Refusal::Rejected(_) => rejected(refused(answer_saved)(refusal)),
        refusal => refused(answer_saved)(refusal),
    };
    let ledger = answering::read(session).map_err(refused(answer_saved))?;
    if ledger.version() != expected {
        return Err(fail(Why::Stale));
    }
    if ledger.has_current_question() {
        return Ok(Done::Asked(Asked::default()));
    }
    // A decision already recorded, such as an objection, is asked before the model looks for one,
    // and the pre-mortem and real-sample probes are put to the operator before any seed, also
    // without a model.
    if answering::promote(session, expected)
        .map_err(refused(answer_saved))?
        .is_some()
        || answering::probe(session, expected)
            .map_err(refused(answer_saved))?
            .is_some()
    {
        return Ok(Done::Asked(Asked::default()));
    }
    if agent.remaining() == 0 {
        return Err(fail(Why::Budget(agent.limit())));
    }
    let goal = ledger.goal().unwrap_or("").to_owned();
    let settled = think(agent.records.as_deref(), session, &goal);
    let mut input = input(&ledger, settled);
    if let Some(feedback) = feedback.filter(|f| f.applies(session, expected, folder)) {
        input["previous_refusal"] = feedback.input();
    }
    let call = Call {
        folder: Some(folder.to_path_buf()),
        ..agent.call.clone()
    };
    let contract = answering::contract().map_err(refused(answer_saved))?;
    let instructions = format!("{}\n\nSeed reply guidance for Shape:\n{contract}", prompt());
    let reply = call
        .complete(
            &agent.budget,
            Request::SeedStep,
            serde_json::to_vec(&input).unwrap_or_default(),
            instructions,
            cancelled,
        )
        .map_err(|error| {
            if cancelled.load(Ordering::Relaxed) {
                fail(Why::Stopped)
            } else {
                fail(Why::Model(error))
            }
        })?;
    match decode(&reply, &goal).map_err(|e| rejected(fail(Why::Model(e))))? {
        Next::Question(ask) => {
            answering::ask(session, expected, &ask, folder).map_err(reply_refused)?;
            Ok(Done::Asked(Asked {
                withheld: ask.recommendation().filter(|_| ask.hard()),
                assumed: ask.assumed,
            }))
        }
        Next::Seed {
            confirming,
            checks,
            review,
            found,
            resolves,
        } => {
            let records = crate::seed_me::ledger::Value::from(checks.clone());
            // #432: a reply breaking a check rule and its review hears both, so a retry can't
            // trade one fault for the other. Only pure reply checks become retry feedback.
            let review = review.and_then(|r| r.validate(&records).map(|()| r));
            let review = match (ledger::validate_goal_checks(&goal, &records), review) {
                (Ok(()), review) => review.map_err(|why| rejected(fail(Why::Model(why))))?,
                (Err(rule), review) => {
                    let also = review
                        .err()
                        .map(|e| format!(" Also: {e}"))
                        .unwrap_or_default();
                    return Err(rejected(fail(Why::Refused(format!(
                        "Tinkery refused: {rule}{also}"
                    )))));
                }
            };
            let revision = answering::draft_seed(
                session,
                expected,
                &confirming,
                &checks,
                &found,
                folder,
                resolves.as_ref(),
            )
            .map_err(reply_refused)?;
            let seed = std::fs::read_to_string(session.join("seed-contract.md")).map_err(|_| {
                fail(Why::Model(
                    "The draft could not be read. Reopen Shape before trying again.".into(),
                ))
            })?;
            answering::teach_back(
                session,
                answering::read(session)
                    .map_err(refused(answer_saved))?
                    .version(),
                &revision,
                &seed,
                &crate::seed_me::ledger::TeachBackRecord::Review(&review),
                agent.confirmation.as_ref(),
            )
            .map_err(refused(answer_saved))?;
            Ok(Done::Drafted)
        }
    }
}

fn input(ledger: &answering::Ledger, settled: Vec<Value>) -> Value {
    json!({
        "goal": ledger.goal().unwrap_or(""),
        "answered_in_think": settled,
        "found_so_far": ledger.findings(),
        "ledger": ledger.context_without_findings(),
    })
}

/// Restore the operator's exact Done when line where one check kept it whole but changed only its
/// letter case (#432). Other words, and checks that split or reword a line, are left to refuse.
fn restore_case(goal: &str, checks: &mut [Value]) {
    for criterion in ledger::goal_criteria(goal) {
        let keeps = |check: &Value, ignore_case| {
            check["statement"]
                .as_str()
                .is_some_and(|statement| ledger::keeps_criterion(statement, criterion, ignore_case))
        };
        if !checks.iter().any(|check| keeps(check, false))
            && let Some(check) = checks.iter_mut().find(|check| keeps(check, true))
        {
            check["statement"] = criterion.into();
        }
    }
}

/// Exactly two complete options, each with a known undo cost; anything else gives none.
fn alternatives(value: &Value) -> Vec<Alternative> {
    let options: Vec<Alternative> = value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|o| {
            let field =
                |key: &str| Some(o[key].as_str()?.trim().to_owned()).filter(|s| !s.is_empty());
            Some(Alternative {
                choice: field("choice")?,
                tradeoff: field("tradeoff")?,
                undo: field("undo")
                    .map(|u| u.to_lowercase())
                    .filter(|u| ["cheap", "moderate", "hard"].contains(&u.as_str()))?,
            })
        })
        .collect();
    if options.len() == 2 {
        options
    } else {
        Vec::new()
    }
}

/// The system prompt for one step.
pub(super) fn prompt() -> String {
    pi::assemble(PROMPT)
}

/// The questions Think's record says the operator answered for this exact session and goal.
/// Nothing when no record can be attributed.
fn think(records: Option<&Path>, session: &Path, goal: &str) -> Vec<Value> {
    let Some(read) =
        records.and_then(|root| think_record::read_for_shape(root, session, goal).ok())
    else {
        return Vec::new();
    };
    read.answered
        .into_iter()
        .map(|submission| json!({"question": submission.question, "answer": submission.answer}))
        .collect()
}

/// What the model proposed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Next {
    /// The next question.
    Question(Box<Ask>),
    /// The seed's summary and canonical prose sections, and what the step found.
    Seed {
        confirming: Value,
        checks: Value,
        /// Unusable reviews wait for the check rules, so one refusal names every broken rule.
        review: Result<Box<crate::seed_me::ledger::CompletionReview>, String>,
        found: Vec<Found>,
        resolves: Option<Value>,
    },
}

/// The model's reply as one JSON object (surrounding prose or a code fence is tolerated).
/// Tolerates redundant closing braces after one complete object, never missing content or a
/// second object. Refuses a reply that asks to confirm: only the operator confirms. A seed's
/// checks must keep each of `goal`'s Done when lines whole; every broken rule is named at once.
pub(super) fn decode(reply: &str, goal: &str) -> Result<Next, String> {
    let start = reply.find('{').ok_or_else(|| {
        "The reply contains no JSON object. Return one question or seed draft.".to_owned()
    })?;
    let end = reply[start..]
        .rfind('}')
        .map_or(reply.len(), |i| start + i + 1);
    let object = &reply[start..end];
    let mut stream = serde_json::Deserializer::from_str(object).into_iter::<Value>();
    let value = stream.next().expect("Object starts with an opening brace").map_err(|error| {
        if error.is_eof() {
            "The reply ended before its JSON object was complete. No question or seed draft was applied.".to_owned()
        } else {
            format!("The reply has invalid JSON at line {}, column {}. Return one question or seed draft.", error.line(), error.column())
        }
    })?;
    if !object[stream.byte_offset()..]
        .chars()
        .all(|c| c == '}' || c.is_whitespace())
        || reply[end..].contains('{')
    {
        return Err("The reply contains extra content after its JSON object. Return only one question or seed draft.".into());
    }
    let text = |key: &str| {
        value[key]
            .as_str()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    };
    if !matches!(value["next"].as_str(), Some("question" | "seed")) {
        return Err(
            "Reply field next must be question or seed; a model reply cannot confirm a seed."
                .into(),
        );
    }
    let found: Vec<Found> = serde_json::from_value(value["found"].clone()).map_err(|_| {
        "The model's findings are incomplete or use an unsupported format. Each finding needs an id, a typed claim, checked and observed. Retry the step.".to_owned()
    })?;
    match value["next"].as_str() {
        Some("question") => {
            let question = text("question").ok_or_else(|| {
                "Question reply field question must contain non-empty words.".to_owned()
            })?;
            Ok(Next::Question(Box::new(Ask {
                id: text("id").unwrap_or_else(|| "question".into()),
                label: text("label").unwrap_or_else(|| question.clone()),
                gate: text("gate").unwrap_or_else(|| question.clone()),
                question,
                found,
                resolves: value.get("resolves").cloned(),
                supported_by: value.get("supported_by").cloned(),
                prerequisites: value.get("prerequisites").cloned(),
                suggestion: text("suggestion"),
                assumed: value["assumed"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|a| {
                        Some(Assumption {
                            text: a["text"].as_str()?.trim().to_owned(),
                            why: a["why"].as_str()?.trim().to_owned(),
                        })
                    })
                    .filter(|a| !a.text.is_empty() && !a.why.is_empty())
                    .take(5)
                    .collect(),
                options: alternatives(&value["options"]),
                against: text("against"),
                confidence: text("confidence")
                    .filter(|c| ["low", "medium", "high"].contains(&c.as_str())),
            })))
        }
        Some("seed") => {
            let mut checks = value["checks"]
                .as_array()
                .filter(|checks| !checks.is_empty())
                .cloned()
                .ok_or_else(|| {
                    "Seed reply field checks must be a non-empty list of check records.".to_owned()
                })?;
            restore_case(goal, &mut checks);
            let review = review_reply::decode(value["review"].clone(), &mut checks);
            let checks = Value::Array(checks);
            Ok(Next::Seed {
                review,
                confirming: value.get("confirming").cloned().ok_or_else(||
                    "Seed reply field confirming is missing; supply the named summary and prose fields.".to_owned())?,
                checks,
                found,
                resolves: value.get("resolves").cloned(),
            })
        }
        _ => unreachable!("Reply kind checked above"),
    }
}
