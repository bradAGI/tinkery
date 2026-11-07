//! Menu and tend never wait on session discovery. The fake source is held open, so a
//! regression that reads inside paint, tick or input fails instead of passing quietly.
use super::*;
use crate::tend::{Found, Outcome, Session, Stage, scan};
use serde_json::json;
use std::{
    collections::HashSet,
    fs,
    sync::{
        Condvar, Mutex,
        atomic::{AtomicUsize, Ordering::SeqCst},
    },
};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn sess(id: &str, stage: Stage, updated: &str) -> Session {
    Session {
        id: Some(id.into()),
        path: PathBuf::from(format!("/TEST/{id}")),
        goal: format!("goal {id}"),
        stage,
        updated: updated.into(),
    }
}
fn found(sessions: Vec<Session>) -> Outcome {
    Ok(Found {
        sessions,
        unreadable: 0,
        failures: Vec::new(),
    })
}

/// A source that blocks until given a permit, counting entries, finishes and peak workers.
#[derive(Default)]
struct Gate {
    permits: Mutex<usize>,
    wake: Condvar,
    outcome: Mutex<Option<Outcome>>,
    entered: AtomicUsize,
    finished: AtomicUsize,
    live: AtomicUsize,
    peak: AtomicUsize,
}
impl Gate {
    fn new(outcome: Outcome) -> Arc<Self> {
        let gate = Arc::new(Self::default());
        *gate.outcome.lock().unwrap() = Some(outcome);
        gate
    }
    fn source(self: &Arc<Self>) -> Source {
        let gate = self.clone();
        Arc::new(move |_root| {
            gate.entered.fetch_add(1, SeqCst);
            let now = gate.live.fetch_add(1, SeqCst) + 1;
            gate.peak.fetch_max(now, SeqCst);
            let mut permits = gate.permits.lock().unwrap();
            let deadline = Instant::now() + Duration::from_secs(4);
            while *permits == 0 {
                let left = deadline.saturating_duration_since(Instant::now());
                if left.is_zero() {
                    break;
                }
                permits = gate.wake.wait_timeout(permits, left).unwrap().0;
            }
            *permits = permits.saturating_sub(1);
            drop(permits);
            gate.live.fetch_sub(1, SeqCst);
            gate.finished.fetch_add(1, SeqCst);
            gate.outcome.lock().unwrap().clone().unwrap()
        })
    }
    fn release(&self, n: usize) {
        *self.permits.lock().unwrap() += n;
        self.wake.notify_all();
    }
    fn set(&self, outcome: Outcome) {
        *self.outcome.lock().unwrap() = Some(outcome);
    }
}
fn wait_until(what: &str, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !condition() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn settle(h: &mut Home) {
    wait_until("discovery to settle", || {
        h.tick();
        h.discovery.requested() && !h.discovery.running() && h.discovery.fresh()
    });
}
#[test]
fn places_keep_digit_arrow_alias_and_row_navigation() {
    for (index, name) in [
        "think", "tend", "shape", "approve", "watch", "taste", "measure",
    ]
    .into_iter()
    .enumerate()
    {
        let mut direct = home(Arc::new(|_| found(vec![])));
        direct.open(Place::at(index).unwrap());
        let expected = screen(&mut direct);
        let heading = if name == "think" {
            "What's on your mind?"
        } else {
            name
        };
        assert!(expected.contains(heading), "{name}: {expected}");
        let mut digit = home(Arc::new(|_| found(vec![])));
        digit.handle_key(key(KeyCode::Char(char::from(b'1' + index as u8))));
        assert_eq!(screen(&mut digit), expected, "digit for {name}");
        for code in [
            KeyCode::Down,
            KeyCode::Char('j'),
            KeyCode::Up,
            KeyCode::Char('k'),
        ] {
            let steps = if matches!(code, KeyCode::Down | KeyCode::Char('j')) {
                PLACES.len() + index
            } else {
                PLACES.len() - index
            };
            let mut arrows = home(Arc::new(|_| found(vec![])));
            for _ in 0..steps {
                arrows.handle_key(key(code));
            }
            arrows.handle_key(key(KeyCode::Enter));
            assert_eq!(screen(&mut arrows), expected, "{code:?} for {name}");
        }
        let mut mouse = home(Arc::new(|_| found(vec![])));
        mouse.handle_mouse(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 5,
                row: FIRST_ROW + index as u16 * ROW_GAP,
                modifiers: KeyModifiers::NONE,
            },
            Rect::new(0, 0, 100, 30),
        );
        assert_eq!(screen(&mut mouse), expected, "row for {name}");
    }
}

#[test]
fn place_indices_are_checked_before_opening() {
    for (index, (name, _)) in PLACES.iter().enumerate() {
        let mut h = home(Arc::new(|_| found(vec![])));
        h.open(Place::at(index).unwrap());
        let frame = screen(&mut h);
        let heading = if *name == "think" {
            "What's on your mind?"
        } else {
            name
        };
        assert!(frame.contains(heading), "{name}: {frame}");
    }
    for index in [PLACES.len(), PLACES.len() + 1, 100, usize::MAX] {
        assert_eq!(Place::at(index), None, "accepted invalid place {index}");
    }
}

#[test]
fn tend_ignores_clipboard_results_in_garden_and_detail() {
    let mut h = garden(1);
    for detail in [false, true] {
        if detail {
            h.handle_key(key(KeyCode::Enter));
        }
        let before = screen(&mut h);
        h.handle_key(key(KeyCode::Char('c')));
        assert!(h.take_copy_request().is_none());
        for sent in [true, false] {
            h.copy_result(sent);
            assert_eq!(screen(&mut h), before);
        }
    }
}

#[test]
fn watch_f10_returns_home_from_help_without_quitting() {
    for help in [true, false] {
        let mut h = home(Arc::new(|_| found(vec![])));
        h.open(Place::at(4).unwrap());
        if help {
            h.handle_key(key(KeyCode::F(1)));
        }
        assert_eq!(h.help, help);
        h.handle_key(key(KeyCode::F(10)));
        assert!(
            matches!(h.view, View::Menu) && !h.help && !h.quit,
            "F10 from Watch (help={help}) must return Home without quitting"
        );
        h.handle_key(key(KeyCode::F(10)));
        assert!(h.quit, "F10 on Home still quits");
        for mut ignored in [
            key(KeyCode::F(10)),
            KeyEvent::new(KeyCode::F(10), KeyModifiers::ALT),
        ] {
            let mut h = home(Arc::new(|_| found(vec![])));
            h.open(Place::at(4).unwrap());
            if help {
                h.handle_key(key(KeyCode::F(1)));
            }
            if ignored.modifiers == KeyModifiers::NONE {
                ignored.kind = KeyEventKind::Repeat;
            }
            h.handle_key(ignored);
            assert!(matches!(h.view, View::Watch(_)) && !h.quit);
        }
    }
}

fn home(source: Source) -> Home {
    Home::new(
        Box::new(BrainDump::default),
        Ok(PathBuf::from("/TEST/sessions")),
        Err("TEST links unavailable".into()),
    )
    .with_discovery_source(source)
}
fn screen(h: &mut Home) -> String {
    snapshot(100, 30, h, true).unwrap()
}

