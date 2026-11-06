//! Tend: the saved goals garden and one goal's detail, read from saved sessions.
//!
//! Shows what discovery has landed, says plainly when that is unknown, incomplete or failed,
//! and offers explicit needs links and a continuation prompt to copy; it never edits a session,
//! starts an agent or guesses a stage. Everything about session files comes from `seed_me`.

pub mod discovery;
mod garden;
mod graph;
pub(crate) mod history;
mod linking;
pub mod needs;
pub use discovery::{Discovery, Found, Outcome, Source, scan};

use crate::clipped;
pub use crate::seed_me::{Session, Stage};
use crate::{
    Palette,
    goal::confirmed::{SavedGoal, render_saved_goal},
};
use ratatui::{
    Frame,
    layout::Rect,
    widgets::{Paragraph, Wrap},
};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use unicode_width::UnicodeWidthStr;

/// How long a read may be pending before the quiet line says it is still going. A normal
/// read takes milliseconds (about 70 ms for 5,000 sessions on the measured machine), so
/// this only shows when something is actually slow, such as a cold or network disk.
pub(super) const STILL_READING_AFTER: Duration = Duration::from_secs(3);

pub(super) struct Tend {
    pub sessions: Vec<Session>,
    pub error: Option<String>,
    /// Nothing has landed yet: unknown, never "no sessions".
    pub pending: bool,
    /// The read this view waits on has run past [`STILL_READING_AFTER`]. Only the pending
    /// line reads it, and only its wording changes.
    pub still_reading: bool,
    /// The list shown reflects every read asked for. An older empty list is not proof
    /// that nothing was saved since.
    pub current: bool,
    /// A saved session to open in detail once a read that can contain it lands.
    pub want: Option<(PathBuf, Option<PathBuf>)>,
    /// Records the last read could not parse; the list may be missing them.
    pub unreadable: usize,
    /// Readable ended sessions omitted from the garden.
    pub ended: usize,
    /// The discovery version last applied.
    pub seen: u64,
    pub selected: usize,
    pub detail: bool,
    pub scroll: u16,
    /// Rows of goal on screen at the last detail draw, for PgUp/PgDn.
    pub page: u16,
    pub how: bool,
    pub help_scroll: u16,
    pub notice: String,
    pub copy: Option<String>,
    pub history: Option<history::View>,
    graph: graph::GraphView,
    last_press: Option<(PathBuf, Instant)>,
    pub(super) needs: needs::Store,
    links: Result<needs::Links, String>,
    all_sessions: Vec<Session>,
    /// Only exact recorded repo matches; absent outside a repository.
    pub(super) repo_paths: Option<HashSet<PathBuf>>,
    /// Active goals with unknown repository attribution, not known non-matches.
    pub(super) repo_unknown: usize,
    show_all: bool,
    /// The source path stays fixed while selection picks the other goal.
    linking: Option<PathBuf>,
    link_error: Option<String>,
}
impl Tend {
    /// A pending view with a captured links destination or refusal; reads no links and writes nothing.
    pub(super) fn new(link_root: Result<PathBuf, String>) -> Self {
        Self {
            sessions: vec![],
            error: None,
            pending: true,
            still_reading: false,
            current: false,
            want: None,
            unreadable: 0,
            ended: 0,
            seen: 0,
            selected: 0,
            detail: false,
            scroll: 0,
            page: 1,
            how: false,
            help_scroll: 0,
            notice: String::new(),
            copy: None,
            history: None,
            graph: graph::GraphView::default(),
            last_press: None,
            needs: needs::Store::new(link_root),
            links: Ok(needs::Links::default()),
            all_sessions: vec![],
            repo_paths: None,
            repo_unknown: 0,
            show_all: false,
            linking: None,
            link_error: None,
        }
    }
    /// Note how long the read this view waits on has been running (`None`: no worker).
    /// Wording only: it starts nothing and leaves every key as it was.
    pub(super) fn note_wait(&mut self, waited: Option<Duration>) {
        self.still_reading = waited.is_some_and(|waited| waited >= STILL_READING_AFTER);
    }
    /// Take a landed read. Selection follows the session's path, not its position.
    /// A wanted session is opened only from a read that contains it (or that began
    /// after it was asked for); until then the view stays pending.
    pub(super) fn apply(&mut self, outcome: &Outcome, fresh: bool, version: u64) {
        let wanted_at = match (&self.want, outcome) {
            (Some((path, canonical)), Ok(found)) => found
                .sessions
                .iter()
                .position(|s| s.path == *path || canonical.as_ref().is_some_and(|c| s.path == *c)),
            _ => None,
        };
        if self.want.is_some() && wanted_at.is_none() && !fresh {
            return;
        }
        self.cancel_link();
        self.links = self.needs.read().into_links();
        if self.notice.is_empty() {
            self.link_error = self.links.as_ref().err().cloned();
        }
        self.seen = version;
        self.pending = false;
        match outcome {
            Err(error) => {
                self.error = Some(error.clone());
                self.sessions.clear();
                self.all_sessions.clear();
                self.unreadable = 0;
                self.ended = 0;
                self.want = None;
            }
            Ok(found) => {
                let keep = self.current().map(|s| s.path.clone());
                self.error = None;
                self.unreadable = found.unreadable;
                self.all_sessions = found.sessions.clone();
                if let Some(index) = wanted_at
                    && self
                        .repo_paths
                        .as_ref()
                        .is_some_and(|paths| !paths.contains(&found.sessions[index].path))
                {
                    // An explicit receipt/Shape return must still open its exact goal.
                    self.show_all = true;
                }
                self.filter_sessions();
                if let Some(index) = wanted_at {
                    let path = &found.sessions[index].path;
                    let active = self.sessions.iter().position(|s| s.path == *path);
                    self.selected = active.unwrap_or(0);
                    self.detail = active.is_some();
                    self.scroll = 0;
                    self.want = None;
                } else if let Some((path, _)) = self.want.take() {
                    self.error = Some(format!("Saved session is unavailable: {}", path.display()));
                    self.sessions.clear();
                } else {
                    self.selected = keep
                        .and_then(|k| self.sessions.iter().position(|s| s.path == k))
                        .unwrap_or(self.selected)
                        .min(self.sessions.len().saturating_sub(1));
                }
            }
        }
        self.prepare_graph();
    }
    fn filter_sessions(&mut self) {
        let visible = |s: &&Session| {
            self.show_all
                || self
                    .repo_paths
                    .as_ref()
                    .is_none_or(|paths| paths.contains(&s.path))
        };
        self.ended = self
            .all_sessions
            .iter()
            .filter(visible)
            .filter(|s| s.stage.ended())
            .count();
        self.sessions = self
            .all_sessions
            .iter()
            .filter(visible)
            .filter(|s| !s.stage.ended())
            .cloned()
            .collect();
    }

