//! Shape: where a saved goal's seed stands, and working through its questions to a seed.
//!
//! Opened on one session (from tend's detail) or as a list of goals waiting for a seed (from the
//! menu). Incomplete lists retain the unreadable count and never claim there are no waiting goals.
//! It shows the confirmed goal's first line, then one thing: the current question, the seed
//! draft, a ready seed, or an honest unknown or failure, with one next step. Opening, scrolling
//! and typing write nothing and call no model. F2 runs the one action the key bar names: save an
//! answer and start the agent step, keep a flagged node with a reason, save a form, or confirm a
//! seed with the operator's own words; see [`step`]. F1 lists every other action allowed now by
//! number ([`choices`]): "So far" (the whole goal exactly as recorded, decisions, open questions
//! and current findings, without writing), defer, delegate, revise, teach-back, the folder a step
//! reads, and ending. The ledger alone owns these rules. Revision saves then looks, asking recorded
//! dependents before any model call; keeping calls no model. Ending a session for good asks once
//! more, then F2 ends it; Esc keeps it open. Each finished step or ending is followed by a fresh
//! read, so the screen shows what the ledger holds. Confirming waits until the whole draft has
//! been on screen. Under an agent launch declaration Shape only reads: Tinkery would record its
//! every change as the operator's.

pub use step::Agent;

mod choices;
mod discovery_help;
mod forms;
mod step;
mod words;

use crate::seed_me::ledger::AnswerAction;
use crate::{
    Palette,
    seed_me::{
        Session, Stage,
        answering::{self, Assumption, Ending, NOTHING_FOUND, OBJECTION, PROBE, Refusal},
        shaping::{self, Shaped, Shaping},
    },
};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use forms::Form;
use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span, Text},
    widgets::{Paragraph, Wrap},
};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, mpsc},
};
use step::{Done, Failure, RetryFeedback, Running, Why, Work};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;
use words::Words;

/// The widest Shape's open view gets, so lines stay readable on a wide terminal.
const WIDTH: u16 = 76;
/// Why an agent launch without a drafting model only reads: its answers would have no agent attribution.
const HELD: &str = "An agent answers Shape with --shape-pi. Shape only reads; Esc goes back.";
/// The only words that confirm a seed draft, as in Think: anything else may be a correction.
const CONFIRM: &str = "confirm";
/// Confirming before the whole seed draft has been on screen.
const READ_FIRST: &str = "PgDn or Down to 'End of seed' first. Nothing was confirmed.";
/// Refusals about the typed words; changing the words clears them.
const WORD_REFUSALS: [&str; 3] = [
    "Type your answer first; nothing was saved.",
    "Type confirm, or F1 Revise the draft. Nothing was confirmed.",
    "Type why you're keeping this answer first; nothing was saved.",
];

/// Shape's screen: an optional list of goals waiting for a seed and, at most, one open session.
pub struct Shape {
    /// `None` when opened on one session; otherwise the list, pending until a read lands.
    list: Option<Option<Result<Vec<Session>, String>>>,
    /// Session records discovery could not read; they may hide goals from the list.
    unreadable: usize,
    failures: Vec<crate::seed_me::ReadFailure>,
    help_scroll: u16,
    actions: crate::actions::Page,
    selected: usize,
    open: Option<Opened>,
    scroll: u16,
    /// Rows of reading on screen at the last draw, for PgUp/PgDn.
    page: u16,
    agent: Option<Arc<Agent>>,
    /// An agent launch without --shape-pi: Shape only reads.
    held: bool,
    /// F1's Model and runs row asked for the Model panel, which the launch owns.
    model: bool,
    approve: Option<crate::seed_me::ready::SeedTarget>,
}

/// What a key asks of whoever holds Shape.
#[derive(Debug, PartialEq, Eq)]
pub enum Leave {
    /// Stay in Shape.
    Stay,
    /// Leave Shape for wherever it was opened from.
    Back,
}

/// A words box opened from F1 over a seed draft.
#[derive(Clone, Copy)]
enum Aside {
    TeachBack,
    Change,
}

struct Opened {
    confirmation: Option<crate::think_record::GoalConfirmation>,
    session: PathBuf,
    reading: Reading,
    /// The operator's words being typed: an answer, or the words that confirm a seed.
    typed: Words,
    /// A deferral or revision form; cancelling restores the untouched answer/reason editor.
    form: Option<Form>,
    /// Unsaved teach-back or change to the draft; cancelling restores the confirmation words untouched.
    aside: Option<(Aside, Words)>,
    /// What the last key or step did, when it needs saying, and its own next step if any.
    notice: Option<(String, Option<String>)>,
    /// The last rejected reply's diagnostic, only for a matching explicit retry in this visit.
    feedback: Option<RetryFeedback>,
    running: Option<Running>,
    /// What the running step was asked to do.
    doing: Doing,
    /// The folder the next step reads.
    folder: Folder,
    /// A folder being typed after choosing it in F1, until F2 uses it or Esc keeps the one shown.
    choosing: Option<Words>,
    /// The "So far" page's scroll while it shows.
    so_far: Option<u16>,
    /// An ending asked for once, waiting for F2 or Esc.
    asking: Option<Ending>,
    /// What the step that asked the current question assumed instead of asking; shown with
    /// that question only.
    assumed: Vec<Assumption>,
    /// After choosing to object, waiting for the number of the assumption, or Esc.
    objecting: bool,
    /// A hard-to-undo question's suggestion its step didn't record, until F1 asks for it.
    withheld: Option<String>,
    /// Whether F1 showed a hard-to-undo question's options and suggestion.
    revealed: bool,
    /// A change without a model (an ending or an objection) the ledger is carrying out.
    writing: Option<(Write, mpsc::Receiver<Result<(), Refusal>>)>,
    /// The exact draft revision whose last line has been on screen; layout changes don't revoke it.
    read_end: Option<Shaped>,
}

/// A change Shape asks of the ledger without a model.
#[derive(Clone)]
enum Write {
    Revisit {
        id: String,
        expected: u64,
        words: String,
    },
    End(Ending),
    Object(String),
    Keep {
        id: String,
        reason: String,
    },
    /// Record the withheld suggestion on question `id`.
    Recommend {
        id: String,
        text: String,
    },
}

/// What a running step was asked to do, so the screen can say where it is.
#[derive(Clone, Copy)]
enum Doing {
    Answer,
    Defer,
    Delegate,
    Revise,
    Look,
    TeachBack,
    Change,
    Confirm,
}

impl Doing {
    fn saved(self) -> &'static str {
        match self {
            Self::Defer => "Your deferral is saved. ",
            Self::Delegate => "Your delegated answer is saved. ",
            Self::Revise => "Your revised answer is saved. ",
            Self::TeachBack => "Your teach-back is saved. ",
            Self::Change => "Your change is saved. ",
            _ => "Your answer is saved. ",
        }
    }
}

/// Which folder the next step reads: not known until a read with an agent lands, then where
/// Think says the goal was made, or the operator's pick.
enum Folder {
    Unknown,
    Known(PathBuf),
}

/// A read of the session, with the folder a step would read when there's an agent. Both land
/// together, so a step is never offered before its folder is shown.
#[expect(
    clippy::large_enum_variant,
    reason = "Shape holds one reading at a time; boxing would only add indirection"
)]
enum Reading {
    NotStarted,
    Waiting(mpsc::Receiver<(Result<Shaping, String>, Option<PathBuf>)>),
    Done(Result<Shaping, String>),
}

impl Shape {
    /// Shape opened on one session; its first [`Shape::tick`] reads the ledger off the paint
    /// path, or it says plainly that Shape isn't configured.
    pub fn session(session: &Path) -> Self {
        Self {
            list: None,
            unreadable: 0,
            failures: Vec::new(),
            help_scroll: 0,
            actions: crate::actions::Page::default(),
            selected: 0,
            open: Some(Opened::start(session)),
            scroll: 0,
            page: 1,
            agent: None,
            held: false,
            model: false,
            approve: None,
        }
    }

    /// The same Shape, able to answer and confirm through `agent`. Without one, it cannot save
    /// answers, confirm seeds or call a model; ending remains available.
    pub fn with_agent(mut self, agent: Option<Arc<Agent>>) -> Self {
        self.agent = agent.filter(|_| !self.held);
        self
    }

    /// The same Shape for an agent launch without agent attribution for its writes: it only reads.
    /// Answering, confirming and ending are held, and explain what is missing.
    pub fn held(mut self, held: bool) -> Self {
        self.held = held;
        if held {
            self.agent = None;
        }
        self
    }

    /// Whether F1's Model and runs row was picked since the last ask. Asking clears it.
    pub fn take_model_request(&mut self) -> bool {
        std::mem::take(&mut self.model)
    }

    /// Take the deliberate Ready action's exact seed once; routing starts no write or model.
    pub(crate) fn take_approve_request(&mut self) -> Option<crate::seed_me::ready::SeedTarget> {
        self.approve.take()
    }

    /// Reenter this same session through its normal asynchronous read, keeping words and context.
    pub(crate) fn reread(&mut self) {
        if let Some(open) = &mut self.open {
            open.reading = Reading::NotStarted;
        }
    }

    /// Whether keys now type words (an answer, seed confirmation or folder), so letters aren't
    /// commands. Never while an ending or objection asks, or the helper carries one out.
    pub fn takes_text(&self) -> bool {
        self.agent.is_some()
            && self.open.as_ref().is_some_and(|open| {
                open.so_far.is_none()
                    && open.running.is_none()
                    && open.asking.is_none()
                    && !open.objecting
                    && open.writing.is_none()
                    && (open.choosing.is_some() || open.typing().is_some())
            })
    }

