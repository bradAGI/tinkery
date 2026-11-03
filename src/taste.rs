//! Taste reviews one exact result against its unchanged saved goal. Explicit refresh reads
//! checkout status and GitHub metadata; o opens a PR and v runs a declared exact-head preview. Copying hands
//! review back externally without recording a verdict, posting, merging, signing gates or agents.
//! Its one write is the operator's label (w, then F2), through tink-sdlc; agent launches can't.

mod evidence;
mod label;
mod links;
mod next;
mod preview;
mod read;
#[cfg(test)]
mod tests;
mod view;

use crate::{
    Palette,
    sdlc::{self, DEADLINE, Goal, Read},
};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Text},
    widgets::{Paragraph, Wrap},
};
use read::ResultRow;
use std::{path::PathBuf, process::Command, time::Instant};

/// One result visit; snapshots, copy requests and any explicit preview die on leaving.
pub(crate) struct Taste {
    root: Option<PathBuf>,
    /// Launched with `--operator agent`: labels are the operator's, so `w` refuses.
    agent: bool,
    label: Option<label::Form>,
    labelling: Option<Read<String>>,
    reading: Option<Read<Vec<ResultRow>>>,
    opening: Option<Read<()>>,
    preview: Option<preview::Preview>,
    rows: Vec<ResultRow>,
    selected: usize,
    selection: Option<(String, Option<String>)>,
    detail: bool,
    evidence: bool,
    scroll: usize,
    page: usize,
    at: Option<Instant>,
    notice: String,
    copy: Option<String>,
    pause: Option<next::Pause>,
    draft: Option<crate::think_record::DraftSuggestion>,
}

impl Taste {
    /// Open without executing helpers or contacting GitHub. r discloses and starts the read.
    pub(crate) fn open(root: Option<PathBuf>, agent: bool) -> Self {
        Self {
            root,
            agent,
            label: None,
            labelling: None,
            reading: None,
            opening: None,
            preview: None,
            rows: Vec::new(),
            selected: 0,
            selection: None,
            detail: false,
            evidence: false,
            scroll: 0,
            page: 1,
            at: None,
            notice: String::new(),
            copy: None,
            pause: None,
            draft: None,
        }
    }

    fn refresh(&mut self) {
        self.preview = None;
        if self.reading.is_some() || self.opening.is_some() {
            return;
        }
        let Some(root) = self.root.clone() else {
            return;
        };
        self.selection = self
            .rows
            .get(self.selected)
            .map(|r| (r.id.clone(), r.url.clone()));
        self.rows.clear();
        self.evidence = false;
        self.pause = None;
        self.draft = None;
        self.copy = None;
        self.notice.clear();
        self.at = None;
        self.scroll = 0;
        self.reading = Some(Read::task(move |stop| read::collect(&root, stop)));
    }

    /// Land only this visit's explicit read or navigation result; no automatic retries.
    pub(crate) fn poll(&mut self) {
        if let Some(result) = self.reading.as_ref().and_then(Read::poll) {
            self.reading = None;
            match result {
                Ok(rows) => {
                    self.selected = self
                        .selection
                        .as_ref()
                        .and_then(|key| {
                            rows.iter()
                                .position(|r| (&r.id, &r.url) == (&key.0, &key.1))
                        })
                        .unwrap_or(0);
                    if self.selection.as_ref().is_some_and(|key| {
                        !rows.iter().any(|r| (&r.id, &r.url) == (&key.0, &key.1))
                    }) {
                        self.detail = false;
                        self.notice =
                            "Selected result is no longer in this reading; choose a result.".into();
                    }
                    self.rows = rows;
                    self.at = Some(Instant::now());
                }
                Err(why) => {
                    self.detail = false;
                    self.evidence = false;
                    self.notice = format!("Couldn't read results: {why}");
                }
            }
        }
        if let Some(result) = self.labelling.as_ref().and_then(Read::poll) {
            self.labelling = None;
            match result {
                Ok(answer) => {
                    self.label = None;
                    self.notice = answer;
                }
                Err(why) => self.notice = format!("Not recorded. {why}"),
            }
        }
        if let Some(result) = self.opening.as_ref().and_then(Read::poll) {
            self.opening = None;
            self.notice = if result.is_ok() {
                "PR sent to browser; no verdict recorded."
            } else {
                "Couldn't open the PR. Check gh/browser setup, then press o to retry."
            }
            .into();
        }
    }

