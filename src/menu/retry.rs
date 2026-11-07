//! Taste hands one reviewed rejection to Shape through the menu. Opening reads the old
//! result. Saving the displayed attempt is separate from drafting, confirmation and starting.

use super::*;
use crate::{
    sdlc::Read,
    taste::{Rejected, Reviewed},
};

/// A review can be cancelled until a save begins; a partial save never repeats automatically.
pub(super) struct Review {
    rejected: Rejected,
    reading: Option<Read<Reviewed>>,
    reviewed: Option<Reviewed>,
    saving: Option<Read<PathBuf>>,
    notice: String,
    scroll: u16,
    page: u16,
}

impl Review {
    fn new(rejected: Rejected) -> Self {
        let source = rejected.clone();
        Self {
            rejected,
            reading: Some(Read::task(move |stop| source.review(stop))),
            reviewed: None,
            saving: None,
            notice: String::new(),
            scroll: 0,
            page: 1,
        }
    }

    fn poll(&mut self) -> Option<PathBuf> {
        if let Some(result) = self.reading.as_ref().and_then(Read::poll) {
            self.reading = None;
            match result {
                Ok(read) => self.reviewed = Some(read),
                Err(why) => self.notice = why,
            }
        }
        if let Some(result) = self.saving.as_ref().and_then(Read::poll) {
            self.saving = None;
            match result {
                Ok(session) => return Some(session),
                Err(why) => self.notice = why,
            }
        }
        None
    }

    pub(super) fn render(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        palette: Palette,
        human: bool,
        help: bool,
    ) {
        let body = Rect {
            height: area.height.saturating_sub(1),
            ..area
        };
        self.page = crate::page(body.height);
        let mut text = if help {
            "Review the previous goal, rejection, and starting commit. Saving creates a fresh Shape session and isolated checkout. Confirming the new seed happens later in Shape.".into()
        } else if let Some(r) = &self.reviewed {
            let decisions = if r.prior.decisions.is_empty() {
                "None recorded.".into()
            } else {
                r.prior.decisions.join("\n")
            };
            let heading = if self.rejected.pinned() {
                "Starting commit"
            } else {
                "Earlier rejection did not record a commit.\nCurrent checked commit"
            };
            format!(
                "Confirmed goal\n{}\n\nYour rejection\n{}\n\n{heading}\n{}\n\nPrior decisions (provisional)\n{}\n\nSaving creates a Shape session and detached checkout under {}.",
                r.prior.goal,
                self.rejected.words(),
                self.rejected.head(),
                decisions,
                r.worktrees.display()
            )
        } else if self.reading.is_some() {
            "Reading the rejected result…".into()
        } else {
            String::new()
        };
        if !self.notice.is_empty() {
            text.push('\n');
            text.push_str(&self.notice);
        }
        // Preserve original lines and spaces; never emit terminal controls from source text.
        let lines: Vec<Line> = text
            .split('\n')
            .map(|s| Line::raw(crate::taste::safe(s)))
            .collect();
        let paragraph = Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .style(palette.ink);
        let max = paragraph
            .line_count(body.width)
            .saturating_sub(body.height.into());
        self.scroll = self.scroll.min(max.min(usize::from(u16::MAX)) as u16);
        frame.render_widget(paragraph.scroll((self.scroll, 0)), body);
        let bar = if help {
            "? / Esc close help"
        } else if self.saving.is_some() {
            "Saving attempt…"
        } else if !human {
            "Only a human can save this attempt.   Esc cancel"
        } else if self.reviewed.is_some() && self.notice.is_empty() {
            if self.rejected.pinned() {
                "F2 save attempt   ↑↓/PgDn read   Esc cancel"
            } else {
                "F2 use current checked commit and save attempt   ↑↓/PgDn read   Esc cancel"
            }
        } else {
            "↑↓/PgDn read   Esc cancel"
        };
        frame.render_widget(
            Paragraph::new(bar).style(palette.muted),
            Rect::new(area.x, body.bottom(), area.width, 1),
        );
    }
}

impl Home {
    pub(super) fn retry_key(&mut self, key: KeyEvent) {
        if key.kind == KeyEventKind::Release || !key.modifiers.is_empty() {
            return;
        }
        let View::Retry(view, _) = &mut self.view else {
            return;
        };
        if view.saving.is_some() {
            return;
        }
        match key.code {
            KeyCode::F(1) | KeyCode::Char('?') if key.kind == KeyEventKind::Press => {
                self.help = !self.help;
                view.scroll = 0;
            }
            KeyCode::Esc if self.help => self.help = false,
            KeyCode::Down => view.scroll = view.scroll.saturating_add(1),
            KeyCode::Up => view.scroll = view.scroll.saturating_sub(1),
            KeyCode::PageDown => view.scroll = view.scroll.saturating_add(view.page),
            KeyCode::PageUp => view.scroll = view.scroll.saturating_sub(view.page),
            KeyCode::Esc if key.kind == KeyEventKind::Press => {
                if let View::Retry(_, back) = std::mem::replace(&mut self.view, View::Menu) {
                    self.view = View::Taste(back);
                    self.selected = Place::Taste;
                }
            }
            KeyCode::F(10) if key.kind == KeyEventKind::Press => {
                self.view = View::Menu;
                self.help = false;
            }
            KeyCode::F(2)
                if key.kind == KeyEventKind::Press
                    && !self.help
                    && !self.agent
                    && view.notice.is_empty() =>
            {
                if let Some(review) = view.reviewed.clone() {
                    let Some(root) = self.sessions_root.clone() else {
                        view.notice = "No session storage is configured. Name a --seed-session-root and reopen.".into();
                        return;
                    };
                    let source = view.rejected.clone();
                    view.saving = Some(Read::task(move |stop| source.save(&review, &root, stop)));
                }
            }
            _ => {}
        }
    }

    pub(super) fn poll_retry(&mut self) {
        if let View::Retry(review, _) = &mut self.view
            && let Some(session) = review.poll()
        {
            let shape = Shape::session(&session)
                .with_agent(self.shape_agent.clone())
                .held(self.shape_held);
            self.view = View::Shape(Box::new(shape), None);
            self.help = false;
            self.request_sessions();
        }
    }

    pub(super) fn taste_key(&mut self, key: KeyEvent) {
        let View::Taste(taste) = &mut self.view else {
            return;
        };
        if self.help {
            if key.code == KeyCode::Char('q') {
                self.help = false;
                self.view = View::Menu;
            }
        } else if taste.key(key) {
            self.view = View::Menu;
        } else if let Some(rejected) = taste.take_retry() {
            let View::Taste(back) = std::mem::replace(&mut self.view, View::Menu) else {
                unreachable!()
            };
            self.selected = Place::Shape;
            self.view = View::Retry(Box::new(Review::new(rejected)), back);
        } else if let Some(draft) = taste.take_draft() {
            let mut dump = (self.make_dump)().with_key_bar(self.key_bar);
            dump.offer_draft(draft);
            self.selected = Place::Think;
            self.view = View::Think(Box::new(dump));
        }
    }
}