fn garden(count: usize) -> Home {
    let mut h = home(Arc::new(|_| panic!("input and painting never read")));
    let mut tend = Tend::new(Err("TEST links unavailable".into()));
    tend.apply(
        &found(
            (0..count)
                .map(|i| sess(&i.to_string(), Stage::GoalSaved, "unknown"))
                .collect(),
        ),
        true,
        1,
    );
    tend.current = true;
    h.view = View::Tend(Box::new(tend));
    h
}
fn selected(h: &Home) -> usize {
    let View::Tend(tend) = &h.view else { panic!() };
    tend.selected
}
fn press(h: &mut Home, column: u16, row: u16, now: Instant) {
    h.handle_mouse_at(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        },
        Rect::new(0, 0, 100, 30),
        now,
    );
}

fn goal_point(h: &mut Home, name: &str) -> (u16, u16) {
    let frame = screen(h);
    let (row, line) = frame
        .lines()
        .enumerate()
        .skip(3)
        .take(23)
        .find(|(_, line)| line.contains(name))
        .unwrap_or_else(|| panic!("{name}: {frame}"));
    let col = line[..line.find(name).unwrap()].chars().count();
    ((col + name.chars().count() / 2) as u16, row as u16)
}

fn press_goal(h: &mut Home, name: &str, now: Instant) {
    let (x, y) = goal_point(h, name);
    press(h, x, y, now);
}

#[test]
fn ended_goals_never_enter_garden_selection_keys_mouse_or_detail() {
    let stages = [
        Stage::GoalSaved,
        Stage::SeedDrafted,
        Stage::SeedUnchecked,
        Stage::Unknown,
        Stage::Simulated,
    ];
    let mut h = garden(0);
    let View::Tend(tend) = &mut h.view else {
        panic!()
    };
    let sessions = stages
        .iter()
        .enumerate()
        .flat_map(|(i, &stage)| {
            [
                sess(&format!("ended-{i}"), Stage::Completed, "unknown"),
                sess(&i.to_string(), stage, "unknown"),
            ]
        })
        .collect();
    tend.apply(&found(sessions), true, 1);
    assert_eq!(
        tend.sessions.iter().map(|s| s.stage).collect::<Vec<_>>(),
        stages
    );
    assert_eq!(tend.ended, 5);
    let frame = screen(&mut h);
    assert!(frame.contains("5 ended not shown"), "{frame}");
    assert!(!frame.contains("goal ended"), "{frame}");
    for code in [
        KeyCode::Right,
        KeyCode::Char('j'),
        KeyCode::Left,
        KeyCode::Char('k'),
        KeyCode::Down,
        KeyCode::Up,
    ] {
        for _ in 0..7 {
            h.handle_key(key(code));
            let View::Tend(tend) = &h.view else { panic!() };
            assert_ne!(tend.current().unwrap().stage, Stage::Completed);
        }
    }
    let now = Instant::now();
    for (index, &stage) in stages.iter().enumerate() {
        press_goal(
            &mut h,
            &format!("goal {index}"),
            now + Duration::from_secs(index as u64),
        );
        assert_eq!(selected(&h), index);
        h.handle_key(key(KeyCode::Enter));
        let View::Tend(tend) = &h.view else { panic!() };
        assert!(tend.detail);
        assert_eq!(tend.current().unwrap().stage, stage);
        h.handle_key(key(KeyCode::Esc));
    }
}

#[test]
fn all_ended_is_not_empty_and_incomplete_evidence_stays_visible() {
    let tmp = tempfile::Builder::new()
        .prefix("tinkery-TEST-ended-notes-")
        .tempdir()
        .unwrap();
    let mut h = garden(0);
    let View::Tend(tend) = &mut h.view else {
        panic!()
    };
    tend.needs = crate::tend::needs::Store::new(Ok(tmp.path().join("TEST-links")));
    for count in [1, 2] {
        for unreadable in [0, 1, 2] {
            let View::Tend(tend) = &mut h.view else {
                panic!()
            };
            tend.apply(
                &Ok(Found {
                    sessions: (0..count)
                        .map(|i| sess(&format!("ended-{i}"), Stage::Completed, "unknown"))
                        .collect(),
                    unreadable,
                    failures: Vec::new(),
                }),
                true,
                1,
            );
            for (width, height) in [(80, 24), (100, 30), (160, 40), (320, 40)] {
                let frame = snapshot(width, height, &mut h, true).unwrap();
                let text = frame.split_whitespace().collect::<Vec<_>>().join(" ");
                if unreadable == 0 {
                    assert!(text.contains(&format!("No active goals. {count} ended not shown. Confirm a goal in think to start one.")), "{frame}");
                } else {
                    assert!(!text.contains("No active goals"), "{frame}");
                    let records = if unreadable == 1 { "record" } else { "records" };
                    assert!(
                        text.contains(&format!("{unreadable} saved {records} couldn't be read")),
                        "{frame}"
                    );
                    assert_eq!(text.matches("couldn't be read").count(), 1, "{frame}");
                }
                assert!(!text.contains("No saved sessions"), "{frame}");
                assert!(
                    text.contains(&format!("{count} ended not shown")),
                    "{frame}"
                );
                assert_eq!(text.matches("ended not shown").count(), 1);
                assert_eq!(frame.lines().last().unwrap().trim(), "esc back");
                for code in [
                    KeyCode::Right,
                    KeyCode::Down,
                    KeyCode::Char('j'),
                    KeyCode::Enter,
                ] {
                    h.handle_key(key(code));
                }
                press(&mut h, 3, 3, Instant::now());
                let View::Tend(tend) = &h.view else { panic!() };
                assert!(tend.current().is_none() && !tend.detail);
            }
        }
    }
    let View::Tend(tend) = &mut h.view else {
        panic!()
    };
    tend.apply(&found(vec![]), true, 2);
    let frame = screen(&mut h);
    assert!(frame.contains("No saved sessions yet. Confirm a goal in think to start one."));
    assert!(!frame.contains("ended not shown"));
}

#[test]
fn wanted_session_is_matched_before_filtering_and_selected_by_path_after_it() {
    for stage in [Stage::GoalSaved, Stage::Completed] {
        let mut tend = Tend::new(Err("TEST links unavailable".into()));
        tend.want = Some(("/TEST/alias".into(), Some("/TEST/target".into())));
        tend.apply(
            &found(vec![
                sess("ended", Stage::Completed, "unknown"),
                sess("target", stage, "unknown"),
            ]),
            false,
            1,
        );
        assert!(tend.want.is_none() && !tend.pending && tend.error.is_none());
        assert_eq!(tend.detail, stage != Stage::Completed);
        if stage != Stage::Completed {
            assert_eq!(tend.selected, 0);
            assert_eq!(tend.current().unwrap().path, Path::new("/TEST/target"));
        } else {
            assert!(tend.current().is_none());
        }
    }
}

