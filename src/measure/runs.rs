//! Closed runs as postmortems: gates, corrections, Pi responses and whether the merge held, one
//! run at a time and across runs. Only `p` reads; unknown shows as `?`, never as zero. No scores,
//! averages, targets, writes, network or provider calls.

mod records;
#[cfg(test)]
mod tests;

use super::visible;
use crate::{Palette, clipped, sdlc};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};
use records::{By, Closed, HOLD_DAYS, Hold, Postmortem};
use std::path::Path;

const LIST_ROWS: usize = 7;

/// Help text for the postmortem; it states what is counted and what is refused.
pub(super) const HELP: &str = concat!(
    "Measure — closed runs\n\n",
    "p reads this checkout's tink-sdlc runs and Git, only from the list. No network or model calls.\n",
    "The 20 most recently closed runs, newest first. Nothing is saved.\n\n",
    "gates: your decide receipts. Agent and automatic receipts show in details only.\n",
    "corrections: correction labels.\n",
    "pi: Pi answers in each step's launch logs. ? when any step has no readable log.\n",
    "held: not reverted on HEAD and closed 14+ days ago; too recent under that.\n",
    "? means unknown, never zero. The driving agent's responses are not recorded.\n",
    "No averages, rates, targets or scores.\n\n",
    "Esc closes help, then details, then returns to pull requests.\n",
    "q goes back to the menu. F10 or Ctrl-C quits Tinkery."
);

/// The postmortem screen inside Measure. Opening reads; leaving cancels the read.
pub(super) struct Runs {
    read: Option<sdlc::Read<Postmortem>>,
    report: Option<Postmortem>,
    notice: String,
    selected: usize,
    detail: bool,
    scroll: u16,
}

impl Runs {
    /// Start the one explicit read of the launch checkout.
    pub(super) fn start(root: &Path) -> Self {
        let root = root.to_path_buf();
        Self {
            read: Some(sdlc::Read::task(move |stop| {
                records::read(&root, stop, sdlc::DEADLINE)
            })),
            report: None,
            notice: "Reading closed runs… Esc cancels and goes back.".into(),
            selected: 0,
            detail: false,
            scroll: 0,
        }
    }

    /// Receive the read; a cancelled read never delivers.
    pub(super) fn poll(&mut self) {
        if let Some(result) = self.read.as_ref().and_then(sdlc::Read::poll) {
            self.read = None;
            match result {
                Ok(report) => {
                    self.report = Some(report);
                    self.notice.clear();
                }
                Err(reason) => {
                    self.notice = format!("{reason}\nResults unknown; press p to retry.")
                }
            }
        }
    }

    /// Plain presses only, already filtered by Measure. True leaves to the pull requests.
    pub(super) fn key(&mut self, key: KeyEvent, root: &Path) -> bool {
        let count = self.report.as_ref().map_or(0, |r| r.runs.len());
        match key.code {
            KeyCode::Esc if self.detail => {
                self.detail = false;
                self.scroll = 0;
            }
            KeyCode::Esc => return true,
            KeyCode::Char('p') if !self.detail && self.read.is_none() => *self = Self::start(root),
            KeyCode::Enter if count > 0 => {
                self.detail = true;
                self.scroll = 0;
            }
            KeyCode::Down if self.detail => self.scroll = self.scroll.saturating_add(1),
            KeyCode::Up if self.detail => self.scroll = self.scroll.saturating_sub(1),
            KeyCode::Down => self.selected = (self.selected + 1).min(count.saturating_sub(1)),
            KeyCode::Up => self.selected = self.selected.saturating_sub(1),
            _ => {}
        }
        false
    }

    /// Render memory only.
    pub(super) fn render(&mut self, frame: &mut Frame, area: Rect, palette: Palette, help: bool) {
        let footer = if help {
            "esc close help   q menu"
        } else if self.detail {
            "↑↓ scroll   esc list   ? help"
        } else if self.read.is_some() {
            "esc back   ? help"
        } else if self.report.as_ref().is_some_and(|r| !r.runs.is_empty()) {
            "↑↓ move   enter details   p reread   esc back"
        } else {
            "p reread   esc back   ? help"
        };
        frame.render_widget(
            Paragraph::new(footer).style(palette.muted),
            Rect::new(area.x, area.bottom().saturating_sub(1), area.width, 1),
        );
        let content = Rect {
            height: area.height.saturating_sub(2),
            ..area
        };
        let wrap = |text: String| {
            Paragraph::new(text)
                .style(palette.ink)
                .wrap(Wrap { trim: false })
        };
        if help {
            frame.render_widget(wrap(HELP.into()), content);
            return;
        }
        let Some(report) = &self.report else {
            frame.render_widget(wrap(self.notice.clone()), content);
            return;
        };
        if self.detail {
            let paragraph = wrap(details(&report.runs[self.selected]));
            self.scroll = self.scroll.min(
                paragraph
                    .line_count(content.width)
                    .saturating_sub(content.height as usize)
                    .min(u16::MAX as usize) as u16,
            );
            frame.render_widget(paragraph.scroll((self.scroll, 0)), content);
            return;
        }
        render_list(frame, content, report, self.selected, palette);
    }
}

fn count(value: Option<usize>) -> String {
    value.map_or_else(|| "?".into(), |n| n.to_string())
}

