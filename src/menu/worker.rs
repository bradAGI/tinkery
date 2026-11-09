//! A retained worker-action screen. Reviewing is inert; named keys admit bounded SDLC actions.
//! The menu retains an in-flight request across navigation, and no action retries itself.

use crate::{
    Palette,
    approve::WorkerReview,
    worker::{Action, Config, Gate, Job, Recovery, Relay, Run, Snapshot, Start, State, Turn},
};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Text},
    widgets::{Paragraph, Wrap},
};
use std::path::PathBuf;

/// Navigation only: neither destination starts a worker or preview.
pub(super) enum Destination {
    Home,
    Watch(PathBuf),
    Taste(PathBuf),
}

/// One selected start or pinned run, including its finite request across visits.
pub(super) struct Panel {
    goal: String,
    disclosure: String,
    gate: Gate,
    start: Option<Start>,
    /// The start in flight; it comes back for an explicit retry only if nothing was started.
    starting: Option<Start>,
    run: Option<Run>,
    snapshot: Option<Snapshot>,
    fresh: bool,
    job: Option<Job>,
    notice: String,
    help: bool,
    scroll: u16,
    page: u16,
    read_to_end: bool,
}

impl Panel {
    /// Review the exact seed and separate worker allowance without running anything.
    pub(super) fn review(
        WorkerReview {
            seed,
            root,
            cap_advice,
        }: WorkerReview,
        record_root: Result<PathBuf, String>,
        config: Option<Config>,
        gate: Gate,
    ) -> Self {
        let (start, disclosure) = match (seed.target, config) {
            (Some(target), Some(config)) => {
                let authority = gate.disclosure(Some((&target, &config)));
                let checkout = target
                    .retry
                    .as_ref()
                    .map_or(root.as_path(), |base| base.checkout.as_path());
                let retry = target.retry.as_ref().map(|base| format!("Required starting commit: {}. Use this checkout with --sdlc-root to find the new run.\n", base.head)).unwrap_or_default();
                let disclosure = format!(
                    "Pi model: {}\nResponse cap: {} (one response may already be in flight).\n{cap_advice}\n\
                     Checkout: {}\n\
                     {retry}F2 uses committed source, creates the run, imports this seed, prepares an isolated build checkout, \
                     and launches its bounded worker through tink-sdlc.\n\
                     Your goal, confirmed seed and relevant source files go to that provider. \
                     The worker may edit, run commands and commit inside its build checkout.\n\
                     Commit project changes first. Definition gates still apply; passing checks is not your acceptance.",
                    config.model,
                    config.responses,
                    checkout.display(),
                );
                (
                    Some(Start {
                        target,
                        root,
                        record_root,
                        config,
                        authorization: gate.clone(),
                    }),
                    disclosure + &authority,
                )
            }
            _ => (
                None,
                "No worker allowance. Relaunch with --worker-model provider/model:thinking \
                         and --worker-responses N. Drafting calls do not authorize a worker."
                    .into(),
            ),
        };
        Self {
            goal: seed.goal,
            disclosure,
            gate,
            start,
            starting: None,
            run: None,
            snapshot: None,
            fresh: false,
            job: None,
            notice: String::new(),
            help: false,
            scroll: 0,
            page: 1,
            read_to_end: false,
        }
    }

    /// Open an existing source-selected run with a read only; no new allowance is inferred.
    pub(super) fn existing(run: Run, gate: Gate) -> Self {
        let job = Job::act(gate.clone(), run.clone(), Action::Refresh);
        Self {
            goal: String::new(),
            disclosure: gate.disclosure(None),
            gate,
            start: None,
            starting: None,
            run: Some(run),
            snapshot: None,
            fresh: false,
            job: Some(job),
            notice: "Reading worker…".into(),
            help: false,
            scroll: 0,
            page: 1,
            read_to_end: false,
        }
    }

    /// Whether admission or another finite request must finish before the application exits.
    pub(super) fn busy(&self) -> bool {
        self.job.is_some()
    }

    /// Land one source-owned outcome, never an inferred success or automatic next action.
    pub(super) fn poll(&mut self) {
        let Some(result) = self.job.as_mut().and_then(Job::poll) else {
            return;
        };
        self.job = None;
        self.scroll = 0;
        let starting = self.starting.take();
        match result {
            Ok(snapshot) => {
                self.run = Some(snapshot.run.clone());
                self.snapshot = Some(snapshot);
                self.fresh = true;
                self.notice =
                    "Read from tink-sdlc. r reads again; nothing resumes automatically.".into();
            }
            Err(failure) => {
                self.fresh = false;
                self.notice = format!("Request refused or uncertain: {}", failure.message);
                match failure.recovery {
                    Recovery::Run(run) => self.run = Some(run),
                    Recovery::Retry => {
                        self.start = starting;
                        self.scroll = u16::MAX;
                    }
                    Recovery::Inspect => self.scroll = u16::MAX,
                }
            }
        }
        self.read_to_end = false;
    }

