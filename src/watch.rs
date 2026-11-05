//! Watch: the runs in the one tink-sdlc checkout the operator named, read through its own
//! list and per-run `status --json` (API 1) when Watch opens or the operator presses `r`. Runs waiting on a
//! person come first; the rest get one quiet line. It never runs verify, mark, decide, stage or
//! any write, discovers no checkout, polls nothing, and refuses an interface or state it doesn't
//! know rather than guess.

#[cfg(test)]
mod tests;

use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use crate::Palette;
use crate::sdlc::{self, DEADLINE, Goal, Read, SCRIPT};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span, Text},
    widgets::{Paragraph, Wrap},
};
use serde_json::Value;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// What tink-sdlc says about a run's verification, from API 1. Anything else is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Blocked,
    Running,
    NotRun,
    Current,
    Stale,
    Failed,
    Interrupted,
    Invalid,
    Older,
}

// Coupled to tink-sdlc's exact wording for a run without a canonical ID. If that text changes,
// such runs fall back to "can't be read", which fails safe.
const OLDER_ID_ERROR: &str =
    "Run run_id must be a canonical UUIDv4 string; create runs with sdlc.py new.";
const OLDER_REASON: &str =
    "This run has no canonical run ID. Its approvals and verification can't be checked here.";

impl State {
    fn parse(word: &str) -> Option<Self> {
        Some(match word {
            "blocked" => Self::Blocked,
            "running" => Self::Running,
            "not-run" => Self::NotRun,
            "current" => Self::Current,
            "stale" => Self::Stale,
            "failed" => Self::Failed,
            "interrupted" => Self::Interrupted,
            "invalid" => Self::Invalid,
            _ => return None,
        })
    }

    /// Whether a person is owed something: a gate decision (tink-sdlc reports `blocked` only
    /// while a gate isn't approved), or a failed, interrupted or unreadable run. An older run
    /// owes nothing: there is no action to take on it.
    fn needs_you(self) -> bool {
        matches!(
            self,
            Self::Blocked | Self::Failed | Self::Interrupted | Self::Invalid
        )
    }

    fn words(self) -> &'static str {
        match self {
            Self::Blocked => "waiting on a decision",
            Self::Running => "running",
            Self::NotRun => "not verified yet",
            Self::Current => "verified",
            Self::Stale => "verification out of date",
            Self::Failed => "verification failed",
            Self::Interrupted => "verification interrupted",
            Self::Invalid => "can't be read",
            Self::Older => "older run",
        }
    }
}

/// One run as the list summary reports it.
#[derive(Debug, Clone, PartialEq)]
struct Run {
    slug: String,
    id: Option<String>,
    state: State,
    /// tink-sdlc's next action, or the reason an invalid run can't be read.
    next: String,
    passed: u64,
    total: u64,
    attention: Option<Attention>,
    goal: Goal,
}

#[derive(Debug, Clone, Default, PartialEq)]
struct Attention {
    gate: bool,
    owed: usize,
    recheck: usize,
    /// What tink-sdlc's pinned build steps say; None when this tink-sdlc doesn't report them.
    worker: Option<Worker>,
}

/// Whether a worker is building the run, from tink-sdlc's steps and their observed turns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Worker {
    Idle,
    Running,
    /// Its turn ended: someone has to read the report and complete the step.
    Ended,
    Unknown,
}

impl Worker {
    fn words(self) -> &'static str {
        match self {
            Self::Idle => "no worker running",
            Self::Running => "worker running",
            Self::Ended => "worker's turn ended",
            Self::Unknown => "worker state unknown",
        }
    }
}

impl Run {
    fn needs_you(&self) -> bool {
        self.state.needs_you()
            || self.attention.as_ref().is_some_and(|a| {
                a.gate || a.owed > 0 || a.recheck > 0 || a.worker == Some(Worker::Ended)
            })
    }

