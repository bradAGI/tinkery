use super::*;
fn request() -> BoardRequest {
    BoardRequest {
        ask_counts: Default::default(),
        sources: vec![Source {
            id: 1,
            text: "Exact Café 👩‍💻 words.".into(),
            in_reply_to: None,
        }],
        fragments: vec![],
        previous: None,
        skipped: vec![],
        answered: vec![],
        settled: vec![],
        layout: vec![],
    }
}
/// A reading with a decision review joined in, verified; `None` when the review adds nothing.
fn reviewed(raw: &str, review: &str, changes: &mut Vec<String>) -> Option<board::Board> {
    let review = review::Review::decode(review, changes)?;
    let r = request();
    let g = Guess::decode_reviewed(raw, Some(&review), &r, changes).unwrap();
    Some(board::Board::verify(g, &r).unwrap())
}
fn parsed(raw: &str) -> (board::Board, Vec<String>) {
    let r = request();
    let mut changes = vec![];
    let g = Guess::decode(raw, &r, &mut changes).unwrap();
    (board::Board::verify(g, &r).unwrap(), changes)
}
fn success_choice() -> serde_json::Value {
    serde_json::json!({
        "parts": {"who": "TEST players", "outcome": "have a game", "why": "play together"},
        "decisions": [{"part": "done_when", "text": "TEST finish", "answers": ["local", "shared"],
            "question": "TEST local or shared?"}],
        "questions": [{"target": "done_when", "text": "TEST local or shared?"}]
    })
}

#[test]
fn an_open_success_decision_does_not_replace_the_selected_boundary_question() {
    let mut raw = success_choice();
    raw["open"] = serde_json::json!(["must_not"]);
    raw["questions"] =
        serde_json::json!([{"target": "must_not", "text": "May drafts leave this laptop?"}]);
    let (board, _) = parsed(&raw.to_string());
    assert_eq!(board.questions[0].text, "May drafts leave this laptop?");
    assert_eq!(board.decisions[0].answers, ["local", "shared"]);
    assert_eq!(
        board.decisions[0].question.as_deref(),
        Some("TEST local or shared?")
    );
    assert!(board.parts.done_when.is_none());
}

#[test]
fn a_missing_success_field_without_a_selected_question_stays_unknown() {
    let mut raw = success_choice();
    raw["questions"] = serde_json::json!([]);
    raw["decisions"][0]["question"] = serde_json::Value::Null;
    let (board, _) = parsed(&raw.to_string());
    assert!(board.questions.is_empty());
    assert!(board.parts.done_when.is_none());
    assert_eq!(board.decisions[0].answers, ["local", "shared"]);
    assert!(board.open.contains(&Part::DoneWhen));
}

#[test]
fn success_alternatives_do_not_bound_missing_criteria_but_remain_context() {
    let (b, _) = parsed(&success_choice().to_string());
    assert_eq!(b.questions[0].text, "What would tell you this worked?");
    assert_eq!(b.decisions[0].answers, ["local", "shared"]);
    assert!(b.decisions[0].covered_by(&b.questions.iter().cloned().collect::<Vec<_>>()));
    let mut raw = success_choice();
    raw["parts"]["why"] = serde_json::Value::Null;
    let (b, _) = parsed(&raw.to_string());
    assert_eq!(
        b.questions[0].target,
        Some(Part::DoneWhen),
        "an unrelated missing reason does not displace open success elicitation"
    );
    assert_eq!(b.questions[0].text, "What would tell you this worked?");
}

#[test]
fn opening_a_success_question_preserves_unrelated_choice_links() {
    let mut raw = success_choice();
    raw["decisions"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({
            "part": "done_when", "text": "Another unresolved check", "answers": ["A", "B"],
            "question": "Which other check matters?"
        }));
    let (board, _) = parsed(&raw.to_string());
    assert_eq!(board.questions[0].text, "What would tell you this worked?");
    assert_eq!(
        board.decisions[0].question.as_deref(),
        Some("What would tell you this worked?")
    );
    assert_eq!(
        board.decisions[1].question.as_deref(),
        Some("Which other check matters?")
    );
    assert_eq!(board.decisions[1].answers, ["A", "B"]);
    assert!(!board.decisions[1].covered_by(&board.questions.iter().cloned().collect::<Vec<_>>()));
}