    /// Handle one deliberate key. Help, repeats, modifiers and navigation never execute work.
    pub(super) fn key(&mut self, key: KeyEvent) -> Option<Destination> {
        if key.kind != KeyEventKind::Press || key.modifiers != KeyModifiers::NONE {
            return None;
        }
        if key.code == KeyCode::F(10) {
            return Some(Destination::Home);
        }
        if matches!(key.code, KeyCode::F(1) | KeyCode::Char('?')) {
            self.help = !self.help;
            self.scroll = 0;
            return None;
        }
        if self.help {
            if key.code == KeyCode::Esc {
                self.help = false;
                self.scroll = 0;
            } else {
                self.scroll(key.code);
            }
            return None;
        }
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => return Some(Destination::Home),
            KeyCode::Char('w') => {
                return self
                    .run
                    .as_ref()
                    .map(|r| Destination::Watch(r.root.clone()));
            }
            KeyCode::Char('t') => {
                return self
                    .run
                    .as_ref()
                    .map(|r| Destination::Taste(r.root.clone()));
            }
            KeyCode::F(2) if !self.busy() && self.read_to_end => {
                if let Some(start) = self.start.take() {
                    self.starting = Some(start.clone());
                    self.job = Some(Job::start(start));
                    self.notice =
                        "Starting worker… Leaving this screen does not cancel admission.".into();
                }
            }
            KeyCode::F(2) if !self.read_to_end => {
                self.notice = "Read the complete start disclosure first (End or PgDn).".into();
            }
            code if !self.busy() => {
                let action = match code {
                    KeyCode::Char('r') => Some(Action::Refresh),
                    KeyCode::Char('u') if self.read_to_end && self.can_resume() => self
                        .snapshot
                        .as_ref()
                        .and_then(|s| s.pin.clone())
                        .map(Action::Resume),
                    KeyCode::Char('l') if self.read_to_end && self.can_launch() => self
                        .snapshot
                        .as_ref()
                        .and_then(|s| s.pin.clone())
                        .map(Action::Launch),
                    KeyCode::Char('f') if self.can_finish() => Some(Action::Finish),
                    KeyCode::Char('v') => Some(Action::Verify),
                    KeyCode::Char('s') => Some(Action::StopOperator),
                    KeyCode::Char('c') => Some(Action::StopCap),
                    _ => None,
                };
                if let (Some(action), Some(run)) = (action, &self.run) {
                    if !self.fresh && !matches!(action, Action::Refresh) {
                        return None;
                    }
                    self.fresh = false;
                    self.job = Some(Job::act(self.gate.clone(), run.clone(), action));
                    self.notice = "Requesting the named action from tink-sdlc…".into();
                } else {
                    self.scroll(code);
                }
            }
            code => self.scroll(code),
        }
        None
    }

    fn can_resume(&self) -> bool {
        self.snapshot
            .as_ref()
            .is_some_and(|s| matches!(s.state, State::Active { turn: Turn::Ended }))
    }

    fn can_finish(&self) -> bool {
        self.can_resume()
            && self
                .snapshot
                .as_ref()
                .is_some_and(|s| s.verification == "current")
    }

    fn can_launch(&self) -> bool {
        self.snapshot
            .as_ref()
            .is_some_and(|s| s.state == State::Prepared && s.pin.is_some())
    }

    fn scroll(&mut self, code: KeyCode) {
        match code {
            KeyCode::Down | KeyCode::Char('j') => self.scroll = self.scroll.saturating_add(1),
            KeyCode::Up | KeyCode::Char('k') => self.scroll = self.scroll.saturating_sub(1),
            KeyCode::PageDown => self.scroll = self.scroll.saturating_add(crate::page(self.page)),
            KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(crate::page(self.page)),
            KeyCode::Home => self.scroll = 0,
            KeyCode::End => self.scroll = u16::MAX,
            _ => {}
        }
    }

    /// Draw the source state and consequences; painting makes no source reads or requests.
    pub(super) fn render(&mut self, frame: &mut Frame, area: Rect, palette: Palette) {
        let mut body = Rect::new(area.x, area.y, area.width, area.height.saturating_sub(2));
        let mut lines = Vec::new();
        let mut add = |text: &str| {
            for line in text.lines() {
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
                lines.push(Line::raw(safe));
            }
        };
        let keys;
        if self.help {
            add(
                "Worker actions\n\nF2 starts only the reviewed seed with its separate worker allowance.\n\
                 r reads status. l launches a prepared step; u resumes an ended turn on its existing model/session and cumulative cap.\n\
                 v verifies first. Only current passing checks allow f to record the normally ended worker.\n\
                 After a refused request, r must read successfully before any other action.\n\
                 Failed checks keep the ended worker resumable after that read. s stops it; c records that its cap stopped it.\n\
                 w opens Watch; t opens Taste, where r reads and v runs a declared preview.\n\
                 Esc/F10 return Home. b there reopens this worker.\n\
                 Admission finishes before Tinkery exits. The detached worker stays owned by SDLC.\n\
                 No refresh or navigation automatically retries, resumes, verifies or opens a preview.",
            );
            keys = "↑↓ read   esc close help";
        } else if let Some(snapshot) = &self.snapshot {
            if !self.fresh {
                add(
                    "Current worker and verification state unknown. Last successful read follows:\n",
                );
            }
            add(&format!(
                "Worker: {}\n\nRun: {}\nCheckout: {}",
                state(&snapshot.state),
                snapshot.run.slug,
                snapshot.run.root.display()
            ));
            if !snapshot.model.is_empty() {
                add(&format!(
                    "Pi model: {}\nResponse cap: {} (cumulative; one response may be in flight).",
                    snapshot.model, snapshot.responses
                ));
            }
            add(&format!(
                "Verification: {}\n\n{}",
                snapshot.verification, snapshot.next
            ));
            if self.fresh && self.can_launch() {
                add(
                    "\nl sends this prepared task and source context to the displayed provider within its existing cap. It grants no extra responses.",
                );
            }
            if self.fresh && self.can_resume() {
                add(
                    "\nu sends the existing task and source context to the same provider again within the remaining cap.\n\
                     v verifies before f records the ended turn; failed checks leave u available. Neither action accepts the result.",
                );
            }
            keys = if !self.fresh {
                "r read   w watch   t taste   ? help   esc back"
            } else if self.can_launch() {
                "l launch prepared   r read   w watch   t taste   ? help   esc back"
            } else if self.can_finish() {
                "f finish   u resume   r read   v verify   t taste   ? help   esc back"
            } else if self.can_resume() {
                "v verify   u resume   r read   t taste   ? help   esc back"
            } else {
                "r read   v verify   s stop   c cap stop   t taste   ? help   esc back"
            };
        } else {
            add(if self.run.is_some() {
                "Worker result unavailable; current state unknown. r reads again"
            } else {
                "Review worker start"
            });
            add(&format!("\n{}\n\n{}", self.goal, self.disclosure));
            if let Some(run) = &self.run {
                add(&format!(
                    "Run: {}\nCheckout: {}",
                    run.slug,
                    run.root.display()
                ));
            }
            keys = if self.run.is_some() {
                "r read   w watch   t taste   ? help   esc back"
            } else if self.start.is_some() {
                "F2 start worker   ↑↓ read   ? help   esc back"
            } else {
                "↑↓ read   ? help   esc back"
            };
        }
        if !self.notice.is_empty() {
            add(&format!("\n{}", self.notice));
        }
        if self.snapshot.is_some() {
            add(&self.gate.disclosure(None));
        }
        let text = Paragraph::new(Text::from(lines))
            .style(palette.ink)
            .wrap(Wrap { trim: false });
        let rows = text.line_count(body.width);
        if rows > body.height as usize && body.height > 1 {
            body.height -= 1;
        }
        self.page = body.height;
        let hidden = rows
            .saturating_sub(body.height as usize)
            .min(u16::MAX as usize) as u16;
        self.scroll = self.scroll.min(hidden);
        if !self.help && self.scroll == hidden {
            self.read_to_end = true;
        }
        if let Some(cue) = crate::more_cue(self.scroll as usize, hidden as usize) {
            frame.render_widget(
                Paragraph::new(cue).style(palette.muted),
                Rect::new(body.x, body.bottom(), body.width, 1),
            );
        }
        frame.render_widget(text.scroll((self.scroll, 0)), body);
        frame.render_widget(
            Paragraph::new(if self.busy() {
                "Request in progress; navigation keeps it running."
            } else {
                keys
            })
            .style(palette.muted),
            Rect::new(area.x, area.bottom().saturating_sub(1), area.width, 1),
        );
    }
}