    /// Bracketed paste: the words go into the folder, answer or confirmation exactly as pasted.
    pub fn paste(&mut self, text: &str) {
        if self.takes_text()
            && let Some(open) = &mut self.open
        {
            match (&mut open.choosing, &mut open.form) {
                (Some(folder), _) => folder.insert(text),
                (_, Some(form)) => form.words().insert(text),
                (None, None) if open.aside.is_some() => open.aside.as_mut().unwrap().1.insert(text),
                (None, None) => {
                    let before = open.typed.text().to_owned();
                    open.typed.insert(text);
                    open.edited(&before);
                }
            }
        }
    }

    /// Shape opened from the menu: a list of goals waiting for a seed, pending until [`Shape::offer`].
    pub fn list() -> Self {
        Self {
            list: Some(None),
            unreadable: 0,
            failures: Vec::new(),
            help_scroll: 0,
            actions: crate::actions::Page::default(),
            selected: 0,
            open: None,
            scroll: 0,
            page: 1,
            agent: None,
            held: false,
            model: false,
            approve: None,
        }
    }

    /// Whether the list still waits on a read of the sessions.
    pub fn wants_sessions(&self) -> bool {
        matches!(self.list, Some(None))
    }

    /// Supply readable sessions and the count of unreadable records, or the root failure.
    /// Only goals waiting for a seed are listed. Failed reads discard any earlier count.
    pub fn offer(&mut self, read: Result<&[Session], String>, unreadable: usize) {
        if self.list.is_some() {
            self.failures.clear();
            self.help_scroll = 0;
            self.unreadable = if read.is_ok() { unreadable } else { 0 };
            let listed: Result<Vec<Session>, String> = read.map(|sessions| {
                sessions
                    .iter()
                    .filter(|s| matches!(s.stage, Stage::GoalSaved | Stage::SeedDrafted))
                    .cloned()
                    .collect()
            });
            let count = listed.as_ref().map_or(0, Vec::len);
            self.selected = self.selected.min(count.saturating_sub(1));
            self.list = Some(Some(listed));
        }
    }

    /// Take a finished read or step, if one landed. A finished step starts a fresh read. Never
    /// blocks.
    pub fn tick(&mut self) {
        if let Some(open) = &mut self.open
            && let Some(result) = open.running.as_ref().and_then(Running::poll)
        {
            open.running = None;
            // Typed words stay only when nothing of them was saved.
            if result
                .as_ref()
                .map_or_else(|failure| failure.answer_saved, |_| true)
            {
                open.aside = None;
                if !matches!(open.doing, Doing::TeachBack) {
                    open.typed.clear();
                    open.form = None;
                }
            }
            let asked = match &result {
                Ok(Done::Asked(asked)) => asked.clone(),
                _ => step::Asked::default(),
            };
            open.assumed = asked.assumed;
            open.withheld = asked.withheld;
            open.revealed = false;
            open.feedback = result
                .as_ref()
                .err()
                .and_then(|failure| failure.feedback.as_deref().cloned());
            open.notice = said(result, open.doing, self.agent.as_deref().map(Agent::left));
            open.reading = Reading::NotStarted;
        }
        if let Some(open) = &mut self.open
            && let Some((write, receiver)) = &open.writing
        {
            let write = write.clone();
            let result = match receiver.try_recv() {
                Ok(result) => Some(result),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => {
                    Some(Err(Refusal::Refused("Ending stopped unexpectedly.".into())))
                }
            };
            if let Some(result) = result {
                open.writing = None;
                open.notice = Some(match (result, write) {
                    (Ok(()), Write::Revisit { .. }) => {
                        open.form = None;
                        open.typed.clear();
                        ("Deferred prerequisite answered. No model ran.".into(), None)
                    }
                    (Ok(()), Write::End(Ending::Completed)) => {
                        ("Session completed. It's read-only now.".into(), None)
                    }
                    (Ok(()), Write::End(Ending::Stopped)) => (
                        "Session stopped. Its open questions stay recorded; it's read-only now."
                            .into(),
                        None,
                    ),
                    (Ok(()), Write::Object(text)) => {
                        open.assumed.retain(|a| a.text != text);
                        (
                            format!(
                                "Not assumed: {text}. It's an open question now, asked before the seed is drafted."
                            ),
                            None,
                        )
                    }
                    (Ok(()), Write::Keep { .. }) => {
                        open.typed.clear();
                        ("Answer kept with your reason. No model ran.".into(), None)
                    }
                    (Ok(()), Write::Recommend { .. }) => {
                        open.withheld = None;
                        ("Here are the options and my suggestion.".into(), None)
                    }
                    (Err(refusal), _) => (refusal.to_string(), None),
                });
                open.reading = Reading::NotStarted;
            }
        }
        if let Some(open) = &mut self.open
            && let Reading::NotStarted = open.reading
        {
            open.begin(self.agent.clone());
        }
        if let Some(open) = &mut self.open
            && let Reading::Waiting(receiver) = &open.reading
        {
            match receiver.try_recv() {
                Ok((result, folder)) => {
                    if result.as_ref().ok().map(|read| &read.stage) != open.read_end.as_ref() {
                        open.read_end = None;
                    }
                    open.reading = Reading::Done(result);
                    if let (Folder::Unknown, Some(folder)) = (&open.folder, folder) {
                        open.folder = Folder::Known(folder);
                    }
                    if !matches!((&open.reading, &open.folder),
                        (Reading::Done(Ok(Shaping { stage: Shaped::Waiting, version: Some(version), .. })),
                            Folder::Known(folder)) if open.feedback.as_ref().is_some_and(|feedback|
                                feedback.applies(&open.session, *version, folder)))
                    {
                        open.feedback = None;
                    }
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    open.feedback = None;
                    open.reading = Reading::Done(Err("The read stopped unexpectedly.".into()))
                }
            }
        }
    }

    /// One key, opening only the selected session when requested. Esc stops a running step, or closes an
    /// open session back to the list, or leaves Shape. F2 runs the one action the key bar names:
    /// save an answer and look, keep with a reason, save a form or teach-back, use a folder,
    /// confirm the displayed seed with typed words, or end a session after it asked. Every other
    /// action is a row in F1 ([`Shape::pick`]). No other function key does anything.
    pub fn key(&mut self, key: KeyEvent) -> Leave {
        if key.kind == KeyEventKind::Release
            || matches!(key.code, KeyCode::F(n) if n != 2 || key.kind == KeyEventKind::Repeat)
        {
            return Leave::Stay;
        }
        let page = crate::page(self.page);
        if let Some(open) = &mut self.open {
            if let Some(scroll) = &mut open.so_far {
                match key.code {
                    KeyCode::Esc => open.so_far = None,
                    KeyCode::Down | KeyCode::Char('j') => *scroll = scroll.saturating_add(1),
                    KeyCode::Up | KeyCode::Char('k') => *scroll = scroll.saturating_sub(1),
                    KeyCode::PageDown => *scroll = scroll.saturating_add(page),
                    KeyCode::PageUp => *scroll = scroll.saturating_sub(page),
                    _ => {}
                }
                return Leave::Stay;
            }
            if self.held && key.code == KeyCode::F(2) {
                open.notice = Some((HELD.into(), None));
                return Leave::Stay;
            }
            if open.running.is_some() {
                if key.code == KeyCode::Esc
                    && let Some(running) = &open.running
                {
                    running.stop();
                }
                return Leave::Stay;
            }
            if let Some(folder) = &mut open.choosing {
                match key.code {
                    KeyCode::F(2) if key.modifiers.is_empty() => {
                        let typed = open.choosing.take().unwrap();
                        open.notice = open.pick(typed.text());
                    }
                    KeyCode::Esc => open.choosing = None,
                    KeyCode::PageDown => self.scroll = self.scroll.saturating_add(page),
                    KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(page),
                    _ => folder.key(key),
                }
                return Leave::Stay;
            }
            if open.writing.is_some() {
                return Leave::Stay;
            }
            if let Some(form) = &mut open.form {
                match key.code {
                    KeyCode::Esc => {
                        open.form = None;
                        open.notice = Some(("Nothing was changed.".into(), None));
                    }
                    KeyCode::Tab => form.toggle(),
                    KeyCode::PageDown => self.scroll = self.scroll.saturating_add(page),
                    KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(page),
                    KeyCode::F(2) if key.modifiers.is_empty() => {
                        if form.advance()
                            && let (Some(agent), Some(typing)) = (&self.agent, open.typing())
                        {
                            open.act(typing.act, agent);
                        }
                    }
                    KeyCode::F(_) => {}
                    _ => form.words().key(key),
                }
                return Leave::Stay;
            }
            if let Some((aside, _)) = open.aside.take_if(|_| key.code == KeyCode::Esc) {
                let what = match aside {
                    Aside::TeachBack => "Teach-back",
                    Aside::Change => "Change",
                };
                open.notice = Some((format!("{what} cancelled. Nothing was saved."), None));
                return Leave::Stay;
            }
            if open.objecting {
                match key.code {
                    KeyCode::Esc => {
                        open.objecting = false;
                        open.notice = Some(("Nothing was changed.".into(), None));
                    }
                    KeyCode::Char(c) if key.modifiers.is_empty() => {
                        if let Some(text) = c
                            .to_digit(10)
                            .and_then(|n| (n as usize).checked_sub(1))
                            .and_then(|i| open.assumed.get(i))
                            .map(|a| a.text.clone())
                        {
                            open.object(text);
                        }
                    }
                    _ => {}
                }
                return Leave::Stay;
            }
            if let Some(asked) = open.asking {
                match key.code {
                    KeyCode::F(2) if key.modifiers.is_empty() => open.end(asked),
                    KeyCode::Esc => {
                        open.asking = None;
                        open.notice = Some(("Nothing was ended.".into(), None));
                    }
                    _ => {}
                }
                return Leave::Stay;
            }
            if let (Some(agent), Some(typing)) = (&self.agent, open.typing()) {
                match key.code {
                    KeyCode::F(2) if key.modifiers.is_empty() => {
                        open.act(typing.act, agent);
                        return Leave::Stay;
                    }
                    // Confirmation words are one line, so Up and Down scroll the seed being
                    // read, for terminals that take PgUp/PgDn (#614).
                    KeyCode::Down if matches!(typing.act, Act::Confirm { .. }) => {
                        self.scroll = self.scroll.saturating_add(1);
                        return Leave::Stay;
                    }
                    KeyCode::Up if matches!(typing.act, Act::Confirm { .. }) => {
                        self.scroll = self.scroll.saturating_sub(1);
                        return Leave::Stay;
                    }
                    KeyCode::Enter if matches!(typing.act, Act::Confirm { .. }) => {
                        open.notice = Some((
                            "F2 confirms the seed; Enter does not add a line.".into(),
                            None,
                        ));
                        return Leave::Stay;
                    }
                    KeyCode::Esc | KeyCode::F(_) | KeyCode::PageDown | KeyCode::PageUp => {}
                    _ => {
                        if let Some((_, words)) = &mut open.aside {
                            words.key(key);
                            return Leave::Stay;
                        }
                        let before = open.typed.text().to_owned();
                        open.typed.key(key);
                        open.edited(&before);
                        return Leave::Stay;
                    }
                }
            }
            if key.code == KeyCode::F(2)
                && let Some(agent) = &self.agent
                && let Reading::Done(Ok(Shaping {
                    stage: Shaped::Waiting,
                    version: Some(expected),
                    ..
                })) = &open.reading
            {
                let expected = *expected;
                open.act(Act::Look(expected), agent);
                return Leave::Stay;
            }
            match key.code {
                KeyCode::Esc if self.list.is_some() => {
                    // A step may have moved this goal on (a confirmed seed leaves the list).
                    self.open = None;
                    self.list = Some(None);
                    self.unreadable = 0;
                    self.failures.clear();
                    self.help_scroll = 0;
                    self.scroll = 0;
                }
                KeyCode::Esc => return Leave::Back,
                KeyCode::Down | KeyCode::Char('j') => self.scroll = self.scroll.saturating_add(1),
                KeyCode::Up | KeyCode::Char('k') => self.scroll = self.scroll.saturating_sub(1),
                KeyCode::PageDown => self.scroll = self.scroll.saturating_add(page),
                KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(page),
                _ => {}
            }
            return Leave::Stay;
        }
        let listed = match &self.list {
            Some(Some(Ok(sessions))) => sessions.as_slice(),
            _ => &[],
        };
        match key.code {
            KeyCode::Esc => return Leave::Back,
            KeyCode::Down | KeyCode::Char('j') if !listed.is_empty() => {
                self.selected = (self.selected + 1).min(listed.len() - 1)
            }
            KeyCode::Up | KeyCode::Char('k') => self.selected = self.selected.saturating_sub(1),
            KeyCode::Enter if !listed.is_empty() => {
                let path = listed[self.selected].path.clone();
                self.open = Some(Opened::start(&path));
                self.scroll = 0;
            }
            _ => {}
        }
        Leave::Stay
    }

