//! Reply transport tests; semantic fact rules remain in the shared ledger.

use super::*;

/// Why a seed reply's completion review is unusable; decoding leaves that to the check rules.
pub(super) fn review_error(reply: &Value) -> String {
    match decode(&reply.to_string(), "") {
        Ok(Next::Seed { review, .. }) => review.unwrap_err(),
        other => panic!("not a seed reply: {:?}", other.err()),
    }
}

#[test]
fn session_sized_step_sends_findings_once_without_changing_saved_context() {
    use crate::seed_me::{Affirmed, Config, Original};
    let root = tempfile::Builder::new()
        .prefix("TEST-send-once-")
        .tempdir()
        .unwrap();
    let context = "TEST Think context: ".to_owned() + &"c".repeat(3920);
    let goal = "TEST goal: ".to_owned() + &"g".repeat(1015);
    let actor: crate::think_record::GoalConfirmation = serde_json::from_value(json!({
        "by":"agent", "agent":"TEST-agent", "authorizing_operator":"TEST-person",
        "source":"TEST scoped authorization: ".to_owned() + &"s".repeat(200),
        "class":{"door":"two_way", "door_reason":"TEST reversible scratch session",
        "blast_radius":"small", "blast_radius_reason":"TEST isolated scratch roots"}
    }))
    .unwrap();
    let session = Config::new(Some(root.path().join("TEST-sessions")))
        .confirm_as(
            Affirmed {
                goal: goal.clone(),
                outcome: "TEST outcome".into(),
                options: vec![],
                originals: vec![Original {
                    id: 1,
                    in_reply_to: None,
                    text: "TEST original: ".to_owned() + &"o".repeat(700),
                }],
                context: context.clone(),
                source: actor.source(),
            },
            &actor,
        )
        .unwrap()
        .session;
    let path = session.join("ledger.json");
    let mut saved: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let origin = saved["nodes"][0].clone();
    let actor = actor.actor().unwrap();
    let receipt = json!({"checked":"Caller-declared authorization, not authenticated identity",
        "at":"2026-10-21T00:00:00Z", "observed":actor.context().unwrap().details()});
    // The dogfood had nine facts and four agent answers; these payloads are slightly larger.
    let all = saved["nodes"].as_array_mut().unwrap();
    for index in 0..9 {
        let observed = format!("TEST finding {index}: {}", "f".repeat(300));
        all.push(json!({"id":format!("fact-{index}"), "kind":"fact", "status":"settled",
            "prerequisites":[], "answer":observed, "authority":"evidence", "authority_source":"TEST read-only investigation",
            "claim":{"type":"observation", "scope":"TEST README.md:1"},
            "evidence":[{"checked":"TEST README.md:1", "at":"2026-10-21T00:00:00Z", "observed":observed}],
            "history":[], "revision":0}));
    }
    for index in 0..4 {
        all.push(json!({"id":format!("answer-{index}"), "kind":"decision", "status":"settled",
            "prerequisites":[], "owner":"User", "gate":"TEST settled choice", "label":format!("TEST choice {index}"),
            "question":"TEST question: ".to_owned() + &"q".repeat(300),
            "recommendation":"TEST suggestion: ".to_owned() + &"r".repeat(400),
            "answer":"TEST answer: ".to_owned() + &"a".repeat(700), "authority":actor.authority(),
            "authority_source":actor.reason("answered with these words in Tinkery Shape."),
            "evidence":[receipt], "supported_by":["fact-0"], "history":[], "revision":0}));
    }
    std::fs::write(&path, serde_json::to_vec(&saved).unwrap()).unwrap();
    let before = std::fs::read(&path).unwrap();
    let ledger = answering::read(&session).unwrap();
    let think = vec![json!({"question":"TEST Think question", "answer":"t".repeat(250)}); 2];
    let request = input(&ledger, think.clone());
    let mut duplicated = request.clone();
    duplicated["ledger"] = serde_json::from_str(&ledger.context()).unwrap();
    let before_size = serde_json::to_vec(&duplicated).unwrap().len();
    let after_size = serde_json::to_vec(&request).unwrap().len();
    eprintln!("TEST session-sized step: duplicated={before_size}, send-once={after_size}");
    assert!(
        before_size >= 37451,
        "fixture must cover the measured dogfood size"
    );
    assert!(
        after_size <= pi::REQUEST_LIMIT,
        "{after_size} exceeds the unchanged request limit"
    );
    assert_eq!(request["goal"], goal);
    assert_eq!(request["answered_in_think"], json!(think));
    let decisions = request["ledger"]["nodes"].as_array().unwrap();
    assert_eq!(decisions.len(), 5);
    assert!(decisions.iter().all(|n| n["kind"] == "decision"));
    let mut original = origin;
    for field in ["history", "revision", "premise_version", "reviewed_version"] {
        original.as_object_mut().unwrap().remove(field);
    }
    assert_eq!(decisions[0], original, "goal provenance must stay verbatim");
    assert!(
        decisions[0]["evidence"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["observed"] == context)
    );
    assert!(decisions[1..].iter().all(|n| n["evidence"] == json!([])));
    let findings = request["found_so_far"].as_array().unwrap();
    assert_eq!(
        findings.len(),
        10,
        "nine facts and one deduplicated attribution receipt"
    );
    for index in 0..9 {
        assert_eq!(
            findings
                .iter()
                .filter(|n| n["id"] == format!("fact-{index}"))
                .count(),
            1
        );
    }
    assert_eq!(std::fs::read(path).unwrap(), before);
    assert_eq!(
        ledger.context(),
        answering::read(&session).unwrap().context()
    );
}

