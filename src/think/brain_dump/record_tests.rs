//! The think record as a session leaves it: written once by a confirmed save, never before.

use super::tests::amend_board;
use super::*;
use std::path::Path;

const DUMP: &str = "  TEST: calmer mornings. Café 👩‍💻\n\"no phone\" before eight.  ";
const ANSWER: &str = "TEST answer: the calm start matters most ";

fn key(a: &mut BrainDump, code: KeyCode, modifiers: KeyModifiers) {
    a.handle_key(KeyEvent::new(code, modifiers));
}
fn settle(a: &mut BrainDump) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while a.running() && std::time::Instant::now() < deadline {
        a.tick();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert!(!a.running());
}
fn records(root: &Path) -> Vec<std::path::PathBuf> {
    std::fs::read_dir(root)
        .map(|d| d.map(|e| e.unwrap().path()).collect())
        .unwrap_or_default()
}
/// A practice-host session with a dump, a skip, its undo and an answer, writing records to `root`.
fn session(root: &Path) -> BrainDump {
    session_with(Arc::new(Simulated), root, None)
}
fn session_with(
    host: Arc<dyn BoardHost>,
    root: &Path,
    repo: Option<crate::repo::Repo>,
) -> BrainDump {
    let mut a = BrainDump::with_host(host)
        .with_sessions(None)
        .with_record_root(root.to_path_buf())
        .with_repo(repo);
    a.paste(DUMP);
    a.submit();
    settle(&mut a);
    super::tests::choose(&mut a, "Skip this question");
    super::tests::choose(&mut a, "Bring back");
    a.paste(ANSWER);
    a.submit();
    settle(&mut a);
    a
}
/// Hands the review's save a Seed Me result, as the save thread would.
fn save(a: &mut BrainDump, result: Result<seed_me::Receipt, String>) {
    let (tx, rx) = std::sync::mpsc::channel();
    a.handoff.begin(rx);
    tx.send(result).unwrap();
    a.goal_tick();
}
fn receipt(root: &Path, goal: &str) -> seed_me::Receipt {
    let path = root.join("TEST-sessions/20261012-TEST");
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(
        path.join("ledger.json"),
        serde_json::json!({
            "schema_version": 1, "session_id": "TEST-existing-session-id", "goal": goal,
        })
        .to_string(),
    )
    .unwrap();
    let mut receipt = seed_me::Receipt::test(path);
    receipt.goal = goal.into();
    receipt
}

/// Replays decoded recorded candidates, not invented raw provider replies.
fn stacker_226_replay(sends: usize) -> BrainDump {
    struct Frozen {
        candidates: Vec<serde_json::Value>,
    }
    impl BoardHost for Frozen {
        fn reshape(&self, request: BoardRequest, _: &AtomicBool) -> Result<Guess, String> {
            Guess::decode(
                &self.candidates[request.sources.len() - 1].to_string(),
                &request,
                &mut vec![],
            )
        }
    }
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/fixtures/stacker-226.json")).unwrap();
    let candidates = fixture["checks"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|c| c["candidate"].as_object().map(|_| c["candidate"].clone()))
        .collect();
    let mut a = BrainDump::with_host(Arc::new(Frozen { candidates }));
    let originals: Vec<_> = fixture["turns"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|t| t["text"].as_str().or(t["answer"].as_str()))
        .take(sends)
        .collect();
    for original in &originals {
        a.paste(original);
        a.submit();
        settle(&mut a);
    }
    assert_eq!(a.conversation.sources().len(), sends);
    for (source, original) in a.conversation.sources().iter().zip(originals) {
        assert_eq!(source.text, original);
    }
    a
}

#[test]
fn frozen_stacker_226_done_question_is_open_not_either_or() {
    let a = stacker_226_replay(2);
    let q = a.conversation.focused_question().unwrap();
    assert_eq!(q.target, Some(Part::DoneWhen));
    assert!(!q.text.contains(" or "), "binary Done when: {}", q.text);
}

