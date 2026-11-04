use super::*;
use std::{fs, path::Path};

fn envelope(runs: &str) -> String {
    format!(r#"{{"protocol":"tink-sdlc","api_version":1,"workspace":"/TEST","runs":[{runs}]}}"#)
}

fn run(slug: &str, state: &str) -> String {
    format!(
        r#"{{"slug":"{slug}","run_id":"x","profile":"light","kind":"feature","verification_status":"{state}","next_action":"TEST next for {slug}","has_lock":false,"checklist_summary":{{"total":3,"passed":1}}}}"#
    )
}

/// A TEST checkout whose fake `sdlc.py` prints `body` (and sleeps `delay` seconds first).
fn checkout(body: &str, delay: f64) -> tempfile::TempDir {
    let dir = tempfile::Builder::new()
        .prefix("tinkery-TEST-watch-")
        .tempdir()
        .unwrap();
    fs::create_dir_all(dir.path().join("_system/scripts")).unwrap();
    fs::write(
        dir.path().join(SCRIPT),
        format!(
            "import sys, time\nfrom pathlib import Path\npid = Path('{pid}')\npid.with_suffix('.tmp').write_text(str(__import__('os').getpid()))\npid.with_suffix('.tmp').replace(pid)\ntime.sleep({delay})\nimport json\nvalue=json.loads({body:?})\nif len(sys.argv)==4 and 'runs' in value:\n    row=next(r for r in value['runs'] if r['slug']==sys.argv[2])\n    value={{'protocol':'tink-sdlc','api_version':1,'run':{{**row,'gates':[],'checklist':[],'errors':[]}}}}\nprint(json.dumps(value))\n",
            pid = dir.path().join("TEST-pid").display()
        ),
    )
    .unwrap();
    dir
}

fn tree(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else if path.file_name().is_some_and(|n| n != "TEST-pid") {
                files.push((path.clone(), fs::read(&path).unwrap()));
            }
        }
    }
    files.sort();
    files
}