    fn repo_empty_message(&self) -> Option<String> {
        if self.repo_paths.is_none() || self.show_all {
            return None;
        }
        if self.repo_unknown > 0 {
            let goals = if self.repo_unknown == 1 {
                "goal has"
            } else {
                "goals have"
            };
            Some(format!(
                "{} {goals} no readable repo record. Press a for all goals.",
                self.repo_unknown
            ))
        } else if self.unreadable == 0 {
            Some("No active goals are recorded for this repo. Press a for all goals.".into())
        } else {
            Some(format!(
                "{} couldn't be read; this repo may have active goals. Press a for all goals.",
                records(self.unreadable)
            ))
        }
    }

    fn scope_body(&self, area: Rect) -> Rect {
        if self.can_toggle_scope() {
            Rect::new(
                area.x,
                area.y.saturating_add(1),
                area.width,
                area.height.saturating_sub(1),
            )
        } else {
            area
        }
    }

    fn can_toggle_scope(&self) -> bool {
        self.repo_paths.is_some()
            && !self.pending
            && (self.current || !self.sessions.is_empty() || self.unreadable > 0)
            && !self.detail
            && self.want.is_none()
            && self.error.is_none()
            && !self.is_linking()
    }

    /// Change only the visible garden, retaining global needs and exact selection when possible.
    pub(super) fn toggle_scope(&mut self) {
        if !self.can_toggle_scope() {
            return;
        }
        let keep = self.current().map(|s| s.path.clone());
        self.show_all = !self.show_all;
        self.filter_sessions();
        self.prepare_graph();
        self.selected = keep
            .and_then(|path| self.sessions.iter().position(|s| s.path == path))
            .unwrap_or(0);
        self.last_press = None;
        self.graph.release();
        self.notice.clear();
    }

