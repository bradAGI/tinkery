//! The light/dark switch as the operator sees it: one header label beside `?` on every screen,
//! Ctrl-T or a click flips every colour at once, and without colour nothing changes.

use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer, layout::Rect, style::Color};
use tinkery::{
    ThemeSwitch,
    menu::{self, Home, PLACES},
    think::brain_dump::BrainDump,
};

const LIGHT_PAPER: Color = Color::Rgb(255, 252, 240);
const LIGHT_INK: Color = Color::Rgb(16, 15, 15);
const CHARCOAL: Color = Color::Rgb(38, 35, 32);
const WARM_INK: Color = Color::Rgb(232, 224, 208);
const DARK_MUTED: Color = Color::Rgb(166, 157, 142);
const DARK_JADE: Color = Color::Rgb(110, 196, 180);
const DARK_RUST: Color = Color::Rgb(234, 138, 104);

fn home(root: &std::path::Path) -> Home {
    Home::new(
        Box::new(BrainDump::default),
        Ok(root.join("TEST-sessions")),
        Ok(root.join("TEST-links")),
    )
    .with_record_root(root.join("TEST-records"))
}

fn draw(home: &mut Home, switch: &ThemeSwitch, width: u16, height: u16) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| menu::render(frame, home, switch.palette()))
        .unwrap();
    terminal.backend().buffer().clone()
}

fn row(buffer: &Buffer, y: u16) -> String {
    (0..buffer.area.width)
        .map(|x| buffer[(x, y)].symbol())
        .collect::<String>()
        .trim_end()
        .to_string()
}

fn ctrl_t(kind: KeyEventKind) -> KeyEvent {
    KeyEvent::new_with_kind(KeyCode::Char('t'), KeyModifiers::CONTROL, kind)
}

fn click(column: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

/// The menu and every place, opened fresh.
fn every_screen(root: &std::path::Path) -> Vec<(String, Home)> {
    let mut screens = vec![("menu".to_string(), home(root))];
    for (index, (name, _)) in PLACES.iter().enumerate() {
        let mut place = home(root);
        place.open(tinkery::menu::Place::at(index).unwrap());
        screens.push((name.to_string(), place));
    }
    screens
}

#[test]
fn every_screen_names_the_other_theme_beside_help_and_paints_only_that_theme() {
    let tmp = tempfile::tempdir().unwrap();
    for (width, height) in [(80, 24), (100, 30)] {
        for (name, mut screen) in every_screen(tmp.path()) {
            let mut switch = ThemeSwitch::new(false);
            let light = draw(&mut screen, &switch, width, height);
            assert!(
                row(&light, 1).ends_with("dark  ?"),
                "{name}: {:?}",
                row(&light, 1)
            );
            assert!(switch.key(ctrl_t(KeyEventKind::Press)));
            let dark = draw(&mut screen, &switch, width, height);
            assert!(
                row(&dark, 1).ends_with("light  ?"),
                "{name}: {:?}",
                row(&dark, 1)
            );
            let header = row(&dark, 1);
            let title = header.trim_end_matches("light  ?").trim();
            assert!(
                !title.is_empty() && header.starts_with("  "),
                "{name}: {header:?}"
            );
            for cell in dark.content() {
                assert_ne!(cell.bg, LIGHT_PAPER, "{name} left light paper");
                assert_ne!(cell.fg, LIGHT_INK, "{name} left light ink");
            }
            assert_eq!(dark[(0, 0)].bg, CHARCOAL, "{name}");
            screen.handle_key(KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE));
            let help = draw(&mut screen, &switch, width, height);
            assert_ne!(help, dark, "{name}: help opened");
            for cell in help.content() {
                assert_ne!(cell.bg, LIGHT_PAPER, "{name} help left light paper");
                assert_ne!(cell.fg, LIGHT_INK, "{name} help left light ink");
            }
            let label = row(&dark, 1).rfind("light").unwrap() as u16;
            assert_eq!(
                dark[(label, 1)].fg,
                DARK_MUTED,
                "{name}: the label is quiet"
            );
        }
    }
}

