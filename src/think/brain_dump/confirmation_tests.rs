//! Confirmation provenance controls: declared identity is not authentication or permission.
use super::readiness_tests::{app, parts};
use super::*;

#[test]
fn first_review_frame_names_the_current_read_requirement() {
    use std::time::{Duration, Instant};
    struct Host(bool);
    impl BoardHost for Host {
        fn reshape(&self, request: BoardRequest, _: &AtomicBool) -> Result<Guess, String> {
            Guess::decode(
                &serde_json::json!({"parts": {
                    "who":"I", "outcome":"keep a repair log",
                    "why": if self.0 { "Long TEST review. ".repeat(100) } else { "I forget repairs".into() },
                    "done_when":"I find an earlier repair"
                }}).to_string(),
                &request,
                &mut vec![],
            )
        }
    }
    for (width, height) in [(80, 24), (100, 30)] {
        for mono in [false, true] {
            for long in [false, true] {
                let root = tempfile::tempdir().unwrap();
                let mut app = BrainDump::with_host(Arc::new(Host(long)))
                    .with_sessions(Some(root.path().join("TEST-sessions")));
                snapshot(width, height, &mut app, mono).unwrap();
                app.paste("These exact TEST words stay mine.");
                app.handle_key(KeyEvent::new(KeyCode::F(2), KeyModifiers::NONE));
                let deadline = Instant::now() + Duration::from_secs(5);
                while app.running() && Instant::now() < deadline {
                    app.tick();
                    std::thread::sleep(Duration::from_millis(2));
                }
                assert!(!app.running());
                app.handle_key(KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE));
                app.handle_key(KeyEvent::new(KeyCode::Char('1'), KeyModifiers::NONE));
                // The first draw must use its freshly measured bound, not the initial unmeasured bound.
                let first = snapshot(width, height, &mut app, mono).unwrap();
                assert_eq!(
                    first.contains("PgDn: read to the end first."),
                    long,
                    "{first}"
                );
                assert_eq!(first.contains("type confirm"), !long, "{first}");
                if long {
                    app.handle_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE));
                    app.handle_key(KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE));
                    let help = snapshot(width, height, &mut app, mono).unwrap();
                    assert!(
                        help.split_whitespace()
                            .collect::<Vec<_>>()
                            .join(" ")
                            .contains("Esc closes help"),
                        "{help}"
                    );
                    assert!(!help.contains("Type confirm"), "{help}");
                    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
                    for _ in 0..200 {
                        app.handle_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE));
                    }
                    let end = snapshot(width, height, &mut app, mono).unwrap();
                    assert!(
                        end.contains("type confirm") && !end.contains("read to end"),
                        "{end}"
                    );
                    app.handle_key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE));
                    assert!(
                        snapshot(width, height, &mut app, mono)
                            .unwrap()
                            .contains("PgDn: read to the end first.")
                    );
                }
                assert_eq!(
                    app.conversation().sources()[0].text,
                    "These exact TEST words stay mine."
                );
                assert!(app.saved().is_none() && !root.path().join("TEST-sessions").exists());
            }
        }
    }
}
use crate::think_record::{BlastRadius, ConfirmationClass, Door, GoalConfirmation};

fn terminal(declaration: &str) -> GoalConfirmation {
    serde_json::from_value(serde_json::json!({
        "by": "terminal", "declaration": declaration,
        "action": "typed confirm at this terminal",
        "identity": "declaration is not verified identity"
    }))
    .unwrap()
}