    /// Whether the saved-goal garden is showing, rather than detail or a discovery state.
    pub(super) fn garden_showing(&self) -> bool {
        !self.detail
            && !self.pending
            && self.want.is_none()
            && self.error.is_none()
            && !self.sessions.is_empty()
    }
    pub(super) fn release_pointer(&mut self) {
        self.graph.release();
    }
    /// Forward pointer mechanics to graf; only Tend can select, link or open a saved goal.
    pub(super) fn pointer(
        &mut self,
        event: crossterm::event::MouseEvent,
        body: Rect,
        now: Instant,
    ) {
        if !self.garden_showing() {
            self.graph.release();
            return;
        }
        let body = self.scope_body(body);
        self.sync_graph(body);
        if matches!(event.kind, crossterm::event::MouseEventKind::Drag(_)) {
            self.last_press = None;
        }
        let Some(id) = self.graph.pointer(event) else {
            return;
        };
        let Some(index) = self
            .sessions
            .iter()
            .position(|s| format!("{:?}", s.path) == id)
        else {
            return;
        };
        if self.linking.is_some() {
            self.selected = index;
            self.commit_link();
            self.graph.release();
            return;
        }
        let path = self.sessions[index].path.clone();
        let double = self.last_press.as_ref().is_some_and(|(last, at)| {
            *last == path
                && now
                    .checked_duration_since(*at)
                    .is_some_and(|elapsed| elapsed <= Duration::from_millis(500))
        });
        self.selected = index;
        self.last_press = Some((path, now));
        if double {
            self.detail = true;
            self.scroll = 0;
            self.notice.clear();
            self.last_press = None;
            self.graph.release();
        }
    }
    pub(super) fn current(&self) -> Option<&Session> {
        self.sessions.get(self.selected)
    }
    pub(super) fn help_text(&self) -> String {
        if self.how {
            let session = self.current().map_or(String::new(), |s| {
                format!(
                    "\nSession: {}\nStatus: {}",
                    s.path.display(),
                    s.stage.label()
                )
            });
            return format!(
                "How it works\nh or Esc returns to Help · PgUp/PgDn scroll\n{session}\n{}\n{}\n{}\n\nNothing starts on its own.",
                self.error.as_deref().unwrap_or(""),
                self.link_error.as_deref().unwrap_or(""),
                self.graph.error.as_deref().unwrap_or("")
            );
        }
        let copy = if self.detail && self.current().is_some_and(|s| s.stage.can_shape()) {
            "\ns shape: answer its questions and confirm its seed"
        } else {
            ""
        };
        let needs = self.needs_text();
        let needs = if needs.is_empty() {
            String::new()
        } else {
            format!("\n\n{}", needs.trim_start_matches(" · "))
        };
        let scope = if self.can_toggle_scope() {
            "\na toggles this repo / all goals"
        } else {
            ""
        };
        format!(
            "Right now\n{}{}{scope}\nEsc back · F10 menu\n\nF1 close · h how it works · PgUp/PgDn scroll{needs}\n\nNothing runs by itself. A saved goal is not seed or implementation approval.",
            if self.detail {
                "Read the saved goal. ↑↓ / j k or PgUp/PgDn scroll.\nr opens its Think history."
            } else if self.linking.is_some() {
                "Arrows / j k pick · Enter links or unlinks · Esc cancels."
            } else if self.garden_showing() {
                concat!(
                    "Arrows / j k move · Enter or a double click opens the saved goal.\n",
                    "l link or unlink\n",
                    "Up / Left / k previous · Down / Right / j next (newest first).\n",
                    "+ / - zoom · H J K L pan · drag a goal to move it.\n",
                    "Drag empty space to pan · scroll to zoom",
                )
            } else {
                "Esc returns to the menu."
            },
            copy
        )
    }
}

fn row(session: &Session, width: usize, now: Option<u64>) -> (String, String) {
    let suffix = format!(
        "{} · {}",
        session.stage.label(),
        now.map_or_else(
            || "time unknown".into(),
            |now| relative_time(&session.updated, now)
        )
    );
    let budget = width.saturating_sub(suffix.width() + 2);
    let title = clipped(session.goal.lines().next().unwrap_or(""), budget);
    let gap = width.saturating_sub(title.width() + suffix.width());
    (format!("{title}{}", " ".repeat(gap)), suffix)
}