    /// The worker words a row shows: an idle worker matters only while the run awaits building.
    fn worker(&self) -> Option<Worker> {
        let worker = self.attention.as_ref()?.worker?;
        (worker != Worker::Idle || self.state == State::NotRun).then_some(worker)
    }
}

/// One run's detail, from `status <slug> --json`.
#[derive(Debug, Clone, PartialEq)]
struct Detail {
    goal: Goal,
    state: State,
    next: String,
    gates: Vec<(u64, String)>,
    passed: usize,
    proven: usize,
    attested: usize,
    attention: Attention,
    checklist: Vec<String>,
    /// One line per pinned build step, or None when this tink-sdlc doesn't report steps.
    steps: Option<Vec<String>>,
    errors: Vec<String>,
}

fn state(value: &Value) -> Result<State, String> {
    let word = value["verification_status"].as_str().unwrap_or("missing");
    State::parse(word).ok_or_else(|| {
        format!("tink-sdlc reported a state Watch doesn't know ({word}); nothing is shown rather than a guess.")
    })
}

fn text(value: &Value, key: &str) -> Result<String, String> {
    value[key]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("A run is missing {key}; nothing is shown rather than a guess."))
}

/// tink-sdlc's pinned build steps as detail lines, and what they say about a worker.
/// An older tink-sdlc omits them; anything present but unfamiliar is refused, not guessed.
fn steps(run: &Value) -> Result<Option<(Vec<String>, Worker)>, String> {
    let Some(steps) = run.get("steps") else {
        return Ok(None);
    };
    let refuse = || "A worker step Watch doesn't know; nothing is shown rather than a guess.";
    let mut lines = Vec::new();
    let mut worker = Worker::Idle;
    for step in steps.as_array().ok_or_else(refuse)? {
        let name = step["name"].as_str().ok_or_else(refuse)?;
        let state = step["state"].as_str().ok_or_else(refuse)?;
        let turn = step.get("turn").map(Value::as_str);
        let seen = match (state, turn) {
            ("active", Some(Some(turn))) => {
                let now = match turn {
                    "ended" => Worker::Ended,
                    "not-ended" => Worker::Running,
                    "unknown" => Worker::Unknown,
                    _ => return Err(refuse().into()),
                };
                let words = step["observed_turn"].as_str().ok_or_else(refuse)?;
                if worker == Worker::Idle || now == Worker::Ended {
                    worker = now;
                }
                format!(" · {words}")
            }
            ("prepared" | "completed" | "uncertain" | "unreadable", None) => String::new(),
            // A worker stopped before its turn ended is never shown as a finished turn.
            ("stopped", None) => match step["stopped_by"].as_str() {
                Some("operator") => " by you".into(),
                Some("cap") => " at its response cap".into(),
                _ => return Err(refuse().into()),
            },
            _ => return Err(refuse().into()),
        };
        lines.push(format!("Worker step {name}: {state}{seen}"));
    }
    if lines.is_empty() {
        lines.push("No worker step has been started for this run.".into());
    }
    Ok(Some((lines, worker)))
}

/// The list summary, in tink-sdlc's order.
fn parse_list(output: &str) -> Result<Vec<Run>, String> {
    let value = sdlc::envelope(output)?;
    let runs = value["runs"]
        .as_array()
        .ok_or("status printed no runs list.")?;
    runs.iter()
        .map(|run| {
            let state = match state(run)? {
                State::Invalid if run["error"] == OLDER_ID_ERROR => State::Older,
                state => state,
            };
            let count = |key| {
                run["checklist_summary"][key]
                    .as_u64()
                    .ok_or("A run is missing its checklist summary.")
            };
            Ok(Run {
                slug: text(run, "slug")?,
                id: run["run_id"]
                    .as_str()
                    .filter(|id| !id.is_empty())
                    .map(str::to_owned),
                next: if state == State::Older {
                    OLDER_REASON.into()
                } else {
                    text(
                        run,
                        if state == State::Invalid {
                            "error"
                        } else {
                            "next_action"
                        },
                    )?
                },
                passed: count("passed")?,
                total: count("total")?,
                state,
                attention: None,
                goal: sdlc::goal(run),
            })
        })
        .collect()
}