#[test]
fn stacker_terminal_confirmation_keeps_declared_human_agent_and_unknown_distinct() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/stacker-think-confirm.json"
    ))
    .unwrap();
    assert_eq!(fixture["goal_confirmation"]["by"], "unrecorded");
    let expected_goal = concat!(
        "I have created a software version of stacker (https://en.wikipedia.org/wiki/Stacker_(arcade_game)).\n",
        "Why: i like the game and played it at dave and busters today and it was fun.\n",
        "Done when: matches the tink-arcade style and theme; has working sounds; ",
        "just works with the right mechanics as the game has."
    );
    for declaration in ["declared: human", "declared: agent", "not declared"] {
        let tmp = tempfile::Builder::new()
            .prefix("tinkery-TEST-confirmation-")
            .tempdir()
            .unwrap();
        let root = tmp.path().join("TEST-records");
        let candidate = fixture["checks"].as_array().unwrap().last().unwrap()["candidate"].clone();
        let replay = super::readiness_tests::app_with_source(
            candidate,
            fixture["turns"][0]["text"].as_str().unwrap(),
        );
        let mut a = BrainDump::with_host(Arc::new(Simulated))
            .with_sessions(Some(tmp.path().join("TEST-sessions")))
            .with_record_root(root.clone());
        a.paste(fixture["turns"][0]["text"].as_str().unwrap());
        a.submit();
        super::tests::settle(&mut a);
        a.conversation
            .set_board(replay.conversation.board().cloned());
        a.set_goal_confirmation(terminal(declaration)).unwrap();
        a.review_goal();
        let goal = a.handoff.review().unwrap().affirmation.goal.clone();
        assert_eq!(goal, expected_goal);
        let screen = snapshot(100, 30, &mut a, true).unwrap();
        assert!(
            screen.contains(&format!("You are confirming ({declaration})")),
            "{screen}"
        );
        assert!(
            screen.contains(&format!("{declaration} (not verified)")),
            "{screen}"
        );
        let review = a.handoff.save.review_mut().unwrap();
        review.scroll = review.max;
        for c in "confirm".chars() {
            a.goal_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
        }
        a.goal_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(a.handoff.saving());
        // Keep the temporary destination alive until the real save has finished.
        super::tests::settle(&mut a);
        let receipt = a.saved().expect("confirmed TEST goal was saved");
        assert_eq!(receipt.goal, goal);
        assert!(
            receipt
                .session
                .starts_with(tmp.path().canonicalize().unwrap())
        );
        let file = std::fs::read_dir(root)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let record: serde_json::Value =
            serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap();
        assert_eq!(
            record["goal_confirmation"],
            serde_json::to_value(terminal(declaration)).unwrap()
        );
        assert_ne!(record["goal_confirmation"]["by"], "operator");
        assert_eq!(record["goal"], goal);
        assert_eq!(record["turns"][0]["text"], fixture["turns"][0]["text"]);
    }
}

#[test]
fn terminal_declaration_is_session_frozen_but_never_sent_to_the_model() {
    for value in [Some("human"), Some("agent"), None] {
        let confirmation = GoalConfirmation::terminal(value).unwrap();
        let mut a = app(serde_json::json!({"parts": parts()}));
        a.set_goal_confirmation(confirmation.clone()).unwrap();
        assert_eq!(a.fresh_factory()().handoff.confirmation, confirmation);
        assert!(
            a.set_goal_confirmation(operator())
                .unwrap_err()
                .contains("frozen")
        );
        a.review_goal();
        assert!(
            a.set_goal_confirmation(GoalConfirmation::default())
                .is_err()
        );
        assert_eq!(
            a.handoff.review().unwrap().affirmation.confirmation,
            confirmation
        );
        a.goal_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        let wire = serde_json::to_string(&a.preview_send("TEST additional words").unwrap().request)
            .unwrap();
        assert!(!wire.contains("declaration"));
        assert!(!wire.contains("typed confirm at this terminal"));
        assert!(!wire.contains("verified identity"));
    }
    assert!(GoalConfirmation::terminal(Some("operator")).is_err());
}

#[test]
fn malformed_terminal_provenance_cannot_claim_verified_identity_or_an_operator() {
    for (field, value) in [
        ("declaration", "operator"),
        ("action", "operator typed confirm"),
        ("identity", "verified"),
    ] {
        let mut raw =
            serde_json::to_value(GoalConfirmation::terminal(Some("agent")).unwrap()).unwrap();
        raw[field] = value.into();
        let context: GoalConfirmation = serde_json::from_value(raw).unwrap();
        let mut a = app(serde_json::json!({"parts": parts()}));
        assert!(a.set_goal_confirmation(context).is_err());
        assert_eq!(a.handoff.confirmation, GoalConfirmation::default());
    }
}

