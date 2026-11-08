//! Think's conversation with the person, turn by turn.

use super::*;
use crate::think_record::{Event, Turn};
use std::collections::{BTreeMap, BTreeSet};

/// Everything think has heard and read so far: the person's words in order (sources), the
/// excerpts they pulled out (fragments), the verified reading (board), and the question
/// history (answers, skips, restores, asks per part), and every turn in the order it happened,
/// for the think record. It owns think's rules about which
/// question is in focus, what is still open, how added words are scoped, and what the next
/// request carries. It knows nothing about the screen, keys or running jobs, so every rule
/// here can be tested without a terminal.
#[derive(Clone, Default)]
pub struct Conversation {
    sources: Vec<Source>,
    fragments: Vec<Fragment>,
    board: Option<board::Board>,
    board_revision: usize,
    presented_revision: usize,
    context: crate::think_record::ReadingContext,
    /// The highest source id the board has read.
    applied: usize,
    settled: Vec<Settled>,
    skipped: Vec<Question>,
    /// Skipped questions brought back from F1, newest first.
    restored: Vec<Question>,
    ask_counts: BTreeMap<Part, u8>,
    /// Words added outside an answer, with the question about their scope. They stay out of
    /// the reading until that question is answered.
    scope_pending: Option<(usize, Question)>,
    /// Goal lines the last answer changed, so the reading can point them out.
    changed_lines: Option<BTreeSet<String>>,
    /// Why the last reading could not be applied; cleared when one is.
    last_failure: Option<String>,
    /// What happened, in order: the think record's turns.
    turns: Vec<Turn>,
    /// The last displayed question and proposal; changed wording or suggestions get a new record.
    asked: Option<Question>,
}

/// A send that has been prepared but not yet committed to the conversation.
pub(super) struct Staged {
    /// What goes to the model. Added words whose scope is unanswered are left out.
    pub(super) request: BoardRequest,
    /// Every source, including the new words.
    sources: Vec<Source>,
    /// The new words are an addition, not an answer, so their scope must be asked.
    pub(super) addition: bool,
    scope_answered: bool,
    /// The board had read everything before this send.
    fresh: bool,
}
impl Staged {
    /// True when the send carries new words.
    pub(super) fn adds_words(&self, conversation: &Conversation) -> bool {
        self.sources.len() > conversation.sources.len()
    }
}