/// One run's detail.
#[cfg(test)]
fn parse_detail(output: &str) -> Result<Detail, String> {
    let value = sdlc::envelope(output)?;
    parse_run_detail(&value["run"])
}

fn parse_run_detail(run: &Value) -> Result<Detail, String> {
    let gates: Vec<_> = run["gates"]
        .as_array()
        .ok_or("The run has no gates list.")?
        .iter()
        .map(|gate| {
            let stage = gate["stage"].as_u64().ok_or("A gate has no stage.")?;
            let status = gate["status"].as_str().unwrap_or("missing");
            if !matches!(
                status,
                "approved" | "pending" | "stale" | "changes-requested"
            ) {
                return Err(format!("A gate has a state Watch doesn't know ({status})."));
            }
            Ok((stage, status.to_owned()))
        })
        .collect::<Result<_, String>>()?;
    let items = run["checklist"]
        .as_array()
        .ok_or("The run has no checklist.")?;
    let count = |key: &str, word: &str| {
        items
            .iter()
            .filter(|i| i[key].as_str() == Some(word))
            .count()
    };
    let steps = steps(run)?;
    let mut attention = Attention {
        gate: gates.iter().any(|(_, status)| status != "approved"),
        worker: steps.as_ref().map(|(_, worker)| *worker),
        ..Attention::default()
    };
    let mut checklist = Vec::new();
    for item in items {
        let automatic = item["automatic"]
            .as_bool()
            .ok_or("A checklist item has no automatic flag.")?;
        let proof = text(item, "proof")?;
        if proof != if automatic { "automated" } else { "attested" } {
            return Err("A checklist item's proof disagrees with its automatic flag.".into());
        }
        let status = text(item, "status")?;
        if !matches!(status.as_str(), "passed" | "pending" | "failed" | "unknown") {
            return Err(format!("Unknown checklist status: {status}"));
        }
        let recheck = item["needs_recheck"]
            .as_bool()
            .ok_or("A checklist item has no needs_recheck flag.")?;
        let pending = match item.get("pending") {
            None => false,
            Some(value) => value
                .as_bool()
                .ok_or("A checklist item has an invalid pending flag.")?,
        };
        if !automatic {
            attention.owed += usize::from(!pending && status != "passed");
            attention.recheck += usize::from(recheck);
        }
        let suffix = if pending {
            " · provisional"
        } else if recheck {
            " · may need recheck"
        } else {
            ""
        };
        checklist.push(format!(
            "{}: {status} ({proof}){suffix}\n{}",
            text(item, "id")?,
            text(item, "verify")?
        ));
        match item.get("mark") {
            Some(Value::Null) => {}
            Some(mark) if mark.is_object() => {
                checklist.push(format!("Evidence: {}", text(mark, "evidence")?))
            }
            _ => return Err("A checklist item has no readable mark.".into()),
        }
    }
    Ok(Detail {
        goal: sdlc::goal(run),
        state: state(run)?,
        next: text(run, "next_action")?,
        gates,
        passed: count("status", "passed"),
        proven: count("proof", "automated"),
        attested: count("proof", "attested"),
        attention,
        checklist,
        steps: steps.map(|(lines, _)| lines),
        errors: run["errors"]
            .as_array()
            .ok_or("The run has no errors list.")?
            .iter()
            .map(|e| {
                e.as_str()
                    .map(str::to_owned)
                    .ok_or("A run error is not text.".to_owned())
            })
            .collect::<Result<_, _>>()?,
    })
}

