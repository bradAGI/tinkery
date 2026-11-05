use super::goal_tests::{board, right_text};
use super::*;
fn key(a: &mut BrainDump, code: KeyCode) {
    a.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
}
fn left(screen: &str, w: u16) -> Vec<String> {
    screen
        .lines()
        .map(|line| line.chars().take(usize::from(w * 48 / 100)).collect())
        .collect()
}
#[test]
fn reading_scroll_hint_only_appears_on_overflow_and_clears_after_resize() {
    const HINT: &str = "PgUp/PgDn scroll reading";
    for mono in [false, true] {
        let mut a = board();
        super::tests::amend_board(&mut a, |g| {
            g.parts = GoalParts {
                who: Some("I".into()),
                outcome: Some("read the boundaries".into()),
                why: Some("They protect my work".into()),
                done_when: Some("Each exclusion is visible".into()),
                must_not: vec!["delete files".into(), "upload logs".into()],
                ..Default::default()
            };
            g.other_goals.clear();
            g.deferred.clear();
        });
        // Six rows exactly fit the minimum-size reading: don't take one for a hint.
        let fitted = snapshot(80, 24, &mut a, mono).unwrap();
        assert!(fitted.contains("Must not: upload logs."));
        assert!(!fitted.contains(HINT));
        super::tests::amend_board(&mut a, |g| {
            g.parts.must_not.push(
                "change the café 東京 archive, including its names, notes and every original entry"
                    .into(),
            );
        });
        let goal = a.conversation.board().unwrap().goal().unwrap().to_owned();
        let overflow = snapshot(80, 24, &mut a, mono).unwrap();
        assert!(overflow.contains(HINT), "{overflow}");
        assert!(!overflow.contains("original entry"));
        key(&mut a, KeyCode::PageDown);
        let bottom = snapshot(80, 24, &mut a, mono).unwrap();
        assert!(bottom.contains("original entry."), "{bottom}");
        assert!(bottom.contains('東') && bottom.contains('京'), "{bottom}");
        assert!(bottom.contains(HINT));
        assert!(a.screen.paper_scroll > 0);
        let expanded = snapshot(320, 40, &mut a, mono).unwrap();
        assert!(!expanded.contains(HINT));
        assert!(expanded.contains("I think you mean…"));
        assert!(expanded.contains("original entry."));
        assert_eq!(a.screen.paper_scroll, 0);
        a.review_goal();
        assert_eq!(a.handoff.review().unwrap().affirmation.goal, goal);
        assert!(!snapshot(80, 24, &mut a, mono).unwrap().contains(HINT));
        assert!(!a.handoff.saving() && a.saved().is_none() && !a.running());
    }
}

const HEALTHCARE_QUESTION: &str = "What would make this loop satisfying to you: finding a workable balance for different patient populations, understanding consequences across the network, making difficult funding tradeoffs, or some combination?";

