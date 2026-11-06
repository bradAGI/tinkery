//! Shape reading limits use public input, rendered text and real TEST ledger files.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend, layout::Rect};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    thread,
    time::{Duration, Instant},
};
use tinkery::{
    Palette,
    pi::CallBudget,
    seed_me::ledger::{self, Value},
    shape::{Agent, Shape},
};

const STAMP: &str = "2026-10-25T00:00:00+00:00";

fn fixture(goal: &str, body: &str) -> (tempfile::TempDir, PathBuf) {
    let root = tempfile::Builder::new()
        .prefix("tinkery-TEST-large-seed-")
        .tempdir()
        .unwrap();
    let session = ledger::initialize(root.path(), STAMP).unwrap();
    let original = ledger::load(&session).unwrap();
    let mut state = ledger::editable(&original).unwrap().to_json().unwrap();
    state["goal"] = goal.into();
    state["origin"] = "goal".into();
    state["nodes"] = serde_json::json!([{
        "id":"goal", "kind":"decision", "status":"settled", "prerequisites":[], "evidence":[],
        "owner":"TEST", "gate":"TEST", "answer":goal, "authority":"user", "authority_source":"TEST fixture"
    }]);
    let current = ledger::publish(
        &session,
        &Value::from(state),
        &original["version"],
        &Value::from("TEST fixture"),
        None,
        STAMP,
    )
    .unwrap();
    let checks = Value::from(
        serde_json::json!([{"statement":"TEST keeps originals", "kind":"judge", "source":"TEST fixture"}]),
    );
    ledger::set_checks(&session, &checks).unwrap();
    let seed = format!(
        "status: draft\n# Seed: TEST large\nrevision: r1\n## You are confirming\nTEST keeps originals\n\n## Now  (the agent keeps these current)\n{body}\n\n## Goal\nTEST goal\n\n{}\n## Constraints and exclusions\nTEST only\n\n{}## Open questions\nNone\n\n## Knowledge map\nTEST only\n\n## Evidence\nTEST seed final tail 中 e\u{301} 👩‍💻\n",
        ledger::decisions_block(&current).unwrap(),
        ledger::checks_block(&checks).unwrap()
    );
    fs::write(session.join("seed-contract.md"), seed).unwrap();
    (root, session)
}

fn files(session: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fs::read_dir(session)
        .unwrap()
        .map(|entry| {
            let path = entry.unwrap().path();
            let bytes = fs::read(&path).unwrap();
            (path, bytes)
        })
        .collect()
}

fn key(shape: &mut Shape, code: KeyCode) {
    shape.key(KeyEvent::new(code, KeyModifiers::NONE));
}

/// Picks the F1 row whose words contain `label`, by its number, as the operator does.
fn choose(shape: &mut Shape, label: &str) {
    let help = shape.help_text();
    let digit = help
        .lines()
        .find_map(|line| {
            let (n, words) = line.split_once("  ")?;
            (n.len() == 1 && words.contains(label)).then(|| n.chars().next().unwrap())
        })
        .unwrap_or_else(|| panic!("F1 offers no {label:?}:\n{help}"));
    assert!(shape.pick(KeyEvent::new(KeyCode::Char(digit), KeyModifiers::NONE)));
}