#[test]
fn garden_keys_step_one_goal_in_newest_order_and_clamp_after_resize() {
    let mut h = garden(23);
    h.handle_key(key(KeyCode::Down));
    assert_eq!(selected(&h), 1, "the pre-render stride defaults to one");
    screen(&mut h);
    for (code, expected) in [
        (KeyCode::Right, 2),
        (KeyCode::Char('j'), 3),
        (KeyCode::Down, 4),
        (KeyCode::Up, 3),
        (KeyCode::Up, 2),
        (KeyCode::Left, 1),
        (KeyCode::Char('k'), 0),
        (KeyCode::Left, 0),
        (KeyCode::Left, 0),
    ] {
        h.handle_key(key(code));
        assert_eq!(selected(&h), expected);
    }
    snapshot(80, 24, &mut h, true).unwrap();
    h.handle_key(key(KeyCode::Down));
    assert_eq!(selected(&h), 1, "resize never changes selection order");
    for _ in 0..30 {
        h.handle_key(key(KeyCode::Right));
    }
    assert_eq!(selected(&h), 22);
    h.handle_key(key(KeyCode::Down));
    assert_eq!(selected(&h), 22);
    h.handle_key(key(KeyCode::Char('c')));
    assert!(h.take_copy_request().is_none(), "garden cannot copy");
    h.handle_key(key(KeyCode::Enter));
    let View::Tend(tend) = &h.view else { panic!() };
    assert!(tend.detail);
    h.handle_key(key(KeyCode::Down));
    h.handle_key(key(KeyCode::Right));
    let View::Tend(tend) = &h.view else { panic!() };
    assert_eq!(tend.scroll, 1);
    assert_eq!(tend.selected, 22);
    h.handle_key(key(KeyCode::Char('c')));
    assert!(
        h.take_copy_request().is_none(),
        "the retired skill prompt is not copied"
    );
    h.handle_key(key(KeyCode::Esc));
    let View::Tend(tend) = &h.view else { panic!() };
    assert!(!tend.detail && tend.scroll == 0);
    h.handle_key(key(KeyCode::F(1)));
    assert!(
        screen(&mut h)
            .contains("Arrows / j k move · Enter or a double click opens the saved goal.")
    );
    h.handle_key(key(KeyCode::Down));
    assert_eq!(selected(&h), 22);
    h.handle_key(key(KeyCode::Esc));
    h.handle_key(key(KeyCode::Esc));
    assert!(matches!(h.view, View::Menu) && !h.quit);
    let mut h = garden(1);
    h.handle_key(key(KeyCode::F(1)));
    h.handle_key(key(KeyCode::F(10)));
    assert!(matches!(h.view, View::Menu) && !h.help && !h.quit);
}

#[test]
fn linking_routes_keys_and_single_click_and_help_guards_hidden_actions() {
    let tmp = tempfile::Builder::new()
        .prefix("tinkery-TEST-link-input-")
        .tempdir()
        .unwrap();
    let root = tmp.path().join("TEST-links");
    let mut h = garden(7);
    let View::Tend(tend) = &mut h.view else {
        panic!()
    };
    tend.needs = tend::needs::Store::new(Ok(root.clone()));
    for (width, height) in [(80, 24), (100, 30), (160, 40)] {
        snapshot(width, height, &mut h, true).unwrap();
        h.handle_key(key(KeyCode::Char('l')));
        let frame = snapshot(width, height, &mut h, true).unwrap();
        assert!(
            frame.contains("↑↓←→ pick   enter link   esc cancel"),
            "{frame}"
        );
        h.handle_key(key(KeyCode::Enter)); // self-pick
        h.handle_key(key(KeyCode::Right));
        let frame = snapshot(width, height, &mut h, true).unwrap();
        assert!(
            frame.contains("› goal 0") && frame.contains("→ goal 1"),
            "{frame}"
        );
        assert!(frame.contains("goal 0 needs goal 1"), "{frame}");
        h.handle_key(key(KeyCode::F(1)));
        for code in [KeyCode::Char('l'), KeyCode::Enter, KeyCode::Right] {
            h.handle_key(key(code));
        }
        assert_eq!(selected(&h), 1);
        h.handle_key(key(KeyCode::Esc));
        h.handle_key(key(KeyCode::Esc));
        assert_eq!(selected(&h), 0);
        assert!(!root.exists());
    }
    screen(&mut h);
    h.handle_key(key(KeyCode::Char('l')));
    press_goal(&mut h, "goal 1", Instant::now());
    let View::Tend(tend) = &h.view else { panic!() };
    assert!(!tend.is_linking() && !tend.detail);
    assert_eq!(selected(&h), 0);
    assert!(tend.needs.read().into_links().unwrap().contains("0", "1"));
    h.handle_key(key(KeyCode::Char('l')));
    h.handle_key(key(KeyCode::Char('j')));
    assert!(screen(&mut h).contains("enter unlink"));
    h.handle_key(key(KeyCode::Enter));
    assert_eq!(selected(&h), 0);
    assert!(screen(&mut h).contains("l link"));
    let View::Tend(tend) = &h.view else { panic!() };
    assert!(!tend.needs.read().into_links().unwrap().contains("0", "1"));
}

#[test]
fn link_key_is_ignored_outside_garden_and_repeated_presses_do_not_start_linking() {
    for state in [
        "detail",
        "help",
        "how",
        "reading",
        "wanted",
        "unavailable",
        "empty",
        "repeat",
    ] {
        let mut h = garden(2);
        if matches!(state, "help" | "how") {
            h.handle_key(key(KeyCode::F(1)));
            if state == "how" {
                h.handle_key(key(KeyCode::Char('h')));
            }
        }
        let View::Tend(tend) = &mut h.view else {
            panic!()
        };
        match state {
            "detail" => tend.detail = true,
            "reading" => tend.pending = true,
            "wanted" => tend.want = Some(("/TEST/wanted".into(), None)),
            "unavailable" => tend.error = Some("TEST unavailable".into()),
            "empty" => tend.sessions.clear(),
            _ => {}
        }
        let mut event = key(KeyCode::Char('l'));
        if state == "repeat" {
            event.kind = KeyEventKind::Repeat;
        }
        h.handle_key(event);
        let View::Tend(tend) = &h.view else { panic!() };
        assert!(!tend.is_linking() && tend.notice.is_empty(), "{state}");
    }
}

#[test]
fn garden_press_selects_and_only_a_timely_same_session_press_opens() {
    let now = Instant::now();
    for (elapsed, opens) in [(499, true), (500, true), (501, false)] {
        let mut h = garden(7);
        screen(&mut h);
        // Hit the ring through its displayed label.
        press_goal(&mut h, "goal 1", now);
        assert_eq!(selected(&h), 1);
        press_goal(&mut h, "goal 1", now + Duration::from_millis(elapsed));
        let View::Tend(tend) = &h.view else { panic!() };
        assert_eq!(tend.detail, opens);
    }
    let mut h = garden(7);
    screen(&mut h);
    press_goal(&mut h, "goal 1", now);
    press_goal(&mut h, "goal 2", now + Duration::from_millis(100));
    let View::Tend(tend) = &h.view else { panic!() };
    assert!(
        !tend.detail && tend.selected == 2,
        "different goals aren't a double click"
    );
}

#[test]
fn late_read_cannot_redirect_a_double_press_to_a_different_session() {
    let now = Instant::now();
    let mut h = garden(7);
    screen(&mut h);
    press_goal(&mut h, "goal 1", now);
    let View::Tend(tend) = &mut h.view else {
        panic!()
    };
    let mut sessions = tend.sessions.clone();
    sessions.insert(0, sess("new", Stage::GoalSaved, "unknown"));
    tend.apply(&found(sessions), true, 2);
    assert_eq!(tend.selected, 2);
    screen(&mut h);
    press_goal(&mut h, "goal 0", now + Duration::from_millis(100));
    let View::Tend(tend) = &h.view else { panic!() };
    assert!(!tend.detail && tend.current().unwrap().path == Path::new("/TEST/0"));
    // The same identity moving to another position still counts as a double press.
    let mut h = garden(7);
    screen(&mut h);
    press_goal(&mut h, "goal 1", now);
    let View::Tend(tend) = &mut h.view else {
        panic!()
    };
    let mut sessions = tend.sessions.clone();
    sessions.insert(0, sess("new", Stage::GoalSaved, "unknown"));
    tend.apply(&found(sessions), true, 2);
    screen(&mut h);
    press_goal(&mut h, "goal 1", now + Duration::from_millis(100));
    let View::Tend(tend) = &h.view else { panic!() };
    assert!(tend.detail && tend.current().unwrap().path == Path::new("/TEST/1"));
}