    /// Help for what is on screen now: what to do, then how it works with the paths it reads.
    pub fn help_text(&self) -> String {
        let agent = self.agent.as_deref();
        let back = match &self.open {
            Some(open) if open.running.is_some() => "Esc stop",
            Some(Opened {
                aside: Some((Aside::TeachBack, _)),
                ..
            }) => "Esc cancel teach-back",
            Some(Opened {
                aside: Some((Aside::Change, _)),
                ..
            }) => "Esc cancel change",
            Some(open) if open.form.is_some() => "Esc cancel form",
            _ => "Esc back",
        };
        let (now, how) = match &self.open {
            Some(open) => {
                let failure = match &open.reading {
                    Reading::Done(Err(error)) => format!("What went wrong:\n{error}\n\n"),
                    _ => String::new(),
                };
                let model = agent.map_or_else(
                    || {
                        if self.held {
                            HELD
                        } else {
                            "No model is set up, so Shape only reads."
                        }
                        .into()
                    },
                    |a| {
                        let step = if open.offers_teach_back() {
                            "Each teach-back comparison uses one drafting run, without repository access."
                        } else {
                            "Asking the model uses one drafting run. Read-only tools may need several provider requests; manual actions may use none."
                        };
                        format!(
                            "{} {step}\n\
                             Tinkery saves every change; the model never writes the session.",
                            a.left()
                        )
                    },
                );
                let reads = match (agent, &open.folder) {
                    (Some(_), Folder::Known(folder)) => {
                        format!("\nThe next step reads files in: {}", folder.display())
                    }
                    _ => String::new(),
                };
                let now = match (agent, open.typing()) {
                    _ if open.running.is_some() => "A step is running. Esc stops it.",
                    _ if open.form.is_some() => {
                        "Edit the form. Enter adds a line; Esc cancels without saving."
                    }
                    _ if open.writing.is_some() => "Tinkery is saving a change.",
                    _ if open.objecting => {
                        "Press the number of the assumption you object to; it becomes an open question. Esc keeps them all."
                    }
                    _ if open.asking == Some(Ending::Completed) => {
                        "F2 ends this session as completed, for good; Esc keeps it open."
                    }
                    _ if open.asking.is_some() => {
                        "F2 stops this session for good; Esc keeps it open."
                    }
                    _ if open.choosing.is_some() => {
                        "Type the folder the next step should read. Enter adds a line; F2 uses it; Esc keeps the one shown."
                    }
                    (
                        Some(_),
                        Some(Typing {
                            act: Act::TeachBack { .. },
                            ..
                        }),
                    ) => {
                        "Say what will and won't be built in your own words. F2 saves and compares (one drafting run); Esc cancels. Mismatches are guesses and never block confirming."
                    }
                    (
                        Some(_),
                        Some(Typing {
                            act: Act::Change { .. },
                            ..
                        }),
                    ) => {
                        "Say what should change in the draft. F2 saves your words as your decision and redrafts (one drafting run); Esc cancels. Nothing is confirmed."
                    }
                    (
                        Some(_),
                        Some(Typing {
                            act: Act::Keep { .. },
                            ..
                        }),
                    ) => {
                        "Type why you're keeping this answer. F2 keeps it unchanged with your reason; no model runs."
                    }
                    (
                        Some(_),
                        Some(Typing {
                            act: Act::Answer { .. },
                            ..
                        }),
                    ) => {
                        concat!(
                            "Type your answer. Enter adds a line. F2 saves it and looks into the next question. A model step uses one drafting run. ",
                            "PgUp/PgDn scroll the reading."
                        )
                    }
                    (
                        Some(_),
                        Some(Typing {
                            act: Act::Confirm { .. },
                            ..
                        }),
                    ) => {
                        concat!(
                            "Read the seed, type confirm, then F2 confirms it. To change the draft instead, ",
                            "F1 Revise the draft. It is not approval to build. ",
                            "Without a completed teach-back, confirming records: Skipped teach-back."
                        )
                    }
                    (Some(_), Some(_)) => {
                        "Edit the form. Enter adds a line; Esc cancels without saving."
                    }
                    _ => "Read where this goal's seed stands. ↑↓ or PgUp/PgDn scroll.",
                };
                let form = open
                    .form
                    .as_ref()
                    .map(|f| format!("\n{}", f.description()))
                    .unwrap_or_default();
                (
                    now,
                    format!(
                        "{failure}{model}{reads}{form}\nSession: {}",
                        open.session.display()
                    ),
                )
            }
            None => (
                "↑↓ / j k choose a goal waiting for a seed · Enter opens it.",
                match &self.list {
                    Some(Some(Err(error))) => format!("What went wrong:\n{error}"),
                    Some(Some(Ok(_))) if self.unreadable > 0 => self.discovery_help(),
                    _ => "Goals whose seed is ready, ended or unknown are in tend.".into(),
                },
            ),
        };
        let promise = if self.held {
            "Shape reads and calls no model. An agent answers Shape with --shape-pi; Esc goes back."
                .to_owned()
        } else if agent.is_some() {
            "Opening, scrolling and typing write nothing. F2 runs the one action the key bar names; F1 lists the rest by number. \
                 Saving an answer, a deferral or a delegation then looks into the next; only F2 on a drafted seed confirms it, with your words. \
                 Revision saves and looks, asking recorded dependents before a model call. \
                 Ending a session, from F1, asks once more, then ends it for good."
                .to_owned()
        } else {
            "Shape reads and calls no model; its only write is ending a session, from F1, which asks once more first. \
                 To set up Shape, press Esc until the menu, then m: choose a model and runs, and s saves them. \
                 Then q quits; start Tinkery again, keeping the same --seed-session-root if you supplied one. \
                 Open Tend (2), select this saved goal, press Enter, then s to reopen Shape."
                .to_owned()
        };
        let editing = if self.takes_text() {
            "\nArrows, Home/End and Ctrl+Left/Right move; Shift selects; Ctrl+W or Alt+Backspace deletes a word.\n\
             Ctrl+U undoes, Ctrl+R redoes; PgUp/PgDn scroll the reading."
        } else {
            ""
        };
        // The numbered actions come first, so even a short panel shows them without scrolling.
        let rows = self.actions.text(&self.rows());
        let rows = if rows.is_empty() {
            rows
        } else {
            format!("{rows}\n\n")
        };
        format!(
            "Right now\n{rows}{now}{editing}\n{back}\n\nF1 close\n\nHow it works\n{how}\n\n{promise}"
        )
    }

