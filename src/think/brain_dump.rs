//! Think's screen: the writing box, the original, the reading and its one question, review and
//! the receipt, wired to the conversation, the model and ledger.
//!
//! The rules live in the parts: [`Conversation`] decides what is asked and sent, `reading` and
//! `review` own the model's contracts, [`board`] verifies a reply, and `seed_me` saves. This
//! file routes input to them and paints the result. Nothing is sent without F2, and nothing is
//! saved without a typed `confirm`.

use super::{
    Motion, Note,
    canvas::Canvas,
    drafting::{Approach, Job, ModelHost, REQUEST_LIMIT, Request},
};
use crate::{MIN_HEIGHT, MIN_WIDTH, Palette, seed_me};
use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use pinstar::data::{CanvasNode, TextNode};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    sync::{Arc, atomic::AtomicBool},
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;
pub mod board;
mod now;
mod suggestion;
use crate::goal::{GoalParts, Part};
pub use suggestion::Suggestion;

pub use host::BoardHost;
use host::Simulated;
pub use reading::{
    Anchor, BoardRequest, Fragment, Framing, Guess, OpenDecision, Question, Settled, Source,
};
use reading::{grapheme_boundary, same_question};
use review::Review;
pub use review::{Review as DecisionReview, input as decision_review_input};

/// Think: one place where the person writes, reads the reading, answers one question at a
/// time, and confirms a goal. It routes keys, mouse and ticks to its parts: the
/// [`Conversation`] (what was said and read), the [`Screen`] (what is shown where), the
/// [`Shaping`] call and the [`Handoff`] that saves a goal. Whoever opened think reads
/// [`BrainDump::exit`] after each tick.
pub struct BrainDump {
    /// The writing box: words not yet sent.
    input: Note,
    /// The writing box holds a new dump, not an answer to the focused question.
    add_more: bool,
    conversation: Conversation,
    screen: Screen,
    shaping: Shaping,
    handoff: Handoff,
    host: Arc<dyn BoardHost>,
    voice: Option<crate::voice::Voice>,
    /// Real provider mode, not practice.
    real: bool,
    notice: String,
    /// Local checks on each send, shown in "Why" (F1).
    local_checks: Vec<checks::Decision>,
    /// Text waiting to be copied by the terminal host.
    copy: Option<String>,
    exit: Exit,
}

/// What think asks of whoever opened it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Exit {
    /// Keep thinking.
    Stay,
    /// Back to the menu, or out when think was opened on its own.
    Menu,
    /// Open this saved session in tend.
    Tend(std::path::PathBuf),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Refusal {
    Other,
    DraftTooLong,
    EarlierWordsTooLong,
}

/// What is shown where: rectangles from the last paint, scrolls, focus, overlays and
/// selection. Nothing here is part of the conversation.
struct Screen {
    key_bar: bool,
    /// This frame contains reading content; overlays can suppress its acknowledgment.
    reading_visible: bool,
    /// Help (F1), the originals and why cover the right side.
    help: bool,
    how: bool,
    actions: crate::actions::Page,
    original: bool,
    details: bool,
    leave_prompt: bool,
    /// F1's Model and runs row asked for the Model panel, which the launch owns.
    model: bool,
    /// The notice says why the last action did nothing; drawn until the next input.
    refused: Option<Refusal>,
    /// Focus is on the original (or its cards), not the writing box.
    board_focus: bool,
    show_extractions: bool,
    original_scroll: u16,
    input_scroll: usize,
    paper_scroll: u16,
    source_view: usize,
    source_scroll: usize,
    source_cursor: usize,
    selection: Option<(usize, usize)>,
    drag_anchor: Option<usize>,
    /// A left-button drag that began in the writing box. The anchor lives in `Note`, so
    /// any edit ends the drag without a stale offset surviving it.
    input_drag: bool,
    /// The extracted cards and their positions.
    canvas: Canvas,
    area: Rect,
    input_area: Rect,
    agent_area: Rect,
    source_area: Rect,
    goal_button: Rect,
    back_button: Rect,
}
impl Default for Screen {
    fn default() -> Self {
        Self {
            key_bar: true,
            reading_visible: false,
            help: false,
            how: false,
            actions: crate::actions::Page::default(),
            original: false,
            details: false,
            leave_prompt: false,
            model: false,
            refused: None,
            board_focus: false,
            show_extractions: false,
            original_scroll: 0,
            input_scroll: 0,
            paper_scroll: 0,
            source_view: 0,
            source_scroll: 0,
            source_cursor: 0,
            selection: None,
            drag_anchor: None,
            input_drag: false,
            canvas: Canvas::new(),
            area: Rect::default(),
            input_area: Rect::default(),
            agent_area: Rect::default(),
            source_area: Rect::default(),
            goal_button: Rect::default(),
            back_button: Rect::default(),
        }
    }
}

struct SendPreview {
    staged: conversation::Staged,
    request: BoardRequest,
    bytes: usize,
    context_over: bool,
    waiting_failure: Option<String>,
}

/// The shaping call in flight, or its reply waiting while the person types.
#[derive(Default)]
struct Shaping {
    job: Option<Job<Guess>>,
    request: Option<BoardRequest>,
    ready: Option<(BoardRequest, Result<Guess, String>)>,
    /// Encoded size, including unknown, reused until an input or reply event.
    encoded_bytes: Option<Option<usize>>,
}

/// Saving a confirmed goal through the ledger: the review, the save in flight, and its receipt.
#[derive(Default)]
struct Handoff {
    config: Option<seed_me::Config>,
    /// Visit-scoped named attribution, or the terminal declaration frozen for this session.
    confirmation: crate::think_record::GoalConfirmation,
    save: Save,
    /// Where a confirmed session's think record goes; `None` writes none.
    record_root: Option<std::path::PathBuf>,
    /// The think record written with the save, or why it wasn't.
    record: Option<Result<crate::think_record::WrittenRecord, String>>,
    /// The repository Tinkery was started in; the review shows it and the record keeps it.
    repo: Option<crate::repo::Repo>,
}

type SaveJob = std::sync::mpsc::Receiver<Result<seed_me::Receipt, String>>;

/// Where saving the goal stands. The review stays on screen while its save runs; a failed
/// save returns to it, and only a receipt ends it.
#[derive(Default)]
enum Save {
    #[default]
    Idle,
    Reviewing(goal_view::Review),
    Saving {
        review: goal_view::Review,
        job: SaveJob,
        /// Esc during the save: leave once it reads back.
        leave: bool,
    },
    Saved(seed_me::Receipt),
}

impl Save {
    /// The review on screen, editable while it is open or saving.
    fn review_mut(&mut self) -> Option<&mut goal_view::Review> {
        match self {
            Save::Reviewing(review) | Save::Saving { review, .. } => Some(review),
            _ => None,
        }
    }
}