/// Read only details with the list's original slug and ID; refresh is always explicit.
fn read_run(root: &Path, run: Run, deadline: Duration) -> Load<Detail> {
    let root = root.to_path_buf();
    Load::Reading(Read::task(move |cancel| {
        let output = sdlc::status(
            &root,
            &["status".into(), run.slug.clone(), "--json".into()],
            cancel,
            deadline,
        )?;
        let value = sdlc::envelope(&output)?;
        let detail = &value["run"];
        if run.id.is_none()
            || detail["slug"].as_str() != Some(run.slug.as_str())
            || detail["run_id"].as_str() != run.id.as_deref()
        {
            return Err(
                "Run identity changed or is missing. Press r on the runs screen to refresh.".into(),
            );
        }
        parse_run_detail(detail)
    }))
}

/// Runs that need a person first, then the rest, each group in tink-sdlc's order.
fn ordered(runs: &[Run]) -> Vec<&Run> {
    let (mut needs, quiet): (Vec<&Run>, Vec<&Run>) = runs.iter().partition(|run| run.needs_you());
    needs.extend(quiet);
    needs
}

/// A read that hasn't landed, or what it found and when.
enum Load<T> {
    Reading(Read<T>),
    Done(Result<T, String>, Instant),
}

impl<T: Send + 'static> Load<T> {
    fn start(
        root: &Path,
        args: Vec<String>,
        deadline: Duration,
        parse: fn(&str) -> Result<T, String>,
    ) -> Self {
        Self::Reading(Read::start(root, args, deadline, parse))
    }

    fn poll(&mut self) {
        if let Self::Reading(read) = self
            && let Some(result) = read.poll()
        {
            *self = Self::Done(result, Instant::now());
        }
    }
}

/// Watch's screen: the list, at most one run's detail, and the read behind each.
pub(crate) struct Watch {
    root: Option<PathBuf>,
    list: Option<Load<Vec<Run>>>,
    selected: usize,
    detail: Option<(String, Load<Detail>)>,
    inspection: Option<(usize, Load<Detail>)>,
    scroll: u16,
    page: u16,
    deadline: Duration,
}

impl Watch {
    /// Watch over `root`, reading its runs now; without a root it only says how to name one.
    pub(crate) fn open(root: Option<PathBuf>) -> Self {
        Self::with_deadline(root, DEADLINE)
    }

    fn with_deadline(root: Option<PathBuf>, deadline: Duration) -> Self {
        let mut watch = Self {
            root,
            list: None,
            selected: 0,
            detail: None,
            inspection: None,
            scroll: 0,
            page: 1,
            deadline,
        };
        watch.read_list();
        watch
    }

    /// Source-selected identity for the menu's separate worker controls; this starts nothing.
    pub(crate) fn worker_run(&self) -> Option<crate::worker::Run> {
        let runs = self.runs()?;
        let run = if let Some((slug, _)) = &self.detail {
            runs.iter().find(|run| &run.slug == slug)?
        } else {
            *ordered(runs).get(self.selected)?
        };
        Some(crate::worker::Run {
            root: self.root.clone()?,
            slug: run.slug.clone(),
            id: run.id.clone()?,
        })
    }

    fn read_list(&mut self) {
        self.inspection = None;
        self.selected = 0;
        let Some(root) = &self.root else { return };
        self.list = Some(if root.join(SCRIPT).is_file() {
            Load::start(
                root,
                vec!["status".into(), "--json".into()],
                self.deadline,
                parse_list,
            )
        } else {
            Load::Done(
                Err(format!(
                    "No tink-sdlc in {}: {SCRIPT} is missing.",
                    root.display()
                )),
                Instant::now(),
            )
        });
    }

    /// Collect any finished read; never blocks.
    pub(crate) fn poll(&mut self) {
        if let Some(list) = &mut self.list {
            list.poll();
        }
        if let Some((_, detail)) = &mut self.detail {
            detail.poll();
        }
        self.inspect();
    }