    /// Draws Shape in `area`, with its key bar on the terminal's last row. The "So far" page,
    /// like help, fills the body and has no key bar.
    pub fn render(&mut self, frame: &mut Frame, area: Rect, palette: Palette) {
        if let Some(Opened {
            so_far: Some(scroll),
            reading: Reading::Done(Ok(shaping)),
            ..
        }) = &mut self.open
        {
            self.page = area.height;
            render_so_far(frame, scroll, shaping, area, palette);
            return;
        }
        let agent = self.agent.is_some();
        let overflow = match &mut self.open {
            Some(open) => {
                let (overflow, page) = render_open(
                    frame,
                    &mut self.scroll,
                    open,
                    agent,
                    self.held,
                    area,
                    palette,
                );
                self.page = page;
                overflow
            }
            None => {
                self.render_list(frame, area, palette);
                false
            }
        };
        let keys = match (&self.open, &self.list) {
            (Some(open), _) if open.running.is_some() => "esc stop".to_owned(),
            (Some(open), _) if open.writing.is_some() => String::new(),
            (Some(open), _) if open.objecting => {
                "1-9 object to that one   esc keep them all".to_owned()
            }
            (
                Some(Opened {
                    asking: Some(ending),
                    ..
                }),
                _,
            ) => match ending {
                Ending::Completed => "F2 end for good   esc keep it".to_owned(),
                Ending::Stopped => "F2 stop for good   esc keep it".to_owned(),
            },
            (Some(open), _) if open.form.is_some() => {
                format!("{}   esc cancel", open.form.as_ref().unwrap().bar())
            }
            (Some(open), _) if open.choosing.is_some() => {
                format!(
                    "{}enter newline   F2 use folder   esc cancel",
                    if overflow { "pgup/dn read   " } else { "" }
                )
            }
            (Some(open), _) => {
                let action = match (&self.agent, open.typing(), &open.reading) {
                    (
                        Some(_),
                        Some(Typing {
                            act: Act::TeachBack { .. },
                            ..
                        }),
                        _,
                    ) => Some("F2 save + compare".to_owned()),
                    (
                        Some(_),
                        Some(Typing {
                            act: Act::Change { .. },
                            ..
                        }),
                        _,
                    ) => Some("F2 save + redraft".to_owned()),
                    (
                        Some(_),
                        Some(Typing {
                            act: Act::Keep { .. },
                            ..
                        }),
                        _,
                    ) => Some("F2 keep with reason".to_owned()),
                    (
                        Some(_),
                        Some(Typing {
                            act: Act::Answer { .. },
                            ..
                        }),
                        _,
                    ) => Some(
                        if overflow || open.offers_answers() {
                            "F2 save + look"
                        } else {
                            "F2 save answer and look into next"
                        }
                        .to_owned(),
                    ),
                    (
                        Some(_),
                        Some(Typing {
                            act: Act::Confirm { revision: r, .. },
                            ..
                        }),
                        _,
                    ) if open.read_end.is_some() => Some(format!("F2 confirm seed {r}")),
                    (
                        Some(_),
                        None,
                        Reading::Done(Ok(Shaping {
                            stage: Shaped::Waiting,
                            ..
                        })),
                    ) => Some("F2 look into the next question".to_owned()),
                    _ => None,
                };
                let mut keys = Vec::new();
                if overflow {
                    keys.push(
                        if agent
                            && open.read_end.is_none()
                            && matches!(
                                open.typing(),
                                Some(Typing {
                                    act: Act::Confirm { .. },
                                    ..
                                })
                            )
                        {
                            "pgup/dn read to end"
                        } else if open.typing().is_some() && agent {
                            "pgup/dn read"
                        } else {
                            "↑↓ scroll"
                        }
                        .to_owned(),
                    );
                }
                if let Some(action) = action {
                    keys.push(action);
                }
                if !self.rows().is_empty() {
                    keys.push("F1 more".to_owned());
                }
                keys.push(
                    if open.aside.is_some() {
                        "esc cancel"
                    } else {
                        "esc back"
                    }
                    .to_owned(),
                );
                // Esc is never cut off: a full bar closes up before it runs out of room.
                let bar = keys.join("   ");
                if bar.width() > frame.area().width.saturating_sub(4) as usize {
                    let tight = keys.join("  ");
                    if tight.width() > frame.area().width.saturating_sub(4) as usize {
                        tight
                            .replace("pgup/dn read", "pgup/dn")
                            .replace("F2 save + look", "F2 answer")
                    } else {
                        tight
                    }
                } else {
                    bar
                }
            }
            (None, Some(Some(Ok(sessions)))) if !sessions.is_empty() => {
                "↑↓ move   enter open   esc back".into()
            }
            _ => "esc back".into(),
        };
        frame.render_widget(
            Paragraph::new(keys).style(palette.muted),
            Rect::new(
                2,
                frame.area().height.saturating_sub(1),
                frame.area().width.saturating_sub(4),
                1,
            ),
        );
    }

    fn render_list(&self, frame: &mut Frame, mut area: Rect, palette: Palette) {
        if self.unreadable > 0 && matches!(&self.list, Some(Some(Ok(_)))) {
            let records = if self.unreadable == 1 {
                "record"
            } else {
                "records"
            };
            let notice = format!(
                "{} {records} couldn't be read. F1 shows why.",
                self.unreadable
            );
            let height = area.height.min(2);
            frame.render_widget(
                Paragraph::new(notice)
                    .style(palette.muted)
                    .wrap(Wrap { trim: false }),
                Rect::new(area.x, area.y, area.width, height),
            );
            area.y += height;
            area.height -= height;
        }
        let message = match &self.list {
            Some(Some(Ok(sessions))) if !sessions.is_empty() => {
                let width = area.width as usize;
                let lines: Vec<Line> = sessions
                    .iter()
                    .enumerate()
                    .map(|(i, session)| {
                        let label = session.stage.label();
                        let mark = if i == self.selected { "› " } else { "  " };
                        let budget = width.saturating_sub(label.width() + 4);
                        let title = clipped(session.goal.lines().next().unwrap_or(""), budget);
                        let gap = width.saturating_sub(2 + title.width() + label.width());
                        let style = if i == self.selected {
                            palette.jade
                        } else {
                            palette.ink
                        };
                        Line::from(vec![
                            Span::styled(format!("{mark}{title}{}", " ".repeat(gap)), style),
                            Span::styled(label.to_owned(), palette.muted),
                        ])
                    })
                    .collect();
                let top = (self.selected as u16).saturating_sub(area.height.saturating_sub(1));
                frame.render_widget(Paragraph::new(lines).scroll((top, 0)), area);
                return;
            }
            Some(Some(Ok(_))) if self.unreadable > 0 => {
                "Nothing can be listed from the readable sessions. Esc goes back."
            }
            Some(Some(Ok(_))) => {
                "No goals are waiting for a seed. Confirm a goal in think, then shape it here."
            }
            Some(Some(Err(_))) => "Your sessions couldn't be read. F1 shows why. Esc goes back.",
            _ => "Reading your sessions. Esc goes back.",
        };
        frame.render_widget(
            Paragraph::new(message)
                .style(palette.muted)
                .wrap(Wrap { trim: false }),
            area,
        );
    }
}

/// What a key asks of an open session.
#[derive(Clone)]
enum Act {
    Respond {
        id: String,
        expected: u64,
        action: AnswerAction,
    },
    Keep {
        id: String,
    },
    /// Save the typed answer to question `id`, read at `expected`, then look into the next.
    Answer {
        id: String,
        expected: u64,
    },
    /// Look into the next question from `expected`.
    Look(u64),
    /// Compare typed teach-back with the exact displayed draft.
    TeachBack {
        revision: String,
        expected: u64,
        seed: String,
    },
    /// Save the typed change to seed draft `revision`, read at `expected`, then redraft.
    Change {
        revision: String,
        expected: u64,
    },
    /// Confirm seed `revision` with the typed word `confirm`.
    Confirm {
        revision: String,
        expected: u64,
        seed: String,
    },
}

/// The words box an open stage offers, and the key that acts on them.
struct Typing {
    act: Act,
    label: &'static str,
}

impl Opened {
    /// The words box this stage offers, if any: an answer to the current question, or the words
    /// that confirm a seed draft.
    fn typing(&self) -> Option<Typing> {
        let Reading::Done(Ok(shaping)) = &self.reading else {
            return None;
        };
        if let (
            Some((aside, _)),
            Shaped::Drafted {
                revision: Some(revision),
                text: Some(seed),
            },
            Some(expected),
        ) = (&self.aside, &shaping.stage, shaping.version)
        {
            return Some(match aside {
                Aside::TeachBack => Typing {
                    act: Act::TeachBack {
                        revision: revision.clone(),
                        expected,
                        seed: seed.clone(),
                    },
                    label: "your teach-back",
                },
                Aside::Change => Typing {
                    act: Act::Change {
                        revision: revision.clone(),
                        expected,
                    },
                    label: "your change",
                },
            });
        }
        if let Some(form) = &self.form {
            return Some(Typing {
                act: Act::Respond {
                    id: form.id.clone(),
                    expected: form.expected,
                    action: form.action(),
                },
                label: form.label(),
            });
        }
        match (&shaping.stage, shaping.version) {
            (Shaped::Review { id, .. }, Some(_)) => Some(Typing {
                act: Act::Keep { id: id.clone() },
                label: "your reason",
            }),
            (Shaped::Question { id, .. }, Some(expected)) => Some(Typing {
                act: Act::Answer {
                    id: id.clone(),
                    expected,
                },
                label: "your answer",
            }),
            (
                Shaped::Drafted {
                    revision: Some(revision),
                    text: Some(seed),
                },
                Some(expected),
            ) => Some(Typing {
                act: Act::Confirm {
                    revision: revision.clone(),
                    expected,
                    seed: seed.clone(),
                },
                label: "your words",
            }),
            _ => None,
        }
    }

    fn offers_teach_back(&self) -> bool {
        matches!(
            &self.reading,
            Reading::Done(Ok(Shaping {
                stage: Shaped::Drafted {
                    revision: Some(_),
                    text: Some(_)
                },
                version: Some(_),
                ..
            }))
        )
    }

    /// F1 actions are unavailable while another form, view or operation owns the session.
    fn offers_choices(&self) -> bool {
        self.running.is_none()
            && self.writing.is_none()
            && self.form.is_none()
            && self.choosing.is_none()
            && self.aside.is_none()
            && !self.objecting
            && self.asking.is_none()
            && self.so_far.is_none()
    }

