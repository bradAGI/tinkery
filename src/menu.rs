//! The home menu: the operator's seven places, in pipeline order.
//!
//! think · tend · shape · approve · watch · taste · measure. Each place has its own boundary.
//! Named worker actions use the shared SDLC boundary; navigation never starts work.

mod retry;
mod worker;

use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use crate::{
    Palette, approve, measure, seed_me,
    shape::{Leave, Shape},
    taste::Taste,
    tend::{self, Discovery, Source, Tend},
    think::brain_dump::{self, BrainDump, Exit},
    watch::Watch,
};
use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Padding, Paragraph, Wrap},
};

/// Name and one-line purpose of each place.
pub const PLACES: [(&str, &str); 7] = [
    ("think", "Clarify a goal"),
    ("tend", "Open saved goals"),
    ("shape", "Prepare a brief"),
    ("approve", "Prepare work for approval"),
    ("watch", "See work status"),
    ("taste", "Review a result"),
    ("measure", "Browse recent pull requests"),
];

/// A checked index into [`PLACES`]; callers cannot construct an out-of-range place.
///
/// ```compile_fail
/// let invalid = tinkery::menu::Place(7);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Place(usize);

impl Place {
    /// The place at `index`, or `None` when no such place exists.
    pub fn at(index: usize) -> Option<Self> {
        PLACES.get(index).map(|_| Self(index))
    }
}

const FIRST_ROW: u16 = 4;
const ROW_GAP: u16 = 2;

/// The menu and whichever place is open in it. It routes input to the open place and owns what
/// outlives a visit: the key bar preference, receipts to print on exit, and the one session
/// scan tend reads from. A place returns here by its own exit; nothing opens by itself.
pub struct Home {
    /// The operator chose to leave Tinkery; the terminal loop stops after this tick.
    pub quit: bool,
    make_dump: Box<dyn Fn() -> BrainDump>,
    sessions_root: Option<PathBuf>,
    link_root: Result<PathBuf, String>,
    selected: usize,
    view: View,
    help: bool,
    /// The muted key bar preference lasts the session, across visits to think.
    key_bar: bool,
    receipts: Vec<seed_me::ExitReceipt>,
    discovery: Discovery,
    record_root: Result<PathBuf, String>,
    history_reader: tend::history::Reader,
    repo: Option<crate::repo::Repo>,
    shape_agent: Option<Arc<crate::shape::Agent>>,
    sdlc_root: Option<PathBuf>,
    codex_report: Option<crate::watch::CodexReport>,
    /// An agent launch without --shape-pi: Shape only reads.
    shape_held: bool,
    /// Launched with `--operator agent`; Taste then refuses the operator's labels.
    agent: bool,
    approve_reader: approve::Reader,
    approve_origin: Option<ApproveOrigin>,
    /// `m` on the menu asked for the Model panel, which the launch owns.
    model_request: bool,
    worker_config: Option<crate::worker::Config>,
    worker_panel: Option<worker::Panel>,
    quit_after_worker: bool,
}

enum View {
    Menu,
    Think(Box<BrainDump>),
    Tend(Box<Tend>),
    /// Shape, and the tend it was opened from, restored as it was when Shape is left.
    Shape(Box<Shape>, Option<Box<Tend>>),
    Approve(Box<approve::Approve>),
    Worker,
    Measure(Box<measure::Measure>),
    Watch(Box<Watch>),
    Taste(Box<Taste>),
    Retry(Box<retry::Review>, Box<Taste>),
}

/// Only the deliberate Shape-to-Approve visit returns here; F10 discards it for Home.
struct ApproveOrigin {
    shape: Box<Shape>,
    tend: Option<Box<Tend>>,
    selected: usize,
}

impl Home {
    /// A menu on its first row. `make_dump` makes a fresh think each time think is opened;
    /// tend reads the selected sessions root or shows its refusal without scanning. Links use
    /// the separate captured `link_root` (or its refusal).
    pub fn new(
        make_dump: Box<dyn Fn() -> BrainDump>,
        sessions_root: Result<PathBuf, String>,
        link_root: Result<PathBuf, String>,
    ) -> Self {
        let (sessions_root, discovery) = match sessions_root {
            Ok(root) => (Some(root), Discovery::new(Arc::new(tend::scan))),
            Err(reason) => (None, Discovery::unavailable(reason)),
        };
        Self {
            quit: false,
            make_dump,
            sessions_root,
            link_root,
            selected: 0,
            view: View::Menu,
            help: false,
            key_bar: true,
            receipts: Vec::new(),
            discovery,
            record_root: crate::think_record::default_root(std::env::var_os("HOME").as_deref()),
            history_reader: tend::history::Reader::default(),
            repo: None,
            shape_agent: None,
            sdlc_root: None,
            codex_report: None,
            shape_held: false,
            agent: false,
            approve_reader: approve::Reader::default(),
            approve_origin: None,
            model_request: false,
            worker_config: None,
            worker_panel: None,
            quit_after_worker: false,
        }
    }

    /// The model Shape answers and drafts through; without one, Shape only reads. Starts nothing.
    pub fn with_shape_agent(mut self, agent: Option<Arc<crate::shape::Agent>>) -> Self {
        self.shape_agent = agent;
        self
    }

    /// The one tink-sdlc checkout the read-only run views use; absent roots explain configuration.
    pub fn with_sdlc_root(mut self, root: Option<PathBuf>) -> Self {
        self.sdlc_root = root;
        self
    }

    /// Show one explicitly bound saved Codex report in Watch; it grants no execution authority.
    pub fn with_codex_report(mut self, report: Option<crate::watch::CodexReport>) -> Self {
        self.codex_report = report;
        self
    }

    /// Set a separate, explicit worker allowance for this launch; configuration runs nothing.
    pub fn with_worker(mut self, model: Option<String>, responses: usize) -> Result<Self, String> {
        if model.is_some() || responses > 0 {
            let config = crate::worker::Config {
                model: model.unwrap_or_default(),
                responses,
            };
            config.validate()?;
            self.worker_config = Some(config);
        }
        Ok(self)
    }

    /// An agent launch without named context only reads; see [`Shape::held`].
    pub fn with_shape_held(mut self, held: bool) -> Self {
        self.shape_held = held;
        self
    }

    /// An `--operator agent` launch: places that record the operator's own words refuse it.
    pub fn with_agent_operator(mut self, agent: bool) -> Self {
        self.agent = agent;
        self
    }

    /// Replace the reader for a selected root; never override a root refusal.
    pub fn with_discovery_source(mut self, source: Source) -> Self {
        if self.sessions_root.is_some() {
            self.discovery = Discovery::new(source);
        }
        self
    }

    /// The repository Tinkery was started in, named once on the menu; `None` shows nothing.
    pub fn with_repo(mut self, repo: Option<crate::repo::Repo>) -> Self {
        self.repo = repo;
        self
    }

    /// Select the existing Think records to read. Configuring a root creates and reads nothing.
    pub fn with_record_root(mut self, root: PathBuf) -> Self {
        self.record_root = Ok(root);
        self
    }

    /// The menu think returns to after saving a goal when it was launched on its own. The
    /// receipt and captured links destination carry over; `open_tend` opens the saved session.
    pub fn from_receipt(
        mut dump: BrainDump,
        session: &Path,
        open_tend: bool,
        link_root: Result<PathBuf, String>,
        shape_agent: Option<Arc<crate::shape::Agent>>,
    ) -> Self {
        let root = session
            .parent()
            .map(|path| Ok(path.to_path_buf()))
            .unwrap_or_else(|| crate::seed_me::default_root(std::env::var_os("HOME").as_deref()));
        let mut home =
            Self::new(dump.fresh_factory(), root, link_root).with_shape_agent(shape_agent);
        if let Some(root) = dump.record_root() {
            home.record_root = Ok(root.to_path_buf());
        }
        home.key_bar = dump.key_bar_on();
        home.repo = dump.repo().cloned();
        if let Some(receipt) = dump.finish_handoff() {
            home.receipts.push(receipt);
        }
        if open_tend {
            home.open_tend_session(session);
        }
        home
    }

    /// Advances time: keeps the session scan moving and tend's wait notice current. Never
    /// blocks; a slow scan shows as waiting, not as a frozen screen.
    pub fn tick(&mut self) {
        self.tick_at(Instant::now());
    }

