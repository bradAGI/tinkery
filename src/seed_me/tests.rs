use super::*;
use serde_json::json;
use std::fs;

fn affirmed() -> Affirmed {
    Affirmed {
        goal: "TEST goal".into(),
        outcome: "PR ready".into(),
        options: vec![Choice {
            label: "A".into(),
            benefit: "b".into(),
            cost: "c".into(),
            undo_cost: "u".into(),
        }],
        originals: vec![
            Original {
                id: 0,
                in_reply_to: None,
                text: "  dump 👩‍💻\n".into(),
            },
            Original {
                id: 1,
                in_reply_to: Some("q0".into()),
                text: "answer".into(),
            },
        ],
        context: r#"{"TEST":"context"}"#.into(),
        source: "TEST typed confirm".into(),
    }
}

#[test]
fn confirmed_goal_and_originals_are_read_back_without_a_skill_or_viewer() {
    let root = tempfile::Builder::new()
        .prefix("tinkery-TEST-save-")
        .tempdir()
        .unwrap();
    let config = Config::new(Some(root.path().join("sessions")));
    let receipt = config.confirm(affirmed()).unwrap();
    assert_eq!(receipt.goal, affirmed().goal);
    let ledger = ledger::load(&receipt.session).unwrap().to_json().unwrap();
    let origin = &ledger["nodes"][0];
    assert_eq!(origin["answer"], affirmed().goal);
    assert_eq!(origin["authority"], "user");
    for original in affirmed().originals {
        assert!(
            origin["evidence"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["observed"] == original.text)
        );
    }
    assert!(!receipt.session.join("seed-contract.md").exists());
    assert!(!receipt.session.join("viewer.json").exists());
    let before = fs::read(receipt.session.join("ledger.json")).unwrap();
    assert_eq!(config.confirm(affirmed()).unwrap().session, receipt.session);
    assert_eq!(
        fs::read(receipt.session.join("ledger.json")).unwrap(),
        before
    );
    let mut different = affirmed();
    different.goal = "TEST other goal".into();
    assert!(
        config
            .confirm(different)
            .err()
            .unwrap()
            .contains("another goal")
    );
    assert_eq!(
        fs::read(receipt.session.join("ledger.json")).unwrap(),
        before
    );
    let mut changed = affirmed();
    changed.originals[0].text = "TEST changed".into();
    assert!(
        config
            .confirm(changed)
            .err()
            .unwrap()
            .contains("originals differ")
    );
    assert_eq!(
        fs::read(receipt.session.join("ledger.json")).unwrap(),
        before
    );
}

#[test]
fn session_operator_and_answers_share_the_authorized_actor() {
    let agent: crate::think_record::GoalConfirmation = serde_json::from_value(json!({
        "by":"agent", "agent":"TEST-agent", "authorizing_operator":"TEST-person",
        "source":"TEST scoped goal and Shape permission", "class":{"door":"two_way",
        "door_reason":"TEST reversible", "blast_radius":"small", "blast_radius_reason":"TEST isolated"}
    })).unwrap();
    for (context, operator, authority) in [
        (
            crate::think_record::GoalConfirmation::default(),
            "human",
            "user",
        ),
        (agent, "agent", "delegated"),
    ] {
        let root = tempfile::Builder::new()
            .prefix("tinkery-TEST-session-actor-")
            .tempdir()
            .unwrap();
        let config = Config::new(Some(root.path().join("TEST-sessions")));
        let receipt = config.confirm_as(affirmed(), &context).unwrap();
        let saved = ledger::load(&receipt.session).unwrap().to_json().unwrap();
        assert_eq!(saved["operator"], operator);
        assert_eq!(saved["nodes"][0]["authority"], authority);
        let before = fs::read(receipt.session.join("ledger.json")).unwrap();
        assert_eq!(
            config.confirm_as(affirmed(), &context).unwrap().session,
            receipt.session
        );
        assert_eq!(
            fs::read(receipt.session.join("ledger.json")).unwrap(),
            before
        );
        let version = answering::probe(&receipt.session, saved["version"].as_u64().unwrap())
            .unwrap()
            .unwrap();
        answering::answer(
            &receipt.session,
            version,
            answering::SAMPLE.0,
            "TEST exact answer\r\n ",
            Some(&context),
        )
        .unwrap();
        let saved = ledger::load(&receipt.session).unwrap().to_json().unwrap();
        assert_eq!(saved["operator"], operator);
        assert_eq!(saved["nodes"][1]["authority"], authority);
        assert_eq!(saved["nodes"][1]["answer"], "TEST exact answer\r\n ");
    }
}

