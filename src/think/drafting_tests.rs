use super::*;
use std::time::Instant;

fn input(text: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({"words": [text]})).unwrap()
}

#[cfg(unix)]
fn host(script: &str, budget: Arc<CallBudget>) -> (tempfile::TempDir, ModelHost) {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir().unwrap();
    let program = directory.path().join("pi");
    std::fs::write(&program, format!("#!/usr/bin/env python3\n{script}\n")).unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let host = ModelHost::new(
        crate::pi::Harness::Pi,
        program,
        "anthropic/claude-haiku-4-5".into(),
        budget,
    )
    .unwrap();
    (directory, host)
}

#[cfg(unix)]
#[test]
fn pi_process_receives_only_bounded_inputs_and_disabled_resources() {
    let script = r#"
import json, os, sys
required = ['--print', '--no-session', '--no-tools', '--no-extensions', '--no-mcp', '--no-skills', '--no-prompt-templates', '--no-themes', '--no-context-files', '--no-approve', '--offline']
assert all(flag in sys.argv for flag in required)
assert 'PI_SESSION_FILE' not in os.environ
assert 'PI_SESSION_ID' not in os.environ
instructions = sys.argv[sys.argv.index('--system-prompt') + 1]
assert 'Distill whatever arrived' in instructions
assert 'DO NOT RUN THIS LATER STEP' not in instructions
assert sys.argv[sys.argv.index('--thinking') + 1] == 'low'
assert sys.argv[sys.argv.index('--model') + 1] == 'anthropic/claude-haiku-4-5'
data = json.load(sys.stdin)
assert data['words'] == ['I want a graph, so I can find reasons.']
assert not any('I want a graph' in arg for arg in sys.argv)
print('done')
"#;
    let (directory, host) = host(script, CallBudget::new(1));
    assert!(
        ModelHost::new(
            crate::pi::Harness::Pi,
            directory.path().join("pi"),
            "test/model".into(),
            CallBudget::new(0),
        )
        .unwrap()
        .with_thinking("automatic")
        .is_err()
    );
    let before = std::fs::read_dir(directory.path()).unwrap().count();
    let prompt = host.working_instructions().to_owned();
    let output = host
        .complete(
            Request::Shaping,
            input("I want a graph, so I can find reasons."),
            prompt,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(output.trim(), "done");
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), before);
}

#[cfg(unix)]
#[test]
fn failures_timeout_overflow_and_cancellation_never_return_output() {
    let call = |host: &ModelHost, text: &str, cancelled: &AtomicBool| {
        host.complete(Request::Shaping, input(text), "prompt".into(), cancelled)
    };
    for script in [
        "import sys; sys.exit(7)",
        "print('x' * 40000)",
        "import time; time.sleep(10)",
    ] {
        let (_directory, mut host) = host(script, CallBudget::new(1));
        host.timeout = Duration::from_millis(100);
        assert!(
            call(&host, "words", &AtomicBool::new(false)).is_err(),
            "Accepted {script}"
        );
    }
    let (_directory, limited) = host(
        "import sys; print('429 rate_limit_error', file=sys.stderr); sys.exit(1)",
        CallBudget::new(1),
    );
    assert!(
        call(&limited, "words", &AtomicBool::new(false))
            .unwrap_err()
            .contains("rate limit (429)")
    );
    assert!(
        call(
            &limited,
            &"x".repeat(REQUEST_LIMIT + 1),
            &AtomicBool::new(false)
        )
        .unwrap_err()
        .contains("This is too much to send at once.")
    );
    let (_directory, host) = host("import time; time.sleep(10)", CallBudget::new(1));
    let job = Job::launch(move |cancelled| call(&host, "words", cancelled));
    thread::sleep(Duration::from_millis(50));
    let started = Instant::now();
    drop(job);
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "Exit waited for the model instead of cancelling"
    );
}