    /// [`Home::tick`] with the clock as an input, so tests never sleep.
    fn tick_at(&mut self, now: Instant) {
        if let Some(panel) = &mut self.worker_panel {
            panel.poll();
        }
        if self.quit_after_worker && !self.worker_busy() {
            self.quit = true;
        }

        if let View::Watch(watch) = &mut self.view {
            watch.poll();
        }
        if let View::Measure(measure) = &mut self.view {
            measure.poll();
        }
        if let View::Taste(taste) = &mut self.view {
            taste.poll();
        }
        self.poll_retry();
        if !self.discovery.requested() {
            self.request_sessions();
        }
        self.discovery.poll();
        self.sync_tend();
        self.sync_shape();
        self.sync_approve();
        self.approve_reader
            .poll(if let View::Approve(view) = &mut self.view {
                Some(view)
            } else {
                None
            });
        self.update_stall(now);
        if let Some(result) = self.history_reader.poll()
            && let View::Tend(tend) = &mut self.view
            && let Some(history) = &mut tend.history
        {
            history.finish(result);
        }
        let mut tend_request = None;
        if let View::Think(dump) = &mut self.view {
            dump.tick();
            if let Exit::Tend(path) = dump.exit() {
                tend_request = Some(path.clone());
            }
            if *dump.exit() != Exit::Stay {
                self.key_bar = dump.key_bar_on();
                if let Some(receipt) = dump.finish_handoff() {
                    self.receipts.push(receipt);
                }
                self.view = View::Menu;
                // Back from think: a goal may have been saved, so read again.
                self.request_sessions();
            }
        }
        if let Some(path) = tend_request {
            self.open_tend_session(&path);
        }
    }

    /// Show the newest landed read in tend. Detail stays fixed unless explicitly requested
    /// again on return from Shape; that return waits for the fresh read.
    fn sync_tend(&mut self) {
        let View::Tend(tend) = &mut self.view else {
            return;
        };
        tend.current = self.discovery.fresh();
        let Some(landed) = self.discovery.landed() else {
            return;
        };
        let version = self.discovery.version();
        if tend.seen == version
            || ((tend.detail || tend.is_linking())
                && (tend.want.is_none() || !self.discovery.fresh()))
        {
            return;
        }
        tend.repo_paths = landed.repo_paths.clone();
        tend.repo_unknown = landed.repo_unknown;
        tend.apply(&landed.outcome, self.discovery.fresh(), version);
    }

    /// Hand Shape the readable sessions and unreadable count from its fresh discovery result,
    /// and let an open session's read finish.
    fn sync_shape(&mut self) {
        let View::Shape(shape, _) = &mut self.view else {
            return;
        };
        shape.tick();
        if shape.wants_sessions()
            && self.discovery.fresh()
            && let Some(landed) = self.discovery.landed()
        {
            let unreadable = landed.outcome.as_ref().map_or(0, |found| found.unreadable);
            shape.offer(
                match &landed.outcome {
                    Ok(found) => Ok(&found.sessions),
                    Err(error) => Err(error.clone()),
                },
                unreadable,
            );
            if let Ok(found) = &landed.outcome {
                shape.offer_failures(&found.failures);
            }
        }
    }

    fn sync_approve(&mut self) {
        let View::Approve(view) = &mut self.view else {
            return;
        };
        if view.wants_sessions()
            && self.discovery.fresh()
            && let Some(landed) = self.discovery.landed()
        {
            let (sessions, unavailable, reasons) = match &landed.outcome {
                Ok(found) => (
                    Ok(found.sessions.clone()),
                    found.unreadable,
                    found
                        .failures
                        .iter()
                        .map(|f| format!("{}: {}", f.path.display(), f.reason))
                        .collect(),
                ),
                Err(reason) => (Err(reason.clone()), 0, Vec::new()),
            };
            view.offer(
                sessions,
                unavailable,
                reasons,
                &mut self.approve_reader,
                self.record_root.clone(),
                self.worker_config.as_ref().map(|config| config.responses),
            );
        }
    }

    /// Let tend say when the read it waits on is taking a while. A check on the tick that
    /// already runs: no timer, no new worker.
    fn update_stall(&mut self, now: Instant) {
        if let View::Tend(tend) = &mut self.view {
            tend.note_wait(
                self.discovery
                    .running_since()
                    .map(|since| now.saturating_duration_since(since)),
            );
        }
    }

