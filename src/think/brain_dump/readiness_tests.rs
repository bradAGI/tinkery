use super::tests::settle;
use super::*;

#[test]
fn review_row_tracks_public_edit_reading_held_reply_and_retry_transitions() {
    use std::{
        sync::atomic::{AtomicUsize, Ordering},
        time::{Duration, Instant},
    };
    struct Host {
        calls: AtomicUsize,
        release: AtomicBool,
        fail: AtomicBool,
    }
    impl BoardHost for Host {
        fn reshape(&self, request: BoardRequest, cancelled: &AtomicBool) -> Result<Guess, String> {
            let call = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
            while call == 2 && !self.release.load(Ordering::SeqCst) {
                if cancelled.load(Ordering::Relaxed) {
                    return Err("TEST cancelled".into());
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            if self.fail.load(Ordering::SeqCst) {
                return Err("TEST reading failed".into());
            }
            Guess::decode(
                &serde_json::json!({"parts":{"who":"I", "outcome":"keep a repair log",
                    "why":"I forget repairs", "done_when":"I find earlier attempts"}})
                .to_string(),
                &request,
                &mut vec![],
            )
        }
    }
    fn key(app: &mut BrainDump, code: KeyCode) {
        app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
    }
    fn finish(app: &mut BrainDump) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while app.running() && Instant::now() < deadline {
            app.tick();
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(!app.running());
    }
    fn offered(app: &mut BrainDump, w: u16, h: u16, mono: bool) -> bool {
        let instruction = super::goal_tests::right_text(&snapshot(w, h, app, mono).unwrap(), w);
        key(app, KeyCode::F(1));
        let help = snapshot(w, h, app, mono).unwrap();
        let offered = help.contains("1  Review the goal");
        assert_eq!(
            instruction.contains("Ready for your review. F1, then 1: Review the goal."),
            offered,
            "{instruction}\n{help}"
        );
        let help_words = super::goal_tests::right_text(&help, w);
        assert!(!help_words.contains("F1, then 1"), "{help_words}");
        offered
    }
    for (w, h) in [(80, 24), (100, 30)] {
        for mono in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let host = Arc::new(Host {
                calls: AtomicUsize::new(0),
                release: AtomicBool::new(false),
                fail: AtomicBool::new(false),
            });
            let mut app = BrainDump::with_host(host.clone())
                .with_sessions(Some(root.path().join("TEST-sessions")));
            let words = "My exact original repair-log words.";
            snapshot(w, h, &mut app, mono).unwrap();
            app.paste(words);
            key(&mut app, KeyCode::F(2));
            finish(&mut app);
            assert!(offered(&mut app, w, h, mono));
            key(&mut app, KeyCode::Char('1'));
            assert!(
                snapshot(w, h, &mut app, mono)
                    .unwrap()
                    .contains("You are confirming")
            );
            assert_eq!(host.calls.load(Ordering::SeqCst), 1);
            assert!(app.saved().is_none() && !root.path().join("TEST-sessions").exists());
            key(&mut app, KeyCode::Esc);

            app.paste("my unsent answer");
            assert!(
                !offered(&mut app, w, h, mono),
                "unsent words must hide Review"
            );
            key(&mut app, KeyCode::F(2));
            assert!(!app.running() && host.calls.load(Ordering::SeqCst) == 1);
            assert!(
                snapshot(w, h, &mut app, mono)
                    .unwrap()
                    .contains("my unsent answer")
            );
            app.review_goal();
            assert_eq!(
                app.notice(),
                "Send (F2) or clear your unsent words, then review."
            );
            key(&mut app, KeyCode::Esc);
            app.handle_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
            assert!(offered(&mut app, w, h, mono));
            key(&mut app, KeyCode::Esc);

            app.paste("Read this refinement.");
            key(&mut app, KeyCode::F(2));
            assert!(app.running());
            assert!(!offered(&mut app, w, h, mono));
            app.review_goal();
            assert_eq!(
                app.notice(),
                "Still thinking; review once the reading is back."
            );
            key(&mut app, KeyCode::Esc);
            app.paste("keep this local draft");
            host.release.store(true, Ordering::SeqCst);
            finish(&mut app);
            assert!(app.notice().contains("Reply ready"));
            assert!(
                snapshot(w, h, &mut app, mono)
                    .unwrap()
                    .contains("keep this local draft")
            );
            app.handle_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
            assert!(
                !offered(&mut app, w, h, mono),
                "a held reply blocks even after clearing typing"
            );
            app.review_goal();
            assert_eq!(
                app.notice(),
                "A reply is waiting; F2 applies it, then review."
            );
            key(&mut app, KeyCode::Esc);
            key(&mut app, KeyCode::F(2));
            finish(&mut app);
            assert_eq!(
                host.calls.load(Ordering::SeqCst),
                2,
                "applying a held reply must not send again"
            );
            assert!(offered(&mut app, w, h, mono));
            key(&mut app, KeyCode::Esc);

            host.fail.store(true, Ordering::SeqCst);
            app.paste("Keep this submitted refinement too.");
            key(&mut app, KeyCode::F(2));
            finish(&mut app);
            assert!(!app.conversation().caught_up());
            assert!(
                !offered(&mut app, w, h, mono),
                "an unapplied reading hides Review"
            );
            app.review_goal();
            assert_eq!(
                app.notice(),
                "The reading is behind your words; F2 retries, then review."
            );
            key(&mut app, KeyCode::Esc);
            host.fail.store(false, Ordering::SeqCst);
            key(&mut app, KeyCode::F(2));
            finish(&mut app);
            assert!(offered(&mut app, w, h, mono));
            key(&mut app, KeyCode::Char('1'));
            assert!(
                snapshot(w, h, &mut app, mono)
                    .unwrap()
                    .contains("You are confirming")
            );
            assert_eq!(app.conversation().sources()[0].text, words);
            assert_eq!(
                app.conversation()
                    .sources()
                    .iter()
                    .map(|source| source.text.as_str())
                    .collect::<Vec<_>>(),
                [
                    words,
                    "Read this refinement.",
                    "Keep this submitted refinement too."
                ]
            );
            assert_eq!(host.calls.load(Ordering::SeqCst), 4);
            assert!(app.saved().is_none() && !root.path().join("TEST-sessions").exists());
        }
    }
}

pub(super) fn parts() -> GoalParts {
    GoalParts {
        who: Some("I".into()),
        outcome: Some("tell whether yesterday went well".into()),
        when: Some("each morning".into()),
        why: Some("the report is hard to act on".into()),
        done_when: Some("useful changes per hour are better than my usual week".into()),
        must: vec![],
        must_not: vec!["no targets; no dashboard; cost only when it's way off".into()],
    }
}
pub(super) fn app(raw: serde_json::Value) -> BrainDump {
    app_with_source(raw, "An intact original")
}
pub(super) fn app_with_source(raw: serde_json::Value, source: &str) -> BrainDump {
    let mut a = BrainDump::with_host(Arc::new(Simulated)).with_sessions(None);
    a.conversation.sources_mut().push(Source {
        id: 1,
        text: source.into(),
        in_reply_to: None,
    });
    a.conversation.set_applied(1);
    let r = request(&a);
    let g = Guess::decode(&raw.to_string(), &r, &mut vec![]).unwrap();
    a.conversation
        .set_board(Some(board::Board::verify(g, &r).unwrap()));
    a
}
fn request(a: &BrainDump) -> BoardRequest {
    BoardRequest {
        ask_counts: a.conversation.ask_counts().clone(),
        sources: a.conversation.sources().to_vec(),
        fragments: vec![],
        previous: a.conversation.board().map(|g| g.wire()),
        skipped: a.conversation.skipped().to_vec(),
        answered: a
            .conversation
            .sources()
            .iter()
            .filter_map(|s| s.in_reply_to.clone())
            .collect(),
        settled: a.conversation.settled().to_vec(),
        layout: vec![],
    }
}
#[test]
fn why_and_copy_label_success_criteria_without_claiming_experience_is_missing() {
    const EXPERIENCE: &str = "the approachable feel of RollerCoaster Tycoon";
    const ORIGINAL: &str = "A city healthcare game with the approachable feel of RollerCoaster Tycoon.\r\nKeep clinics, hospitals, providers and budgeting.";
    for criteria in [None, Some("I can allocate funds across providers.")] {
        let raw = serde_json::json!({"parts": {
            "who": "I",
            "outcome": format!("manage citywide healthcare with {EXPERIENCE}"),
            "why": "Population needs and limited budgets interact",
            "done_when": criteria
        }});
        let mut a = app_with_source(raw, ORIGINAL);
        a.paste("Unsent café answer");
        let goal = a.conversation.board().unwrap().goal().unwrap().to_owned();
        let expected = criteria.unwrap_or("Not yet captured");
        let details = a.details_text();
        let copied = a.copy_text();
        assert!(
            details.contains(&format!("Success criteria\n{expected}")),
            "{details}"
        );
        assert!(
            copied.contains(&format!("## Success criteria / proposed\n\n{expected}")),
            "{copied}"
        );
        assert!(copied.contains(EXPERIENCE) && copied.contains(ORIGINAL));
        for text in [&details, &copied] {
            assert!(!text.contains("Desired experience") && !text.contains("Not supplied"));
        }
        super::tests::choose(&mut a, "Why");
        for (w, h) in [(80, 24), (100, 30), (82, 24)] {
            for mono in [false, true] {
                let screen = snapshot(w, h, &mut a, mono).unwrap();
                let right = super::goal_tests::right_text(&screen, w);
                assert!(right.contains("Success criteria"), "{screen}");
                assert!(right.contains(expected), "{screen}");
                assert!(!right.contains("Not supplied") && !right.contains("Desired experience"));
            }
        }
        assert_eq!(a.conversation.sources()[0].text, ORIGINAL);
        assert_eq!(a.conversation.board().unwrap().goal(), Some(goal.as_str()));
        assert_eq!(a.input.text, "Unsent café answer");
        assert!(!a.handoff.saving() && a.saved().is_none() && !a.running());
    }
}

#[test]
fn rejected_summary_labels_raw_success_criteria_without_changing_evidence() {
    let raw = serde_json::json!({
        "parts": {"done_when": "Budget consequences are visible."},
        "outcome": "Legacy freeform outcome, not criteria."
    })
    .to_string();
    let mut decision = checks::Decision::new(
        "structure",
        "blocking",
        "reject",
        "TEST rejected reply".into(),
        std::time::Instant::now(),
    );
    decision.raw = Some(raw.clone());
    let history = vec![serde_json::to_string(&decision).unwrap()];
    let summary = checks::why(&history);
    assert!(
        summary.contains("Success criteria (unaccepted)\nBudget consequences are visible."),
        "{summary}"
    );
    assert!(
        !summary.contains("Desired experience") && !summary.contains("Legacy freeform outcome")
    );
    assert!(summary.contains("Rejected attempt — not confirmable"));
    assert!(summary.contains("this summary is not an accepted board"));
    assert_eq!(checks::recorded(&history)[0]["raw"], raw);
}

#[test]
fn beneficiary_and_labelled_lines_preserve_criteria_and_constraints() {
    assert_eq!(
        parts().compose().unwrap(),
        "I tell whether yesterday went well, each morning.\nWhy: the report is hard to act on.\nDone when: useful changes per hour are better than my usual week.\nMust not: no targets; no dashboard; cost only when it's way off."
    );
    let p = GoalParts {
        who: Some("learners".into()),
        outcome: Some("understand why they got a question wrong".into()),
        must: vec!["No gamification; It must work on phones".into()],
        ..Default::default()
    };
    assert_eq!(
        p.compose().unwrap(),
        "Learners understand why they got a question wrong.\nMust: No gamification; It must work on phones."
    );
    assert_eq!(
        GoalParts {
            outcome: Some("understand".into()),
            ..Default::default()
        }
        .compose()
        .as_deref(),
        Some("Understand.")
    );
}
#[test]
fn constraint_items_are_one_line_and_quotes_decimals_unicode_survive() {
    let p = GoalParts {
        who: Some("café learners 👩‍💻".into()),
        outcome: Some("read 2.5 hours".into()),
        must_not: vec!["No dashboard. No targets; Showing \"same. Words\" differently".into()],
        ..Default::default()
    };
    assert_eq!(
        p.compose().unwrap(),
        "Café learners 👩‍💻 read 2.5 hours.\nMust not: No dashboard. No targets; Showing \"same. Words\" differently."
    );
}
#[test]
fn supplied_can_is_not_duplicated_in_the_goal_line() {
    for outcome in ["can coordinate a handover", "can can coordinate a handover"] {
        let p = GoalParts {
            who: Some("I".into()),
            outcome: Some(outcome.into()),
            ..Default::default()
        };
        assert_eq!(p.compose().as_deref(), Some("I can coordinate a handover."));
    }
}
#[test]
fn every_combination_omits_missing_lines_without_inventing_a_beneficiary() {
    let full = GoalParts {
        must: vec!["unchanged originals".into()],
        ..parts()
    };
    for mask in 0..128 {
        let mut v = serde_json::to_value(&full).unwrap();
        for (i, p) in Part::ALL.into_iter().enumerate() {
            if mask & (1 << i) == 0 {
                v[p.name()] = if matches!(p, Part::Must | Part::MustNot) {
                    serde_json::json!([])
                } else {
                    serde_json::Value::Null
                };
            }
        }
        let partial: GoalParts = serde_json::from_value(v).unwrap();
        let text = partial.compose().unwrap_or_default();
        assert!(
            !text.contains("Not supplied")
                && !text.contains("can .")
                && !text.starts_with([',', ';'])
        );
        assert_eq!(
            text.contains("I tell"),
            partial.get(Part::Who).is_some() && partial.get(Part::Outcome).is_some()
        );
        assert_eq!(
            partial.goal_line().is_some(),
            partial.get(Part::Outcome).is_some()
        );
        if partial.get(Part::Who).is_none() && partial.get(Part::Outcome).is_some() {
            assert!(text.starts_with("Tell whether yesterday went well"));
        }
        for p in [Part::Why, Part::DoneWhen, Part::Must, Part::MustNot] {
            assert_eq!(
                text.contains(&format!("{}:", p.label())),
                partial.get(p).is_some()
            );
        }
    }
}
#[test]
fn readiness_requires_four_parts_optional_constraints_only_block_when_open() {
    let mut a = app(serde_json::json!({"parts":parts()}));
    assert!(a.goal_ready());
    for p in Part::ALL {
        let mut value = serde_json::to_value(parts()).unwrap();
        value[p.name()] = serde_json::Value::Null;
        let mut partial = app(serde_json::json!({"parts":value}));
        assert_eq!(partial.goal_ready(), !Part::REQUIRED.contains(&p));
        assert_eq!(
            snapshot(160, 40, &mut partial, false)
                .unwrap()
                .contains("Ready for your review"),
            !Part::REQUIRED.contains(&p)
        );
        let mut vague = app(serde_json::json!({"parts":parts(),"open":[p]}));
        assert!(!vague.goal_ready());
        vague.review_goal();
        assert!(
            vague
                .handoff
                .review()
                .unwrap()
                .affirmation
                .unresolved_notes
                .iter()
                .any(|s| s.contains(p.label()))
        );
    }
    a.conversation.set_scope_pending(Some((
        1,
        Question {
            suggestion: None,
            target: None,
            id: "scope-addition-1".into(),
            text: "Scope?".into(),
        },
    )));
    assert!(!a.goal_ready());
    a.conversation.set_scope_pending(None);
    a.conversation.skipped_mut().push(Question {
        suggestion: None,
        target: Some(Part::Must),
        id: "skip".into(),
        text: "Keep what?".into(),
    });
    assert!(!a.goal_ready());
}
#[test]
fn coarse_criteria_are_present_and_empty_queue_is_not_completion() {
    let mut a = app(serde_json::json!({"parts":parts()}));
    assert!(a.goal_ready());
    a.review_goal();
    assert_eq!(
        a.handoff.review().unwrap().affirmation.outcome,
        parts().done_when.unwrap()
    );
    let mut blank = app(serde_json::json!({"parts":{}}));
    assert!(!blank.goal_ready());
    blank.review_goal();
    assert!(blank.handoff.review().is_none());
}
#[test]
fn review_freezes_the_displayed_block_not_a_recomposition() {
    // Artificial state (#76): ordinary verification never shows a block that differs from
    // its parts. Review must freeze what was shown, whatever the parts would compose to.
    let mut a = app(serde_json::json!({"parts":parts()}));
    let shown = "I keep field notes, before dinner.\nWhy: a calm check.";
    let board = a
        .conversation
        .board()
        .unwrap()
        .clone()
        .with_displayed_goal(shown);
    a.conversation.set_board(Some(board));
    assert!(a.copy_text().contains(shown));
    a.review_goal();
    assert_eq!(a.handoff.review().unwrap().affirmation.goal, shown);
}
#[test]
fn quiet_review_freezes_exact_block_and_seed_mapping_without_duplicate_lines() {
    for (w, h) in [(80, 24), (100, 30), (160, 40), (320, 40)] {
        let mut a = app(serde_json::json!({"parts":parts(),"open":["why"]}));
        let reading = a.conversation.board().unwrap().goal().unwrap().to_owned();
        assert!(a.copy_text().contains(&reading));
        a.review_goal();
        let frozen = a.handoff.review().unwrap();
        assert_eq!(frozen.affirmation.goal, reading);
        assert_eq!(frozen.affirmation.parts, parts());
        assert_eq!(frozen.affirmation.outcome, parts().done_when.unwrap());
        let mut seen = String::new();
        for _ in 0..12 {
            seen.push_str(&snapshot(w, h, &mut a, false).unwrap());
            a.handle_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE));
        }
        for term in [
            "You are confirming",
            "Done when:",
            "Must not:",
            "Still open",
            "type confirm",
        ] {
            assert!(seen.contains(term), "{term} at {w}x{h}");
        }
        assert!(!seen.contains("Not supplied") && !seen.contains("Doesn’t fit yet"));
        super::tests::amend_board(&mut a, |g| g.parts.outcome = Some("different".into()));
        assert_eq!(a.handoff.review().unwrap().affirmation.goal, reading);
        assert_eq!(a.handoff.review().unwrap().affirmation.parts, parts());
        assert!(!a.handoff.saving());
    }
    let mut partial = app(
        serde_json::json!({"parts":{"who":"Learners","outcome":"learn","must_not":"new targets"}}),
    );
    partial.review_goal();
    assert_eq!(
        partial
            .handoff
            .review()
            .unwrap()
            .affirmation
            .unresolved_notes
            .len()
            + partial
                .handoff
                .review()
                .unwrap()
                .affirmation
                .unresolved
                .len(),
        2
    );
}
struct Repeated;
impl BoardHost for Repeated {
    fn reshape(&self, r: BoardRequest, _: &AtomicBool) -> Result<Guess, String> {
        Guess::decode(&serde_json::json!({"parts":parts(),"open":["done_when"],"questions":[{"target":"done_when","id":format!("done-{}",r.sources.len()),"text":format!("Which check {}?",r.sources.len())}]}).to_string(),&r,&mut vec![])
    }
}
#[test]
fn at_most_two_asks_then_the_part_stays_open_for_seed() {
    let mut a = BrainDump::with_host(Arc::new(Repeated));
    a.paste("original");
    a.submit();
    settle(&mut a);
    assert_eq!(
        a.conversation.focused_question().unwrap().text,
        "Which check 1?"
    );
    a.paste("first answer");
    a.submit();
    settle(&mut a);
    assert_eq!(
        a.conversation.focused_question().unwrap().text,
        "Which check 2?"
    );
    a.paste("second answer");
    a.submit();
    settle(&mut a);
    assert!(a.conversation.focused_question().is_none());
    assert!(!a.goal_ready());
    assert_eq!(a.conversation.open_parts(), [Part::DoneWhen]);
}
#[test]
fn skipped_target_blocks_renamed_questions_and_undo_is_not_another_ask() {
    let mut a = BrainDump::with_host(Arc::new(Repeated));
    a.paste("original");
    a.submit();
    settle(&mut a);
    super::tests::choose(&mut a, "Skip this question");
    assert!(!a.goal_ready());
    a.undo_skip();
    assert!(a.conversation.focused_question().is_some());
    assert_eq!(a.conversation.ask_counts()[&Part::DoneWhen], 1);
    super::tests::choose(&mut a, "Skip this question");
    let r = request(&a);
    let raw = serde_json::json!({"parts":parts(),"open":["done_when"],"questions":[{"target":"done_when","id":"renamed","text":"Reworded check?"}]});
    let g = Guess::decode(&raw.to_string(), &r, &mut vec![]).unwrap();
    assert!(g.questions.is_empty());
    assert!(g.open.contains(&Part::DoneWhen));
    a.apply_result(r, Ok(g));
    assert!(a.conversation.focused_question().is_none());
    assert!(!a.goal_ready());
}
#[test]
fn delayed_results_use_current_counts_and_skipped_targets() {
    let mut a = app(serde_json::json!({"parts":parts()}));
    let r = request(&a);
    let g = Repeated
        .reshape(r.clone(), &AtomicBool::new(false))
        .unwrap();
    a.conversation.ask_counts_mut().insert(Part::DoneWhen, 2);
    a.apply_result(r.clone(), Ok(g));
    assert!(a.conversation.focused_question().is_none());
    assert!(!a.goal_ready());
    a.conversation.ask_counts_mut().clear();
    a.conversation.skipped_mut().push(Question {
        suggestion: None,
        target: Some(Part::DoneWhen),
        id: "local-skip".into(),
        text: "Skipped check".into(),
    });
    let g = Repeated
        .reshape(r.clone(), &AtomicBool::new(false))
        .unwrap();
    a.apply_result(r, Ok(g));
    assert!(a.conversation.focused_question().is_none());
    assert!(!a.goal_ready());
}
#[test]
fn unsent_words_and_unknown_fields_never_grant_authority() {
    let mut a = app(
        serde_json::json!({"parts":parts(),"confirmed":true,"settled":[{}],"open":["unknown"]}),
    );
    assert!(a.goal_ready());
    assert!(a.conversation.settled().is_empty() && a.handoff.receipt().is_none());
    a.paste("new local words");
    assert!(!a.goal_ready());
    a.review_goal();
    assert!(a.handoff.review().is_none());
}
#[test]
fn trailing_commas_normalize_only_outside_strings_and_span_guards_remain() {
    let mut a = app(serde_json::json!({"parts":parts()}));
    let r = request(&a);
    let mut changes = vec![];
    let g = Guess::decode(
        r#"{"parts":{"outcome":"retain literal ,} and \\\"quotes\\\"",},"open":["done_when",],}"#,
        &r,
        &mut changes,
    )
    .unwrap();
    assert_eq!(
        g.parts.outcome.as_deref(),
        Some("retain literal ,} and \\\"quotes\\\"")
    );
    assert!(changes.iter().any(|s| s.contains("trailing JSON")));
    a.apply_result(r.clone(), Ok(g));
    assert!(a.conversation.board().is_some());
    for raw in [
        r#"{"parts":{"outcome":"a"} "open":[],}"#,
        r#"{"supports":[{"source":99,"quote":"An intact original",},],}"#,
    ] {
        assert!(Guess::decode(raw, &r, &mut vec![]).is_err());
    }
}
#[derive(Default)]
struct Refinements(std::sync::Mutex<Vec<BoardRequest>>);
impl BoardHost for Refinements {
    fn reshape(&self, r: BoardRequest, _: &AtomicBool) -> Result<Guess, String> {
        self.0.lock().unwrap().push(r.clone());
        let mut p = parts();
        if r.sources.len() > 1 {
            p.done_when = Some(r.sources.last().unwrap().text.clone());
            p.must_not = vec!["no targets".into()];
        }
        Guess::decode(&serde_json::json!({"parts":p}).to_string(), &r, &mut vec![])
    }
}
#[test]
fn typing_after_ready_refines_goal_with_verbatim_reply_and_explicit_f2() {
    let host = Arc::new(Refinements::default());
    let mut a = BrainDump::with_host(host.clone());
    a.paste("original");
    a.submit();
    settle(&mut a);
    assert!(a.goal_ready());
    a.paste("better than my usual week. no targets");
    assert_eq!(host.0.lock().unwrap().len(), 1);
    assert!(!a.goal_ready());
    a.submit();
    settle(&mut a);
    assert_eq!(host.0.lock().unwrap().len(), 2);
    assert!(a.conversation.scope_pending().is_none());
    assert_eq!(
        a.conversation.sources()[1].text,
        "better than my usual week. no targets"
    );
    let replied_to = a.conversation.sources()[1].in_reply_to.clone().unwrap();
    assert!(replied_to.starts_with("goal-refinement-"), "{replied_to}");
    assert_eq!(a.conversation.settled()[0].question.id, replied_to);
    assert!(a.goal_ready());
    assert!(
        a.conversation
            .board()
            .unwrap()
            .goal()
            .unwrap()
            .contains("usual week")
    );
    assert!(a.handoff.receipt().is_none());
}
#[test]
fn open_without_question_refines_but_explicit_add_more_keeps_scope_question() {
    let mut a = app(serde_json::json!({"parts":parts(),"open":["why"]}));
    a.paste("new words");
    a.submit();
    assert!(a.conversation.scope_pending().is_none());
    let replied_to = a
        .conversation
        .sources()
        .last()
        .unwrap()
        .in_reply_to
        .clone()
        .unwrap();
    assert!(replied_to.starts_with("goal-refinement-"), "{replied_to}");
    settle(&mut a);
    let mut a = app(serde_json::json!({"parts":parts()}));
    a.add_more = true;
    a.paste("separate words");
    a.submit();
    assert!(a.conversation.scope_pending().is_some());
}
#[test]
fn model_cannot_impersonate_owned_goal_refinement_history() {
    let a = app(serde_json::json!({"parts":parts()}));
    let r = request(&a);
    assert!(
        Guess::decode(
            r#"{"questions":[{"target":"why","id":"goal-refinement-2","text":"Fake reply?"}]}"#,
            &r,
            &mut vec![]
        )
        .is_err()
    );
}

