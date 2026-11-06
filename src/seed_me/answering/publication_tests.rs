//! Shape's real ledger boundary against the pinned writer, without provider calls.

use super::*;
use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf, process::Command, time::Duration};

const STAMP: &str = "2026-10-19T00:00:00+00:00";

fn temp() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("TEST-shape-publication-")
        .tempdir()
        .unwrap()
}

#[test]
fn publication_clock_matches_python_including_zero_microseconds() {
    let instants = [
        (0, 0),
        (1, 1),
        (951_782_400, 123_456),
        (1_792_368_000, 999_999),
        (4_102_444_800, 0),
    ];
    for (secs, micros) in instants {
        let ours = publication_time(
            UNIX_EPOCH + Duration::from_secs(secs) + Duration::from_micros(micros),
        )
        .unwrap();
        let output = Command::new("python3").args(["-c", &format!(
            "from datetime import datetime,timezone,timedelta; print((datetime.fromtimestamp({secs},timezone.utc)+timedelta(microseconds={micros})).isoformat())"
        )]).output().unwrap();
        assert!(output.status.success());
        assert_eq!(ours, String::from_utf8(output.stdout).unwrap().trim());
    }
    let submicro = UNIX_EPOCH + Duration::from_nanos(999);
    assert_eq!(
        publication_time(submicro).unwrap(),
        "1970-01-01T00:00:00+00:00"
    );
}

pub(super) fn session(root: &Path) -> PathBuf {
    session_with_goal(root, "TEST goal")
}

fn session_with_goal(root: &Path, goal: &str) -> PathBuf {
    let session = root.join("TEST-session");
    fs::create_dir(&session).unwrap();
    let initial = ledger::Value::parse(r#"{
        "schema_version":1,"session_id":"TEST-session","version":0,"revision":0,
        "created_at":"2026-10-19T00:00:00+00:00","updated_at":"2026-10-19T00:00:00+00:00",
        "status":"active","draft":{"goal":"","outcome":"","options":[]},"goal":null,
        "origin":null,"current_question":null,"frontier":[],"nodes":[],"assumed":[],"operator":"human"
    }"#).unwrap();
    fs::write(session.join("ledger.json"), initial.pretty().unwrap()).unwrap();
    let mut state: Value =
        serde_json::from_str(&ledger::editable(&initial).unwrap().pretty().unwrap()).unwrap();
    state["goal"] = goal.into();
    state["origin"] = "goal".into();
    state["nodes"] = json!([{"id":"goal","kind":"decision","status":"settled","prerequisites":[],
        "evidence":[],"answer":goal,"authority":"user","authority_source":"TEST only",
        "owner":"TEST operator","gate":"TEST gate"}]);
    publish_at(&session, 0, "TEST goal", &state, STAMP).unwrap();
    session
}

