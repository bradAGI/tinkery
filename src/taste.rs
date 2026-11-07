//! Taste shows one exact result beside its unchanged saved goal and what needs judgment. Explicit
//! refresh reads checkout status and GitHub metadata; o opens a PR and v runs a declared exact-head
//! preview. Its one write is the operator's label, their verdict on the result (w, then F2),
//! through tink-sdlc; agent launches can't. Copying hands review back externally. Nothing here
//! posts, merges, signs gates or launches agents.

mod evidence;
mod label;
mod links;
mod next;
mod preview;
mod read;
mod retry;
pub(crate) use retry::{Rejected, Reviewed};
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

const HUMAN_VERDICT: &str = "Only a human can record the verdict.";

/// One result visit; snapshots, copy requests and any explicit preview die on leaving.
pub(crate) struct Taste {
    root: Option<PathBuf>,
    /// Launched with `--operator agent`: labels are the operator's, so `w` refuses.
    agent: bool,
    label: Option<label::Form>,
    /// The name the open form will sign with, as read when it opened or on a retry.
    signer: Option<Result<String, String>>,
    signing: Option<Read<String>>,
    labelling: Option<Read<String>>,
    reading: Option<Read<(Vec<ResultRow>, usize)>>,
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
    retry: Option<Rejected>,
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
            signer: None,
            signing: None,
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
            retry: None,
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
            .map(|r| (r.id.clone(), r.url.clone().ok()));
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
                Ok((rows, skipped)) => {
                    let selected = self.selection.as_ref().and_then(|(id, url)| {
                        rows.iter()
                            .position(|r| &r.id == id && r.url.as_ref().ok() == url.as_ref())
                    });
                    self.selected = selected.unwrap_or(0);
                    if self.selection.is_some() && selected.is_none() {
                        self.detail = false;
                        self.notice =
                            "Selected result is no longer in this reading; choose a result.".into();
                    }
                    self.rows = rows;
                    self.at = Some(Instant::now());
                    if skipped > 0 && self.notice.is_empty() {
                        self.notice = format!(
                            "Skipped {skipped} folder{} in runs/ whose name isn't a run name; nothing in {} was read.",
                            if skipped == 1 { "" } else { "s" },
                            if skipped == 1 { "it" } else { "them" }
                        );
                    }
                }
                Err(why) => {
                    self.detail = false;
                    self.evidence = false;
                    self.notice = format!("Couldn't read results: {why}");
                }
            }
        }
        if let Some(result) = self.signing.as_ref().and_then(Read::poll) {
            self.signing = None;
            self.signer = Some(result);
        }
        if let Some(result) = self.labelling.as_ref().and_then(Read::poll) {
            self.labelling = None;
            match result {
                Ok(_) => {
                    let verdict = self.label.take().and_then(|form| form.ready().ok());
                    self.notice = "Saved your label.".into();
                    if let Some(row) = self.rows.get_mut(self.selected) {
                        row.labelled = Some(true);
                        row.rejection = verdict
                            .filter(|(kind, _)| {
                                *kind == label::Kind::Fail
                                    && row.local().is_some()
                                    && matches!(row.goal, Goal::Matched(_))
                            })
                            .map(|(_, words)| retry::Verdict {
                                words,
                                pinned: true,
                            });
                    }
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
                KeyCode::Char('r' | 'o' | 'a' | 'c' | 'l' | 't' | 'e' | 'v' | 'w' | 's')
                    | KeyCode::Enter
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
                self.notice = "Preview stopped.".into();
            }
            KeyCode::Char('v') if !self.detail || self.evidence => {
                self.detail = true;
                self.evidence = false;
                self.scroll = 0;
            }
            KeyCode::Char('v') if self.preview.is_none() => {
                if let Some(row) = self.rows.get(self.selected) {
                    match (row.preview(), row.preview_source()) {
                        (Ok(Some(spec)), Some(source)) => {
                            self.preview = Some(preview::Preview::start(source, spec.clone()));
                            self.notice.clear();
                            self.scroll = 0;
                        }
                        (Ok(None), _) => {
                            self.notice =
                                format!("{} ? says how to add one.", preview::NO_DECLARATION)
                        }
                        (Err(why), _) => self.notice = why.into(),
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
                if let (Some(root), Some(row)) = (&self.root, self.rows.get(self.selected)) {
                    match &row.url {
                        Ok(url) => {
                            let (root, url) = (root.clone(), url.clone());
                            self.opening = Some(Read::task(move |stop| {
                                let mut command = Command::new("gh");
                                command.args(["pr", "view", &url, "--web"]);
                                sdlc::execute(&root, command, "gh", stop, DEADLINE).map(|_| ())
                            }));
                        }
                        Err(_) if row.local().is_some() => {
                            self.notice =
                                "This local result has no PR. Use v to try its declared preview."
                                    .into();
                        }
                        Err(gap) => self.notice = gap.next.into(),
                    }
                } else {
                    self.notice = "Select a result first.".into();
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
            KeyCode::Char('s') if self.detail && !self.evidence => {
                if let Some(row) = self.rows.get(self.selected)
                    && let (Some(local), Some(verdict)) = (row.local(), &row.rejection)
                {
                    self.retry = Some(Rejected {
                        local: local.clone(),
                        verdict: verdict.clone(),
                    });
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
                } else if let Some(root) = self.label_root() {
                    self.label = Some(label::Form::default());
                    self.signer = None;
                    self.signing = Some(Read::task(move |stop| label::signer(&root, stop)));
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

    fn label_root(&self) -> Option<PathBuf> {
        self.rows
            .get(self.selected)
            .and_then(ResultRow::local)
            .map(|local| local.checkout().to_path_buf())
            .or_else(|| self.root.clone())
    }

    /// The reviewed local result for one explicit transition into Shape.
    pub(crate) fn take_retry(&mut self) -> Option<Rejected> {
        self.retry.take()
    }

    /// The open label form takes every key: words are typed, F2 saves once, Esc discards.
    fn label_key(&mut self, key: KeyEvent) {
        if self.labelling.is_some() {
            return;
        }
        match key.code {
            KeyCode::Up => return self.scroll = self.scroll.saturating_sub(1),
            KeyCode::Down => return self.scroll = self.scroll.saturating_add(1),
            KeyCode::PageUp => return self.scroll = self.scroll.saturating_sub(self.page),
            KeyCode::PageDown => return self.scroll = self.scroll.saturating_add(self.page),
            _ => {}
        }
        let root = self.label_root();
        let Some(form) = self.label.as_mut() else {
            return;
        };
        match form.key(key) {
            label::Action::Stay => {}
            label::Action::Discard => {
                self.label = None;
                self.signer = None;
                self.signing = None;
                self.notice = "Label discarded; nothing recorded.".into();
            }
            label::Action::Save => {
                let (Some(root), Some(row)) = (root, self.rows.get(self.selected)) else {
                    return self.notice = "Select a result first.".into();
                };
                match (form.ready(), &self.signer) {
                    (Err(why), _) => self.notice = why.into(),
                    // Only the name the form showed signs; a missing one is read again, never guessed.
                    (Ok(_), None) => {
                        self.notice = "Still checking who signs; press F2 again.".into()
                    }
                    (Ok(_), Some(Err(_))) => {
                        self.signer = None;
                        self.signing = Some(Read::task(move |stop| label::signer(&root, stop)));
                        self.notice = "Checking your name again; press F2 once it shows.".into();
                    }
                    (Ok((kind, words)), Some(Ok(name))) => {
                        let (row, name) = (row.clone(), name.clone());
                        self.notice = "Saving your label…".into();
                        self.labelling = Some(Read::task(move |stop| {
                            label::write(&root, &row, kind, &words, &name, stop)
                        }));
                    }
                }
            }
        }
    }

    /// Whether the label form is taking words, so `?` and digits type instead of acting.
    pub(crate) fn takes_text(&self) -> bool {
        self.label.is_some()
    }

    /// A label is being saved; leaving waits for tink-sdlc's answer.
    pub(crate) fn saving(&self) -> bool {
        self.labelling.is_some()
    }

    /// Pasted text joins the label's words unchanged once a kind is chosen.
    pub(crate) fn paste(&mut self, text: &str) {
        if self.labelling.is_none()
            && let Some(form) = self.label.as_mut()
        {
            form.paste(text);
        }
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
        let verdict = if self.agent {
            HUMAN_VERDICT
        } else {
            "w opens the form. Only F2 saves pass or fail with your words and git name."
        };
        if let Some(pause) = &self.pause {
            return format!(
                "Right now\nq goes back to the menu · ? / Esc close help\n\
                 {verdict}\n\
                 t edits the unsent draft in Think; only F2 sends.\n\
                 e opens details after closing help; check words stay intact.\n\n{}",
                pause.lines.join("\n")
            );
        }
        let selected = self.rows.get(self.selected);
        let open_result = if !self.detail && selected.is_some() {
            "Enter opens a result\n"
        } else {
            ""
        };
        let open_pr = if selected.is_some_and(|row| row.url.is_ok()) {
            "o opens the PR · "
        } else {
            ""
        };
        let handoffs = if selected.is_some_and(|row| row.url.is_ok()) {
            "a/c copy accept/correct requests · l report a person's prior acceptance\n"
        } else {
            ""
        };
        let after = if selected.is_some_and(|row| row.url.is_ok()) {
            concat!(
                "After external acceptance\nUse l only if a person already accepted this result elsewhere.\n",
                "This unverified report stays in this view and saves no verdict.\n",
                "t drafts Next. Copying and green checks never imply acceptance or a merge."
            )
        } else {
            ""
        };
        let preview = selected.map_or("", |row| match row.preview() {
            Ok(Some(_)) => "v runs the project's preview at its exact commit; Esc stops it",
            Ok(None) => preview::NO_DECLARATION,
            Err(why) => why,
        });
        let declaration = if selected.is_some_and(|row| row.head.is_some()) {
            concat!(
                "Preview format: commit tinkery.preview at the project root, one line:\n",
                r#"{"command":"npm run dev -- --host 127.0.0.1","url":"http://127.0.0.1:5173/"}"#,
                "\n"
            )
        } else {
            ""
        };
        format!(
            concat!(
                "Right now\nq goes back to the menu · F10 Home · ? / Esc close help\n",
                "{}{}e details: checks, goal, IDs, evidence · n/p result\n",
                "{}\n{}\n",
                "Verdict\n{}\n\n",
                "What Taste reads\nr reads run status and committed preview declarations.\n",
                "Recorded PRs use gh and your login; checked local results use Git.\n",
                "v rechecks local source identity before running the disclosed preview.\n",
                "Reading starts no preview, agent or model request.\n{}\n{}"
            ),
            open_result, open_pr, preview, handoffs, verdict, declaration, after
        )
    }

    /// Paint only the in-memory list, headed detail, or deliberately opened evidence.
    pub(crate) fn render(&mut self, frame: &mut Frame, area: Rect, palette: Palette) {
        let footer = Rect::new(
            area.x,
            area.y + area.height.saturating_sub(2),
            area.width,
            2,
        );
        let selected = self.rows.get(self.selected);
        let review_actions;
        let actions = if self.labelling.is_some() {
            "Saving your label with tink-sdlc…\n? help"
        } else if self.label.is_some() {
            "1 pass  2 fail, then type your words\nF2 save  Esc cancel"
        } else if self.preview.is_some() {
            "Preview · Esc stops and returns · F10 Home\narrows/PgUp/PgDn scroll  ? help"
        } else if selected.is_none() {
            if self.root.is_some() && self.reading.is_none() && self.opening.is_none() {
                "r read results  ? help  Esc back"
            } else {
                "? help  Esc back"
            }
        } else if self.evidence {
            "e back to the result  arrows/PgUp/PgDn scroll\n? more keys  Esc back"
        } else if self.detail {
            let next = if self.pause.is_some() {
                "t edit in Think (unsent)  "
            } else if selected.is_some_and(|r| r.rejection.is_some()) {
                "s Shape the next attempt  "
            } else {
                ""
            };
            let verdict = if self.agent {
                "Human verdict only"
            } else {
                "w write verdict"
            };
            let open_pr = if selected.is_some_and(|row| row.url.is_ok()) {
                "  o open PR"
            } else {
                ""
            };
            review_actions = format!("{next}{verdict}{open_pr}  Esc list\ne details  ? more keys");
            &review_actions
        } else {
            "arrows move  Enter open result  r reread\n? more keys  Esc back"
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
            view::label_form(
                row,
                form,
                self.signer.as_ref(),
                &self.notice,
                body.width,
                palette,
            )
        } else if let Some(preview) = &self.preview {
            let row = &self.rows[self.selected];
            view::running_preview(row, &preview.status(), body.width, palette)
        } else if self.detail {
            self.rows.get(self.selected).map_or_else(Vec::new, |row| {
                if self.evidence {
                    return view::details(row, self.at, body.width, palette);
                }
                view::review(
                    row,
                    self.pause.as_ref(),
                    self.at,
                    body.width,
                    palette,
                    self.agent,
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

/// Show untrusted source words on one line, escaping terminal controls.
pub(crate) fn safe(text: &str) -> String {
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
    view::review(row, None, None, u16::MAX, Palette::new(true), false)
        .into_iter()
        .chain(view::details(row, None, u16::MAX, Palette::new(true)))
        .map(|line| line.to_string())
        .collect()
}

fn handoff(row: &ResultRow, action: char) -> Result<String, &'static str> {
    if row.local().is_some() {
        return Err("This local result has no PR. Use v to try its declared preview.");
    }
    if !matches!(row.goal, Goal::Matched(_)) || row.id.is_empty() {
        return Err("Goal/run identity unknown; repair sources and press r before copying.");
    }
    let (Ok(url), Some(head)) = (&row.url, &row.head) else {
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
