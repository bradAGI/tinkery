//! A model host must not launch a harness without an explicit call allowance.

use std::{os::unix::fs::PermissionsExt, sync::atomic::AtomicBool};
use tinkery::{
    pi::{CallBudget, Harness},
    think::{
        brain_dump::{BoardHost, BoardRequest, Source},
        drafting::ModelHost,
    },
};

#[test]
fn a_zero_budget_refuses_before_the_stub_harness_starts() {
    let root = tempfile::Builder::new()
        .prefix("tinkery-TEST-required-budget-")
        .tempdir()
        .unwrap();
    let program = root.path().join("TEST-pi");
    let starts = root.path().join("TEST-starts");
    std::fs::write(
        &program,
        format!(
            r#"#!/usr/bin/env python3
import json, sys
with open({starts:?}, 'a') as started:
    started.write('started\n')
request = json.load(sys.stdin)
if request.get('kind') == 'decision-review':
    print('{{"decisions":[]}}')
else:
    print('{{"parts":{{"who":"I","outcome":"require a budget","why":"calls need permission","done_when":"no unbudgeted launches"}}}}')
"#,
            starts = starts.to_str().unwrap(),
        ),
    )
    .unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let budget = CallBudget::new(0);
    let host = ModelHost::new(Harness::Pi, program, "TEST/stub".into(), budget.clone()).unwrap();
    let request = BoardRequest {
        ask_counts: Default::default(),
        sources: vec![Source {
            id: 1,
            text: "TEST require a budget".into(),
            in_reply_to: None,
        }],
        fragments: vec![],
        previous: None,
        skipped: vec![],
        answered: vec![],
        settled: vec![],
        layout: vec![],
    };
    let result = host.reshape(request, &AtomicBool::new(false));
    let started = std::fs::read_to_string(starts)
        .unwrap_or_default()
        .lines()
        .count();
    eprintln!(
        "stub starts={started}; allowance={:?}; request succeeded={}",
        host.calls_left(),
        result.is_ok()
    );
    assert_eq!(started, 0, "a zero-budget host started a request");
    assert!(
        result
            .unwrap_err()
            .contains("Drafting run limit of 0 reached")
    );
    assert_eq!(budget.used(), 0);
    let attempts = budget.attempts();
    assert_eq!(attempts.len(), 1);
    assert_eq!(attempts[0].outcome, "refused");
    assert_eq!(
        host.calls_left().as_deref(),
        Some("Drafting runs left this launch: 0 of 0.")
    );
}