#[cfg(unix)]
#[test]
fn delegated_pre_mortem_choices_reach_the_seed_as_settled() {
    settled_probe_step();
}

#[cfg(unix)]
#[test]
fn answered_pre_mortem_is_sent_once_and_not_reasked() {
    super::unknown_tests::resolved_read_only_step(true);
    super::unknown_tests::resolved_read_only_step(false);
}

#[cfg(unix)]
fn settled_probe_step() {
    use crate::seed_me::{Affirmed, Config, ledger::fixtures::prose_reply};
    use std::{fs, os::unix::fs::PermissionsExt};
    let root = tempfile::Builder::new()
        .prefix("TEST-settled-probe-")
        .tempdir()
        .unwrap();
    let confirmation: crate::think_record::GoalConfirmation = serde_json::from_value(json!({
        "by":"agent", "agent":"TEST-agent", "authorizing_operator":"TEST-person",
        "source":"TEST permission to plan this disposable CLI",
        "class":{"door":"two_way", "door_reason":"TEST disposable plan",
        "blast_radius":"small", "blast_radius_reason":"TEST root only"}
    }))
    .unwrap();
    let session = Config::new(Some(root.path().join("TEST-sessions")))
        .confirm_as(
            Affirmed {
                goal: "TEST count words in files".into(),
                outcome: "TEST CLI".into(),
                options: vec![],
                originals: vec![],
                context: "{}".into(),
                source: confirmation.source(),
            },
            &confirmation,
        )
        .unwrap()
        .session;
    let version = answering::read(&session).unwrap().version();
    let version = answering::probe(&session, version).unwrap().unwrap();
    let version = answering::answer(
        &session,
        version,
        answering::SAMPLE.0,
        "TEST no sample",
        None,
    )
    .unwrap();
    let version = answering::probe(&session, version).unwrap().unwrap();
    let answer = "TEST Use standard-library Python 3. Reject directories, continue other inputs, exit 1 and omit total on any input failure.\n";
    let version = answering::answer(
        &session,
        version,
        answering::PROBE.0,
        answer,
        Some(&confirmation),
    )
    .unwrap();
    let ledger_path = session.join("ledger.json");
    let before = fs::read(&ledger_path).unwrap();
    let body = prose_reply(include_str!("../../../tests/fixtures/shape-seed-reply.txt"));
    let checks: Value =
        serde_json::from_str(include_str!("../../../tests/fixtures/shape-checks.json")).unwrap();
    let reply = json!({"next":"seed", "confirming":body, "checks":checks, "found":[],
        "review":{"contradictions":[], "decided":[], "done":[],
        "failure_cases":checks.as_array().unwrap().iter().map(|_| json!("TEST wrong output must fail")).collect::<Vec<_>>()}});
    let program = root.path().join("TEST-pi");
    fs::write(&program, format!(r##"#!/usr/bin/env python3
import json, sys
raw = sys.stdin.read()
request = json.loads(raw)
probe = next(n for n in request['ledger']['nodes'] if n['id'] == 'probe-pre-mortem')
assert probe['status'] == 'settled' and probe['authority'] == 'delegated'
assert probe['answer'] == {answer:?}
assert probe['question'] == 'Imagine this shipped and turned out wrong. What is the most likely reason?'
# Both verbatim strings have one transport owner, the decision node.
assert raw.count(json.dumps(probe['answer'], ensure_ascii=False)[1:-1]) == 1
assert raw.count(probe['question']) == 1
assert len(raw.encode()) <= 32768
prompt = sys.argv[sys.argv.index('--system-prompt') + 1]
reply = json.loads({reply:?})
settled = 'Carry settled Think and ledger answers, including delegated choices, into the seed as settled; never list them as unknown.' in prompt
reply['confirming']['Knowledge map'] = 'Settled: standard-library Python 3; see Decisions.' if settled else 'Language is unknown.'
print(json.dumps(reply))
"##, reply = reply.to_string())).unwrap();
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
    let agent = Agent::new(
        crate::pi::Harness::Pi,
        program,
        "test/stub".into(),
        String::new(),
        Some(root.path().into()),
        CallBudget::new(1),
        None,
    )
    .unwrap();
    let result = run(
        &Work::Look {
            feedback: None,
            expected: version,
            folder: root.path().into(),
        },
        &session,
        &agent,
        &AtomicBool::new(false),
        &AtomicBool::new(false),
    );
    assert_eq!(
        result,
        Ok(Done::Drafted),
        "the answered pre-mortem was re-asked"
    );
    let seed = fs::read_to_string(session.join("seed-contract.md")).unwrap();
    assert!(seed.contains("Settled: standard-library Python 3; see Decisions."));
    assert!(!seed.contains("Language is unknown."));
    assert!(seed.contains("probe-pre-mortem [delegated] TEST Use standard-library Python 3."));
    assert_eq!(agent.remaining(), 0);
    // Draft receipts may append to goal provenance; the settled answer itself stays verbatim.
    let previous: Value = serde_json::from_slice(&before).unwrap();
    let current: Value = serde_json::from_slice(&fs::read(ledger_path).unwrap()).unwrap();
    let probe = |state: &Value| {
        state["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["id"] == answering::PROBE.0)
            .unwrap()
            .clone()
    };
    assert_eq!(probe(&previous), probe(&current));
}

#[test]
fn zero_budget_refuses_revision_before_reading_or_publishing() {
    let root = tempfile::Builder::new()
        .prefix("TEST-revise-budget-")
        .tempdir()
        .unwrap();
    let ledger = root.path().join("ledger.json");
    std::fs::write(&ledger, "TEST untouched ledger").unwrap();
    let agent = Agent::new(
        crate::pi::Harness::Pi,
        root.path().join("TEST-never-run"),
        "test/model".into(),
        String::new(),
        Some(root.path().into()),
        CallBudget::new(0),
        None,
    )
    .unwrap();
    let work = Work::Respond {
        id: "TEST-choice".into(),
        expected: 1,
        action: crate::seed_me::ledger::AnswerAction::Revise("TEST replacement".into()),
        folder: root.path().into(),
    };
    let saved = AtomicBool::new(false);
    assert_eq!(
        run(&work, root.path(), &agent, &AtomicBool::new(false), &saved),
        Err(Failure {
            answer_saved: false,
            why: Why::Budget(0),
            feedback: None,
        })
    );
    assert!(!saved.load(Ordering::Relaxed));
    assert_eq!(
        std::fs::read_to_string(ledger).unwrap(),
        "TEST untouched ledger"
    );
}

#[test]
fn teach_back_zero_budget_refuses_before_capture_or_any_read() {
    let root = tempfile::Builder::new()
        .prefix("TEST-teach-back-budget-")
        .tempdir()
        .unwrap();
    let path = root.path().join("ledger.json");
    std::fs::write(&path, "TEST untouched").unwrap();
    let agent = Agent::new(
        crate::pi::Harness::Pi,
        root.path().join("TEST-never-run"),
        "test/model".into(),
        String::new(),
        Some(root.path().into()),
        CallBudget::new(0),
        None,
    )
    .unwrap();
    let work = Work::TeachBack {
        revision: "r1".into(),
        words: "TEST exact words\r\n".into(),
        expected: 1,
        seed: "TEST draft".into(),
    };
    let saved = AtomicBool::new(false);
    assert_eq!(
        run(&work, root.path(), &agent, &AtomicBool::new(false), &saved),
        Err(Failure {
            answer_saved: false,
            why: Why::Budget(0),
            feedback: None,
        })
    );
    assert!(!saved.load(Ordering::Relaxed));
    assert_eq!(std::fs::read_to_string(path).unwrap(), "TEST untouched");
}

#[test]
fn a_seed_reply_requires_a_typed_completion_review_without_parser_diagnostics() {
    let mut reply = json!({"next":"seed", "found":[], "confirming":{},
        "checks":[{"statement":"TEST exact", "kind":"judge", "source":"TEST"}]});
    for review in [
        Value::Null,
        json!("reviewed"),
        json!({"contradictions":[]}),
        json!({"contradictions":[], "decided":[], "done":[], "failure_cases":[], "extra":true}),
    ] {
        reply["review"] = review;
        let error = review_error(&reply);
        assert!(error.contains("typed completion review"), "{error}");
        assert!(
            !error.contains("invalid type")
                && !error.contains("missing field")
                && !error.contains("unknown field")
        );
    }
    reply.as_object_mut().unwrap().remove("review");
    assert!(review_error(&reply).contains("typed completion review"));
    reply["review"] = json!({"contradictions":[], "decided":["TEST choice"], "done":[],
        "failure_cases":["TEST wrong result must fail"]});
    let Next::Seed { review, checks, .. } = decode(&reply.to_string(), "").unwrap() else {
        panic!()
    };
    let review = review.unwrap();
    review
        .validate(&crate::seed_me::ledger::Value::parse(&checks.to_string()).unwrap())
        .unwrap();
    let prompt = prompt();
    for rule in [
        "Before drafting in this same step",
        "Separate decided",
        "one case that must fail",
        "a new blocker returns as the next question, never a seed",
        "never a separate call",
    ] {
        assert!(prompt.contains(rule), "{rule}");
    }
}

#[test]
fn decodes_each_claim_and_keeps_support_and_contradiction_references() {
    let reply = json!({"next":"question","id":"choice","question":"TEST choose?",
    "supported_by":["read","inferred"], "found":[
        {"id":"read","claim":{"type":"observation","scope":"README:1"},
            "checked":"README.md:1","observed":"TEST local"},
        {"id":"inferred","claim":{"type":"inference","scope":"TEST local path","limits":"not a runtime check"},
            "checked":"README.md:1","observed":"TEST likely local","supported_by":["read"],"contradicts":"earlier"},
        {"id":"gap","claim":{"type":"unknown","scope":"TEST sync"},
            "checked":"README.md:1","observed":"This investigation has not established sync."}
    ]});
    let Next::Question(ask) = decode(&reply.to_string(), "").unwrap() else {
        panic!("not a question")
    };
    assert_eq!(ask.supported_by, Some(json!(["read", "inferred"])));
    for (found, expected) in ask
        .found
        .iter()
        .zip(["observation", "inference", "unknown"])
    {
        assert_eq!(found.claim["type"], expected);
    }
    assert_eq!(ask.found[1].supported_by, Some(json!(["read"])));
    assert_eq!(ask.found[1].contradicts, Some(json!("earlier")));
    assert_eq!(ask.found[1].claim["limits"], "not a runtime check");
}

#[test]
fn question_prerequisites_pass_unchanged_to_the_ledger_boundary() {
    let mut reply = json!({"next":"question","question":"TEST?","found":[]});
    let Next::Question(legacy) = decode(&reply.to_string(), "").unwrap() else {
        panic!()
    };
    assert_eq!(legacy.prerequisites, None);
    for proposed in [
        json!(["b", "a"]),
        Value::Null,
        json!("not a list"),
        json!([42]),
    ] {
        reply["prerequisites"] = proposed.clone();
        let Next::Question(ask) = decode(&reply.to_string(), "").unwrap() else {
            panic!()
        };
        assert_eq!(ask.prerequisites, Some(proposed));
    }
}

#[test]
fn untyped_or_partial_findings_are_refused_never_dropped_or_guessed() {
    for found in [
        json!([{"checked":"README.md:1","observed":"TEST local"}]),
        json!([{"id":"read","claim":{"type":"observation","scope":"TEST"},"checked":"README.md:1"}]),
        json!([{"id":"read","claim":{"type":"observation","scope":"TEST"},"checked":"README.md:1","observed":"TEST", "authority":"user"}]),
        json!({}),
    ] {
        let parser_error = serde_json::from_value::<Vec<Found>>(found.clone())
            .unwrap_err()
            .to_string();
        let error = decode(
            &json!({"next":"question","question":"TEST?","found":found}).to_string(),
            "",
        )
        .unwrap_err();
        assert!(error.contains("typed claim"), "{error}");
        assert!(!error.contains(&parser_error), "{error}");
        assert!(error.ends_with("Retry the step."), "{error}");
    }
}

#[test]
fn verified_think_origin_selects_the_same_folder_or_existing_fallback() {
    use crate::{
        seed_me::{Affirmed, Config},
        think_record::{self, Origin, Session},
    };
    let root = tempfile::Builder::new()
        .prefix("tinkery-TEST-shape-origin-")
        .tempdir()
        .unwrap();
    let goal = "TEST exact saved goal";
    let session = Config::new(Some(root.path().join("TEST-sessions")))
        .confirm(Affirmed {
            goal: goal.into(),
            outcome: goal.into(),
            options: vec![],
            originals: vec![],
            context: "{}".into(),
            source: "TEST fixture".into(),
        })
        .unwrap()
        .session;
    let records = root.path().join("TEST-records");
    let repo = root.path().join("TEST-origin-repo");
    let fallback = root.path().join("TEST-fallback");
    let record = think_record::write(
        &records,
        &Session {
            origin: &Origin::simulated(),
            turns: &[],
            checks: vec![],
            goal,
            seed_me_session: &session,
            repo: Some(&repo),
            confirmed: std::time::UNIX_EPOCH,
        },
    )
    .unwrap();
    let original = std::fs::read(&record).unwrap();
    let agent = Agent::new(
        Harness::Pi,
        root.path().join("TEST-never-run"),
        "TEST/model".into(),
        String::new(),
        Some(fallback.clone()),
        CallBudget::new(0),
        Some(records),
    )
    .unwrap();
    assert_eq!(agent.folder(&session, goal), repo);
    for variant in ["no repo", "old schema", "wrong goal", "missing"] {
        let mut value: Value = serde_json::from_slice(&original).unwrap();
        match variant {
            "no repo" => value["repo"] = Value::Null,
            "old schema" => {
                value["schema"] = 2.into();
                value.as_object_mut().unwrap().remove("repo");
            }
            "wrong goal" => value["goal"] = "TEST changed record goal".into(),
            "missing" => {
                std::fs::remove_file(&record).unwrap();
            }
            _ => unreachable!(),
        }
        if variant != "missing" {
            std::fs::write(&record, serde_json::to_vec(&value).unwrap()).unwrap();
        }
        assert_eq!(agent.folder(&session, goal), fallback, "{variant}");
        std::fs::write(&record, &original).unwrap();
    }
    assert_eq!(agent.remaining(), 0);
    assert!(!root.path().join("TEST-never-run").exists());
}