    fn inspect(&mut self) {
        if let Some((_, load)) = &mut self.inspection {
            load.poll();
        }
        if matches!(self.inspection, Some((_, Load::Done(..)))) {
            let Some((index, Load::Done(result, _))) = self.inspection.take() else {
                unreachable!()
            };
            match result {
                Ok(detail) => {
                    if let Some(Load::Done(Ok(runs), _)) = &mut self.list {
                        let run = &mut runs[index];
                        run.state = detail.state;
                        run.next = detail.next;
                        run.passed = detail.passed as u64;
                        run.total = (detail.proven + detail.attested) as u64;
                        run.attention = Some(detail.attention);
                        run.goal = detail.goal;
                    }
                }
                Err(why) => {
                    self.list = Some(Load::Done(
                        Err(format!("Could not determine run attention: {why}")),
                        Instant::now(),
                    ))
                }
            }
        }
        if self.inspection.is_none()
            && let Some(Load::Done(Ok(runs), _)) = &self.list
            && let Some(index) = runs.iter().position(|r| {
                !matches!(r.state, State::Invalid | State::Older) && r.attention.is_none()
            })
            && let Some(root) = &self.root
        {
            self.inspection = Some((index, read_run(root, runs[index].clone(), self.deadline)));
        }
    }

    fn runs(&self) -> Option<&[Run]> {
        match &self.list {
            Some(Load::Done(Ok(runs), _)) if self.inspection.is_none() => Some(runs),
            _ => None,
        }
    }