    /// Read once and wait a bounded time, for the non-interactive `--snapshot` view only.
    pub fn discover_for_snapshot(&mut self, wait: Duration) {
        if self.sessions_root.is_none() {
            return;
        }
        self.request_sessions();
        let deadline = Instant::now() + wait;
        while !self.discovery.poll() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    fn request_sessions(&mut self) {
        self.discovery.scope_to_repo(
            self.repo.as_ref().map(|repo| repo.root().to_path_buf()),
            self.record_root.clone(),
        );
        if let Some(root) = &self.sessions_root {
            self.discovery.request(root);
        }
    }

    fn open_tend_session(&mut self, path: &Path) {
        self.open(Place::at(1).unwrap());
        if let View::Tend(tend) = &mut self.view {
            // One stat of a path the handoff just wrote; matching is otherwise by identity.
            tend.want = Some((path.to_path_buf(), path.canonicalize().ok()));
            tend.seen = 0;
        }
        self.sync_tend();
    }

    /// Receipts from confirmed goals, printed after the terminal is restored.
    pub fn finish(&mut self) -> Option<String> {
        if let View::Think(dump) = &mut self.view
            && let Some(receipt) = dump.finish_handoff()
        {
            self.receipts.push(receipt);
        }
        (!self.receipts.is_empty()).then(|| {
            self.receipts
                .iter()
                .map(seed_me::ExitReceipt::text)
                .collect::<Vec<_>>()
                .join("\n")
        })
    }

    /// Bracketed paste. Think, and Shape while it takes words, take text; elsewhere a paste
    /// does nothing.
    pub fn paste(&mut self, text: &str) {
        match &mut self.view {
            View::Think(dump) => dump.paste(text),
            View::Shape(shape, _) if !self.help => shape.paste(text),
            View::Taste(taste) if !self.help => taste.paste(text),
            _ => {}
        }
    }

    /// Text the open place asked to copy, taken once. The terminal loop does the copying and
    /// reports back through [`Home::copy_result`].
    pub fn take_copy_request(&mut self) -> Option<String> {
        match &mut self.view {
            View::Think(dump) => dump.take_copy_request(),
            View::Approve(view) => view.take_copy_request(),
            View::Taste(view) => view.take_copy_request(),
            _ => None,
        }
    }

    /// Whether `m` on the menu, or F1's Model and runs row in Think or Shape, asked for the
    /// Model panel since the last ask. Asking clears it.
    pub fn take_model_request(&mut self) -> bool {
        let place = match &mut self.view {
            View::Think(dump) => dump.take_model_request(),
            View::Shape(shape, _) => shape.take_model_request(),
            _ => false,
        };
        std::mem::take(&mut self.model_request) || place
    }

    /// Whether the last copy reached the clipboard, so the place can say so.
    pub fn copy_result(&mut self, sent: bool) {
        match &mut self.view {
            View::Think(dump) => dump.copy_result(sent),
            View::Approve(view) => view.copy_result(sent),
            View::Taste(view) => view.copy_result(sent),
            _ => {}
        }
    }

    /// Opens a checked place. Opening tend asks for a fresh scan; opening never starts an agent.
    pub fn open(&mut self, Place(index): Place) {
        if self.quit {
            return;
        }
        self.history_reader.cancel();
        self.approve_reader.cancel();
        self.approve_origin = None;
        self.selected = index;
        self.help = false;
        self.view = match index {
            0 => View::Think(Box::new((self.make_dump)().with_key_bar(self.key_bar))),
            1 => {
                // Deliberate reopening reads again; the last result shows meanwhile.
                self.request_sessions();
                View::Tend(Box::new(Tend::new(self.link_root.clone())))
            }
            2 => {
                self.request_sessions();
                View::Shape(
                    Box::new(
                        Shape::list()
                            .with_agent(self.shape_agent.clone())
                            .held(self.shape_held),
                    ),
                    None,
                )
            }
            3 => {
                self.request_sessions();
                View::Approve(Box::new(approve::Approve::open(self.sdlc_root.clone())))
            }
            4 => View::Watch(Box::new(
                Watch::open(self.sdlc_root.clone()).with_codex_report(self.codex_report.clone()),
            )),
            5 => View::Taste(Box::new(Taste::open(self.sdlc_root.clone(), self.agent))),
            6 => View::Measure(Box::new(measure::Measure::open(self.repo.clone()))),
            _ => unreachable!("Place is checked against PLACES"),
        };
        self.sync_tend();
        self.sync_shape();
        self.sync_approve();
    }

    /// Open only the chosen seed through normal verification, keeping this Ready return context.
    fn open_ready_seed(&mut self, target: crate::seed_me::ready::SeedTarget) {
        let View::Shape(shape, tend) = std::mem::replace(&mut self.view, View::Menu) else {
            return;
        };
        let origin = ApproveOrigin {
            shape,
            tend,
            selected: self.selected,
        };
        self.history_reader.cancel();
        self.approve_reader.cancel();
        self.request_sessions();
        self.selected = 3;
        self.help = false;
        self.view = View::Approve(Box::new(
            approve::Approve::open(self.sdlc_root.clone()).for_seed(target),
        ));
        self.approve_origin = Some(origin);
        self.sync_approve();
    }

    fn worker_busy(&self) -> bool {
        self.worker_panel.as_ref().is_some_and(worker::Panel::busy)
    }

    fn open_worker(&mut self, review: approve::WorkerReview) {
        if !self.worker_busy() {
            self.worker_panel = Some(worker::Panel::review(
                review,
                self.record_root.clone(),
                self.worker_config.clone(),
            ));
        }
        self.approve_reader.cancel();
        self.help = false;
        self.view = View::Worker;
    }

    /// One key press, routed to the open place or the menu. Releases are ignored.
    pub fn handle_key(&mut self, key: KeyEvent) {
        if key.kind == KeyEventKind::Release {
            return;
        }
        let quitting = (key.modifiers.contains(KeyModifiers::CONTROL)
            && key.code == KeyCode::Char('c'))
            || (matches!(self.view, View::Menu)
                && matches!(key.code, KeyCode::Char('q') | KeyCode::F(10)));
        if quitting && self.worker_busy() {
            self.quit_after_worker = true;
            self.view = View::Worker;
            self.help = false;
            return;
        }
        if matches!(self.view, View::Worker) {
            if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                self.quit = true;
            } else if let Some(destination) = self.worker_panel.as_mut().and_then(|p| p.key(key)) {
                self.help = false;
                self.view = match destination {
                    worker::Destination::Home => View::Menu,
                    worker::Destination::Watch(root) => {
                        View::Watch(Box::new(Watch::open(Some(root))))
                    }
                    worker::Destination::Taste(root) => {
                        View::Taste(Box::new(Taste::open(Some(root), self.agent)))
                    }
                };
            }
            return;
        }
        if !self.help
            && key.kind == KeyEventKind::Press
            && key.modifiers == KeyModifiers::NONE
            && key.code == KeyCode::Char('b')
            && let View::Watch(watch) = &self.view
            && let Some(run) = watch.worker_run()
        {
            if !self.worker_busy() {
                self.worker_panel = Some(worker::Panel::existing(run));
            }
            self.view = View::Worker;
            return;
        }
        if let View::Think(dump) = &mut self.view {
            dump.handle_key(key);
            return;
        }
        if matches!(self.view, View::Tend(_)) {
            let shifted_view_key = key.modifiers == KeyModifiers::SHIFT
                && matches!(
                    key.code,
                    KeyCode::Char('H' | 'J' | 'K' | 'L' | '+' | '=' | '-')
                );
            if key.modifiers != KeyModifiers::NONE && !shifted_view_key
                || key.kind == KeyEventKind::Repeat
                    && matches!(
                        key.code,
                        KeyCode::Char('a' | 'c' | 'r' | 'l' | 's')
                            | KeyCode::F(1)
                            | KeyCode::F(10)
                            | KeyCode::Enter
                    )
            {
                return;
            }
            if let View::Tend(tend) = &mut self.view {
                tend.release_pointer();
            }
            if key.code == KeyCode::F(10) {
                self.history_reader.cancel();
                self.view = View::Menu;
                self.help = false;
                return;
            }
            if matches!(key.code, KeyCode::F(1) | KeyCode::Char('?')) {
                self.help = !self.help;
                if let View::Tend(tend) = &mut self.view {
                    tend.how = false;
                    tend.help_scroll = 0;
                    if let Some(history) = &mut tend.history {
                        history.reset_help();
                    }
                }
                return;
            }
            if self.help {
                if let View::Tend(tend) = &mut self.view {
                    if let Some(history) = &mut tend.history {
                        match key.code {
                            KeyCode::Char('h') => {
                                tend.how = !tend.how;
                                history.reset_help();
                            }
                            KeyCode::Esc if tend.how => {
                                tend.how = false;
                                history.reset_help();
                            }
                            KeyCode::Esc => self.help = false,
                            code => history.scroll(code, true, tend.how),
                        }
                        return;
                    }
                    match key.code {
                        KeyCode::Char('h') => {
                            tend.how = !tend.how;
                            tend.help_scroll = 0;
                        }
                        KeyCode::Esc if tend.how => {
                            tend.how = false;
                            tend.help_scroll = 0;
                        }
                        KeyCode::Esc => self.help = false,
                        KeyCode::Down | KeyCode::PageDown => {
                            tend.help_scroll = tend.help_scroll.saturating_add(5)
                        }
                        KeyCode::Up | KeyCode::PageUp => {
                            tend.help_scroll = tend.help_scroll.saturating_sub(5)
                        }
                        _ => {}
                    }
                }
                return;
            }
            if let View::Tend(tend) = &mut self.view {
                if let Some(history) = &mut tend.history {
                    if key.code == KeyCode::Esc {
                        self.history_reader.cancel();
                        tend.history = None;
                    } else {
                        history.scroll(key.code, false, false);
                    }
                    return;
                }
                if key.code == KeyCode::Char('r')
                    && tend.detail
                    && let Some(session) = tend.current()
                {
                    let mut history = tend::history::View::waiting();
                    match &self.record_root {
                        Ok(root) => self.history_reader.request(
                            root.clone(),
                            session.path.clone(),
                            session.goal.clone(),
                        ),
                        Err(reason) => history.finish(Err(crate::think_record::Unavailable {
                            message: "The Think record folder is not configured.".into(),
                            details: reason.clone(),
                        })),
                    }
                    tend.history = Some(history);
                    return;
                }
            }
        }
        if let View::Approve(view) = &mut self.view {
            if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                self.approve_reader.cancel();
                self.quit = true;
            } else if key.modifiers == KeyModifiers::NONE && view.key(key, &mut self.help) {
                self.approve_reader.cancel();
                self.help = false;
                self.view = match self.approve_origin.take() {
                    Some(mut origin) if key.code != KeyCode::F(10) => {
                        self.selected = origin.selected;
                        origin.shape.reread();
                        View::Shape(origin.shape, origin.tend)
                    }
                    _ => View::Menu,
                };
            }
            if let View::Approve(view) = &mut self.view
                && let Some(review) = view.take_worker_request()
            {
                self.open_worker(review);
            }
            return;
        }
        if matches!(self.view, View::Retry(..)) {
            self.retry_key(key);
            return;
        }
        if matches!(self.view, View::Shape(..)) {
            self.shape_key(key);
            return;
        }
        if matches!(self.view, View::Watch(_) | View::Taste(_)) && key.code == KeyCode::F(10) {
            // A label being saved finishes first, so its outcome is never left unknown.
            let saving = matches!(&self.view, View::Taste(taste) if taste.saving());
            if !saving && key.modifiers == KeyModifiers::NONE && key.kind == KeyEventKind::Press {
                self.help = false;
                self.view = View::Menu;
            }
            return;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl && key.code == KeyCode::Char('c') || key.code == KeyCode::F(10) {
            self.quit = true;
            return;
        }
        // Taste's label form takes words: `?` and q type; F1 still opens help.
        if let View::Taste(taste) = &mut self.view
            && taste.takes_text()
            && !self.help
            && key.code != KeyCode::F(1)
        {
            taste.key(key);
            return;
        }
        if matches!(key.code, KeyCode::Char('?') | KeyCode::F(1)) {
            self.help = !self.help;
            return;
        }
        if self.help && key.code == KeyCode::Esc {
            self.help = false;
            return;
        }
        if self.help && matches!(self.view, View::Menu) {
            if key.code == KeyCode::Char('q') {
                self.quit = true;
            }
            return;
        }
        match &mut self.view {
            View::Menu => match key.code {
                KeyCode::Char(c @ '1'..='7') => {
                    self.open(Place::at(c as usize - '1' as usize).unwrap())
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.selected = (self.selected + 1) % PLACES.len()
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.selected = (self.selected + PLACES.len() - 1) % PLACES.len()
                }
                KeyCode::Enter => self.open(Place::at(self.selected).unwrap()),
                KeyCode::Char('q') => self.quit = true,
                KeyCode::Char('m') => self.model_request = true,
                KeyCode::Char('b')
                    if self.worker_panel.is_some()
                        && key.kind == KeyEventKind::Press
                        && key.modifiers == KeyModifiers::NONE =>
                {
                    self.view = View::Worker;
                }
                _ => {}
            },
            View::Tend(tend) => match key.code {
                KeyCode::Char('a') => tend.toggle_scope(),
                KeyCode::Char('l') => tend.start_link(),
                KeyCode::Esc if tend.is_linking() => tend.cancel_link(),
                KeyCode::Enter if tend.is_linking() => tend.commit_link(),
                KeyCode::Esc if tend.detail => {
                    tend.detail = false;
                    tend.want = None;
                    tend.scroll = 0;
                    tend.notice.clear();
                }
                KeyCode::Esc => self.view = View::Menu,
                KeyCode::Char('s')
                    if tend.detail && tend.current().is_some_and(|s| s.stage.can_shape()) =>
                {
                    let shape = Shape::session(&tend.current().unwrap().path)
                        .with_agent(self.shape_agent.clone())
                        .held(self.shape_held);
                    let View::Tend(tend) = std::mem::replace(&mut self.view, View::Menu) else {
                        unreachable!()
                    };
                    self.view = View::Shape(Box::new(shape), Some(tend));
                }
                KeyCode::Down | KeyCode::Char('j') if tend.detail => {
                    tend.scroll = tend.scroll.saturating_add(1)
                }
                KeyCode::Up | KeyCode::Char('k') if tend.detail => {
                    tend.scroll = tend.scroll.saturating_sub(1)
                }
                KeyCode::PageDown if tend.detail => {
                    tend.scroll = tend.scroll.saturating_add(crate::page(tend.page))
                }
                KeyCode::PageUp if tend.detail => {
                    tend.scroll = tend.scroll.saturating_sub(crate::page(tend.page))
                }
                KeyCode::Right | KeyCode::Down | KeyCode::Char('j') if tend.garden_showing() => {
                    tend.step_selection(true)
                }
                KeyCode::Left | KeyCode::Up | KeyCode::Char('k') if tend.garden_showing() => {
                    tend.step_selection(false)
                }
                KeyCode::Char(c @ ('+' | '=' | '-' | 'H' | 'J' | 'K' | 'L'))
                    if tend.garden_showing() =>
                {
                    tend.adjust_view(c);
                }
                KeyCode::Enter if tend.garden_showing() => {
                    tend.detail = true;
                    tend.scroll = 0;
                    tend.notice.clear();
                }
                _ => {}
            },
            View::Measure(measure) => {
                if self.help {
                    if key.code == KeyCode::Char('q') {
                        self.help = false;
                        self.view = View::Menu;
                    }
                } else if measure.key(key) {
                    self.view = View::Menu;
                }
            }
            View::Watch(watch) => {
                if self.help {
                    if key.code == KeyCode::Char('q') {
                        self.help = false;
                        self.view = View::Menu;
                    }
                    return;
                }
                if watch.key(key) {
                    self.view = View::Menu;
                }
            }
            View::Taste(_) => self.taste_key(key),
            View::Retry(..) => {}
            View::Think(_) | View::Shape(..) | View::Approve(_) | View::Worker => {}
        }
    }