fn class() -> ConfirmationClass {
    ConfirmationClass {
        door: Door::TwoWay,
        door_reason: "TEST throwaway goal; nothing runs".into(),
        blast_radius: BlastRadius::Small,
        blast_radius_reason: "TEST isolated goal only".into(),
    }
}
fn agent() -> GoalConfirmation {
    GoalConfirmation::Agent {
        agent: "TEST-agent".into(),
        authorizing_operator: "TEST-person".into(),
        source: "TEST explicit authorization for this goal only".into(),
        class: class(),
    }
}
fn operator() -> GoalConfirmation {
    GoalConfirmation::Operator {
        operator: "TEST-later-person".into(),
        source: "TEST unrelated later confirmation".into(),
        class: class(),
    }
}
fn declared() -> BrainDump {
    let mut a = app(serde_json::json!({"parts": parts()}));
    a.set_goal_confirmation(agent()).unwrap();
    a
}

#[test]
fn typing_confirmation_keeps_a_fully_read_goal_at_the_end() {
    for (w, h) in [(94, 51), (80, 24), (100, 30)] {
        for no_color in [false, true] {
            let mut context = agent();
            if let GoalConfirmation::Agent { source, .. } = &mut context {
                source.push_str(&" TEST permission covers this temporary goal only.".repeat(40));
            }
            let mut a = app(serde_json::json!({"parts": parts()}));
            a.set_goal_confirmation(context.clone()).unwrap();
            a.review_goal();
            snapshot(w, h, &mut a, no_color).unwrap();
            assert!(a.handoff.review().unwrap().max > 0);
            while a.handoff.review().unwrap().scroll < a.handoff.review().unwrap().max {
                a.handle_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE));
            }
            snapshot(w, h, &mut a, no_color).unwrap();
            for c in "confirm".chars() {
                a.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
                snapshot(w, h, &mut a, no_color).unwrap();
                let review = a.handoff.review().unwrap();
                assert_eq!(review.scroll, review.max, "{w}x{h}: typing moved the end");
                assert_eq!(review.affirmation.confirmation, context);
                assert!(!a.handoff.saving());
            }
            // A shorter wrong-word refusal must not move the end before correction.
            a.handle_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
            a.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
            a.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
            snapshot(w, h, &mut a, no_color).unwrap();
            assert!(
                a.notice
                    .contains("Type confirm, then Enter. Nothing saved.")
            );
            assert!(!a.handoff.saving());
            // Even a rejected paste clears the refusal and restores the instruction.
            a.paste("ignored\npaste");
            snapshot(w, h, &mut a, no_color).unwrap();
            let review = a.handoff.review().unwrap();
            assert_eq!(review.input, "confirn");
            assert_eq!(
                review.scroll, review.max,
                "{w}x{h}: refused paste moved the end"
            );
            a.handle_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
            a.handle_key(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE));
            snapshot(w, h, &mut a, no_color).unwrap();
            let review = a.handoff.review().unwrap();
            assert_eq!(
                review.scroll, review.max,
                "{w}x{h}: correcting moved the end"
            );
            // The terminal journey verifies saving after this redraw; no save starts here.
        }
    }
}