    /// Handle unmodified deliberate actions. Help is isolated by the menu; repeats never open,
    /// reread or copy. true returns Home, dropping and cancelling every owned helper.
    pub(crate) fn key(&mut self, key: KeyEvent) -> bool {
        if self.label.is_some() {
            self.label_key(key);
            return false;
        }
        if key.modifiers != KeyModifiers::NONE || key.kind == KeyEventKind::Release {
            return false;
        }
        if key.kind == KeyEventKind::Repeat
            && matches!(
                key.code,
                KeyCode::Char('r' | 'o' | 'a' | 'c' | 'l' | 't' | 'e' | 'v' | 'w') | KeyCode::Enter
            )
        {
            return false;
        }
        if self.preview.is_some()
            && !matches!(
                key.code,
                KeyCode::Esc
                    | KeyCode::Char('q' | 'r' | 'n' | 'p')
                    | KeyCode::Left
                    | KeyCode::Right
                    | KeyCode::Up
                    | KeyCode::Down
                    | KeyCode::PageUp
                    | KeyCode::PageDown
                    | KeyCode::Home
                    | KeyCode::End
                    | KeyCode::Char('j' | 'k')
            )
        {
            return false;
        }
        match key.code {
            KeyCode::Esc if self.preview.is_some() => {
                self.preview = None;
                self.notice = "Preview stopped and reaped.".into();
            }
            KeyCode::Char('v') if !self.detail || self.evidence => {
                self.detail = true;
                self.evidence = false;
                self.scroll = 0;
            }
            KeyCode::Char('v') if self.preview.is_none() => {
                if let Some(row) = self.rows.get(self.selected) {
                    match (&row.preview, &row.url, &row.head) {
                        (Ok(spec), Some(url), Some(head)) => {
                            self.preview = Some(preview::Preview::start(
                                url.clone(),
                                head.clone(),
                                spec.clone(),
                            ));
                            self.notice.clear();
                            self.scroll = 0;
                        }
                        (Err(_), _, _) => {
                            self.detail = true;
                            self.evidence = false;
                            self.scroll = usize::MAX;
                            self.notice.clear();
                        }
                        _ => self.notice = "Result identity unknown; press r.".into(),
                    }
                }
            }
            KeyCode::Esc if self.evidence => {
                self.evidence = false;
                self.scroll = 0;
            }
            KeyCode::Esc if self.detail => {
                self.detail = false;
                self.pause = None;
                self.draft = None;
                self.scroll = 0;
            }
            KeyCode::Esc | KeyCode::Char('q') => return true,
            KeyCode::Char('r') => self.refresh(),
            KeyCode::Enter if !self.rows.is_empty() => {
                self.detail = true;
                self.evidence = false;
                self.scroll = 0;
            }
            KeyCode::Char('e') if self.detail && self.rows.get(self.selected).is_some() => {
                self.evidence = !self.evidence;
                self.scroll = 0;
            }
            KeyCode::Down | KeyCode::Char('j') if self.detail => {
                self.scroll = self.scroll.saturating_add(1)
            }
            KeyCode::Up | KeyCode::Char('k') if self.detail => {
                self.scroll = self.scroll.saturating_sub(1)
            }
            KeyCode::PageDown if self.detail => self.scroll = self.scroll.saturating_add(self.page),
            KeyCode::PageUp if self.detail => self.scroll = self.scroll.saturating_sub(self.page),
            KeyCode::Home => self.scroll = 0,
            KeyCode::End if self.detail => self.scroll = usize::MAX,
            KeyCode::Down | KeyCode::Char('j' | 'n') | KeyCode::Right => self.select(true),
            KeyCode::Up | KeyCode::Char('k' | 'p') | KeyCode::Left => self.select(false),
            KeyCode::Char('o') if self.reading.is_none() && self.opening.is_none() => {
                if let (Some(root), Some(url)) = (
                    &self.root,
                    self.rows.get(self.selected).and_then(|r| r.url.clone()),
                ) {
                    let root = root.clone();
                    self.opening = Some(Read::task(move |stop| {
                        let mut command = Command::new("gh");
                        command.args(["pr", "view", &url, "--web"]);
                        sdlc::execute(&root, command, stop, DEADLINE).map(|_| ())
                    }));
                } else {
                    self.notice =
                        "No usable recorded PR for this result; its run status says what to record.".into();
                }
            }
            KeyCode::Char('l') if self.reading.is_none() && self.opening.is_none() => {
                if let Some(row) = self.rows.get(self.selected) {
                    match handoff(row, 'a') {
                        Ok(_) => {
                            self.pause = Some(next::suggest(row));
                            self.detail = true;
                            self.evidence = false;
                            self.scroll = 0;
                            self.copy = None;
                            self.notice.clear();
                        }
                        Err(why) => self.notice = why.into(),
                    }
                }
            }
            KeyCode::Char('w')
                if self.detail
                    && self.reading.is_none()
                    && self.rows.get(self.selected).is_some() =>
            {
                if self.agent {
                    self.notice =
                        "Labels are the operator's; an agent launch can't record one.".into();
                } else {
                    self.label = Some(label::Form::default());
                    self.pause = None;
                    self.draft = None;
                    self.evidence = false;
                    self.scroll = 0;
                    self.notice.clear();
                }
            }
            KeyCode::Char('t') => {
                if let Some(pause) = &self.pause {
                    self.draft = Some(pause.draft.clone());
                }
            }
            KeyCode::Char(action @ ('a' | 'c')) => {
                self.pause = None;
                self.draft = None;
                match self
                    .rows
                    .get(self.selected)
                    .ok_or("Select a result first.")
                    .and_then(|row| handoff(row, action))
                {
                    Ok(text) => self.copy = Some(text),
                    Err(why) => self.notice = why.into(),
                }
            }
            _ => {}
        }
        false
    }