#[test]
fn opening_a_success_question_cannot_repeat_an_answered_generic_question() {
    let (previous, _) = parsed(&success_choice().to_string());
    let mut r = request();
    r.answered.push(previous.questions[0].id.clone());
    r.settled.push(Settled {
        question: previous.questions[0].clone(),
        source: 1,
    });
    let mut raw = success_choice();
    raw["questions"] = serde_json::json!([{
        "id": "new-check", "target": "done_when", "text": "Another bounded check?"
    }]);
    raw["decisions"][0]["question"] = "Another bounded check?".into();
    let guess = Guess::decode(&raw.to_string(), &r, &mut vec![]).unwrap();
    let board = board::Board::verify(guess, &r).unwrap();
    assert!(board.questions.is_empty());
    assert!(board.parts.done_when.is_none());
    assert!(board.open.contains(&Part::DoneWhen));
    assert_eq!(board.decisions[0].answers, ["local", "shared"]);
}

#[test]
fn an_unknown_beneficiary_does_not_manufacture_a_success_followup() {
    let mut raw = success_choice();
    raw["parts"]["who"] = serde_json::Value::Null;
    let (previous, _) = parsed(&raw.to_string());
    let mut r = request();
    r.previous = Some(previous.wire());
    raw["parts"]["done_when"] = "local".into();
    raw["decisions"] = serde_json::json!([]);
    raw["questions"] = serde_json::json!([]);
    let guess = Guess::decode(&raw.to_string(), &r, &mut vec![]).unwrap();
    let board = board::Board::verify(guess, &r).unwrap();
    assert!(board.parts.who.is_none());
    assert_eq!(board.parts.done_when.as_deref(), Some("local"));
    assert!(board.questions.is_empty());
}

#[test]
fn criteria_length_never_manufactures_a_followup() {
    let (previous, _) = parsed(&success_choice().to_string());
    for done in ["local", "a very detailed single criterion", "local\nshared"] {
        for has_choices in [false, true] {
            let mut r = request();
            r.previous = Some(previous.wire());
            if !has_choices {
                r.previous.as_mut().unwrap().decisions.clear();
            }
            let raw = serde_json::json!({"parts": {"who": "TEST players", "outcome": "have a game",
                "why": "play together", "done_when": done}});
            let g = Guess::decode(&raw.to_string(), &r, &mut vec![]).unwrap();
            let b = board::Board::verify(g, &r).unwrap();
            assert!(b.questions.is_empty());
            assert!(!b.open.contains(&Part::DoneWhen));
            assert_eq!(b.parts.done_when.as_deref(), Some(done));
        }
    }
}

#[test]
fn success_elicitation_respects_skip_two_asks_and_settled_history() {
    for followup in [false, true] {
        let (previous, _) = parsed(&success_choice().to_string());
        for history in ["skip", "two", "answered"] {
            let mut r = request();
            let raw = if followup {
                r.previous = Some(previous.wire());
                serde_json::json!({"parts": {"who": "TEST players", "outcome": "have a game",
                    "why": "play together", "done_when": "local"}, "open": ["done_when"],
                    "questions": [{"target": "done_when", "text": "Which remaining check matters?"}]})
            } else {
                success_choice()
            };
            let decode =
                |r: &BoardRequest| Guess::decode(&raw.to_string(), r, &mut vec![]).unwrap();
            let b = board::Board::verify(decode(&r), &r).unwrap();
            match history {
                "skip" => r.skipped.push(b.questions[0].clone()),
                "two" => {
                    r.ask_counts.insert(Part::DoneWhen, 2);
                }
                _ => {
                    r.answered.push(b.questions[0].id.clone());
                    r.settled.push(Settled {
                        question: b.questions[0].clone(),
                        source: 1,
                    });
                }
            }
            let b = board::Board::verify(decode(&r), &r).unwrap();
            assert!(b.questions.is_empty(), "{followup} {history}");
            assert!(b.open.contains(&Part::DoneWhen));
        }
    }
}

#[test]
fn unreadable_done_when_is_refused_instead_of_losing_checks() {
    for value in [
        serde_json::json!({"check": "tests pass"}),
        serde_json::json!(42),
        serde_json::json!(true),
        serde_json::json!(["keep this check", 42]),
        serde_json::json!(["keep this check", null]),
        serde_json::json!([["nested check"]]),
    ] {
        let raw = serde_json::json!({"parts": {"done_when": value}}).to_string();
        let error = Guess::decode(&raw, &request(), &mut vec![]).unwrap_err();
        assert!(error.contains("Unreadable done_when"), "{error}");
    }
    for value in [serde_json::Value::Null, serde_json::json!([])] {
        let raw = serde_json::json!({"parts": {"done_when": value}}).to_string();
        assert!(parsed(&raw).0.parts.done_when.is_none());
    }
}