/// A Pi that leaves one file per start in the returned folder, then prints `reply`, or exits
/// with 3 when `reply` is empty. The files show exactly how many processes ran.
#[cfg(unix)]
fn counting_host(
    reply: &str,
    budget: Arc<CallBudget>,
) -> (tempfile::TempDir, ModelHost, tempfile::TempDir) {
    let starts = tempfile::tempdir().unwrap();
    let (directory, host) = host(
        &format!(
            "import os, sys, uuid\nopen(os.path.join({:?}, uuid.uuid4().hex), 'w').close()\nprint({reply:?}) if {reply:?} else sys.exit(3)",
            starts.path().display().to_string()
        ),
        budget,
    );
    (directory, host, starts)
}

#[cfg(unix)]
fn started(starts: &tempfile::TempDir) -> usize {
    std::fs::read_dir(starts.path()).unwrap().count()
}

#[cfg(unix)]
#[test]
fn a_budget_refuses_before_pi_starts_and_counts_failures_and_retries() {
    let call = |host: &ModelHost, kind| {
        host.complete(
            kind,
            input("words"),
            "prompt".into(),
            &AtomicBool::new(false),
        )
    };
    for limit in [0, 1, 2] {
        let budget = CallBudget::new(limit);
        let (_directory, pi, starts) = counting_host("done", budget.clone());
        // One send is a shaping call then a review; a third request is a retry.
        let results = [
            call(&pi, Request::Shaping),
            call(&pi, Request::DecisionReview),
            call(&pi, Request::Shaping),
        ];
        assert_eq!(started(&starts), limit, "limit {limit}");
        assert_eq!(budget.used(), limit);
        for (i, result) in results.iter().enumerate() {
            assert_eq!(result.is_ok(), i < limit, "limit {limit}, request {i}");
        }
        let attempts = budget.attempts();
        assert_eq!(attempts.len(), 3);
        assert!(attempts[limit..].iter().all(|a| a.outcome == "refused"));
        assert_eq!(attempts[1].kind, Request::DecisionReview);
    }
    // A failed call still spends its unit, so a retry after it is refused.
    let budget = CallBudget::new(1);
    let (_directory, pi, starts) = counting_host("", budget.clone());
    assert!(call(&pi, Request::Shaping).is_err());
    assert!(
        call(&pi, Request::Shaping)
            .unwrap_err()
            .contains("Drafting run limit of 1 reached")
    );
    assert_eq!(started(&starts), 1);
    assert_eq!(budget.attempts()[0].outcome, "failure");
}

#[cfg(unix)]
#[test]
fn a_new_session_shares_its_budget_and_cancellation_is_counted_only_once_started() {
    let budget = CallBudget::new(1);
    let (_directory, first, starts) = counting_host("done", budget.clone());
    let other = first.new_session();
    let cancelled = AtomicBool::new(true);
    assert_eq!(
        first.complete(
            Request::Shaping,
            input("words"),
            "prompt".into(),
            &cancelled
        ),
        Err("Request cancelled.".into())
    );
    assert_eq!(
        budget.used(),
        0,
        "a request cancelled before it starts costs nothing"
    );
    let fresh = AtomicBool::new(false);
    assert!(
        first
            .complete(Request::Shaping, input("words"), "prompt".into(), &fresh)
            .is_ok()
    );
    assert!(
        other
            .complete(Request::Shaping, input("words"), "prompt".into(), &fresh)
            .is_err()
    );
    assert_eq!(started(&starts), 1, "a new session can't reset the budget");

    let budget = CallBudget::new(2);
    let (_directory, slow) = host("import time; time.sleep(10)", budget.clone());
    let job = Job::launch(move |cancelled| {
        slow.complete(Request::Shaping, input("words"), "prompt".into(), cancelled)
    });
    thread::sleep(Duration::from_millis(100));
    drop(job);
    assert_eq!(
        budget.used(),
        1,
        "a started request stays spent when cancelled"
    );
}