impl Conversation {
    /// Everything the person has sent, in order and verbatim.
    pub fn sources(&self) -> &[Source] {
        &self.sources
    }
    /// The excerpts the person pulled out.
    pub fn fragments(&self) -> &[Fragment] {
        &self.fragments
    }
    /// The current verified reading, if one has been applied.
    pub fn board(&self) -> Option<&board::Board> {
        self.board.as_ref()
    }
    /// Questions answered, with their answers.
    pub fn settled(&self) -> &[Settled] {
        &self.settled
    }
    /// Questions skipped and not brought back.
    pub fn skipped(&self) -> &[Question] {
        &self.skipped
    }
    /// The highest source id the reading has taken in.
    pub fn applied(&self) -> usize {
        self.applied
    }
    /// Added words waiting on their scope question: the source id and the question. Until it's
    /// answered those words stay out of every request.
    pub fn scope_pending(&self) -> Option<&(usize, Question)> {
        self.scope_pending.as_ref()
    }
    /// Goal lines the last answer changed, so they can be pointed out until the next input.
    pub fn changed_lines(&self) -> Option<&BTreeSet<String>> {
        self.changed_lines.as_ref()
    }
    pub(super) fn forget_changes(&mut self) {
        self.changed_lines = None;
    }
    /// Every turn so far, in the order it happened: words, questions asked, answers, skips,
    /// restores, scope and separate proposals. Kept for the record, never model context.
    pub fn turns(&self) -> &[Turn] {
        &self.turns
    }
    pub(super) fn offer_draft(&mut self, draft: crate::think_record::DraftSuggestion) {
        self.note(Event::DraftSuggested {
            text: draft.text,
            provenance: draft.provenance,
        });
    }
    pub(super) fn has_draft_suggestion(&self) -> bool {
        self.sources.is_empty()
            && self
                .turns
                .iter()
                .any(|t| matches!(t.event, Event::DraftSuggested { .. }))
    }
    pub(super) fn reading_context(&self) -> &crate::think_record::ReadingContext {
        &self.context
    }
    pub(super) fn presentation(&self) -> Option<super::Presentation> {
        (self.board_revision > self.presented_revision).then(|| super::Presentation {
            revision: self.board_revision,
            board: serde_json::to_value(self.board.as_ref().expect("applied board").wire())
                .expect("verified board serializes"),
        })
    }
    pub(super) fn presented(&mut self, presentation: super::Presentation) {
        if presentation.revision == self.board_revision
            && presentation.revision > self.presented_revision
        {
            self.context
                .presented(presentation.board, std::time::SystemTime::now());
            self.presented_revision = presentation.revision;
        }
    }
    fn note(&mut self, event: Event) {
        self.turns.push(Turn {
            at: std::time::SystemTime::now(),
            event,
        });
    }
    /// Records the question in focus as asked when it is not the one last recorded.
    fn note_focus(&mut self) {
        let focused = self.focused_question().cloned();
        if focused == self.asked {
            return;
        }
        self.asked = focused.clone();
        let Some(q) = focused else {
            return;
        };
        let suggestion = q.suggestion.clone();
        let suggestion_id = q.id.clone();
        let event = match &self.scope_pending {
            Some((source, pending)) if pending.id == q.id => Event::ScopeAsked {
                id: q.id,
                question: q.text,
                source: *source,
            },
            _ => Event::Asked {
                id: q.id,
                part: q.target,
                question: q.text,
            },
        };
        self.note(event);
        if let Some(s) = suggestion {
            self.note(Event::Suggested {
                id: suggestion_id,
                answer: s.answer,
                reason: s.reason,
            });
        }
    }
    /// Why the last reading couldn't be applied; cleared once one is.
    pub fn last_failure(&self) -> Option<&str> {
        self.last_failure.as_deref()
    }
    /// Nothing written and nothing read yet.
    pub fn is_empty(&self) -> bool {
        self.sources.is_empty() && self.board.is_none()
    }
    /// The board has read every source.
    pub fn caught_up(&self) -> bool {
        self.applied == self.sources.len()
    }

    /// A question may be asked: not skipped (nor its part), not settled, not already answered.
    pub fn available(&self, q: &Question) -> bool {
        !q.target
            .is_some_and(|target| self.skipped.iter().any(|s| s.target == Some(target)))
            && !self
                .skipped
                .iter()
                .any(|s| s.id == q.id || s.text.trim().eq_ignore_ascii_case(q.text.trim()))
            && !self.settled.iter().any(|s| same_question(&s.question, q))
            && !self
                .sources
                .iter()
                .any(|s| s.in_reply_to.as_deref() == Some(q.id.as_str()))
    }
    /// The one question in focus: added words' scope first, then a restored skip, then the
    /// board's queue.
    pub fn focused_question(&self) -> Option<&Question> {
        self.scope_pending
            .as_ref()
            .filter(|(_, q)| self.available(q))
            .map(|(_, q)| q)
            .or_else(|| {
                self.restored
                    .iter()
                    .find(|q| self.available(q))
                    .or_else(|| {
                        self.board
                            .as_ref()?
                            .questions
                            .iter()
                            .find(|q| self.available(q))
                    })
            })
    }
    /// Added words are still waiting for their scope question to be answered.
    pub fn awaiting_scope(&self) -> bool {
        self.scope_pending
            .as_ref()
            .is_some_and(|(_, q)| self.available(q))
    }
    /// Any question with this id the conversation has seen: settled, skipped or queued.
    pub fn question(&self, id: &str) -> Option<&Question> {
        self.settled
            .iter()
            .map(|s| &s.question)
            .chain(self.skipped.iter())
            .chain(self.board.iter().flat_map(|g| g.questions.iter()))
            .find(|q| q.id == id)
    }
    /// Goal parts still unclear: open on the board, missing, or skipped.
    pub fn open_parts(&self) -> Vec<Part> {
        let mut open = self
            .board
            .as_ref()
            .map_or_else(Vec::new, |g| g.open.clone());
        if let Some(g) = &self.board {
            open.extend(g.parts.missing());
        }
        open.extend(self.skipped.iter().filter_map(|q| q.target));
        open.sort();
        open.dedup();
        open
    }
    /// The reading covers every word and leaves nothing open, unplaced or unasked.
    pub fn nothing_open(&self) -> bool {
        self.board.as_ref().is_some_and(|g| g.goal().is_some())
            && self.caught_up()
            && self.open_parts().is_empty()
            && self
                .board
                .as_ref()
                .is_some_and(|g| g.unresolved_notes.is_empty() && g.misfits.is_empty())
            && self.scope_pending.is_none()
            && self.focused_question().is_none()
    }
    /// Questions a confirmed goal leaves open: queued, scope, restored, and skips never settled.
    pub fn unresolved(&self) -> Vec<Question> {
        let mut unresolved = self
            .board
            .iter()
            .flat_map(|g| g.questions.iter())
            .filter(|q| self.available(q))
            .cloned()
            .collect::<Vec<_>>();
        if let Some((_, q)) = &self.scope_pending
            && !unresolved.iter().any(|old| same_question(old, q))
        {
            unresolved.push(q.clone());
        }
        for q in &self.restored {
            if self.available(q) && !unresolved.iter().any(|old| same_question(old, q)) {
                unresolved.push(q.clone());
            }
        }
        for q in &self.skipped {
            if !self.settled.iter().any(|s| same_question(&s.question, q))
                && !unresolved.iter().any(|u| same_question(u, q))
            {
                unresolved.push(q.clone());
            }
        }
        unresolved
    }