#[test]
fn garden_presses_ignore_blank_rows_help_detail_and_discovery_states() {
    let now = Instant::now();
    for state in [
        "garden",
        "help",
        "detail",
        "reading",
        "wanted",
        "unavailable",
        "empty",
    ] {
        let mut h = garden(7);
        screen(&mut h);
        if state == "help" {
            h.handle_key(key(KeyCode::F(1)));
        }
        let View::Tend(tend) = &mut h.view else {
            panic!()
        };
        match state {
            "detail" => tend.detail = true,
            "reading" => tend.pending = true,
            "wanted" => tend.want = Some(("/TEST/wanted".into(), None)),
            "unavailable" => tend.error = Some("unavailable".into()),
            "empty" => tend.sessions.clear(),
            _ => {}
        }
        let detail = tend.detail;
        let row = if state == "garden" { 26 } else { 3 };
        press(&mut h, 0, row, now);
        press(&mut h, 0, row, now + Duration::from_millis(100));
        let View::Tend(tend) = &h.view else { panic!() };
        assert_eq!(tend.selected, 0, "{state}");
        assert_eq!(tend.detail, detail, "{state}");
    }
    let mut h = garden(7);
    for (column, row) in [(0, 3), (99, 3), (2, 26), (2, 27), (2, 28), (2, 29)] {
        press(&mut h, column, row, now);
    }
    assert_eq!(selected(&h), 0);
}

#[test]
fn garden_test_session_plain_text_renders() {
    let mut h = garden(7);
    let View::Tend(tend) = &mut h.view else {
        panic!()
    };
    let mut sessions = tend.sessions.clone();
    for (session, goal) in sessions.iter_mut().zip([
        "Find invoices",
        "Read a calm theme",
        "Keep goals intact",
        "Plan the next seed",
        "Review saved work",
        "Make room to think",
        "Leave manual control",
    ]) {
        session.goal = goal.into();
    }
    tend.apply(&found(sessions), true, tend.seen + 1);
    for (width, height) in [(100, 30), (80, 24)] {
        let screen = snapshot(width, height, &mut h, true).unwrap();
        println!("TEST garden {width}×{height}:\n{screen}");
        assert_eq!(screen.split('\n').count(), height as usize);
        assert!(screen.contains("› Find invoices") && screen.contains("goal saved · time unknown"));
    }
}

#[test]
fn blocked_discovery_still_paints_six_places_and_handles_queued_input() {
    let gate = Gate::new(found(vec![sess(
        "a",
        Stage::GoalSaved,
        "2026-10-09T10:00:00Z",
    )]));
    let mut h = home(gate.source());
    h.tick();
    wait_until("the worker to start", || gate.entered.load(SeqCst) == 1);
    for _ in 0..50 {
        let frame = screen(&mut h);
        for name in ["think", "tend", "approve", "watch", "taste", "measure"] {
            assert!(frame.contains(name), "{frame}");
        }
        assert!(!frame.contains("Reading") && !frame.contains("No saved"));
    }
    assert_eq!(h.count(1), None, "unknown is not shown as a number or zero");
    assert_eq!(gate.entered.load(SeqCst), 1, "painting never reads sources");
    // Queued navigation and quit, handled while the source is still blocked.
    for code in [
        KeyCode::Char('j'),
        KeyCode::Down,
        KeyCode::Up,
        KeyCode::Char('k'),
    ] {
        h.handle_key(key(code));
        h.tick();
    }
    h.handle_key(key(KeyCode::Char('2')));
    h.tick();
    assert!(matches!(h.view, View::Tend(_)));
    h.handle_key(key(KeyCode::Esc));
    h.handle_key(key(KeyCode::Char('q')));
    assert!(h.quit, "quit is handled before discovery finishes");
    assert_eq!(
        gate.finished.load(SeqCst),
        0,
        "the source never returned first"
    );
    gate.release(8);
    wait_until("late landing", || {
        h.tick();
        gate.finished.load(SeqCst) >= 1 && !h.discovery.running()
    });
    assert!(gate.peak.load(SeqCst) <= 1, "one worker at a time");
}

#[test]
fn many_requests_while_blocked_stay_bounded_to_one_worker() {
    let gate = Gate::new(found(vec![]));
    let mut h = home(gate.source());
    h.tick();
    wait_until("start", || gate.entered.load(SeqCst) == 1);
    for _ in 0..20 {
        h.open(Place::at(1).unwrap());
        h.handle_key(key(KeyCode::Esc));
        h.open(Place::at(0).unwrap());
        h.handle_key(key(KeyCode::F(10)));
        h.tick();
    }
    assert_eq!(gate.live.load(SeqCst), 1);
    gate.release(100);
    settle(&mut h);
    assert_eq!(gate.peak.load(SeqCst), 1);
    assert!(
        gate.entered.load(SeqCst) <= 2,
        "requests coalesce into one follow-up"
    );
}

#[test]
fn tend_opened_during_discovery_is_responsive_and_never_says_no_sessions() {
    let gate = Gate::new(found(vec![sess(
        "a",
        Stage::GoalSaved,
        "2026-10-09T10:00:00Z",
    )]));
    let mut h = home(gate.source());
    h.tick();
    h.handle_key(key(KeyCode::Char('2')));
    let frame = screen(&mut h);
    assert!(
        frame.contains("Reading your sessions. Esc goes back."),
        "{frame}"
    );
    assert!(!frame.contains("No saved sessions"), "{frame}");
    assert!(frame.to_lowercase().contains("esc back"));
    assert!(
        !frame.contains("move") && !frame.contains("enter open"),
        "the key bar offers only what is possible now:\n{frame}"
    );
    h.handle_key(key(KeyCode::Down));
    h.handle_key(key(KeyCode::Enter));
    let View::Tend(tend) = &h.view else { panic!() };
    assert!(!tend.detail, "nothing to open yet");
    h.handle_key(key(KeyCode::F(1)));
    h.handle_key(key(KeyCode::Esc));
    h.handle_key(key(KeyCode::F(10)));
    assert!(matches!(h.view, View::Menu), "F10 leaves tend");
    assert!(!h.quit);
    h.handle_key(key(KeyCode::Char('2')));
    h.handle_key(key(KeyCode::Esc));
    assert!(matches!(h.view, View::Menu));
    let selected = h.selected;
    gate.release(8);
    settle(&mut h);
    assert!(
        matches!(h.view, View::Menu),
        "a late result never reopens tend"
    );
    assert_eq!(h.selected, selected, "or moves the menu selection");
    assert_eq!(
        h.count(1),
        Some("1".into()),
        "the count appears once complete"
    );
}