fn held_word(held: &Hold) -> &'static str {
    match held {
        Hold::Abandoned => "abandoned",
        Hold::Reverted(_) => "reverted",
        Hold::Held => "held",
        Hold::TooRecent => "too recent",
        Hold::Unknown(_) => "?",
    }
}

fn heading(report: &Postmortem) -> String {
    let merged = report.runs.iter().filter(|r| r.merge.is_some()).count();
    let reverted = report
        .runs
        .iter()
        .filter(|r| matches!(r.held, Hold::Reverted(_)))
        .count();
    let mut text = format!(
        "{} closed runs · {merged} merged · {reverted} reverted",
        report.runs.len()
    );
    if report.more > 0 {
        text.push_str(&format!(" · {} older unread", report.more));
    }
    if report.unreadable > 0 {
        text.push_str(&format!(" · {} unreadable", report.unreadable));
    }
    text
}

fn render_list(
    frame: &mut Frame,
    area: Rect,
    report: &Postmortem,
    selected: usize,
    palette: Palette,
) {
    if report.runs.is_empty() {
        let text = format!("{}\n\nNo closed runs in this checkout.", heading(report));
        frame.render_widget(Paragraph::new(text).style(palette.muted), area);
        return;
    }
    let rows = LIST_ROWS.min(report.runs.len());
    let start = selected
        .saturating_sub(rows / 2)
        .min(report.runs.len().saturating_sub(rows));
    frame.render_widget(
        Paragraph::new(heading(report)).style(palette.muted),
        Rect::new(area.x, area.y, area.width, 1),
    );
    let columns = "  gates  corrections  pi     held        run";
    frame.render_widget(
        Paragraph::new(columns).style(palette.muted),
        Rect::new(area.x, area.y.saturating_add(2), area.width, 1),
    );
    let mut y = area.y.saturating_add(3);
    for (index, run) in report.runs.iter().enumerate().skip(start).take(rows) {
        if y >= area.bottom() {
            break;
        }
        let chosen = index == selected;
        let numbers = format!(
            "{:<5}  {:<11}  {:<5}  {:<10}  ",
            run.gates(),
            count(run.corrections()),
            count(run.pi()),
            held_word(&run.held)
        );
        let name = clipped(
            &visible(&run.slug),
            usize::from(area.width).saturating_sub(numbers.len() + 2),
        );
        let line = Line::from(vec![
            Span::styled(if chosen { "> " } else { "  " }, palette.jade),
            Span::styled(numbers, palette.muted),
            Span::styled(
                name,
                if chosen {
                    palette.ink.bold()
                } else {
                    palette.ink
                },
            ),
        ]);
        frame.render_widget(Paragraph::new(line), Rect::new(area.x, y, area.width, 1));
        y = y.saturating_add(1);
    }
}

fn details(run: &Closed) -> String {
    let mut lines = vec![visible(&run.slug)];
    lines.push(match &run.merge {
        Some(commit) => format!("Merged as {commit}"),
        None => "Abandoned".into(),
    });
    lines.push(
        time::OffsetDateTime::from_unix_timestamp_nanos(run.closed_ns as i128)
            .map_or("Closed: unknown".into(), |t| {
                format!("Closed: {} {:02}:{:02} UTC", t.date(), t.hour(), t.minute())
            }),
    );
    lines.push(format!(
        "PR: {}",
        run.pr.as_deref().map_or("not recorded".into(), visible)
    ));
    lines.push(match &run.held {
        Hold::Abandoned => "Held: no merge to hold".into(),
        Hold::Reverted(commit) => format!("Held: reverted by {commit}"),
        Hold::Held => format!("Held: not reverted on HEAD, closed {HOLD_DAYS}+ days ago"),
        Hold::TooRecent => {
            format!("Held: too recent; not reverted yet, closed under {HOLD_DAYS} days ago")
        }
        Hold::Unknown(why) => format!("Held: unknown; {}", visible(why)),
    });
    lines.push(String::new());
    lines.push(format!("Gates: {} by you", run.gates()));
    for d in &run.decisions {
        let by = match d.by {
            By::Operator => "",
            By::Agent => " · agent-recorded, not counted",
            By::Automatic => " · automatic seed binding, not counted",
        };
        lines.push(format!(
            "  stage {} {} · {}{by}",
            d.stage,
            visible(&d.decision),
            visible(&d.reviewer)
        ));
    }
    lines.push(String::new());
    match &run.labels {
        Ok(labels) => {
            lines.push(format!(
                "Corrections: {} · labels, newest first",
                count(run.corrections())
            ));
            for label in labels.iter().rev() {
                lines.push(format!("  {}: {}", label.kind, visible(&label.words)));
            }
        }
        Err(why) => lines.push(format!("Corrections: unknown; {why}")),
    }
    lines.push(String::new());
    lines.push(format!("Pi responses: {}", count(run.pi())));
    if run.steps.is_empty() {
        lines.push("  no steps recorded".into());
    }
    for (step, n) in &run.steps {
        lines.push(format!(
            "  {}: {}",
            visible(step),
            n.map_or("unknown, no readable launch log".into(), |n| n.to_string())
        ));
    }
    lines.push("Driving agent responses: not recorded".into());
    lines.join("\n")
}
