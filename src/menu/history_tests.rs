//! Saved-history navigation stays inside the selected goal and never copies under an overlay.

use super::*;
use crate::think_record::{self, Event, Origin, Session as RecordSession, Turn};
use std::{fs, time::SystemTime};

fn home() -> (tempfile::TempDir, Home, PathBuf, Vec<u8>) {
    let dir = tempfile::tempdir().unwrap();
    let sessions = dir.path().join("TEST-sessions");
    let session = sessions.join("same-name");
    let records = dir.path().join("TEST-records");
    fs::create_dir_all(&session).unwrap();
    let goal = "I keep this exact goal.\nWhy: remember.\nDone when: I can read it.";
    fs::write(session.join("ledger.json"), serde_json::json!({"schema_version":1, "status":"active", "session_id":"TEST-original-session", "goal":goal}).to_string()).unwrap();
    let path = think_record::write(
        &records,
        &RecordSession {
            origin: &Origin::simulated(),
            turns: &[Turn {
                at: SystemTime::UNIX_EPOCH,
                event: Event::Words {
                    source: 1,
                    text: "  My original Café 👩‍💻\nsecond line.  ".into(),
                },
            }],
            checks: vec![],
            goal,
            seed_me_session: &session,
            repo: None,
            confirmed: SystemTime::now(),
        },
    )
    .unwrap();
    let bytes = fs::read(&path).unwrap();
    let mut home = Home::new(
        Box::new(BrainDump::default),
        Ok(sessions),
        Ok(dir.path().join("TEST-links")),
    )
    .with_record_root(records);
    home.discover_for_snapshot(Duration::from_secs(3));
    home.open(Place::at(1).unwrap());
    (dir, home, path, bytes)
}

fn press(home: &mut Home, code: KeyCode) {
    home.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn rendered(home: &mut Home) -> String {
    snapshot(100, 30, home, true).unwrap()
}

fn wait_for_history(home: &mut Home) {
    let until = Instant::now() + Duration::from_secs(3);
    loop {
        home.tick();
        if rendered(home).contains("Original words 1") {
            return;
        }
        assert!(Instant::now() < until, "{}", rendered(home));
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn history_is_deliberate_and_help_copy_and_exit_keep_the_same_goal() {
    let (_dir, mut home, path, bytes) = home();
    press(&mut home, KeyCode::Char('r'));
    assert!(!rendered(&mut home).contains("Think history"));
    press(&mut home, KeyCode::Enter);
    let goal_view = rendered(&mut home);
    assert!(goal_view.contains("r history"));
    press(&mut home, KeyCode::F(1));
    press(&mut home, KeyCode::Char('r'));
    assert!(!rendered(&mut home).contains("Original words 1"));
    press(&mut home, KeyCode::Esc);
    let reads = think_record::count_reads(path.parent().unwrap());
    press(&mut home, KeyCode::Char('r'));
    wait_for_history(&mut home);
    let opened_reads = reads.totals();
    assert_eq!(opened_reads, (1, 1), "opening owns one record scan/read");
    for code in [
        KeyCode::Down,
        KeyCode::Up,
        KeyCode::PageDown,
        KeyCode::PageUp,
        KeyCode::End,
        KeyCode::Home,
    ] {
        press(&mut home, code);
        home.tick();
        rendered(&mut home);
    }
    snapshot(80, 24, &mut home, true).unwrap();
    snapshot(140, 42, &mut home, true).unwrap();
    let history = rendered(&mut home);
    assert!(history.contains("My original Café 👩‍💻") && !history.contains("/TEST"));
    press(&mut home, KeyCode::Char('c'));
    assert!(home.take_copy_request().is_none());
    press(&mut home, KeyCode::Char('l'));
    press(&mut home, KeyCode::Enter);
    assert_eq!(rendered(&mut home), history);
    let View::Tend(tend) = &home.view else {
        panic!("History must stay in Tend");
    };
    assert!(!tend.is_linking());
    press(&mut home, KeyCode::F(1));
    press(&mut home, KeyCode::Char('h'));
    assert!(rendered(&mut home).contains("How it works"));
    press(&mut home, KeyCode::Esc);
    assert!(rendered(&mut home).contains("Right now"));
    press(&mut home, KeyCode::Esc);
    assert_eq!(rendered(&mut home), history);
    assert_eq!(
        reads.totals(),
        opened_reads,
        "scrolling, help and resize re-read history"
    );
    press(&mut home, KeyCode::Esc);
    assert_eq!(rendered(&mut home), goal_view);
    assert_eq!(fs::read(path).unwrap(), bytes);
    press(&mut home, KeyCode::Char('r'));
    press(&mut home, KeyCode::F(10));
    home.tick();
    assert!(matches!(home.view, View::Menu));
    assert!(!home.quit);
}

#[test]
fn absent_record_still_leaves_the_saved_goal_and_a_way_back() {
    let (_dir, mut home, path, _) = home();
    fs::remove_file(path).unwrap();
    press(&mut home, KeyCode::Enter);
    let goal_view = rendered(&mut home);
    press(&mut home, KeyCode::Char('r'));
    let until = Instant::now() + Duration::from_secs(3);
    while !rendered(&mut home).contains("Think history unavailable") {
        home.tick();
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(1));
    }
    press(&mut home, KeyCode::Esc);
    assert_eq!(rendered(&mut home), goal_view);
}