#[test]
fn active_question_is_readable_without_review_and_survives_resize() {
    for mono in [false, true] {
        for question in [
            HEALTHCARE_QUESTION.to_owned(),
            format!(
                "{} Last question words?",
                "Which population needs care? ".repeat(40)
            ),
        ] {
            let mut a = board();
            super::tests::amend_board(&mut a, |g| {
                g.parts.must = vec!["preserve citywide care ".repeat(30)];
                g.questions = vec![Question {
                    suggestion: None,
                    id: "city-care".into(),
                    target: None,
                    text: question.clone(),
                }];
            });
            a.paste(&format!("{}Café 東京", "My unsent answer\r\n".repeat(40)));
            key(&mut a, KeyCode::Left);
            a.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::SHIFT));
            let input = a.input.text.clone();
            let selected = a.input.selected_text().map(str::to_owned);
            let sources = serde_json::to_value(a.conversation.sources()).unwrap();
            let goal = a.conversation.board().unwrap().goal().unwrap().to_owned();
            for (w, h) in [(80, 24), (100, 30), (82, 51), (160, 60), (80, 24)] {
                key(&mut a, KeyCode::PageUp);
                let first = snapshot(w, h, &mut a, mono).unwrap();
                if question == HEALTHCARE_QUESTION && h >= 30 {
                    assert!(
                        right_text(&first, w).contains(HEALTHCARE_QUESTION),
                        "{first}"
                    );
                }
                if question.len() > 1000 && h <= 30 {
                    assert!(first.contains("PgUp/PgDn scroll question"), "{first}");
                }
                let mut seen = String::new();
                for _ in 0..30 {
                    let screen = snapshot(w, h, &mut a, mono).unwrap();
                    seen.push_str(&right_text(&screen, w));
                    seen.push(' ');
                    key(&mut a, KeyCode::PageDown);
                }
                for words in question.split_whitespace().collect::<Vec<_>>().windows(3) {
                    assert!(
                        seen.contains(&words.join(" ")),
                        "{w}x{h}: {words:?}\n{seen}"
                    );
                }
                let screen = snapshot(w, h, &mut a, mono).unwrap();
                assert!(screen.contains("F2 send"));
                assert_eq!(a.input.text, input);
                assert_eq!(a.input.selected_text(), selected.as_deref());
                assert_eq!(
                    serde_json::to_value(a.conversation.sources()).unwrap(),
                    sources
                );
                assert_eq!(a.conversation.board().unwrap().goal(), Some(goal.as_str()));
                assert!(a.handoff.review().is_none() && !a.handoff.saving());
                assert!(!a.running() && a.saved().is_none());
                for _ in 0..30 {
                    key(&mut a, KeyCode::PageUp);
                }
            }
        }
    }
}

#[test]
fn wheel_over_question_uses_the_same_overflow_route() {
    let mut a = board();
    super::tests::amend_board(&mut a, |g| {
        g.questions = vec![Question {
            suggestion: None,
            id: "long-question".into(),
            target: None,
            text: format!(
                "{} Last question words?",
                "Which population needs care? ".repeat(40)
            ),
        }];
    });
    let first = snapshot(80, 24, &mut a, false).unwrap();
    assert!(!first.contains("Last question words?"));
    let row = a.screen.input_area.y - 2;
    for _ in 0..40 {
        a.handle_mouse(
            MouseEvent {
                kind: MouseEventKind::ScrollDown,
                column: a.screen.agent_area.x,
                row,
                modifiers: KeyModifiers::NONE,
            },
            Rect::new(0, 0, 80, 24),
        );
        snapshot(80, 24, &mut a, false).unwrap();
    }
    let last = snapshot(80, 24, &mut a, false).unwrap();
    assert!(
        right_text(&last, 80).contains("Last question words?"),
        "{last}"
    );
    assert!(a.handoff.review().is_none() && !a.running());
}

#[test]
fn how_it_works_names_the_existing_reading_scroll_route() {
    let mut a = board();
    key(&mut a, KeyCode::F(1));
    key(&mut a, KeyCode::Char('h'));
    assert!(layout::how_text(&a).contains("PgUp/PgDn scroll"));
}