#[test]
fn frozen_stacker_226_short_answer_does_not_invent_criteria_or_a_followup() {
    let mut a = stacker_226_replay(3);
    assert!(a.conversation.focused_question().is_none());
    assert!(
        a.goal_ready(),
        "readiness is not a judgment that more criteria are unnecessary"
    );
    assert_eq!(
        a.conversation.board().unwrap().parts.done_when.as_deref(),
        Some("stacker available to tink-arcade")
    );
    for (w, h) in [(80, 24), (100, 30), (160, 40), (320, 40)] {
        let view = snapshot(w, h, &mut a, true).unwrap();
        assert!(!view.contains("What else matters"));
        let goal = a.conversation.board().unwrap().goal().unwrap();
        for later_criterion in [
            "matches the tink-arcade style",
            "working sounds",
            "plays like the real game",
        ] {
            assert!(!goal.contains(later_criterion));
        }
    }
}

#[test]
fn frozen_stacker_226_explicit_criteria_can_settle() {
    let a = stacker_226_replay(4);
    assert!(a.goal_ready());
    assert_eq!(
        a.conversation.board().unwrap().parts.done_when.as_deref(),
        Some(
            "stacker available to tink-arcade\nmatches the tink-arcade style\nhas working sounds\nplays like the real game"
        )
    );
}

#[test]
fn recorded_stacker_done_when_items_survive_reading_review_and_save() {
    fn visible_items(a: &mut BrainDump, width: u16, height: u16, mono: bool) {
        let mut seen = String::new();
        for _ in 0..12 {
            let screen = snapshot(width, height, a, mono).unwrap();
            seen.push_str(&super::goal_tests::right_text(&screen, width));
            seen.push(' ');
            key(a, KeyCode::PageDown, KeyModifiers::NONE);
        }
        for boundary in ["Done when:", "theme;", "sounds;", "game has."] {
            assert!(
                seen.contains(boundary),
                "{width}x{height}: {boundary}\n{seen}"
            );
        }
    }
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/fixtures/stacker-218.json")).unwrap();
    let parts = fixture["checks"]
        .as_array()
        .unwrap()
        .iter()
        .rev()
        .find_map(|check| check["candidate"].get("parts"))
        .unwrap();
    let original = fixture["turns"][4]["answer"].as_str().unwrap();
    let done = "Done when: matches the tink-arcade style and theme; has working sounds; just works with the right mechanics as the game has.";
    let expected = format!(
        "{}\n{done}",
        fixture["goal"]
            .as_str()
            .unwrap()
            .split("\nDone when:")
            .next()
            .unwrap()
    );
    for (width, height, mono) in [(80, 24), (100, 30), (160, 40), (320, 40)]
        .into_iter()
        .flat_map(|(w, h)| [false, true].map(|mono| (w, h, mono)))
    {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("TEST-records");
        let mut a =
            super::readiness_tests::app_with_source(serde_json::json!({"parts": parts}), original)
                .with_record_root(root.clone());
        assert_eq!(
            a.conversation.board().unwrap().goal(),
            Some(expected.as_str())
        );
        assert_eq!(
            serde_json::to_value(&a.conversation.board().unwrap().parts).unwrap(),
            *parts
        );
        visible_items(&mut a, width, height, mono);
        assert!(a.copy_text().contains(&expected));
        a.review_goal();
        let frozen = &a.handoff.review().unwrap().affirmation;
        assert_eq!(frozen.goal, expected);
        assert_eq!(serde_json::to_value(&frozen.parts).unwrap(), *parts);
        assert_eq!(frozen.sources[0].text, original);
        visible_items(&mut a, width, height, mono);
        save(&mut a, Ok(receipt(dir.path(), &expected)));
        assert_eq!(a.saved().unwrap().goal, expected);
        visible_items(&mut a, width, height, mono);
        let files = records(&root);
        assert_eq!(files.len(), 1);
        let record: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&files[0]).unwrap()).unwrap();
        assert_eq!(record["goal"], expected);
        assert_eq!(a.conversation.sources()[0].text, original);
    }
}