    fn offers_answers(&self) -> bool {
        matches!(
            &self.reading,
            Reading::Done(Ok(Shaping {
                stage: Shaped::Question { .. },
                ..
            }))
        )
    }

    fn revisable(&self) -> bool {
        matches!(
            &self.reading,
            Reading::Done(Ok(Shaping {
                stage: Shaped::Review {
                    revisable: true,
                    ..
                },
                ..
            }))
        )
    }

    /// Whether "So far" can show the goal and decisions now: once read, and not while a step runs.
    fn so_far_ready(&self) -> bool {
        matches!(self.reading, Reading::Done(Ok(_))) && self.running.is_none()
    }

    /// Whether this stage offers an agent step (a question to answer, or none asked yet).
    fn offers_step(&self) -> bool {
        matches!(
            &self.reading,
            Reading::Done(Ok(Shaping {
                stage: Shaped::Question { .. } | Shaped::Waiting,
                ..
            }))
        )
    }

    /// How this stage can be ended, with the helper configured: completed once the seed is
    /// ready, stopped while it's still being shaped. Never for an ended or unknown session.
    fn offers_end(&self) -> Option<Ending> {
        let Reading::Done(Ok(shaping)) = &self.reading else {
            return None;
        };
        match shaping.stage {
            Shaped::Ready { ended: false, .. } => Some(Ending::Completed),
            Shaped::Question { .. }
            | Shaped::Review { .. }
            | Shaped::Waiting
            | Shaped::Drafted { .. } => Some(Ending::Stopped),
            _ => None,
        }
    }

    /// Whether F1 can object to an assumption: the question its step asked is still current.
    fn offers_objection(&self) -> bool {
        !self.assumed.is_empty()
            && matches!(
                &self.reading,
                Reading::Done(Ok(Shaping {
                    stage: Shaped::Question { .. },
                    version: Some(_),
                    ..
                }))
            )
    }

    /// Whether the current question is hard to undo and its options and suggestion are still held
    /// back for the operator's instinct.
    fn instinct_first(&self) -> bool {
        !self.revealed
            && matches!(
                &self.reading,
                Reading::Done(Ok(Shaping {
                    stage: Shaped::Question { hard: true, .. },
                    ..
                }))
            )
    }

    /// Show what a hard-to-undo question held back, recording its withheld suggestion first.
    fn reveal(&mut self) {
        self.revealed = true;
        let id = match &self.reading {
            Reading::Done(Ok(Shaping {
                stage: Shaped::Question { id, .. },
                ..
            })) => id.clone(),
            _ => return,
        };
        match self.withheld.clone() {
            Some(text) => self.write(Write::Recommend { id, text }, "Saving my suggestion."),
            _ => {
                self.notice = Some((
                    "Here are the options. No suggestion was kept for this question.".into(),
                    None,
                ))
            }
        }
    }

    /// End the session through the helper off the paint path; [`Shape::tick`] reports it.
    fn end(&mut self, ending: Ending) {
        self.asking = None;
        self.write(Write::End(ending), "Ending the session.");
    }

    /// Object to assuming `text`, from the version shown.
    fn object(&mut self, text: String) {
        self.objecting = false;
        self.write(Write::Object(text), "Saving your objection.");
    }

    /// Carry out `write` through the helper off the paint path; [`Shape::tick`] reports it.
    fn write(&mut self, write: Write, said: &str) {
        let Reading::Done(Ok(shaping)) = &self.reading else {
            return;
        };
        let expected = shaping.version;
        let session = self.session.clone();
        let (sender, receiver) = mpsc::channel();
        let work = write.clone();
        let confirmation = self.confirmation.clone();
        std::thread::spawn(move || {
            let actor = confirmation.as_ref();
            let result = match (work, expected) {
                (Write::Keep { id, reason }, Some(expected)) => {
                    answering::keep(&session, expected, &id, &reason, actor)
                }
                (
                    Write::Revisit {
                        id,
                        expected,
                        words,
                    },
                    _,
                ) => answering::respond(
                    &session,
                    expected,
                    &id,
                    &AnswerAction::Revisit(words),
                    actor,
                )
                .map(|_| ()),
                (Write::End(ending), _) => answering::end(&session, ending, actor),
                (Write::Object(text), Some(expected)) => {
                    answering::object(&session, expected, &text, actor).map(|_| ())
                }
                (Write::Recommend { id, text }, Some(expected)) => {
                    answering::recommend(&session, expected, &id, &text, actor).map(|_| ())
                }
                (Write::Keep { .. } | Write::Object(_) | Write::Recommend { .. }, None) => Err(
                    Refusal::Refused("Tinkery read no version to change from.".into()),
                ),
            };
            let _ = sender.send(result);
        });
        self.writing = Some((write, receiver));
        self.notice = Some((said.into(), None));
    }

    /// Use the folder the operator typed for the next steps, or say why not and keep the one shown.
    /// `~/` means the home folder; anything else must be an absolute path to a folder. The exact
    /// text is tried first, then without surrounding whitespace, so a pasted line break still works.
    fn pick(&mut self, typed: &str) -> Option<(String, Option<String>)> {
        let home = || std::env::var_os("HOME").map(PathBuf::from);
        let folder = |typed: &str| {
            match typed.strip_prefix("~/") {
                Some(rest) => home().map(|home| home.join(rest)),
                None if typed == "~" => home(),
                None => Some(PathBuf::from(typed)),
            }
            .filter(|p| p.is_absolute())
            .and_then(|p| p.canonicalize().ok())
            .filter(|p| p.is_dir())
        };
        match folder(typed).or_else(|| folder(typed.trim())) {
            Some(folder) => {
                if !matches!(&self.folder, Folder::Known(previous) if previous == &folder) {
                    self.feedback = None;
                }
                self.folder = Folder::Known(folder);
                None
            }
            None => Some((
                format!(
                    "There's no folder at {typed}, so the step still reads {}.",
                    self.folder_name().unwrap_or_default()
                ),
                None,
            )),
        }
    }

    /// The name of the folder the next step reads, once known.
    fn folder_name(&self) -> Option<String> {
        let Folder::Known(folder) = &self.folder else {
            return None;
        };
        Some(folder.file_name().map_or_else(
            || folder.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        ))
    }

    /// The words changed from `before`: a refusal about the old words no longer applies.
    fn edited(&mut self, before: &str) {
        if self.typed.text() != before
            && self
                .notice
                .as_ref()
                .is_some_and(|(said, _)| WORD_REFUSALS.contains(&said.as_str()))
        {
            self.notice = None;
        }
    }

    /// Start what `act` asks, or say why not. Refuses empty words, an exhausted budget, a folder
    /// not yet known and a seed not yet read to the end before anything is written or called.
    fn act(&mut self, act: Act, agent: &Arc<Agent>) {
        if let Act::Respond {
            id,
            expected,
            action: AnswerAction::Revisit(words),
        } = &act
        {
            self.write(
                Write::Revisit {
                    id: id.clone(),
                    expected: *expected,
                    words: words.clone(),
                },
                "Saving your answer. No model runs.",
            );
            return;
        }
        let words = self
            .aside
            .as_ref()
            .map_or(&self.typed, |(_, words)| words)
            .text()
            .to_owned();
        let refuse = |said: &str| Some((said.to_owned(), None));
        let folder = match &self.folder {
            Folder::Known(folder) => Some(folder.clone()),
            _ => None,
        };
        self.notice = match &act {
            Act::TeachBack { .. } if words.trim().is_empty() => {
                refuse("Type your teach-back first; nothing was saved.")
            }
            Act::Change { .. } if words.trim().is_empty() => {
                refuse("Type what should change first; nothing was saved.")
            }
            Act::Keep { .. } if words.trim().is_empty() => refuse(WORD_REFUSALS[2]),
            Act::Answer { .. } if words.trim().is_empty() => refuse(WORD_REFUSALS[0]),
            Act::Confirm { .. } if self.read_end.is_none() => refuse(READ_FIRST),
            Act::Confirm { .. } if words != CONFIRM => refuse(WORD_REFUSALS[1]),
            Act::TeachBack { .. }
            | Act::Change { .. }
            | Act::Respond { .. }
            | Act::Answer { .. }
            | Act::Look(_)
                if agent.remaining() == 0 =>
            {
                said(
                    Err(Failure {
                        answer_saved: false,
                        why: Why::Budget(agent.limit()),
                        feedback: None,
                    }),
                    Doing::Answer,
                    None,
                )
            }
            Act::Respond { .. } | Act::Answer { .. } | Act::Change { .. } | Act::Look(_)
                if folder.is_none() =>
            {
                refuse("Shape is still finding which folder the step reads; nothing was saved.")
            }
            _ => None,
        };
        let None = &self.notice else {
            return;
        };
        let folder = folder.unwrap_or_default();
        let feedback = self.feedback.take();
        let (work, doing) = match act {
            Act::Respond {
                id,
                expected,
                action,
            } => {
                let doing = match &action {
                    AnswerAction::Defer { .. } => Doing::Defer,
                    AnswerAction::Delegate => Doing::Delegate,
                    AnswerAction::Revise(_) | AnswerAction::Revisit(_) => Doing::Revise,
                };
                (
                    Work::Respond {
                        id,
                        expected,
                        action,
                        folder,
                    },
                    doing,
                )
            }
            Act::Keep { id } => {
                self.write(Write::Keep { id, reason: words }, "Saving your reason.");
                return;
            }
            Act::Answer { id, expected } => (
                Work::Answer {
                    id,
                    words,
                    expected,
                    folder,
                },
                Doing::Answer,
            ),
            Act::TeachBack {
                revision,
                expected,
                seed,
            } => (
                Work::TeachBack {
                    revision,
                    expected,
                    seed,
                    words,
                },
                Doing::TeachBack,
            ),
            Act::Change { revision, expected } => (
                Work::Change {
                    revision,
                    words,
                    expected,
                    folder,
                },
                Doing::Change,
            ),
            Act::Look(expected) => (
                Work::Look {
                    expected,
                    feedback: feedback.filter(|f| f.applies(&self.session, expected, &folder)),
                    folder,
                },
                Doing::Look,
            ),
            Act::Confirm {
                revision,
                expected,
                seed,
            } => (
                Work::Confirm {
                    revision,
                    words,
                    expected,
                    seed,
                },
                Doing::Confirm,
            ),
        };
        self.doing = doing;
        self.running = Some(Running::start(work, self.session.clone(), agent.clone()));
    }

