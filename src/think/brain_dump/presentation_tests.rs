//! Reading context through submitted input, successful-output acknowledgment and confirmed files.
use super::*;
use std::{
    path::Path,
    time::{Duration, Instant},
};

fn settle(app: &mut BrainDump) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.running() && Instant::now() < deadline {
        app.tick();
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(!app.running());
}
fn app(root: &Path) -> BrainDump {
    BrainDump::with_host(Arc::new(Simulated))
        .with_sessions(Some(root.join("TEST-sessions")))
        .with_record_root(root.join("TEST-records"))
}
fn send(app: &mut BrainDump, text: &str) {
    app.paste(text);
    app.submit();
    settle(app);
}
fn paint(app: &mut BrainDump, acknowledge: bool) {
    snapshot(140, 42, app, true).unwrap();
    if acknowledge && let Some(presentation) = app.presentation() {
        app.presented(presentation);
    }
}
fn save(app: &mut BrainDump, root: &Path) -> serde_json::Value {
    app.review_goal();
    snapshot(160, 60, app, true).unwrap();
    app.paste("confirm");
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.saved().is_none() && Instant::now() < deadline {
        app.tick();
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(app.saved().is_some(), "{}", app.notice());
    let files: Vec<_> = std::fs::read_dir(root.join("TEST-records"))
        .unwrap()
        .collect();
    assert_eq!(files.len(), 1);
    serde_json::from_slice(&std::fs::read(files[0].as_ref().unwrap().path()).unwrap()).unwrap()
}
#[test]
fn presentation_context_connects_words_to_successful_output_not_applied_replies() {
    for waiting_behind_typing in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let mut app = app(root.path());
        send(&mut app, "  TEST original Café\r\n");
        paint(&mut app, true); // A
        paint(&mut app, true); // ordinary redraw must not duplicate A
        if waiting_behind_typing {
            app.paste("TEST answer");
            app.submit();
            app.paste("TEST refinement after the last output");
            settle(&mut app); // B is held while the unsent refinement stays in the input.
            assert!(app.notice().contains("Reply ready"));
            app.submit();
            settle(&mut app);
            // Answer the scope question raised for words added while the reading was stale.
            send(&mut app, "TEST yes, these added words belong to this goal");
        } else {
            send(&mut app, "TEST answer"); // B is applied but has not been presented.
            send(&mut app, "TEST refinement after the last output");
        }
        paint(&mut app, true); // C, not B, reaches output
        let record = save(&mut app, root.path());
        let events = record["reading_context"]["events"].as_array().unwrap();
        let kinds: Vec<_> = events.iter().map(|e| e["kind"].as_str().unwrap()).collect();
        let mut expected = vec!["submitted", "presented", "submitted", "submitted"];
        if waiting_behind_typing {
            expected.push("submitted");
        }
        expected.push("presented");
        assert_eq!(kinds, expected);
        assert_eq!(events[3]["source"], 3);
        assert!(events[1]["at"].as_str().unwrap().ends_with('Z'));
        assert!(
            events[1]["board"]["framings"][0]["text"]
                .as_str()
                .unwrap()
                .contains("Simulated")
        );
        assert_eq!(record["turns"][0]["text"], "  TEST original Café\r\n");
        let history = crate::think_record::read(
            &root.path().join("TEST-records"),
            &app.saved().unwrap().session,
            app.saved().unwrap().goal.as_str(),
        )
        .unwrap();
        let label = history
            .turns
            .iter()
            .filter_map(|turn| turn.event.source_label())
            .nth(2)
            .unwrap();
        let relation = format!("{label} · after reading 1");
        assert!(history.details.contains(&relation));
        assert!(history.sequence.contains(&relation));
    }
}
#[test]
fn hidden_and_unacknowledged_frames_do_not_establish_presentation() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app(root.path());
    send(&mut app, "TEST first words");
    paint(&mut app, false); // output was not acknowledged
    app.handle_key(KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE));
    paint(&mut app, true); // help covers the reading
    assert!(app.presentation().is_none());
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    snapshot(60, 15, &mut app, true).unwrap();
    assert!(app.presentation().is_none());
    assert!(!root.path().join("TEST-records").exists());
    send(&mut app, "TEST answer without a presented reading");
    let record = save(&mut app, root.path());
    let events = record["reading_context"]["events"].as_array().unwrap();
    assert_eq!(events.len(), 2);
    assert!(events.iter().all(|e| e["kind"] == "submitted"));
}