#[test]
fn newline_prose_items_use_clause_separators_without_splitting_quotes() {
    let parts = GoalParts {
        who: Some("I".into()),
        outcome: Some("check sound\ncheck mechanics".into()),
        when: Some("at home\nat the arcade".into()),
        why: Some("The game matters\r\n\r\nIT stays fun".into()),
        done_when: Some("matches theme.\nhas sound;\nshows \"one\ntwo\"".into()),
        must: vec!["keep one\nconstraint".into()],
        ..Default::default()
    };
    assert_eq!(
        parts.compose().unwrap(),
        "I check sound; check mechanics, at home; at the arcade.\nWhy: The game matters; IT stays fun.\nDone when: matches theme; has sound; shows \"one two\".\nMust: keep one constraint."
    );
}

#[test]
fn a_confirmed_session_leaves_exactly_one_record_with_its_words_turns_and_goal() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("TEST-records");
    let mut a = session(&root);
    a.review_goal();
    assert!(records(&root).is_empty(), "opening review wrote a record");
    key(&mut a, KeyCode::Esc, KeyModifiers::NONE);
    assert!(a.handoff.review().is_none());
    assert!(records(&root).is_empty(), "Esc wrote a record");
    a.review_goal();
    let shown = a.handoff.review().unwrap().affirmation.goal.clone();
    // A later reading must not leak into the record: only the confirmed block counts.
    amend_board(&mut a, |g| {
        g.parts.outcome = Some("TEST later reading".into())
    });
    save(&mut a, Ok(receipt(dir.path(), &shown)));
    assert_eq!(a.notice, "Saved.");
    let files = records(&root);
    assert_eq!(files.len(), 1, "{files:?}");
    let name = files[0].file_name().unwrap().to_string_lossy().into_owned();
    assert!(name.ends_with("Z_20261012-TEST.json"), "{name}");
    let record: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&files[0]).unwrap()).unwrap();
    assert_eq!(record["schema"], 3);
    assert!(record["repo"].is_null(), "no repository was given");
    assert_eq!(record["origin"], "simulated");
    assert_eq!(record["seed_me_session"], "20261012-TEST");
    assert_eq!(
        record["goal"].as_str().unwrap(),
        shown,
        "record goal differs from the confirmed block"
    );
    let turns = record["turns"].as_array().unwrap();
    let kinds: Vec<_> = turns.iter().map(|t| t["kind"].as_str().unwrap()).collect();
    assert_eq!(
        kinds,
        ["words", "asked", "skipped", "restored", "asked", "answered"]
    );
    let sources = a.conversation.sources();
    assert_eq!(turns[0]["text"].as_str().unwrap(), DUMP);
    assert_eq!(turns[0]["text"].as_str().unwrap(), sources[0].text);
    assert_eq!(turns[5]["answer"].as_str().unwrap(), ANSWER);
    assert_eq!(turns[5]["answer"].as_str().unwrap(), sources[1].text);
    assert_eq!(turns[5]["id"], "priority");
    assert_eq!(turns[1]["question"], turns[5]["question"]);
    let times: Vec<_> = turns.iter().map(|t| t["at"].as_str().unwrap()).collect();
    assert!(times.windows(2).all(|w| w[0] <= w[1]), "{times:?}");
    assert_eq!(record["started"], turns[0]["at"]);
    assert!(!record["checks"].as_array().unwrap().is_empty());
    let line = a.record_line().unwrap();
    assert!(
        line.starts_with("Think record: ") && line.contains(&name),
        "{line}"
    );
    assert!(a.finish_handoff().unwrap().text().contains(&line));
}

#[test]
fn a_session_that_never_confirms_or_fails_to_save_leaves_no_record() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("TEST-records");
    let mut a = session(&root);
    a.review_goal();
    save(&mut a, Err("TEST helper refused".into()));
    assert!(a.handoff.receipt().is_none());
    assert!(
        records(&root).is_empty(),
        "a failed Seed Me save wrote a record"
    );
    let mut quit = session(&root);
    key(&mut quit, KeyCode::Esc, KeyModifiers::NONE);
    key(&mut quit, KeyCode::Char('y'), KeyModifiers::NONE);
    assert_eq!(*quit.exit(), Exit::Menu);
    assert!(quit.finish_handoff().is_none());
    assert!(!root.exists(), "leaving without confirming wrote a record");
}

