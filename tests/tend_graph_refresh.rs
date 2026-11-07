//! Graph updates through public discovery, navigation and needs-link actions.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tinkery::{
    Palette,
    menu::{Home, render},
    tend::{Found, Session, Stage, needs::Store},
};

fn key(home: &mut Home, code: KeyCode) {
    home.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
}
fn refresh(home: &mut Home) {
    home.discover_for_snapshot(Duration::from_secs(5));
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        home.tick();
        if home.count(1).is_some() {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "discovery did not settle"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn paint(home: &mut Home) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    terminal
        .draw(|frame| render(frame, home, Palette::new(true)))
        .unwrap();
    terminal.backend().buffer().clone()
}
fn text(buffer: &Buffer) -> String {
    buffer.content.iter().map(|cell| cell.symbol()).collect()
}
fn graph(buffer: &Buffer) -> &[ratatui::buffer::Cell] {
    &buffer.content[300..2400]
}

#[test]
fn public_refresh_updates_labels_and_links_but_keeps_an_unchanged_view() {
    let dir = tempfile::Builder::new()
        .prefix("tinkery-TEST-520-")
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
    let source = found.clone();
    let links = dir.path().join("TEST-links");
    let mut home = Home::new(
        Box::new(Default::default),
        Ok(dir.path().join("sessions")),
        Ok(links.clone()),
    )
    .with_record_root(dir.path().join("records"))
    .with_discovery_source(Arc::new(move |_| Ok(source.lock().unwrap().clone())));
    home.open(tinkery::menu::Place::at(1).unwrap());
    refresh(&mut home);
    let initial = paint(&mut home);
    assert!(text(&initial).contains("Goal 0"));
    for _ in 0..3 {
        assert_eq!(initial, paint(&mut home));
    }
    key(&mut home, KeyCode::Char('+'));
    key(&mut home, KeyCode::Char('H'));
    let moved = paint(&mut home);
    assert_ne!(initial, moved);
    refresh(&mut home);
    assert_eq!(
        moved,
        paint(&mut home),
        "unchanged discovery must preserve the viewport"
    );

    found.lock().unwrap().sessions[0].goal = "Temporary label".into();
    refresh(&mut home);
    found.lock().unwrap().sessions[0].goal = "Goal 0".into();
    refresh(&mut home);
    assert_eq!(
        moved,
        paint(&mut home),
        "changes undone before paint preserve positions"
    );

    found.lock().unwrap().sessions[0].goal = "Revised goal".into();
    refresh(&mut home);
    assert!(text(&paint(&mut home)).contains("Revised goal"));
    // Restore the initial viewport through the opposite public controls.
    key(&mut home, KeyCode::Char('-'));
    key(&mut home, KeyCode::Char('L'));
    let unlinked = paint(&mut home);
    key(&mut home, KeyCode::Char('l'));
    key(&mut home, KeyCode::Down);
    key(&mut home, KeyCode::Enter);
    key(&mut home, KeyCode::Up);
    let linked = paint(&mut home);
    assert!(
        Store::new(Ok(links.clone()))
            .read()
            .into_links()
            .unwrap()
            .contains("id-0", "id-1")
    );
    assert_ne!(
        graph(&unlinked),
        graph(&linked),
        "new needs line must appear"
    );
    refresh(&mut home);
    assert_eq!(linked, paint(&mut home));

    // A duplicate outside the linked endpoints still makes that ID ambiguous globally.
    found.lock().unwrap().sessions[2].id = Some("id-1".into());
    refresh(&mut home);
    let ambiguous = paint(&mut home);
    assert_eq!(
        graph(&unlinked),
        graph(&ambiguous),
        "ambiguous ID must remove its line"
    );
    found.lock().unwrap().sessions[2].id = Some("id-2".into());
    refresh(&mut home);
    assert_eq!(graph(&linked), graph(&paint(&mut home)));

    // Starting the link picker rereads external needs edits even without new discovery.
    Store::new(Ok(links)).set("id-0", "id-1", false).unwrap();
    key(&mut home, KeyCode::Char('l'));
    key(&mut home, KeyCode::Esc);
    assert_eq!(graph(&unlinked), graph(&paint(&mut home)));
}