#[test]
fn done_when_list_keeps_order_duplicates_and_wording() {
    let checks = [
        "  Keep Café exactly.  ",
        "Keep the second\nline.",
        "  Keep Café exactly.  ",
    ];
    let raw = serde_json::json!({"parts": {"done_when": checks}}).to_string();
    assert_eq!(
        Guess::decode(&raw, &request(), &mut vec![])
            .unwrap()
            .parts
            .done_when
            .as_deref(),
        Some(checks.join("\n").as_str())
    );
}

#[test]
fn omissions_extras_and_narrative_notes_never_fabricate_citations_or_authority() {
    let (b, changes) = parsed(
        r#"{"parts":{"who":"I","outcome":"remember books","unexpected":"ignore"},"alternatives":{"label":"Maybe","precondition":"unknown"},"misfits":[{"text":"Unsure about a relationship"}],"confirmed":true,"settled":[{"source":99}]}"#,
    );
    assert!(matches!(
        b.reading.as_ref().unwrap().supports,
        board::Grounding::Uncited
    ));
    assert_eq!(b.parts.outcome.as_deref(), Some("remember books"));
    assert_eq!(b.parts.missing().count(), 2);
    assert!(
        b.questions.is_empty(),
        "missing fields alone do not manufacture questions"
    );
    assert!(b.misfits.is_empty());
    assert_eq!(b.unresolved_notes, ["Unsure about a relationship"]);
    assert!(b.alternatives[0].benefit.is_none());
    for key in ["unexpected", "precondition", "confirmed", "settled"] {
        assert!(changes.iter().any(|c| c.contains(key)));
    }
    assert_eq!(b.wire().parts, b.parts);
}
#[test]
fn lengths_and_optional_types_are_not_boundary_rules_and_one_question_is_presented() {
    let input = serde_json::json!({"parts":{"who":"I","outcome":vec!["word";100].join(" "),"why":{},"done_when":null},"open":["done_when","nonsense"],"questions":(0..9).map(|i|serde_json::json!({"target":"done_when","text":format!("Unresolved {i} without question mark")})).collect::<Vec<_>>(),"alternatives":vec!["same";7]});
    let (b, changes) = parsed(&input.to_string());
    assert!(b.goal().is_some());
    assert_eq!(b.questions.len(), 1);
    assert_eq!(b.alternatives.len(), 7);
    assert!(b.parts.why.is_none() && b.parts.done_when.is_none());
    assert!(changes.iter().any(|s| s.contains("unknown open part")));
    assert!(changes.iter().any(|s| s.contains("Extra questions")));
}
#[test]
fn malformed_json_fails_but_empty_partial_parts_display_without_fabrication() {
    for raw in ["not JSON", "[]"] {
        assert!(Guess::decode(raw, &request(), &mut vec![]).is_err());
    }
    for raw in [
        r#"{}"#,
        r#"{"parts":{"outcome":3,"why":null},"questions":[null,{"text":4}],"alternatives":[true]}"#,
        r#"{"framings":["Do not infer this old goal"]}"#,
    ] {
        let (b, _) = parsed(raw);
        assert!(b.goal().is_none());
        assert_eq!(b.parts.missing().count(), 4);
        let mut app = BrainDump::default();
        *app.conversation.sources_mut() = request().sources;
        app.conversation.set_applied(1);
        app.conversation.set_board(Some(b));
        for (w, h) in [(80, 24), (100, 30), (160, 40), (320, 40)] {
            let screen = snapshot(w, h, &mut app, false).unwrap();
            assert!(!screen.contains("Ready for your review"));
            assert!(!screen.contains("Do not infer this old goal"));
        }
    }
}
#[test]
fn claimed_citations_remain_exact_and_grapheme_safe_even_without_parts() {
    for span in [
        serde_json::json!({"source":2,"quote":"Exact"}),
        serde_json::json!({"source":1,"quote":"invented"}),
        serde_json::json!({"source":1,"quote":"👩"}),
        serde_json::json!({"source":1,"quote":"Exact","occurrence":1}),
        serde_json::json!({"source":"2","quote":"Exact"}),
        serde_json::json!({"source":"1","quote":"invented"}),
        serde_json::json!({"source":"1","quote":"👩"}),
        serde_json::json!({"source":"1","quote":"Exact","occurrence":1}),
        serde_json::json!({"source":"1","quote":"Exact","occurrence":"0"}),
        serde_json::json!({"source":"-1","quote":"Exact"}),
        serde_json::json!({"source":"+1","quote":"Exact"}),
        serde_json::json!({"source":" 1","quote":"Exact"}),
        serde_json::json!({"source":"1.0","quote":"Exact"}),
        serde_json::json!({"source":"1e0","quote":"Exact"}),
        serde_json::json!({"source":"","quote":"Exact"}),
        serde_json::json!({"source":"18446744073709551616","quote":"Exact"}),
        serde_json::json!({"source":1.0,"quote":"Exact"}),
        serde_json::json!({"source":true,"quote":"Exact"}),
        serde_json::json!({"quote":"Exact"}),
    ] {
        for raw in [
            serde_json::json!({"supports":[span.clone()]}),
            serde_json::json!({"framings":[{"supports":[span.clone()]}]}),
            serde_json::json!({"misfits":[span]}),
        ] {
            assert!(
                Guess::decode(&raw.to_string(), &request(), &mut vec![]).is_err(),
                "{raw}"
            );
        }
    }
    let (b, _) = parsed(
        r#"{"parts":{"who":"I","outcome":"read"},"supports":[{"source":1,"quote":"Café 👩‍💻"}]}"#,
    );
    let span = &b.reading.as_ref().unwrap().supports[0];
    assert_eq!(
        &request().sources[0].text[span.range(&request().sources).unwrap()],
        "Café 👩‍💻"
    );
    let mut changed = request().sources;
    changed[0].text = changed[0].text.replace("Café", "Cafe");
    assert!(span.range(&changed).is_err());
}
#[test]
fn recorded_360_answer_and_added_dump_repair_decimal_string_source_ids() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/think-source-ref-360.json"
    ))
    .unwrap();
    let originals: Vec<Source> = fixture["sources"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .map(|(i, text)| Source {
            id: i + 1,
            text: text.as_str().unwrap().into(),
            in_reply_to: None,
        })
        .collect();
    for (i, raw) in fixture["replies"].as_array().unwrap().iter().enumerate() {
        let mut r = request();
        r.sources = originals[..4 + i].to_vec();
        let before = serde_json::to_string(&r).unwrap();
        let raw = raw.as_str().unwrap();
        let mut changes = vec![];
        let guess = Guess::decode(raw, &r, &mut changes).unwrap();
        let board = board::Board::verify(guess, &r).unwrap();
        let wire = board.wire();
        let spans = &wire.framings[0].supports;
        let recorded: serde_json::Value = serde_json::from_str(raw).unwrap();
        let supports = recorded["supports"].as_array().unwrap();
        assert_eq!(spans.len(), supports.len());
        for (span, recorded) in spans.iter().zip(supports) {
            assert_eq!(
                span.source.to_string(),
                recorded["source"].as_str().unwrap()
            );
            assert_eq!(span.quote, recorded["quote"].as_str().unwrap());
        }
        assert_eq!(
            changes
                .iter()
                .filter(|s| s.contains("decimal-string source reference"))
                .count(),
            supports.len()
        );
        assert!(board.goal().unwrap().contains("public aggregated data"));
        assert_eq!(serde_json::to_string(&r).unwrap(), before);
        assert_eq!(r.sources.last().unwrap().text, originals[3 + i].text);
    }
}

