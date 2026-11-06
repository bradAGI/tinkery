//! Approve reads verified seeds and offers copying or an explicit worker-start review.
//! Opening, navigating and copying never execute work. One retained reader verifies
//! seeds off the paint path, and abandoned visits cannot receive its results.

#[cfg(test)]
mod tests;

use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
};

use crate::{
    Palette,
    repo::Repo,
    sdlc,
    seed_me::{
        Session, Stage,
        ready::{self, ReadySeed},
    },
    think_record::{self, MadeIn},
};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Text},
    widgets::{Paragraph, Wrap},
};

enum OriginNote {
    Different(String),
    Unknown,
    NoRepo,
}

struct Reading {
    seed: ReadySeed,
    origin_note: Option<OriginNote>,
}

#[derive(Default)]
struct Found {
    checkout: Option<PathBuf>,
    target_failure: Option<String>,
    seeds: Vec<Reading>,
    unavailable: usize,
    reasons: Vec<String>,
    start_unknown: Option<String>,
    changed: usize,
}

/// One verifier across visits. Cancellation stops between sessions; an in-flight helper keeps
/// its existing timeout. Reopening cannot spawn a replacement until the old worker returns.
#[derive(Default)]
pub(crate) struct Reader {
    running: Option<(mpsc::Receiver<Found>, Arc<AtomicBool>)>,
}

impl Reader {
    /// Stop publication and further seed verification without blocking terminal input.
    pub(crate) fn cancel(&mut self) {
        if let Some((_, stop)) = &self.running {
            stop.store(true, Ordering::SeqCst);
        }
    }

    /// Land only the current visit's result, or reap an abandoned worker without showing it.
    pub(crate) fn poll(&mut self, view: Option<&mut Approve>) {
        let Some((rx, stop)) = &self.running else {
            return;
        };
        let result = match rx.try_recv() {
            Ok(found) => Ok(found),
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => Err(
                "Reading seeds stopped unexpectedly. Return to the menu and reopen Approve.".into(),
            ),
        };
        let cancelled = stop.load(Ordering::SeqCst);
        self.running = None;
        if !cancelled && let Some(view) = view {
            view.found = Some(
                result.and_then(|mut found| found.target_failure.take().map_or(Ok(found), Err)),
            );
        }
    }