#[test]
fn a_slow_read_changes_one_quiet_line_and_starts_nothing() {
    let gate = Gate::new(found(vec![sess(
        "a",
        Stage::GoalSaved,
        "2026-10-09T10:00:00Z",
    )]));
    let mut h = home(gate.source());
    h.tick();
    wait_until("the worker to start", || gate.entered.load(SeqCst) == 1);
    h.handle_key(key(KeyCode::Char('2')));
    // The clock is an input of the real tick, so no test sleeps: the read's start plus a wait.
    let since = h.discovery.running_since().expect("a read is pending");
    let threshold = tend::STILL_READING_AFTER;
    h.tick_at(since + threshold - Duration::from_millis(1));
    let before = screen(&mut h);
    assert!(
        before.contains("Reading your sessions. Esc goes back."),
        "{before}"
    );
    assert!(!before.contains("Still reading"), "{before}");
    h.tick_at(since + threshold);
    let after = screen(&mut h);
    assert!(
        after.contains("Still reading your sessions. Esc goes back."),
        "{after}"
    );
    assert!(!after.contains("Reading your sessions"), "{after}");
    // Only that line changed: no spinner, no loading screen.
    assert_eq!(before.lines().count(), after.lines().count());
    let changed = before.lines().zip(after.lines()).filter(|(a, b)| a != b);
    assert_eq!(changed.count(), 1, "{before}\n{after}");
    // Nothing started, and the view is as live as before.
    assert_eq!(gate.entered.load(SeqCst), 1, "no replacement worker");
    assert_eq!(gate.live.load(SeqCst), 1);
    h.handle_key(key(KeyCode::Down));
    h.handle_key(key(KeyCode::Enter));
    let View::Tend(tend) = &h.view else { panic!() };
    assert!(!tend.detail, "nothing to open yet");
    h.handle_key(key(KeyCode::Esc));
    assert!(matches!(h.view, View::Menu), "Esc still goes back");
    // The wait belongs to the read, not the visit: reopening meets the same slow read.
    h.open(Place::at(1).unwrap());
    h.tick_at(since + threshold);
    let reopened = screen(&mut h);
    assert!(
        reopened.contains("Still reading your sessions."),
        "{reopened}"
    );
    assert_eq!(gate.entered.load(SeqCst), 1, "still one worker");
    // When the read lands the line is gone, however late the clock says it is.
    gate.release(8);
    settle(&mut h);
    h.tick_at(since + threshold * 10);
    let landed = screen(&mut h);
    assert!(landed.contains("goal a"), "{landed}");
    assert!(!landed.to_lowercase().contains("reading"), "{landed}");
}

#[test]
fn a_slow_read_behind_a_saved_session_says_so_too() {
    let gate = Gate::new(found(vec![]));
    let mut h = home(gate.source());
    h.tick();
    wait_until("the worker to start", || gate.entered.load(SeqCst) == 1);
    h.open_tend_session(Path::new("/TEST/new"));
    let since = h.discovery.running_since().expect("a read is pending");
    h.tick_at(since + tend::STILL_READING_AFTER - Duration::from_millis(1));
    assert!(screen(&mut h).contains("Reading your sessions. Esc goes back."));
    h.tick_at(since + tend::STILL_READING_AFTER);
    let frame = screen(&mut h);
    assert!(frame.contains("Still reading your sessions."), "{frame}");
    assert!(!frame.contains("unavailable"), "{frame}");
    gate.set(found(vec![sess(
        "new",
        Stage::GoalSaved,
        "2026-10-10T10:00:00Z",
    )]));
    gate.release(8);
    settle(&mut h);
    let View::Tend(tend) = &h.view else { panic!() };
    assert!(tend.detail && tend.error.is_none());
}

#[test]
fn a_slow_reread_leaves_a_list_already_on_screen_alone() {
    let gate = Gate::new(found(vec![sess(
        "a",
        Stage::GoalSaved,
        "2026-10-09T10:00:00Z",
    )]));
    gate.release(1);
    let mut h = home(gate.source());
    settle(&mut h);
    // Reopening reads again, and this read gets no permit: the last result shows meanwhile.
    h.open(Place::at(1).unwrap());
    wait_until("the second read", || gate.entered.load(SeqCst) == 2);
    let since = h
        .discovery
        .running_since()
        .expect("the second read is running");
    let before = screen(&mut h);
    h.tick_at(since + tend::STILL_READING_AFTER * 10);
    let after = screen(&mut h);
    assert!(after.contains("goal a"), "{after}");
    assert!(!after.to_lowercase().contains("reading"), "{after}");
    assert_eq!(before, after, "a slow re-read changes nothing on screen");
    gate.release(8);
    settle(&mut h);
}

#[test]
fn a_slow_reread_behind_an_empty_list_never_says_no_sessions() {
    let gate = Gate::new(found(vec![]));
    gate.release(1);
    let mut h = home(gate.source());
    settle(&mut h);
    // A goal was saved since the empty read; the read that would contain it is slow.
    gate.set(found(vec![sess(
        "a",
        Stage::GoalSaved,
        "2026-10-09T10:00:00Z",
    )]));
    h.open(Place::at(1).unwrap());
    let frame = screen(&mut h);
    assert!(
        frame.contains("Reading your sessions. Esc goes back."),
        "{frame}"
    );
    wait_until("the second read", || gate.entered.load(SeqCst) == 2);
    let since = h
        .discovery
        .running_since()
        .expect("the second read is running");
    h.tick_at(since + tend::STILL_READING_AFTER * 10);
    let frame = screen(&mut h);
    assert!(
        frame.contains("Still reading your sessions. Esc goes back."),
        "{frame}"
    );
    assert!(!frame.contains("No saved sessions"), "{frame}");
    assert!(!frame.contains("enter open"), "{frame}");
    gate.release(8);
    settle(&mut h);
    let frame = screen(&mut h);
    assert!(frame.contains("goal a"), "{frame}");
    assert!(
        frame.contains("↑↓←→ move   enter open   l link   esc back"),
        "{frame}"
    );
}

#[test]
fn late_results_keep_the_selected_session_by_identity_and_do_not_replace_detail() {
    let old = [
        sess("a", Stage::GoalSaved, "2026-10-09T10:00:00Z"),
        sess("b", Stage::GoalSaved, "2026-10-08T10:00:00Z"),
        sess("c", Stage::GoalSaved, "2026-10-07T10:00:00Z"),
    ];
    let gate = Gate::new(found(old.to_vec()));
    gate.release(1);
    let mut h = home(gate.source());
    settle(&mut h);
    h.open(Place::at(1).unwrap());
    gate.release(1);
    settle(&mut h);
    h.handle_key(key(KeyCode::Down));
    h.handle_key(key(KeyCode::Down));
    let View::Tend(tend) = &h.view else { panic!() };
    assert_eq!(tend.current().unwrap().path, PathBuf::from("/TEST/c"));
    // A newer session appears at the top; the reader's selection does not move to it.
    let mut newer = vec![sess("n", Stage::GoalSaved, "2026-10-10T10:00:00Z")];
    newer.extend(old.clone());
    gate.set(found(newer.clone()));
    h.open(Place::at(1).unwrap());
    gate.release(1);
    settle(&mut h);
    h.handle_key(key(KeyCode::Down));
    h.handle_key(key(KeyCode::Down));
    let View::Tend(tend) = &h.view else { panic!() };
    let chosen = tend.current().unwrap().path.clone();
    // Open detail, then let a read land that edits that session's goal and drops another.
    h.handle_key(key(KeyCode::Enter));
    let mut edited = newer.clone();
    edited.iter_mut().find(|s| s.path == chosen).unwrap().goal = "EDITED elsewhere".into();
    edited.remove(0);
    gate.set(found(edited));
    h.discovery.request(h.sessions_root.as_deref().unwrap());
    gate.release(2);
    settle(&mut h);
    let View::Tend(tend) = &h.view else { panic!() };
    assert!(tend.detail);
    assert_eq!(tend.current().unwrap().path, chosen);
    assert_ne!(
        tend.current().unwrap().goal,
        "EDITED elsewhere",
        "frozen while read"
    );
    h.handle_key(key(KeyCode::Esc));
    h.tick();
    let View::Tend(tend) = &h.view else { panic!() };
    assert_eq!(
        tend.current().unwrap().path,
        chosen,
        "selection follows identity"
    );
    assert_eq!(tend.current().unwrap().goal, "EDITED elsewhere");
}