    /// Read the session off the paint path and, with an agent, find the folder its step reads.
    fn begin(&mut self, agent: Option<Arc<Agent>>) {
        self.confirmation = agent.as_ref().and_then(|a| a.confirmation.clone());
        let (sender, receiver) = mpsc::channel();
        {
            let session = self.session.clone();
            std::thread::spawn(move || {
                let read = shaping::read(&session);
                let folder = match (&agent, &read) {
                    (
                        Some(agent),
                        Ok(Shaping {
                            goal: Some(goal), ..
                        }),
                    ) => Some(agent.folder(&session, goal)),
                    _ => None,
                };
                let _ = sender.send((read, folder));
            });
        }
        self.reading = Reading::Waiting(receiver);
    }

    fn start(session: &Path) -> Self {
        Self {
            session: session.to_path_buf(),
            confirmation: None,
            reading: Reading::NotStarted,
            typed: Words::new(""),
            form: None,
            aside: None,
            notice: None,
            feedback: None,
            running: None,
            doing: Doing::Look,
            folder: Folder::Unknown,
            choosing: None,
            so_far: None,
            asking: None,
            assumed: Vec::new(),
            objecting: false,
            withheld: None,
            revealed: false,
            writing: None,
            read_end: None,
        }
    }
}

/// Refuses an oversized reading before wrapping; source bytes stay untouched.
fn reading_fits(frame: &mut Frame, texts: &[&Text<'_>], area: Rect, palette: Palette) -> bool {
    let bytes: usize = texts
        .iter()
        .flat_map(|text| &text.lines)
        .map(|line| {
            1 + line
                .spans
                .iter()
                .map(|span| span.content.len())
                .sum::<usize>()
        })
        .sum();
    if bytes < 60_000 {
        return true;
    }
    frame.render_widget(
        Paragraph::new("Too long to show. Nothing was confirmed or changed. Esc goes back.")
            .style(palette.ink)
            .wrap(Wrap { trim: false }),
        area,
    );
    false
}

/// "So far": the confirmed goal, settled decisions, open questions and current findings.
/// Only the operator's own answers are marked as theirs. It scrolls when it doesn't fit.
fn render_so_far(
    frame: &mut Frame,
    scroll: &mut u16,
    shaping: &Shaping,
    area: Rect,
    palette: Palette,
) {
    let mut lines = vec![Line::styled(
        "Your goal, as you confirmed it",
        palette.muted,
    )];
    match &shaping.goal {
        Some(goal) => lines.extend(
            goal.split('\n')
                .map(|line| Line::styled(line.to_owned(), palette.ink)),
        ),
        None => lines.push(Line::styled(
            "This session records no confirmed goal.",
            palette.ink,
        )),
    }
    if !shaping.decided.is_empty() {
        lines.push(Line::default());
        lines.push(Line::styled("Decided", palette.muted));
    }
    for decided in &shaping.decided {
        let by = match decided.authority.as_str() {
            "user" => "you",
            "simulated" => "practice",
            other => other,
        };
        let rows: Vec<&str> = decided.answer.split('\n').collect();
        for (i, row) in rows.iter().enumerate() {
            let mut spans = Vec::new();
            if i == 0 {
                spans.push(Span::styled(format!("{}: ", decided.title), palette.muted));
            }
            spans.push(Span::styled((*row).to_owned(), palette.ink));
            if i + 1 == rows.len() {
                spans.push(Span::styled(format!(" ({by})"), palette.muted));
            }
            lines.push(Line::from(spans));
        }
    }
    if !shaping.open.is_empty() {
        lines.push(Line::default());
        lines.push(Line::styled("Open questions", palette.muted));
        for question in &shaping.open {
            lines.push(Line::styled(question.clone(), palette.ink));
        }
    }
    if let Shaped::Question { found, .. } | Shaped::Review { found, .. } = &shaping.stage
        && !found.is_empty()
    {
        lines.push(Line::default());
        lines.push(Line::styled("Findings for this question", palette.muted));
        for entry in found {
            lines.push(Line::styled(entry.clone(), palette.ink));
            lines.push(Line::default());
        }
    }
    let text = Text::from(lines);
    if !reading_fits(frame, &[&text], area, palette) {
        return;
    }
    let page = Paragraph::new(text).wrap(Wrap { trim: false });
    // How to close stays on the page's last row, even when the rest scrolls.
    let rows = page.line_count(area.width) as u16;
    let height = area.height.saturating_sub(2);
    *scroll = (*scroll).min(rows.saturating_sub(height));
    frame.render_widget(page.scroll((*scroll, 0)), Rect { height, ..area });
    let close = if rows > height {
        "↑↓ scroll · esc close"
    } else {
        "esc close"
    };
    frame.render_widget(
        Paragraph::new(close).style(palette.ink),
        Rect {
            y: area.y + rows.min(height) + 1,
            height: 1,
            ..area
        }
        .intersection(area),
    );
}

/// What to read (the goal's first line, or the seed draft when one waits, then where the seed
/// stands) comes first and scrolls when it doesn't fit; the words box, what was said and any next
/// step follow it and stay in view. Returns whether the reading scrolls, so the key bar offers
/// scrolling only then.
/// Draws the open session; returns whether the reading overflows and how many of its rows show.
fn render_open(
    frame: &mut Frame,
    scroll: &mut u16,
    open: &mut Opened,
    agent: bool,
    held: bool,
    area: Rect,
    palette: Palette,
) -> (bool, u16) {
    let area = Rect {
        width: area.width.min(WIDTH),
        ..area
    };
    if area.width == 0 || area.height == 0 {
        return (false, 1);
    }
    let (reading, input) = match &open.reading {
        Reading::NotStarted | Reading::Waiting(_) => (
            Text::styled("Reading where this seed stands.", palette.muted),
            Text::default(),
        ),
        Reading::Done(Err(error)) => (
            plain(
                &match error
                    .strip_prefix("Tinkery refused: ")
                    .and_then(|reason| reason.lines().next())
                    .filter(|reason| !reason.trim().is_empty())
                {
                    Some(reason) => format!("Tinkery refused to read this session: {reason}"),
                    None => "Shape couldn't read this session. F1 shows why.".into(),
                },
                "Next: Esc goes back; F1 shows where the session is.",
                palette,
            ),
            Text::default(),
        ),
        Reading::Done(Ok(shaping)) => {
            let top = match &shaping.stage {
                Shaped::Drafted {
                    revision,
                    text: Some(seed),
                } => Some(seed_view(seed, revision.as_deref(), palette)),
                _ => shaping.goal.as_deref().map(|goal| {
                    Text::styled(goal.lines().next().unwrap_or("").to_owned(), palette.ink)
                }),
            };
            let name = open.folder_name();
            let typing = open.typing().filter(|_| agent);
            let words = || typing.as_ref().map(|t| t.label);
            let acts = typing.is_some()
                || agent
                    && matches!(
                        shaping,
                        Shaping {
                            stage: Shaped::Waiting,
                            version: Some(_),
                            ..
                        }
                    );
            let stand = match (&open.running, &open.choosing) {
                (Some(running), _) => {
                    let name = name.as_deref().unwrap_or_default();
                    let said = match (open.doing, running.saved()) {
                        (Doing::Answer, false) => "Saving your answer.".to_owned(),
                        (Doing::Answer, true) => {
                            format!("Answer saved. Looking in {name} for the next question.")
                        }
                        (Doing::Defer, false) => "Saving your deferral.".into(),
                        (Doing::Delegate, false) => "Saving the delegated suggestion.".into(),
                        (Doing::Revise, false) => {
                            "Revising your answer and reopening dependents.".into()
                        }
                        (Doing::Defer | Doing::Delegate | Doing::Revise, true) => format!(
                            "{}Looking in {name} for the next question.",
                            open.doing.saved()
                        ),
                        (Doing::Look, _) => format!("Looking in {name} for the next question."),
                        (Doing::TeachBack, false) => "Saving your teach-back.".into(),
                        (Doing::TeachBack, true) => {
                            "Teach-back saved. Comparing with the draft.".into()
                        }
                        (Doing::Change, false) => "Saving your change.".into(),
                        (Doing::Change, true) => {
                            format!("Change saved. Redrafting the seed from {name}.")
                        }
                        (Doing::Confirm, _) => "Confirming the seed.".to_owned(),
                    };
                    Stand {
                        acts,
                        reads: None,
                        words: words(),
                        notice: None,
                        running: Some(Line::from(vec![
                            Span::styled(said, palette.ink),
                            Span::styled(
                                format!("  {}s", running.elapsed().as_secs()),
                                palette.muted,
                            ),
                        ])),
                        assumed: &open.assumed,
                        instinct: open.instinct_first(),
                    }
                }
                (None, Some(_)) => Stand {
                    acts,
                    reads: None,
                    words: Some("Folder:"),
                    notice: Some((
                        "",
                        Some(format!(
                            "Next: F2 uses this folder; Esc keeps {}.",
                            name.as_deref().unwrap_or_default()
                        )),
                    )),
                    running: None,
                    assumed: &open.assumed,
                    instinct: open.instinct_first(),
                },
                (None, None) => Stand {
                    acts,
                    reads: (agent && open.offers_step()).then(|| match &name {
                        Some(name) => {
                            format!("The step reads files in {name}. F1 picks another folder.")
                        }
                        None => "Finding which folder the step reads.".into(),
                    }),
                    words: words(),
                    notice: open
                        .notice
                        .as_ref()
                        .map(|(said, next)| (said.as_str(), next.clone()))
                        .or(held.then_some((HELD, None))),
                    running: None,
                    assumed: &open.assumed,
                    instinct: open.instinct_first(),
                },
            };
            let (mut said, mut input) = standing(shaping, stand, palette);
            if agent && !held && open.offers_choices() && shaping.deferred.is_some() {
                said.lines.push(Line::styled(
                    "F1 offers an explicit answer; no model runs.",
                    palette.muted,
                ));
            }
            if let Some(note) = &shaping.confirmation_note {
                input.lines.push(Line::styled(note.clone(), palette.muted));
            }
            let mut reading = top.unwrap_or_default();
            if matches!(shaping.stage, Shaped::Drafted { .. }) {
                reading.lines.push(Line::default());
                reading
                    .lines
                    .push(Line::styled("Before you confirm", palette.muted));
                for line in &shaping.teach_back {
                    reading
                        .lines
                        .extend(Text::styled(line.clone(), palette.ink).lines);
                }
                for line in &shaping.confirmation {
                    reading
                        .lines
                        .extend(Text::styled(line.clone(), palette.ink).lines);
                }
            }
            let (mut reading, input) = if let Some(form) = &open.form {
                reading.lines.extend(said.lines);
                reading.lines.push(Line::default());
                reading
                    .lines
                    .extend(Text::styled(form.description(), palette.ink).lines);
                if !open.notice.as_ref().is_some_and(|(_, next)| next.is_some()) {
                    input
                        .lines
                        .retain(|line| !line.to_string().starts_with("Next:"));
                }
                (reading, input)
            } else {
                if !reading.lines.is_empty() && !said.lines.is_empty() {
                    reading.lines.push(Line::default());
                }
                reading.lines.extend(said.lines);
                (reading, input)
            };
            if matches!(shaping.stage, Shaped::Drafted { text: Some(_), .. }) {
                reading.lines.push(Line::default());
                reading
                    .lines
                    .push(Line::styled("End of seed", palette.muted));
            }
            (reading, input)
        }
    };
    if !reading_fits(frame, &[&reading, &input], area, palette) {
        open.read_end = None;
        return (false, 1);
    }
    let rows = |text: &Text<'static>| {
        Paragraph::new(text.clone())
            .wrap(Wrap { trim: false })
            .line_count(area.width) as u16
    };
    let label = if open.choosing.is_some() {
        Some("Folder:")
    } else {
        open.typing().filter(|_| agent).map(|t| t.label)
    };
    let split = label.and_then(|label| input.lines.iter().position(|l| l.to_string() == label));
    let (before, mut after) = match split {
        Some(i) => (
            Text::from(input.lines[..=i].to_vec()),
            Text::from(input.lines[i + 1..].to_vec()),
        ),
        None => (Text::default(), input),
    };
    let editor = split.map(|_| {
        if let Some(folder) = &mut open.choosing {
            folder
        } else if let Some((_, words)) = &mut open.aside {
            words
        } else if let Some(form) = &mut open.form {
            form.words()
        } else {
            &mut open.typed
        }
    });
    let editor_rows = editor.as_ref().map_or(0, |words| words.rows(area.width));
    let (read_rows, before_rows, after_rows) = (rows(&reading), rows(&before), rows(&after));
    let input_rows = before_rows
        .saturating_add(editor_rows)
        .saturating_add(after_rows);
    let gap = u16::from(read_rows > 0 && input_rows > 0);
    // Reserve at least half the body for a long reading. The editor scrolls to its caret;
    // labels and consequences stay outside that viewport, even while a step is running.
    let input_height =
        input_rows
            .min(area.height.saturating_sub(
                gap + read_rows.min(area.height / 2).max(u16::from(read_rows > 0)),
            ));
    let height = read_rows.min(area.height.saturating_sub(input_height + gap));
    *scroll = (*scroll).min(read_rows.saturating_sub(height));
    if height > 0
        && open.read_end.is_none()
        && *scroll >= read_rows.saturating_sub(height)
        && let Reading::Done(Ok(shaping)) = &open.reading
        && matches!(shaping.stage, Shaped::Drafted { text: Some(_), .. })
    {
        open.read_end = Some(shaping.stage.clone());
    }
    // Reaching the end answers "read to the end first".
    if open.read_end.is_some()
        && open
            .notice
            .as_ref()
            .is_some_and(|(said, _)| said == READ_FIRST)
    {
        open.notice = None;
        after.lines.retain(|line| line.to_string() != READ_FIRST);
    }
    frame.render_widget(
        Paragraph::new(reading)
            .wrap(Wrap { trim: false })
            .scroll((*scroll, 0)),
        Rect { height, ..area },
    );
    let mut input_area = Rect {
        y: area.y + height + gap,
        height: input_height,
        ..area
    }
    .intersection(area);
    let after_height = after_rows.min(input_height.saturating_sub(u16::from(editor.is_some())));
    let before_height =
        before_rows.min(input_height.saturating_sub(after_height + u16::from(editor.is_some())));
    frame.render_widget(
        Paragraph::new(before).wrap(Wrap { trim: false }),
        Rect {
            height: before_height,
            ..input_area
        },
    );
    input_area.y += before_height;
    input_area.height = input_height.saturating_sub(before_height + after_height);
    if let Some(editor) = editor {
        editor.render(frame, input_area, palette, open.running.is_some());
        if label == Some("your words") && editor.text().is_empty() && open.read_end.is_some() {
            frame.render_widget(
                Paragraph::new("type confirm, then F2").style(palette.muted),
                Rect {
                    height: input_area.height.min(1),
                    ..input_area
                },
            );
        }
    }
    input_area.y += input_area.height;
    input_area.height = after_height;
    frame.render_widget(
        Paragraph::new(after)
            .wrap(Wrap { trim: false })
            .scroll((after_rows.saturating_sub(after_height), 0)),
        input_area,
    );
    (read_rows > height, height)
}

