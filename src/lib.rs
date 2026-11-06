//! Tinkery: a calm, keyboard-first front door for agentic work.
//!
//! Each place in the menu is a phase with its own module (`think`, `tend`, `shape`, `approve`, `watch`, `taste`, `measure`); the menu
//! is the only module that knows them all. Phases never use each other: anything two of them
//! share (`goal`, `model_setup`, `repo`, `seed_me`, `think_record`, `voice`, and the palette and sizes below) is shared code that names
//! no phase. `docs/design/architecture.md` has the map and `tests/architecture.rs` enforces it.
#![deny(missing_docs, unreachable_pub)]

use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    widgets::Paragraph,
};

pub mod actions;
mod approve;
mod calendar;
pub mod goal;
pub mod measure;
pub mod menu;
pub mod model_setup;
pub mod pi;
pub mod repo;
mod sdlc;
pub mod seed_me;
pub mod shape;
/// Exact-result review without verdicts, posts or agent launches.
pub mod taste;
pub mod tend;
pub mod think;
pub mod think_record;
pub mod voice;
pub mod watch;
mod worker;

/// The narrowest terminal any screen draws for. Below it, a screen says what size it needs and
/// keeps every word; nothing is laid out half-visible.
pub const MIN_WIDTH: u16 = 80;
/// The shortest terminal any screen draws for; see [`MIN_WIDTH`].
pub const MIN_HEIGHT: u16 = 24;

/// How far PgUp/PgDn move a view `height` rows tall: a page less one row, so the last line read
/// stays in sight. Never less than one row.
pub fn page(height: u16) -> u16 {
    height.saturating_sub(1).max(1)
}

/// The muted cue for a clipped view scrolled `scroll` rows into `hidden` rows that don't fit:
/// which way the rest lies. Nothing when every row fits, so a view never claims more than it has.
pub fn more_cue(scroll: usize, hidden: usize) -> Option<&'static str> {
    (hidden > 0).then_some(if scroll >= hidden {
        "↑ more"
    } else if scroll > 0 {
        "↕ more"
    } else {
        "↓ more"
    })
}

/// Light paper or warm charcoal. The choice lasts for one run: every launch starts light, and
/// nothing reads the system appearance or saves the choice.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Theme {
    /// Dark ink on warm paper.
    #[default]
    Light,
    /// Warm light ink on charcoal, never pure black.
    Dark,
}

/// The four styles every screen draws with, one job each: ink for words, jade for the one thing to
/// act on, muted for what can wait, rust for consequences. With no colour each keeps its meaning
/// through weight, dimming or underline, so no cue depends on colour alone.
#[derive(Clone, Copy)]
pub struct Palette {
    ink: Style,
    jade: Style,
    muted: Style,
    /// Consequence only (`!`, loss prompts). Never the sole cue: always paired with `!` and words.
    rust: Style,
    /// The theme drawn, or none without colour, where the theme switch is hidden.
    theme: Option<Theme>,
}

impl Palette {
    /// The light palette for this terminal; `no_color` honours `NO_COLOR` and `--no-color`.
    pub fn new(no_color: bool) -> Self {
        Self::themed(no_color, Theme::Light)
    }

    /// The palette in `theme`. Without colour every theme is the same monochrome palette.
    fn themed(no_color: bool, theme: Theme) -> Self {
        if no_color {
            let base = Style::default();
            return Self {
                ink: base,
                jade: base.bold(),
                muted: base.add_modifier(Modifier::DIM),
                rust: base.bold().underlined(),
                theme: None,
            };
        }
        // Each colour keeps at least 4.5:1 contrast against its own background.
        let [ink, paper, jade, muted, rust] = match theme {
            Theme::Light => [
                (16, 15, 15),
                (255, 252, 240),
                (31, 122, 114),
                (87, 86, 83),
                (160, 62, 36),
            ],
            Theme::Dark => [
                (232, 224, 208),
                (38, 35, 32),
                (110, 196, 180),
                (166, 157, 142),
                (234, 138, 104),
            ],
        }
        .map(|(r, g, b)| Color::Rgb(r, g, b));
        let base = Style::default().fg(ink).bg(paper);
        Self {
            ink: base,
            jade: base.fg(jade).bold(),
            muted: base.fg(muted),
            rust: base.fg(rust).bold(),
            theme: Some(theme),
        }
    }
}

/// The header's light/dark switch: Ctrl-T, or a click on its `dark` or `light` label, flips the
/// theme at once on every screen. It never takes a key that types. Without colour it is hidden
/// and changes nothing.
#[derive(Clone, Copy)]
pub struct ThemeSwitch {
    theme: Theme,
    no_color: bool,
}

/// The widest switch label, so the switch keeps its place whichever way it points.
const SWITCH_WIDTH: u16 = 5;

impl ThemeSwitch {
    /// A switch starting light; `no_color` hides it.
    pub fn new(no_color: bool) -> Self {
        Self {
            theme: Theme::Light,
            no_color,
        }
    }

