//! Recheck #310's stale outside-view unknown, through stub steps and the real ledger.

use super::*;

const UNKNOWN: &str = "This investigation has not established whether the operator expects tally to guarantee read-only behavior for input files.";
const ANSWER: &str = "Most likely it would count words inconsistently or mishandle paths. Settle these choices now: Python 3 standard library only; words are non-empty whitespace-separated tokens using str.split; one or more explicit file paths, never stdin; directories are refused with a plain error. UTF-8 text only. Strictly read-only: never modify input files. On unreadable or invalid UTF-8 input, print a plain error to stderr and exit nonzero. Print COUNT PATH for each file and COUNT total at the end, including for one file. Supply tally.py invoked with python3 tally.py, unittest tests, and a short README example. No packaging or installation needed.";

#[cfg(unix)]
pub(super) fn resolved_read_only_step(question_first: bool) {
    use crate::seed_me::{
        Affirmed, Config,
        ledger::{self, fixtures::prose_reply},
    };
    use std::{fs, os::unix::fs::PermissionsExt};
    let root = tempfile::Builder::new()
        .prefix("TEST-resolved-read-only-")
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
                goal: "TEST tally counts words in files".into(),
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
    answering::probe(&session, version).unwrap().unwrap();
    // The outside view predates the answer; the fixed pre-mortem cannot link it at question time.
    let saved = ledger::load(&session).unwrap();
    let mut state: Value =
        serde_json::from_str(&ledger::editable(&saved).unwrap().pretty().unwrap()).unwrap();
    state["nodes"].as_array_mut().unwrap().push(json!({
        "id":"unknown-1", "kind":"fact", "status":"settled", "prerequisites":[],
        "claim":{"type":"unknown","scope":"outside-view probe"}, "answer":UNKNOWN,
        "authority":"evidence", "authority_source":"read-only investigation in Tinkery Shape",
        "evidence":[{"checked":"repository contents examined", "at":"2026-10-21T00:00:00Z", "observed":UNKNOWN}]
    }));
    let state = ledger::Value::from(state);
    let saved = ledger::publish(
        &session,
        &state,
        &saved["version"],
        &"TEST outside view".into(),
        None,
        "2026-10-21T00:00:00Z",
    )
    .unwrap();
    let before: Value = serde_json::from_str(&saved.pretty().unwrap()).unwrap();
    let version: u64 = saved["version"].pretty().unwrap().trim().parse().unwrap();
    let body = prose_reply(include_str!("../../../tests/fixtures/shape-seed-reply.txt"));
    let checks: Value =
        serde_json::from_str(include_str!("../../../tests/fixtures/shape-checks.json")).unwrap();
    let seed_reply = json!({"next":"seed", "confirming":body, "checks":checks, "found":[],
        "review":{"contradictions":[], "decided":["Strictly read-only: never modify input files."], "done":[],
        "failure_cases":checks.as_array().unwrap().iter().map(|_| json!("TEST wrong output must fail")).collect::<Vec<_>>()}});
    let program = root.path().join("TEST-pi");
    fs::write(&program, format!(r##"#!/usr/bin/env python3
import json, pathlib, sys, traceback
def failure(kind, value, trace):
    pathlib.Path(__file__).with_name('TEST-error').write_text(''.join(traceback.format_exception(kind, value, trace)))
sys.excepthook = failure
request = json.load(sys.stdin)
marker = pathlib.Path(__file__).with_name('TEST-called')
probe = next(n for n in request['ledger']['nodes'] if n['id'] == 'probe-pre-mortem')
assert probe['status'] == 'settled' and probe['authority'] == 'delegated'
assert probe['answer'] == {ANSWER:?}
assert all(n['kind'] != 'fact' for n in request['ledger']['nodes'])
unknowns = [n for n in request['found_so_far'] if n.get('claim', {{}}).get('type') == 'unknown']
reply = json.loads({seed_reply:?})
if not marker.exists():
    assert len(unknowns) == 1 and unknowns[0]['id'] == 'unknown-1'
    prompt = sys.argv[sys.argv.index('--system-prompt') + 1]
    assert 'optional resolves' in prompt
    if {question_first}:
        reply = {{'next':'question', 'id':'output-order', 'question':'TEST Which output order?', 'found':[]}}
    reply['resolves'] = [{{'unknown':'unknown-1', 'resolved_by':'probe-pre-mortem'}}]
    marker.write_text('TEST one step')
elif unknowns:
    # A later model that trusts open context would ask the settled question again.
    reply = {{'next':'question', 'id':'reasked-read-only', 'question':'Should tally be strictly read-only for the files it counts?', 'found':[]}}
print(json.dumps(reply))
"##, seed_reply=seed_reply.to_string(), question_first=if question_first {"True"} else {"False"})).unwrap();
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
    let agent = Agent::new(
        crate::pi::Harness::Pi,
        program,
        "test/stub".into(),
        String::new(),
        Some(root.path().into()),
        CallBudget::new(if question_first { 2 } else { 1 }),
        None,
    )
    .unwrap()
    .with_confirmation(Some(confirmation));
    let cancelled = AtomicBool::new(false);
    let result = run(
        &Work::Answer {
            id: answering::PROBE.0.into(),
            words: ANSWER.into(),
            expected: version,
            folder: root.path().into(),
        },
        &session,
        &agent,
        &cancelled,
        &AtomicBool::new(false),
    );
    let result = result.unwrap_or_else(|error| {
        panic!(
            "{error:?}: {}",
            fs::read_to_string(root.path().join("TEST-error")).unwrap_or_default()
        )
    });
    let read = answering::read(&session).unwrap();
    let context = input(&read, vec![]);
    assert!(
        !context.to_string().contains(UNKNOWN),
        "resolved unknown was sent as open"
    );
    if question_first {
        assert!(matches!(result, Done::Asked(_)));
        assert_eq!(
            run(
                &Work::Answer {
                    id: "output-order".into(),
                    words: "TEST path order".into(),
                    expected: read.version(),
                    folder: root.path().into()
                },
                &session,
                &agent,
                &cancelled,
                &AtomicBool::new(false)
            ),
            Ok(Done::Drafted),
            "settled read-only choice was re-asked"
        );
    } else {
        assert_eq!(result, Done::Drafted);
    }
    let current: Value =
        serde_json::from_slice(&fs::read(session.join("ledger.json")).unwrap()).unwrap();
    let fact = |state: &Value| {
        state["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["id"] == "unknown-1")
            .unwrap()
            .clone()
    };
    let original = fact(&before);
    let resolved = fact(&current);
    assert_eq!(resolved["resolved_by"], "probe-pre-mortem");
    assert_eq!(resolved["answer"], UNKNOWN);
    assert_eq!(resolved["claim"], original["claim"]);
    assert_eq!(resolved["evidence"], original["evidence"]);
    assert_eq!(
        resolved["history"].as_array().unwrap().last().unwrap()["state"]["answer"],
        UNKNOWN
    );
    assert!(
        resolved["history"].as_array().unwrap().last().unwrap()["state"]
            .get("resolved_by")
            .is_none()
    );
    let seed = fs::read_to_string(session.join("seed-contract.md")).unwrap();
    assert!(
        !seed.contains(UNKNOWN),
        "resolved unknown leaked into seed Decisions or Evidence"
    );
    assert!(!seed.contains("unknown-1"));
    assert!(seed.contains("Strictly read-only: never modify input files."));
    assert_eq!(agent.remaining(), 0);
}

#[test]
fn resolution_reply_field_is_optional_but_never_silently_coerced() {
    for next in ["question", "seed"] {
        let mut reply = json!({"next":next, "question":"TEST next?", "found":[], "confirming":{},
            "checks":[{"statement":"TEST", "kind":"judge", "source":"TEST"}],
            "review":{"contradictions":[],"decided":[],"done":[],"failure_cases":["TEST wrong output must fail"]}});
        for links in [
            None,
            Some(Value::Null),
            Some(json!("not a list")),
            Some(json!([{"unknown":"gap", "resolved_by":"probe-pre-mortem"}])),
        ] {
            if let Some(links) = &links {
                reply["resolves"] = links.clone();
            }
            let decoded = decode(&reply.to_string(), "").unwrap();
            let actual = match decoded {
                Next::Question(ask) => ask.resolves,
                Next::Seed { resolves, .. } => resolves,
            };
            assert_eq!(actual, links);
        }
    }
}