#[test]
fn hidden_review_keeps_input_and_read_progress_without_starting_a_save() {
    for (w, h) in [(40, 12), (79, 24), (79, 30), (80, 23)] {
        for read in [false, true] {
            let mut a = declared();
            if let GoalConfirmation::Agent { source, .. } = &mut a.handoff.confirmation {
                source.push_str(&" TEST scoped permission.".repeat(80));
            }
            a.review_goal();
            snapshot(100, 30, &mut a, true).unwrap();
            if read {
                while a.handoff.review().unwrap().scroll < a.handoff.review().unwrap().max {
                    a.handle_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE));
                }
            }
            a.paste("confirm");
            let review = a.handoff.review().unwrap();
            let (scroll, max) = (review.scroll, review.max);
            let goal = review.affirmation.goal.clone();
            let actor = review.affirmation.confirmation.clone();
            let original = a.conversation.sources()[0].text.clone();
            let back = a.screen.back_button;
            assert!(
                snapshot(w, h, &mut a, true)
                    .unwrap()
                    .contains("Need 80×24.")
            );
            for code in [
                KeyCode::PageDown,
                KeyCode::PageUp,
                KeyCode::Down,
                KeyCode::Up,
                KeyCode::Backspace,
                KeyCode::Char('x'),
                KeyCode::F(3),
                KeyCode::Enter,
            ] {
                a.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
                let review = a.handoff.review().unwrap();
                assert_eq!(
                    (review.scroll, review.max),
                    (scroll, max),
                    "hidden key {code:?} moved review"
                );
                assert_eq!(review.input, "confirm", "hidden key {code:?} changed input");
                assert!(!a.handoff.saving());
            }
            a.paste("hidden paste");
            a.handle_mouse(
                MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column: back.x,
                    row: back.y,
                    modifiers: KeyModifiers::NONE,
                },
                Rect::new(0, 0, w, h),
            );
            let review = a.handoff.review().unwrap();
            assert_eq!(review.input, "confirm");
            assert_eq!(review.affirmation.goal, goal);
            assert_eq!(review.affirmation.confirmation, actor);
            assert_eq!(a.conversation.sources()[0].text, original);
            snapshot(100, 30, &mut a, true).unwrap();
            let review = a.handoff.review().unwrap();
            assert_eq!((review.scroll, review.max), (scroll, max));
            if !read {
                a.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
                assert!(!a.handoff.saving());
                assert!(a.notice.starts_with("Read to the end first:"));
            }
            snapshot(w, h, &mut a, true).unwrap();
            a.handle_key(KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE));
            assert!(a.screen.help);
            a.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
            assert!(!a.screen.help && a.handoff.review().is_some());
            a.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
            assert!(a.handoff.review().is_none() && !a.handoff.saving());
        }
    }
}

#[test]
fn hidden_review_finishes_an_already_started_save_and_requested_exit() {
    for leave in [false, true] {
        let mut a = declared();
        a.review_goal();
        snapshot(100, 30, &mut a, true).unwrap();
        let goal = a.handoff.review().unwrap().affirmation.goal.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        a.handoff.begin(rx);
        snapshot(40, 12, &mut a, true).unwrap();
        if leave {
            a.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
            assert!(a.handoff.leaving() && !a.quitting());
        }
        let mut receipt = seed_me::Receipt::test("/tmp/tinkery-TEST-completed-save/session".into());
        receipt.goal = goal.clone();
        tx.send(Ok(receipt)).unwrap();
        a.tick();
        assert_eq!(a.saved().unwrap().goal, goal);
        assert!(!a.handoff.saving() && a.handoff.review().is_none());
        assert_eq!(a.quitting(), leave);
    }
}

#[test]
fn unspecified_confirmer_is_not_claimed_as_the_operator() {
    let mut a = app(serde_json::json!({"parts": parts()}));
    a.review_goal();
    let source = a.handoff.review().unwrap().affirmation.affirmed().source;
    assert!(!source.contains("operator typed confirm"), "{source}");
    assert!(source.contains("unrecorded"), "{source}");
    assert!(source.contains("goal only, not seed or implementation approval"));
}

#[test]
fn declared_agent_source_never_claims_an_operator_keypress_or_implementation_permission() {
    let mut a = declared();
    a.review_goal();
    let affirmation = &a.handoff.review().unwrap().affirmation;
    let saved = affirmation.affirmed();
    assert!(saved.source.contains("agent") && saved.source.contains("TEST-agent"));
    assert!(saved.source.contains("TEST-person") && saved.source.contains("two-way"));
    assert!(saved.source.contains("small") && saved.source.contains("TEST explicit authorization"));
    assert!(!saved.source.contains("operator typed confirm"));
    assert!(saved.source.contains("not seed or implementation approval"));
    assert_eq!(saved.goal, affirmation.goal);
    assert_eq!(saved.originals[0].text, a.conversation.sources()[0].text);
}