    fn start(
        &mut self,
        sessions: Vec<Session>,
        root: Option<PathBuf>,
        record_root: Result<PathBuf, String>,
        mut found: Found,
        target: Option<ready::SeedTarget>,
    ) -> bool {
        if self.running.is_some() {
            return false;
        }
        let stop = Arc::new(AtomicBool::new(false));
        let cancelled = stop.clone();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let resolved_root = root.as_deref().and_then(|root| root.canonicalize().ok());
            found.checkout = resolved_root.clone();
            let checkout = resolved_root.as_deref();
            let repo = checkout.and_then(Repo::enclosing);
            let origins = checkout.and_then(|_| {
                let selected: Vec<_> = sessions
                    .iter()
                    .map(|session| (session.path.as_path(), session.goal.as_str()))
                    .collect();
                record_root
                    .ok()
                    .and_then(|root| think_record::read_origins(&root, &selected).ok())
            });
            let origins = origins.unwrap_or_else(|| vec![MadeIn::Unknown; sessions.len()]);
            let runs = match checkout {
                Some(path) => sdlc::seeds(path, &cancelled),
                None if root.is_some() => Err("The named checkout could not be resolved.".into()),
                None => Err("Start Tinkery with --sdlc-root PATH to check.".into()),
            };
            if let Err(reason) = &runs {
                found.start_unknown = Some(reason.clone());
            }
            // Discovery owns chronological sorting. Reverse its newest-first order, not timestamps.
            let mut target_failure =
                "This seed is missing or unavailable in the current sessions root.".to_owned();
            for (session, origin) in sessions.into_iter().zip(origins).rev() {
                if cancelled.load(Ordering::SeqCst) {
                    return;
                }
                if target
                    .as_ref()
                    .is_some_and(|target| target.session != session.path)
                {
                    continue;
                }
                if session.stage != Stage::SeedUnchecked && !session.stage.ended() {
                    target_failure =
                        "This seed is no longer verified ready. Return to Shape to read it again."
                            .into();
                    continue;
                }
                if let Ok(runs) = &runs {
                    match sdlc::started(runs, &ready::path(&session.path)) {
                        Ok(true) => {
                            target_failure =
                                "This seed has already started. Open Watch to inspect its run."
                                    .into();
                            continue;
                        }
                        Ok(false) => {}
                        Err(reason) => {
                            found.changed += 1;
                            if found.reasons.len() < crate::seed_me::MAX_READ_FAILURES {
                                found
                                    .reasons
                                    .push(format!("{}: {reason}", session.path.display()));
                            }
                        }
                    }
                }
                match ready::read(&session.path, checkout) {
                    Ok(Some(mut seed)) => {
                        if target.as_ref().is_some_and(|target| seed.target.as_ref() != Some(target)) {
                            target_failure = "This seed changed since Shape read it. Return to Shape to read it again.".into();
                            continue;
                        }
                        let origin_note = root.as_deref().and_then(|root| {
                            if seed.target.as_ref().is_some_and(|target| target.retry.is_some()) {
                                None // The confirmed retry itself binds its checkout; it did not go through Think.
                            } else {
                                origin_note(&mut seed, origin, repo.as_ref(), root)
                            }
                        });
                        found.seeds.push(Reading { seed, origin_note });
                    }
                    Ok(None) => target_failure = "This seed is missing or no longer verified ready. Return to Shape to read it again.".into(),
                    Err(reason) => {
                        target_failure = format!("This seed is unavailable: {reason}");
                        found.unavailable += 1;
                        if found.reasons.len() < crate::seed_me::MAX_READ_FAILURES {
                            found
                                .reasons
                                .push(format!("{}: {reason}", session.path.display()));
                        }
                    }
                }
            }
            if target.is_some() && found.seeds.is_empty() {
                found.target_failure = Some(target_failure);
            }
            let _ = tx.send(found);
        });
        self.running = Some((rx, stop));
        true
    }
}

impl Drop for Reader {
    fn drop(&mut self) {
        self.cancel();
    }
}

/// A visit shows only verified seeds. Navigation and copying use the same immutable reading.
pub(crate) struct Approve {
    target: Option<ready::SeedTarget>,
    root: Option<PathBuf>,
    waiting: bool,
    found: Option<Result<Found, String>>,
    selected: usize,
    scroll: u16,
    page: u16,
    help_scroll: u16,
    copy: Option<String>,
    worker_request: Option<(ReadySeed, PathBuf)>,
    notice: &'static str,
}

impl Approve {
    /// Wait for the menu's fresh session scan. Starts no helper and reads no files.
    pub(crate) fn open(root: Option<PathBuf>) -> Self {
        Self {
            target: None,
            root,
            waiting: true,
            found: None,
            selected: 0,
            scroll: 0,
            page: 1,
            help_scroll: 0,
            copy: None,
            worker_request: None,
            notice: "",
        }
    }

    /// Select only this exact verified seed after the normal worker checks; never substitute.
    pub(crate) fn for_seed(mut self, target: ready::SeedTarget) -> Self {
        self.target = Some(target);
        self
    }

    /// Whether this visit still needs the menu's fresh discovery result.
    pub(crate) fn wants_sessions(&self) -> bool {
        self.waiting
    }

    /// Hand off the discovered sessions without reaching into another phase. Missing discovery
    /// is unavailable, never an empty ready list. At most one verifier runs.
    pub(crate) fn offer(
        &mut self,
        sessions: Result<Vec<Session>, String>,
        unavailable: usize,
        reasons: Vec<String>,
        reader: &mut Reader,
        record_root: Result<PathBuf, String>,
    ) {
        if !self.waiting {
            return;
        }
        self.waiting = false;
        let result = sessions.and_then(|sessions| {
            if reader.start(
                sessions,
                self.root.clone(),
                record_root,
                Found {
                    unavailable,
                    reasons,
                    ..Found::default()
                },
                self.target.clone(),
            ) {
                Ok(())
            } else {
                Err(
                    "The previous seed read is finishing. Return to the menu and reopen Approve."
                        .into(),
                )
            }
        });
        if let Err(reason) = result {
            self.found = Some(Err(reason));
        }
    }