    /// One key. Returns true when the operator leaves Watch; leaving drops and stops any read.
    pub(crate) fn key(&mut self, key: KeyEvent) -> bool {
        if key.kind == KeyEventKind::Repeat
            && matches!(key.code, KeyCode::Char('r') | KeyCode::Enter)
        {
            return false;
        }
        if let Some((slug, detail)) = &self.detail {
            match key.code {
                KeyCode::Esc => self.detail = None,
                KeyCode::Char('q') => return true,
                KeyCode::Char('r') if matches!(detail, Load::Done(Ok(d), _) if d.state == State::Older) =>
                {
                    self.detail = None;
                    self.read_list();
                }
                KeyCode::Char('r') => self.read_detail(slug.clone()),
                KeyCode::Down | KeyCode::Char('j') => self.scroll = self.scroll.saturating_add(1),
                KeyCode::Up | KeyCode::Char('k') => self.scroll = self.scroll.saturating_sub(1),
                KeyCode::PageDown => self.scroll = self.scroll.saturating_add(self.page),
                KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(self.page),
                KeyCode::Home => self.scroll = 0,
                KeyCode::End => self.scroll = u16::MAX,
                _ => {}
            }
            return false;
        }
        let count = self.runs().map_or(0, <[Run]>::len);
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => return true,
            KeyCode::Char('r') => self.read_list(),
            KeyCode::Down | KeyCode::Char('j') if count > 0 => {
                self.selected = (self.selected + 1).min(count - 1)
            }
            KeyCode::Up | KeyCode::Char('k') => self.selected = self.selected.saturating_sub(1),
            KeyCode::Enter => {
                if let Some(run) = self
                    .runs()
                    .and_then(|runs| ordered(runs).get(self.selected).map(|r| r.slug.clone()))
                {
                    self.read_detail(run);
                }
            }
            _ => {}
        }
        false
    }

    fn read_detail(&mut self, slug: String) {
        self.scroll = 0;
        // An older run's detail comes from the same summary read, so its time is that read's.
        let older = match &self.list {
            Some(Load::Done(Ok(runs), at))
                if runs
                    .iter()
                    .any(|run| run.slug == slug && run.state == State::Older) =>
            {
                Some(*at)
            }
            _ => None,
        };
        if let Some(at) = older {
            self.detail = Some((
                slug,
                Load::Done(
                    Ok(Detail {
                        goal: Goal::Unknown,
                        state: State::Older,
                        next: OLDER_REASON.into(),
                        gates: Vec::new(),
                        passed: 0,
                        proven: 0,
                        attested: 0,
                        attention: Attention::default(),
                        checklist: Vec::new(),
                        steps: None,
                        errors: Vec::new(),
                    }),
                    at,
                ),
            ));
        } else if let Some(root) = &self.root
            && let Some(run) = self
                .runs()
                .and_then(|runs| runs.iter().find(|run| run.slug == slug))
        {
            let read = read_run(root, run.clone(), self.deadline);
            self.detail = Some((slug, read));
        }
    }

    /// Watch's help: what it reads, when, and what it never does.
    pub(crate) fn help_text(&self) -> &'static str {
        concat!(
            "Right now\n↑↓ choose · Enter details · r read again · b worker controls · Esc back · ? help\n\n",
            "What Watch does\nIt reads the runs in the tink-sdlc checkout you named with --sdlc-root, ",
            "by running that checkout's own `python3 _system/scripts/sdlc.py status --json` when you open ",
            "Watch or press r. Watch changes nothing. b opens the separate worker controls; only their named action keys execute work.\n\n",
            "Needs you lists decisions, owed attestations, possible attestation rechecks, workers whose turn ended, ",
            "and failed, interrupted or unreadable runs. Opening or refreshing also reads each run's status to determine attention.\n",
            "Checklist counts separate passed results from how each is checked: automated or by you. ",
            "A proof kind does not mean the item passed.\n",
            "In details: ↑↓, PgUp/PgDn and Home/End scroll the whole evidence and errors."
        )
    }

    /// Paint the list or the open detail into `area`.
    pub(crate) fn render(&mut self, frame: &mut Frame, area: Rect, palette: Palette) {
        let keys = if self.detail.is_some() {
            "↑↓ read   r read again   b worker controls   esc back   ? help"
        } else {
            "↑↓ choose   enter details   r read again   b worker   esc back   ? help"
        };
        let body = Rect::new(area.x, area.y, area.width, area.height.saturating_sub(2));
        frame.render_widget(
            Paragraph::new(keys).style(palette.muted),
            Rect::new(
                area.x,
                area.y + area.height.saturating_sub(1),
                area.width,
                1,
            ),
        );
        let Some(root) = &self.root else {
            frame.render_widget(
                Paragraph::new("Watch reads the runs in one tink-sdlc checkout you name.\n\nStart Tinkery with --sdlc-root PATH to choose it. Nothing is discovered or run until you do.")
                    .wrap(Wrap { trim: false })
                    .style(palette.ink),
                body,
            );
            return;
        };
        if let Some((slug, detail)) = &self.detail {
            let text = detail_text(slug, detail, palette);
            let paragraph = Paragraph::new(text).wrap(Wrap { trim: false });
            self.page = body.height.max(1);
            let max = paragraph
                .line_count(body.width)
                .saturating_sub(body.height as usize)
                .min(u16::MAX as usize) as u16;
            self.scroll = self.scroll.min(max);
            frame.render_widget(paragraph.scroll((self.scroll, 0)), body);
            return;
        }
        if let Some((index, _)) = &self.inspection {
            frame.render_widget(
                Paragraph::new(format!(
                    "Reading run details… {}\n{}",
                    index + 1,
                    root.display()
                ))
                .wrap(Wrap { trim: false }),
                body,
            );
            return;
        }
        let mut lines = vec![
            Line::styled(format!("Runs in {}", root.display()), palette.ink),
            Line::styled(
                source(&["status", "--json"], self.list.as_ref()),
                palette.muted,
            ),
            Line::styled("and status <slug> --json for attention", palette.muted),
            Line::default(),
        ];
        let mut selected_line = 0;
        match &self.list {
            None | Some(Load::Reading(_)) => {
                lines.push(Line::styled("Reading runs…", palette.muted))
            }
            Some(Load::Done(Err(why), _)) => {
                lines.push(Line::styled("! Couldn't read the runs.", palette.rust));
                lines.push(Line::styled(why.clone(), palette.ink));
            }
            Some(Load::Done(Ok(runs), _)) if runs.is_empty() => lines.push(Line::styled(
                "No runs in this checkout yet. tink-sdlc creates them with sdlc.py new.",
                palette.ink,
            )),
            Some(Load::Done(Ok(runs), _)) => {
                let rows = ordered(runs);
                let waiting = rows.iter().filter(|run| run.needs_you()).count();
                lines.push(Line::styled("Needs you", palette.ink.bold()));
                if waiting == 0 {
                    lines.push(Line::styled(
                        "  Nothing needs you right now.",
                        palette.muted,
                    ));
                }
                for (index, run) in rows.iter().enumerate() {
                    if index == waiting {
                        lines.push(Line::default());
                        lines.push(Line::styled("Quiet", palette.ink.bold()));
                    }
                    let mark = if index == self.selected { "› " } else { "  " };
                    let name = if index == self.selected {
                        palette.jade
                    } else {
                        palette.ink
                    };
                    if index == self.selected {
                        selected_line = lines.len();
                    }
                    if run.needs_you() {
                        let worker = run
                            .worker()
                            .map_or(String::new(), |w| format!(" · {}", w.words()));
                        lines.push(Line::from(vec![
                            Span::styled(format!("{mark}{}", run.slug), name),
                            Span::styled(format!("  {}{worker}", run.state.words()), palette.ink),
                        ]));
                        if let Some(a) = &run.attention
                            && (a.owed > 0 || a.recheck > 0)
                        {
                            lines.push(Line::styled(
                                format!(
                                    "    {} attestations owed · {} may need recheck",
                                    a.owed, a.recheck
                                ),
                                palette.ink,
                            ));
                        }
                        lines.push(Line::styled(format!("    {}", run.next), palette.muted));
                        if run.goal != Goal::NoSeed && run.state != State::Older {
                            let first = goal_text(&run.goal).split('\n').next().unwrap_or("");
                            lines.extend(goal_lines(&format!("    {first}"), palette));
                        }
                    } else {
                        let mut line = Line::from(vec![
                            Span::styled(format!("{mark}{}", run.slug), name),
                            Span::styled(
                                format!(
                                    "  {}{} · {}/{} checks passed",
                                    run.state.words(),
                                    run.worker()
                                        .map_or(String::new(), |w| format!(" · {}", w.words())),
                                    run.passed,
                                    run.total
                                ),
                                palette.muted,
                            ),
                        ]);
                        clip_line(&mut line, body.width as usize);
                        lines.push(line);
                    }
                }
            }
        }
        let selected_row = Paragraph::new(Text::from(lines[..selected_line].to_vec()))
            .wrap(Wrap { trim: false })
            .line_count(body.width);
        let paragraph = Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false });
        // Needs-you sub-lines wrap; quiet rows stay clipped to one line. Keep selection visible.
        if matches!(self.list, Some(Load::Done(Err(_), _))) {
            frame.render_widget(paragraph.wrap(Wrap { trim: false }), body);
            return;
        }
        let scroll = (selected_row + 4)
            .saturating_sub(body.height as usize)
            .min(u16::MAX as usize) as u16;
        frame.render_widget(paragraph.scroll((scroll, 0)), body);
    }
}