pub(super) fn render(frame: &mut Frame, tend: &mut Tend, area: Rect, palette: Palette) {
    let linking_keys = format!("↑↓←→ pick   enter {}   esc cancel", tend.link_verb());
    let keys = if tend.linking.is_some() {
        linking_keys.as_str()
    } else if tend.detail {
        if tend.current().is_some_and(|s| s.stage.can_shape()) {
            "s shape   r history   esc back   F10 menu"
        } else {
            "r history   esc back   F10 menu"
        }
    } else if !tend.garden_showing() {
        // An empty repo view still offers the way back to all goals.
        if tend.can_toggle_scope() {
            "a toggle goals   esc back"
        } else {
            "esc back"
        }
    } else {
        "↑↓←→ move   enter open   l link   esc back"
    };
    frame.render_widget(
        Paragraph::new(keys).style(palette.muted),
        Rect::new(
            2,
            frame.area().height.saturating_sub(1),
            frame.area().width.saturating_sub(4),
            1,
        ),
    );
    if tend.can_toggle_scope() {
        frame.render_widget(
            Paragraph::new(if tend.show_all {
                "all · a this repo"
            } else {
                "this repo · a all"
            })
            .style(palette.muted),
            Rect {
                height: 1.min(area.height),
                ..area
            },
        );
    }
    let area = tend.scope_body(area);
    let stale_empty =
        tend.sessions.is_empty() && tend.unreadable == 0 && tend.error.is_none() && !tend.current;
    if tend.pending || tend.want.is_some() || stale_empty {
        let status = if tend.still_reading {
            "Still reading your sessions. Esc goes back."
        } else {
            "Reading your sessions. Esc goes back."
        };
        frame.render_widget(Paragraph::new(status).style(palette.muted), area);
        return;
    }
    if tend.error.is_some() {
        frame.render_widget(
            Paragraph::new("Saved session unavailable. F1 then h for details.").style(palette.ink),
            area,
        );
        return;
    }
    if tend.sessions.is_empty() {
        frame.render_widget(
            Paragraph::new(if let Some(message) = tend.repo_empty_message() {
                message
            } else if tend.unreadable > 0 {
                format!(
                    "{} couldn't be read, so nothing can be listed. Esc goes back.",
                    records(tend.unreadable)
                )
            } else if tend.ended > 0 {
                format!(
                    "No active goals. {} ended not shown. Confirm a goal in think to start one.",
                    tend.ended
                )
            } else {
                "No saved sessions yet. Confirm a goal in think to start one.".into()
            })
            .style(palette.muted)
            .wrap(Wrap { trim: false }),
            area,
        );
        if tend.ended > 0 || tend.links.is_err() || tend.repo_unknown > 0 && tend.unreadable > 0 {
            garden::render_note(frame, tend, area, palette);
        }
        return;
    }
    if tend.detail {
        let session = &tend.sessions[tend.selected];
        tend.page = render_saved_goal(
            frame,
            &mut tend.scroll,
            area,
            palette,
            SavedGoal {
                goal: &session.goal,
                next: session.stage.next(),
                notice: &tend.notice,
                saved: false,
            },
        );
        return;
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs());
    garden::render(frame, tend, area, palette, now);
}
fn records(n: usize) -> String {
    if n == 1 {
        "1 saved record".into()
    } else {
        format!("{n} saved records")
    }
}

pub(super) fn timestamp(value: &str) -> Option<u64> {
    let number = |a, b| value.get(a..b)?.parse::<i64>().ok();
    if value.get(4..5)? != "-"
        || value.get(7..8)? != "-"
        || value.get(10..11)? != "T"
        || value.get(13..14)? != ":"
        || value.get(16..17)? != ":"
    {
        return None;
    }
    let (year, month, day, hour, minute, second) = (
        number(0, 4)?,
        number(5, 7)?,
        number(8, 10)?,
        number(11, 13)?,
        number(14, 16)?,
        number(17, 19)?,
    );
    let tail = value.get(19..)?;
    let zone = if let Some(fraction) = tail.strip_prefix('.') {
        let n = fraction.bytes().take_while(u8::is_ascii_digit).count();
        if n == 0 {
            return None;
        }
        fraction.get(n..)?
    } else {
        tail
    };
    let offset = if zone == "Z" {
        0
    } else {
        if zone.len() != 6 || zone.get(3..4)? != ":" {
            return None;
        }
        let h = zone.get(1..3)?.parse::<i64>().ok()?;
        let m = zone.get(4..6)?.parse::<i64>().ok()?;
        if !(0..24).contains(&h) || !(0..60).contains(&m) {
            return None;
        }
        match zone.get(..1)? {
            "+" => h * 3600 + m * 60,
            "-" => -(h * 3600 + m * 60),
            _ => return None,
        }
    };
    let date = time::Date::from_calendar_date(
        year.try_into().ok()?,
        u8::try_from(month).ok()?.try_into().ok()?,
        day.try_into().ok()?,
    )
    .ok()?;
    let local = date
        .with_hms(
            hour.try_into().ok()?,
            minute.try_into().ok()?,
            second.try_into().ok()?,
        )
        .ok()?;
    let offset = time::UtcOffset::from_whole_seconds(offset.try_into().ok()?).ok()?;
    u64::try_from(local.assume_offset(offset).unix_timestamp()).ok()
}
fn relative_time(updated: &str, now: u64) -> String {
    let Some(at) = timestamp(updated) else {
        return "time unknown".into();
    };
    let seconds = now.abs_diff(at);
    let label = match seconds {
        0..60 => return if at <= now { "just now" } else { "in a moment" }.into(),
        60..3600 => format!("{}m", seconds / 60),
        3600..86400 => format!("{}h", seconds / 3600),
        86400..172800 if at <= now => return "yesterday".into(),
        _ => format!("{}d", seconds / 86400),
    };
    if at <= now {
        format!("{label} ago")
    } else {
        format!("in {label}")
    }
}