    /// The open label form takes every key: words are typed, F2 saves once, Esc discards.
    fn label_key(&mut self, key: KeyEvent) {
        if self.labelling.is_some() {
            return;
        }
        let Some(form) = self.label.as_mut() else {
            return;
        };
        match form.key(key) {
            label::Action::Stay => {}
            label::Action::Discard => {
                self.label = None;
                self.notice = "Label discarded; nothing recorded.".into();
            }
            label::Action::Save => match (form.ready(), &self.root, self.rows.get(self.selected)) {
                (Ok((kind, words)), Some(root), Some(row)) => {
                    let (root, slug) = (root.clone(), row.slug.clone());
                    self.notice = "Saving your label…".into();
                    self.labelling = Some(Read::task(move |stop| {
                        label::write(&root, &slug, kind, &words, stop)
                    }));
                }
                (Err(why), _, _) => self.notice = why.into(),
                _ => self.notice = "Select a result first.".into(),
            },
        }
    }

    /// Whether the label form is taking words, so `?` and digits type instead of acting.
    pub(crate) fn takes_text(&self) -> bool {
        self.label.is_some()
    }

    fn select(&mut self, next: bool) {
        self.preview = None;
        if self.rows.is_empty() {
            return;
        }
        self.selected = if next {
            (self.selected + 1).min(self.rows.len() - 1)
        } else {
            self.selected.saturating_sub(1)
        };
        self.scroll = 0;
        self.notice.clear();
        self.copy = None;
        self.evidence = false;
        self.pause = None;
        self.draft = None;
    }

    /// Take the deliberately requested unsent draft; neither copying nor acceptance opens Think.
    pub(crate) fn take_draft(&mut self) -> Option<crate::think_record::DraftSuggestion> {
        self.draft.take()
    }

    /// Take a clipboard request once; never writes a review record.
    pub(crate) fn take_copy_request(&mut self) -> Option<String> {
        self.copy.take()
    }

    /// Report transport success only, not verified clipboard contents or acceptance.
    pub(crate) fn copy_result(&mut self, sent: bool) {
        self.notice = if sent {
            "Handoff copied; decide externally. No verdict recorded."
        } else {
            "Clipboard unavailable; try again."
        }
        .into();
    }