#[test]
fn scrolled_review_counts_the_visible_spelling_of_direction_controls() {
    struct Echo;
    impl BoardHost for Echo {
        fn reshape(&self, request: BoardRequest, cancelled: &AtomicBool) -> Result<Guess, String> {
            let outcome = request.sources[0].text.clone();
            let mut guess = Simulated.reshape(request, cancelled)?;
            guess.parts.outcome = Some(outcome);
            Ok(guess)
        }
    }
    let root = tempfile::tempdir().unwrap();
    let mut app =
        BrainDump::with_host(Arc::new(Echo)).with_sessions(Some(root.path().join("TEST-sessions")));
    send(
        &mut app,
        &format!("archive report{}cod.exe", "\u{202e}".repeat(160)),
    );
    app.review_goal();
    snapshot(80, 24, &mut app, true).unwrap();
    for _ in 0..2 {
        app.handle_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE));
    }
    let frame = snapshot(80, 24, &mut app, true).unwrap();
    assert!(
        frame.contains("\\u{202e}") && !frame.contains("You are confirming"),
        "{frame}"
    );
    assert!(
        app.presentation().is_some(),
        "The scrolled goal is visible."
    );
}

#[test]
fn partial_readings_distinguish_deferred_content_from_a_heading_and_long_question() {
    struct Partial(bool);
    impl BoardHost for Partial {
        fn reshape(&self, request: BoardRequest, cancelled: &AtomicBool) -> Result<Guess, String> {
            let initial = request.sources.len() == 1;
            let mut guess = Simulated.reshape(request, cancelled)?;
            if initial {
                guess.parts = GoalParts::default();
                guess.framings.clear();
                guess.outcome.clear();
                guess.questions[0].text = "TEST long unanswered question ".repeat(300);
                if self.0 {
                    guess
                        .deferred
                        .push("TEST delivery condition for the seed".into());
                }
            }
            Ok(guess)
        }
    }
    for deferred in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let mut app = BrainDump::with_host(Arc::new(Partial(deferred)))
            .with_sessions(Some(root.path().join("TEST-sessions")))
            .with_record_root(root.path().join("TEST-records"));
        send(&mut app, "TEST incomplete first words");
        for _ in 0..6 {
            app.handle_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE));
        }
        snapshot(140, 42, &mut app, true).unwrap();
        assert_eq!(
            app.presentation().is_some(),
            deferred,
            "The heading alone is not a reading, even when the question scrolls."
        );
        if let Some(presentation) = app.presentation() {
            app.presented(presentation);
        }
        send(&mut app, "TEST answer makes the goal saveable");
        let record = save(&mut app, root.path());
        let events = record["reading_context"]["events"].as_array().unwrap();
        assert_eq!(events.len(), if deferred { 3 } else { 2 });
        if deferred {
            assert_eq!(
                events[1]["board"]["deferred"][0],
                "TEST delivery condition for the seed"
            );
        }
    }
}

#[test]
fn rejected_reply_retains_context_and_identical_new_boards_each_get_one_presentation() {
    struct Refusing;
    impl BoardHost for Refusing {
        fn reshape(&self, request: BoardRequest, cancelled: &AtomicBool) -> Result<Guess, String> {
            if request
                .sources
                .last()
                .is_some_and(|s| s.text == "TEST rejected answer")
            {
                return Err("TEST malformed model reply".into());
            }
            Simulated.reshape(request, cancelled)
        }
    }
    let root = tempfile::tempdir().unwrap();
    let mut app = BrainDump::with_host(Arc::new(Refusing))
        .with_sessions(Some(root.path().join("TEST-sessions")))
        .with_record_root(root.path().join("TEST-records"));
    send(&mut app, "TEST original");
    paint(&mut app, true);
    send(&mut app, "TEST rejected answer");
    paint(&mut app, true);
    assert!(app.notice().contains("Reshape failed"));
    send(&mut app, "TEST recovery");
    send(&mut app, "TEST yes, the recovery belongs to this goal");
    paint(&mut app, true);
    send(&mut app, "TEST same reading again");
    paint(&mut app, true);
    let record = save(&mut app, root.path());
    let events = record["reading_context"]["events"].as_array().unwrap();
    assert_eq!(
        events
            .iter()
            .map(|e| e["kind"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [
            "submitted",
            "presented",
            "submitted",
            "submitted",
            "submitted",
            "presented",
            "submitted",
            "presented"
        ]
    );
    assert_eq!(events[5]["board"], events[7]["board"]);
}