    /// Shape's keys: F1 (or ? when Shape isn't taking words) for help, where a digit runs its
    /// numbered action, and Esc steps back (to the same Tend detail, freshly read). Held keys
    /// don't repeat an open, a save or a confirmation. Shift types capitals. Only F2 and the
    /// numbered actions write or run.
    fn shape_key(&mut self, key: KeyEvent) {
        let typing = matches!(&self.view, View::Shape(shape, _) if shape.takes_text());
        let editor_key = typing
            && !self.help
            && matches!(
                key.code,
                KeyCode::Char(_)
                    | KeyCode::Left
                    | KeyCode::Right
                    | KeyCode::Up
                    | KeyCode::Down
                    | KeyCode::Home
                    | KeyCode::End
                    | KeyCode::Backspace
                    | KeyCode::Delete
                    | KeyCode::Enter
                    | KeyCode::Tab
            );
        let shifted_char =
            key.modifiers == KeyModifiers::SHIFT && matches!(key.code, KeyCode::Char(_));
        if key.modifiers != KeyModifiers::NONE && !editor_key && !shifted_char
            || key.kind == KeyEventKind::Repeat
                && (matches!(key.code, KeyCode::F(2..=5)) || key.code == KeyCode::Enter && !typing)
        {
            return;
        }
        match key.code {
            KeyCode::Char('?') if typing && !self.help => {
                if let View::Shape(shape, _) = &mut self.view {
                    shape.key(key);
                }
            }
            KeyCode::F(1) | KeyCode::Char('?') => {
                self.help = !self.help;
                if let View::Shape(shape, _) = &mut self.view {
                    shape.reset_help();
                }
            }
            KeyCode::Esc if self.help => self.help = false,
            _ if self.help => {
                if let View::Shape(shape, _) = &mut self.view {
                    if shape.pick(key) {
                        self.help = false;
                    } else {
                        shape.scroll_help(key.code);
                    }
                }
            }
            _ => {
                let View::Shape(shape, _) = &mut self.view else {
                    return;
                };
                let leave = shape.key(key);
                if shape.wants_sessions() {
                    self.request_sessions();
                }
                if leave == Leave::Back {
                    let View::Shape(_, back) = std::mem::replace(&mut self.view, View::Menu) else {
                        unreachable!()
                    };
                    if let Some(mut tend) = back {
                        tend.want = tend.current().map(|s| (s.path.clone(), None));
                        self.view = View::Tend(tend);
                        self.request_sessions();
                        self.sync_tend();
                    }
                }
            }
        }
        let target = match &mut self.view {
            View::Shape(shape, _) => shape.take_approve_request(),
            _ => None,
        };
        if let Some(target) = target {
            self.open_ready_seed(target);
        }
    }

    /// One mouse event in a terminal of size `area`. Routes deliberate presses to the open place.
    pub fn handle_mouse(&mut self, event: MouseEvent, area: Rect) {
        self.handle_mouse_at(event, area, Instant::now());
    }

    /// [`Home::handle_mouse`] with the clock as an input, so double-click tests never sleep.
    fn handle_mouse_at(&mut self, event: MouseEvent, area: Rect, now: Instant) {
        if let View::Think(dump) = &mut self.view {
            dump.handle_mouse(event, area);
            return;
        }
        if let View::Tend(tend) = &mut self.view {
            if !self.help {
                tend.pointer(event, body(area), now);
            }
            return;
        }
        if event.kind != MouseEventKind::Down(MouseButton::Left) {
            return;
        }
        if !matches!(self.view, View::Menu) {
            return;
        }
        if event.row == 1 && event.column + 3 >= area.width {
            self.help = !self.help;
            return;
        }
        if self.help {
            return;
        }
        if let Some(index) =
            (0..PLACES.len()).find(|i| event.row == FIRST_ROW + *i as u16 * ROW_GAP)
        {
            self.open(Place::at(index).unwrap());
        }
    }

    /// The Think reading associated with this frame, if one awaits successful output.
    pub fn presentation(&self) -> Option<brain_dump::Presentation> {
        match &self.view {
            View::Think(dump) => dump.presentation(),
            _ => None,
        }
    }

    /// Acknowledge successful terminal output to the open Think visit only.
    pub fn presented(&mut self, presentation: brain_dump::Presentation) {
        if let View::Think(dump) = &mut self.view {
            dump.presented(presentation);
        }
    }

    /// Memory only: an exact count of active sessions once a complete, current read has
    /// landed. Unknown, partial or failed reads show nothing, never zero.
    pub fn count(&self, index: usize) -> Option<String> {
        if index != 1 {
            return None;
        }
        self.discovery
            .exact_active()
            .filter(|active| *active > 0)
            .map(|active| active.to_string())
    }
}