    /// Unresolved questions and notes for review and saved context, in their display order.
    /// Linked questions replace only their decision notes; other intent and excerpts stay intact.
    /// Synthetic missing-part notes lead in part order, before authored notes and excerpts.
    pub(super) fn review_context(&self) -> (Vec<Question>, Vec<String>) {
        let unresolved = self.unresolved();
        let Some(g) = self.board() else {
            return (unresolved, vec![]);
        };
        let missing = self
            .open_parts()
            .into_iter()
            .filter(|p| {
                !unresolved.iter().any(|q| q.target == Some(*p))
                    && !g.decisions.iter().any(|d| d.part == *p)
            })
            .map(|p| format!("{} — still open", p.label()))
            .collect::<Vec<_>>();
        let notes = g
            .unresolved_notes
            .iter()
            .filter(|note| {
                !g.decisions
                    .iter()
                    .any(|d| d.covered_by(&unresolved) && d.note() == **note)
            })
            .cloned()
            .collect::<Vec<_>>();
        let notes = missing
            .into_iter()
            .chain(notes)
            .chain(
                g.misfits
                    .iter()
                    .map(|s| format!("{} (original {})", s.quote, s.source)),
            )
            .collect();
        (unresolved, notes)
    }

    /// Skip the question in focus. False when there is none.
    pub(super) fn skip_focused(&mut self) -> bool {
        let Some(q) = self.focused_question().cloned() else {
            return false;
        };
        self.note(Event::Skipped {
            id: q.id.clone(),
            part: q.target,
            question: q.text.clone(),
        });
        self.skipped.push(q);
        self.note_focus();
        true
    }
    /// Bring back the last skip. `None` when nothing was skipped; `Some(false)` when it has
    /// since been answered, so it stays closed.
    pub(super) fn undo_skip(&mut self) -> Option<bool> {
        let q = self.skipped.pop()?;
        if self.settled.iter().any(|s| same_question(&s.question, &q))
            || self
                .sources
                .iter()
                .any(|s| s.in_reply_to.as_ref() == Some(&q.id))
        {
            return Some(false);
        }
        if self
            .scope_pending
            .as_ref()
            .is_none_or(|(_, pending)| pending.id != q.id)
            && !self.restored.iter().any(|old| same_question(old, &q))
        {
            self.restored.insert(0, q.clone());
        }
        self.note(Event::Restored {
            id: q.id,
            part: q.target,
            question: q.text,
        });
        self.note_focus();
        Some(true)
    }

