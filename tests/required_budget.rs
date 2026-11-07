//! A model host must not launch a harness without an explicit call allowance.

use std::{os::unix::fs::PermissionsExt, sync::atomic::AtomicBool};
use tinkery::{
    pi::Harness,
    think::{
        brain_dump::{BoardHost, BoardRequest, Source},
        drafting::ModelHost,
    },
};

#[test]
fn an_unbudgeted_host_must_not_start_a_harness() {
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
    let host = ModelHost::new(Harness::Pi, program, "TEST/stub".into()).unwrap();
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
    assert_eq!(started, 0, "an unbudgeted host started uncounted requests");
}