    /// Flips on a Ctrl-T press and reports whether it took the key. Ctrl-T always belongs to the
    /// switch, so no screen sees it; every other key passes through.
    pub fn key(&mut self, key: KeyEvent) -> bool {
        if key.code != KeyCode::Char('t') || key.modifiers != KeyModifiers::CONTROL {
            return false;
        }
        if key.kind == KeyEventKind::Press {
            self.flip();
        }
        true
    }

    /// Flips on a left click on the label in a terminal of size `area`, and reports whether it
    /// took the click. Without colour there is no label, so every click passes through.
    pub fn click(&mut self, event: MouseEvent, area: Rect) -> bool {
        let at = switch_area(area);
        let hit = !self.no_color
            && event.kind == MouseEventKind::Down(MouseButton::Left)
            && event.row == at.y
            && (at.x..at.right()).contains(&event.column);
        if hit {
            self.flip();
        }
        hit
    }

    /// The palette every screen draws with now.
    pub fn palette(&self) -> Palette {
        Palette::themed(self.no_color, self.theme)
    }

    fn flip(&mut self) {
        self.theme = match self.theme {
            Theme::Light => Theme::Dark,
            Theme::Dark => Theme::Light,
        };
    }
}

/// Where the switch label sits on header row 1, leaving two columns before the `?` help control.
pub(crate) fn switch_area(area: Rect) -> Rect {
    Rect::new(
        area.width.saturating_sub(5 + SWITCH_WIDTH),
        1,
        SWITCH_WIDTH,
        1,
    )
    .intersection(area)
}

/// Room on header row 1 for the model label, which ends two columns before the switch's place.
const LABEL_ROOM: u16 = 14;

/// Where the model label may sit on header row 1; its text is right-aligned inside. It keeps
/// its place without colour, so the hidden switch's place stays empty.
pub(crate) fn label_area(area: Rect) -> Rect {
    let end = switch_area(area).x.saturating_sub(2);
    Rect::new(end.saturating_sub(LABEL_ROOM), 1, LABEL_ROOM.min(end), 1).intersection(area)
}

/// Where header text before the model label must end: a column short of it, or `fallback`.
pub(crate) fn header_end(area: Rect, fallback: u16) -> u16 {
    fallback.min(label_area(area).x.saturating_sub(1))
}

/// Paints the switch label, naming the theme it switches to. Nothing without colour.
pub(crate) fn render_theme_switch(frame: &mut Frame, palette: Palette) {
    let label = match palette.theme {
        Some(Theme::Light) => "dark",
        Some(Theme::Dark) => "light",
        None => return,
    };
    frame.render_widget(
        Paragraph::new(label)
            .alignment(Alignment::Right)
            .style(palette.muted),
        switch_area(frame.area()),
    );
}

#[cfg(test)]
mod palette_tests {
    use super::*;

    fn luminance(colour: Option<Color>) -> f64 {
        let Some(Color::Rgb(r, g, b)) = colour else {
            panic!("every coloured role is RGB: {colour:?}")
        };
        let linear = |v: u8| {
            let v = f64::from(v) / 255.0;
            if v <= 0.03928 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b)
    }

    #[test]
    fn both_themes_keep_four_distinct_readable_roles_on_their_own_paper() {
        for theme in [Theme::Light, Theme::Dark] {
            let p = Palette::themed(false, theme);
            let roles = [p.ink, p.jade, p.muted, p.rust];
            let paper = luminance(p.ink.bg);
            for style in roles {
                assert_eq!(style.bg, p.ink.bg, "{theme:?}: one paper behind every role");
                let text = luminance(style.fg);
                let contrast = (text.max(paper) + 0.05) / (text.min(paper) + 0.05);
                assert!(contrast >= 4.5, "{theme:?} {:?}: {contrast:.2}", style.fg);
            }
            for (i, a) in roles.iter().enumerate() {
                for b in &roles[i + 1..] {
                    assert_ne!(a.fg, b.fg, "{theme:?}: each colour has one job");
                }
            }
            assert!(p.jade.add_modifier.contains(Modifier::BOLD), "{theme:?}");
            assert!(p.rust.add_modifier.contains(Modifier::BOLD), "{theme:?}");
        }
        assert_ne!(
            Palette::themed(false, Theme::Dark).ink.bg,
            Some(Color::Rgb(0, 0, 0)),
            "warm charcoal, never pure black"
        );
    }
    #[test]
    fn no_color_keeps_meaning_without_any_colour() {
        let mono = Palette::new(true);
        let mono_dark = Palette::themed(true, Theme::Dark);
        for (a, b) in [
            (mono.ink, mono_dark.ink),
            (mono.jade, mono_dark.jade),
            (mono.muted, mono_dark.muted),
            (mono.rust, mono_dark.rust),
        ] {
            assert_eq!(a, b, "without colour every theme is the same");
        }
        for style in [mono.ink, mono.jade, mono.muted, mono.rust] {
            assert_eq!((style.fg, style.bg), (None, None));
        }
        assert_eq!(mono.jade.add_modifier, Modifier::BOLD);
        assert_eq!(mono.muted.add_modifier, Modifier::DIM);
        assert_eq!(
            mono.rust.add_modifier,
            Modifier::BOLD | Modifier::UNDERLINED
        );
    }
}