#[test]
fn a_record_that_cannot_be_written_leaves_the_saved_goal_standing_and_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let blocked = dir.path().join("TEST-not-a-folder");
    std::fs::write(&blocked, "TEST").unwrap();
    let mut a = session(&blocked);
    a.review_goal();
    let shown = a.handoff.review().unwrap().affirmation.goal.clone();
    save(&mut a, Ok(receipt(dir.path(), &shown)));
    assert_eq!(a.saved().unwrap().goal, shown, "the goal save must stand");
    for (w, h) in [(80, 24), (100, 30), (160, 40)] {
        let screen = snapshot(w, h, &mut a, false).unwrap();
        let right = super::goal_tests::right_text(&screen, w);
        assert!(
            right.contains("Saved. ")
                && right.contains("The think record wasn't written; F1 then h shows why."),
            "{w}x{h}: the whole warning, apart from the saved goal\n{screen}"
        );
        assert!(
            !screen.contains("TEST-not-a-folder"),
            "the path stays in help"
        );
    }
    assert!(layout::how_text(&a).contains("Think record not written: "));
    let printed = a.finish_handoff().unwrap().text();
    assert!(printed.contains("Think record not written: "), "{printed}");
}

#[test]
fn an_unresolved_saved_session_leaves_the_goal_standing_without_an_unbound_record() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("TEST-records");
    let mut a = session(&root);
    a.review_goal();
    let shown = a.handoff.review().unwrap().affirmation.goal.clone();
    let mut saved = seed_me::Receipt::test(dir.path().join("TEST-missing-session"));
    saved.goal = shown.clone();
    save(&mut a, Ok(saved));
    assert_eq!(a.saved().unwrap().goal, shown);
    assert!(
        !root.exists(),
        "a record without an exact session was written"
    );
    assert!(
        a.record_line()
            .unwrap()
            .contains("Think record not written:")
    );
    assert!(a.notice.contains("The think record wasn't written"));
}

#[test]
fn review_says_confirm_also_saves_the_record_only_when_one_will_be_written() {
    let dir = tempfile::tempdir().unwrap();
    let mut a = session(&dir.path().join("TEST-records"));
    a.review_goal();
    let screen = snapshot(160, 40, &mut a, false).unwrap();
    assert!(screen.contains("save goal and record"), "{screen}");
    assert!(
        screen.contains("type confirm to save the goal and its record."),
        "{screen}"
    );
    let mut plain = BrainDump::with_host(Arc::new(Simulated)).with_sessions(None);
    plain.paste(DUMP);
    plain.submit();
    settle(&mut plain);
    plain.review_goal();
    let screen = snapshot(160, 40, &mut plain, false).unwrap();
    assert!(!screen.contains("record"), "{screen}");
}

/// How many shaping calls a record's checks describe.
fn shaping_checks(record: &serde_json::Value) -> usize {
    record["checks"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["check"] == "structure-spans-history")
        .count()
}

#[cfg(unix)]
#[test]
fn records_keep_the_model_reply_only_for_rejected_checks() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let program = dir.path().join("pi");
    std::fs::write(&program, r#"#!/usr/bin/env python3
import json,sys
r=json.load(sys.stdin)
if r.get('kind')=='decision-review':print('{"decisions":[]}');sys.exit(0)
marker=r['sources'][-1]['text']
if marker=='TEST-BAD':print('TEST not a reading');sys.exit(0)
print(json.dumps(dict(parts=dict(who='I',outcome='finish TEST-GOOD-REPLY',why='the work is stuck',done_when='tests pass'),questions=[])))
"#).unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let host = Arc::new(
        ModelHost::new(
            crate::pi::Harness::Pi,
            program,
            "test/model".into(),
            crate::pi::CallBudget::new(3),
        )
        .unwrap(),
    );
    let root = dir.path().join("TEST-records");
    let mut a = BrainDump::with_host(host)
        .with_sessions(None)
        .with_record_root(root.clone());
    a.paste("TEST-BAD");
    a.submit();
    settle(&mut a);
    a.paste("TEST-GOOD");
    a.submit();
    settle(&mut a);
    a.review_goal();
    let goal = a.handoff.review().unwrap().affirmation.goal.clone();
    save(&mut a, Ok(receipt(dir.path(), &goal)));
    let file = records(&root).pop().unwrap();
    let record: serde_json::Value = serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap();
    let checks: Vec<_> = record["checks"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["check"] == "structure-spans-history")
        .collect();
    assert_eq!(checks.len(), 2, "{record:#}");
    assert_eq!(checks[0]["decision"], "reject");
    assert_eq!(checks[0]["raw"], "TEST not a reading\n");
    assert_eq!(checks[1]["decision"], "pass");
    assert!(checks[1]["raw"].is_null(), "passing reply kept: {record:#}");
    assert!(
        checks[1]["candidate"]
            .to_string()
            .contains("TEST-GOOD-REPLY"),
        "the decoded reading stays: {record:#}"
    );
    // F5 still has every reply this session.
    assert!(a.diagnostics().join("\n").contains("TEST-GOOD-REPLY"));
}