/// The seed draft for reading: a muted `Seed draft rN` header in place of the file's `status:`
/// and `revision:` lines, and headings bold without their `#` marks. Every other line is shown
/// as written. Display only: the file is never changed.
fn seed_view(seed: &str, revision: Option<&str>, palette: Palette) -> Text<'static> {
    let mut lines = vec![
        Line::styled(
            revision.map_or_else(|| "Seed draft".to_owned(), |r| format!("Seed draft {r}")),
            palette.muted,
        ),
        Line::default(),
    ];
    let mut header = true;
    for line in seed.trim_end_matches('\n').split('\n') {
        header &= !line.trim().is_empty();
        if header && (line.starts_with("status:") || line.starts_with("revision:")) {
            continue;
        }
        lines.push(
            match line.strip_prefix("## ").or_else(|| line.strip_prefix("# ")) {
                Some(heading) => Line::styled(heading.to_owned(), palette.ink.bold()),
                None => Line::styled(line.to_owned(), palette.ink),
            },
        );
    }
    Text::from(lines)
}

/// What a finished step needs to say, and its own next step when the stage's won't do.
/// What choosing an ending asks, and how to answer it.
fn asked(ending: Ending) -> (String, Option<String>) {
    match ending {
        Ending::Completed => (
            "End this session as completed? It becomes read-only for good, and tend no longer lists it; Approve still offers its seed.".into(),
            Some("Next: F2 ends it; Esc keeps it open.".into()),
        ),
        Ending::Stopped => (
            "Stop this session? It becomes read-only for good and tend no longer lists it; its open questions stay recorded.".into(),
            Some("Next: F2 stops it; Esc keeps it open.".into()),
        ),
    }
}

/// `left` is the allowance, added where the operator decides whether to retry (#435).
fn said(
    result: Result<Done, Failure>,
    doing: Doing,
    left: Option<String>,
) -> Option<(String, Option<String>)> {
    if result == Ok(Done::Compared) {
        return Some((
            "Teach-back compared. Mismatches never block confirming.".into(),
            None,
        ));
    }
    let Failure {
        answer_saved,
        why,
        feedback,
    } = result.err()?;
    let saved = if answer_saved { doing.saved() } else { "" };
    let left = left.map_or_else(String::new, |left| format!(" {left}"));
    Some(match why {
        Why::Stale if answer_saved => (
            format!(
                "{saved}Another writer changed this session before the next step, so Shape saved nothing more and read it again."
            ),
            None,
        ),
        Why::Stale => (
            "Another writer changed this session first, so nothing was saved. Shape read it again."
                .into(),
            matches!(doing, Doing::Defer | Doing::Revise).then(|| {
                "Next: Esc cancels this form; open it again from the refreshed question.".into()
            }),
        ),
        Why::Budget(limit) => (
            format!(
                "{saved}No drafting runs are left: this launch's allowance is {limit}. Nothing {}was saved.",
                if answer_saved { "more " } else { "" }
            ),
            Some("Next: add runs from F1, Model and runs, then send again.".into()),
        ),
        Why::Model(error) => (
            format!(
                "{saved}{}: {error}{left}",
                if matches!(doing, Doing::TeachBack) {
                    "Comparing your teach-back failed"
                } else {
                    "Looking into the next question failed"
                }
            ),
            None,
        ),
        // A rule the model's reply broke: the retry hears the full reason; the screen says
        // whose reply it was and the rule, without the ledger's node names (#649).
        Why::Refused(reason) if feedback.is_some() => (
            format!(
                "{saved}The model's reply was refused, so nothing from it was saved: {}.{left}",
                broken_rule(&reason)
            ),
            None,
        ),
        Why::Refused(reason) => (format!("{saved}{reason}{left}"), None),
        Why::Stopped => (
            format!(
                "{saved}Stopped before the next step finished. The drafting attempt still counts against this launch's allowance.{left}"
            ),
            None,
        ),
    })
}

/// The rule in a refused reply's reason, without Tinkery's prefix or the node it names.
fn broken_rule(reason: &str) -> &str {
    let rule = reason.strip_prefix("Tinkery refused: ").unwrap_or(reason);
    let rule = rule
        .strip_prefix("node ")
        .and_then(|named| named.split_once(": "))
        .map_or(rule, |(_, rule)| rule);
    rule.trim_end_matches('.')
}

