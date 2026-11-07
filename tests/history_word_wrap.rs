//! Saved-history wrapping through public Home rendering and navigation; authored TEST records only.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer, style::Color};
use std::{
    fs,
    time::{Duration, Instant},
};
use tinkery::{
    Palette,
    menu::{Home, render},
};

const GOAL: &str = "TEST confirmed goal stays exact.";
const WATER: &str = "I want an offline checklist for watering my three balcony pots. Let me name each pot, tick it when watered today, and clear the ticks the next morning. I do not want reminders, accounts, a network connection, or plant advice.";

struct Fixture {
    dir: tempfile::TempDir,
    home: Home,
    ledger: std::path::PathBuf,
    record: std::path::PathBuf,
    bytes: (Vec<u8>, Vec<u8>),
}
fn key(home: &mut Home, code: KeyCode) {
    home.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
}
fn paint(home: &mut Home, size: (u16, u16)) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(size.0, size.1)).unwrap();
    terminal
        .draw(|frame| render(frame, home, Palette::new(false)))
        .unwrap();
    terminal.backend().buffer().clone()
}
fn rows(buffer: &Buffer) -> Vec<String> {
    (5..buffer.area.height - 1)
        .map(|y| {
            (2..buffer.area.width - 2)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                .trim_end_matches(' ')
                .to_owned()
        })
        .collect()
}
fn fixture(original: &str, size: (u16, u16)) -> Fixture {
    let dir = tempfile::Builder::new()
        .prefix("tinkery-TEST-540-")
        .tempdir()
        .unwrap();
    let session = dir.path().join("sessions/TEST-goal");
    let records = dir.path().join("records");
    fs::create_dir_all(&session).unwrap();
    let session = session.canonicalize().unwrap();
    fs::create_dir(&records).unwrap();
    let ledger = session.join("ledger.json");
    let record = records.join("TEST-history.json");
    fs::write(&ledger, serde_json::json!({"schema_version":1,"session_id":"TEST-id","status":"active","goal":GOAL}).to_string()).unwrap();
    fs::write(
        &record,
        serde_json::json!({
            "schema":3,"tinkery":"TEST","origin":"simulated","harness":null,"model":null,
            "thinking":null,"prompt_hash":null,"started":"2026-10-28T12:00:00Z",
            "confirmed":"2026-10-28T12:00:00Z","seed_me_session":"TEST-goal",
            "seed_me_session_path":session,"seed_me_session_id":"TEST-id","repo":null,
            "turns":[{"at":"2026-10-28T12:00:00Z","kind":"words","source":1,"text":original}],
            "checks":[],"goal":GOAL
        })
        .to_string(),
    )
    .unwrap();
    let bytes = (fs::read(&ledger).unwrap(), fs::read(&record).unwrap());
    let mut home = Home::new(
        Box::new(Default::default),
        Ok(session.parent().unwrap().to_owned()),
        Ok(dir.path().join("links")),
    )
    .with_record_root(records);
    home.open(tinkery::menu::Place::Tend);
    let deadline = Instant::now() + Duration::from_secs(5);
    while home.count(tinkery::menu::Place::Tend).is_none() {
        home.tick();
        assert!(
            Instant::now() < deadline,
            "session discovery did not settle"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    key(&mut home, KeyCode::Enter);
    key(&mut home, KeyCode::Char('r'));
    loop {
        home.tick();
        if rows(&paint(&mut home, size))
            .iter()
            .any(|row| row == "Original words 1")
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "history did not settle: {:?}",
            rows(&paint(&mut home, size))
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    Fixture {
        dir,
        home,
        ledger,
        record,
        bytes,
    }
}
impl Fixture {
    fn unchanged(&self) {
        assert_eq!(fs::read(&self.ledger).unwrap(), self.bytes.0);
        assert_eq!(fs::read(&self.record).unwrap(), self.bytes.1);
        assert!(!self.dir.path().join("links").exists());
    }
    fn visible(&mut self, size: (u16, u16)) -> Vec<String> {
        let mut all = rows(&paint(&mut self.home, size));
        for _ in 0..100 {
            if all.iter().any(|row| row == GOAL) {
                return all;
            }
            key(&mut self.home, KeyCode::Down);
            all.push(rows(&paint(&mut self.home, size)).pop().unwrap());
        }
        panic!("history never reached the unchanged confirmed goal");
    }
}

#[test]
fn ordinary_words_stay_whole_while_explicit_breaks_spaces_and_long_tokens_survive() {
    let repair = include_str!("fixtures/history-repair-original.txt");
    for size in [(80, 24), (100, 30)] {
        for (source, words) in [
            (WATER, vec!["when", "network", "reminders"]),
            (
                repair,
                vec!["appliance", "tried", "reminders", "landlord", "relevant"],
            ),
        ] {
            let mut f = fixture(source, size);
            let rows = f.visible(size);
            for word in words {
                assert!(
                    rows.iter().any(|row| row.contains(word)),
                    "{word} was split at {size:?}: {rows:?}"
                );
            }
            f.unchanged();
        }
        let hard = "Explicit source breaks: wh\nen and net\nwork.\n\n  Preserve this indentation.\n  Keep   these  spaces.";
        let mut f = fixture(hard, size);
        let rows = f.visible(size);
        let start = rows
            .iter()
            .position(|row| row == "Explicit source breaks: wh")
            .unwrap();
        assert_eq!(
            &rows[start..start + 6],
            &hard.split('\n').collect::<Vec<_>>()
        );
        f.unchanged();
        let token = format!("LONGTOKEN_{}", "0123456789".repeat(14));
        let mut f = fixture(&token, size);
        let rows = f.visible(size);
        let start = rows
            .iter()
            .position(|row| row.starts_with("LONGTOKEN_"))
            .unwrap();
        let shown = rows[start..]
            .iter()
            .take_while(|row| !row.is_empty())
            .cloned()
            .collect::<String>();
        assert_eq!(shown, token);
        f.unchanged();
    }
}

#[test]
fn unicode_clusters_remain_whole_in_public_history_cells() {
    let original = "Unicode: café e\u{301} 界 👩‍💻. ".repeat(12);
    for size in [(80, 24), (100, 30)] {
        let mut f = fixture(&original, size);
        let buffer = paint(&mut f.home, size);
        for cluster in ["e\u{301}", "👩‍💻"] {
            assert!(
                buffer.content.iter().any(|cell| cell.symbol() == cluster),
                "missing whole {cluster}"
            );
        }
        for cell in &buffer.content {
            if cell.symbol().contains('\u{301}') || cell.symbol().contains('\u{200d}') {
                assert!(["e\u{301}", "👩‍💻"].contains(&cell.symbol()));
                assert_eq!(cell.fg, Color::Rgb(16, 15, 15));
            }
        }
        f.unchanged();
    }
}

#[test]
fn public_navigation_reaches_history_after_65535_rows_and_survives_resize_and_help() {
    let original = format!(
        "START\n{}TAIL BEYOND SIXTY FIVE THOUSAND ROWS",
        "row\n".repeat(70_000)
    );
    let mut f = fixture(&original, (80, 24));
    let first = paint(&mut f.home, (80, 24));
    key(&mut f.home, KeyCode::End);
    let end = paint(&mut f.home, (80, 24));
    assert!(
        rows(&end)
            .iter()
            .any(|row| row == "TAIL BEYOND SIXTY FIVE THOUSAND ROWS")
    );
    key(&mut f.home, KeyCode::PageUp);
    assert_ne!(end, paint(&mut f.home, (80, 24)));
    key(&mut f.home, KeyCode::PageDown);
    assert_eq!(end, paint(&mut f.home, (80, 24)));
    assert!(
        rows(&paint(&mut f.home, (100, 30)))
            .iter()
            .any(|row| row == GOAL)
    );
    key(&mut f.home, KeyCode::F(1));
    assert!(
        rows(&paint(&mut f.home, (100, 30)))
            .iter()
            .any(|row| row.contains("Read how this goal took shape."))
    );
    key(&mut f.home, KeyCode::Esc);
    assert!(
        rows(&paint(&mut f.home, (100, 30)))
            .iter()
            .any(|row| row == GOAL)
    );
    key(&mut f.home, KeyCode::Home);
    assert_eq!(first, paint(&mut f.home, (80, 24)));
    f.unchanged();
}
