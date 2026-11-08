//! Review and receipt: freezing the displayed goal, the typed `confirm`, saving through the ledger,
//! and the receipt that follows a save that read back. Saving records the goal and the
//! person's originals, then the think record when a record folder is set; it never approves a
//! seed or any implementation. A record that fails to write leaves the saved goal standing.

use super::*;
use crate::goal::confirmed::{SavedGoal, goal_block, render_saved_goal};
use crate::seed_me::{self, Config, Receipt};
use crate::think_record as record;
use serde_json::json;
use std::{
    path::{Path, PathBuf},
    sync::mpsc,
};

/// The receipt's notice when the goal saved but its think record didn't; the reason and path
/// stay in How it works.
const RECORD_FAILED: &str = "The think record wasn't written; F1 then h shows why.";
/// How the notice begins when a save didn't finish; the rest is the reason, for How it works.
const HANDOFF_FAILED: &str = "Handoff not complete:";
/// What review shows for a save that didn't finish, short enough for the narrowest pane.
const HANDOFF_FAILED_TEXT: &str = "Handoff not complete. F1 then h shows why; Enter retries.";

/// The goal under review and everything think records with it.
#[derive(Clone)]
pub(super) struct Affirmation {
    pub goal: String,
    pub parts: GoalParts,
    pub other_goals: Vec<String>,
    pub deferred: Vec<String>,
    pub outcome: String,
    pub options: Vec<Approach>,
    pub sources: Vec<Source>,
    pub answered: Vec<Settled>,
    pub unresolved: Vec<Question>,
    pub unresolved_notes: Vec<String>,
    pub decisions: Vec<OpenDecision>,
    pub confirmation: record::GoalConfirmation,
}
impl Affirmation {
    /// What the ledger records: the originals verbatim, and think's questions as provisional context.
    pub(super) fn affirmed(&self) -> seed_me::Affirmed {
        let context = json!({
            "answered": self.answered,
            "unresolved": self.unresolved,
            "unresolved_notes": self.unresolved_notes,
            "decisions": self.decisions,
            "goal_parts": self.parts,
            "success": self.parts.done_when,
            "constraints": {"must": self.parts.must, "must_not": self.parts.must_not},
            "other_goals": self.other_goals,
            "deferred": self.deferred,
            "problem": self.parts.why,
        });
        seed_me::Affirmed {
            goal: self.goal.clone(),
            outcome: self.outcome.clone(),
            options: self
                .options
                .iter()
                .map(|o| seed_me::Choice {
                    label: o.label.clone(),
                    benefit: o.benefit.clone(),
                    cost: o.cost.clone(),
                    undo_cost: o.undo_cost.clone(),
                })
                .collect(),
            originals: self
                .sources
                .iter()
                .map(|s| seed_me::Original {
                    id: s.id,
                    in_reply_to: s.in_reply_to.clone(),
                    text: s.text.clone(),
                })
                .collect(),
            context: context.to_string(),
            source: self.confirmation.source(),
        }
    }
}