#[test]
fn shape_return_waits_for_fresh_discovery_and_cannot_reopen_exited_detail() {
    for leave_detail in [false, true] {
        let gate = Gate::new(found(vec![sess("a", Stage::GoalSaved, "")]));
        gate.release(1);
        let mut h = home(gate.source());
        h.open(Place::at(1).unwrap());
        settle(&mut h);
        h.handle_key(key(KeyCode::Enter));
        h.handle_key(key(KeyCode::Char('s')));
        assert!(matches!(h.view, View::Shape(..)));
        // A read already in flight cannot satisfy the return's later request.
        h.request_sessions();
        h.handle_key(key(KeyCode::Esc));
        gate.set(found(vec![sess("a", Stage::SeedDrafted, "")]));
        let version = h.discovery.version();
        gate.release(1);
        wait_until("older read to land", || {
            h.tick();
            h.discovery.version() > version
        });
        let View::Tend(tend) = &h.view else { panic!() };
        assert!(tend.detail && tend.want.is_some());
        assert_eq!(tend.current().unwrap().stage, Stage::GoalSaved);
        if leave_detail {
            h.handle_key(key(KeyCode::Esc));
        }
        gate.set(found(vec![sess("a", Stage::SeedUnchecked, "")]));
        gate.release(1);
        settle(&mut h);
        let View::Tend(tend) = &h.view else { panic!() };
        assert_eq!(tend.detail, !leave_detail, "a late read reopened detail");
        assert_eq!(tend.current().unwrap().stage, Stage::SeedUnchecked);
        assert!(tend.want.is_none());
        assert_eq!(gate.peak.load(SeqCst), 1);
    }
}

#[test]
fn landed_discovery_does_not_cancel_or_redirect_link_picking() {
    let tmp = tempfile::Builder::new()
        .prefix("tinkery-TEST-link-refresh-")
        .tempdir()
        .unwrap();
    let sessions = vec![
        sess("a", Stage::GoalSaved, ""),
        sess("b", Stage::GoalSaved, ""),
    ];
    let gate = Gate::new(found(sessions.clone()));
    gate.release(4);
    let mut h = home(gate.source());
    h.link_root = Ok(tmp.path().join("TEST-links"));
    h.open(Place::at(1).unwrap());
    settle(&mut h);
    h.handle_key(key(KeyCode::Char('l')));
    h.handle_key(key(KeyCode::Right));
    let mut newer = sessions;
    newer.reverse();
    gate.set(found(newer));
    h.discovery.request(h.sessions_root.as_deref().unwrap());
    settle(&mut h);
    let View::Tend(tend) = &h.view else { panic!() };
    assert!(tend.is_linking());
    assert_eq!(tend.current().unwrap().id.as_deref(), Some("b"));
    assert!(screen(&mut h).contains("goal a needs goal b"));
    h.handle_key(key(KeyCode::Enter));
    h.tick();
    let View::Tend(tend) = &h.view else { panic!() };
    assert!(!tend.is_linking());
    assert_eq!(tend.current().unwrap().id.as_deref(), Some("a"));
    assert!(tend.needs.read().into_links().unwrap().contains("a", "b"));
}

#[test]
fn receipt_detail_waits_for_a_read_that_can_contain_the_session() {
    let target = PathBuf::from("/TEST/new");
    let gate = Gate::new(found(vec![sess(
        "a",
        Stage::GoalSaved,
        "2026-10-09T10:00:00Z",
    )]));
    let mut h = home(gate.source());
    h.tick();
    wait_until("first read started", || gate.entered.load(SeqCst) == 1);
    h.open_tend_session(&target);
    assert!(screen(&mut h).contains("Reading your sessions"));
    // The read that began before the save lands without it: still waiting, not "unavailable".
    gate.release(1);
    wait_until("first read to land", || {
        h.tick();
        gate.finished.load(SeqCst) == 1 && gate.entered.load(SeqCst) == 2
    });
    let frame = screen(&mut h);
    assert!(frame.contains("Reading your sessions"), "{frame}");
    assert!(!frame.contains("unavailable"), "{frame}");
    gate.set(found(vec![
        sess("new", Stage::GoalSaved, "2026-10-10T10:00:00Z"),
        sess("a", Stage::GoalSaved, "2026-10-09T10:00:00Z"),
    ]));
    gate.release(1);
    settle(&mut h);
    let View::Tend(tend) = &h.view else { panic!() };
    assert!(tend.detail && tend.error.is_none());
    assert_eq!(tend.current().unwrap().path, target);
    assert_eq!(tend.current().unwrap().goal, "goal new");
}

#[test]
fn receipt_detail_for_a_session_a_complete_read_lacks_is_explicit() {
    let gate = Gate::new(found(vec![]));
    gate.release(4);
    let mut h = home(gate.source());
    h.open_tend_session(Path::new("/TEST/missing"));
    settle(&mut h);
    let View::Tend(tend) = &h.view else { panic!() };
    assert!(
        tend.error
            .as_ref()
            .unwrap()
            .contains("Saved session is unavailable")
    );
    assert!(tend.sessions.is_empty() && !tend.detail);
}

#[test]
fn a_late_receipt_target_does_not_resurrect_an_exited_visit() {
    let gate = Gate::new(found(vec![sess(
        "a",
        Stage::GoalSaved,
        "2026-10-09T10:00:00Z",
    )]));
    let mut h = home(gate.source());
    h.open_tend_session(Path::new("/TEST/a"));
    h.handle_key(key(KeyCode::Esc));
    assert!(matches!(h.view, View::Menu));
    h.handle_key(key(KeyCode::Char('4')));
    h.handle_key(key(KeyCode::Esc));
    gate.release(4);
    settle(&mut h);
    assert!(matches!(h.view, View::Menu));
    h.open(Place::at(1).unwrap());
    let View::Tend(tend) = &h.view else { panic!() };
    assert!(
        tend.want.is_none() && !tend.detail,
        "the old target is gone"
    );
}