    fn seeds(&self) -> &[Reading] {
        match &self.found {
            Some(Ok(found)) => &found.seeds,
            _ => &[],
        }
    }

    /// Take one explicit copy request. The terminal owns OSC 52, as it does for other places.
    pub(crate) fn take_copy_request(&mut self) -> Option<String> {
        self.copy.take()
    }

    /// Take the deliberate request to review a worker start; this does not execute it.
    pub(crate) fn take_worker_request(&mut self) -> Option<(ReadySeed, PathBuf)> {
        self.worker_request.take()
    }

    /// Show clipboard transport feedback, not a claim of host clipboard verification.
    pub(crate) fn copy_result(&mut self, sent: bool) {
        self.notice = if sent {
            "Copied. Run it in a terminal; after it succeeds, open Watch."
        } else {
            "Couldn't copy; try again."
        };
    }

    /// One local key; true leaves for the menu. Help never leaks copy or selection keys.
    pub(crate) fn key(&mut self, key: KeyEvent, help: &mut bool) -> bool {
        if key.kind == KeyEventKind::Repeat
            && matches!(
                key.code,
                KeyCode::Char('b' | 'c' | '?' | 'n' | 'p') | KeyCode::F(1)
            )
        {
            return false;
        }
        if key.code == KeyCode::F(10) {
            return true;
        }
        if matches!(key.code, KeyCode::F(1) | KeyCode::Char('?')) {
            *help = !*help;
            self.help_scroll = 0;
            return false;
        }
        if *help {
            if key.code == KeyCode::Esc {
                *help = false;
            } else {
                scroll(&mut self.help_scroll, key.code, self.page);
            }
            return false;
        }
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => return true,
            KeyCode::Char('n') | KeyCode::Right if self.selected + 1 < self.seeds().len() => {
                self.selected += 1;
                self.scroll = 0;
                self.notice = "";
            }
            KeyCode::Char('p') | KeyCode::Left if self.selected > 0 => {
                self.selected -= 1;
                self.scroll = 0;
                self.notice = "";
            }
            KeyCode::Char('b') => {
                if let Some(Ok(found)) = &self.found
                    && found.start_unknown.is_none()
                    && let Some(root) = &found.checkout
                    && let Some(reading) = found.seeds.get(self.selected)
                    && reading.seed.commands.is_some()
                    && reading.seed.target.is_some()
                {
                    self.worker_request = Some((reading.seed.clone(), root.clone()));
                }
            }
            KeyCode::Char('c') => {
                self.copy = self
                    .seeds()
                    .get(self.selected)
                    .and_then(|reading| reading.seed.commands.clone());
            }
            code => scroll(&mut self.scroll, code, self.page),
        }
        false
    }

    /// Draw one seed's unchanged goal, acceptance section and manual commands; keep navigation
    /// visible while long words, paths and errors wrap and scroll, and say which way hidden rows
    /// lie. Painting never reads sources.
    pub(crate) fn render(&mut self, frame: &mut Frame, area: Rect, palette: Palette, help: bool) {
        let mut body = Rect::new(area.x, area.y, area.width, area.height.saturating_sub(2));
        self.page = body.height;
        let mut lines = Vec::new();
        let mut keys = "↑↓ scroll   esc back   ? help";
        if help {
            keys = "↑↓ scroll   esc close help";
            lines.push(Line::styled("Right now", palette.jade));
            lines.push(Line::raw(
                "n/p choose seed · ↑↓ or PgUp/PgDn read · b review worker start · c copy command",
            ));
            lines.push(Line::raw(if self.target.is_some() {
                "Esc returns to Shape. F10 returns to the menu even from help."
            } else {
                "Esc returns to the menu. F10 returns to the menu even from help."
            }));
            lines.push(Line::default());
            lines.push(Line::raw("Copying starts nothing and grants no approval. b reviews a worker start; only F2 there authorizes execution. tink-sdlc keeps its definition approval gate."));
            lines.push(Line::raw("Return to the menu and reopen Approve for a fresh read. Seeds are oldest first by the session update time."));
            lines.push(Line::raw(
                "Seeds already recorded by a run move to Watch. Only the original location and unchanged contents establish a match.",
            ));
            if let Some(Ok(found)) = &self.found {
                for reason in &found.reasons {
                    lines.push(Line::raw(display(reason)));
                }
            }
        } else {
            let start_unknown = if self.root.is_none() {
                Some("Start Tinkery with --sdlc-root PATH to check.")
            } else {
                match &self.found {
                    Some(Ok(found)) => found.start_unknown.as_deref(),
                    _ => None,
                }
            };
            if let Some(reason) = start_unknown {
                lines.push(Line::styled(
                    "Can't tell which seeds have started; none are hidden.",
                    palette.muted,
                ));
                lines.push(Line::raw(display(reason)));
            }
            match &self.found {
                None => lines.push(Line::raw(if self.waiting {
                    "Reading sessions…"
                } else {
                    "Verifying seeds…"
                })),
                Some(Err(reason)) => {
                    lines.push(Line::raw("Ready seeds unavailable."));
                    lines.push(Line::raw(display(reason)));
                }
                Some(Ok(found)) => {
                    if found.changed > 0 {
                        lines.push(Line::styled(format!("{} seeds have an unknown start: the original is changed or unavailable. Check in Shape; F1 shows why.", found.changed), palette.muted));
                    }
                    if found.unavailable > 0 {
                        lines.push(Line::styled(format!("{} sessions couldn't be read; this list may be incomplete. F1 shows why.", found.unavailable), palette.muted));
                    }
                    if let Some(reading) = found.seeds.get(self.selected) {
                        let seed = &reading.seed;
                        keys = if seed.commands.is_some() {
                            if found.start_unknown.is_none() && seed.target.is_some() {
                                "b review start   c copy command   n/p seed   ↑↓ read   esc back   ? help"
                            } else {
                                "n/p seed   ↑↓ read   c copy command   esc back   ? help"
                            }
                        } else {
                            "n/p seed   ↑↓ read   esc back   ? help"
                        };
                        lines.push(Line::styled(
                            format!(
                                "Seed {} of {} · {}",
                                self.selected + 1,
                                found.seeds.len(),
                                seed.revision
                            ),
                            palette.jade,
                        ));
                        lines.push(Line::default());
                        for line in seed.goal.split('\n') {
                            lines.push(Line::raw(display(line)));
                        }
                        lines.push(Line::default());
                        let acceptance =
                            crate::seed_me::ledger::readable_acceptance(&seed.acceptance);
                        for line in acceptance.split('\n') {
                            lines.push(Line::raw(display(line)));
                        }
                        lines.push(Line::styled(
                            "Choose how to start · seed confirmation is not implementation permission",
                            palette.muted,
                        ));
                        if let Some(base) = seed
                            .target
                            .as_ref()
                            .and_then(|target| target.retry.as_ref())
                        {
                            lines.push(Line::raw(display(&format!(
                                "Retry from {} in {}",
                                base.head,
                                base.checkout.display()
                            ))));
                        }
                        if let Some(note) = &seed.confirmation_note {
                            lines.push(Line::raw(note.clone()));
                        }
                        if let Some(note) = &reading.origin_note {
                            let (text, style) = match note {
                                OriginNote::Different(reason) => (reason.as_str(), palette.ink),
                                OriginNote::Unknown => {
                                    ("Where this seed was made is unknown.", palette.muted)
                                }
                                OriginNote::NoRepo => {
                                    ("Made outside any repository.", palette.muted)
                                }
                            };
                            lines.push(Line::styled(display(text), style));
                        }
                        if seed.commands.is_some() {
                            lines.push(Line::raw(
                                "b reviews an explicit worker start. Or copy this command for a manual handoff. Copying runs nothing."
                            ));
                        }
                        if let Some(commands) = &seed.commands {
                            for line in commands.lines() {
                                lines.push(Line::raw(line.to_owned()));
                            }
                        } else if let Some(reason) = &seed.retry_problem {
                            lines.push(Line::raw(display(reason)));
                        } else if !matches!(reading.origin_note, Some(OriginNote::Different(_))) {
                            lines.push(Line::raw(concat!(
                                "Cannot copy: name a checkout with tink-sdlc installed using --sdlc-root PATH; ",
                                "checkout, seed, title and executable must be safe for one shell line."
                            )));
                        }
                    } else {
                        lines.push(Line::raw(
                            if found.unavailable == 0
                                && start_unknown.is_none()
                                && found.changed == 0
                            {
                                "No verified seeds are waiting."
                            } else {
                                "No verified seeds could be listed."
                            },
                        ));
                        lines.push(Line::raw(
                            "Confirm a seed in Shape, or open Watch for started runs.",
                        ));
                    }
                }
            }
        }
        let position = if help {
            &mut self.help_scroll
        } else {
            &mut self.scroll
        };
        let paragraph = Paragraph::new(Text::from(lines))
            .wrap(Wrap { trim: false })
            .style(palette.ink);
        let rows = paragraph.line_count(body.width);
        // A clipped view gives its last row to the cue, so nothing hidden passes for the end.
        if rows > body.height as usize && body.height > 1 {
            body.height -= 1;
            self.page = body.height;
        }
        let hidden = rows.saturating_sub(body.height as usize);
        *position = (*position).min(hidden.min(u16::MAX as usize) as u16);
        if let Some(cue) = crate::more_cue(*position as usize, hidden) {
            frame.render_widget(
                Paragraph::new(cue).style(palette.muted),
                Rect::new(body.x, body.bottom(), body.width, 1),
            );
        }
        frame.render_widget(paragraph.scroll((*position, 0)), body);
        frame.render_widget(
            Paragraph::new(self.notice).style(palette.muted),
            Rect::new(
                area.x,
                area.y + area.height.saturating_sub(2),
                area.width,
                1,
            ),
        );
        frame.render_widget(
            Paragraph::new(keys).style(palette.muted),
            Rect::new(
                area.x,
                area.y + area.height.saturating_sub(1),
                area.width,
                1,
            ),
        );
    }
}