#[test]
fn decimal_string_citations_validate_in_every_anchor_location() {
    for raw in [
        serde_json::json!({"supports":[{"source":"1","quote":"Exact"}]}),
        serde_json::json!({"framings":[{"supports":[{"source":"1","quote":"Exact"}]}]}),
        serde_json::json!({"misfits":[{"source":"1","quote":"Exact"}]}),
    ] {
        let (_, changes) = parsed(&raw.to_string());
        assert!(
            changes
                .iter()
                .any(|s| s.contains("decimal-string source reference"))
        );
    }
}

#[test]
fn local_history_and_scope_identity_cannot_be_replaced_by_model_output() {
    let mut r = request();
    r.answered.push("done".into());
    r.skipped.push(Question {
        suggestion: None,
        target: None,
        id: "skipped".into(),
        text: "Leave this aside".into(),
    });
    let mut changes = vec![];
    let g=Guess::decode(r#"{"questions":[{"id":"done","text":"Again"},{"id":"new","text":"Leave this aside"},{"id":"fresh","text":"A real uncertainty"},{"id":"other","text":"A real uncertainty"}]}"#,&r,&mut changes).unwrap();
    assert_eq!(g.questions.len(), 1);
    assert_eq!(g.questions[0].id, "fresh");
    assert_eq!(r.answered, ["done"]);
    assert_eq!(r.skipped.len(), 1);
    assert!(
        Guess::decode(
            r#"{"questions":[{"id":"scope-addition-99","text":"Take over scope"}]}"#,
            &r,
            &mut vec![]
        )
        .is_err()
    );
}
#[test]
fn normalization_escapes_controls_and_logs_fences_without_changing_originals() {
    let raw = "```json\n{\"parts\":{\"who\":\"I\",\"outcome\":\"Meaning\\u001b[31m\"},\"misfits\":[\"Uncertain\"]}\n```";
    let (b, changes) = parsed(raw);
    assert!(!b.goal().unwrap().contains('\x1b'));
    assert!(b.goal().unwrap().contains("\\u{1b}"));
    assert!(changes.iter().any(|s| s.contains("code fence")));
    assert!(
        changes
            .iter()
            .any(|s| s.contains("Escaped display control"))
    );
    assert_eq!(request().sources[0].text, "Exact Café 👩‍💻 words.");
    let (_, changes) = parsed(r#"{"parts":{},"\u001b[31m":true}"#);
    assert!(changes.iter().all(|s| !s.contains('\x1b')));
}
#[test]
fn historical_raw_captures_display_without_inferred_parts_or_provider_helpers() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    for (folder, names) in [
        (
            "tests/fixtures/historical-captures/log-only",
            vec!["onboarding-emails", "trap"],
        ),
        (
            "tests/fixtures/historical-captures/subtraction",
            vec!["books", "onboarding-emails", "trap"],
        ),
    ] {
        for name in names {
            let dir = root.join(folder).join(name);
            let records: serde_json::Value =
                serde_json::from_slice(&std::fs::read(dir.join("decisions.json")).unwrap())
                    .unwrap();
            let mut r = request();
            r.sources[0].text = std::fs::read_to_string(dir.join("original.txt")).unwrap();
            let mut changes = vec![];
            let g = Guess::decode(
                records["decisions"][0]["raw"].as_str().unwrap(),
                &r,
                &mut changes,
            )
            .unwrap();
            let b = board::Board::verify(g, &r).unwrap();
            assert_eq!(b.parts, GoalParts::default());
            assert!(b.goal().is_none());
            assert!(!changes.is_empty());
            let mut app = BrainDump::default();
            *app.conversation.sources_mut() = r.sources;
            app.conversation.set_applied(1);
            app.conversation.set_board(Some(b));
            for (w, h) in [(80, 24), (100, 30), (160, 40), (320, 40)] {
                snapshot(w, h, &mut app, false).unwrap();
            }
            assert!(!app.goal_ready());
            assert!(app.handoff.receipt().is_none());
        }
    }
}
#[test]
fn freeform_readings_are_ignored_and_notes_survive_without_becoming_a_goal() {
    let (b, changes) = parsed(
        r#"{"parts":{"must_not":"no dashboard"},"framings":["Reading zero","Reading one"],"misfits":["Unresolved note"]}"#,
    );
    assert_eq!(b.goal().unwrap(), "Must not: no dashboard.");
    assert!(changes.iter().any(|s| s.contains("Ignored freeform")));
    assert_eq!(b.unresolved_notes, ["Unresolved note"]);
    let mut app = BrainDump::with_host(Arc::new(Simulated)).with_sessions(None);
    *app.conversation.sources_mut() = request().sources;
    app.conversation.set_applied(1);
    app.conversation.set_board(Some(b));
    app.review_goal();
    let frozen = app.handoff.review().unwrap();
    assert_eq!(frozen.affirmation.goal, "Must not: no dashboard.");
    assert!(
        frozen
            .affirmation
            .unresolved_notes
            .iter()
            .any(|s| s == "Unresolved note")
    );
    assert!(app.handoff.receipt().is_none());
}
#[test]
fn a_typed_decision_keeps_its_part_open_and_can_be_asked_after_required_parts() {
    let parts = r#""parts":{"who":"I","outcome":"have a clean workspace","why":"I lost work","done_when":"merged branches are gone","must_not":["force delete anything"]}"#;
    let (b, _) = parsed(&format!(
        r#"{{{parts},"open":[],"decisions":[{{"part":"must_not","text":"Worktrees that still hold work","answers":["leave them and report them","save their work, then remove them"]}}],"questions":[{{"target":"must_not","text":"Should worktrees that still hold work be left alone or rescued first?"}}]}}"#
    ));
    assert_eq!(b.open, [Part::MustNot]);
    assert_eq!(b.questions[0].target, Some(Part::MustNot));
    assert_eq!(
        b.unresolved_notes,
        [
            "Open decision: Worktrees that still hold work (leave them and report them / save their work, then remove them)"
        ]
    );
    let mut app = BrainDump::with_host(Arc::new(Simulated));
    *app.conversation.sources_mut() = request().sources;
    app.conversation.set_applied(1);
    app.conversation.set_board(Some(b));
    assert!(!app.goal_ready());
    let (settled, _) = parsed(&format!(r#"{{{parts},"open":[],"decisions":[]}}"#));
    app.conversation.set_board(Some(settled));
    assert!(app.goal_ready());
}
#[test]
fn a_decision_without_two_answers_or_a_known_part_is_ignored() {
    let (b, changes) = parsed(
        r#"{"parts":{"who":"I","outcome":"x","why":"y","done_when":"z"},"decisions":[{"part":"must","text":"One answer","answers":["only","only"]},{"part":"nonsense","text":"Bad part","answers":["a","b"]},"bare text"]}"#,
    );
    assert!(b.open.is_empty());
    assert!(b.unresolved_notes.is_empty());
    assert!(changes.iter().any(|c| c.contains("needs two answers")));
    assert!(changes.iter().any(|c| c.contains("needs a known part")));
    assert!(changes.iter().any(|c| c.contains("not an object")));
}
#[test]
fn a_decision_review_is_asked_first_once_required_parts_are_present() {
    let full = r#"{"parts":{"who":"I","outcome":"have a clean workspace","why":"I lost work","done_when":"merged branches are gone"},"open":["must"],"questions":[{"target":"must","text":"Anything that must stay?"}]}"#;
    let review = r#"{"decisions":[{"part":"must_not","text":"Worktrees that still hold work","answers":["leave them","rescue then remove"],"question":"Should worktrees that still hold work be left alone?"}]}"#;
    let mut changes = vec![];
    let b = reviewed(full, review, &mut changes).unwrap();
    assert_eq!(b.open, [Part::Must, Part::MustNot]);
    assert_eq!(
        b.questions[0].text,
        "Should worktrees that still hold work be left alone?"
    );
    assert_eq!(
        b.unresolved_notes,
        ["Open decision: Worktrees that still hold work (leave them / rescue then remove)"]
    );
    // A missing required part is still asked before any review decision.
    let partial = r#"{"parts":{"who":"I","outcome":"have a clean workspace"},"questions":[{"target":"why","text":"What prompted this?"}]}"#;
    let b = reviewed(partial, review, &mut vec![]).unwrap();
    assert_eq!(b.questions[0].text, "What prompted this?");
    // An empty or unreadable review leaves the board as it was.
    assert!(reviewed(full, r#"{"decisions":[]}"#, &mut vec![]).is_none());
    assert!(reviewed(full, "not JSON", &mut changes).is_none());
    assert!(
        changes
            .iter()
            .any(|c| c.starts_with("Ignored decision review"))
    );
}
#[test]
fn the_review_change_note_counts_only_the_decisions_it_added() {
    let board = r#"{"parts":{"who":"I"},"decisions":[{"part":"must","text":"Keep drafts","answers":["yes","no"]}]}"#;
    let review = r#"{"decisions":[{"part":"must_not","text":"Delete drafts","answers":["yes","no"],"question":"May drafts be deleted?"}]}"#;
    let mut changes = vec![];
    reviewed(board, review, &mut changes).unwrap();
    let notes: Vec<_> = changes
        .iter()
        .filter(|c| c.starts_with("Decision review"))
        .collect();
    assert_eq!(notes, ["Decision review added 1 decision(s)"]);
}

#[test]
fn review_preserves_specific_questions_for_normalized_missing_parts() {
    let review = r#"{"decisions":[{"part":"must_not","text":"Worktrees that still hold work","answers":["leave them","rescue then remove"],"question":"Should worktrees that still hold work be left alone?"}]}"#;
    for (part, duplicate, question) in [
        (
            Part::Why,
            "have a clean workspace",
            "What is getting lost in this workspace?",
        ),
        (
            Part::DoneWhen,
            "have a clean workspace",
            "Which remaining branches would show the cleanup is unfinished?",
        ),
        (
            Part::DoneWhen,
            "I have a clean workspace",
            "Which remaining branches would show the cleanup is unfinished?",
        ),
    ] {
        let mut raw = serde_json::json!({
            "parts": {"who": "I", "outcome": "have a clean workspace", "why": "I lost work", "done_when": "merged branches are gone"},
            "questions": [{"target": part, "text": question}]
        });
        raw["parts"][serde_json::to_value(part).unwrap().as_str().unwrap()] = duplicate.into();
        let raw = raw.to_string();
        let (before, _) = parsed(&raw);
        let after = reviewed(&raw, review, &mut vec![]).unwrap();
        assert_eq!(before.questions[0].text, question);
        assert_eq!(after.questions[0].text, question);
        assert_eq!(after.questions[0].target, Some(part));
        assert_eq!(
            after.unresolved_notes,
            ["Open decision: Worktrees that still hold work (leave them / rescue then remove)"]
        );
    }
}

#[test]
fn review_shows_a_linked_decision_once_and_keeps_other_choices_on_the_same_part() {
    let shape = serde_json::json!({"parts":{"who":"Tinkery users","outcome":"understand refused sends","why":"empty sends look like nothing happened","done_when":"tests show the reason"},"decisions":[{"part":"must_not","text":"Whether refusal messages may include private text","answers":["may include private text","must omit private text"]},{"part":"must_not","text":"Whether drafts may leave this machine","answers":["local only","may upload"]}],"questions":[{"target":"must_not","text":"May refusals show private text?"}]}).to_string();
    let review = r#"{"decisions":[{"part":"must_not","text":"Whether refusal messages may include private text","answers":["omit private text","may include private text"],"question":"May refusals show private text?","covers":[0]}]}"#;
    let b = reviewed(&shape, review, &mut vec![]).unwrap();
    let mut app = BrainDump::with_host(Arc::new(Simulated)).with_sessions(None);
    *app.conversation.sources_mut() = request().sources;
    app.conversation.set_applied(1);
    app.conversation.set_board(Some(b));
    app.review_goal();
    let notes = &app.handoff.review().unwrap().affirmation.unresolved_notes;
    assert!(
        !notes.iter().any(|n| n.contains("Whether refusal messages")),
        "question repeated as a note: {notes:?}"
    );
    assert!(
        notes.iter().any(|n| n.contains("Whether drafts may leave")),
        "a different same-part decision disappeared"
    );
    for (w, h) in [(80, 24), (100, 30), (160, 40), (320, 40)] {
        for no_color in [false, true] {
            let screen = snapshot(w, h, &mut app, no_color).unwrap();
            let right = super::goal_tests::right_text(&screen, w);
            assert_eq!(
                right.matches("May refusals show private text?").count(),
                1,
                "{right}"
            );
            assert!(!right.contains("Whether refusal messages"), "{right}");
            assert!(right.contains("Whether drafts may leave"), "{right}");
        }
    }
    let saved = app.handoff.review().unwrap().affirmation.affirmed();
    assert!(saved.context.contains("local only") && saved.context.contains("may upload"));
    assert!(
        saved.context.contains("omit private text")
            && saved.context.contains("may include private text")
    );
    assert!(app.handoff.receipt().is_none());
}

#[test]
fn coverage_never_removes_a_choice_on_invalid_indices_parts_or_replies() {
    let shape = r#"{"parts":{"who":"I","outcome":"review refusals","why":"nothing appears","done_when":"tests pass"},"decisions":[{"part":"must_not","text":"Private text","answers":["omit","include"]}]}"#;
    for coverage in [
        serde_json::json!([99]),
        serde_json::json!([-1]),
        serde_json::json!(["0"]),
        serde_json::json!([0, 99]),
    ] {
        let reply=serde_json::json!({"decisions":[{"part":"must_not","text":"A separate choice","answers":["local","remote"],"question":"Local or remote?","covers":coverage}]}).to_string();
        let b = reviewed(shape, &reply, &mut vec![]).unwrap();
        assert!(b.decisions.iter().any(|d| d.text == "Private text"));
        assert_eq!(b.decisions.len(), 2);
    }
    for (part, answers) in [
        ("must", serde_json::json!(["omit", "include"])),
        ("must_not", serde_json::json!(["omit"])),
        ("must_not", serde_json::json!(["same", "same"])),
    ] {
        let reply=serde_json::json!({"decisions":[{"part":part,"text":"Changed choice","answers":answers,"question":"What do you want?","covers":[0]}]}).to_string();
        let b = reviewed(shape, &reply, &mut vec![]).unwrap_or_else(|| parsed(shape).0);
        assert!(b.decisions.iter().any(|d| d.text == "Private text"));
    }
}

#[test]
fn unasked_decisions_and_unlinked_notes_remain_visible_in_review_and_handoff() {
    let partial = r#"{"parts":{"who":"I","outcome":"review refusals","why":"nothing appears"},"decisions":[{"part":"must_not","text":"Private text","answers":["omit","include"],"question":"May private text appear?"}],"unresolved_notes":["Keep this different unresolved note"],"questions":[{"target":"done_when","text":"What would show that the refusal is understood?"}]}"#;
    let (board, _) = parsed(partial);
    let mut a = BrainDump::with_host(Arc::new(Simulated)).with_sessions(None);
    *a.conversation.sources_mut() = request().sources;
    a.conversation.set_applied(1);
    a.conversation.set_board(Some(board));
    a.review_goal();
    let review = &a.handoff.review().unwrap().affirmation;
    assert_eq!(review.unresolved[0].target, Some(Part::DoneWhen));
    assert!(
        review
            .unresolved_notes
            .iter()
            .any(|n| n.contains("Private text"))
    );
    assert!(
        review
            .unresolved_notes
            .iter()
            .any(|n| n.contains("different unresolved note"))
    );
    assert_eq!(review.decisions.len(), 1);
    let saved = review.affirmed();
    assert!(
        saved.context.contains("May private text appear?") && saved.context.contains("include")
    );
    assert_eq!(saved.originals[0].text, "Exact Café 👩‍💻 words.");
}

#[test]
fn approaches_keyed_option_are_kept_through_review_and_saving() {
    // #86's live recheck (2026-10-13): the reply keyed each approach `option`, and all three
    // were dropped, so "Possible approaches" was empty (#89).
    let approach = |key: &str, label: &str| serde_json::json!({key: label, "benefit": format!("{label} helps"), "cost": "some work", "undo_cost": "low"});
    let mut both = approach("label", "Labelled hint");
    both["option"] = "Ignored option".into();
    let raw = serde_json::json!({
        "parts": {"who": "learners", "outcome": "leave a wrong answer feeling smarter", "why": "they feel dumb", "done_when": "they say so"},
        "alternatives": [approach("option", "Hint first"), approach("option", "Worked example"), both]
    })
    .to_string();
    let (b, changes) = parsed(&raw);
    let labels: Vec<_> = b.alternatives.iter().map(|a| a.wire().label).collect();
    assert_eq!(labels, ["Hint first", "Worked example", "Labelled hint"]);
    assert_eq!(b.alternatives[0].wire().benefit, "Hint first helps");
    assert!(changes.contains(&"Read candidate 1 label from its option field".to_string()));
    assert!(
        changes
            .iter()
            .any(|c| c.contains("candidate 2 field \"option\"")),
        "an ignored option beside a label is noted: {changes:?}"
    );
    let mut app = BrainDump::with_host(Arc::new(Simulated)).with_sessions(None);
    *app.conversation.sources_mut() = request().sources;
    app.conversation.set_applied(1);
    app.conversation.set_board(Some(b));
    app.review_goal();
    let saved = app.handoff.review().unwrap().affirmation.affirmed();
    let saved: Vec<_> = saved.options.iter().map(|o| o.label.as_str()).collect();
    assert_eq!(saved, ["Hint first", "Worked example", "Labelled hint"]);
}

#[test]
fn the_review_joins_as_typed_decisions_without_knowing_the_readings_keys() {
    // Design score row 3: the review contributes only its own decisions (#89). Reading keys
    // inside a review reply change nothing about the reading it joins.
    let full = r#"{"parts":{"who":"I","outcome":"have a clean workspace","why":"I lost work","done_when":"merged branches are gone"},"open":["must"],"questions":[{"target":"must","text":"Anything that must stay?"}]}"#;
    let decision = r#"{"part":"must_not","text":"Worktrees that still hold work","answers":["leave them","rescue then remove"],"question":"Should worktrees that still hold work be left alone?"}"#;
    let plain = format!(r#"{{"decisions":[{decision}]}}"#);
    let noisy = format!(
        r#"{{"parts":{{"who":"HIJACKED"}},"alternatives":[{{"label":"HIJACKED","benefit":"x","cost":"y","undo_cost":"z"}}],"questions":[{{"target":"who","text":"HIJACKED?"}}],"decisions":[{decision}]}}"#
    );
    let clean = reviewed(full, &plain, &mut vec![]).unwrap();
    let with_noise = reviewed(full, &noisy, &mut vec![]).unwrap();
    assert_eq!(with_noise.parts.compose(), clean.parts.compose());
    assert_eq!(with_noise.open, clean.open);
    assert_eq!(with_noise.alternatives.len(), clean.alternatives.len());
    let questions = |b: &board::Board| {
        b.questions
            .iter()
            .map(|q| q.text.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(questions(&with_noise), questions(&clean));
    assert_eq!(with_noise.unresolved_notes, clean.unresolved_notes);
    assert!(
        !with_noise
            .parts
            .compose()
            .unwrap_or_default()
            .contains("HIJACKED")
    );
}