    /// Keep an exact excerpt of one source as a fragment. At most 32, never a duplicate.
    pub(super) fn extract(
        &mut self,
        source: usize,
        start: usize,
        end: usize,
    ) -> Result<&Fragment, String> {
        if self.fragments.len() >= 32 {
            return Err("Extraction limit: 32 cards. Nothing extracted.".into());
        }
        let text = &self
            .sources
            .iter()
            .find(|s| s.id == source)
            .ok_or("Unknown original")?
            .text;
        if !grapheme_boundary(text, start) || !grapheme_boundary(text, end) {
            return Err("Selection cuts a grapheme; nothing extracted".into());
        }
        let excerpt = text
            .get(start..end)
            .filter(|s| !s.trim().is_empty())
            .ok_or("Select a nonempty exact source range first")?;
        if self
            .fragments
            .iter()
            .any(|f| f.source == source && f.start == start && f.end == end)
        {
            return Err("That exact selection is already extracted.".into());
        }
        let fragment = Fragment {
            id: format!("f{}", self.fragments.len() + 1),
            source,
            start,
            end,
            text: excerpt.to_owned(),
        };
        self.fragments.push(fragment);
        Ok(self.fragments.last().unwrap())
    }

    /// Prepare a send of `words`. With no goal or focused question, words remain observations.
    /// Words after a current goal refine it; `add_more` explicitly asks about their scope.
    /// `ready` says a reply is waiting. Errors when an earlier addition is still unscoped.
    pub(super) fn stage(
        &self,
        words: &str,
        add_more: bool,
        ready: bool,
        layout: Vec<(String, f64, f64)>,
    ) -> Result<Staged, ()> {
        let mut sources = self.sources.clone();
        let mut settled = self.settled.clone();
        let fresh = self.caught_up() && !ready;
        let has_words = !words.trim().is_empty();
        let scope_answered = has_words
            && !add_more
            && self
                .scope_pending
                .as_ref()
                .is_some_and(|(_, q)| self.focused_question().is_some_and(|f| f.id == q.id));
        let has_goal = self.board.as_ref().is_some_and(|g| g.goal().is_some());
        let refinement = !add_more
            && self.focused_question().is_none()
            && self.scope_pending.is_none()
            && fresh
            && has_goal;
        let addition = (add_more || (has_goal && self.focused_question().is_none() && !refinement))
            && self.board.is_some()
            && has_words;
        if addition && self.scope_pending.is_some() {
            return Err(());
        }
        if has_words {
            let sid = sources.len() + 1;
            let question = self
                .focused_question()
                .filter(|_| !add_more)
                .cloned()
                .or_else(|| {
                    refinement.then(|| Question {
                        suggestion: None,
                        target: None,
                        id: format!("goal-refinement-{sid}"),
                        text: "What should change about this goal?".into(),
                    })
                });
            if let Some(q) = &question {
                settled.push(Settled {
                    question: q.clone(),
                    source: sid,
                });
            }
            sources.push(Source {
                id: sid,
                text: words.to_owned(),
                in_reply_to: question.map(|q| q.id),
            });
        }
        let mut request = BoardRequest {
            ask_counts: self.ask_counts.clone(),
            answered: vec![],
            sources,
            fragments: self.fragments.clone(),
            previous: self.board.as_ref().map(|g| g.wire()),
            skipped: self.skipped.clone(),
            settled,
            layout,
        };
        let staged = request.sources.clone();
        if !scope_answered && let Some((id, _)) = &self.scope_pending {
            request.sources.retain(|s| s.id != *id);
            request.fragments.retain(|f| f.source != *id);
        }
        request.answered = request
            .sources
            .iter()
            .filter_map(|s| s.in_reply_to.clone())
            .collect();
        Ok(Staged {
            request,
            sources: staged,
            addition,
            scope_answered,
            fresh,
        })
    }
    /// Record a staged send's words. Added words get a scope question and stay out of the
    /// reading; an answer to that question brings them in.
    pub(super) fn commit(&mut self, staged: &Staged) {
        self.sources = staged.sources.clone();
        self.settled = staged.request.settled.clone();
        if let Some(source) = self.sources.last().cloned() {
            self.context.submitted(source.id);
            let question = self
                .settled
                .iter()
                .find(|s| s.source == source.id)
                .map(|s| s.question.clone());
            self.note(match (source.in_reply_to, question) {
                (Some(id), Some(q)) if staged.scope_answered => Event::ScopeAnswered {
                    id,
                    question: q.text,
                    source: source.id,
                    answer: source.text,
                },
                (Some(id), q) => Event::Answered {
                    id,
                    question: q.map(|q| q.text).unwrap_or_default(),
                    source: source.id,
                    answer: source.text,
                },
                (None, _) => Event::Words {
                    source: source.id,
                    text: source.text,
                },
            });
        }
        if staged.addition {
            let sid = self.sources.len();
            self.scope_pending = Some((
                sid,
                Question {
                    suggestion: None,
                    target: None,
                    id: format!("scope-addition-{sid}"),
                    text: "Should these added words be part of this goal, or stay separate?".into(),
                },
            ));
            if staged.fresh {
                self.applied = self.sources.len();
            }
        } else if staged.scope_answered {
            self.scope_pending = None;
        }
        self.note_focus();
    }
    /// Bring a request up to date after a waiting reply was applied.
    pub(super) fn refresh(&self, request: &mut BoardRequest) {
        request.previous = self.board.as_ref().map(|g| g.wire());
        request.ask_counts = self.ask_counts.clone();
    }