impl Handoff {
    /// The review on screen, including while its save runs.
    fn review(&self) -> Option<&goal_view::Review> {
        match &self.save {
            Save::Reviewing(review) | Save::Saving { review, .. } => Some(review),
            _ => None,
        }
    }
    /// A save is in flight.
    fn saving(&self) -> bool {
        matches!(self.save, Save::Saving { .. })
    }
    /// Esc during the save asked to leave once it reads back.
    fn leaving(&self) -> bool {
        matches!(self.save, Save::Saving { leave: true, .. })
    }
    /// The saved goal's receipt, once the save read back.
    fn receipt(&self) -> Option<&seed_me::Receipt> {
        match &self.save {
            Save::Saved(receipt) => Some(receipt),
            _ => None,
        }
    }
    /// Anything of the save is on screen: a review, a save or a receipt.
    fn started(&self) -> bool {
        !matches!(self.save, Save::Idle)
    }
    /// Open `review`; nothing is saved.
    fn open(&mut self, review: goal_view::Review) {
        self.save = Save::Reviewing(review);
    }
    /// Close an idle review; a running save or receipt stays.
    fn close(&mut self) {
        if matches!(self.save, Save::Reviewing(_)) {
            self.save = Save::Idle;
        }
    }
    /// The open review's save started as `job`; a save never starts without one.
    fn begin(&mut self, job: SaveJob) {
        debug_assert!(matches!(self.save, Save::Reviewing(_)), "no review is open");
        if !matches!(self.save, Save::Reviewing(_)) {
            return;
        }
        if let Save::Reviewing(review) = std::mem::take(&mut self.save) {
            self.save = Save::Saving {
                review,
                job,
                leave: false,
            };
        }
    }
    /// Leave once the running save reads back.
    fn leave_after_save(&mut self) {
        if let Save::Saving { leave, .. } = &mut self.save {
            *leave = true;
        }
    }
    /// The running save's result once it reads back. A disconnected worker is a failure.
    fn poll(&self) -> Option<Result<seed_me::Receipt, String>> {
        let Save::Saving { job, .. } = &self.save else {
            return None;
        };
        match job.try_recv() {
            Ok(result) => Some(result),
            Err(std::sync::mpsc::TryRecvError::Empty) => None,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => Some(Err(
                "Handoff worker disconnected; inspect the recovery path before retrying.".into(),
            )),
        }
    }
    /// End the save with its result: a receipt, or back to the review after a failure.
    fn finish(&mut self, result: &Result<seed_me::Receipt, String>) {
        if let Save::Saving { review, .. } = std::mem::take(&mut self.save) {
            self.save = match result {
                Ok(receipt) => Save::Saved(receipt.clone()),
                Err(_) => Save::Reviewing(review),
            };
        }
    }
}

impl Default for BrainDump {
    fn default() -> Self {
        Self::with_mode(Arc::new(Simulated), false)
    }
}
/// The verified board associated with a rendered frame. Only the terminal owner acknowledges
/// successful output; this is neither a full screen snapshot nor proof of human attention.
pub struct Presentation {
    revision: usize,
    board: serde_json::Value,
}