// These are projections of live verified readings, not fake-provider quality claims.
// The prompt byte test guards the instructions; the bounded live eval guards their meaning.
#[test]
fn captured_evidence_context_stays_in_originals_and_real_choices_stay_open() {
    for (fixture, ready, question_part) in [
        (
            include_str!("../../../tests/fixtures/evidence_notes/before-provenance.json"),
            false,
            None,
        ),
        (
            include_str!("../../../tests/fixtures/evidence_notes/provenance.json"),
            true,
            None,
        ),
        (
            include_str!("../../../tests/fixtures/evidence_notes/missing-check.json"),
            false,
            Some(Part::DoneWhen),
        ),
        (
            include_str!("../../../tests/fixtures/evidence_notes/boundary.json"),
            false,
            Some(Part::MustNot),
        ),
    ] {
        let capture: serde_json::Value = serde_json::from_str(fixture).unwrap();
        let source = capture["source"].as_str().unwrap();
        let mut a = app_with_source(capture["verified_projection"].clone(), source);
        assert_eq!(a.goal_ready(), ready);
        assert_eq!(
            a.conversation.focused_question().and_then(|q| q.target),
            question_part
        );
        a.review_goal();
        let review = &a.handoff.review().unwrap().affirmation;
        assert_eq!(review.sources[0].text, source);
        let saved = review.affirmed();
        assert_eq!(saved.originals[0].text, source);
        let context: serde_json::Value = serde_json::from_str(&saved.context).unwrap();
        if ready {
            assert!(review.unresolved.is_empty() && review.unresolved_notes.is_empty());
            assert_eq!(context["unresolved_notes"], serde_json::json!([]));
            assert!(!review.goal.contains("dogfood") && !review.goal.contains("not reproduced"));
        } else {
            assert!(!review.unresolved.is_empty() || !review.unresolved_notes.is_empty());
            assert!(
                context["unresolved_notes"]
                    .as_array()
                    .is_some_and(|v| !v.is_empty())
            );
        }
        assert!(a.handoff.receipt().is_none() && !a.handoff.saving());
    }
}