pub(super) struct Review {
    pub affirmation: Affirmation,
    pub(super) input: String,
    pub(super) scroll: u16,
    pub(super) max: u16,
}
impl BrainDump {
    /// Supplies caller-declared goal confirmation attribution before review. No identity or
    /// authorization is inferred; invalid or late changes refuse without losing words or review.
    /// Named context is visit-scoped; a terminal launch declaration instead stays frozen across
    /// visits in this session. Neither grants seed confirmation or implementation approval.
    pub fn set_goal_confirmation(
        &mut self,
        confirmation: record::GoalConfirmation,
    ) -> Result<(), String> {
        confirmation.validate()?;
        if matches!(
            self.handoff.confirmation,
            record::GoalConfirmation::Terminal { .. }
        ) {
            return Err("The launch declaration is frozen for this session.".into());
        }
        if self.handoff.started() {
            return Err(
                "Set goal confirmation attribution before review; the displayed context is frozen."
                    .into(),
            );
        }
        self.handoff.confirmation = confirmation;
        Ok(())
    }
    /// Attribution is frozen during review; otherwise it is this visit's supplied context.
    pub(super) fn goal_confirmation(&self) -> &record::GoalConfirmation {
        self.handoff
            .review()
            .map_or(&self.handoff.confirmation, |r| &r.affirmation.confirmation)
    }
    /// Saves confirmed goals under `root` or the checked HOME default,
    /// default. Without this, review explains that saving needs it.
    pub fn with_sessions(mut self, root: Option<PathBuf>) -> Self {
        self.handoff.config = Some(Config::new(root));
        self
    }
    /// Writes a think record into `root` with every confirmed save, and says so on the review
    /// screen. Without this, confirming saves the goal only.
    pub fn with_record_root(mut self, root: PathBuf) -> Self {
        self.handoff.record_root = Some(root);
        self
    }
    /// Records `repo` as where a confirmed goal was made, and shows it on the review screen
    /// before `confirm`. `None` (outside a repository) shows nothing and records none.
    pub fn with_repo(mut self, repo: Option<crate::repo::Repo>) -> Self {
        self.handoff.repo = repo;
        self
    }
    /// The repository this think was started in, if any.
    pub(crate) fn repo(&self) -> Option<&crate::repo::Repo> {
        self.handoff.repo.as_ref()
    }
    /// True when a goal can be saved here: real mode with a sessions folder. Practice never saves, so it
    /// never offers review.
    pub(super) fn can_save(&self) -> bool {
        self.real && self.handoff.config.is_some()
    }
    /// Whether normal Think can open review now. Opening help or observing this starts nothing;
    /// the entry point still checks and explains each blocker when called directly.
    pub(super) fn can_review(&self) -> bool {
        !self.voice_active() && !self.handoff.started() && self.review_candidate().is_ok()
    }

