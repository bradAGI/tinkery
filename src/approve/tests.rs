use super::*;
use ratatui::{Terminal, backend::TestBackend};

fn seed(goal: &str) -> Reading {
    Reading {
        seed: ReadySeed {
            target: None,
            goal: goal.into(),
            revision: "r1".into(),
            acceptance: "## Acceptance criteria\n- TEST exact check  \n".into(),
            commands: Some("python3 _system/scripts/sdlc.py new <run>\nTEST intake".into()),
            confirmation_note: None,
        },
        origin_note: None,
    }
}

fn view() -> Approve {
    let mut view = Approve::open(None);
    view.waiting = false;
    view.found = Some(Ok(Found {
        seeds: vec![seed("TEST oldest"), seed("TEST newest")],
        ..Found::default()
    }));
    view
}

fn screen(view: &mut Approve, width: u16, height: u16, help: bool) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|f| view.render(f, f.area(), Palette::new(true), help))
        .unwrap();
    terminal
        .backend()
        .buffer()
        .content
        .chunks(width as usize)
        .map(|row| row.iter().map(|c| c.symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn one_seed_at_a_time_and_copy_never_runs_a_command() {
    let mut view = view();
    let mut help = false;
    assert!(view.take_copy_request().is_none());
    let shown = screen(&mut view, 80, 24, false);
    assert!(shown.contains("TEST oldest") && !shown.contains("TEST newest"));
    view.key(KeyEvent::from(KeyCode::Char('c')), &mut help);
    assert_eq!(
        view.take_copy_request().unwrap(),
        view.seeds()[0].seed.commands.clone().unwrap()
    );
    assert!(view.take_copy_request().is_none());
    view.copy_result(true);
    for (width, height) in [(80, 24), (100, 30)] {
        let shown = screen(&mut view, width, height, false);
        assert!(shown.contains("Copied. Run it in a terminal; after it succeeds, open Watch."));
        assert!(shown.contains("c copy command") && shown.contains("esc back"));
    }
    assert!(view.take_copy_request().is_none());
    view.copy_result(false);
    assert!(screen(&mut view, 80, 24, false).contains("Couldn't copy; try again."));
    view.key(KeyEvent::from(KeyCode::Char('n')), &mut help);
    let shown = screen(&mut view, 80, 24, false);
    assert!(shown.contains("TEST newest"));
    assert!(!shown.contains("Copied.") && !shown.contains("Couldn't copy"));
    view.key(KeyEvent::from(KeyCode::F(1)), &mut help);
    view.key(KeyEvent::from(KeyCode::Char('c')), &mut help);
    view.key(KeyEvent::from(KeyCode::Char('p')), &mut help);
    assert!(view.take_copy_request().is_none());
    assert_eq!(view.selected, 1);
    assert!(!view.key(KeyEvent::from(KeyCode::Esc), &mut help));
    assert!(!help);
    assert!(view.key(KeyEvent::from(KeyCode::Esc), &mut help));
}

#[test]
fn agent_confirmation_notice_keeps_copy_and_missing_checkout_guidance() {
    for (width, height) in [(80, 24), (100, 30)] {
        let mut view = view();
        let Some(Ok(found)) = &mut view.found else {
            panic!()
        };
        found.seeds[0].seed.confirmation_note =
            Some("Agent-confirmed seed. Intake requires tink-sdlc 1.38.0 or newer.".into());
        let shown = screen(&mut view, width, height, false);
        assert!(shown.contains("Agent-confirmed seed."));
        assert!(
            shown
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .contains("Copying runs nothing.")
        );
        assert!(shown.contains("c copy command"));
        view.key(KeyEvent::from(KeyCode::Char('c')), &mut false);
        assert_eq!(view.take_copy_request(), view.seeds()[0].seed.commands);
        let Some(Ok(found)) = &mut view.found else {
            panic!()
        };
        found.seeds[0].seed.commands = None;
        view.key(KeyEvent::from(KeyCode::End), &mut false);
        let shown = screen(&mut view, width, height, false);
        assert!(shown.contains("Agent-confirmed seed."));
        assert!(shown.contains("Cannot copy:"));
        view.key(KeyEvent::from(KeyCode::Char('c')), &mut false);
        assert!(view.take_copy_request().is_none());
    }
}

#[test]
fn long_readings_reach_the_tail_and_keep_exit_visible() {
    for (width, height) in [(80, 24), (100, 30)] {
        let mut view = view();
        let Some(Ok(found)) = &mut view.found else {
            panic!()
        };
        found.seeds[0].seed.goal = (0..100).map(|i| format!("TEST goal row {i}\n")).collect();
        found.seeds[0].seed.commands = None;
        let before = screen(&mut view, width, height, false);
        assert!(before.contains("TEST goal row 0"));
        assert!(!before.contains("Cannot copy:"));
        view.key(KeyEvent::from(KeyCode::End), &mut false);
        let tail = screen(&mut view, width, height, false);
        assert!(tail.contains("Cannot copy:") && tail.contains("esc back"));
        assert!(!tail.contains("c copy commands"));
        view.key(KeyEvent::from(KeyCode::Home), &mut false);
        assert!(screen(&mut view, width, height, false).contains("TEST goal row 0"));
    }
}

#[test]
fn unknown_and_incomplete_are_never_a_complete_empty_list() {
    let mut view = Approve::open(None);
    let shown = screen(&mut view, 80, 24, false);
    assert!(shown.contains("Reading sessions"));
    assert!(shown.contains("Can't tell which seeds have started; none are hidden."));
    view.found = Some(Ok(Found {
        unavailable: 2,
        reasons: vec!["TEST failure".into()],
        ..Found::default()
    }));
    let shown = screen(&mut view, 80, 24, false);
    assert!(
        shown.contains("may be incomplete") && shown.contains("No verified seeds could be listed")
    );
    assert!(!shown.contains("No verified seeds are waiting"));
    assert!(screen(&mut view, 80, 24, true).contains("TEST failure"));
}

#[test]
fn abandoned_read_cannot_publish_or_spawn_a_second_worker() {
    let (tx, rx) = mpsc::channel();
    let mut reader = Reader {
        running: Some((rx, Arc::new(AtomicBool::new(false)))),
    };
    reader.cancel();
    assert!(!reader.start(
        Vec::new(),
        None,
        Err("TEST no records".into()),
        Found::default(),
        None
    ));
    tx.send(Found {
        seeds: vec![seed("TEST stale")],
        ..Found::default()
    })
    .unwrap();
    let mut reopened = Approve::open(None);
    reader.poll(Some(&mut reopened));
    assert!(reopened.found.is_none());
    assert!(reader.running.is_none());
}

#[test]
fn a_clipped_reading_says_where_the_hidden_command_lies() {
    for (width, height) in [(80, 24), (100, 30)] {
        let mut view = view();
        assert!(!screen(&mut view, width, height, false).contains("more"));
        let Some(Ok(found)) = &mut view.found else {
            panic!()
        };
        found.seeds[0].seed.goal = (0..40).map(|i| format!("TEST goal row {i}\n")).collect();
        let top = screen(&mut view, width, height, false);
        assert!(
            top.contains("↓ more") && !top.contains("TEST intake"),
            "{top}"
        );
        view.key(KeyEvent::from(KeyCode::Down), &mut false);
        assert!(screen(&mut view, width, height, false).contains("↕ more"));
        view.key(KeyEvent::from(KeyCode::End), &mut false);
        let tail = screen(&mut view, width, height, false);
        assert!(
            tail.contains("↑ more") && tail.contains("TEST intake"),
            "{tail}"
        );
        assert!(tail.contains("esc back"), "{tail}");
    }
}

fn origin_visit(case: &str, expected_note: Option<&str>, can_copy: bool) {
    use std::{
        fs,
        time::{Duration, Instant, SystemTime},
    };
    let temporary = tempfile::Builder::new()
        .prefix("tinkery-TEST-approve-origin-")
        .tempdir()
        .unwrap();
    let root = temporary.path().canonicalize().unwrap();
    let checkout = root.join("TEST-checkout");
    let other = root.join("TEST-other");
    fs::create_dir_all(checkout.join(".git")).unwrap();
    fs::create_dir_all(other.join(".git")).unwrap();
    // Repository contents are irrelevant to attribution.
    fs::write(other.join(".git/TEST-unread"), "TEST not Git data").unwrap();
    let session = root.join("TEST-sessions/TEST-session");
    fs::create_dir_all(&session).unwrap();
    crate::seed_me::ledger::fixtures::confirmed(&session, "TEST unchanged goal", "r1");
    let records = root.join("TEST-records");
    fs::create_dir(&records).unwrap();
    if case != "missing" {
        let made_in = match case {
            "different" | "unresolved" | "no-checkout" => Some(other.as_path()),
            "no-repo" => None,
            _ => Some(checkout.as_path()),
        };
        let record = think_record::write(
            &records,
            &think_record::Session {
                origin: &think_record::Origin::simulated(),
                turns: &[],
                checks: vec![],
                goal: "TEST unchanged goal",
                seed_me_session: &session,
                repo: made_in,
                confirmed: SystemTime::now(),
            },
        )
        .unwrap();
        if case == "unknown" {
            let mut value: serde_json::Value =
                serde_json::from_slice(&fs::read(&record).unwrap()).unwrap();
            value["schema"] = 2.into();
            value.as_object_mut().unwrap().remove("repo");
            fs::write(record, value.to_string()).unwrap();
        } else if case == "unreadable" {
            fs::write(record, "TEST not JSON").unwrap();
        }
    }
    let named = match case {
        "dot-dot" => {
            fs::create_dir(checkout.join("TEST-subdir")).unwrap();
            Some(checkout.join("TEST-subdir/.."))
        }
        "unresolved" => Some(root.join("TEST-missing-checkout")),
        "no-checkout" => None,
        _ => Some(checkout),
    };
    let crate::seed_me::Record::Session(session) = crate::seed_me::read(&session) else {
        panic!()
    };
    let mut view = Approve::open(named);
    let mut reader = Reader::default();
    view.offer(Ok(vec![session]), 0, vec![], &mut reader, Ok(records));
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        reader.poll(Some(&mut view));
        if screen(&mut view, 100, 30, false).contains("Seed 1 of 1") {
            break;
        }
        assert!(Instant::now() < deadline, "Approve did not finish");
        std::thread::sleep(Duration::from_millis(1));
    }
    let shown = screen(&mut view, 100, 30, false);
    assert!(
        shown.contains("TEST unchanged goal") && shown.contains("TEST exact café"),
        "{shown}"
    );
    view.key(KeyEvent::from(KeyCode::End), &mut false);
    let shown = screen(&mut view, 100, 30, false);
    let flat = shown.split_whitespace().collect::<Vec<_>>().join(" ");
    if let Some(note) = expected_note {
        assert!(flat.contains(note), "{shown}");
    } else {
        assert!(
            !flat.contains("Made in ") && !flat.contains("unknown."),
            "{shown}"
        );
    }
    assert_eq!(shown.contains("c copy command"), can_copy, "{shown}");
    view.key(KeyEvent::from(KeyCode::Char('c')), &mut false);
    let copy = view.take_copy_request();
    assert_eq!(copy.is_some(), can_copy, "{shown}");
    if let Some(copy) = copy {
        assert!(copy.contains("--seed-contract") && copy.contains("--tinkery"));
    }
    if case == "different" {
        assert!(
            !shown.contains("python3") && !shown.contains("Cannot copy:"),
            "{shown}"
        );
    }
}

#[test]
fn matching_origin_and_a_dot_dot_checkout_offer_copy() {
    origin_visit("matching", None, true);
    origin_visit("dot-dot", None, true);
}

#[test]
fn different_origin_stays_readable_without_command_or_copy() {
    origin_visit(
        "different",
        Some(
            "Made in TEST-other, not the checkout you named (TEST-checkout). To start it, reopen Tinkery with --sdlc-root naming TEST-other.",
        ),
        false,
    );
}

#[test]
fn unknown_missing_unreadable_and_outside_origins_keep_copy_with_a_note() {
    for case in ["unknown", "missing", "unreadable"] {
        origin_visit(case, Some("Where this seed was made is unknown."), true);
    }
    origin_visit("no-repo", Some("Made outside any repository."), true);
}

#[test]
fn no_checkout_is_unchanged_and_unresolved_checkout_does_not_claim_a_mismatch() {
    origin_visit("no-checkout", None, false);
    origin_visit(
        "unresolved",
        Some("Where this seed was made is unknown."),
        false,
    );
}

#[test]
fn acceptance_checks_read_as_written_without_markdown_escapes() {
    let records = crate::seed_me::ledger::Value::parse(
        r#"[{"statement":"Visiting `/foo` returns 404 & *nothing* else","kind":"cmd","source":"TEST goal","cmd":"grep -c '\\*' TEST-file","expect":"1","cwd":"."},
            {"statement":"1. TEST [step] #2","kind":"judge","source":"TEST goal","how_to_judge":"Read `TEST.md` <head>"}]"#,
    )
    .unwrap();
    let mut view = view();
    view.found.as_mut().unwrap().as_mut().unwrap().seeds[0]
        .seed
        .acceptance = crate::seed_me::ledger::checks_block(&records).unwrap();
    let shown = screen(&mut view, 100, 30, false);
    for written in [
        "Visiting `/foo` returns 404 & *nothing* else",
        "check: `grep -c '\\*' TEST-file`",
        "1. TEST [step] #2",
        "you judge: Read `TEST.md` <head>",
    ] {
        assert!(shown.contains(written), "{written} not shown:\n{shown}");
    }
    assert!(
        !shown.contains("\\`") && !shown.contains("&amp;"),
        "{shown}"
    );
}