#[cfg(unix)]
#[test]
fn new_goals_keep_their_own_checks_and_the_standalone_record_setting() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let program = dir.path().join("pi");
    std::fs::write(&program, r#"#!/usr/bin/env python3
import json,sys
r=json.load(sys.stdin)
if r.get('kind')=='decision-review':print('{"decisions":[]}');sys.exit(0)
marker=r['sources'][0]['text']
print(json.dumps(dict(parts=dict(who='I',outcome='finish '+marker,why='the work is stuck',done_when='tests pass for '+marker),questions=[])))
"#).unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let host = Arc::new(
        ModelHost::new(
            crate::pi::Harness::Pi,
            program,
            "test/model".into(),
            crate::pi::CallBudget::new(8),
        )
        .unwrap(),
    );
    let root = dir.path().join("TEST-records");
    let mut first = BrainDump::with_host(host.clone())
        .with_sessions(None)
        .with_record_root(root.clone());
    first.paste("ALPHA");
    first.submit();
    settle(&mut first);
    first.paste("Also keep this goal's second check.");
    first.submit();
    settle(&mut first);
    first.review_goal();
    let goal = first.handoff.review().unwrap().affirmation.goal.clone();
    save(&mut first, Ok(receipt(dir.path(), &goal)));
    let alpha_file = records(&root).pop().unwrap();
    let alpha_bytes = std::fs::read(&alpha_file).unwrap();
    let alpha: serde_json::Value = serde_json::from_slice(&alpha_bytes).unwrap();
    assert_eq!(shaping_checks(&alpha), 2);

    // Home passes its configured prototype again; standalone uses fresh_factory.
    for (name, mut next) in [
        (
            "BRAVO",
            BrainDump::with_host(host)
                .with_sessions(None)
                .with_record_root(root.clone()),
        ),
        ("CHARLIE", first.fresh_factory()()),
    ] {
        assert!(next.records(), "a new visit lost --record-root");
        next.paste(name);
        next.submit();
        settle(&mut next);
        assert!(
            !next.diagnostics().join("\n").contains("ALPHA"),
            "previous goal in F5 history"
        );
        next.review_goal();
        let goal = next.handoff.review().unwrap().affirmation.goal.clone();
        let path = dir.path().join("TEST-sessions").join(name);
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(
            path.join("ledger.json"),
            serde_json::json!({
                "schema_version": 1, "session_id": name, "goal": goal,
            })
            .to_string(),
        )
        .unwrap();
        let mut result = seed_me::Receipt::test(path);
        result.goal = goal;
        save(&mut next, Ok(result));
        let file = records(&root)
            .into_iter()
            .find(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .ends_with(&format!("_{name}.json"))
            })
            .unwrap();
        let text = std::fs::read_to_string(file).unwrap();
        assert!(!text.contains("ALPHA"));
        let record: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(shaping_checks(&record), 1);
        assert_eq!(record["turns"][0]["text"], name);
    }
    assert_eq!(std::fs::read(alpha_file).unwrap(), alpha_bytes);
}

/// The practice reading, keeping every request exactly as it would leave for a model.
#[derive(Default)]
struct Spy(std::sync::Mutex<Vec<String>>);
impl BoardHost for Spy {
    fn reshape(
        &self,
        request: BoardRequest,
        cancelled: &std::sync::atomic::AtomicBool,
    ) -> Result<Guess, String> {
        self.0
            .lock()
            .unwrap()
            .push(serde_json::to_string(&request).unwrap());
        Simulated.reshape(request, cancelled)
    }
}