    /// The current board and its displayed goal, or the same refusal review entry uses.
    fn review_candidate(&self) -> Result<(&board::Board, &str), &'static str> {
        if !self.can_save() {
            return Err(layout::SAVE_NEEDS_REAL);
        }
        if self.running() {
            return Err("Still thinking; review once the reading is back.");
        }
        if !self.input.text.is_empty() {
            return Err("Send (F2) or clear your unsent words, then review.");
        }
        if self.shaping.ready.is_some() {
            return Err("A reply is waiting; F2 applies it, then review.");
        }
        if !self.conversation.caught_up() {
            return Err("The reading is behind your words; F2 retries, then review.");
        }
        let board = self
            .conversation
            .board()
            .ok_or("No reading to review yet; F2 sends your words.")?;
        let goal = board
            .goal()
            .ok_or("No goal to review yet; parts remain open.")?;
        Ok((board, goal))
    }
    /// True when confirming also writes a think record.
    pub(super) fn records(&self) -> bool {
        self.handoff.record_root.is_some()
    }
    /// The configured record folder, for reading saved history after leaving this visit.
    pub(crate) fn record_root(&self) -> Option<&Path> {
        self.handoff.record_root.as_deref()
    }
    /// The written record's path and any lost-context reason, or why it wasn't written.
    /// `None` before a save or when no record folder is set.
    pub(super) fn record_line(&self) -> Option<String> {
        self.handoff.record.as_ref().map(|r| match r {
            Ok(record) => record.message(),
            Err(e) => format!("Think record not written: {e}"),
        })
    }
    /// Writes the think record for the session just saved in `receipt`: every turn, every
    /// check (with the model's reply only where it rejected), and the goal exactly as it was
    /// confirmed on the review screen.
    fn write_record(
        &self,
        root: &Path,
        receipt: &Receipt,
    ) -> Result<record::WrittenRecord, String> {
        let affirmation = &self
            .handoff
            .review()
            .ok_or("No confirmed goal to record.")?
            .affirmation;
        let origin = self.host.origin();
        record::write_with_context(
            root,
            &record::Session {
                origin: &origin,
                turns: self.conversation.turns(),
                checks: checks::recorded(&self.diagnostics()),
                goal: &affirmation.goal,
                seed_me_session: &receipt.session,
                repo: self.handoff.repo.as_ref().map(crate::repo::Repo::root),
                confirmed: std::time::SystemTime::now(),
            },
            &affirmation.confirmation,
            Some(self.conversation.reading_context()),
        )
    }
    /// Opens review on the displayed goal, frozen as shown. Refused, with a visible reason, in
    /// practice, without a sessions folder, during a take, or while words are unsent or a reading is
    /// still coming, so what is confirmed is exactly what was read.
    pub fn review_goal(&mut self) {
        if self.voice_active() {
            self.refuse("Stop or cancel voice before reviewing a goal.");
            return;
        }
        if self.handoff.receipt().is_some() || self.handoff.saving() {
            return;
        }
        let (g, goal) = match self.review_candidate() {
            Ok(candidate) => candidate,
            Err(reason) => {
                self.refuse(reason);
                return;
            }
        };
        let goal = goal.to_owned();
        let (unresolved, unresolved_notes) = self.conversation.review_context();
        let affirmation = Affirmation {
            goal,
            parts: g.parts.clone(),
            other_goals: g.other_goals.clone(),
            deferred: g.deferred.clone(),
            outcome: g.parts.done_when.clone().unwrap_or_default(),
            options: g.alternatives.iter().map(board::Candidate::wire).collect(),
            sources: self.conversation.sources().to_vec(),
            answered: self.conversation.settled().to_vec(),
            decisions: g.decisions.clone(),
            unresolved,
            unresolved_notes,
            confirmation: self.handoff.confirmation.clone(),
        };
        self.screen.original = false;
        self.screen.help = false;
        self.screen.details = false;
        self.screen.source_scroll = 0;
        self.screen.source_cursor = 0;
        self.screen.selection = None;
        self.screen.drag_anchor = None;
        self.notice =
            "Nothing saved. Review the goal and remaining questions before affirming.".into();
        self.handoff.open(Review {
            affirmation,
            input: String::new(),
            scroll: 0,
            max: u16::MAX,
        });
    }
    pub(super) fn goal_tick(&mut self) {
        let Some(result) = self.handoff.poll() else {
            return;
        };
        if self.handoff.leaving() {
            self.leave();
        }
        if let Ok(receipt) = &result {
            // The record keeps the goal as confirmed on the review, so it is written first.
            self.handoff.record = self
                .handoff
                .record_root
                .clone()
                .map(|root| self.write_record(&root, receipt));
        }
        self.handoff.finish(&result);
        match result {
            Ok(_) => {
                self.notice = match &self.handoff.record {
                    Some(Err(_)) => RECORD_FAILED.into(),
                    Some(Ok(record)) => record.notice().unwrap_or("Saved.").into(),
                    None => "Saved.".into(),
                };
                self.screen.paper_scroll = 0;
            }
            Err(error) => {
                self.notice = format!(
                    "{HANDOFF_FAILED} {error}{}",
                    self.handoff
                        .config
                        .as_ref()
                        .and_then(Config::recovery_path)
                        .map_or(String::new(), |p| format!(
                            " Recovery session: {}",
                            p.display()
                        ))
                );
            }
        }
    }
    /// Retain a saved goal and recording notice, or an incomplete save's recovery location.
    /// No session is read here: the exit receipt derives its next step when printed.
    /// `None` when nothing was saved or attempted.
    pub fn finish_handoff(&mut self) -> Option<seed_me::ExitReceipt> {
        if let Some(receipt) = self.handoff.receipt() {
            return Some(seed_me::ExitReceipt::Saved {
                receipt: receipt.clone(),
                record: self.record_line(),
            });
        }
        self.handoff
            .config
            .as_ref()
            .and_then(Config::recovery_path)
            .map(|session| seed_me::ExitReceipt::Incomplete { session })
    }
    pub(super) fn goal_key(&mut self, key: KeyEvent) -> bool {
        if self.handoff.saving() {
            self.notice = "Saving your goal; this can't be undone. One moment.".into();
            return true;
        }
        if self.handoff.receipt().is_some() {
            match key.code {
                KeyCode::Char('s') if key.modifiers.is_empty() => {
                    if let Some(receipt) = self.handoff.receipt() {
                        self.exit = Exit::Tend(receipt.session.clone());
                    }
                }
                KeyCode::PageDown | KeyCode::Down => {
                    self.screen.paper_scroll = self.screen.paper_scroll.saturating_add(5)
                }
                KeyCode::PageUp | KeyCode::Up => {
                    self.screen.paper_scroll = self.screen.paper_scroll.saturating_sub(5)
                }
                _ => {}
            }
            return true;
        }
        let Some(review) = self.handoff.save.review_mut() else {
            return false;
        };
        match key.code {
            KeyCode::Esc => self.handoff.close(),
            KeyCode::PageDown | KeyCode::Down => {
                review.scroll = review.scroll.saturating_add(5).min(review.max)
            }
            KeyCode::PageUp | KeyCode::Up => review.scroll = review.scroll.saturating_sub(5),
            KeyCode::Backspace => {
                review.input.pop();
            }
            KeyCode::Char(c)
                if !key.modifiers.contains(KeyModifiers::CONTROL) && review.input.len() < 32 =>
            {
                review.input.push(c)
            }
            KeyCode::Enter if review.input == "confirm" && review.scroll >= review.max => {
                let config = self.handoff.config.clone().unwrap();
                let affirmation = review.affirmation.affirmed();
                let confirmation = review.affirmation.confirmation.clone();
                let (tx, rx) = mpsc::channel();
                std::thread::spawn(move || {
                    let _ = tx.send(config.confirm_as(affirmation, &confirmation));
                });
                self.handoff.begin(rx);
                self.notice = "Saving your goal…".into();
            }
            KeyCode::Enter if review.scroll < review.max => {
                self.refuse("Read to the end first: PgDn scrolls. Then type confirm and Enter.")
            }
            KeyCode::Enter => self.refuse("Type confirm, then Enter. Nothing saved."),
            _ => {}
        }
        true
    }
    pub(super) fn goal_paste(&mut self, text: &str) -> bool {
        if self.handoff.saving() || self.handoff.receipt().is_some() {
            return true;
        }
        if let Some(review) = self.handoff.save.review_mut() {
            if text.chars().all(|c| !c.is_control()) && review.input.len() + text.len() <= 32 {
                review.input.push_str(text);
            }
            return true;
        }
        false
    }
}
pub(super) fn render_declaration(frame: &mut Frame, app: &BrainDump, palette: Palette) {
    let declaration = match app.goal_confirmation() {
        record::GoalConfirmation::Terminal { declaration, .. } => declaration.as_str(),
        record::GoalConfirmation::Agent { .. } => "declared: agent",
        _ => return,
    };
    let area = frame.area();
    frame.render_widget(
        Paragraph::new(format!("{declaration} (not verified)")).style(palette.muted),
        Rect::new(
            11,
            1,
            crate::header_end(area, area.width.saturating_sub(5)).saturating_sub(11),
            1,
        )
        .intersection(area),
    );
}