fn plain(said: &str, next: &str, palette: Palette) -> Text<'static> {
    Text::from(vec![
        Line::styled(said.to_owned(), palette.ink),
        Line::styled(next.to_owned(), palette.ink),
    ])
}

/// What [`standing`] shows beside the stage.
struct Stand<'a> {
    /// The key bar names this stage's one next action, so no Next line repeats it.
    acts: bool,
    /// Which folder the next step reads, when a step is offered.
    reads: Option<String>,
    /// The editor's label, when this stage takes words.
    words: Option<&'a str>,
    /// What the last key or step said, and its own next step if any.
    notice: Option<(&'a str, Option<String>)>,
    /// What a running step is doing; everything else then stays, muted.
    running: Option<Line<'static>>,
    /// Defaults the current question's step assumed instead of asking.
    assumed: &'a [Assumption],
    /// Whether a hard-to-undo question's options and suggestion still wait for the operator's instinct.
    instinct: bool,
}

/// Where the seed stands, in plain words, as what to read and then what to act with (the words box,
/// what was said and the next step). The question's label is muted and its wording is the one
/// thing to act on (jade); its suggestion is a guess (italic). The operator's words are in ink.
/// At most one Next line: none when the key bar already names the one next action and nothing
/// was said, and a notice's own next step replaces the stage's. While a step runs, the question and the words
/// stay, muted, under what the step is doing.
fn standing(shaping: &Shaping, stand: Stand, palette: Palette) -> (Text<'static>, Text<'static>) {
    const SETUP: &str = "Next: reopen this goal with Shape set up; F1 shows how.";
    const INSPECT: &str = "Next: check this session's files; F1 shows the path.";
    let Stand {
        acts,
        reads,
        words,
        notice,
        running,
        assumed,
        instinct,
    } = stand;
    let calm = running.is_some();
    let mut lines = Vec::new();
    let mut flag = None;
    let next = match &shaping.stage {
        Shaped::Review {
            title,
            answer,
            found,
            ..
        } => {
            lines.push(Line::styled(
                "Current question: review flagged answer",
                palette.muted,
            ));
            lines.push(Line::styled(format!("Keep {title}?"), palette.jade));
            lines.push(Line::styled(
                format!("Settled answer: {answer}"),
                palette.ink,
            ));
            for reason in found {
                lines.push(Line::styled(reason.clone(), palette.ink));
            }
            lines.push(Line::styled(
                "Confirmation is held until each flagged node is reviewed.",
                palette.muted,
            ));
            Some(if acts {
                "Next: type why, then F2 keeps the answer unchanged with your reason.".into()
            } else {
                SETUP.into()
            })
        }
        Shaped::Question {
            id,
            title,
            ask,
            found,
            suggestion,
            options,
            against,
            ..
        } => {
            lines.push(Line::styled(title.clone(), palette.muted));
            lines.push(Line::styled(
                ask.clone(),
                if calm { palette.muted } else { palette.jade },
            ));
            if !calm {
                lines.push(Line::default());
                // Tinkery's own probe and objection questions ask for the operator's words, not evidence.
                if found.is_empty() && id != PROBE.0 && title != OBJECTION {
                    lines.push(Line::styled(
                        "Nothing found: this question records nothing checked in the repository.",
                        palette.ink,
                    ));
                }
                if id == PROBE.0 {
                    lines.push(Line::styled(
                        "Answer in your own words, or type skip and send to skip it.",
                        palette.ink,
                    ));
                }
                if let Some(entry) = found.first() {
                    lines.push(Line::styled(format!("What I found: {entry}"), palette.ink));
                }
                if instinct {
                    lines.push(Line::styled(
                        "This is hard to undo, so what's your instinct? F1 shows the options and my suggestion.",
                        palette.ink,
                    ));
                } else {
                    for option in options {
                        lines.push(Line::styled(option.clone(), palette.ink));
                    }
                    if let Some(against) = against {
                        lines.push(Line::styled(
                            format!("Against my suggestion: {against}"),
                            palette.muted,
                        ));
                    }
                    if let Some(suggestion) = suggestion {
                        lines.push(Line::styled(
                            format!("Suggestion: {suggestion}"),
                            palette.ink.italic(),
                        ));
                    }
                }
                if !assumed.is_empty() {
                    lines.push(Line::default());
                    lines.push(Line::styled(
                        "I'll assume these unless you object:",
                        palette.muted,
                    ));
                    for (i, a) in assumed.iter().enumerate() {
                        lines.push(Line::from(vec![
                            Span::styled(format!("{}. {}", i + 1, a.text), palette.ink),
                            Span::styled(format!(" (why: {})", a.why), palette.muted),
                        ]));
                    }
                }
            }
            Some(if acts {
                "Next: type your answer, then F2 saves it and looks into the next question.".into()
            } else {
                SETUP.into()
            })
        }
        Shaped::Waiting => {
            if let Some(target) = &shaping.deferred {
                lines.push(Line::styled(
                    format!("Deferred prerequisite: {}", target.title),
                    palette.ink,
                ));
            }
            if !calm {
                lines.push(Line::styled(
                    if shaping.success_open {
                        "The saved goal leaves Done when open."
                    } else {
                        "Shape hasn't asked its next question yet."
                    },
                    palette.ink,
                ));
            }
            Some(if acts {
                "Next: F2 looks into the next question.".into()
            } else {
                SETUP.into()
            })
        }
        Shaped::Drafted { revision, text } => {
            // When the draft itself is shown, its header already says this.
            if text.is_none() {
                lines.push(Line::styled(
                    match revision {
                        Some(r) => format!("Seed draft {r} is waiting for you to confirm it."),
                        None => "A seed draft is waiting for you to confirm it.".into(),
                    },
                    palette.ink,
                ));
            }
            // Beside the words box, so a long seed can't scroll it away.
            if !calm && text.as_deref().is_some_and(|t| t.contains(NOTHING_FOUND)) {
                flag = Some(Line::styled(
                    "Nothing found: this seed records nothing checked in the repository.",
                    palette.ink,
                ));
            }
            match (acts, revision) {
                (true, _) if words == Some("your teach-back") => {
                    Some("Next: F2 saves your words and compares with the draft.".into())
                }
                (true, _) if words == Some("your change") => {
                    Some("Next: F2 saves your change and redrafts the seed.".into())
                }
                // The key bar owns the next action, including while a refusal is shown.
                (true, Some(_)) => None,
                (false, Some(_)) => Some(SETUP.into()),
                _ => Some(INSPECT.into()),
            }
        }
        Shaped::Ready { revision, ended } => {
            lines.push(Line::styled(
                match revision {
                    Some(r) => format!("Seed ready: you confirmed {r}, and it still matches."),
                    None => "Seed ready: what you confirmed still matches.".into(),
                },
                palette.ink,
            ));
            if *ended {
                lines.push(Line::styled(
                    "This session is completed, so nothing in it can change.",
                    palette.muted,
                ));
            }
            Some(if shaping.ready_target.is_some() {
                "Next: F1, then 1: Open this seed in Approve.".into()
            } else {
                "Next: open Approve from the menu to inspect this seed.".into()
            })
        }
        Shaped::Unverified { reason } => {
            lines.push(Line::styled(
                format!("The seed says it's confirmed, but Tinkery can't verify it: {reason}."),
                palette.ink,
            ));
            Some(INSPECT.into())
        }
        Shaped::Practice => {
            lines.push(Line::styled(
                "This is a practice session's seed, not a real one.",
                palette.ink,
            ));
            Some("Next: confirm a goal in think to start a real one.".into())
        }
        Shaped::Ended { status } => {
            lines.push(Line::styled(
                format!("This session has ended ({status}). There's nothing left to shape."),
                palette.ink,
            ));
            Some("Next: Esc goes back.".into())
        }
        Shaped::Unknown { what } => {
            lines.push(Line::styled(
                format!("Shape can't tell where this seed stands. {what}"),
                palette.ink,
            ));
            Some(INSPECT.into())
        }
    };
    let gap = |lines: &mut Vec<Line<'static>>| {
        if !lines.is_empty() {
            lines.push(Line::default());
        }
    };
    if let Some(reads) = reads {
        gap(&mut lines);
        lines.push(Line::styled(reads, palette.ink));
    }
    // What the operator acts with stays in view beneath what they read.
    let mut input: Vec<Line<'static>> = flag.into_iter().collect();
    if let Some(label) = words {
        let style = if calm { palette.muted } else { palette.ink };
        gap(&mut input);
        input.push(Line::styled(label.to_owned(), style));
        if acts && !calm && matches!(shaping.stage, Shaped::Drafted { .. }) {
            input.push(Line::default());
            input.push(Line::styled(
                "Confirming the seed is not approval to build.",
                palette.ink,
            ));
        }
    }
    if let Some(running) = running {
        gap(&mut input);
        input.push(running);
        return (Text::from(lines), Text::from(input));
    }
    let next = match notice {
        Some((said, own)) => {
            if !said.is_empty() {
                gap(&mut input);
                input.push(Line::styled(said.to_owned(), palette.ink));
            }
            own.or(next)
        }
        // The key bar already names the one next action, unless something needs saying first.
        None => next.filter(|_| !acts),
    };
    if let Some(next) = next {
        gap(&mut input);
        input.push(Line::styled(next, palette.ink));
    }
    (Text::from(lines), Text::from(input))
}

fn clipped(text: &str, width: usize) -> String {
    if text.width() <= width {
        return text.into();
    }
    let mut result = String::new();
    let mut used = 0;
    for g in text.graphemes(true) {
        if used + g.width() + 1 > width {
            break;
        }
        result.push_str(g);
        used += g.width();
    }
    if width > 0 {
        result.push('…');
    }
    result
}

#[cfg(test)]
mod tests;