/// Paints the menu or the open place. Painting never waits on the session scan.
pub fn render(frame: &mut Frame, home: &mut Home, palette: Palette) {
    if let View::Think(dump) = &mut home.view {
        brain_dump::render(frame, dump, palette);
        return;
    }
    let area = frame.area();
    frame.render_widget(Paragraph::new("").style(palette.ink), area);
    let title = match &home.view {
        View::Menu => "tinkery".to_string(),
        View::Tend(_) => "tinkery › tend".into(),
        View::Shape(..) | View::Retry(..) => "tinkery › shape".into(),
        View::Approve(_) => "tinkery › approve".into(),
        View::Worker => "tinkery › worker".into(),
        View::Measure(measure) => measure.title(),
        View::Watch(_) => "tinkery › watch".into(),
        View::Taste(_) => "tinkery › taste".into(),
        View::Think(_) => unreachable!(),
    };
    let mut title = vec![Span::styled(title, palette.ink)];
    if let (View::Menu, Some(repo)) = (&home.view, &home.repo) {
        title.push(Span::styled(format!("  in {}", repo.name()), palette.muted));
    }
    frame.render_widget(
        Paragraph::new(Line::from(title)),
        Rect::new(
            2,
            1,
            crate::header_end(area, area.width.saturating_sub(4)).saturating_sub(2),
            1,
        ),
    );
    crate::render_theme_switch(frame, palette);
    if area.width >= 4 {
        frame.render_widget(
            Paragraph::new("?").style(palette.muted),
            Rect::new(area.width - 3, 1, 1, 1),
        );
    }
    let body = body(area);
    if let View::Tend(tend) = &mut home.view
        && let Some(history) = &mut tend.history
    {
        history.render(frame, body, palette, home.help, tend.how);
        return;
    }
    if matches!(home.view, View::Worker) {
        if let Some(panel) = &mut home.worker_panel {
            panel.render(frame, body, palette);
        }
        if home.quit_after_worker {
            frame.render_widget(
                Paragraph::new("Finishing the current request before exit…").style(palette.muted),
                Rect::new(body.x, body.bottom().saturating_sub(2), body.width, 1),
            );
        }
        return;
    }
    if let View::Retry(view, _) = &mut home.view {
        view.render(frame, body, palette, !home.agent, home.help);
        return;
    }
    if let View::Approve(view) = &mut home.view {
        view.render(frame, body, palette, home.help);
        return;
    }
    if let View::Measure(measure) = &mut home.view {
        measure.render(frame, body, palette, home.help);
        return;
    }
    if home.help && !matches!(home.view, View::Menu) {
        if let View::Shape(shape, _) = &mut home.view {
            shape.render_help(frame, body, palette);
            return;
        }
        if let View::Taste(taste) = &home.view {
            frame.render_widget(
                Paragraph::new(taste.help_text())
                    .style(palette.ink)
                    .wrap(Wrap { trim: false }),
                body,
            );
            return;
        }
        if let View::Watch(watch) = &home.view {
            frame.render_widget(
                Paragraph::new(watch.help_text())
                    .style(palette.ink)
                    .wrap(Wrap { trim: false }),
                body,
            );
            return;
        }
        if let View::Tend(tend) = &mut home.view {
            let p = Paragraph::new(tend.help_text())
                .wrap(Wrap { trim: false })
                .style(palette.ink);
            let max = p
                .line_count(body.width)
                .saturating_sub(body.height as usize) as u16;
            tend.help_scroll = tend.help_scroll.min(max);
            frame.render_widget(p.scroll((tend.help_scroll, 0)), body);
            return;
        }
    }
    match &mut home.view {
        View::Menu => {
            for (i, (name, description)) in PLACES.iter().enumerate() {
                let y = FIRST_ROW + i as u16 * ROW_GAP;
                if y >= area.height.saturating_sub(2) {
                    break;
                }
                let style = if i == home.selected {
                    palette.jade
                } else {
                    palette.ink
                };
                let mut spans = vec![Span::styled(format!("{}  {name}", i + 1), style)];
                spans.push(Span::styled(
                    format!(
                        "{:>width$}  {description}",
                        home.count(i).unwrap_or_default(),
                        width = 15 - name.len()
                    ),
                    palette.muted,
                ));
                frame.render_widget(
                    Paragraph::new(Line::from(spans)),
                    Rect::new(2, y, area.width.saturating_sub(4), 1),
                );
            }
            if let Some(receipt) = home.receipts.last() {
                frame.render_widget(
                    Paragraph::new(receipt.label()).style(palette.muted),
                    Rect::new(
                        5,
                        FIRST_ROW + PLACES.len() as u16 * ROW_GAP + 1,
                        area.width.saturating_sub(7),
                        1,
                    ),
                );
            }
            if home.help {
                render_menu_help(frame, palette);
            } else {
                frame.render_widget(
                    Paragraph::new(if home.worker_panel.is_some() {
                        "↑↓ move   enter open   b worker   ? help   q quit"
                    } else {
                        "↑↓ move   enter open   ? help   q quit"
                    })
                    .style(palette.muted),
                    Rect::new(
                        2,
                        area.height.saturating_sub(2),
                        area.width.saturating_sub(4),
                        1,
                    ),
                );
            }
        }
        View::Tend(tend) => tend::render(frame, tend, body, palette),
        View::Shape(shape, _) => shape.render(frame, body, palette),
        View::Measure(_) | View::Retry(..) => unreachable!(),
        View::Watch(watch) => watch.render(frame, body, palette),
        View::Taste(taste) => taste.render(frame, body, palette),
        View::Think(_) | View::Approve(_) | View::Worker => {}
    }
}

fn body(area: Rect) -> Rect {
    Rect::new(
        2,
        3,
        area.width.saturating_sub(4),
        area.height.saturating_sub(4),
    )
}

fn render_menu_help(frame: &mut Frame, palette: Palette) {
    let area = frame.area();
    let width = 38.min(area.width.saturating_sub(4));
    let panel = Rect::new(area.width.saturating_sub(width + 2), 3, width, 14).intersection(area);
    let action = |key: &str, label: &str| {
        Line::from(vec![
            Span::styled(format!("{key:<8}"), palette.ink.bold()),
            Span::styled(label.to_owned(), palette.muted),
        ])
    };
    let lines = vec![
        Line::default(),
        Line::styled("Move around", palette.ink.bold()),
        Line::default(),
        action("↑ ↓", "Choose a place"),
        action("Enter", "Open it"),
        action("1–7", "Open directly"),
        Line::default(),
        action("q", "Quit Tinkery"),
        action("m", "Model and runs"),
        if palette.theme.is_some() {
            action("Ctrl-T", "Dark or light")
        } else {
            Line::default()
        },
        Line::default(),
        action("Esc", "Close help"),
    ];
    let block = Block::default()
        .title(Span::styled(" Help ", palette.jade))
        .borders(Borders::ALL)
        .border_style(palette.muted)
        .padding(Padding::new(1, 1, 0, 0));
    frame.render_widget(Clear, panel);
    frame.render_widget(Paragraph::new(lines).style(palette.ink).block(block), panel);
}