    /// Source and authority explanation; the menu blocks all actions under help.
    pub(crate) fn help_text(&self) -> String {
        if let Some(pause) = &self.pause {
            return format!(
                "Right now\nq goes back to the menu · ? / Esc close help\n\
                 t edits the unsent draft in Think; only F2 sends.\n\
                 e opens evidence after closing help; check words stay intact.\n\n{}",
                pause.lines.join("\n")
            );
        }
        concat!(
            "Right now\nq goes back to the menu · F10 Home · ? / Esc close help\n",
            "Enter reviews · e opens evidence · n/p result\n",
            "o opens PR · v runs declared preview at the exact commit; Esc stops/reaps\n",
            "a/c copy accept/correct requests, never decisions\n",
            "w labels the run in your words; only F2 saves it\n\n",
            "What Taste reads\nOnly r reads the named checkout's API 1 status and each run's\n",
            "recorded PR with gh, using your login. A run without a recorded PR\n",
            "stays unknown; prose supplies none. Reading writes nothing and launches no agents.\n",
            "r also reads tinkery.preview from the result repo at its exact commit.\n",
            "Passes need current, exact-head evidence; review is separate.\n",
            "External handoffs must recheck head, inputs, receipts and owed gates;\n",
            "copying cannot enforce that binding, record verdicts or merge.\n\n",
            "After external acceptance\nl reports you accepted this result externally (not verified).\n",
            "One deterministic guess follows; t opens an unsent Think draft.\n",
            "Only F2 sends. Copying or green checks never imply acceptance.\n",
            "Esc, correction, selection and rereading discard the guess.\n",
            "No verdict, gate or permission is recorded; you merge externally."
        ).into()
    }

    /// Paint only the in-memory list, headed detail, or deliberately opened evidence.
    pub(crate) fn render(&mut self, frame: &mut Frame, area: Rect, palette: Palette) {
        let footer = Rect::new(
            area.x,
            area.y + area.height.saturating_sub(2),
            area.width,
            2,
        );
        let actions = if self.label.is_some() {
            "1 correction  2 none  3 accepted  4 rejected, then type your words\nF2 save label  Esc discard"
        } else if self.preview.is_some() {
            "Preview · Esc stops and returns · F10 Home\narrows/PgUp/PgDn scroll  ? help"
        } else if self.rows.get(self.selected).is_none() {
            if self.root.is_some() && self.reading.is_none() && self.opening.is_none() {
                "r read results  ? help  Esc back"
            } else {
                "? help  Esc back"
            }
        } else if self.evidence {
            "e review  a/c copy  arrows/PgUp/PgDn scroll\nHome/End  ? help  Esc back"
        } else if self.detail && self.pause.is_some() {
            "t edit in Think (unsent)  e evidence  a/c copy\narrows/PgUp/PgDn scroll  ? help  Esc list"
        } else if self.detail {
            "v run preview  l accepted externally  w label  e evidence  a/c copy  o open PR\narrows/PgUp/PgDn scroll  n/p result  r reread  ? help  Esc list"
        } else {
            "arrows move  enter/v open result  l accepted externally\na/c copy  o open PR  r reread  ? help  Esc back"
        };
        frame.render_widget(Paragraph::new(actions).style(palette.muted), footer);
        let body = Rect::new(area.x, area.y, area.width, area.height.saturating_sub(3));
        let lines = if self.root.is_none() {
            vec![
                Line::raw("Start Tinkery with --sdlc-root PATH to choose its run checkout."),
                Line::raw("Nothing is discovered or run without that choice."),
            ]
        } else if self.reading.is_some() {
            vec![Line::raw(
                "Reading results from the named checkout and GitHub…",
            )]
        } else if self.at.is_none() && self.notice.is_empty() {
            vec![
                Line::raw("Press r to read results from the named checkout and GitHub."),
                Line::raw("Runs python3 _system/scripts/sdlc.py status --json and per-run status."),
                Line::raw("Reads each run's recorded PR with gh using your login."),
            ]
        } else if let (Some(form), Some(row)) = (&self.label, self.rows.get(self.selected)) {
            view::label_form(row, form, &self.notice, body.width, palette)
        } else if let Some(preview) = &self.preview {
            let row = &self.rows[self.selected];
            view::running_preview(row, &preview.status(), body.width, palette)
        } else if self.detail {
            self.rows.get(self.selected).map_or_else(Vec::new, |row| {
                view::detail(
                    row,
                    self.pause.as_ref(),
                    self.at,
                    body.width,
                    palette,
                    self.evidence,
                )
            })
        } else if self.at.is_some() && self.rows.is_empty() {
            vec![Line::raw(
                "No open or merged results in this reading. See Watch for runs; r reads again.",
            )]
        } else {
            view::list(&self.rows, self.selected, body.width, palette)
        };
        let paragraph = Paragraph::new(Text::from(lines.clone())).wrap(Wrap { trim: false });
        self.page = usize::from(crate::page(body.height));
        let max = paragraph
            .line_count(body.width)
            .saturating_sub(body.height as usize);
        if self.detail {
            self.scroll = self.scroll.min(max);
        } else {
            let before = (self.selected * 3).min(lines.len());
            self.scroll = Paragraph::new(Text::from(lines[..before].to_vec()))
                .wrap(Wrap { trim: false })
                .line_count(body.width)
                .saturating_sub(body.height.saturating_sub(3) as usize)
                .min(max);
        }
        let visible = Paragraph::new(Text::from(
            lines
                .into_iter()
                .skip(self.scroll)
                .take(body.height as usize)
                .collect::<Vec<_>>(),
        ));
        frame.render_widget(visible.style(palette.ink).wrap(Wrap { trim: false }), body);
        if !self.notice.is_empty() && self.label.is_none() {
            frame.render_widget(
                Paragraph::new(safe(&self.notice)).style(palette.muted),
                Rect::new(
                    area.x,
                    area.y + area.height.saturating_sub(3),
                    area.width,
                    1,
                ),
            );
        }
    }
}