impl BrainDump {
    /// The first presentation candidate for this board, only when this frame exposes reading
    /// content. Calling this never records output; the owner must acknowledge successful IO.
    pub fn presentation(&self) -> Option<Presentation> {
        if !self.screen.reading_visible || self.screen.help || self.screen.leave_prompt {
            return None;
        }
        self.conversation.presentation()
    }
    /// Acknowledge the frame only after terminal drawing and synchronized output succeed.
    /// Repeated or stale acknowledgments add nothing, and nothing is written to disk here.
    pub fn presented(&mut self, presentation: Presentation) {
        self.conversation.presented(presentation);
    }
    /// Think that reads with `host`. [`BrainDump::default`] is practice: a simulated reading
    /// and nothing leaves the machine.
    pub fn with_host(host: Arc<dyn BoardHost>) -> Self {
        Self::with_mode(host, true)
    }
    fn with_mode(host: Arc<dyn BoardHost>, real: bool) -> Self {
        let host = host.new_session().unwrap_or(host);
        Self {
            input: Note::new(""),
            add_more: false,
            conversation: Conversation::default(),
            screen: Screen::default(),
            shaping: Shaping::default(),
            handoff: Handoff::default(),
            host,
            voice: None,
            real,
            notice: "Nothing saved. Submit when ready; no guess before then.".into(),
            local_checks: vec![],
            copy: None,
            exit: Exit::Stay,
        }
    }
    /// Populate a fresh unsent draft and retain its separate provenance, never as a source.
    /// Menu supplies a new visit; subsequent edits are submitted only by the normal F2 path.
    pub(crate) fn offer_draft(&mut self, draft: crate::think_record::DraftSuggestion) {
        assert!(self.conversation.is_empty() && self.input.text.is_empty());
        if self.input.insert(&draft.text).is_err() {
            self.refuse(layout::LIMIT_TEXT);
            return;
        }
        self.conversation.offer_draft(draft);
        self.notice = "Tinkery's guess · edit freely; F2 sends your words. Nothing saved.".into();
    }
    /// What think asks of whoever opened it, read after each tick.
    pub fn exit(&self) -> &Exit {
        &self.exit
    }
    /// What has been said and read so far.
    pub fn conversation(&self) -> &Conversation {
        &self.conversation
    }
    /// The status line: what just happened, in plain words.
    pub fn notice(&self) -> &str {
        &self.notice
    }
    /// The goal saved in this visit, if any.
    pub fn saved(&self) -> Option<&seed_me::Receipt> {
        self.handoff.receipt()
    }
    /// Ask to go back. Never replaces a request to open tend.
    fn leave(&mut self) {
        if self.exit == Exit::Stay {
            self.exit = Exit::Menu;
        }
    }
    /// Stand in for a saved goal, as if the handoff had read back.
    #[cfg(test)]
    pub(crate) fn set_saved(&mut self, receipt: seed_me::Receipt) {
        self.handoff.save = Save::Saved(receipt);
    }
    fn quitting(&self) -> bool {
        self.exit == Exit::Menu
    }
    /// A model call or a save is in flight.
    pub fn running(&self) -> bool {
        self.shaping.job.is_some() || self.handoff.saving()
    }
    fn reading_line_style(&self, line: &str, palette: Palette) -> ratatui::style::Style {
        if self
            .conversation
            .changed_lines()
            .is_some_and(|changed| !changed.contains(line))
        {
            palette.muted
        } else {
            palette.ink
        }
    }
    /// The recorded parts leave no open question, and nothing is in flight or unsent.
    /// This does not establish that the reading captured every requirement.
    /// Ready is not required to save, and saving still needs confirmation.
    pub fn goal_ready(&self) -> bool {
        self.input.text.is_empty() && self.goal_ready_without_local_text()
    }
    fn goal_ready_without_local_text(&self) -> bool {
        self.conversation.nothing_open()
            && !self.running()
            && self.shaping.ready.is_none()
            && !self.voice_active()
    }
    /// Each extracted card's id and position, as the next request carries them.
    pub fn card_positions(&self) -> Vec<(String, f64, f64)> {
        self.screen
            .canvas
            .state
            .data
            .nodes
            .iter()
            .map(|n| {
                let (x, y) = n.pos();
                (n.id().to_owned(), x, y)
            })
            .collect()
    }
    /// Every original dump and answer, verbatim and labelled, as Markdown.
    pub fn originals(&self) -> String {
        self.conversation
            .sources()
            .iter()
            .map(|s| {
                format!(
                    "## Original {} / {}\n\n{}",
                    s.id,
                    s.in_reply_to.as_deref().unwrap_or("dump"),
                    s.text
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    }
    /// What "Copy the reading" copies: the reading, its questions, settled answers, extractions and originals,
    /// each marked provisional. Copying confirms nothing.
    pub fn copy_text(&self) -> String {
        let mut text = "# Tinkery / provisional board\n\nNothing confirmed. Unsaved.\n".to_owned();
        if let Some(g) = self.conversation.board() {
            if let Some(f) = &g.reading {
                text.push_str(&format!(
                    "\n## I think this is about… / guess\n\n{}\n\nSupporting words:\n\n{}\n",
                    f.text,
                    f.supports
                        .iter()
                        .map(|anchor| format!("> {} (original {})", anchor.quote, anchor.source))
                        .collect::<Vec<_>>()
                        .join("\n\n")
                ));
            }
            if !g.other_goals.is_empty() {
                text.push_str(&format!(
                    "\nalso in your dump: {}\n",
                    g.other_goals.join("; ")
                ));
            }
            if !g.deferred.is_empty() {
                text.push_str(&format!("\nfor the seed: {}\n", g.deferred.join("; ")));
            }
            text.push_str(&format!(
                "\n## Success criteria / proposed\n\n{}\n\n## Doesn't fit yet\n\n{}\n",
                agent_text(g.outcome.as_deref().unwrap_or("Not yet captured")),
                self.misfit_text()
            ));
            if let Some(q) = self.conversation.focused_question() {
                text.push_str(&format!(
                    "\n## One question in focus\n\n{}\n",
                    agent_text(&q.text)
                ));
            }
            for q in g.questions.iter().filter(|q| {
                self.conversation
                    .focused_question()
                    .is_none_or(|f| f.id != q.id)
                    && self.conversation.available(q)
            }) {
                text.push_str(&format!("\nQueued: {}\n", agent_text(&q.text)));
            }
            text.push_str("\n## Possible approaches / not accepted\n");
            for a in &g.alternatives {
                text.push_str(&format!("\n### {} / candidate\n", a.text()));
            }
        }
        if !self.conversation.settled().is_empty() {
            text.push_str(&format!(
                "\n## Settled questions / answered, not confirmation\n\n{}\n",
                self.settled_text()
            ));
        }
        text.push_str("\n## Your deliberate extractions / exact excerpts\n");
        for f in self.conversation.fragments() {
            text.push_str(&format!(
                "\n### {} / original {} bytes {}..{}\n\n{}\n",
                f.id, f.source, f.start, f.end, f.text
            ));
        }
        text.push_str(&format!("\n{}", self.originals()));
        text
    }
    fn settled_text(&self) -> String {
        self.conversation
            .settled()
            .iter()
            .map(|s| {
                format!(
                    "{}\nAnswer / original {}: {}",
                    s.question.text,
                    s.source,
                    self.conversation.sources()[s.source - 1].text
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    }
    fn details_text(&self) -> String {
        let settled = if self.conversation.settled().is_empty() {
            String::new()
        } else {
            format!(
                "Settled questions / answered, not confirmation\n{}\n\n",
                self.settled_text()
            )
        };
        let open = self.conversation.board().map_or(String::new(), |g| {
            g.questions
                .iter()
                .filter(|q| self.conversation.available(q))
                .map(|q| q.text.as_str())
                .collect::<Vec<_>>()
                .join("\n")
        });
        let skipped = self
            .conversation
            .skipped()
            .iter()
            .map(|q| q.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let context = format!(
            "{settled}{}{}",
            if open.is_empty() {
                String::new()
            } else {
                format!("Still open\n{open}\n\n")
            },
            if skipped.is_empty() {
                String::new()
            } else {
                format!("Skipped, not resolved\n{skipped}\n\n")
            }
        );
        context+&self.conversation.board().map_or_else(|| "No reading yet.".into(), |g| format!("Success criteria\n{}\n\nDoesn’t fit yet\n{}\n\nPossible approaches / not accepted\n{}", agent_text(g.outcome.as_deref().unwrap_or("Not yet captured")), self.misfit_text(), g.alternatives.iter().map(|a| a.text()).collect::<Vec<_>>().join("\n\n")))
    }
    fn misfit_text(&self) -> String {
        let mut unresolved = self
            .conversation
            .board()
            .map(|g| {
                g.misfits
                    .iter()
                    .map(|a| format!("{} (original {})", a.quote, a.source))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if let Some(g) = self.conversation.board() {
            unresolved.extend(g.unresolved_notes.iter().cloned());
        }
        if let Some((id, _)) = self.conversation.scope_pending()
            && let Some(source) = self.conversation.sources().iter().find(|s| s.id == *id)
        {
            unresolved.push(format!("Added words — scope not answered\n{}", source.text));
        }
        if unresolved.is_empty() {
            "None identified by the agent; this does not mean everything fits.".into()
        } else {
            unresolved.join("\n\n")
        }
    }
    fn encoded_bytes(&mut self) -> Option<usize> {
        if let Some(bytes) = self.shaping.encoded_bytes {
            return bytes;
        }
        let bytes = self
            .preview_send(&self.input.text)
            .ok()
            .filter(|preview| preview.waiting_failure.is_none())
            .map(|preview| preview.bytes);
        self.shaping.encoded_bytes = Some(bytes);
        bytes
    }
    fn preview_send(&self, words: &str) -> Result<SendPreview, String> {
        let staged = self
            .conversation
            .stage(
                words,
                self.add_more,
                self.shaping.ready.is_some(),
                self.card_positions(),
            )
            .map_err(|()| layout::SCOPE_REFUSAL.to_owned())?;
        let mut preview = self.conversation.clone();
        let mut request = staged.request.clone();
        if staged.adds_words(&preview) {
            preview.commit(&staged);
        }
        let waiting_failure = if let Some((r, result)) = &self.shaping.ready {
            let failure = preview.apply(r.clone(), result.clone()).err();
            // Submit retains the previous board on failure and still retries. The size
            // display stays unknown until that failure has been handled by submit.
            preview.refresh(&mut request);
            failure
        } else {
            None
        };
        let bytes = serde_json::to_vec(&request)
            .map_err(|e| format!("Cannot encode request: {e}. Words retained."))?
            .len();
        let context_over = bytes > REQUEST_LIMIT
            && self
                .conversation
                .stage(
                    "",
                    self.add_more,
                    self.shaping.ready.is_some(),
                    self.card_positions(),
                )
                .is_ok_and(|s| {
                    let mut context = s.request;
                    preview.refresh(&mut context);
                    serde_json::to_vec(&context).map_or(true, |b| b.len() > REQUEST_LIMIT)
                });
        Ok(SendPreview {
            staged,
            request,
            bytes,
            context_over,
            waiting_failure,
        })
    }
    fn diagnostics(&self) -> Vec<String> {
        let mut logs = self
            .local_checks
            .iter()
            .map(|d| serde_json::to_string(d).expect("decision serializes"))
            .collect::<Vec<_>>();
        logs.extend(self.host.diagnostics());
        logs
    }
    /// F2: sends the writing box as an answer or added words. Refuses, with a notice, while a
    /// call is running, during review, or before an addition's scope is answered; during a
    /// take it stops the take and sends its text once.
    pub fn submit(&mut self) {
        self.shaping.encoded_bytes = None;
        if self.handoff.started() {
            return;
        }
        if self.voice_active() {
            self.voice
                .as_mut()
                .unwrap()
                .stop(true, std::time::Instant::now());
            return;
        }
        if self.running() {
            self.refuse("Still thinking; new typing is kept for your next submit.");
            return;
        }
        if self.add_more && self.conversation.scope_pending().is_some() {
            self.refuse("Answer the added-words scope question first; your draft stays local.");
            return;
        }
        let SendPreview {
            staged,
            request,
            bytes: measured,
            context_over,
            ..
        } = match self.preview_send(&self.input.text) {
            Ok(preview) => preview,
            Err(message) => {
                self.refuse(message);
                return;
            }
        };
        let mut cap = checks::Decision::new(
            "encoded-request-cap",
            "blocking",
            if measured > REQUEST_LIMIT {
                "reject"
            } else {
                "pass"
            },
            format!(
                "Encoded request: {measured} / {REQUEST_LIMIT} bytes; no truncation or dropped originals."
            ),
            std::time::Instant::now(),
        );
        cap.request_key = checks::request_key(&request);
        self.local_checks.push(cap);
        if measured > REQUEST_LIMIT {
            self.refuse(if context_over {
                "Earlier words are too long.\nWords kept. F1: copy elsewhere."
            } else {
                "Too long to send. Words kept.\nCtrl-A/C copy; replace, F2."
            });
            self.screen.refused = Some(if context_over {
                Refusal::EarlierWordsTooLong
            } else {
                Refusal::DraftTooLong
            });
            return;
        }
        if staged.adds_words(&self.conversation) {
            self.conversation.commit(&staged);
            // Answers keep the dump in view (reachable with Ctrl-PgUp/PgDn). Added words are
            // unresolved and asked about, so they come into view.
            if staged.addition {
                self.screen.source_view = self.conversation.sources().len() - 1;
                self.screen.source_scroll = 0;
                self.screen.source_cursor = 0;
            }
            self.screen.selection = None;
            self.screen.drag_anchor = None;
            self.input = Note::new("");
            self.screen.input_scroll = 0;
            self.add_more = false;
            if staged.addition {
                self.notice="Added words kept unresolved, outside the reading, until you answer their scope question.".into();
                return;
            }
        }
        if self.conversation.awaiting_scope() {
            self.refuse(
                "Added words remain unresolved; answer their scope question before reshaping.",
            );
            return;
        }
        if self.conversation.caught_up() {
            self.refuse(layout::EMPTY_SEND_TEXT);
            return;
        }
        if let Some((r, result)) = self.shaping.ready.take() {
            self.apply_result(r, result);
            if self.conversation.caught_up() {
                return;
            }
        }
        self.conversation.forget_changes();
        let host = self.host.clone();
        let worker_request = request.clone();
        self.shaping.job = Some(Job::launch(move |cancelled| {
            host.reshape(worker_request, cancelled)
        }));
        self.shaping.request = Some(request);
        self.screen.canvas.cancel_gesture();
        self.screen.drag_anchor = None;
        self.notice = "Thinking after submit… Esc cancels. New typing stays local.".into();
    }
    /// Place a card for the newest fragment on the canvas, clear of the others.
    fn place_card(&mut self) {
        let Some(fragment) = self.conversation.fragments().last() else {
            return;
        };
        let canvas = &mut self.screen.canvas;
        let i = self.conversation.fragments().len() - 1;
        if i == 0 {
            canvas.state.zoom = 0.1;
            canvas.state.viewport_x = (f64::from(canvas.area.width) / 2.0 - 1.0) / 0.1;
            canvas.state.viewport_y = (f64::from(canvas.area.height) / 2.0 - 1.0) / 0.1;
        }
        let x = (i % 2) as f64 * 200.0;
        let mut y = (i / 2) as f64 * 80.0;
        while canvas.state.data.nodes.iter().any(|n| {
            let (nx, ny) = n.pos();
            let (w, h) = n.size();
            x < nx + w && x + 190.0 > nx && y < ny + h && y + 70.0 > ny
        }) {
            y += 80.0;
        }
        canvas.state.data.nodes.push(CanvasNode::Text(TextNode {
            id: fragment.id.clone(),
            title: Some(intact_view::title(&fragment.text)),
            text: fragment.text.clone(),
            x,
            y,
            width: 190.0,
            height: 70.0,
            color: None,
            shape: Default::default(),
        }));
    }
    /// Advances time: lands a finished reading or save and reads voice events. Never blocks.
    pub fn tick(&mut self) {
        self.voice_tick();
        self.goal_tick();
        if let Some(result) = self.shaping.job.as_ref().and_then(Job::poll) {
            self.shaping.encoded_bytes = None;
            self.shaping.job.take();
            let request = self.shaping.request.take().unwrap();
            if self.input.text.is_empty() && !self.voice_active() {
                self.apply_result(request, result);
            } else {
                self.shaping.ready = Some((request, result));
                self.notice =
                    "Reply ready; no interruption while typing. F2 submits when ready.".into();
            }
        }
    }
    fn apply_result(&mut self, request: BoardRequest, result: Result<Guess, String>) {
        self.shaping.encoded_bytes = None;
        match self.conversation.apply(request, result) {
            Ok(()) => {
                self.screen.paper_scroll = 0;
                self.notice =
                    "Reshaped after submit. Your positions kept; nothing confirmed.".into();
            }
            Err(e) => {
                self.notice = format!(
                    "Reshape failed: {e} Original and previous board retained; F2 retries."
                );
            }
        }
    }
    fn below_minimum(&self) -> bool {
        self.screen.area.width < MIN_WIDTH || self.screen.area.height < MIN_HEIGHT
    }
    // A size warning hides the review: retain its words and read position until it is visible.
    fn review_below_minimum(&self) -> bool {
        self.handoff.review().is_some() && self.below_minimum()
    }
    /// Bracketed paste into whatever has focus. Never sends.
    pub fn paste(&mut self, text: &str) {
        if self.review_below_minimum() {
            return;
        }
        self.shaping.encoded_bytes = None;
        self.conversation.forget_changes();
        if !self.screen.help {
            self.screen.refused = None;
        }
        if self.screen.leave_prompt
            || self.screen.help
            || self.screen.details
            || self.screen.original
        {
            return;
        }
        if self.goal_paste(text) {
            return;
        }
        if !self.screen.original
            && !self.screen.help
            && !self.screen.board_focus
            && let Err(_) = self.input.insert(text)
        {
            self.refuse(layout::LIMIT_TEXT);
        }
    }
    /// Makes a blank think with this one's host, saving and voice settings, for the next visit
    /// from the menu. Only the session's frozen launch declaration carries over, never
    /// conversation, named confirmer attribution or goal-scoped authorization.
    pub fn fresh_factory(&self) -> Box<dyn Fn() -> Self> {
        let host = self.host.clone();
        let real = self.real;
        let config = self.handoff.config.clone();
        let voice = self.voice.is_some();
        let record_root = self.handoff.record_root.clone();
        let repo = self.handoff.repo.clone();
        let declaration = self.handoff.confirmation.fresh_declaration();
        Box::new(move || {
            let mut dump = Self::with_mode(host.clone(), real);
            dump.handoff.config = config.as_ref().map(seed_me::Config::fresh);
            dump.handoff.record_root = record_root.clone();
            dump.handoff.repo = repo.clone();
            dump.handoff.confirmation = declaration.clone();
            if voice {
                dump = dump.with_voice(Box::new(crate::voice::ProcessHost::new()));
            }
            dump
        })
    }
    /// Text think asked to copy, taken once; see [`BrainDump::copy_result`].
    pub fn take_copy_request(&mut self) -> Option<String> {
        self.copy.take()
    }
    /// Whether F1's Model and runs row was picked since the last ask; the launch opens the
    /// panel. Asking clears it.
    pub fn take_model_request(&mut self) -> bool {
        std::mem::take(&mut self.screen.model)
    }
    /// Whether the copy reached the terminal's clipboard, so the notice can say so.
    pub fn copy_result(&mut self, sent: bool) {
        self.notice = match (sent, self.handoff.receipt().is_some()) {
            (true, true) => "Copied.",
            (true, false) => "Copy sent; host may require clipboard permission.",
            (false, true) => "Clipboard send failed; goal retained.",
            (false, false) => "Clipboard send failed; board retained.",
        }
        .into();
    }
    /// One key press. Releases and F2 below the minimum size are ignored;
    /// navigation and copying never send or save.
    pub fn handle_key(&mut self, key: KeyEvent) {
        self.shaping.encoded_bytes = None;
        self.conversation.forget_changes();
        if key.kind == KeyEventKind::Release {
            return;
        }
        if key.code == KeyCode::F(2) && self.below_minimum() {
            return;
        }
        if !self.screen.help && key.code != KeyCode::F(1) {
            self.screen.refused = None;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        // One key map (#440): F1 lists, F2 sends, F6 speaks; other function keys do nothing.
        if let KeyCode::F(n) = key.code
            && (!matches!(n, 1 | 2 | 6)
                || n != 6 && (key.kind == KeyEventKind::Repeat || !key.modifiers.is_empty()))
        {
            return;
        }
        if key.code == KeyCode::Esc
            && self.handoff.saving()
            && !self.screen.help
            && key.modifiers.is_empty()
        {
            if let Some(voice) = &mut self.voice {
                voice.close();
            }
            self.handoff.leave_after_save();
            self.notice = "Saving your goal; back to the menu once it's saved.".into();
            return;
        }
        if help_view::handle_key(self, key) {
            return;
        }
        if self.review_below_minimum()
            && !self.screen.leave_prompt
            && !matches!(key.code, KeyCode::Esc | KeyCode::F(1))
        {
            return;
        }
        if self.handoff.receipt().is_some() {
            if key.kind == KeyEventKind::Repeat || !key.modifiers.is_empty() {
                return;
            }
            if key.code == KeyCode::F(1) {
                self.screen.help = true;
                self.screen.how = false;
                self.screen.original_scroll = 0;
            } else if key.code == KeyCode::Esc {
                self.request_leave();
            } else {
                self.goal_key(key);
            }
            return;
        }
        if self.voice_key(key) {
            return;
        }
        if self.screen.leave_prompt && key.kind == KeyEventKind::Repeat {
            return;
        }
        // Ctrl-C never leaves: it copies a selection, or cancels a shaping call. A save
        // cannot be cancelled, so Ctrl-C during one claims nothing.
        if ctrl && key.code == KeyCode::Char('c') {
            if let Some(text) = self.selected_text() {
                self.copy = Some(text);
            } else if self.shaping.job.is_some() && !self.screen.leave_prompt {
                self.shaping.job.take();
                self.shaping.request = None;
                self.notice = "Cancelled; original and previous board retained. F2 retries.".into();
            }
            return;
        }
        if self.screen.leave_prompt {
            match key.code {
                KeyCode::Char('y' | 'Y')
                    if !key.modifiers.intersects(
                        KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER,
                    ) =>
                {
                    self.leave()
                }
                KeyCode::Char('n' | 'N') | KeyCode::Esc => self.screen.leave_prompt = false,
                _ => {}
            }
            return;
        }
        if key.code == KeyCode::F(1) {
            self.screen.actions.reset();
            self.screen.help = true;
            self.screen.how = false;
            self.screen.original = false;
            self.screen.details = false;
            self.screen.original_scroll = 0;
            return;
        }
        if self.handoff.review().is_some()
            && !self.screen.help
            && !self.screen.details
            && !self.screen.original
            && ctrl
            && matches!(key.code, KeyCode::PageUp | KeyCode::PageDown)
        {
            self.handle_source_key(key);
            return;
        }
        if !self.screen.original && !self.screen.help && !self.screen.details && self.goal_key(key)
        {
            return;
        }
        if key.code == KeyCode::Esc
            && self.shaping.job.is_some()
            && !self.screen.original
            && !self.screen.help
            && !self.screen.details
        {
            self.shaping.job.take();
            self.shaping.request = None;
            self.notice = "Cancelled; original and previous board retained. F2 retries.".into();
            return;
        }
        if self.screen.original || self.screen.details {
            if key.code == KeyCode::Char('h') && !ctrl {
                self.screen.help = true;
                self.screen.original = false;
                self.screen.details = false;
                self.screen.how = !self.screen.how;
                self.screen.original_scroll = 0;
                return;
            }
            match key.code {
                KeyCode::Esc => {
                    self.screen.original = false;
                    self.screen.help = false;
                    self.screen.details = false;
                }
                KeyCode::PageDown | KeyCode::Down => {
                    self.screen.original_scroll = self.screen.original_scroll.saturating_add(5)
                }
                KeyCode::PageUp | KeyCode::Up => {
                    self.screen.original_scroll = self.screen.original_scroll.saturating_sub(5)
                }
                _ => {}
            }
            return;
        }
        if self.handle_source_key(key) {
            return;
        }
        match key.code {
            KeyCode::Esc if key.kind == KeyEventKind::Press && key.modifiers.is_empty() => {
                self.request_leave();
            }
            KeyCode::F(2) => self.submit(),
            KeyCode::Tab | KeyCode::BackTab
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.toggle_original_focus()
            }
            KeyCode::PageDown => {
                self.screen.paper_scroll = self.screen.paper_scroll.saturating_add(5)
            }
            KeyCode::PageUp => {
                self.screen.paper_scroll = self.screen.paper_scroll.saturating_sub(5)
            }
            KeyCode::Char('f') if self.screen.board_focus && ctrl => self
                .screen
                .canvas
                .state
                .fit_to_view(self.screen.canvas.area),
            KeyCode::Char(c) if !ctrl => {
                // Typing always writes; the app returns focus to the writing box itself.
                self.focus_writing();
                if self.input.insert(&c.to_string()).is_err() {
                    self.refuse(layout::LIMIT_TEXT);
                }
            }
            KeyCode::Enter => {
                self.focus_writing();
                if self.input.insert("\n").is_err() {
                    self.refuse(layout::LIMIT_TEXT);
                }
            }
            KeyCode::Backspace => {
                self.focus_writing();
                self.input.backspace()
            }
            KeyCode::Delete => {
                self.focus_writing();
                self.input.delete()
            }
            KeyCode::Left
            | KeyCode::Right
            | KeyCode::Up
            | KeyCode::Down
            | KeyCode::Home
            | KeyCode::End => {
                let motion = match key.code {
                    KeyCode::Left => Motion::Left,
                    KeyCode::Right => Motion::Right,
                    KeyCode::Up => Motion::Up,
                    KeyCode::Down => Motion::Down,
                    KeyCode::Home => Motion::Home,
                    _ => Motion::End,
                };
                let extend = key.modifiers.contains(KeyModifiers::SHIFT);
                self.input
                    .navigate(motion, extend, self.screen.input_area.width);
            }
            KeyCode::Char('a') if ctrl => self.select_all(),
            KeyCode::Char('u') if ctrl => self.input = Note::new(""),
            _ => {}
        }
    }
    /// Says why an action did nothing. The reason shows where the person is looking until
    /// their next key or paste, then stays only as the last result in How it works.
    fn refuse(&mut self, why: impl Into<String>) {
        self.notice = why.into();
        self.screen.refused = Some(Refusal::Other);
    }
    /// Skip the question in focus; F1 can bring it back.
    fn skip_question(&mut self) {
        if self.conversation.skip_focused() {
            self.notice = "Question skipped. F1 can bring it back.".into();
        }
    }
    /// Switch the writing box between a new dump and an answer to the focused question.
    fn new_dump(&mut self) {
        self.add_more = !self.add_more;
        self.focus_writing();
        self.notice = if self.add_more {
            "New dump, not an answer. F2 sends it; F1 goes back to answering."
        } else {
            "Typing answers the focused question; F2 sends."
        }
        .into();
    }
    /// Copy the reading.
    fn copy_board(&mut self) {
        let text = self.copy_text();
        if text.len() > 128 * 1024 {
            self.refuse("Copy exceeds 128 KiB; not sent.");
        } else {
            self.copy = Some(text);
        }
    }
    /// Show or hide the extracted cards that replace the original. While
    /// they cover it the original is hidden, so it holds no selection and no focus.
    /// F1 offers it only once a card exists.
    fn toggle_cards(&mut self) {
        self.screen.show_extractions = !self.screen.show_extractions;
        if self.screen.show_extractions {
            // Cards take the focus the original had (Enter on a card returns to its
            // source); the hidden original keeps no selection to copy.
            self.screen.board_focus = true;
            self.input.collapse();
            self.screen.input_drag = false;
            self.screen.selection = None;
        }
        self.screen.canvas.cancel_gesture();
        self.screen.drag_anchor = None;
    }
    fn undo_skip(&mut self) {
        match self.conversation.undo_skip() {
            Some(true) => self.notice = "Skip undone; question restored.".into(),
            Some(false) => {
                self.refuse("That question has since been answered; it wasn't reopened.")
            }
            None => {}
        }
    }
    /// Tab / Shift-Tab toggle between writing and the left original. Needs a source.
    fn toggle_original_focus(&mut self) {
        if self.screen.board_focus {
            self.focus_writing();
        } else if !self.conversation.sources().is_empty()
            && self.handoff.review().is_none()
            && !self.screen.show_extractions
        {
            self.screen.board_focus = true;
            self.input.collapse();
            self.screen.input_drag = false;
            self.screen.selection = None;
            self.screen.drag_anchor = None;
            self.screen.canvas.cancel_gesture();
        }
    }
    /// Return to the writing box. A selection in the original belongs to that focus
    /// and is dropped here, so a later Ctrl-C can never copy it by accident.
    pub(super) fn focus_writing(&mut self) {
        self.screen.board_focus = false;
        self.screen.selection = None;
        self.screen.drag_anchor = None;
    }
    /// Ctrl-A selects everything in the active text surface only: the writing box, or the
    /// one original in view. It never spans sources.
    fn select_all(&mut self) {
        if self.screen.board_focus {
            if self.screen.show_extractions {
                return; // the original is covered by cards: nothing to select
            }
            if let Some(source) = self.conversation.sources().get(self.screen.source_view) {
                self.screen.selection = (!source.text.is_empty()).then_some((0, source.text.len()));
                self.screen.source_cursor = source.text.len();
            }
        } else {
            self.input.select_all();
        }
    }
    /// Exact text of the active surface's nonempty selection: the original while it has
    /// focus, otherwise the writing box. Nothing is copied from a surface that is hidden.
    fn selected_text(&self) -> Option<String> {
        if self.screen.original || self.screen.help || self.screen.details {
            return None;
        }
        if !self.screen.board_focus {
            if self.handoff.review().is_some() {
                return None;
            }
            return self.input.selected_text().map(str::to_owned);
        }
        if self.screen.show_extractions {
            return None; // cards cover the original: never copy text that isn't on screen
        }
        let (a, b) = self.screen.selection?;
        let text = &self
            .conversation
            .sources()
            .get(self.screen.source_view)?
            .text;
        let (start, end) = (a.min(b), a.max(b));
        text.get(start..end)
            .filter(|t| !t.is_empty())
            .map(str::to_owned)
    }
    /// What the left side is showing, in plain words; none while there's only the dump.
    pub(super) fn source_label(&self) -> Option<String> {
        if self.conversation.sources().len() < 2 {
            return None;
        }
        let source = self.conversation.sources().get(self.screen.source_view)?;
        let Some(id) = &source.in_reply_to else {
            return Some(
                if self.screen.source_view == 0 {
                    "your dump"
                } else {
                    "added later"
                }
                .into(),
            );
        };
        let question = self.conversation.question(id).map(|q| q.text.clone());
        Some(match question {
            Some(q) => {
                let short: String = q.chars().take(48).collect();
                let more = if q.chars().count() > 48 { "…" } else { "" };
                format!("your answer to: “{short}{more}”")
            }
            None => "your answer".into(),
        })
    }
    /// Whether the muted key bar is showing.
    pub fn key_bar_on(&self) -> bool {
        self.screen.key_bar
    }
    /// Starts with the key bar shown or hidden, so the preference outlives one visit.
    pub fn with_key_bar(mut self, on: bool) -> Self {
        self.screen.key_bar = on;
        self
    }
    fn request_leave(&mut self) {
        if let Some(voice) = &mut self.voice {
            voice.close();
        }
        if self.screen.leave_prompt
            || self.handoff.receipt().is_some()
            || (self.input.text.is_empty() && self.conversation.is_empty() && !self.running())
        {
            self.leave();
        } else {
            self.screen.leave_prompt = true;
        }
    }
    /// One mouse event in a terminal of size `area`. Ignored while asking whether to leave, or
    /// when the terminal has been resized since the last paint.
    pub fn handle_mouse(&mut self, event: MouseEvent, area: Rect) {
        self.shaping.encoded_bytes = None;
        if self.screen.leave_prompt {
            return;
        }
        if area != self.screen.area || self.review_below_minimum() {
            return;
        }
        if event.kind == MouseEventKind::Down(MouseButton::Left)
            && event.row == 1
            && event.column == area.width.saturating_sub(3)
        {
            self.screen.help = !self.screen.help;
            self.screen.actions.reset();
            self.screen.how = false;
            self.screen.original = false;
            self.screen.details = false;
            self.screen.original_scroll = 0;
            return;
        }
        if self.screen.help || self.screen.original || self.screen.details {
            self.screen.input_drag = false;
            match event.kind {
                MouseEventKind::ScrollDown => {
                    self.screen.original_scroll = self.screen.original_scroll.saturating_add(3)
                }
                MouseEventKind::ScrollUp => {
                    self.screen.original_scroll = self.screen.original_scroll.saturating_sub(3)
                }
                _ => {}
            }
            return;
        }
        if self.handoff.review().is_some() || self.handoff.receipt().is_some() {
            self.screen.input_drag = false;
            if !self.handoff.saving()
                && self.handoff.review().is_some()
                && event.kind == MouseEventKind::Down(MouseButton::Left)
                && self
                    .screen
                    .back_button
                    .contains((event.column, event.row).into())
            {
                self.handoff.close();
                self.notice = "Review closed; nothing confirmed.".into();
                return;
            }
            if matches!(
                event.kind,
                MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
            ) && self
                .screen
                .source_area
                .contains((event.column, event.row).into())
            {
                self.handle_source_mouse(event);
            }
            if self.handoff.receipt().is_some()
                && self
                    .screen
                    .agent_area
                    .contains((event.column, event.row).into())
            {
                match event.kind {
                    MouseEventKind::ScrollDown => {
                        self.screen.paper_scroll = self.screen.paper_scroll.saturating_add(3)
                    }
                    MouseEventKind::ScrollUp => {
                        self.screen.paper_scroll = self.screen.paper_scroll.saturating_sub(3)
                    }
                    _ => {}
                }
            }
            return;
        }
        if self.handoff.saving() {
            return;
        }
        if event.kind == MouseEventKind::Down(MouseButton::Left)
            && self
                .screen
                .goal_button
                .contains((event.column, event.row).into())
        {
            self.review_goal();
            return;
        }
        if self.handle_source_mouse(event) {
            return;
        }
        if self
            .screen
            .input_area
            .contains((event.column, event.row).into())
            && event.kind == MouseEventKind::Down(MouseButton::Left)
        {
            self.focus_writing();
            let wrapped = self.input.wrap(self.screen.input_area.width);
            let index = wrapped.index_at(
                usize::from(event.row - self.screen.input_area.y) + self.screen.input_scroll,
                event.column - self.screen.input_area.x,
            );
            self.input.begin_drag(index);
            self.screen.input_drag = true;
            return;
        }
        if self.screen.input_drag
            && matches!(
                event.kind,
                MouseEventKind::Drag(MouseButton::Left) | MouseEventKind::Up(MouseButton::Left)
            )
        {
            let area = self.screen.input_area;
            let row =
                usize::from(event.row.clamp(area.y, area.bottom().saturating_sub(1)) - area.y)
                    + self.screen.input_scroll;
            let column = event.column.clamp(area.x, area.right().saturating_sub(1)) - area.x;
            let index = self.input.wrap(area.width).index_at(row, column);
            // False once an edit has ended the drag: release then selects nothing.
            if !self.input.drag_to(index) || matches!(event.kind, MouseEventKind::Up(_)) {
                self.screen.input_drag = false;
            }
            return;
        }
        if self
            .screen
            .agent_area
            .contains((event.column, event.row).into())
        {
            match event.kind {
                MouseEventKind::ScrollDown => {
                    self.screen.paper_scroll = self.screen.paper_scroll.saturating_add(3)
                }
                MouseEventKind::ScrollUp => {
                    self.screen.paper_scroll = self.screen.paper_scroll.saturating_sub(3)
                }
                _ => {}
            }
            return;
        }
        if self.screen.show_extractions
            && !matches!(
                event.kind,
                MouseEventKind::Down(MouseButton::Right)
                    | MouseEventKind::Drag(MouseButton::Right)
                    | MouseEventKind::Up(MouseButton::Right)
            )
        {
            self.screen.canvas.mouse(event);
            if self.screen.canvas.state.floating_editor.is_some() {
                self.screen.canvas.finish_edit();
            }
        }
    }
}

/// Paints think, remembering where things went so the mouse lands where it was drawn.
pub fn render(frame: &mut Frame, app: &mut BrainDump, palette: Palette) {
    app.screen.reading_visible = false;
    layout::render(frame, app, palette);
    layout::render_header(frame, palette);
    goal_view::render_declaration(frame, app, palette);
    layout::render_key_bar(frame, app, palette);
    voice_input::render_notice(frame, app, palette);
    help_view::render(frame, app, palette);
    layout::render_leave_prompt(frame, app, palette);
}
fn agent_text(text: &str) -> String {
    text.lines()
        .map(|line| {
            let mut line = line.trim_start();
            let mut stripped = false;
            // The interface supplies "I think you mean…"; drop a self-label such as
            // "Goal:", "Possible goal:" or "PROVISIONAL:" (at most three words before the colon).
            while let Some(colon) = line.find(':').filter(|&i| i <= 32) {
                let label = &line[..colon];
                let lower = label.to_ascii_lowercase();
                if (1..=3).contains(&label.split_whitespace().count())
                    && (lower.ends_with("goal") || lower.starts_with("provisional"))
                {
                    line = line[colon + 1..].trim_start();
                    stripped = true;
                } else {
                    break;
                }
            }
            // A stripped label can leave a lowercase start; the reading begins as a sentence.
            let mut chars = line.chars();
            match chars.next() {
                Some(first) if stripped => first.to_uppercase().chain(chars).collect(),
                _ => line.to_owned(),
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn draw_fragments(frame: &mut Frame, app: &BrainDump, area: Rect, palette: Palette) {
    for node in &app.screen.canvas.state.data.nodes {
        let (x, y) = node.pos();
        let (w, h) = node.size();
        let z = app.screen.canvas.state.zoom;
        let x = (x - app.screen.canvas.state.viewport_x) * z
            + f64::from(area.x)
            + f64::from(area.width) / 2.0;
        let y = (y - app.screen.canvas.state.viewport_y) * z
            + f64::from(area.y)
            + f64::from(area.height) / 2.0;
        let right = x + w * z;
        let bottom = y + h * z;
        let left = x.max(f64::from(area.x));
        let top = y.max(f64::from(area.y));
        let r = right.min(f64::from(area.right()));
        let b = bottom.min(f64::from(area.bottom()));
        if r <= left || b <= top {
            continue;
        }
        let rect = Rect::new(
            left.round() as u16,
            top.round() as u16,
            (r - left).round() as u16,
            (b - top).round() as u16,
        )
        .intersection(area);
        if rect.is_empty() {
            continue;
        }
        let supported = app
            .conversation
            .board()
            .and_then(|g| g.reading.as_ref())
            .is_some_and(|f| {
                app.conversation
                    .fragments()
                    .iter()
                    .find(|fragment| fragment.id == node.id())
                    .is_some_and(|fragment| {
                        f.supports.iter().any(|anchor| {
                            anchor.source == fragment.source
                                && anchor
                                    .range(app.conversation.sources())
                                    .is_ok_and(|r| r.start < fragment.end && fragment.start < r.end)
                        })
                    })
            });
        let misfit = app.conversation.board().is_some_and(|g| {
            app.conversation
                .fragments()
                .iter()
                .find(|fragment| fragment.id == node.id())
                .is_some_and(|fragment| {
                    g.misfits.iter().any(|anchor| {
                        anchor.source == fragment.source
                            && anchor
                                .range(app.conversation.sources())
                                .is_ok_and(|r| r.start < fragment.end && fragment.start < r.end)
                    })
                })
        });
        let mut block = Block::default()
            .borders(Borders::ALL)
            .style(palette.ink)
            .border_style(if supported {
                palette.jade
            } else {
                palette.muted
            })
            .border_type(if supported {
                ratatui::widgets::BorderType::Double
            } else {
                ratatui::widgets::BorderType::Plain
            });
        if app
            .screen
            .canvas
            .state
            .selection
            .is_selected(&node.id().to_owned())
        {
            block = block.border_style(
                if supported { palette.jade } else { palette.ink }
                    .add_modifier(ratatui::style::Modifier::BOLD),
            );
        }
        if let CanvasNode::Text(n) = node
            && let Some(title) = &n.title
        {
            block = block.title(intact_view::short_title(
                title,
                rect.width.saturating_sub(2),
            ));
        }
        let inner = block.inner(rect);
        let paragraph = Paragraph::new(Note::display(node.text()))
            .style(palette.ink)
            .wrap(Wrap { trim: false });
        let clipped = x < left
            || y < top
            || right > r
            || bottom > b
            || inner.width == 0
            || paragraph.line_count(inner.width) > usize::from(inner.height);
        block = block.title_bottom(if clipped {
            "more"
        } else if misfit {
            "unresolved"
        } else {
            ""
        });
        frame.render_widget(Clear, rect);
        frame.render_widget(block, rect);
        frame.render_widget(paragraph, inner);
    }
}
fn render_input(frame: &mut Frame, app: &mut BrainDump, palette: Palette) {
    let (wrapped, mut text) = voice_input::input_text(app, palette);
    if wrapped.cursor.0 < app.screen.input_scroll {
        app.screen.input_scroll = wrapped.cursor.0;
    }
    if wrapped.cursor.0 >= app.screen.input_scroll + usize::from(app.screen.input_area.height) {
        app.screen.input_scroll = wrapped
            .cursor
            .0
            .saturating_sub(usize::from(app.screen.input_area.height.saturating_sub(1)));
    }
    text.lines = text
        .lines
        .into_iter()
        .skip(app.screen.input_scroll)
        .take(usize::from(app.screen.input_area.height))
        .collect();
    frame.render_widget(
        Paragraph::new(text).style(if app.conversation.has_draft_suggestion() {
            palette.ink.add_modifier(ratatui::style::Modifier::ITALIC)
        } else {
            palette.ink
        }),
        app.screen.input_area,
    );
    if let Some((start, end)) = app.input.selection()
        && !voice_input::previewing(app)
    {
        let area = app.screen.input_area;
        for (index, row, column) in &wrapped.positions {
            if *index < start || *index >= end {
                continue;
            }
            let Some(row) = row.checked_sub(app.screen.input_scroll) else {
                continue;
            };
            let Some(grapheme) = app.input.text[*index..].graphemes(true).next() else {
                continue;
            };
            if row >= usize::from(area.height) || grapheme == "\n" {
                continue;
            }
            for offset in 0..Note::display(grapheme)
                .trim_end_matches('\n')
                .width()
                .max(1)
            {
                if column + offset < usize::from(area.width) {
                    frame.buffer_mut()[(area.x + (column + offset) as u16, area.y + row as u16)]
                        .set_style(palette.ink.add_modifier(ratatui::style::Modifier::REVERSED));
                }
            }
        }
    }
    if !app.screen.board_focus && !app.screen.original && !app.screen.help && !app.handoff.started()
    {
        frame.set_cursor_position((
            app.screen.input_area.x
                + wrapped
                    .cursor
                    .1
                    .min(app.screen.input_area.width.saturating_sub(1)),
            app.screen.input_area.y
                + wrapped.cursor.0.saturating_sub(app.screen.input_scroll) as u16,
        ));
    }
}
/// Think as plain text at `width`×`height`, for `--snapshot` and tests.
pub fn snapshot(
    width: u16,
    height: u16,
    app: &mut BrainDump,
    no_color: bool,
) -> Result<String, std::convert::Infallible> {
    let mut t = ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height))?;
    t.draw(|f| render(f, app, Palette::new(no_color)))?;
    let b = t.backend().buffer();
    Ok((0..height)
        .map(|y| (0..width).map(|x| b[(x, y)].symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n"))
}

#[cfg(test)]
mod board_tests;
pub mod checks;
#[cfg(test)]
mod confirmation_tests;
mod conversation;
mod host;
#[cfg(test)]
mod readiness_tests;
mod reading;
#[cfg(test)]
mod record_tests;
mod review;
pub use conversation::Conversation;
#[cfg(test)]
mod goal_lines_tests;
#[cfg(test)]
mod goal_tests;
mod goal_view;
#[cfg(test)]
mod help_exit_tests;
#[cfg(test)]
mod help_overlay_tests;
mod help_view;
mod intact_view;
#[cfg(test)]
mod intent_tests;
mod layout;
#[cfg(test)]
mod layout_tests;
#[cfg(test)]
mod prompt_tests;
#[cfg(test)]
mod refusal_tests;
#[cfg(test)]
mod scope_and_keys_tests;
#[cfg(test)]
mod selection_tests;
mod voice_input;
#[cfg(test)]
mod voice_tests;

#[cfg(test)]
#[path = "brain_dump_tests.rs"]
mod tests;

#[cfg(test)]
mod presentation_tests;