/// The screen as plain text at `width`×`height`, for `--snapshot` and tests. No terminal mode,
/// no provider calls.
pub fn snapshot(
    width: u16,
    height: u16,
    home: &mut Home,
    no_color: bool,
) -> Result<String, std::convert::Infallible> {
    let backend = ratatui::backend::TestBackend::new(width, height);
    let mut terminal = ratatui::Terminal::new(backend)?;
    terminal.draw(|frame| render(frame, home, Palette::new(no_color)))?;
    let buffer = terminal.backend().buffer();
    Ok((0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n"))
}

#[cfg(test)]
mod history_tests;
#[cfg(test)]
mod measure_tests;
#[cfg(test)]
mod menu_tests;
#[cfg(test)]
mod tests {
    use super::*;
    use crate::tend::{Stage, load_sessions};
    use serde_json::Value;
    use std::fs;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn home(root: &Path) -> Home {
        Home::new(
            Box::new(BrainDump::default),
            Ok(root.to_path_buf()),
            Ok(root.join("TEST-links")),
        )
    }

    /// Tick until the read that was asked for has landed (bounded).
    fn settle(h: &mut Home) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            h.tick();
            if h.discovery.requested() && !h.discovery.running() && h.discovery.fresh() {
                return;
            }
            assert!(Instant::now() < deadline, "discovery did not settle");
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    fn session(root: &Path, id: &str, status: &str, updated: &str, seed: Option<&str>) {
        let dir = root.join(id);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("ledger.json"),
            serde_json::json!({"status": status, "goal": format!("goal {id}"), "updated_at": updated}).to_string(),
        )
        .unwrap();
        if let Some(first) = seed {
            fs::write(dir.join("seed-contract.md"), format!("{first}\n# Seed\n")).unwrap();
        }
    }

    #[test]
    fn home_shows_seven_places_and_only_counts_what_exists() {
        let tmp = tempfile::tempdir().unwrap();
        session(tmp.path(), "a", "active", "2026-10-09T10:00:00Z", None);
        let mut h = home(tmp.path());
        settle(&mut h);
        let screen = snapshot(100, 30, &mut h, true).unwrap();
        for name in [
            "think", "tend", "shape", "approve", "watch", "taste", "measure",
        ] {
            assert!(screen.contains(name), "{name} missing:\n{screen}");
        }
        assert!(screen.contains("tinkery"));
        assert_eq!(h.count(1), Some("1".into()));
        assert_eq!(h.count(0), None);
    }

    #[test]
    fn home_key_bar_keeps_inline_descriptions_and_receipt_space() {
        let tmp = tempfile::tempdir().unwrap();
        let bar = "↑↓ move   enter open   ? help   q quit";
        for (width, height) in [(80, 24), (100, 30), (160, 40), (320, 40)] {
            for no_color in [false, true] {
                let mut h = home(tmp.path());
                let screen = snapshot(width, height, &mut h, no_color).unwrap();
                assert_eq!(screen.lines().last().unwrap().trim(), bar, "{screen}");
                assert_eq!(screen.matches(bar).count(), 1);
                assert!(screen.lines().nth(19).unwrap().is_empty());
                h.handle_key(key(KeyCode::Down));
                let screen = snapshot(width, height, &mut h, no_color).unwrap();
                assert!(screen.lines().nth(19).unwrap().is_empty());
                assert!(screen.lines().nth(height as usize - 3).unwrap().is_empty());
                assert_eq!(screen.lines().last().unwrap().trim(), bar);
                h.handle_key(key(KeyCode::F(1)));
                assert!(
                    !snapshot(width, height, &mut h, no_color)
                        .unwrap()
                        .contains(bar)
                );
                h.handle_key(key(KeyCode::Esc));
                h.handle_key(key(KeyCode::Char('4')));
                assert!(
                    !snapshot(width, height, &mut h, no_color)
                        .unwrap()
                        .contains(bar)
                );
            }
        }
    }

    #[test]
    fn menu_help_is_a_bounded_overlay_with_the_menu_still_visible() {
        let tmp = tempfile::tempdir().unwrap();
        for (width, height) in [(80, 24), (100, 30), (160, 40), (320, 40)] {
            for no_color in [false, true] {
                let mut h = home(tmp.path());
                h.handle_key(key(KeyCode::F(1)));
                let screen = snapshot(width, height, &mut h, no_color).unwrap();
                let rows: Vec<_> = screen.split('\n').collect();
                for (index, (name, _)) in PLACES.iter().enumerate() {
                    assert!(rows[4 + index * 2].starts_with(&format!("  {}  {name}", index + 1)));
                }
                assert!(rows[19].is_empty());
                assert_eq!(screen.contains("Browse recent pull requests"), width >= 100);
                assert!(screen.contains("Choose a place") && screen.contains("Esc     Close help"));
                let panel_content = |row: &str| {
                    row.chars()
                        .skip(width as usize - 38)
                        .take(34)
                        .collect::<String>()
                };
                assert!(panel_content(rows[4]).trim().is_empty());
                assert!(panel_content(rows[5]).starts_with("Move around"));
                assert!(!screen.contains("Leave"));
                assert_eq!(
                    rows[3].chars().position(|c| c == '┌'),
                    Some(width as usize - 40)
                );
                assert!(rows[16].starts_with("  7  measure") && rows[16].contains('└'));
                assert!(panel_content(rows[12]).starts_with("m       Model and runs"));
                assert!(!screen.contains("↑↓ move   enter open"));
                assert!(!screen.contains("Inside think"));
                h.handle_key(key(KeyCode::Esc));
                let closed = snapshot(width, height, &mut h, no_color).unwrap();
                assert!(closed.contains("Browse recent pull requests"));
                assert!(!closed.contains('┌') && closed.contains("↑↓ move   enter open"));
            }
        }
    }

    #[test]
    fn menu_help_uses_jade_for_place_bold_keys_and_quiet_labels_and_border() {
        use ratatui::style::{Color, Modifier};
        let tmp = tempfile::tempdir().unwrap();
        for no_color in [false, true] {
            let mut h = home(tmp.path());
            h.handle_key(key(KeyCode::F(1)));
            let palette = Palette::new(no_color);
            let backend = ratatui::backend::TestBackend::new(100, 30);
            let mut terminal = ratatui::Terminal::new(backend).unwrap();
            terminal
                .draw(|frame| render(frame, &mut h, palette))
                .unwrap();
            let buffer = terminal.backend().buffer();
            let find = |needle: &str| -> (u16, u16) {
                let want: Vec<String> = needle.chars().map(String::from).collect();
                (0..buffer.area.height)
                    .find_map(|y| {
                        let row: Vec<&str> = (0..buffer.area.width)
                            .map(|x| buffer[(x, y)].symbol())
                            .collect();
                        row.windows(want.len())
                            .position(|w| w.iter().zip(&want).all(|(a, b)| a == b))
                            .map(|x| (x as u16, y))
                    })
                    .unwrap_or_else(|| panic!("{needle:?} is not on screen"))
            };
            let title = find("Help");
            assert_eq!(buffer[title].fg, palette.jade.fg.unwrap_or(Color::Reset));
            assert!(buffer[title].modifier.contains(Modifier::BOLD));
            for key in [
                "Move around",
                "↑ ↓     Choose",
                "Enter   Open it",
                "1–7     Open",
                "q       Quit",
                "Esc     Close help",
            ] {
                let at = find(key);
                assert_eq!(
                    buffer[at].fg,
                    palette.ink.fg.unwrap_or(Color::Reset),
                    "{key}"
                );
                assert!(buffer[at].modifier.contains(Modifier::BOLD), "{key}");
            }
            let border = find("Move around");
            let mut quiet = vec![(border.0 - 2, border.1)];
            for label in [
                "Choose a place",
                "Open it",
                "Open directly",
                "Quit Tinkery",
                "Close help",
            ] {
                quiet.push(find(label));
            }
            for at in quiet {
                assert_eq!(buffer[at].fg, palette.muted.fg.unwrap_or(Color::Reset));
                assert!(!buffer[at].modifier.contains(Modifier::BOLD));
                assert_eq!(buffer[at].modifier.contains(Modifier::DIM), no_color);
            }
        }
    }

    #[test]
    fn menu_help_blocks_underlying_keys_paste_and_clicks_but_keeps_close_and_quit() {
        let tmp = tempfile::tempdir().unwrap();
        for code in [
            KeyCode::Char('1'),
            KeyCode::Char('2'),
            KeyCode::Char('3'),
            KeyCode::Char('4'),
            KeyCode::Char('5'),
            KeyCode::Char('6'),
            KeyCode::Char('7'),
            KeyCode::Enter,
            KeyCode::Down,
            KeyCode::Up,
            KeyCode::Char('j'),
            KeyCode::Char('k'),
        ] {
            let mut h = home(tmp.path());
            h.handle_key(key(KeyCode::F(1)));
            h.handle_key(key(code));
            h.paste("1\nq");
            h.handle_mouse(
                MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column: 5,
                    row: 6,
                    modifiers: KeyModifiers::NONE,
                },
                Rect::new(0, 0, 80, 24),
            );
            assert!(h.help && matches!(h.view, View::Menu) && h.selected == 0 && !h.quit);
        }
        for close in [KeyCode::Esc, KeyCode::F(1), KeyCode::Char('?')] {
            let mut h = home(tmp.path());
            h.handle_key(key(KeyCode::F(1)));
            h.handle_key(key(close));
            assert!(!h.help && matches!(h.view, View::Menu) && !h.quit);
        }
        let mut h = home(tmp.path());
        h.handle_key(key(KeyCode::F(1)));
        h.handle_mouse(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 77,
                row: 1,
                modifiers: KeyModifiers::NONE,
            },
            Rect::new(0, 0, 80, 24),
        );
        assert!(!h.help && matches!(h.view, View::Menu));
        for quit in [
            key(KeyCode::Char('q')),
            key(KeyCode::F(10)),
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
        ] {
            let mut h = home(tmp.path());
            h.handle_key(key(KeyCode::F(1)));
            h.handle_key(quit);
            assert!(h.quit);
        }
    }

    #[test]
    fn menu_descriptions_distinguish_outputs_without_granting_permission() {
        let tmp = tempfile::tempdir().unwrap();
        let descriptions = [
            "Clarify a goal",
            "Open saved goals",
            "Prepare a brief",
            "Prepare work for approval",
            "See work status",
            "Review a result",
            "Browse recent pull requests",
        ];
        for (width, height) in [(80, 24), (100, 30), (160, 40), (320, 40)] {
            for no_color in [false, true] {
                let mut h = home(tmp.path());
                for _ in 0..PLACES.len() {
                    let screen = snapshot(width, height, &mut h, no_color).unwrap();
                    for (index, description) in descriptions.iter().enumerate() {
                        assert_eq!(
                            screen.lines().nth(4 + index * 2).unwrap().find(description),
                            Some(22)
                        );
                        assert_eq!(screen.matches(description).count(), 1);
                    }
                    assert!(screen.lines().nth(19).unwrap().is_empty());
                    assert!(matches!(h.view, View::Menu));
                    assert!(!h.quit && h.take_copy_request().is_none());
                    h.handle_key(key(KeyCode::Down));
                }
                let saved = tmp.path().join("TEST-saved");
                let mut dump = BrainDump::default();
                dump.set_saved(seed_me::Receipt::test(saved.clone()));
                let mut h = Home::from_receipt(
                    dump,
                    &saved,
                    false,
                    Ok(tmp.path().join("TEST-links")),
                    None,
                );
                let screen = snapshot(width, height, &mut h, no_color).unwrap();
                assert_eq!(screen.lines().nth(19).unwrap().trim(), "Saved.");
                for description in descriptions {
                    assert_eq!(screen.matches(description).count(), 1);
                }
            }
        }
    }

    #[test]
    fn home_menu_uses_one_grid_and_keeps_the_count_near_its_place() {
        let tmp = tempfile::tempdir().unwrap();
        for i in 0..10 {
            session(
                tmp.path(),
                &format!("s{i}"),
                "active",
                "2026-10-09T10:00:00Z",
                None,
            );
        }
        let mut h = home(tmp.path());
        settle(&mut h);
        for (width, height) in [(80, 24), (100, 30), (160, 40), (320, 40)] {
            for no_color in [false, true] {
                let screen = snapshot(width, height, &mut h, no_color).unwrap();
                let rows: Vec<_> = screen.split('\n').collect();
                assert_eq!(rows[1].find("tinkery"), Some(2));
                assert!(rows[4].starts_with("  1  think"));
                assert!(rows[6].starts_with("  2  tend         10"));
                assert_eq!(rows[6].find("Open saved goals"), Some(22));
                assert!(rows[8].starts_with("  3  shape"));
                assert!(rows[16].starts_with("  7  measure"));
                assert!(rows[17].is_empty() && rows[18].is_empty());
                assert!(rows[19].is_empty());
                assert_eq!(rows[height as usize - 2].find('↑'), Some(2));
                assert!(
                    rows[height as usize - 1].is_empty(),
                    "footer needs a blank row below"
                );
            }
        }
    }

    #[test]
    fn a_toggles_only_tend_garden_and_never_repeats_or_conflicts_with_shape() {
        let tmp = tempfile::tempdir().unwrap();
        for id in ["a", "b"] {
            session(tmp.path(), id, "active", "2026-10-17T00:00:00Z", None);
        }
        let mut h = home(tmp.path());
        h.open(Place::at(1).unwrap());
        settle(&mut h);
        let View::Tend(tend) = &mut h.view else {
            panic!()
        };
        tend.repo_paths = Some([tend.sessions[0].path.clone()].into_iter().collect());
        tend.toggle_scope(); // all
        tend.toggle_scope(); // this repo
        assert_eq!(tend.sessions.len(), 1);
        h.handle_key(KeyEvent::new_with_kind(
            KeyCode::Char('a'),
            KeyModifiers::NONE,
            KeyEventKind::Repeat,
        ));
        let View::Tend(tend) = &h.view else { panic!() };
        assert_eq!(tend.sessions.len(), 1);
        h.handle_key(key(KeyCode::Char('s')));
        assert!(matches!(h.view, View::Tend(_)));
        h.handle_key(key(KeyCode::F(1)));
        h.handle_key(key(KeyCode::Char('a')));
        h.handle_key(key(KeyCode::Esc));
        let View::Tend(tend) = &h.view else { panic!() };
        assert_eq!(tend.sessions.len(), 1, "help ignores a");
        h.handle_key(key(KeyCode::Char('a')));
        let View::Tend(tend) = &h.view else { panic!() };
        assert_eq!(tend.sessions.len(), 2);
        h.handle_key(key(KeyCode::Enter));
        h.handle_key(key(KeyCode::Char('a')));
        let View::Tend(tend) = &h.view else { panic!() };
        assert!(tend.detail && tend.sessions.len() == 2);
        h.handle_key(key(KeyCode::Esc));
        h.handle_key(key(KeyCode::Char('a')));
        let View::Tend(tend) = &h.view else { panic!() };
        assert_eq!(tend.sessions.len(), 1);
    }

    #[test]
    fn s_in_tend_detail_opens_shape_and_esc_restores_the_same_detail() {
        let tmp = tempfile::tempdir().unwrap();
        session(tmp.path(), "a", "active", "2026-10-09T10:00:00Z", None);
        let mut h = home(tmp.path());
        h.open(Place::at(1).unwrap());
        settle(&mut h);
        h.handle_key(key(KeyCode::Char('s')));
        assert!(matches!(h.view, View::Tend(_)), "the garden has no s");
        h.handle_key(key(KeyCode::Enter));
        let mut held = key(KeyCode::Char('s'));
        held.kind = KeyEventKind::Repeat;
        h.handle_key(held);
        assert!(matches!(h.view, View::Tend(_)), "a held key opens nothing");
        h.handle_key(key(KeyCode::Char('s')));
        assert!(matches!(h.view, View::Shape(..)));
        let screen = snapshot(100, 30, &mut h, true).unwrap();
        assert!(screen.contains("tinkery › shape"), "{screen}");
        assert!(
            screen.contains("Reading where this seed stands."),
            "{screen}"
        );
        // A terminal may report ? with Shift; it still opens and closes help.
        let shifted = KeyEvent::new(KeyCode::Char('?'), KeyModifiers::SHIFT);
        h.handle_key(shifted);
        assert!(h.help, "Shift+? opens help");
        h.handle_key(shifted);
        assert!(!h.help, "Shift+? closes help");
        h.handle_key(key(KeyCode::Esc));
        let View::Tend(tend) = &h.view else { panic!() };
        assert!(tend.detail);
        assert!(tend.current().unwrap().path.ends_with("a"));
        fs::write(tmp.path().join("a/ledger.json"), r#"{"status": "active"}"#).unwrap();
        h.handle_key(key(KeyCode::Esc));
        h.open(Place::at(1).unwrap());
        settle(&mut h);
        h.handle_key(key(KeyCode::Enter));
        h.handle_key(key(KeyCode::Char('s')));
        assert!(
            matches!(h.view, View::Tend(_)),
            "no s when the stage is unknown"
        );
    }

    #[test]
    fn returning_from_shape_rereads_the_same_detail_and_garden_stage() {
        for (seed, stage, next) in [
            (
                "status: draft",
                Stage::SeedDrafted,
                "confirm the seed draft in Shape",
            ),
            (
                "status: confirmed for intake",
                Stage::SeedUnchecked,
                "check the seed in Shape",
            ),
        ] {
            let tmp = tempfile::tempdir().unwrap();
            session(tmp.path(), "a", "active", "2026-10-09T10:00:00Z", None);
            let mut h = home(tmp.path());
            h.open(Place::at(1).unwrap());
            settle(&mut h);
            h.handle_key(key(KeyCode::Enter));
            h.handle_key(key(KeyCode::Char('s')));
            assert!(matches!(h.view, View::Shape(..)));
            session(
                tmp.path(),
                "a",
                "active",
                "2026-10-09T11:00:00Z",
                Some(seed),
            );
            // A newer goal must not steal selection while the same detail refreshes.
            session(tmp.path(), "b", "active", "2026-10-09T12:00:00Z", None);
            h.handle_key(key(KeyCode::Esc));
            settle(&mut h);
            let View::Tend(tend) = &h.view else { panic!() };
            assert!(tend.detail);
            assert!(tend.current().unwrap().path.ends_with("a"));
            assert_eq!(tend.current().unwrap().stage, stage);
            let screen = snapshot(80, 24, &mut h, true).unwrap();
            assert!(screen.contains(next), "{screen}");
            h.handle_key(key(KeyCode::Esc));
            let screen = snapshot(80, 24, &mut h, true).unwrap();
            assert!(screen.contains(stage.label()), "{screen}");
            assert!(h.take_copy_request().is_none());
        }
    }

    #[test]
    fn digits_open_places_and_esc_returns_home() {
        let tmp = tempfile::tempdir().unwrap();
        let mut h = home(tmp.path());
        h.handle_key(key(KeyCode::Char('4')));
        assert!(matches!(h.view, View::Approve(_)));
        let shown = snapshot(100, 30, &mut h, true).unwrap();
        assert!(shown.contains("tinkery › approve") && shown.contains("esc back"));
        assert!(h.take_copy_request().is_none());
        h.handle_key(key(KeyCode::Esc));
        assert!(matches!(h.view, View::Menu));
        h.handle_key(key(KeyCode::Char('1')));
        assert!(matches!(h.view, View::Think(_)));
    }

    #[test]
    fn leaving_think_returns_home_not_out_of_the_app() {
        let tmp = tempfile::tempdir().unwrap();
        let mut h = home(tmp.path());
        h.open(Place::at(0).unwrap());
        if let View::Think(dump) = &mut h.view {
            dump.handle_key(key(KeyCode::Esc));
        }
        h.tick();
        assert!(matches!(h.view, View::Menu));
        assert!(!h.quit);
    }

    #[test]
    fn key_bar_preference_lasts_across_visits_to_think() {
        let tmp = tempfile::tempdir().unwrap();
        let mut h = home(tmp.path());
        h.open(Place::at(0).unwrap());
        if let View::Think(dump) = &mut h.view {
            assert!(dump.key_bar_on(), "on by default");
            dump.handle_key(key(KeyCode::F(1)));
            dump.handle_key(key(KeyCode::Char('b')));
            dump.handle_key(key(KeyCode::Esc));
            dump.handle_key(key(KeyCode::Esc));
        }
        h.tick();
        assert!(matches!(h.view, View::Menu));
        h.open(Place::at(0).unwrap());
        let View::Think(dump) = &h.view else { panic!() };
        assert!(!dump.key_bar_on(), "turned off stays off");
    }

    #[test]
    fn tend_reads_stage_from_seed_me_files_newest_first() {
        let tmp = tempfile::tempdir().unwrap();
        session(tmp.path(), "old", "active", "2026-10-01T00:00:00Z", None);
        session(
            tmp.path(),
            "new",
            "active",
            "2026-10-09T00:00:00Z",
            Some("status: confirmed for intake"),
        );
        session(
            tmp.path(),
            "draft",
            "active",
            "2026-10-05T00:00:00Z",
            Some("status: draft"),
        );
        fs::create_dir_all(tmp.path().join("broken")).unwrap();
        fs::write(tmp.path().join("broken/ledger.json"), "not json").unwrap();
        let sessions = load_sessions(tmp.path()).unwrap();
        let got: Vec<(&str, &str)> = sessions
            .iter()
            .map(|s| (s.goal.as_str(), s.stage.label()))
            .collect();
        assert_eq!(
            got,
            vec![
                ("goal new", "seed needs checking"),
                ("goal draft", "seed drafted"),
                ("goal old", "goal saved")
            ]
        );
    }

    #[test]
    fn receipt_tend_request_selects_exact_session_and_missing_is_explicit() {
        let tmp = tempfile::tempdir().unwrap();
        session(tmp.path(), "old", "active", "2026-10-01T00:00:00Z", None);
        session(tmp.path(), "new", "active", "2026-10-10T00:00:00Z", None);
        session(tmp.path(), "ended", "stopped", "2026-10-11T00:00:00Z", None);
        let path = tmp.path().join("old");
        let before = fs::read(path.join("ledger.json")).unwrap();
        let mut h = home(tmp.path());
        h.open(Place::at(0).unwrap());
        let View::Think(dump) = &mut h.view else {
            panic!()
        };
        dump.set_saved(crate::seed_me::Receipt::test(path.clone()));
        h.handle_key(key(KeyCode::Char('s')));
        h.tick();
        settle(&mut h);
        let View::Tend(tend) = &h.view else { panic!() };
        assert_eq!(tend.selected, 1);
        assert_eq!(
            tend.sessions[tend.selected].path,
            path.canonicalize().unwrap()
        );
        assert!(tend.detail && tend.error.is_none());
        assert_eq!(fs::read(path.join("ledger.json")).unwrap(), before);
        #[cfg(unix)]
        {
            let alias = tmp.path().join("alias-root");
            std::os::unix::fs::symlink(tmp.path(), &alias).unwrap();
            h.sessions_root = Some(alias);
            h.open_tend_session(&path.canonicalize().unwrap());
            settle(&mut h);
            let View::Tend(tend) = &h.view else { panic!() };
            assert_eq!(tend.selected, 1);
            assert_eq!(
                tend.sessions[tend.selected].path.canonicalize().unwrap(),
                path.canonicalize().unwrap()
            );
            h.sessions_root = Some(tmp.path().to_path_buf());
        }
        h.open_tend_session(&tmp.path().join("missing"));
        settle(&mut h);
        let View::Tend(tend) = &h.view else { panic!() };
        assert!(tend.sessions.is_empty());
        assert!(
            tend.error
                .as_ref()
                .unwrap()
                .contains("Saved session is unavailable")
        );
        h.sessions_root = Some(tmp.path().join("not-a-directory"));
        fs::write(h.sessions_root.as_ref().unwrap(), "not a directory").unwrap();
        h.open_tend_session(&path);
        settle(&mut h);
        let View::Tend(tend) = &h.view else { panic!() };
        assert!(
            tend.error.is_some() && tend.sessions.is_empty(),
            "{:?} {:?} {} {}",
            tend.error,
            tend.want,
            tend.pending,
            tend.sessions.len()
        );
    }

    #[test]
    fn tend_garden_derives_each_state_and_only_shows_goal_lines_newest_first() {
        let tmp = tempfile::tempdir().unwrap();
        for (id, status, date, seed) in [
            ("saved", "active", "2026-10-10T00:00:00Z", None),
            (
                "drafted",
                "active",
                "2026-10-09T00:00:00Z",
                Some("status: draft"),
            ),
            (
                "ready",
                "active",
                "2026-10-08T00:00:00Z",
                Some("status: confirmed for intake"),
            ),
            (
                "ended",
                "stopped",
                "2026-10-07T00:00:00Z",
                Some("status: confirmed for intake"),
            ),
        ] {
            session(tmp.path(), id, status, date, seed);
            let path = tmp.path().join(id).join("ledger.json");
            let mut ledger: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            ledger["goal"] = format!(
                "{id} {}\nWhy: never on list\nDone when: separate in detail",
                "a long goal line ".repeat(20)
            )
            .into();
            fs::write(path, ledger.to_string()).unwrap();
        }
        let loaded = load_sessions(tmp.path()).unwrap();
        assert_eq!(
            loaded.iter().map(|s| s.stage).collect::<Vec<_>>(),
            vec![
                Stage::GoalSaved,
                Stage::SeedDrafted,
                Stage::SeedUnchecked,
                Stage::Stopped
            ]
        );
        for (w, height) in [(80, 24), (100, 30), (160, 40), (320, 40)] {
            let mut h = home(tmp.path());
            h.open(Place::at(1).unwrap());
            settle(&mut h);
            let screen = snapshot(w, height, &mut h, false).unwrap();
            assert!(
                !screen.contains("Why:") && !screen.contains("Done when:") && !screen.contains("/")
            );
            assert!(!screen.contains("ended a long goal"), "{screen}");
            assert!(screen.contains("1 ended not shown"), "{screen}");
            for id in ["saved", "drafted", "ready"] {
                assert!(screen.contains(id), "{screen}");
            }
            for (id, label) in [
                ("saved", "goal saved"),
                ("drafted", "seed drafted"),
                ("ready", "seed needs checking"),
            ] {
                let selected = snapshot(w, height, &mut h, false).unwrap();
                let line = selected.lines().nth(height as usize - 3).unwrap();
                assert!(
                    line.trim_start().starts_with(id)
                        && line.contains(label)
                        && line.contains('…')
                        && line.contains(" · "),
                    "{selected}"
                );
                h.handle_key(key(KeyCode::Char('j')));
            }
            assert_eq!(
                screen.lines().last().unwrap().trim(),
                "↑↓←→ move   enter open   l link   esc back"
            );
        }
    }

    #[test]
    fn tend_detail_preserves_receipt_block_and_copy_is_exact_repeatable_and_manual() {
        let tmp = tempfile::tempdir().unwrap();
        session(tmp.path(), "saved", "active", "2026-10-11T00:00:00Z", None);
        let path = tmp.path().join("saved").canonicalize().unwrap();
        let goal = "I can read Café 👩‍💻 in a calm dark theme.\nWhy: Night work.\nDone when: The block stays readable.\nMust: Keep every line.\nMust not: Run agents.";
        let mut ledger: Value =
            serde_json::from_slice(&fs::read(path.join("ledger.json")).unwrap()).unwrap();
        ledger["goal"] = goal.into();
        fs::write(path.join("ledger.json"), ledger.to_string()).unwrap();
        let before = fs::read(path.join("ledger.json")).unwrap();
        let mut receipt = crate::seed_me::Receipt::test(path.clone());
        receipt.goal = goal.into();
        for (w, height) in [(80, 24), (100, 30), (160, 40), (320, 40)] {
            let mut h = home(tmp.path());
            h.open_tend_session(&path);
            settle(&mut h);
            let screen = snapshot(w, height, &mut h, false).unwrap();
            let normalized = screen.split_whitespace().collect::<Vec<_>>().join(" ");
            for line in receipt.goal.lines() {
                assert!(normalized.contains(line), "{screen}");
            }
            let View::Tend(tend) = &h.view else { panic!() };
            assert_eq!(tend.current().unwrap().goal, receipt.goal);
            for label in ["Why:", "Done when:", "Must:", "Must not:"] {
                assert!(
                    screen
                        .lines()
                        .any(|line| line.trim_start().starts_with(label)),
                    "{screen}"
                );
            }
            assert!(
                !screen.contains("Saved.")
                    && !screen.contains("any harness")
                    && !screen.contains("/")
            );
            assert!(screen.contains("Next: shape it into a seed in Shape."));
            assert_eq!(
                screen.lines().last().unwrap().trim(),
                "s shape   r history   esc back   F10 menu"
            );
            h.handle_key(key(KeyCode::Char('c')));
            assert!(h.take_copy_request().is_none());
            h.handle_key(key(KeyCode::F(1)));
            assert!(
                !snapshot(w, height, &mut h, false)
                    .unwrap()
                    .contains(path.to_str().unwrap())
            );
            h.handle_key(key(KeyCode::Char('h')));
            let View::Tend(tend) = &h.view else { panic!() };
            assert!(tend.help_text().contains(path.to_str().unwrap()));
            h.handle_key(key(KeyCode::Esc));
            h.handle_key(key(KeyCode::Esc));
            h.handle_key(key(KeyCode::Esc));
            let View::Tend(tend) = &h.view else { panic!() };
            assert!(!tend.detail);
            h.handle_key(key(KeyCode::Enter));
            h.handle_key(key(KeyCode::F(10)));
            assert!(matches!(h.view, View::Menu) && !h.quit);
            assert_eq!(fs::read(path.join("ledger.json")).unwrap(), before);
        }
    }

    #[test]
    fn tend_next_action_follows_status_and_ended_sessions_cannot_copy() {
        let tmp = tempfile::tempdir().unwrap();
        for (id, status, seed, next) in [
            (
                "draft",
                "active",
                Some("status: draft"),
                Some("Next: confirm the seed draft in Shape."),
            ),
            (
                "ready",
                "active",
                Some("status: confirmed for intake"),
                Some("Next: check the seed in Shape."),
            ),
            ("stopped", "stopped", None, None),
            (
                "completed",
                "completed",
                Some("status: confirmed for intake"),
                None,
            ),
        ] {
            session(tmp.path(), id, status, "2026-10-11T00:00:00Z", seed);
            let mut h = home(tmp.path());
            h.open_tend_session(&tmp.path().join(id));
            settle(&mut h);
            let screen = snapshot(100, 30, &mut h, false).unwrap();
            if let Some(next) = next {
                assert!(screen.contains(next));
            } else {
                assert!(!screen.contains("Next:") && !screen.contains("c copy prompt"));
            }
            h.handle_key(key(KeyCode::Char('c')));
            assert!(h.take_copy_request().is_none());
        }
        assert_eq!(
            Stage::from_files("active", Some("status: simulated")),
            Stage::Simulated
        );
        assert!(!Stage::Simulated.can_shape());
        assert_eq!(
            Stage::from_files("active", Some("not a status")),
            Stage::Unknown
        );
        assert!(!Stage::Unknown.can_shape());
    }

    #[test]
    fn q_quits_from_home() {
        let tmp = tempfile::tempdir().unwrap();
        let mut h = home(tmp.path());
        h.handle_key(key(KeyCode::Char('q')));
        assert!(h.quit);
    }
}