fn goal(goal: &Goal) -> &str {
    match goal {
        Goal::Matched(text) => text,
        Goal::NoSeed => "No confirmed seed was recorded for this run.",
        Goal::Unknown => "Goal unknown: the original seed changed or can't be found.",
    }
}

fn safe(text: &str) -> String {
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

#[cfg(test)]
fn result_lines(row: &ResultRow) -> Vec<String> {
    view::detail(row, None, None, u16::MAX, Palette::new(true), false)
        .into_iter()
        .chain(view::detail(
            row,
            None,
            None,
            u16::MAX,
            Palette::new(true),
            true,
        ))
        .map(|line| line.to_string())
        .collect()
}

fn handoff(row: &ResultRow, action: char) -> Result<String, &'static str> {
    if !matches!(row.goal, Goal::Matched(_)) || row.id.is_empty() {
        return Err("Goal/run identity unknown; repair sources and press r before copying.");
    }
    let (Some(url), Some(head)) = (&row.url, &row.head) else {
        return Err("Result identity unknown; press r after repairing the source.");
    };
    Ok(format!(
        concat!(
            "{} requested: {} @{}; run {} ({}); seed digest {}. Goal (unchanged):\n{}\n",
            "Evidence and unresolved conditions: {}. Gates owed: {}. Source gaps: {}. ",
            "Recheck current PR head, inputs and receipts; resolve gates and inspect this exact result ",
            "before recording any decision externally. Add the operator's requested changes externally. ",
            "No verdict recorded; copying posts nothing and merges nothing."
        ),
        if action == 'a' {
            "Acceptance review"
        } else {
            "Correction"
        },
        url,
        head,
        row.id,
        row.slug,
        row.seed,
        goal(&row.goal),
        row.items
            .iter()
            .map(|i| format!("{}: {} ({}, {})", i.label, i.state, i.kind, i.evidence))
            .collect::<Vec<_>>()
            .join("; "),
        if row.receipts_missing {
            read::MISSING_RECEIPTS.into()
        } else {
            row.gates.join("; ")
        },
        row.gaps.join("; ")
    ))
}