impl super::Home {
    /// Share the declaration and relay snapshot without granting permission on navigation.
    pub(super) fn worker_gate(&self) -> Gate {
        Gate::new(self.agent, self.worker_authorization.clone())
    }

    /// Set a separate, explicit worker allowance for this launch; configuration runs nothing.
    pub fn with_worker(mut self, model: Option<String>, responses: usize) -> Result<Self, String> {
        if model.is_some() || responses > 0 {
            let config = Config {
                model: model.unwrap_or_default(),
                responses,
            };
            config.validate()?;
            self.worker_config = Some(config);
        }
        Ok(self)
    }

    /// Read caller-supplied relayed words for later review; never creates authorization or starts work.
    pub fn with_worker_authorization(mut self, path: Option<PathBuf>) -> Self {
        self.worker_authorization = Relay::read(path.as_deref());
        self
    }
}

fn state(state: &State) -> &'static str {
    match state {
        State::NotStarted => "not started",
        State::Prepared => "prepared, not launched",
        State::Active { turn: Turn::Ended } => "turn ended; completion still unrecorded",
        State::Active {
            turn: Turn::Running,
        } => "running",
        State::Active {
            turn: Turn::Unknown,
        } => "turn state unknown; inspect before resuming",
        State::Uncertain => "launch uncertain; inspect before starting another",
        State::Completed => "turn completed; verification is separate",
        State::Stopped { cap: true } => "stopped by response cap",
        State::Stopped { cap: false } => "stopped by operator",
    }
}