#[test]
fn dark_keeps_jade_for_where_you_are_and_rust_for_consequence() {
    let tmp = tempfile::tempdir().unwrap();
    let mut switch = ThemeSwitch::new(false);
    switch.key(ctrl_t(KeyEventKind::Press));
    let mut menu = home(tmp.path());
    let buffer = draw(&mut menu, &switch, 80, 24);
    let think = (0..24)
        .find(|y| row(&buffer, *y).contains("1  think"))
        .unwrap();
    assert_eq!(
        buffer[(2, think)].fg,
        DARK_JADE,
        "the selected place is jade"
    );

    let mut think = home(tmp.path());
    think.open(tinkery::menu::Place::at(0).unwrap());
    draw(&mut think, &switch, 80, 24);
    think.paste("TEST words I would lose");
    think.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    let buffer = draw(&mut think, &switch, 80, 24);
    let prompt = (0..24)
        .find(|y| row(&buffer, *y).contains("! Back to menu and lose this draft?"))
        .expect("the loss prompt shows");
    let bang = (0..80)
        .find(|x| buffer[(*x, prompt)].symbol() == "!")
        .unwrap();
    assert_eq!(
        buffer[(bang, prompt)].fg,
        DARK_RUST,
        "the loss mark is rust"
    );
    assert_eq!(
        buffer[(bang + 2, prompt)].fg,
        WARM_INK,
        "its words stay ink"
    );
}

#[test]
fn ctrl_t_is_the_only_key_the_switch_takes_and_repeats_never_flicker() {
    let tmp = tempfile::tempdir().unwrap();
    let mut screen = home(tmp.path());
    let mut switch = ThemeSwitch::new(false);
    for passes in [
        KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE),
        KeyEvent::new(KeyCode::Char('T'), KeyModifiers::SHIFT),
        KeyEvent::new(
            KeyCode::Char('t'),
            KeyModifiers::CONTROL | KeyModifiers::ALT,
        ),
        KeyEvent::new(KeyCode::Char('l'), KeyModifiers::CONTROL),
        KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE),
    ] {
        assert!(!switch.key(passes), "{passes:?} must reach the screen");
    }
    for held in [KeyEventKind::Repeat, KeyEventKind::Release] {
        assert!(switch.key(ctrl_t(held)), "no screen sees a held Ctrl-T");
    }
    assert!(row(&draw(&mut screen, &switch, 80, 24), 1).ends_with("dark  ?"));
    switch.key(ctrl_t(KeyEventKind::Press));
    switch.key(ctrl_t(KeyEventKind::Press));
    assert!(row(&draw(&mut screen, &switch, 80, 24), 1).ends_with("dark  ?"));
}

#[test]
fn a_click_on_the_label_flips_and_clicks_elsewhere_pass_through() {
    let tmp = tempfile::tempdir().unwrap();
    let mut screen = home(tmp.path());
    let area = Rect::new(0, 0, 80, 24);
    let mut switch = ThemeSwitch::new(false);
    let header = row(&draw(&mut screen, &switch, 80, 24), 1);
    let d = header.find("dark").unwrap() as u16;
    let help = header.rfind('?').unwrap() as u16;
    for elsewhere in [click(help, 1), click(d, 0), click(d, 2), click(d - 2, 1)] {
        assert!(!switch.click(elsewhere, area));
    }
    let release = MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        ..click(d, 1)
    };
    assert!(!switch.click(release, area));
    assert!(switch.click(click(d + 3, 1), area));
    let header = row(&draw(&mut screen, &switch, 80, 24), 1);
    assert!(header.ends_with("light  ?"), "{header:?}");
    let l = header.find("light").unwrap() as u16;
    assert!(switch.click(click(l, 1), area));
    assert!(row(&draw(&mut screen, &switch, 80, 24), 1).ends_with("dark  ?"));
}

#[test]
fn without_colour_every_screen_stays_monochrome_and_the_switch_changes_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    for (name, mut screen) in every_screen(tmp.path()) {
        let mut switch = ThemeSwitch::new(true);
        let before = draw(&mut screen, &switch, 80, 24);
        assert!(!row(&before, 1).contains("dark"), "{name}");
        assert_eq!(
            before,
            {
                let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
                terminal
                    .draw(|frame| menu::render(frame, &mut screen, tinkery::Palette::new(true)))
                    .unwrap();
                terminal.backend().buffer().clone()
            },
            "{name}"
        );
        assert!(switch.key(ctrl_t(KeyEventKind::Press)));
        let header = row(&before, 1);
        let anywhere = (0..80).map(|x| click(x, 1));
        assert!(
            anywhere
                .into_iter()
                .all(|c| !switch.click(c, Rect::new(0, 0, 80, 24))),
            "{name}: no label to click in {header:?}"
        );
        let after = draw(&mut screen, &switch, 80, 24);
        assert_eq!(before, after, "{name}");
        for cell in after.content() {
            assert_eq!((cell.fg, cell.bg), (Color::Reset, Color::Reset), "{name}");
        }
    }
}