#[test]
fn counts_are_exact_only_after_complete_current_reads() {
    let sessions = vec![
        sess("a", Stage::GoalSaved, "2026-10-09T10:00:00Z"),
        sess("b", Stage::SeedUnchecked, "2026-10-08T10:00:00Z"),
        sess("c", Stage::Completed, "2026-10-07T10:00:00Z"),
    ];
    let tmp = tempfile::Builder::new()
        .prefix("tinkery-TEST-count-notes-")
        .tempdir()
        .unwrap();
    let gate = Gate::new(found(sessions.clone()));
    gate.release(1);
    let mut h = home(gate.source());
    h.link_root = Ok(tmp.path().join("TEST-links"));
    assert_eq!(h.count(1), None, "not known before any read");
    settle(&mut h);
    assert_eq!(
        h.count(1),
        Some("2".into()),
        "ended sessions are not active"
    );
    // Partial evidence: valid records are kept but no number is claimed.
    gate.set(Ok(Found {
        sessions: sessions.clone(),
        unreadable: 1,
        failures: Vec::new(),
    }));
    gate.release(1);
    h.open(Place::at(1).unwrap());
    settle(&mut h);
    assert_eq!(h.count(1), None, "incomplete is not an exact result");
    let frame = screen(&mut h);
    assert!(
        frame.contains("1 saved record couldn't be read; this view may be missing sessions."),
        "{frame}"
    );
    assert!(
        frame.contains("goal a") && frame.contains("goal b"),
        "{frame}"
    );
    // A failed root is not "no sessions" and not a count.
    gate.set(Err("Cannot read /TEST: boom".into()));
    gate.release(1);
    h.open(Place::at(1).unwrap());
    settle(&mut h);
    assert_eq!(h.count(1), None);
    assert!(!screen(&mut h).contains("No saved sessions"));
    // Complete and empty is the only way to "no sessions".
    gate.set(found(vec![]));
    gate.release(1);
    h.open(Place::at(1).unwrap());
    settle(&mut h);
    assert!(screen(&mut h).contains("No saved sessions yet"));
    assert_eq!(h.count(1), None, "zero is never printed");
}

#[test]
fn only_unreadable_records_never_read_as_no_sessions() {
    let gate = Gate::new(Ok(Found {
        sessions: vec![],
        unreadable: 2,
        failures: Vec::new(),
    }));
    gate.release(2);
    let mut h = home(gate.source());
    h.open(Place::at(1).unwrap());
    settle(&mut h);
    let frame = screen(&mut h);
    assert!(
        frame.contains("2 saved records couldn't be read"),
        "{frame}"
    );
    assert!(!frame.contains("No saved sessions"), "{frame}");
}

#[test]
fn returning_from_think_and_reopening_tend_read_again_and_hide_a_stale_count() {
    let tmp = tempfile::tempdir().unwrap();
    write_session(tmp.path(), "first", "2026-10-09T10:00:00Z");
    let mut h = Home::new(
        Box::new(BrainDump::default),
        Ok(tmp.path().to_path_buf()),
        Ok(tmp.path().join("TEST-links")),
    );
    settle(&mut h);
    assert_eq!(h.count(1), Some("1".into()));
    h.open(Place::at(0).unwrap());
    write_session(tmp.path(), "saved-in-think", "2026-10-10T10:00:00Z");
    if let View::Think(dump) = &mut h.view {
        dump.handle_key(key(KeyCode::Esc));
    }
    h.tick();
    assert!(matches!(h.view, View::Menu));
    assert_eq!(
        h.count(1),
        None,
        "the old number is not shown while rereading"
    );
    settle(&mut h);
    assert_eq!(h.count(1), Some("2".into()), "count refreshed after saving");
    write_session(tmp.path(), "later", "2026-10-11T10:00:00Z");
    h.open(Place::at(1).unwrap());
    settle(&mut h);
    let View::Tend(tend) = &h.view else { panic!() };
    assert_eq!(tend.sessions.len(), 3, "deliberate reopening reads again");
    assert_eq!(tend.sessions[0].path.file_name().unwrap(), "later");
}

fn write_session(root: &Path, id: &str, updated: &str) {
    let dir = root.join(id);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("ledger.json"),
        json!({"status": "active", "goal": format!("goal {id}"), "updated_at": updated})
            .to_string(),
    )
    .unwrap();
}

#[test]
fn scan_reads_thousands_once_each_newest_first_with_exact_active_count() {
    let tmp = tempfile::tempdir().unwrap();
    let total = 3000;
    for i in 0..total {
        let dir = tmp.path().join(format!("s{i:05}"));
        fs::create_dir_all(&dir).unwrap();
        let status = if i % 5 == 0 { "completed" } else { "active" };
        fs::write(
            dir.join("ledger.json"),
            json!({"status": status, "goal": format!("goal {i}"),
                   "updated_at": format!("2026-10-{:02}T{:02}:{:02}:{:02}Z", 1 + i / 1440 % 28, i / 60 % 24, i % 60, 0)})
            .to_string(),
        )
        .unwrap();
    }
    let found = scan(tmp.path()).unwrap();
    assert!(found.complete());
    assert_eq!(found.sessions.len(), total);
    let unique: HashSet<_> = found.sessions.iter().map(|s| &s.path).collect();
    assert_eq!(unique.len(), total, "every session exactly once");
    let times: Vec<_> = found
        .sessions
        .iter()
        .map(|s| tend::timestamp(&s.updated).unwrap())
        .collect();
    assert!(times.windows(2).all(|w| w[0] >= w[1]), "newest first");
    let mut h = Home::new(
        Box::new(BrainDump::default),
        Ok(tmp.path().to_path_buf()),
        Ok(tmp.path().join("TEST-links")),
    );
    settle(&mut h);
    assert_eq!(h.count(1), Some((total - total / 5).to_string()));
}

#[test]
fn equal_timestamps_have_path_order_independent_of_creation_order() {
    let tmp = tempfile::Builder::new()
        .prefix("tinkery-TEST-sort-")
        .tempdir()
        .unwrap();
    for name in ["z", "a", "m"] {
        write_session(tmp.path(), name, "2026-10-09T10:00:00Z");
    }
    let found = scan(tmp.path()).unwrap();
    assert_eq!(
        found
            .sessions
            .iter()
            .map(|s| s.goal.as_str())
            .collect::<Vec<_>>(),
        ["goal a", "goal m", "goal z"]
    );
}

#[test]
fn shifted_graph_view_keys_work_but_modified_actions_stay_blocked() {
    for ch in ['H', 'J', 'K', 'L', '+', '=', '-'] {
        let mut plain = garden(7);
        let mut shifted = garden(7);
        let before = screen(&mut shifted);
        screen(&mut plain);
        plain.handle_key(key(KeyCode::Char(ch)));
        shifted.handle_key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::SHIFT));
        let after = screen(&mut shifted);
        assert_ne!(before, after, "{ch}");
        assert_eq!(screen(&mut plain), after, "{ch}");
        assert_eq!(selected(&shifted), 0);
    }
    let mut h = garden(7);
    let before = screen(&mut h);
    for modifiers in [
        KeyModifiers::CONTROL,
        KeyModifiers::ALT,
        KeyModifiers::SHIFT | KeyModifiers::CONTROL,
    ] {
        for ch in ['H', 'J', 'K', 'L', '+', '=', '-', 'l', 'c'] {
            h.handle_key(KeyEvent::new(KeyCode::Char(ch), modifiers));
            assert_eq!(screen(&mut h), before);
        }
    }
    for code in [KeyCode::Char('l'), KeyCode::Enter, KeyCode::Down] {
        h.handle_key(KeyEvent::new(code, KeyModifiers::SHIFT));
        assert_eq!(screen(&mut h), before);
    }
    assert!(h.take_copy_request().is_none());
}