fn goal_text(goal: &Goal) -> &str {
    match goal {
        Goal::Matched(goal) => goal,
        Goal::NoSeed => "No confirmed seed was recorded for this run.",
        Goal::Unknown => "Goal unknown: the original seed changed or can't be found.",
    }
}

fn clip_line(line: &mut Line<'_>, mut remaining: usize) {
    for span in &mut line.spans {
        let mut bytes = 0;
        for grapheme in span.content.graphemes(true) {
            let width = grapheme.width();
            if width > remaining {
                remaining = 0;
                break;
            }
            remaining -= width;
            bytes += grapheme.len();
        }
        span.content.to_mut().truncate(bytes);
    }
}

fn goal_lines(goal: &str, palette: Palette) -> Vec<Line<'static>> {
    goal.split('\n')
        .map(|line| {
            let safe: String = line
                .chars()
                .flat_map(|c| {
                    if c.is_control()
                        || matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
                    {
                        c.escape_default().collect::<Vec<_>>()
                    } else {
                        vec![c]
                    }
                })
                .collect();
            Line::styled(safe, palette.ink)
        })
        .collect()
}

/// Where the shown words came from, and how fresh they are, to the minute.
fn source<T>(args: &[&str], load: Option<&Load<T>>) -> String {
    let fresh = match load {
        Some(Load::Done(_, at)) => match at.elapsed().as_secs() / 60 {
            0 => " · read just now".to_string(),
            1 => " · read 1 min ago".to_string(),
            minutes => format!(" · read {minutes} min ago"),
        },
        _ => String::new(),
    };
    format!("From python3 {SCRIPT} {}{fresh}", args.join(" "))
}