fn screen(shape: &mut Shape, width: u16, height: u16, body_height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| {
            shape.render(
                frame,
                Rect::new(0, 0, width, body_height),
                Palette::new(true),
            )
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn opened(session: &Path) -> Shape {
    let agent = Agent::new(
        tinkery::pi::Harness::Pi,
        PathBuf::from("/tinkery-TEST-no-provider"),
        "TEST/fake".into(),
        String::new(),
        Some(session.into()),
        CallBudget::new(0),
        None,
    )
    .unwrap();
    let mut shape = Shape::session(session).with_agent(Some(Arc::new(agent)));
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        shape.tick();
        let shown = screen(&mut shape, 80, 24, 20);
        if shown.contains("Seed draft r1") || shown.contains("Too long to show") {
            break;
        }
        assert!(Instant::now() < deadline, "TEST seed never loaded");
        thread::sleep(Duration::from_millis(1));
    }
    shape
}

#[test]
fn oversized_readings_are_refused_and_f2_changes_nothing() {
    for source in ["seed", "review", "so far"] {
        let goal = if source == "so far" {
            "g".repeat(60_000)
        } else {
            "TEST goal".into()
        };
        let body = "x".repeat(if source == "seed" { 60_000 } else { 30_000 });
        let (_root, session) = fixture(&goal, &body);
        if source == "review" {
            let seed = fs::read_to_string(session.join("seed-contract.md")).unwrap();
            let version = ledger::load(&session).unwrap()["version"]
                .to_json()
                .unwrap()
                .as_u64()
                .unwrap();
            let review = ledger::CompletionReview {
                contradictions: vec![],
                decided: vec!["r".repeat(30_000)],
                done: vec![],
                failure_cases: vec![ledger::FailureCase {
                    statement: "TEST keeps originals".into(),
                    case: "TEST changed originals".into(),
                }],
            };
            ledger::record_teach_back(
                &session,
                version,
                "r1",
                &seed,
                &ledger::TeachBackRecord::Review(&review),
                STAMP,
                None,
            )
            .unwrap();
        }
        let before = files(&session);
        let mut shape = opened(&session);
        shape.paste("TEST confirm only a readable seed");
        if source == "so far" {
            choose(&mut shape, "So far");
        }
        for _ in 0..4_000 {
            key(&mut shape, KeyCode::PageDown);
        }
        let shown = screen(&mut shape, 80, 24, 20);
        assert!(
            shown.contains("Too long to show. Nothing was confirmed or changed."),
            "{source}: {shown}"
        );
        assert!(
            !shown.contains("End of seed") && !shown.contains("F2 confirm"),
            "{shown}"
        );
        key(&mut shape, KeyCode::F(2));
        assert!(!screen(&mut shape, 80, 24, 20).contains("esc stop"));
        for _ in 0..10 {
            shape.tick();
        }
        assert_eq!(files(&session), before);
        assert!(!session.join("seed-confirmation.json").exists());
    }
}

#[test]
fn a_zero_height_reading_never_authorizes_confirmation() {
    let (_root, session) = fixture("TEST exact goal", &"TEST small seed\n".repeat(100));
    let before = files(&session);
    let mut shape = opened(&session);
    shape.paste("TEST confirm only displayed words");
    screen(&mut shape, 80, 24, 1);
    for _ in 0..1_000 {
        key(&mut shape, KeyCode::PageDown);
    }
    assert!(screen(&mut shape, 80, 24, 1).contains("read to end"));
    key(&mut shape, KeyCode::F(2));
    assert!(!screen(&mut shape, 80, 24, 1).contains("esc stop"));
    for _ in 0..1_000 {
        key(&mut shape, KeyCode::PageUp);
    }
    assert!(screen(&mut shape, 80, 24, 20).contains("Nothing was confirmed"));
    for _ in 0..10 {
        shape.tick();
    }
    assert_eq!(files(&session), before);
    assert!(!session.join("seed-confirmation.json").exists());
}

#[test]
fn public_shape_status_preserves_seed_and_confirmation_states() {
    use tinkery::seed_me::shaping::{self, Shaped};
    let (_root, session) = fixture("TEST exact goal", "TEST small body");
    let assert_read_only = |session: &Path| {
        let before = files(session);
        let view = shaping::read(session).unwrap();
        let status = ledger::status(session).unwrap();
        assert_eq!(view.version, status["version"].to_json().unwrap().as_u64());
        assert_eq!(view.goal.as_deref(), Some("TEST exact goal"));
        assert_eq!(files(session), before);
        (view, status)
    };
    let (view, _) = assert_read_only(&session);
    assert!(matches!(view.stage, Shaped::Drafted { revision: Some(ref r), .. } if r == "r1"));
    ledger::confirm_seed(&session, "r1", "TEST explicit confirmation", STAMP).unwrap();
    let (view, status) = assert_read_only(&session);
    assert!(matches!(view.stage, Shaped::Ready { ended: false, .. }));
    assert_eq!(status["handoff"]["verified"].to_json().unwrap(), true);
    let seed = session.join("seed-contract.md");
    let confirmed = fs::read(&seed).unwrap();
    let mut changed = confirmed.clone();
    changed.extend_from_slice(b"\nTEST changed bytes\n");
    fs::write(&seed, changed).unwrap();
    let (view, status) = assert_read_only(&session);
    assert!(matches!(view.stage, Shaped::Unverified { .. }));
    assert_eq!(status["handoff"]["verified"].to_json().unwrap(), false);
    fs::write(&seed, &confirmed).unwrap();
    let record = session.join("seed-confirmation.json");
    let confirmation = fs::read(&record).unwrap();
    fs::remove_file(&record).unwrap();
    let (view, status) = assert_read_only(&session);
    assert!(matches!(view.stage, Shaped::Unverified { .. }));
    assert_eq!(status["handoff"]["verified"].to_json().unwrap(), false);
    fs::write(&record, &confirmation).unwrap();

    fs::remove_file(&seed).unwrap();
    fs::create_dir(&seed).unwrap();
    let ledger_bytes = fs::read(session.join("ledger.json")).unwrap();
    assert!(!matches!(
        shaping::read(&session).unwrap().stage,
        Shaped::Ready { .. }
    ));
    assert_eq!(
        ledger::status(&session).unwrap()["handoff"]["verified"]
            .to_json()
            .unwrap(),
        false
    );
    assert!(seed.is_dir());
    assert_eq!(fs::read(session.join("ledger.json")).unwrap(), ledger_bytes);
    assert_eq!(fs::read(&record).unwrap(), confirmation);
    fs::remove_dir(&seed).unwrap();
    fs::write(&seed, confirmed).unwrap();
    ledger::end(&session, "completed", "TEST completed", STAMP).unwrap();
    assert!(matches!(
        assert_read_only(&session).0.stage,
        Shaped::Ready { ended: true, .. }
    ));

    let (_other, session) = fixture("TEST exact goal", "TEST body");
    fs::remove_file(session.join("seed-contract.md")).unwrap();
    let current = ledger::load(&session).unwrap()["version"]
        .to_json()
        .unwrap()
        .as_u64()
        .unwrap();
    let version = tinkery::seed_me::answering::probe(&session, current)
        .unwrap()
        .unwrap();
    let (view, status) = assert_read_only(&session);
    assert_eq!(view.version, Some(version));
    assert!(
        matches!(view.stage, Shaped::Question { ref id, .. } if id == tinkery::seed_me::answering::SAMPLE.0)
    );
    assert_eq!(
        status["current_question"].string(),
        Some(tinkery::seed_me::answering::SAMPLE.0)
    );
}