#[test]
fn graph_selection_keys_reveal_the_goal_after_panning_it_offscreen() {
    let mut h = garden(7);
    screen(&mut h);
    for _ in 0..80 {
        h.handle_key(key(KeyCode::Char('H')));
    }
    assert!(!screen(&mut h).contains("› goal 0"));
    h.handle_key(key(KeyCode::Up)); // clamped at newest, but reveal it again
    assert!(screen(&mut h).contains("› goal 0"));
    for _ in 0..80 {
        h.handle_key(key(KeyCode::Char('H')));
    }
    h.handle_key(key(KeyCode::Down));
    assert!(screen(&mut h).contains("› goal 1"));
}

#[test]
fn scan_completeness_semantics() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    // Missing root: nothing has been saved yet. Complete and empty.
    assert_eq!(scan(&root.join("missing")), Ok(Found::default()));
    // Root that cannot be listed: failed, never empty.
    fs::write(root.join("a-file"), "x").unwrap();
    assert!(scan(&root.join("a-file")).is_err());
    // Mixed root: valid records survive; non-sessions are ignored; unreadable ones are counted.
    let sessions = root.join("sessions");
    write_session(&sessions, "good", "2026-10-09T10:00:00Z");
    fs::create_dir_all(sessions.join("empty-dir")).unwrap();
    fs::write(sessions.join("stray-file"), "x").unwrap();
    fs::create_dir_all(sessions.join("bad-json")).unwrap();
    fs::write(sessions.join("bad-json/ledger.json"), "not json").unwrap();
    fs::create_dir_all(sessions.join("no-status")).unwrap();
    fs::write(sessions.join("no-status/ledger.json"), "{}").unwrap();
    fs::create_dir_all(sessions.join("ledger-is-dir/ledger.json")).unwrap();
    fs::create_dir_all(sessions.join("bad-utf8")).unwrap();
    fs::write(sessions.join("bad-utf8/ledger.json"), [0xff, 0xfe]).unwrap();
    let found = scan(&sessions).unwrap();
    assert_eq!(found.sessions.len(), 1);
    assert_eq!(found.sessions[0].goal, "goal good");
    assert_eq!(
        found.unreadable, 4,
        "bad-json, no-status, ledger-is-dir, bad-utf8"
    );
    assert!(!found.complete());
    // Only genuinely absent records leave a root complete.
    let clean = root.join("clean");
    write_session(&clean, "x", "2026-10-09T10:00:00Z");
    fs::create_dir_all(clean.join("not-a-session")).unwrap();
    assert!(scan(&clean).unwrap().complete());
}

#[test]
fn unavailable_session_root_never_scans_or_claims_an_empty_list() {
    let reason = "HOME is unavailable; pass --seed-session-root PATH";
    let mut h = Home::new(
        Box::new(BrainDump::default),
        Err(reason.into()),
        Err("TEST links unavailable".into()),
    )
    .with_discovery_source(Arc::new(|_| panic!("No root must never reach a source")));
    h.tick();
    h.discover_for_snapshot(Duration::from_secs(1));
    assert!(!h.discovery.running());
    assert_eq!(h.count(1), None);
    for _ in 0..2 {
        h.open(Place::at(1).unwrap());
        h.tick();
        let View::Tend(tend) = &mut h.view else {
            panic!()
        };
        assert_eq!(tend.error.as_deref(), Some(reason));
        assert!(!tend.pending);
        tend.how = true;
        assert!(tend.help_text().contains("--seed-session-root PATH"));
        let frame = snapshot(80, 24, &mut h, false).unwrap();
        assert!(frame.contains("unavailable"), "{frame}");
        assert!(!frame.contains("No saved sessions"), "{frame}");
        assert!(!h.discovery.running());
    }
}

#[test]
fn links_destination_survives_menu_reopening_and_standalone_receipt_handoff() {
    let tmp = tempfile::Builder::new()
        .prefix("tinkery-TEST-link-wiring-")
        .tempdir()
        .unwrap();
    let sessions = tmp.path().join("TEST-sessions");
    write_session(&sessions, "goal", "2026-10-09T10:00:00Z");
    let links = tmp.path().join("TEST-links");
    fs::create_dir_all(&links).unwrap();
    let bytes = b"TEST deliberately unreadable; navigation must leave it alone";
    fs::write(links.join("needs.json"), bytes).unwrap();
    for receipt in [false, true] {
        let mut h = if receipt {
            Home::from_receipt(
                BrainDump::default(),
                &sessions.join("goal"),
                true,
                Ok(links.clone()),
                None,
            )
        } else {
            Home::new(
                Box::new(BrainDump::default),
                Ok(sessions.clone()),
                Ok(links.clone()),
            )
        };
        for _ in 0..2 {
            h.open(Place::at(1).unwrap());
            settle(&mut h);
            let View::Tend(tend) = &h.view else { panic!() };
            assert_eq!(tend.sessions.len(), 1);
            assert!(
                matches!(tend.needs.read(), tend::needs::Read::Unreadable(ref reason)
                if reason == "Cannot read links: needs.json is not valid JSON.")
            );
            assert_eq!(fs::read(links.join("needs.json")).unwrap(), bytes);
            h.handle_key(key(KeyCode::Esc));
        }
    }
}

#[test]
fn unavailable_linking_does_not_stop_tend_reading_sessions() {
    let reason = "TEST linking unavailable; pass --link-root PATH";
    let mut h = home(Arc::new(|_| {
        found(vec![sess("goal", Stage::GoalSaved, "")])
    }));
    h.link_root = Err(reason.into());
    h.open(Place::at(1).unwrap());
    settle(&mut h);
    let View::Tend(tend) = &h.view else { panic!() };
    assert_eq!(tend.sessions.len(), 1);
    assert!(tend.error.is_none());
    assert_eq!(
        tend.needs.read(),
        tend::needs::Read::Unreadable(reason.into())
    );
}

#[test]
fn the_menu_names_the_repository_once_and_says_nothing_outside_one() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("TEST-repo-here");
    std::fs::create_dir_all(root.join(".git")).unwrap();
    let mut h = home(Arc::new(|_| found(vec![]))).with_repo(crate::repo::Repo::enclosing(&root));
    let menu = screen(&mut h);
    let title = menu.lines().nth(1).unwrap();
    assert!(title.contains("tinkery  in TEST-repo-here"), "{title}");
    assert_eq!(menu.matches("TEST-repo-here").count(), 1, "{menu}");
    let mut plain = home(Arc::new(|_| found(vec![])));
    assert!(!screen(&mut plain).contains(" in "));
}

#[test]
fn page_keys_move_a_screen_of_goal_detail_less_one_row() {
    let mut h = garden(1);
    let View::Tend(tend) = &mut h.view else {
        panic!()
    };
    tend.sessions[0].goal = (0..200).map(|i| format!("TEST line {i}\n")).collect();
    tend.detail = true;
    screen(&mut h);
    let View::Tend(tend) = &h.view else { panic!() };
    let page = tend.page;
    assert!(page > 5, "{page} rows");
    h.handle_key(key(KeyCode::PageDown));
    let View::Tend(tend) = &h.view else { panic!() };
    assert_eq!(tend.scroll, page - 1);
    h.handle_key(key(KeyCode::PageUp));
    let View::Tend(tend) = &h.view else { panic!() };
    assert_eq!(tend.scroll, 0);
}