fn detail_text<'a>(slug: &str, detail: &Load<Detail>, palette: Palette) -> Text<'a> {
    let mut lines = vec![
        Line::styled(slug.to_owned(), palette.jade),
        Line::styled(
            if matches!(detail, Load::Done(Ok(d), _) if d.state == State::Older) {
                source(&["status", "--json"], Some(detail))
            } else {
                source(&["status", slug, "--json"], Some(detail))
            },
            palette.muted,
        ),
        Line::default(),
    ];
    match detail {
        Load::Reading(_) => lines.push(Line::styled("Reading this run…", palette.muted)),
        Load::Done(Err(why), _) => {
            lines.push(Line::styled("! Couldn't read this run.", palette.rust));
            lines.push(Line::styled(why.clone(), palette.ink));
        }
        Load::Done(Ok(d), _) if d.state == State::Older => {
            lines.push(Line::styled(d.state.words(), palette.ink.bold()));
            lines.push(Line::styled(d.next.clone(), palette.ink));
            lines.push(Line::raw(
                "No run files were changed. Esc returns to the runs.",
            ));
        }
        Load::Done(Ok(d), _) => {
            lines.extend(goal_lines(goal_text(&d.goal), palette));
            lines.push(Line::default());
            lines.push(Line::styled(d.state.words(), palette.ink.bold()));
            lines.push(Line::styled(format!("Next: {}", d.next), palette.ink));
            for step in d.steps.iter().flatten() {
                lines.push(Line::styled(step.clone(), palette.ink));
            }
            lines.push(Line::default());
            for (stage, status) in &d.gates {
                lines.push(Line::styled(
                    format!("Stage {stage} gate: {status}"),
                    palette.ink,
                ));
            }
            lines.push(Line::styled(
                format!(
                    "Checklist: {}/{} passed. How each is checked: {} automated, {} by you.",
                    d.passed,
                    d.proven + d.attested,
                    d.proven,
                    d.attested
                ),
                palette.ink,
            ));
            lines.push(Line::styled(
                format!(
                    "{} attestations owed · {} may need recheck",
                    d.attention.owed, d.attention.recheck
                ),
                palette.ink,
            ));
            for item in &d.checklist {
                for line in item.lines() {
                    lines.push(Line::styled(line.to_owned(), palette.ink));
                }
            }
            for error in &d.errors {
                lines.push(Line::styled(format!("! {error}"), palette.rust));
            }
        }
    }
    // Each line costs at least one row, even with no spans; rendered labels count too.
    let bytes: usize = lines
        .iter()
        .map(|line| {
            1 + line
                .spans
                .iter()
                .map(|span| span.content.len())
                .sum::<usize>()
        })
        .sum();
    if bytes >= 60_000 {
        return Text::from(vec![
            Line::styled("! Detail is too long to show completely.", palette.rust),
            Line::raw("Read it in the named tink-sdlc checkout:"),
            Line::raw(format!("python3 {SCRIPT} status <slug> --json")),
            Line::raw("Replace <slug> with the selected run's name."),
            Line::raw("Esc returns to the runs."),
        ]);
    }
    Text::from(lines)
}