fn settle(watch: &mut Watch) {
    let until = Instant::now() + Duration::from_secs(10);
    while matches!(watch.list, Some(Load::Reading(_)))
        || watch.inspection.is_some()
        || matches!(&watch.detail, Some((_, Load::Reading(_))))
    {
        assert!(Instant::now() < until, "read never landed");
        watch.poll();
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn runs_waiting_on_a_person_lead_and_the_rest_keep_tink_sdlcs_order() {
    let runs = parse_list(&envelope(
        &[
            run("quiet-a", "current"),
            run("waits", "blocked"),
            run("quiet-b", "not-run"),
            run("broke", "failed"),
            run("cut", "interrupted"),
            run("busy", "running"),
            run("old", "stale"),
        ]
        .join(","),
    ))
    .unwrap();
    let order: Vec<_> = ordered(&runs).iter().map(|r| r.slug.as_str()).collect();
    assert_eq!(
        order,
        ["waits", "broke", "cut", "quiet-a", "quiet-b", "busy", "old"]
    );
}

#[test]
fn an_invalid_run_needs_you_and_shows_its_reason() {
    let invalid = r#"{"slug":"bad","verification_status":"invalid","error":"TEST reason","checklist_summary":{"total":0,"passed":0}}"#;
    let runs = parse_list(&envelope(&format!("{},{invalid}", run("fine", "current")))).unwrap();
    let first = ordered(&runs)[0];
    assert_eq!(
        (first.slug.as_str(), first.state, first.next.as_str()),
        ("bad", State::Invalid, "TEST reason")
    );
}

#[test]
fn older_runs_without_canonical_ids_are_read_only_not_unreadable() {
    for id in [serde_json::Value::Null, serde_json::json!("old-slug")] {
        let row = serde_json::json!({
            "slug": "TEST-older", "run_id": id, "verification_status": "invalid",
            "error": OLDER_ID_ERROR, "checklist_summary": {"passed": 0, "total": 0}
        });
        let dir = checkout(&envelope(&row.to_string()), 0.0);
        let legacy = dir.path().join("runs/TEST-older");
        fs::create_dir_all(&legacy).unwrap();
        fs::write(legacy.join("meta.json"), "{\"slug\":\"TEST-older\"}\n").unwrap();
        fs::write(legacy.join("approval.json"), "TEST older approval bytes\n").unwrap();
        let before = tree(dir.path());
        let mut watch = Watch::open(Some(dir.path().to_path_buf()));
        settle(&mut watch);
        let run = &watch.runs().unwrap()[0];
        assert_eq!(run.state, State::Older);
        assert_eq!(run.next, OLDER_REASON);
        assert!(
            !run.needs_you(),
            "an older run owes nothing; it lists plainly"
        );
        watch.key(KeyEvent::from(KeyCode::Enter));
        settle(&mut watch);
        let text = detail_text(
            "TEST-older",
            &watch.detail.as_ref().unwrap().1,
            Palette::new(true),
        )
        .to_string();
        assert!(
            text.contains("older run") && text.contains(OLDER_REASON),
            "{text}"
        );
        for absent in [
            "can't be read",
            "UUIDv4",
            "Checklist:",
            "Stage",
            "Goal unknown",
        ] {
            assert!(!text.contains(absent), "{text}");
        }
        watch.key(KeyEvent::from(KeyCode::Char('r')));
        settle(&mut watch);
        assert!(watch.detail.is_none());
        assert_eq!(watch.runs().unwrap()[0].state, State::Older);
        assert_eq!(tree(dir.path()), before);
    }
}

#[test]
fn another_interface_or_an_unknown_state_is_refused_not_shown() {
    let body = run("a", "current");
    for (text, needle) in [
        (
            envelope(&body).replace("tink-sdlc", "other"),
            "isn't supported",
        ),
        (
            envelope(&body).replace(r#""api_version":1"#, r#""api_version":2"#),
            "isn't supported",
        ),
        (
            envelope(&body).replace(r#""api_version":1"#, r#""api_version":"1""#),
            "isn't supported",
        ),
        (
            envelope(&body).replace(r#""api_version":1,"#, ""),
            "isn't supported",
        ),
        (
            envelope(&run("a", "merged")),
            "state Watch doesn't know (merged)",
        ),
        ("not json".to_string(), "Couldn't read the run report"),
    ] {
        let error = parse_list(&text).unwrap_err();
        assert!(error.contains(needle), "{needle}: {error}");
    }
}

#[test]
fn detail_counts_proven_and_attested_items_and_refuses_unknown_gates() {
    let detail = r#"{"protocol":"tink-sdlc","api_version":1,"workspace":"/T","run":{"verification_status":"blocked","next_action":"TEST review","gates":[{"stage":3,"status":"pending","blocked":false}],"checklist":[{"id":"a","verify":"TEST automatic","status":"passed","proof":"automated","automatic":true,"needs_recheck":false,"mark":null},{"id":"b","verify":"TEST observation","status":"pending","proof":"attested","automatic":false,"needs_recheck":false,"mark":null}],"errors":["TEST error"]}}"#;
    let parsed = parse_detail(detail).unwrap();
    assert_eq!((parsed.passed, parsed.proven, parsed.attested), (1, 1, 1));
    assert_eq!(parsed.gates, [(3, "pending".to_string())]);
    assert_eq!(parsed.errors, ["TEST error"]);
    let load = Load::Done(Ok(parsed), Instant::now());
    let text = detail_text("TEST", &load, Palette::new(true)).to_string();
    assert!(text.contains("Checklist: 1/2 passed. How each is checked: 1 automated, 1 by you."));
    assert!(text.contains("pending (attested)"));
    let help = Watch::open(None).help_text();
    assert!(help.contains("how each is checked: automated or by you"));
    assert!(help.contains("A proof kind does not mean the item passed."));
    assert!(
        parse_detail(&detail.replace("pending\",\"blocked", "skipped\",\"blocked"))
            .unwrap_err()
            .contains("skipped")
    );
}

#[test]
fn pending_attestation_kinds_do_not_claim_completed_attestations() {
    let mut report = detail(vec![item("pending", false, false); 4]);
    report["run"]["verification_status"] = "blocked".into();
    let parsed = parse_detail(&report.to_string()).unwrap();
    let load = Load::Done(Ok(parsed), Instant::now());
    let text = detail_text("TEST", &load, Palette::new(true)).to_string();
    assert!(text.contains("Checklist: 0/4 passed. How each is checked: 0 automated, 4 by you."));
    assert_eq!(text.matches("pending (attested)").count(), 4);
    assert!(!text.contains("4 attested"));
}

#[test]
fn opening_and_reading_again_change_nothing_in_the_checkout() {
    let dir = checkout(&envelope(&run("a", "blocked")), 0.0);
    let before = tree(dir.path());
    let mut watch = Watch::open(Some(dir.path().to_path_buf()));
    settle(&mut watch);
    assert_eq!(watch.runs().unwrap().len(), 1);
    watch.key(KeyEvent::from(KeyCode::Char('r')));
    settle(&mut watch);
    watch.key(KeyEvent::from(KeyCode::Enter));
    settle(&mut watch);
    assert_eq!(tree(dir.path()), before);
}

#[test]
fn a_missing_script_is_said_plainly_and_nothing_runs() {
    let dir = tempfile::Builder::new()
        .prefix("tinkery-TEST-watch-")
        .tempdir()
        .unwrap();
    let watch = Watch::open(Some(dir.path().to_path_buf()));
    match &watch.list {
        Some(Load::Done(Err(why), _)) => assert!(why.contains("is missing"), "{why}"),
        _ => panic!("expected a refusal without running anything"),
    }
}

#[test]
fn a_slow_read_is_stopped_at_its_deadline() {
    let dir = checkout(&envelope(""), 30.0);
    let mut watch = Watch::with_deadline(Some(dir.path().to_path_buf()), Duration::from_secs(2));
    settle(&mut watch);
    match &watch.list {
        Some(Load::Done(Err(why), _)) => assert!(why.contains("was stopped"), "{why}"),
        _ => panic!("expected a timeout"),
    }
    assert!(!alive(dir.path()), "the timed-out process still runs");
}

#[test]
fn checkout_publishes_readiness_only_after_the_pid_is_written() {
    let dir = checkout(&envelope(""), 30.0);
    let script = dir.path().join(SCRIPT);
    let body = fs::read_to_string(&script).unwrap();
    // Hold write_text after file creation, before its contents are written: the old readiness race.
    let pause = r#"from pathlib import Path
import time
original_write = Path.write_text
def paused_write(path, text):
    if path.name.startswith('TEST-pid'):
        with path.open('w') as stream:
            original_write(Path('TEST-writing'), 'ready')
            while not Path('TEST-release').exists():
                time.sleep(0.005)
            stream.write(text)
        return len(text)
    return original_write(path, text)
Path.write_text = paused_write
"#;
    fs::write(&script, format!("{pause}{body}")).unwrap();
    let watch = Watch::open(Some(dir.path().to_path_buf()));
    let until = Instant::now() + Duration::from_secs(5);
    while !dir.path().join("TEST-writing").exists() {
        assert!(Instant::now() < until, "PID write never started");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        !dir.path().join("TEST-pid").exists(),
        "readiness was published before the PID was written"
    );
    fs::write(dir.path().join("TEST-release"), "continue").unwrap();
    while !dir.path().join("TEST-pid").exists() {
        assert!(Instant::now() < until, "PID was never published");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(alive(dir.path()));
    drop(watch);
    assert!(!alive(dir.path()), "leaving left the read running");
}

#[test]
fn leaving_mid_read_stops_the_process_and_delivers_nothing() {
    let dir = checkout(&envelope(""), 30.0);
    let mut watch = Watch::open(Some(dir.path().to_path_buf()));
    let until = Instant::now() + Duration::from_secs(5);
    while !dir.path().join("TEST-pid").exists() {
        assert!(Instant::now() < until, "read never started");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(watch.key(KeyEvent::from(KeyCode::Esc)), "Esc leaves Watch");
    drop(watch);
    assert!(!alive(dir.path()), "leaving left the read running");
}

#[test]
fn a_failing_status_shows_its_own_message() {
    let dir = checkout("", 0.0);
    fs::write(
        dir.path().join(SCRIPT),
        "import sys\nprint('{\"protocol\":\"tink-sdlc\",\"api_version\":1,\"error\":{\"code\":\"not-found\",\"message\":\"TEST no such run\"}}')\nsys.exit(1)\n",
    )
    .unwrap();
    let mut watch = Watch::open(Some(dir.path().to_path_buf()));
    settle(&mut watch);
    match &watch.list {
        Some(Load::Done(Err(why), _)) => assert!(why.contains("TEST no such run"), "{why}"),
        _ => panic!("expected the refusal"),
    }
}

fn item(status: &str, automatic: bool, recheck: bool) -> Value {
    serde_json::json!({"id":"TEST-item", "verify":"TEST observation", "status":status,
        "automatic":automatic, "proof":if automatic {"automated"} else {"attested"},
        "needs_recheck":recheck, "mark":null})
}

fn detail(items: Vec<Value>) -> Value {
    serde_json::json!({"protocol":"tink-sdlc", "api_version":1, "run": {
        "verification_status":"current", "next_action":"TEST next",
        "gates":[{"stage":3,"status":"approved"}], "checklist":items, "errors":[]}})
}

#[test]
fn owed_attestations_and_possible_rechecks_lead_even_after_verification() {
    for (status, automatic, recheck, owed, review) in [
        ("pending", false, false, 1, 0),
        ("failed", false, false, 1, 0),
        ("unknown", false, false, 1, 0),
        ("passed", false, true, 0, 1),
        ("passed", false, false, 0, 0),
        ("pending", true, false, 0, 0),
    ] {
        let d = parse_detail(&detail(vec![item(status, automatic, recheck)]).to_string()).unwrap();
        assert_eq!((d.attention.owed, d.attention.recheck), (owed, review));
        let mut runs = parse_list(&envelope(&format!(
            "{},{}",
            run("quiet", "current"),
            run("observe", "current")
        )))
        .unwrap();
        runs[1].attention = Some(d.attention);
        assert_eq!(runs[1].needs_you(), owed + review > 0);
        assert_eq!(
            ordered(&runs)[0].slug,
            if owed + review > 0 {
                "observe"
            } else {
                "quiet"
            }
        );
    }
    let mut provisional = item("pending", false, false);
    provisional["pending"] = true.into();
    assert_eq!(
        parse_detail(&detail(vec![provisional]).to_string())
            .unwrap()
            .attention
            .owed,
        0
    );
    let mut running = detail(vec![]);
    running["run"]["verification_status"] = "running".into();
    running["run"]["gates"][0]["status"] = "stale".into();
    assert!(parse_detail(&running.to_string()).unwrap().attention.gate);
}

#[test]
fn unknown_attestation_fields_refuse_instead_of_claiming_quiet() {
    let valid = detail(vec![item("pending", false, false)]);
    for field in [
        "automatic",
        "proof",
        "status",
        "needs_recheck",
        "mark",
        "id",
        "verify",
    ] {
        let mut value = valid.clone();
        value["run"]["checklist"][0]
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert!(parse_detail(&value.to_string()).is_err(), "{field}");
    }
    for (field, value) in [
        ("status", serde_json::json!("finished")),
        ("proof", serde_json::json!("automated")),
        ("needs_recheck", serde_json::json!("false")),
        ("pending", serde_json::json!("false")),
    ] {
        let mut changed = valid.clone();
        changed["run"]["checklist"][0][field] = value;
        assert!(parse_detail(&changed.to_string()).is_err(), "{field}");
    }
}

#[test]
fn a_run_says_whether_a_worker_is_building_it_and_an_ended_turn_needs_you() {
    // tinkery#614: an imported run read as being worked on while no worker ran.
    let step = |state: &str, turn: Option<&str>| {
        let mut step = serde_json::json!({"name": "build", "state": state});
        if let Some(turn) = turn {
            step["turn"] = turn.into();
            step["observed_turn"] = format!("TEST words for {turn}").into();
        }
        step
    };
    let cases = [
        ("idle", Some(vec![])),
        ("building", Some(vec![step("active", Some("not-ended"))])),
        ("ended", Some(vec![step("active", Some("ended"))])),
        ("older-sdlc", None),
    ];
    let slugs: Vec<_> = cases.iter().map(|(slug, _)| run(slug, "not-run")).collect();
    let list = envelope(&slugs.join(","));
    let mut details = serde_json::Map::new();
    for (slug, steps) in &cases {
        let mut value = detail(vec![]);
        value["run"]["verification_status"] = "not-run".into();
        if let Some(steps) = steps {
            value["run"]["steps"] = steps.clone().into();
        }
        details.insert(slug.to_string(), value);
    }
    let dir = checkout(&list, 0.0);
    let details = Value::Object(details).to_string();
    fs::write(
        dir.path().join(SCRIPT),
        format!(
            "import json,sys\nvalue=json.loads({list:?})\nif len(sys.argv)==4:\n    value=json.loads({details:?})[sys.argv[2]]\n    value['run'].update(slug=sys.argv[2],run_id='x')\nprint(json.dumps(value))\n"
        ),
    )
    .unwrap();
    let mut watch = Watch::open(Some(dir.path().to_path_buf()));
    settle(&mut watch);
    let order: Vec<_> = ordered(watch.runs().unwrap())
        .iter()
        .map(|r| r.slug.clone())
        .collect();
    assert_eq!(order, ["ended", "idle", "building", "older-sdlc"]);
    let text = screen(&mut watch, 100, 30);
    for (slug, words) in [
        ("ended", "not verified yet · worker's turn ended"),
        ("idle", "not verified yet · no worker running ·"),
        ("building", "not verified yet · worker running ·"),
        ("older-sdlc", "not verified yet · 0/0 checks passed"),
    ] {
        assert!(
            text.contains(&format!("{slug}  {words}")),
            "{slug}:\n{text}"
        );
    }
    watch.key(KeyEvent::from(KeyCode::Down));
    watch.key(KeyEvent::from(KeyCode::Enter));
    settle(&mut watch);
    assert!(screen(&mut watch, 100, 30).contains("No worker step has been started for this run."));
    for (by, words) in [
        ("operator", "stopped by you"),
        ("cap", "stopped at its response cap"),
    ] {
        // tink-sdlc#151: a step finished as stopped reads as stopped, never as an ended turn.
        let mut value = detail(vec![]);
        let mut stopped = step("stopped", None);
        stopped["stopped_by"] = by.into();
        value["run"]["steps"] = vec![stopped].into();
        let detail = parse_detail(&value.to_string()).unwrap();
        assert_eq!(
            detail.steps,
            Some(vec![format!("Worker step build: {words}")])
        );
    }
    for bad in [
        step("stopped", None),
        step("running", None),
        step("active", None),
        step("prepared", Some("ended")),
        step("active", Some("TEST finished maybe")),
    ] {
        let mut value = detail(vec![]);
        value["run"]["steps"] = vec![bad.clone()].into();
        assert!(parse_detail(&value.to_string()).is_err(), "{bad}");
    }
}

#[test]
fn opening_reads_attention_before_offering_any_quiet_run() {
    let list = envelope(&format!(
        "{},{}",
        run("quiet", "current"),
        run("observe", "current")
    ));
    let dir = checkout(&list, 0.0);
    let source = format!(
        "import json,sys\nvalue=json.loads({list:?})\nif len(sys.argv)==4:\n    value=json.loads({detail:?} if sys.argv[2]=='observe' else {quiet:?})\n    value['run'].update(slug=sys.argv[2],run_id='x')\nprint(json.dumps(value))\n",
        detail = detail(vec![item("pending", false, false)]).to_string(),
        quiet = detail(vec![]).to_string()
    );
    fs::write(dir.path().join(SCRIPT), source).unwrap();
    let before = tree(dir.path());
    let mut watch = Watch::open(Some(dir.path().to_path_buf()));
    assert!(watch.runs().is_none());
    settle(&mut watch);
    assert_eq!(ordered(watch.runs().unwrap())[0].slug, "observe");
    assert_eq!(tree(dir.path()), before);
    fs::write(
        dir.path().join(SCRIPT),
        format!("import sys\nsys.stdout.write({list:?})\n"),
    )
    .unwrap();
    watch.read_list();
    settle(&mut watch);
    assert!(matches!(watch.list, Some(Load::Done(Err(_), _))));
    assert!(
        watch.runs().is_none(),
        "unreadable detail must not become quiet"
    );
}

#[test]
fn detail_scrolling_reaches_wrapped_evidence_and_last_error_at_small_sizes() {
    for (width, height) in [(80, 24), (100, 30)] {
        let mut value = detail(vec![item("passed", false, false)]);
        value["run"]["checklist"][0]["mark"] = serde_json::json!({"evidence":format!("{} TEST-LAST-EVIDENCE", "TEST evidence ".repeat(500))});
        value["run"]["errors"] = serde_json::json!(["TEST-LAST-ERROR"]);
        let mut watch = Watch::open(None);
        watch.root = Some(PathBuf::from("/TEST"));
        watch.detail = Some((
            "TEST-run".into(),
            Load::Done(parse_detail(&value.to_string()), Instant::now()),
        ));
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
        let paint =
            |watch: &mut Watch, terminal: &mut ratatui::Terminal<ratatui::backend::TestBackend>| {
                terminal
                    .draw(|f| watch.render(f, f.area(), Palette::new(true)))
                    .unwrap();
                terminal
                    .backend()
                    .buffer()
                    .content()
                    .iter()
                    .map(|c| c.symbol())
                    .collect::<String>()
            };
        assert!(!paint(&mut watch, &mut terminal).contains("TEST-LAST-ERROR"));
        watch.key(KeyEvent::from(KeyCode::PageDown));
        paint(&mut watch, &mut terminal);
        assert!(watch.scroll > 0);
        watch.key(KeyEvent::from(KeyCode::End));
        let last = paint(&mut watch, &mut terminal);
        assert!(last.contains("TEST-LAST-EVIDENCE"), "{last}");
        assert!(last.contains("TEST-LAST-ERROR"), "{last}");
        watch.key(KeyEvent::from(KeyCode::Home));
        assert!(paint(&mut watch, &mut terminal).contains("TEST-run"));
        assert_eq!(watch.scroll, 0);
    }
}

#[test]
fn goal_display_keeps_exact_words_and_escapes_unsafe_controls_only() {
    let goal = " TEST café  \nTEST exact goal tail  ";
    let lines = goal_lines(goal, Palette::new(true));
    let shown = lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(shown, goal);
    let lines = goal_lines("TEST\u{1b}[31m\u{202e}", Palette::new(true));
    assert_eq!(lines[0].spans[0].content, "TEST\\u{1b}[31m\\u{202e}");
    assert!(
        goal_lines(goal_text(&Goal::Unknown), Palette::new(true))[0].spans[0]
            .content
            .starts_with("Goal unknown:")
    );
}

fn screen(watch: &mut Watch, width: u16, height: u16) -> String {
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|f| watch.render(f, f.area(), Palette::new(true)))
        .unwrap();
    terminal
        .backend()
        .buffer()
        .content()
        .chunks(width as usize)
        .map(|row| row.iter().map(|c| c.symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

fn listed(runs: Vec<Run>) -> Watch {
    let mut watch = Watch::open(None);
    watch.root = Some(PathBuf::from("/TEST"));
    watch.list = Some(Load::Done(Ok(runs), Instant::now()));
    watch
}

#[test]
fn all_three_goal_states_are_distinct_in_list_and_detail() {
    let root = tempfile::Builder::new()
        .prefix("tinkery-TEST-watch-goals-")
        .tempdir()
        .unwrap();
    let seed = root.path().canonicalize().unwrap().join("seed-contract.md");
    let exact = " TEST café  \nTEST full goal tail  ";
    crate::seed_me::ledger::fixtures::confirmed(root.path(), exact, "r1");
    let record = serde_json::json!({"source":seed, "path":"runs/TEST/seed-contract.md",
        "sha256":format!("{:x}", <sha2::Sha256 as sha2::Digest>::digest(fs::read(&seed).unwrap()))});
    for (record, expected) in [
        (record, Goal::Matched(exact.into())),
        (Value::Null, Goal::NoSeed),
        (serde_json::json!({}), Goal::Unknown),
    ] {
        for status in ["blocked", "current"] {
            let mut row: Value = serde_json::from_str(&run("TEST-run", status)).unwrap();
            row["seed_contract"] = record.clone();
            let runs = parse_list(&envelope(&row.to_string())).unwrap();
            assert_eq!(runs[0].goal, expected);
            let mut value = detail(vec![]);
            value["run"]["verification_status"] = status.into();
            value["run"]["seed_contract"] = record.clone();
            let d = parse_detail(&value.to_string()).unwrap();
            assert_eq!(d.goal, expected);
            let mut watch = listed(runs);
            let list = screen(&mut watch, 80, 24);
            if status == "blocked" {
                match &expected {
                    Goal::Matched(_) => {
                        assert!(
                            list.lines().any(|line| line.starts_with("     TEST café")),
                            "{list}"
                        );
                        assert!(!list.contains("TEST full goal tail"));
                    }
                    Goal::NoSeed => assert!(!list.contains("seed"), "{list}"),
                    Goal::Unknown => assert!(
                        list.lines().any(|line| line.starts_with(
                            "    Goal unknown: the original seed changed or can't be found."
                        )),
                        "{list}"
                    ),
                }
            } else {
                assert!(
                    !list.contains("café")
                        && !list.contains("seed")
                        && !list.contains("TEST full goal tail"),
                    "{list}"
                );
                let rows: Vec<_> = list.lines().collect();
                let at = rows
                    .iter()
                    .position(|line| line.contains("› TEST-run"))
                    .unwrap();
                assert!(
                    rows[at + 1].trim().is_empty(),
                    "quiet run gained a sub-line: {list}"
                );
            }
            watch.detail = Some(("TEST-run".into(), Load::Done(Ok(d), Instant::now())));
            let shown = screen(&mut watch, 80, 24);
            let Load::Done(Ok(d), _) = &watch.detail.as_ref().unwrap().1 else {
                panic!()
            };
            let raw = detail_text(
                "TEST-run",
                &Load::Done(Ok(d.clone()), Instant::now()),
                Palette::new(true),
            );
            match &expected {
                Goal::Matched(_) => {
                    let raw: Vec<_> = raw
                        .lines
                        .iter()
                        .map(|line| {
                            line.spans
                                .iter()
                                .map(|span| span.content.as_ref())
                                .collect::<String>()
                        })
                        .collect();
                    assert!(
                        raw.windows(2)
                            .any(|lines| lines == [" TEST café  ", "TEST full goal tail  "])
                    );
                    assert!(shown.contains("TEST café") && shown.contains("TEST full goal tail"));
                }
                Goal::NoSeed => {
                    assert_eq!(
                        shown
                            .matches("No confirmed seed was recorded for this run.")
                            .count(),
                        1
                    );
                    assert!(!shown.contains("Goal unknown"));
                }
                Goal::Unknown => {
                    assert!(
                        shown
                            .contains("Goal unknown: the original seed changed or can't be found.")
                    );
                    assert!(!shown.contains("No confirmed seed"));
                }
            }
        }
    }
}

#[test]
fn quiet_rows_clip_to_one_line_and_selected_run_stays_visible() {
    for (width, height) in [(80, 24), (100, 30)] {
        let mut runs = parse_list(&envelope(&run(
            &format!("TEST-wide-{}", "界".repeat(100)),
            "current",
        )))
        .unwrap();
        runs[0].goal = Goal::Matched("TEST hidden quiet goal\nTEST hidden quiet tail".into());
        let mut watch = listed(runs);
        let shown = screen(&mut watch, width, height);
        let rows: Vec<_> = shown.lines().collect();
        let at = rows
            .iter()
            .position(|line| line.contains("TEST-wide-"))
            .unwrap();
        assert!(rows[at + 1].trim().is_empty(), "{shown}");
        assert!(!shown.contains("hidden quiet"));
        let runs = (0..60)
            .map(|i| {
                let mut run = parse_list(&envelope(&run(
                    &format!("TEST-scroll-{i}"),
                    if i % 2 == 0 { "blocked" } else { "current" },
                )))
                .unwrap()
                .remove(0);
                run.next = "TEST wrapped next step ".repeat(10);
                run.goal = Goal::Matched(format!("TEST first line {i}\nTEST hidden tail {i}"));
                run
            })
            .collect();
        let mut watch = listed(runs);
        for _ in 0..59 {
            watch.key(KeyEvent::from(KeyCode::Down));
        }
        let shown = screen(&mut watch, width, height);
        assert!(
            shown.contains("› TEST-scroll-59") && shown.contains("esc back"),
            "{shown}"
        );
        assert!(!shown.contains("TEST hidden tail"));
        watch.key(KeyEvent::from(KeyCode::Up));
        assert!(screen(&mut watch, width, height).contains("› TEST-scroll-57"));
    }
}

fn alive(dir: &Path) -> bool {
    let pid: i32 = fs::read_to_string(dir.join("TEST-pid"))
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    // SAFETY: signal 0 only checks whether the process exists; it sends nothing.
    unsafe { libc::kill(pid, 0) == 0 }
}