#[test]
fn review_and_saved_context_keep_conversations_unresolved_intent() {
    let mut a = app(serde_json::json!({
        "parts": {"outcome": "review refusals"},
        "decisions": [{"part": "must_not", "text": "Private text",
            "answers": ["omit", "include"], "question": "May private text appear?"}],
        "unresolved_notes": ["Keep this separate note."],
        "questions": [{"target": "done_when", "text": "What would show this worked?"}]
    }));
    let (questions, notes) = a.conversation.review_context();
    assert!(!questions.is_empty() && !notes.is_empty());
    let goal = a.conversation.board().unwrap().goal().unwrap().to_owned();
    a.review_goal();
    let review = &a.handoff.review().unwrap().affirmation;
    assert_eq!(
        serde_json::to_value(&review.unresolved).unwrap(),
        serde_json::to_value(&questions).unwrap()
    );
    assert_eq!(review.unresolved_notes, notes);
    assert_eq!(review.goal, goal);
    let saved = review.affirmed();
    let context: serde_json::Value = serde_json::from_str(&saved.context).unwrap();
    assert_eq!(
        context["unresolved"],
        serde_json::to_value(&questions).unwrap()
    );
    assert_eq!(
        context["unresolved_notes"],
        serde_json::to_value(&notes).unwrap()
    );
    assert_eq!(saved.goal, goal);
    assert_eq!(saved.originals[0].text, "An intact original");
}