#[test]
fn bare_agent_declaration_saves_as_the_agent() {
    let tmp = tempfile::Builder::new()
        .prefix("tinkery-TEST-agent-confirmation-")
        .tempdir()
        .unwrap();
    let mut a = declared().with_sessions(Some(tmp.path().join("TEST-sessions")));
    a.set_goal_confirmation(GoalConfirmation::terminal(Some("agent")).unwrap())
        .unwrap();
    assert!(layout::how_text(&a).contains("Only typing confirm saves"));
    a.review_goal();
    let saved = a.handoff.review().unwrap().affirmation.affirmed();
    assert!(saved.source.contains("declared: agent"), "{}", saved.source);
    assert!(!saved.source.contains("declared: human"));
    let review = a.handoff.save.review_mut().unwrap();
    review.scroll = review.max;
    for c in "confirm".chars() {
        a.goal_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
    }
    a.goal_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(a.handoff.saving(), "{}", a.notice);
    super::tests::settle(&mut a);
    let receipt = a.saved().expect("confirmed TEST goal was saved");
    assert!(
        receipt
            .session
            .starts_with(tmp.path().canonicalize().unwrap())
    );
    let ledger: serde_json::Value =
        serde_json::from_slice(&std::fs::read(receipt.session.join("ledger.json")).unwrap())
            .unwrap();
    assert_eq!(ledger["operator"], "agent");
}

#[test]
fn new_visits_do_not_inherit_goal_authorization_and_review_cannot_be_relabelled() {
    let mut a = declared();
    let fresh = a.fresh_factory()();
    assert_eq!(
        fresh.handoff.confirmation,
        GoalConfirmation::terminal(Some("agent")).unwrap()
    );
    a.review_goal();
    assert!(
        a.set_goal_confirmation(operator())
            .err()
            .unwrap()
            .contains("before review")
    );
    assert_eq!(
        a.handoff.review().unwrap().affirmation.confirmation,
        agent()
    );
    assert_eq!(a.conversation.sources()[0].text, "An intact original");
}

#[test]
fn attribution_is_not_added_to_the_shaping_request() {
    let a = declared();
    let request = a
        .preview_send("TEST additional goal words")
        .unwrap()
        .request;
    let wire = serde_json::to_string(&request).unwrap();
    for metadata in ["TEST-agent", "TEST-person", "TEST explicit authorization"] {
        assert!(!wire.contains(metadata), "{wire}");
    }
}

#[test]
fn invalid_declared_agent_class_is_refused_by_the_think_api() {
    let mut context = agent();
    if let GoalConfirmation::Agent { class, .. } = &mut context {
        class.blast_radius = BlastRadius::Large;
    }
    let mut a = app(serde_json::json!({"parts": parts()}));
    let unsent = "  TEST unsent Café 👩‍💻\nexact words  ";
    a.paste(unsent);
    let error = a.set_goal_confirmation(context).unwrap_err();
    assert_eq!(a.input.text, unsent);
    assert_eq!(a.conversation.sources()[0].text, "An intact original");
    assert_eq!(a.handoff.confirmation, GoalConfirmation::default());
    assert!(error.contains("two-way") && error.contains("small"));
}

#[test]
fn a_receipt_records_the_frozen_agent_context_not_a_later_identity() {
    // Receipt injection tests record plumbing; PTYs exercise real ledger saving.
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("TEST-records");
    let mut a = declared().with_record_root(root.clone());
    a.review_goal();
    let goal = a.handoff.review().unwrap().affirmation.goal.clone();
    a.handoff.confirmation = operator();
    let path = tmp.path().join("TEST-sessions/one");
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(
        path.join("ledger.json"),
        serde_json::json!({
            "schema_version": 1, "session_id": "TEST-existing-id", "status": "active", "goal": goal,
        })
        .to_string(),
    )
    .unwrap();
    let mut receipt = seed_me::Receipt::test(path);
    receipt.goal = goal.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    a.handoff.begin(rx);
    tx.send(Ok(receipt)).unwrap();
    a.goal_tick();
    let file = std::fs::read_dir(&root)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let record: serde_json::Value = serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap();
    assert_eq!(
        record["goal_confirmation"],
        serde_json::to_value(agent()).unwrap()
    );
    assert_eq!(record["goal_confirmation"]["by"], "agent");
    assert_eq!(
        record["origin"], "simulated",
        "model origin is not the confirmer"
    );
    assert_eq!(record["goal"], goal);
    assert_eq!(a.conversation.sources()[0].text, "An intact original");
}