#[test]
fn evidence_labels_are_plain_and_leave_original_bytes_untouched() {
    let mut original = Original {
        id: 2,
        text: "  both\n".into(),
        in_reply_to: Some("compounding-referent".into()),
    };
    assert_eq!(
        original.label(),
        "Tinkery original 2 / reply to compounding-referent"
    );
    assert_eq!(original.text, "  both\n");
    original.in_reply_to = None;
    assert_eq!(original.label(), "Tinkery original 2");
}

#[test]
fn reading_a_session_never_guesses() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(matches!(read(tmp.path()), Record::NotASession));
    fs::write(tmp.path().join("ledger.json"), r#"{"goal":"g"}"#).unwrap();
    assert!(matches!(read(tmp.path()), Record::Unreadable(_)));
    fs::write(
        tmp.path().join("ledger.json"),
        r#"{"status":"active","goal":"  "}"#,
    )
    .unwrap();
    fs::write(tmp.path().join("seed-contract.md"), "status: draft\n").unwrap();
    let Record::Session(session) = read(tmp.path()) else {
        panic!("a ledger with a status is a session")
    };
    assert_eq!(session.stage, Stage::Unknown);
    fs::write(
        tmp.path().join("ledger.json"),
        r#"{"status":"active","goal":"g","updated_at":"t"}"#,
    )
    .unwrap();
    let Record::Session(session) = read(tmp.path()) else {
        panic!("a ledger with a status is a session")
    };
    assert_eq!(
        (
            session.goal.as_str(),
            session.stage,
            session.updated.as_str()
        ),
        ("g", Stage::SeedDrafted, "t")
    );
}

#[test]
fn session_identity_comes_only_from_a_nonempty_control_free_ledger_string() {
    let tmp = tempfile::Builder::new()
        .prefix("tinkery-TEST-id-")
        .tempdir()
        .unwrap();
    for (value, expected) in [
        (Some(json!("TEST-id")), Some("TEST-id")),
        (Some(json!("  verbatim 界  ")), Some("  verbatim 界  ")),
        (None, None),
        (Some(json!(null)), None),
        (Some(json!(42)), None),
        (Some(json!("")), None),
        (Some(json!("bad\nline")), None),
        (Some(json!("bad\u{7f}")), None),
        (Some(json!("bad\u{85}")), None),
    ] {
        let mut ledger = json!({"status":"active", "goal":"Not the session id"});
        if let Some(value) = value {
            ledger["session_id"] = value;
        }
        fs::write(tmp.path().join("ledger.json"), ledger.to_string()).unwrap();
        let Record::Session(session) = read(tmp.path()) else {
            panic!("readable ledger")
        };
        assert_eq!(session.id.as_deref(), expected);
    }
}

#[test]
fn default_root_is_under_the_given_home() {
    for home in [None, Some(""), Some("TEST-relative"), Some(".")] {
        let error = default_root(home.map(std::ffi::OsStr::new)).unwrap_err();
        assert!(error.contains("--seed-session-root PATH"));
    }
    assert_eq!(
        default_root(Some(std::ffi::OsStr::new("/TEST/home"))).unwrap(),
        Path::new("/TEST/home/.local/share/tinkery/sessions")
    );
}

#[test]
fn an_empty_explicit_root_never_means_the_current_folder() {
    let error = resolve_root(Some(PathBuf::new())).unwrap_err();
    assert!(error.contains("--seed-session-root"), "{error}");
}
