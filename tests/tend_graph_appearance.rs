//! Populated Tend gardens update cached colors through public theme and navigation controls.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tinkery::{
    ThemeSwitch,
    menu::{Home, render},
    tend::{Found, Session, Stage},
};

fn key(home: &mut Home, code: KeyCode) {
    home.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
}
fn settle(home: &mut Home) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        home.tick();
        if home.count(1).is_some() {
            break;
        }
        assert!(Instant::now() < deadline, "discovery did not settle");
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn paint(home: &mut Home, theme: &ThemeSwitch, width: u16, height: u16) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| render(frame, home, theme.palette()))
        .unwrap();
    terminal.backend().buffer().clone()
}
fn toggle(theme: &mut ThemeSwitch) {
    theme.key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL));
}

#[test]
fn warmed_garden_matches_fresh_colors_after_theme_source_and_graph_changes() {
    let dir = tempfile::Builder::new()
        .prefix("tinkery-TEST-526-")
        .tempdir()
        .unwrap();
    let found = Arc::new(Mutex::new(Found {
        sessions: (0..3)
            .map(|i| Session {
                id: Some(format!("id-{i}")),
                path: dir.path().join(format!("goal-{i}")),
                goal: format!("Goal {i}"),
                stage: Stage::GoalSaved,
                updated: String::new(),
            })
            .collect(),
        ..Found::default()
    }));
    let home = || {
        let source = found.clone();
        let mut home = Home::new(
            Box::new(Default::default),
            Ok(dir.path().join("sessions")),
            Ok(dir.path().join("links")),
        )
        .with_record_root(dir.path().join("records"))
        .with_discovery_source(Arc::new(move |_| Ok(source.lock().unwrap().clone())));
        home.open(tinkery::menu::Place::at(1).unwrap());
        settle(&mut home);
        home
    };
    for (width, height) in [(80, 24), (100, 30)] {
        for mono in [false, true] {
            let mut actual = home();
            let mut theme = ThemeSwitch::new(mono);
            let initial = paint(&mut actual, &theme, width, height);
            for _ in 0..3 {
                assert_eq!(initial, paint(&mut actual, &theme, width, height));
            }
            toggle(&mut theme);
            let dark = paint(&mut actual, &theme, width, height);
            assert_eq!(
                dark,
                paint(&mut home(), &theme, width, height),
                "theme must recolor cached rings"
            );
            toggle(&mut theme);
            assert_eq!(initial, paint(&mut actual, &theme, width, height));

            key(&mut actual, KeyCode::Down);
            let mut reference = home();
            key(&mut reference, KeyCode::Down);
            assert_eq!(
                paint(&mut actual, &theme, width, height),
                paint(&mut reference, &theme, width, height),
                "selection must clear the old source color"
            );
            key(&mut actual, KeyCode::Char('l'));
            key(&mut actual, KeyCode::Down);
            key(&mut reference, KeyCode::Char('l'));
            key(&mut reference, KeyCode::Down);
            let picking = paint(&mut actual, &theme, width, height);
            assert_eq!(
                picking,
                paint(&mut reference, &theme, width, height),
                "target movement keeps the link source marked"
            );
            key(&mut actual, KeyCode::Esc);
            key(&mut actual, KeyCode::Down);
            let mut reference = home();
            key(&mut reference, KeyCode::Down);
            key(&mut reference, KeyCode::Down);
            assert_eq!(
                paint(&mut actual, &theme, width, height),
                paint(&mut reference, &theme, width, height)
            );

            // New graph data needs both metadata rebuilding and the current source highlight.
            found.lock().unwrap().sessions[0].goal = "Changed goal".into();
            actual.discover_for_snapshot(Duration::from_secs(5));
            settle(&mut actual);
            let mut reference = home();
            key(&mut reference, KeyCode::Down);
            key(&mut reference, KeyCode::Down);
            assert_eq!(
                paint(&mut actual, &theme, width, height),
                paint(&mut reference, &theme, width, height),
                "new graph must retain the marked source color"
            );
            found.lock().unwrap().sessions[0].goal = "Goal 0".into();
        }
    }
}