/// saved sessions, newest first (blocking). Unreadable records are left out here;
/// [`scan`] reports how many.
pub fn load_sessions(root: &Path) -> Result<Vec<Session>, String> {
    scan(root).map(|found| found.sessions)
}

#[cfg(test)]
mod scope_tests;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn detail_footer_names_f10_for_copyable_and_noncopyable_goals() {
        use ratatui::{Terminal, backend::TestBackend};
        for stage in [Stage::GoalSaved, Stage::Unknown] {
            let mut tend = Tend::new(Err("TEST no links".into()));
            tend.pending = false;
            tend.current = true;
            tend.detail = true;
            tend.sessions.push(Session {
                id: Some("TEST-id".into()),
                path: "/TEST/goal".into(),
                goal: "Synthetic goal".into(),
                stage,
                updated: String::new(),
            });
            let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
            terminal
                .draw(|frame| {
                    render(
                        frame,
                        &mut tend,
                        Rect::new(2, 3, 96, 26),
                        Palette::new(true),
                    )
                })
                .unwrap();
            let footer: String = (0..100)
                .map(|x| terminal.backend().buffer()[(x, 29)].symbol())
                .collect();
            assert!(footer.contains("F10 menu"), "{footer}");
            assert_eq!(footer.contains("s shape"), stage.can_shape());
        }
    }

    #[test]
    fn relative_time_validates_dates_offsets_and_handles_unknown_or_future_times() {
        let now = timestamp("2026-10-11T12:00:00Z").unwrap();
        for (value, expected) in [
            ("2026-10-11T10:00:00.123+00:00", "2h ago"),
            ("2026-10-11T12:00:00+02:00", "2h ago"),
            ("2026-10-10T12:00:00Z", "yesterday"),
            ("2026-10-11T11:58:00Z", "2m ago"),
            ("2026-10-11T13:00:00Z", "in 1h"),
            ("2026-02-30T12:00:00Z", "time unknown"),
            ("not a time", "time unknown"),
        ] {
            assert_eq!(relative_time(value, now), expected);
        }
        assert_eq!(timestamp("1970-01-01T00:00:00Z"), Some(0));
        assert!(timestamp("2024-02-29T00:00:00Z").is_some());
        assert!(timestamp("2025-02-29T00:00:00Z").is_none());
    }
    #[test]
    fn row_only_uses_first_line_and_preserves_status_with_unicode_truncation() {
        let s = Session {
            id: None,
            path: "/TEST/session".into(),
            goal: "界👩‍💻 Café goal with many words\nWhy: never in row".into(),
            stage: Stage::SeedDrafted,
            updated: "2026-10-11T10:00:00Z".into(),
        };
        let (title, status) = row(&s, 40, timestamp("2026-10-11T12:00:00Z"));
        assert!(!title.contains("Why:"));
        assert!(title.trim_end().ends_with('…'));
        assert_eq!(status, "seed drafted · 2h ago");
        assert_eq!(title.width() + status.width(), 40);
        assert!(title.contains("👩‍💻"));
        assert_eq!(row(&s, 40, None).1, "seed drafted · time unknown");
    }
}
