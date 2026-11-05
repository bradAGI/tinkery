//! One rendering of a confirmed goal, shared by think's review and receipt and tend's detail,
//! so the person sees the same words, wrapping and scroll behaviour in each place. It draws only;
//! saving and reading are elsewhere.

use crate::Palette;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    widgets::{Paragraph, Wrap},
};

/// Direction marks, overrides and isolates that need a visible spelling in a goal.
pub(crate) fn is_direction_control(c: char) -> bool {
    matches!(c, '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
}

/// The goal block in ink, with line breaks kept and direction controls shown by code point.
pub fn goal_block(body: String, palette: Palette) -> Paragraph<'static> {
    let body = if body.contains(is_direction_control) {
        let mut shown = String::with_capacity(body.len());
        for c in body.chars() {
            if is_direction_control(c) {
                shown.extend(c.escape_default());
            } else {
                shown.push(c);
            }
        }
        shown
    } else {
        body
    };
    Paragraph::new(body)
        .wrap(Wrap { trim: false })
        .style(palette.ink)
}

/// What a saved-goal view shows; the caller owns the state, this only borrows it to draw.
pub struct SavedGoal<'a> {
    /// The composed goal block, exactly as saved.
    pub goal: &'a str,
    /// What to do next (copying the continuation prompt), pinned under the goal.
    pub next: Option<&'a str>,
    /// A muted notice at the bottom, wrapped whole so it is never clipped, or empty.
    pub notice: &'a str,
    /// Show "Saved." above the goal: true on think's receipt, false in review and tend.
    pub saved: bool,
}

/// Draws `view` in `area` and returns how many rows of the goal show. The goal scrolls when it
/// doesn't fit, and `scroll` is clamped to the last useful row, so a resize never leaves the goal
/// scrolled out of sight. `next` and the notice never scroll away.
pub fn render_saved_goal(
    frame: &mut Frame,
    scroll: &mut u16,
    area: Rect,
    palette: Palette,
    view: SavedGoal<'_>,
) -> u16 {
    if area.width == 0 || area.height == 0 {
        return 0;
    }
    let wrapped = |text: &str| {
        Paragraph::new(text)
            .wrap(Wrap { trim: false })
            .line_count(area.width) as u16
    };
    let next_rows = view.next.map_or(0, wrapped);
    let notice_rows = wrapped(view.notice).max(1);
    let rows = Layout::vertical([
        Constraint::Length(if view.saved { 2 } else { 0 }),
        Constraint::Min(1),
        Constraint::Length(next_rows + 1 + notice_rows),
    ])
    .split(area);
    if view.saved {
        frame.render_widget(Paragraph::new("Saved.").style(palette.ink), rows[0]);
    }
    let goal = goal_block(view.goal.to_owned(), palette);
    let max = goal
        .line_count(rows[1].width)
        .saturating_sub(rows[1].height as usize) as u16;
    *scroll = (*scroll).min(max);
    frame.render_widget(goal.scroll((*scroll, 0)), rows[1]);
    if let Some(next) = view.next {
        frame.render_widget(
            Paragraph::new(next)
                .wrap(Wrap { trim: false })
                .style(palette.ink),
            rows[2],
        );
    }
    if !view.notice.is_empty() {
        frame.render_widget(
            Paragraph::new(view.notice)
                .wrap(Wrap { trim: false })
                .style(palette.muted),
            Rect::new(
                rows[2].x,
                rows[2].bottom().saturating_sub(notice_rows),
                rows[2].width,
                notice_rows.min(rows[2].height),
            ),
        );
    }
    rows[1].height
}