#[test]
fn the_repository_is_shown_before_confirm_kept_in_the_record_and_never_sent_to_the_model() {
    let dir = tempfile::tempdir().unwrap();
    let repo_root = dir.path().join("TEST-repo-made-here");
    std::fs::create_dir_all(repo_root.join(".git")).unwrap();
    let repo = crate::repo::Repo::enclosing(&repo_root);
    let root = dir.path().join("TEST-records");
    let spy = Arc::new(Spy::default());
    let mut a = session_with(spy.clone(), &root, repo);
    a.review_goal();
    let screen = snapshot(100, 30, &mut a, true).unwrap();
    assert!(screen.contains("made in TEST-repo-made-here"), "{screen}");
    let shown = a.handoff.review().unwrap().affirmation.goal.clone();
    assert!(
        !shown.contains("TEST-repo"),
        "the goal itself never names the repository"
    );
    save(&mut a, Ok(receipt(dir.path(), &shown)));
    let files = records(&root);
    let record: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&files[0]).unwrap()).unwrap();
    assert_eq!(record["repo"], repo_root.to_str().unwrap());
    assert_eq!(
        a.fresh_factory()().repo().map(crate::repo::Repo::root),
        Some(repo_root.as_path()),
        "the next goal from the menu keeps where Tinkery was started"
    );
    let sent = spy.0.lock().unwrap();
    assert!(!sent.is_empty(), "the session made readings");
    assert!(
        sent.iter().all(|r| !r.contains("TEST-repo")),
        "the repository reached a model request"
    );
}

#[test]
fn without_a_repository_review_says_nothing_about_one() {
    let dir = tempfile::tempdir().unwrap();
    let mut a = session(&dir.path().join("TEST-records"));
    a.review_goal();
    let screen = snapshot(100, 30, &mut a, true).unwrap();
    assert!(!screen.contains("made in"), "{screen}");
}

#[cfg(unix)]
#[test]
fn a_claude_code_reading_is_recorded_with_its_harness_and_exact_model() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let program = dir.path().join("claude");
    std::fs::write(&program, r#"#!/usr/bin/env python3
import json,sys
r=json.load(sys.stdin)
assert sys.argv[sys.argv.index('--tools')+1]==''
reply={"decisions":[]} if r.get('kind')=='decision-review' else dict(parts=dict(who='I',outcome='finish TEST claude goal',why='the work is stuck',done_when='tests pass'),questions=[])
print(json.dumps(dict(type='result',is_error=False,result=json.dumps(reply),modelUsage={'claude-opus-5-5':{}})))
"#).unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let host = Arc::new(
        ModelHost::new(
            crate::pi::Harness::ClaudeCode,
            program,
            "claude-opus-5-5".into(),
            crate::pi::CallBudget::new(2),
        )
        .unwrap()
        .with_thinking("xhigh")
        .unwrap(),
    );
    let root = dir.path().join("TEST-records");
    let mut a = BrainDump::with_host(host)
        .with_sessions(None)
        .with_record_root(root.clone());
    a.paste("TEST claude words");
    a.submit();
    settle(&mut a);
    a.review_goal();
    let goal = a.handoff.review().unwrap().affirmation.goal.clone();
    assert!(goal.contains("TEST claude goal"), "{goal}");
    save(&mut a, Ok(receipt(dir.path(), &goal)));
    let file = records(&root).pop().unwrap();
    let record: serde_json::Value = serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap();
    assert_eq!(record["origin"], "live");
    assert_eq!(record["harness"], "claude-code");
    assert_eq!(record["model"], "claude-opus-5-5");
    assert_eq!(record["thinking"], "xhigh");
    assert!(
        ModelHost::new(
            crate::pi::Harness::ClaudeCode,
            "claude".into(),
            "opus".into(),
            crate::pi::CallBudget::new(0),
        )
        .is_err(),
        "an alias that changes meaning is never recorded as the model"
    );
}