fn origin_note(
    seed: &mut ReadySeed,
    origin: MadeIn,
    repo: Option<&Repo>,
    root: &std::path::Path,
) -> Option<OriginNote> {
    match origin {
        MadeIn::Repo(other) if repo.is_none_or(|repo| repo.root() != other) => {
            seed.commands = None;
            let name = |path: &std::path::Path| {
                path.file_name().map_or_else(
                    || path.display().to_string(),
                    |name| name.to_string_lossy().into_owned(),
                )
            };
            let checkout = repo.map_or(root, Repo::root);
            let origin_name = name(&other);
            let checkout_name = name(checkout);
            let reason = if display(&origin_name) == display(&checkout_name) {
                format!(
                    "You named {checkout:?}. Reopen Tinkery with --sdlc-root naming {other:?}, where this seed was made."
                )
            } else {
                format!(
                    "Made in {origin_name}, not the checkout you named ({checkout_name}). \
                     To start it, reopen Tinkery with --sdlc-root naming {origin_name}."
                )
            };
            Some(OriginNote::Different(reason))
        }
        MadeIn::Repo(_) => None,
        MadeIn::Unknown => Some(OriginNote::Unknown),
        MadeIn::NoRepo => Some(OriginNote::NoRepo),
    }
}

fn display(text: &str) -> String {
    text.chars()
        .flat_map(|c| {
            if c.is_control() || matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}') {
                c.escape_default().collect::<Vec<_>>()
            } else {
                vec![c]
            }
        })
        .collect()
}

fn scroll(position: &mut u16, code: KeyCode, page: u16) {
    match code {
        KeyCode::Down | KeyCode::Char('j') => *position = position.saturating_add(1),
        KeyCode::Up | KeyCode::Char('k') => *position = position.saturating_sub(1),
        KeyCode::PageDown => *position = position.saturating_add(crate::page(page)),
        KeyCode::PageUp => *position = position.saturating_sub(crate::page(page)),
        KeyCode::Home => *position = 0,
        KeyCode::End => *position = u16::MAX,
        _ => {}
    }
}