/// Quiet display never settles a missing beneficiary or hides a question or choice about one.
pub(super) fn quiet_who(
    parts: &GoalParts,
    questions: &[Question],
    decisions: &[OpenDecision],
) -> bool {
    parts.missing().eq([Part::Who])
        && !questions.iter().any(|q| q.target == Some(Part::Who))
        && !decisions.iter().any(|d| d.part == Part::Who)
}

pub(super) fn render_goal(frame: &mut Frame, app: &mut BrainDump, palette: Palette) {
    let inner = app.screen.agent_area;
    if let Some(receipt) = app.handoff.receipt() {
        let record_notice = app
            .handoff
            .record
            .as_ref()
            .and_then(|record| record.as_ref().ok())
            .and_then(record::WrittenRecord::notice);
        let notice = if app.notice == "Copied."
            || app.notice.starts_with("Clipboard")
            || app.notice == RECORD_FAILED
        {
            app.notice.as_str()
        } else {
            record_notice.unwrap_or("")
        };
        render_saved_goal(
            frame,
            &mut app.screen.paper_scroll,
            inner,
            palette,
            SavedGoal {
                goal: &receipt.goal,
                next: Some("Next: shape it into a seed in Shape."),
                notice,
                saved: true,
            },
        );
        return;
    }
    let saving = app.handoff.saving();
    let Some(review) = app.handoff.save.review_mut() else {
        return;
    };
    // One message row group (a refusal, the save in progress, a failed save, else the
    // instruction), the typed word, then Back on a row of its own, so nothing paints over it.
    let instruction = format!(
        "type confirm to save{}. You can't edit it after.",
        if app.handoff.record_root.is_some() {
            " the goal and its record"
        } else {
            ""
        }
    );
    let message = if app.screen.refused.is_some() {
        Some(app.notice.clone())
    } else if saving {
        Some("Saving…".into())
    } else if app.notice.starts_with(HANDOFF_FAILED) {
        Some(HANDOFF_FAILED_TEXT.into())
    } else {
        None
    };
    let quiet = quiet_who(
        &review.affirmation.parts,
        &review.affirmation.unresolved,
        &review.affirmation.decisions,
    );
    let line_count = |text: &str| {
        Paragraph::new(text)
            .wrap(Wrap { trim: false })
            .line_count(inner.width)
    };
    // Neither an empty input nor a shorter refusal may move an already-read end.
    let prompt_rows = (line_count(message.as_deref().unwrap_or(&instruction))
        .max(line_count(&instruction))
        + line_count(&review.input)) as u16;
    let rows = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(u16::from(quiet)),
        Constraint::Length(prompt_rows + 1),
    ])
    .split(inner);
    let heading = match &review.affirmation.confirmation {
        record::GoalConfirmation::Terminal { declaration, .. } => {
            format!("You are confirming ({declaration})")
        }
        _ => "You are confirming".into(),
    };
    let mut body = format!("{heading}\n{}", review.affirmation.goal);
    if !matches!(
        review.affirmation.confirmation,
        record::GoalConfirmation::Unrecorded {} | record::GoalConfirmation::Terminal { .. }
    ) {
        body.push_str(&format!(
            "\n\nCaller-declared confirmation attribution: {}",
            review.affirmation.confirmation.details()
        ));
    }
    // The record keeps where the goal was made, so the person sees it before confirming.
    if let Some(repo) = app
        .handoff
        .repo
        .as_ref()
        .filter(|_| app.handoff.record_root.is_some())
    {
        body.push_str(&format!("\n\nmade in {}", repo.name()));
    }
    if !review.affirmation.other_goals.is_empty() {
        body.push_str(&format!(
            "\n\nalso in your dump: {}",
            review.affirmation.other_goals.join("; ")
        ));
    }
    if !review.affirmation.deferred.is_empty() {
        body.push_str(&format!(
            "\n\nfor the seed: {}",
            review.affirmation.deferred.join("; ")
        ));
    }
    let mut open = review
        .affirmation
        .unresolved
        .iter()
        .map(|q| {
            q.target
                .map_or_else(|| q.text.clone(), |p| format!("{} — {}", p.label(), q.text))
        })
        .collect::<Vec<_>>();
    open.extend(
        review
            .affirmation
            .unresolved_notes
            .iter()
            .skip(usize::from(quiet))
            .cloned(),
    );
    if !open.is_empty() {
        body.push_str(&format!("\n\nStill open\n{}", open.join("\n\n")));
    }
    let p = goal_block(Note::display(&body), palette);
    review.max = p
        .line_count(rows[0].width)
        .saturating_sub(rows[0].height as usize) as u16;
    review.scroll = review.scroll.min(review.max);
    frame.render_widget(p.scroll((review.scroll, 0)), rows[0]);
    // Only the goal's visible rows count; attribution and confirmation instructions do not.
    let goal_start = 1usize;
    let goal_end = goal_start
        + goal_block(Note::display(&review.affirmation.goal), palette).line_count(rows[0].width);
    let start = goal_start.max(usize::from(review.scroll));
    let end = goal_end.min(usize::from(review.scroll) + usize::from(rows[0].height));
    if end > start {
        let content = Rect::new(
            rows[0].x,
            rows[0].y + (start - usize::from(review.scroll)) as u16,
            rows[0].width,
            (end - start) as u16,
        );
        app.screen.reading_visible = layout::visible_content(frame, content);
    }

    // Choose guidance after this frame's bound is known, keeping the normal instruction's
    // reserved height: a shorter cue must not move the end while the operator reads or types.
    if quiet {
        frame.render_widget(
            Paragraph::new("Who: not named").style(palette.muted),
            rows[1],
        );
    }
    let guidance = if review.scroll < review.max {
        "PgDn: read to the end first."
    } else {
        &instruction
    };
    let prompt = format!(
        "{}\n{}",
        message.as_deref().unwrap_or(guidance),
        review.input
    );
    frame.render_widget(
        Paragraph::new(prompt.clone())
            .wrap(Wrap { trim: false })
            .style(palette.jade),
        rows[2],
    );
    if !app.handoff.saving() {
        app.screen.back_button = Rect::new(rows[2].x, rows[2].bottom() - 1, 4, 1);
        frame.render_widget(
            Paragraph::new("Back").style(palette.muted),
            app.screen.back_button,
        );
        let mut note = Note::new(&prompt);
        note.cursor = prompt.len();
        let wrapped = note.wrap(rows[1].width);
        if wrapped.cursor.0 < usize::from(rows[1].height) {
            frame.set_cursor_position((
                rows[1].x + wrapped.cursor.1.min(rows[1].width - 1),
                rows[1].y + wrapped.cursor.0 as u16,
            ));
        }
    }
}