    /// Apply the model's reply to `request`. Questions already answered, skipped or settled
    /// are dropped before the board is verified. A failure keeps the previous board.
    pub(super) fn apply(
        &mut self,
        mut request: BoardRequest,
        result: Result<Guess, String>,
    ) -> Result<(), String> {
        request.ask_counts = self.ask_counts.clone();
        request.skipped = self.skipped.clone();
        request.settled = self
            .settled
            .iter()
            .filter(|s| request.sources.iter().any(|source| source.id == s.source))
            .cloned()
            .collect();
        request.answered = self
            .sources
            .iter()
            .filter_map(|s| s.in_reply_to.clone())
            .collect();
        match result.and_then(|mut g| {
            g.questions.retain(|q| {
                !request.answered.contains(&q.id)
                    && !request.skipped.iter().any(|s| same_question(s, q))
                    && !request
                        .settled
                        .iter()
                        .any(|s| same_question(&s.question, q))
            });
            board::Board::verify(g, &request)
        }) {
            Ok(board) => {
                self.changed_lines = self.board.as_ref().and_then(|old| {
                    if !request
                        .sources
                        .iter()
                        .any(|s| s.id > self.applied && s.in_reply_to.is_some())
                    {
                        return None;
                    }
                    let previous = old.goal().unwrap_or_default();
                    let current = board.goal().unwrap_or_default();
                    let changed = current
                        .lines()
                        .filter(|line| !previous.lines().any(|old| old == *line))
                        .map(str::to_owned)
                        .collect::<BTreeSet<_>>();
                    (current != previous).then_some(changed)
                });
                if let Some(q) = board.questions.front()
                    && let Some(target) = q.target
                    && !self
                        .board
                        .as_ref()
                        .is_some_and(|old| old.questions.iter().any(|old| same_question(old, q)))
                {
                    *self.ask_counts.entry(target).or_default() += 1;
                }
                self.last_failure = None;
                self.board = Some(board);
                self.board_revision += 1;
                self.applied = request.sources.iter().map(|s| s.id).max().unwrap_or(0);
                self.note_focus();
                Ok(())
            }
            Err(e) => {
                self.last_failure = Some(e.clone());
                Err(e)
            }
        }
    }
}

/// Direct access for tests that set up a conversation mid-way.
#[cfg(test)]
impl Conversation {
    pub(super) fn sources_mut(&mut self) -> &mut Vec<Source> {
        &mut self.sources
    }
    pub(super) fn fragments_mut(&mut self) -> &mut Vec<Fragment> {
        &mut self.fragments
    }
    pub(super) fn settled_mut(&mut self) -> &mut Vec<Settled> {
        &mut self.settled
    }
    pub(super) fn set_last_failure(&mut self, failure: Option<String>) {
        self.last_failure = failure;
    }
    pub(super) fn set_changed_lines(&mut self, changed: Option<BTreeSet<String>>) {
        self.changed_lines = changed;
    }
    pub(super) fn set_board(&mut self, board: Option<board::Board>) {
        self.board = board;
    }
    pub(super) fn set_applied(&mut self, applied: usize) {
        self.applied = applied;
    }
    pub(super) fn skipped_mut(&mut self) -> &mut Vec<Question> {
        &mut self.skipped
    }
    pub(super) fn set_scope_pending(&mut self, pending: Option<(usize, Question)>) {
        self.scope_pending = pending;
    }
    pub(super) fn ask_counts(&self) -> &BTreeMap<Part, u8> {
        &self.ask_counts
    }
    pub(super) fn ask_counts_mut(&mut self) -> &mut BTreeMap<Part, u8> {
        &mut self.ask_counts
    }
}

#[cfg(test)]
#[path = "conversation_tests.rs"]
mod tests;