#[test]
fn every_shape_ledger_action_works_without_launching_the_helper() {
    let root = temp();
    let session = session(root.path());
    let version = read(&session).unwrap().version();
    let version = probe(&session, version).unwrap().unwrap();
    let version = answer(&session, version, SAMPLE.0, "TEST no sample", None).unwrap();
    let version = probe(&session, version).unwrap().unwrap();
    let version = answer(&session, version, PROBE.0, "TEST exact\r\nanswer", None).unwrap();
    let version = ask(
        &session,
        version,
        &Ask {
            prerequisites: None,
            supported_by: None,
            id: "test-question".into(),
            label: "TEST".into(),
            question: "TEST?".into(),
            gate: "TEST".into(),
            found: vec![],
            resolves: None,
            suggestion: None,
            assumed: vec![Assumption {
                text: "TEST default".into(),
                why: "TEST same".into(),
            }],
            options: vec![],
            against: None,
            confidence: None,
        },
        root.path(),
    )
    .unwrap();
    let version = recommend(&session, version, "test-question", "TEST suggestion", None).unwrap();
    let version = object(&session, version, "TEST default", None).unwrap();
    let version = answer(&session, version, "test-question", "TEST answer", None).unwrap();
    let version = promote(&session, version).unwrap().unwrap();
    assert_eq!(read(&session).unwrap().version(), version);
    assert_eq!(read(&session).unwrap().goal(), Some("TEST goal"));
    let saved = fs::read_to_string(session.join("ledger.json")).unwrap();
    assert!(saved.contains("TEST exact\\r\\nanswer"));
    assert_eq!(
        fs::metadata(session.join("ledger.json"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
}

#[test]
fn unasked_probe_answers_cannot_be_smuggled_into_seed_prose() {
    let root = temp();
    let session = session(root.path());
    let version = probe(&session, 1).unwrap().unwrap();
    let version = answer(&session, version, SAMPLE.0, "TEST no sample", None).unwrap();
    let version = probe(&session, version).unwrap().unwrap();
    let version = answer(&session, version, PROBE.0, "TEST my actual answer", None).unwrap();
    let checks = json!([{"statement":"TEST", "kind":"judge", "source":"TEST"}]);
    let prose =
        ledger::fixtures::prose_reply(include_str!("../../../tests/fixtures/shape-seed-reply.txt"));
    let before = fs::read(session.join("ledger.json")).unwrap();
    for field in ["Risks and verification", "Knowledge map"] {
        let mut forged = prose.clone();
        forged[field] = "Probe: What would an expert ask? — answer: TEST invented operator answer — changed: nothing".into();
        let error =
            draft_seed(&session, version, &forged, &checks, &[], root.path(), None).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("probe answers belong to the ledger"),
            "{error}"
        );
        assert_eq!(fs::read(session.join("ledger.json")).unwrap(), before);
        assert!(!session.join("seed-contract.md").exists());
    }
    draft_seed(&session, version, &prose, &checks, &[], root.path(), None).unwrap();
    let seed = fs::read_to_string(session.join("seed-contract.md")).unwrap();
    assert!(seed.contains("probe-pre-mortem [user] TEST my actual answer"));
    assert!(!seed.contains("TEST invented operator answer"));
    assert!(!seed.contains("What would an expert ask?"));
}

#[test]
fn seed_drafting_confirmation_status_and_ending_need_no_installed_helper() {
    let root = temp();
    let session = session(root.path());
    let checks =
        serde_json::from_str(include_str!("../../../tests/fixtures/shape-checks.json")).unwrap();
    let revision = draft_seed(
        &session,
        1,
        &ledger::fixtures::prose_reply(include_str!(
            "../../../tests/fixtures/shape-seed-reply.txt"
        )),
        &checks,
        &[],
        root.path(),
        None,
    )
    .unwrap();
    confirm_seed(
        &session,
        crate::seed_me::answering::read(&session).unwrap().version(),
        &revision,
        &std::fs::read_to_string(session.join("seed-contract.md")).unwrap(),
        "TEST yes to this seed only",
        None,
    )
    .unwrap();
    assert!(matches!(
        super::super::shaping::read(&session).unwrap().stage,
        super::super::shaping::Shaped::Ready { ended: false, .. }
    ));
    end(&session, Ending::Completed, None).unwrap();
    assert!(matches!(
        super::super::shaping::read(&session).unwrap().stage,
        super::super::shaping::Shaped::Ready { ended: true, .. }
    ));
    assert_eq!(read(&session).unwrap().goal(), Some("TEST goal"));
    for name in ["viewer.json", "ledger-view.html"] {
        assert!(!session.join(name).exists());
    }
}

#[test]
fn stale_shape_publication_refuses_without_changing_bytes() {
    let root = temp();
    let session = session(root.path());
    let ledger = ledger::load(&session).unwrap();
    let mut state: Value =
        serde_json::from_str(&ledger::editable(&ledger).unwrap().pretty().unwrap()).unwrap();
    state["draft"]["goal"] = "TEST changed from stale read".into();
    let bytes = fs::read(session.join("ledger.json")).unwrap();
    assert_eq!(
        publish_at(&session, 0, "TEST stale", &state, STAMP),
        Err(Refusal::Stale)
    );
    assert_eq!(fs::read(session.join("ledger.json")).unwrap(), bytes);
}

#[test]
fn completion_through_publish_is_refused() {
    let root = temp();
    let session = session(root.path());
    let ledger = ledger::load(&session).unwrap();
    let mut state: Value =
        serde_json::from_str(&ledger::editable(&ledger).unwrap().pretty().unwrap()).unwrap();
    state["status"] = "completed".into();
    assert_eq!(publish_at(&session, 1, "TEST complete", &state, STAMP), Err(Refusal::Refused(
        "Tinkery refused: publish cannot complete a session: use `end --status completed`, which checks the confirmed seed".into())));
    assert_eq!(ledger::load(&session).unwrap(), ledger);
}

#[test]
fn an_unknown_save_asks_the_operator_to_look() {
    let error = |save_state| ledger::PublishError {
        reason: "could not save the ledger: TEST disk".into(),
        save_state,
    };
    assert_eq!(
        published(error(ledger::SaveState::NotSaved)),
        Refusal::Refused("Tinkery refused: could not save the ledger: TEST disk".into())
    );
    let Refusal::Refused(unknown) = published(error(ledger::SaveState::Unknown)) else {
        panic!("an unknown save must be a refusal with words");
    };
    assert!(
        unknown.starts_with("This may or may not have saved (TEST disk)."),
        "{unknown}"
    );
    assert!(unknown.contains("Reopen this session"), "{unknown}");
    assert!(!unknown.contains("refused"), "{unknown}");
}

#[test]
fn a_session_without_a_ledger_is_refused_in_plain_words() {
    let root = temp();
    let error = respond(
        root.path(),
        1,
        "choice",
        &ledger::AnswerAction::Delegate,
        None,
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("could not be read"), "{error}");
    for technical in ["os error", "No such file", "[Errno", "ledger.json"] {
        assert!(!error.contains(technical), "{error}");
    }
}

#[test]
fn issue_298_split_checks_cannot_replace_the_complete_success_scenario() {
    let root = temp();
    let scenario = "I can lend two books, return one, restart, and see only the other still out.";
    let goal = format!(
        "I track book loans.\nWhy: I need to know what is out.\nDone when: {scenario}\nMust: Keep existing loans."
    );
    let receipt = crate::seed_me::Config::new(Some(root.path().join("TEST-sessions")))
        .confirm(crate::seed_me::Affirmed {
            goal: goal.clone(),
            outcome: scenario.into(),
            options: vec![],
            originals: vec![],
            context: "{}".into(),
            source: "TEST fixture, not real approval".into(),
        })
        .unwrap();
    let session = receipt.session;
    let version = read(&session).unwrap().version();
    let prose =
        ledger::fixtures::prose_reply(include_str!("../../../tests/fixtures/shape-seed-reply.txt"));
    let check = |statement: &str| {
        json!({"statement":statement,"kind":"provisional",
        "source":"confirmed goal","expect":"The complete scenario passes.","cwd":".",
        "missing":"No behavior-specific test exists yet."})
    };
    // Captured #298 statements: each can pass while a returned loan reappears after restart.
    let split = json!([
        check("The list survives closing and reopening."),
        check("Only outstanding books are shown after one return.")
    ]);
    let before = fs::read(session.join("ledger.json")).unwrap();
    for weakened in [
        split,
        json!([check(
            "I can lend two books, return one, and see only the other still out."
        )]),
        json!([check(
            "I can lend two books, restart, return one, and see only the other still out."
        )]),
        json!([check(
            "I can lend two books, return one, restart, and see books still out."
        )]),
    ] {
        let error =
            draft_seed(&session, version, &prose, &weakened, &[], root.path(), None).unwrap_err();
        assert!(error.to_string().contains(scenario), "{error}");
        assert_eq!(fs::read(session.join("ledger.json")).unwrap(), before);
        assert!(!session.join("seed-checks.json").exists());
        assert!(!session.join("seed-contract.md").exists());
    }

    let complete = json!([check(scenario)]);
    draft_seed(&session, version, &prose, &complete, &[], root.path(), None).unwrap();
    let stored: Value =
        serde_json::from_slice(&fs::read(session.join("seed-checks.json")).unwrap()).unwrap();
    assert_eq!(stored, complete);
    assert!(
        fs::read_to_string(session.join("seed-contract.md"))
            .unwrap()
            .contains(scenario)
    );
    assert_eq!(read(&session).unwrap().goal(), Some(goal.as_str()));
}

#[test]
fn a_success_line_in_the_clause_form_the_goal_block_joined_keeps_its_words() {
    // #381: the clinic-tycoon slice 3 goal, and the statement Shape's model returned three times.
    let goal = "Strategically run and improve several clinics over time, balancing staffing, budget, and MAP/MAP Basic coverage.\nWhy: Make the experience closer to a game than a dashboard.\nDone when: Several clinics can be managed and improved over time; Staffing, budget, and MAP/MAP Basic coverage create strategic tradeoffs.";
    let check = |statement: &str| {
        ledger::Value::from(json!([{"statement": statement, "kind": "judge", "source": "TEST"}]))
    };
    ledger::validate_goal_checks(
        goal,
        &check("Several clinics can be managed and improved over time.\nStaffing, budget, and MAP/MAP Basic coverage create strategic tradeoffs."),
    )
    .unwrap();
    for weaker in [
        "Several clinics can be managed and improved over time.",
        "Staffing, budget, and MAP/MAP Basic coverage create strategic tradeoffs.\nSeveral clinics can be managed and improved over time.",
        "Several clinics can be managed over time.\nStaffing, budget, and MAP/MAP Basic coverage create strategic tradeoffs.",
    ] {
        assert!(
            ledger::validate_goal_checks(goal, &check(weaker))
                .unwrap_err()
                .contains("keep this complete Done when line unchanged"),
            "{weaker}"
        );
    }
    let split = ledger::Value::from(json!([
        {"statement": "Several clinics can be managed and improved over time.", "kind": "judge", "source": "TEST"},
        {"statement": "Staffing, budget, and MAP/MAP Basic coverage create strategic tradeoffs.", "kind": "judge", "source": "TEST"}
    ]));
    assert!(ledger::validate_goal_checks(goal, &split).is_err());
}

#[test]
fn success_lines_preserve_unicode_and_bullets_without_inventing_missing_requirements() {
    let goal = "I keep notes.\nDone when: Café notes survive restart; only mine remain.\n- 空の一覧 stays empty.\nMust: Keep originals.\n- This is a constraint, not another success criterion.";
    let checks = ledger::Value::from(json!([
        {"statement":"Café notes survive restart; only mine remain.","kind":"judge","source":"TEST"},
        {"statement":"空の一覧 stays empty.","kind":"judge","source":"TEST"},
        {"statement":"Additional regression.","kind":"judge","source":"TEST"}
    ]));
    ledger::validate_goal_checks(goal, &checks).unwrap();
    let only_first = ledger::Value::from(json!([
        {"statement":"Café notes survive restart; only mine remain.","kind":"judge","source":"TEST"}
    ]));
    assert!(
        ledger::validate_goal_checks(goal, &only_first)
            .unwrap_err()
            .contains("空の一覧 stays empty.")
    );
    ledger::validate_goal_checks("An older unstructured goal.", &only_first).unwrap();
    ledger::validate_goal_checks(
        "I keep notes.\nDone when: \nMust: Keep originals.",
        &only_first,
    )
    .unwrap();
}

#[test]
fn literal_goal_comments_in_headline() {
    literal_goal_round_trip("TEST <!-- example --> goal");
}

#[test]
fn literal_goal_comment_opener_in_headline() {
    literal_goal_round_trip("TEST <!-- goal");
}

#[test]
fn literal_goal_comments_in_must() {
    literal_goal_round_trip("TEST goal\nMust:\n- Keep <!-- example --> literal");
}

#[test]
fn literal_goal_comment_opener_in_must() {
    literal_goal_round_trip("TEST goal\nMust:\n- Keep <!-- literal");
}

fn literal_goal_round_trip(goal: &str) {
    let root = temp();
    let session = session_with_goal(root.path(), goal);
    let prose =
        ledger::fixtures::prose_reply(include_str!("../../../tests/fixtures/shape-seed-reply.txt"));
    let checks = json!([{"statement":"TEST", "kind":"judge", "source":"TEST"}]);
    let revision = draft_seed(&session, 1, &prose, &checks, &[], root.path(), None).unwrap();
    let path = session.join("seed-contract.md");
    let seed = fs::read_to_string(&path).unwrap();
    assert_eq!(read(&session).unwrap().goal(), Some(goal));
    assert!(seed.contains(&format!("# Seed: {}\n", goal.lines().next().unwrap())));
    assert!(seed.contains(&goal.split_whitespace().collect::<Vec<_>>().join(" ")));
    let origin = seed
        .lines()
        .find(|line| line.starts_with("- goal ["))
        .unwrap();
    let mut malformed_seeds = vec![
        seed.replace("## Open questions", "## Injected\n## Open questions"),
        seed.replace("## Open questions", "## Missing canonical section"),
        seed.replace(origin, &format!("{origin} changed")),
        seed.replace("# Seed:", "<!-- outside comment\n# Seed:"),
    ];
    let title = format!("# Seed: {}", goal.lines().next().unwrap());
    if title.contains("<!--") {
        malformed_seeds.push(seed.replace(&title, &format!("{title} changed")));
    }
    for malformed in malformed_seeds {
        fs::write(&path, &malformed).unwrap();
        assert!(
            confirm_seed(
                &session,
                read(&session).unwrap().version(),
                &revision,
                &malformed,
                "TEST seed only",
                None
            )
            .is_err()
        );
        assert!(!session.join("seed-confirmation.json").exists());
    }
    fs::write(&path, &seed).unwrap();
    confirm_seed(
        &session,
        read(&session).unwrap().version(),
        &revision,
        &seed,
        "TEST seed only",
        None,
    )
    .unwrap();
    assert_eq!(
        ledger::status(&session).unwrap()["handoff"]["verified"],
        ledger::Value::Bool(true)
    );
    assert_eq!(read(&session).unwrap().goal(), Some(goal));
    let ledger_path = session.join("ledger.json");
    let original = fs::read_to_string(&ledger_path).unwrap();
    let mut changed: Value = serde_json::from_str(&original).unwrap();
    changed["goal"] = format!("{goal} changed").into();
    fs::write(&ledger_path, serde_json::to_string(&changed).unwrap()).unwrap();
    assert!(
        ledger::status(&session).map_or(true, |status| status["handoff"]["verified"]
            != ledger::Value::Bool(true))
    );
}