#[test]
fn one_help_control_and_stable_original_across_review_help_and_receipt() {
    for (w, h) in [(80, 24), (100, 30), (160, 40), (320, 40)] {
        let mut empty = BrainDump::default();
        let screen = snapshot(w, h, &mut empty, false).unwrap();
        assert_eq!(screen.lines().nth(1).unwrap().matches('?').count(), 1);
        assert!(
            screen
                .lines()
                .rev()
                .skip(1)
                .all(|line| !line.contains("F2"))
        );
        assert!(screen.lines().last().unwrap().contains("F2 send"));
        let mut a = board();
        let before = snapshot(w, h, &mut a, false).unwrap();
        let source_area = a.screen.source_area;
        a.screen.details = true;
        a.review_goal();
        assert!(!a.screen.details);
        let review = snapshot(w, h, &mut a, false).unwrap();
        assert_eq!(a.screen.source_area, source_area);
        // The key bar (last row) follows the state; everything above it stays put.
        let (mut l1, mut l2) = (left(&before, w), left(&review, w));
        l1.pop();
        l2.pop();
        assert_eq!(l1, l2);
        let review_text = right_text(&review, w);
        assert!(!review_text.contains("Running out of questions"));
        assert!(review_text.contains("type confirm to save. You can't edit it after."));
        assert!(review_text.contains("type confirm"));
        key(&mut a, KeyCode::F(1));
        let help = snapshot(w, h, &mut a, false).unwrap();
        assert!(help.contains("Right now"));
        let (mut l1, mut l2) = (left(&before, w), left(&help, w));
        l1.pop();
        l2.pop();
        assert_eq!(l1, l2);
        key(&mut a, KeyCode::Esc);
        a.set_saved(seed_me::Receipt::test(
            "/tmp/Tinkery-TEST/session-id".into(),
        ));
        let receipt = snapshot(w, h, &mut a, false).unwrap();
        assert_eq!(a.screen.source_area, source_area);
        let (mut l1, mut l2) = (left(&before, w), left(&receipt, w));
        l1.pop();
        l2.pop();
        assert_eq!(l1, l2);
        assert_eq!(receipt.matches("Saved.").count(), 1);
        assert!(right_text(&receipt, w).contains("Next: shape it into a seed in Shape."));
        assert!(!receipt.contains("Tinkery-TEST/session-id"));
        assert!(!receipt.contains("seed not written yet"));
        for s in [&before, &review, &receipt] {
            for clutter in [
                "Ctrl-",
                "Tab",
                "real Pi",
                "/ provisional",
                "PROVISIONAL",
                "q exits",
            ] {
                assert!(!s.contains(clutter), "{clutter}");
            }
            assert!(s.lines().rev().skip(1).all(|line| !line.contains("F2")));
            assert_eq!(s.lines().nth(1).unwrap().matches('?').count(), 1);
        }
        key(&mut a, KeyCode::F(1));
        snapshot(w, h, &mut a, false).unwrap();
        assert!(a.screen.help);
        key(&mut a, KeyCode::Esc);
        assert!(!a.quitting() && !a.screen.help);
        assert!(a.handoff.receipt().is_some());
    }
}
#[test]
fn literal_question_mark_and_corner_help_preserve_local_typing_and_review() {
    let mut a = board();
    a.paste("A literal");
    key(&mut a, KeyCode::Char('?'));
    assert_eq!(a.input.text, "A literal?");
    assert!(!a.screen.help);
    let saved = (a.input.text.clone(), a.input.cursor);
    snapshot(100, 30, &mut a, false).unwrap();
    a.handle_mouse(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 97,
            row: 1,
            modifiers: KeyModifiers::NONE,
        },
        a.screen.area,
    );
    assert!(a.screen.help);
    snapshot(100, 30, &mut a, false).unwrap();
    key(&mut a, KeyCode::Esc);
    assert_eq!(a.input.text, saved.0);
    assert_eq!(a.input.cursor, saved.1);
    assert!(!a.running());
}
#[test]
fn long_receipt_goal_is_scrollable_and_path_is_only_in_how_it_works() {
    let mut a = board();
    let path = format!(
        "/tmp/Tinkery-TEST/{}/END-SESSION-ID",
        "long-path-component/".repeat(35)
    );
    let mut receipt = seed_me::Receipt::test(path.clone().into());
    receipt.goal = format!("{}\nEND-GOAL", "Long confirmed goal line.\n".repeat(80));
    a.set_saved(receipt);
    for (w, h) in [(80, 24), (100, 30), (160, 40), (320, 40)] {
        a.screen.paper_scroll = 0;
        let first = snapshot(w, h, &mut a, false).unwrap();
        assert!(right_text(&first, w).contains("Next: shape it into a seed in Shape."));
        assert!(!first.contains("Tinkery-TEST"));
        for _ in 0..50 {
            key(&mut a, KeyCode::PageDown);
        }
        let last = snapshot(w, h, &mut a, false).unwrap();
        assert!(right_text(&last, w).contains("END-GOAL"));
        assert!(right_text(&last, w).contains("Next: shape it into a seed in Shape."));
        assert!(!last.contains("END-SESSION-ID"));
        assert!(layout::how_text(&a).contains(&path));
        assert_eq!(a.handoff.receipt().unwrap().session.to_string_lossy(), path);
    }
}
#[test]
fn full_failure_reason_is_accessible_and_never_promotes_a_rejected_reading() {
    let mut a = board();
    let old = a.conversation.board().cloned().unwrap();
    a.notice = format!(
        "Reshape failed: {} Or the final concrete referent was erased. Originals retained.",
        "source-grounded rejection explanation ".repeat(50)
    );
    a.conversation.set_applied(0);
    let screen = snapshot(100, 30, &mut a, false).unwrap();
    assert!(screen.contains("Reading couldn't be updated."));
    assert_eq!(
        a.conversation.board().unwrap().goal().unwrap(),
        old.goal().unwrap()
    );
    key(&mut a, KeyCode::F(1));
    key(&mut a, KeyCode::Char('h'));
    for _ in 0..100 {
        key(&mut a, KeyCode::PageDown);
    }
    let last = snapshot(100, 30, &mut a, false).unwrap();
    // Ignore the overlay frame between wrapped rows, not any words of the reason.
    let visible = right_text(&last, 100)
        .replace('│', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    assert!(visible.contains("Or the final concrete referent was erased. Originals retained."));
}
#[cfg(unix)]
#[test]
fn one_shape_then_one_review_never_auto_repairs_or_withholds() {
    use std::os::unix::fs::PermissionsExt;
    for (mode, success) in [
        ("pass", true),
        ("syntax", false),
        ("invalid", false),
        ("transport", false),
        ("review-transport", true),
        ("review-syntax", true),
        ("review-cap", true),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let r = BoardRequest {
            ask_counts: Default::default(),
            sources: vec![Source {
                id: 1,
                text: if mode == "review-cap" {
                    "original words ".repeat((REQUEST_LIMIT - 24 * 1024) / 15)
                } else {
                    "the restaurateur judges the result".into()
                },
                in_reply_to: None,
            }],
            fragments: vec![],
            previous: None,
            skipped: vec![],
            answered: vec![],
            settled: vec![],
            layout: vec![],
        };
        let program = dir.path().join("pi");
        std::fs::write(&program,format!(r#"#!/usr/bin/env python3
import json,sys,pathlib
root=pathlib.Path(__file__).parent
r=json.load(sys.stdin)
mode={mode:?}
kind=r.get('kind','shape')
assert '--thinking' not in sys.argv
log=root/'calls.jsonl'
previous=[json.loads(s) for s in log.read_text().splitlines()] if log.exists() else []
with log.open('a') as f:f.write(json.dumps(dict(kind=kind,request=r))+'\n')
if kind=='decision-review':
 if mode=='review-transport':print('provider context rejected',file=sys.stderr);sys.exit(2)
 if mode=='review-syntax':print('unreadable review');sys.exit(0)
 assert sorted(r)==['answers','decisions','draft','kind','words'] and r['words']==['the restaurateur judges the result']
 assert r['decisions']==[]
 p=sys.argv[sys.argv.index('--system-prompt')+1]
 assert 'how an action is carried out' in p and 'edge cases and overlaps' in p
 assert 'return none unless another decision is as costly' in p
 (root/'review-ok').write_text('1')
 print(json.dumps(dict(decisions=[])));sys.exit(0)
assert not previous
if mode=='transport':print('provider context rejected',file=sys.stderr);sys.exit(2)
if mode=='syntax':print('not JSON');sys.exit(0)
print(json.dumps(dict(parts=dict(who='I',outcome='judge the result',why='w'*31500 if mode=='review-cap' else 'A judged result'),uncertain=False,framings=[dict(text='The restaurateur judges the result.',supports=[dict(source=1,quote='invented' if mode=='invalid' else r['sources'][0]['text'][:10] if mode=='review-cap' else r['sources'][0]['text'],occurrence=0)])],outcome='A judged result.',misfits=[],questions=[],alternatives=[])))
"#)).unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        let pi = ModelHost::new(crate::pi::Harness::Pi, program, "test/model".into())
            .unwrap()
            .with_default_thinking();
        let result = pi.reshape(r.clone(), &AtomicBool::new(false));
        assert_eq!(result.is_ok(), success, "{mode}: {result:?}");
        let calls = std::fs::read_to_string(dir.path().join("calls.jsonl")).unwrap();
        let reviews = calls
            .lines()
            .filter(|l| l.contains("\"decision-review\""))
            .count();
        assert_eq!(
            calls.lines().count() - reviews,
            1,
            "{mode}: one shaping call"
        );
        assert!(
            calls
                .lines()
                .all(|l| l.contains("\"shape\"") || l.contains("\"decision-review\"")),
            "{mode}: only shaping and review calls: {calls}"
        );
        // The decision review runs only on a readable board, once.
        let reviewed = !matches!(mode, "syntax" | "transport" | "review-cap");
        assert_eq!(reviews, usize::from(reviewed), "{mode}: {calls}");
        let review_ok = reviewed && !matches!(mode, "review-transport" | "review-syntax");
        assert_eq!(dir.path().join("review-ok").exists(), review_ok, "{mode}");
        if mode.starts_with("review-") {
            let logs = pi.diagnostics().join("\n");
            assert!(
                logs.contains(if mode == "review-syntax" {
                    "Ignored decision review"
                } else {
                    "Decision review skipped"
                }),
                "{logs}"
            );
            assert_eq!(
                result.as_ref().unwrap().parts.outcome.as_deref(),
                Some("judge the result")
            );
            assert!(
                !logs.contains("previous reading retained")
                    && !logs.contains("Previous paper retained")
            );
            if mode == "review-transport" {
                assert!(logs.contains("provider rejected the input/context size"));
            }
            if mode == "review-cap" {
                assert!(logs.contains("Start a smaller goal, keeping this one as a reference."));
                let decision: checks::Decision =
                    serde_json::from_str(&pi.diagnostics()[0]).unwrap();
                let raw = decision.raw.as_ref().unwrap();
                let shape_bytes = serde_json::to_vec(&r).unwrap().len();
                let review_bytes = review::input(&r, raw).unwrap().len();
                assert!(
                    shape_bytes <= REQUEST_LIMIT
                        && raw.len() <= crate::pi::LIMIT
                        && review_bytes > REQUEST_LIMIT
                );
                eprintln!(
                    "review-only cap: shaping input={shape_bytes}, shaping output={}, review input={review_bytes}, fake starts=1",
                    raw.len()
                );
            }
            let mut a = BrainDump::default();
            a.conversation.sources_mut().push(r.sources[0].clone());
            let mut previous = result.clone().unwrap();
            previous.parts.outcome = Some("retain the previous reading".into());
            a.apply_result(r.clone(), Ok(previous));
            assert_eq!(
                a.conversation.board().unwrap().parts.outcome.as_deref(),
                Some("retain the previous reading")
            );
            a.apply_result(r.clone(), result.clone());
            assert_eq!(
                a.conversation.board().unwrap().parts.outcome.as_deref(),
                Some("judge the result")
            );
            assert_eq!(a.conversation.sources()[0].text, r.sources[0].text);
            assert!(a.conversation.last_failure().is_none());
        }
        eprintln!(
            "mode={mode}, fake process starts={}, reviews={reviews}",
            calls.lines().count()
        );
        for log in pi.diagnostics() {
            let d: checks::Decision = serde_json::from_str(&log).unwrap();
            assert!(!d.request_key.is_empty());
        }
        if mode == "invalid" {
            assert!(checks::why(&pi.diagnostics()).contains("Rejected attempt — not confirmable"));
        }
    }
}
#[cfg(unix)]
#[test]
fn a_send_costs_a_shaping_call_and_a_review_and_nothing_runs_after_display() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let program = dir.path().join("pi");
    std::fs::write(&program,r#"#!/usr/bin/env python3
import json,sys,pathlib
root=pathlib.Path(__file__).parent
r=json.load(sys.stdin)
kind=r.get('kind','shape')
with (root/'calls').open('a') as f:f.write(kind+'\n')
if kind=='decision-review':print(json.dumps(dict(decisions=[])));sys.exit(0)
if (root/'reject').exists():print('provider input/context rejected',file=sys.stderr);sys.exit(2)
print(json.dumps(dict(uncertain=False,framings=[dict(text='The restaurateur judges the result.',supports=[dict(source=1,quote=r['sources'][0]['text'],occurrence=0)])],outcome='A judged result.',misfits=[],questions=[],alternatives=[])))
"#).unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let host =
        Arc::new(ModelHost::new(crate::pi::Harness::Pi, program, "test/model".into()).unwrap());
    let mut a = BrainDump::with_host(host);
    a.paste("the restaurateur judges the result");
    a.submit();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while a.running() && std::time::Instant::now() < deadline {
        a.tick();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(a.conversation.board().is_some() && !a.running());
    // Nothing starts after the reading is shown: keep ticking, and no further call arrives.
    let settle = std::time::Instant::now() + std::time::Duration::from_millis(800);
    while std::time::Instant::now() < settle {
        a.tick();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(
        std::fs::read_to_string(dir.path().join("calls")).unwrap(),
        "shape\ndecision-review\n"
    );
    let old = a.conversation.board().unwrap().reading.clone();
    let original = a.conversation.sources()[0].text.clone();
    std::fs::write(dir.path().join("reject"), "1").unwrap();
    let answer = "exact\r\n\t\0".repeat(600);
    a.paste(&answer);
    // This legacy reading has no goal parts/question, so treat the new words as a
    // settled answer for this failure-path fixture rather than an unscoped addition.
    super::tests::amend_board(&mut a, |g| {
        g.questions.push(Question {
            suggestion: None,
            target: None,
            id: "failure-answer".into(),
            text: "What matters?".into(),
        })
    });
    a.submit();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while a.running() && std::time::Instant::now() < deadline {
        a.tick();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(!a.running());
    assert_eq!(a.conversation.board().unwrap().reading, old);
    assert_eq!(a.conversation.sources()[0].text, original);
    assert_eq!(a.conversation.sources()[1].text, answer);
    assert!(
        a.input.text.is_empty(),
        "failed shaping words are committed originals, not local draft"
    );
    assert!(a.notice.contains("Reshape failed"), "{}", a.notice);
    assert!(a.conversation.last_failure().is_some());
    assert!(
        a.notice
            .contains("provider rejected the input/context size")
    );
    for _ in 0..10 {
        a.tick();
    }
    assert_eq!(
        std::fs::read_to_string(dir.path().join("calls")).unwrap(),
        "shape\ndecision-review\nshape\n"
    );
}

/// A gated fake Pi proves both call phases reach both initial and refining screens.
#[cfg(unix)]
#[test]
fn each_send_shows_the_current_call_without_claiming_practice_calls() {
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        time::{Duration, Instant},
    };
    for refining in [false, true] {
        let dir = tempfile::Builder::new()
            .prefix("tinkery-TEST-call-status-")
            .tempdir()
            .unwrap();
        let program = dir.path().join("TEST-pi");
        fs::write(
            &program,
            r#"#!/usr/bin/env python3
import json, pathlib, sys, time
root = pathlib.Path(__file__).parent
request = json.load(sys.stdin)
kind = request.get('kind', 'shape')
with (root / 'calls').open('a') as log:
    log.write(kind + '\n')
(root / ('started-' + kind)).touch()
while not (root / ('release-' + kind)).exists():
    time.sleep(.01)
if kind == 'decision-review':
    print('{"decisions":[]}')
else:
    print(json.dumps({'parts': {'who': 'I', 'outcome': 'read the result'}}))
"#,
        )
        .unwrap();
        fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
        let host =
            Arc::new(ModelHost::new(crate::pi::Harness::Pi, program, "test/stub".into()).unwrap());
        let mut a = if refining {
            board()
        } else {
            BrainDump::default()
        };
        a.host = host.clone();
        a.real = true;
        a.paste("TEST words");
        a.submit();
        for (kind, phase, expected) in [
            ("shape", Request::Shaping, "Thinking… (run 1 of up to 2)"),
            (
                "decision-review",
                Request::DecisionReview,
                "Reviewing decisions… (run 2 of up to 2)",
            ),
        ] {
            let until = Instant::now() + Duration::from_secs(10);
            while !dir.path().join(format!("started-{kind}")).exists() {
                assert!(Instant::now() < until, "fake call did not start");
                std::thread::sleep(Duration::from_millis(5));
            }
            assert_eq!(host.call_in_flight(), Some(phase));
            assert_eq!(host.new_session().call_in_flight(), None);
            for (w, h) in [(80, 24), (100, 30)] {
                for mono in [false, true] {
                    let screen = snapshot(w, h, &mut a, mono).unwrap();
                    let right = right_text(&screen, w)
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ");
                    assert!(right.contains(expected), "{screen}");
                    assert_eq!(right.matches("of up to 2").count(), 1);
                }
            }
            if refining {
                a.refuse("TEST refusal wins.");
                let screen = snapshot(100, 30, &mut a, false).unwrap();
                assert!(screen.contains("TEST refusal wins."));
                assert!(!screen.contains("of up to 2"));
                a.screen.refused = None;
            }
            fs::write(dir.path().join(format!("release-{kind}")), "TEST").unwrap();
        }
        super::tests::settle(&mut a);
        assert_eq!(host.call_in_flight(), None);
        let screen = snapshot(100, 30, &mut a, false).unwrap();
        assert!(!screen.contains("of up to 2"));
        assert_eq!(
            fs::read_to_string(dir.path().join("calls")).unwrap(),
            "shape\ndecision-review\n"
        );
        assert_eq!(Simulated.call_in_flight(), None);
    }
}

#[test]
fn a_practice_send_says_it_simulates_never_thinking() {
    use std::sync::atomic::Ordering;
    static RELEASE: AtomicBool = AtomicBool::new(false);
    struct Held;
    impl BoardHost for Held {
        fn reshape(&self, request: BoardRequest, cancel: &AtomicBool) -> Result<Guess, String> {
            while !RELEASE.load(Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            Simulated.reshape(request, cancel)
        }
    }
    // Release the held send even when an assertion fails, so a failure never hangs.
    struct Release;
    impl Drop for Release {
        fn drop(&mut self) {
            RELEASE.store(true, Ordering::Relaxed);
        }
    }
    let mut a = BrainDump::with_mode(Arc::new(Held), false);
    // Declared after the dump, so it drops first: the dump's held send can finish.
    let release = Release;
    a.paste("TEST words");
    a.submit();
    for (w, h) in [(80, 24), (100, 30)] {
        let screen = snapshot(w, h, &mut a, true).unwrap();
        assert!(screen.contains(super::layout::PRACTICE_STATUS), "{screen}");
        assert!(!screen.contains("Thinking"), "{screen}");
    }
    drop(release);
    super::tests::settle(&mut a);
}

#[test]
fn empty_send_is_visible_without_a_request_and_typing_clears_it() {
    const NOTICE: &str = "Write something, then F2 sends it.";
    struct NoCalls;
    impl BoardHost for NoCalls {
        fn reshape(&self, _: BoardRequest, _: &AtomicBool) -> Result<Guess, String> {
            panic!("empty send must not request a reading");
        }
    }
    for (w, h) in [(80, 24), (100, 30), (160, 40), (320, 40)] {
        for no_color in [false, true] {
            let host = Arc::new(NoCalls);
            let mut a = BrainDump::with_host(host.clone());
            assert!(!snapshot(w, h, &mut a, no_color).unwrap().contains(NOTICE));
            for whitespace in ["", " \n\t"] {
                a.input = Note::new(whitespace);
                key(&mut a, KeyCode::F(2));
                let screen = snapshot(w, h, &mut a, no_color).unwrap();
                assert!(screen.contains(NOTICE), "{w}x{h}: {screen}");
                assert_eq!(a.input.text, whitespace);
                assert!(a.conversation.sources().is_empty());
                assert!(!a.running() && a.handoff.receipt().is_none());
            }
            a.paste("Café 👩‍💻");
            assert!(!snapshot(w, h, &mut a, no_color).unwrap().contains(NOTICE));
            assert_eq!(a.input.text, " \n\tCafé 👩‍💻");

            let mut a = board();
            let original = serde_json::to_value(a.conversation.sources()).unwrap();
            let goal = a.conversation.board().unwrap().parts.compose();
            snapshot(w, h, &mut a, no_color).unwrap();
            key(&mut a, KeyCode::F(2));
            assert!(right_text(&snapshot(w, h, &mut a, no_color).unwrap(), w).contains(NOTICE));
            assert_eq!(
                serde_json::to_value(a.conversation.sources()).unwrap(),
                original
            );
            assert_eq!(a.conversation.board().unwrap().parts.compose(), goal);
            assert!(!a.running() && a.handoff.receipt().is_none());
            key(&mut a, KeyCode::Char('x'));
            assert!(!snapshot(w, h, &mut a, no_color).unwrap().contains(NOTICE));
        }
    }
}
