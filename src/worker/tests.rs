//! Public worker transactions against a logging TEST SDLC process; no provider or operator data.

use super::*;
use serde_json::json;
use std::process::Command;
use std::{fs, time::Instant};

struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    state: PathBuf,
    log: PathBuf,
    request: Start,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::Builder::new()
            .prefix("tinkery-TEST-worker-'")
            .tempdir()
            .unwrap();
        let base = temp.path().canonicalize().unwrap();
        let root = base.join("TEST-source");
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::create_dir_all(root.join("_system/scripts")).unwrap();
        let session = base.join("TEST-sessions/TEST-session");
        fs::create_dir_all(&session).unwrap();
        crate::seed_me::ledger::fixtures::confirmed(&session, "TEST exact goal  \nline two", "r1");
        let target = ready::read(&session, Some(&root))
            .unwrap()
            .unwrap()
            .target
            .unwrap();
        let records = base.join("TEST-records");
        fs::create_dir(&records).unwrap();
        let state = base.join("TEST-state.json");
        let log = base.join("TEST-commands.jsonl");
        fs::write(
            &state,
            json!({
                "source": root, "build": base.join("TEST-build"), "log": log,
                "id": "11111111-1111-4111-8111-111111111111", "exists": false,
                "verification": "not-run", "turn": "not-ended", "calls": 0
            })
            .to_string(),
        )
        .unwrap();
        fs::write(
            root.join("_system/scripts/sdlc.py"),
            SCRIPT.replace("@STATE@", &serde_json::to_string(&state).unwrap()),
        )
        .unwrap();
        fs::write(root.join("_system/scripts/pi-launcher"), "TEST adapter").unwrap();
        fs::write(root.join("context.txt"), "TEST committed context\n").unwrap();
        fs::write(root.join("AGENTS.md"), "TEST committed instructions\n").unwrap();
        fs::write(root.join(".gitignore"), "cache/\n").unwrap();
        git_test(&root, &["init", "-q"]);
        git_test(&root, &["add", "."]);
        git_test(&root, &["commit", "-qm", "TEST baseline"]);
        let request = Start {
            target,
            root: root.clone(),
            record_root: Ok(records),
            config: Config {
                model: "test/exact:low".into(),
                responses: 3,
            },
        };
        Self {
            _temp: temp,
            root,
            state,
            log,
            request,
        }
    }

    fn edit(&self, change: impl FnOnce(&mut Value)) {
        let mut value: Value = serde_json::from_slice(&fs::read(&self.state).unwrap()).unwrap();
        change(&mut value);
        fs::write(&self.state, value.to_string()).unwrap();
    }

    fn commands(&self) -> Vec<Vec<String>> {
        fs::read_to_string(&self.log)
            .unwrap_or_default()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    fn start(&self) -> Snapshot {
        wait(Job::start(self.request.clone())).unwrap()
    }
}

fn git_test(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args([
            "-c",
            "user.name=TEST Worker",
            "-c",
            "user.email=test@localhost",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
        ])
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn wait(mut job: Job) -> Result<Snapshot, Failure> {
    let end = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(result) = job.poll() {
            return result;
        }
        assert!(Instant::now() < end, "worker transaction timed out");
        thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn no_allowance_or_invalid_pin_runs_no_commands() {
    for config in [
        Config::default(),
        Config {
            model: "test/exact:low".into(),
            responses: 0,
        },
        Config {
            model: "default".into(),
            responses: 3,
        },
        Config {
            model: "test/*:low".into(),
            responses: 3,
        },
    ] {
        let fixture = Fixture::new();
        let mut request = fixture.request.clone();
        request.config = config;
        assert!(wait(Job::start(request)).is_err());
        assert!(fixture.commands().is_empty());
    }
}

#[test]
fn changed_selected_seed_refuses_before_any_command() {
    let fixture = Fixture::new();
    fs::write(
        ready::path(&fixture.request.target.session),
        "TEST replaced seed",
    )
    .unwrap();
    assert!(
        wait(Job::start(fixture.request.clone()))
            .unwrap_err()
            .message
            .contains("seed")
    );
    assert!(fixture.commands().is_empty());
}

#[test]
fn stale_selected_version_refuses_even_when_seed_bytes_are_unchanged() {
    let fixture = Fixture::new();
    let mut request = fixture.request.clone();
    request.target.version += 1;
    assert!(
        wait(Job::start(request))
            .unwrap_err()
            .message
            .contains("seed")
    );
    assert!(fixture.commands().is_empty());
}

#[test]
fn known_different_repository_refuses_before_any_command() {
    let fixture = Fixture::new();
    let other = fixture.root.parent().unwrap().join("TEST-other");
    fs::create_dir_all(other.join(".git")).unwrap();
    think_record::write(
        fixture.request.record_root.as_ref().unwrap(),
        &think_record::Session {
            origin: &think_record::Origin::simulated(),
            turns: &[],
            checks: vec![],
            goal: "TEST exact goal  \nline two",
            seed_me_session: &fixture.request.target.session,
            repo: Some(&other),
            confirmed: std::time::SystemTime::now(),
        },
    )
    .unwrap();
    assert!(
        wait(Job::start(fixture.request.clone()))
            .unwrap_err()
            .message
            .contains("different repository")
    );
    assert!(fixture.commands().is_empty());
}

#[test]
fn authored_source_changes_refuse_before_run_creation_or_admission() {
    for kind in ["tracked", "staged", "untracked"] {
        let fixture = Fixture::new();
        let path = fixture.root.join(if kind == "untracked" {
            "new-context.txt"
        } else {
            "context.txt"
        });
        fs::write(&path, "TEST uncommitted context\n").unwrap();
        if kind == "staged" {
            git_test(&fixture.root, &["add", "context.txt"]);
            // Staged and worktree changes can cancel in `git diff HEAD`; both still matter.
            fs::write(&path, "TEST committed context\n").unwrap();
        }
        let failure = wait(Job::start(fixture.request.clone())).unwrap_err();
        assert!(failure.message.contains("Commit source changes first"));
        assert!(failure.run.is_none());
        assert!(fixture.commands().is_empty());
        assert!(!fixture.root.join("runs").exists());
    }
}

#[test]
fn ignored_cache_run_records_and_runtime_generated_rules_do_not_block_start() {
    let fixture = Fixture::new();
    fs::create_dir(fixture.root.join("cache")).unwrap();
    fs::write(fixture.root.join("cache/TEST-cache"), "TEST ignored cache").unwrap();
    fs::create_dir_all(fixture.root.join("runs/TEST-old")).unwrap();
    fs::write(
        fixture.root.join("runs/TEST-old/receipt.json"),
        "TEST existing receipt",
    )
    .unwrap();
    fixture.edit(|value| value["generated_rules"] = true.into());
    assert_eq!(
        fixture.start().state,
        State::Active {
            turn: Turn::Running
        }
    );
}

fn confirmed_retry(fixture: &Fixture) -> Start {
    use crate::seed_me::{ledger, retry};
    let old = &fixture.request.target.session;
    let checkout = fixture
        ._temp
        .path()
        .canonicalize()
        .unwrap()
        .join("TEST-retry");
    git_test(
        &fixture.root,
        &[
            "worktree",
            "add",
            "-q",
            "--detach",
            checkout.to_str().unwrap(),
            "HEAD",
        ],
    );
    let base = retry::Base {
        checkout,
        head: source_head(&fixture.root).unwrap(),
    };
    let session = ledger::initialize(old.parent().unwrap(), "2026-11-06T00:00:00+00:00").unwrap();
    let previous = ledger::load(old).unwrap();
    retry::prior(&ready::path(old), previous["goal"].string().unwrap())
        .unwrap()
        .save(&session, &base, "TEST try with another sample", "TEST-run")
        .unwrap();
    ledger::set_checks(&session, &ledger::read_checks(old).unwrap()).unwrap();
    // Redraft the same TEST checks with the saved retry's own decisions and reviewed base.
    let new = ledger::load(&session).unwrap();
    let draft = fs::read_to_string(ready::path(old))
        .unwrap()
        .replace(
            &ledger::decisions_block(&previous).unwrap(),
            &ledger::decisions_block(&new).unwrap(),
        )
        .replace("status: confirmed for intake", "status: draft")
        .lines()
        .filter(|line| !line.starts_with("Confirmed by:"))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(ready::path(&session), format!("{draft}\n")).unwrap();
    ledger::confirm_seed(
        &session,
        "r1",
        "TEST confirmed retry",
        "2026-11-06T00:00:00+00:00",
    )
    .unwrap();
    let seed = ready::read(&session, Some(&fixture.root)).unwrap().unwrap();
    Start {
        target: seed.target.unwrap(),
        ..fixture.request.clone()
    }
}

#[test]
fn retry_handoff_requires_the_reviewed_detached_commit_for_both_entry_points() {
    let fixture = Fixture::new();
    let request = confirmed_retry(&fixture);
    let base = request.target.retry.as_ref().unwrap();
    let ready = |root: &Path| {
        ready::read(&request.target.session, Some(root))
            .unwrap()
            .unwrap()
    };
    let copy = ready(&fixture.root).commands.unwrap();
    let other = fixture._temp.path().join("TEST-unrelated");
    fs::create_dir(&other).unwrap();
    git_test(&other, &["init", "-q"]);
    assert!(ready(&other).commands.is_none());
    for switch in [
        vec!["switch", "-q", "-c", "TEST-unexpected-branch"],
        vec![
            "commit",
            "--allow-empty",
            "-qm",
            "TEST other hash with identical tree",
        ],
    ] {
        git_test(&base.checkout, &switch);
        assert!(ready(&fixture.root).commands.is_none());
        let manual = Command::new("sh").arg("-c").arg(&copy).status().unwrap();
        assert!(!manual.success());
        let failure = wait(Job::start(request.clone())).unwrap_err();
        assert!(
            failure.message.contains("detached checkout"),
            "{}",
            failure.message
        );
        assert!(fixture.commands().is_empty());
        git_test(&base.checkout, &["switch", "-q", "--detach", &base.head]);
    }
    assert!(
        Command::new("sh")
            .arg("-c")
            .arg(&copy)
            .status()
            .unwrap()
            .success()
    );
    let commands = fixture.commands();
    assert_eq!(commands[0][0], "new");
    assert!(commands[0][1].starts_with("retry-"));
    assert_eq!(commands[1][1], commands[0][1]);
    assert_eq!(commands.len(), 2);
    assert!(!fixture.root.join("runs").exists());
}

#[test]
fn retry_worker_accepts_only_the_candidate_plus_its_new_sdlc_evidence() {
    for project_edit in [false, true] {
        let fixture = Fixture::new();
        let request = confirmed_retry(&fixture);
        let base = request.target.retry.as_ref().unwrap();
        fixture.edit(|state| {
            state["source"] = json!(base.checkout);
            state["retry_stage"] = json!(project_edit);
        });
        let job = wait(Job::start(request));
        if project_edit {
            assert!(
                job.unwrap_err()
                    .message
                    .contains("did not start at the confirmed candidate")
            );
            assert!(!fixture.commands().iter().any(|cmd| cmd[0] == "step"));
        } else {
            let built = job.unwrap();
            assert_eq!(
                built.state,
                State::Active {
                    turn: Turn::Running
                }
            );
            assert_ne!(built.run.root, fixture.root);
            assert_ne!(built.run.slug, "test");
            assert!(!fixture.root.join("runs").exists());
        }
    }
}

#[test]
fn retry_worker_refuses_project_edits_after_checkout_and_at_prepared_launch() {
    for edit_at in ["checkout", "prepare"] {
        let fixture = Fixture::new();
        let request = confirmed_retry(&fixture);
        let base = request.target.retry.as_ref().unwrap();
        fixture.edit(|s| {
            s["source"] = json!(base.checkout);
            s["retry_stage"] = false.into();
            s["edit_at"] = edit_at.into();
        });
        let failure = wait(Job::start(request)).unwrap_err();
        assert!(
            failure.message.contains("Local source"),
            "{}",
            failure.message
        );
        let attempted = |verb| {
            fixture
                .commands()
                .iter()
                .any(|c| c[0] == "step" && c[3] == verb)
        };
        assert_eq!(attempted("prepare"), edit_at == "prepare");
        assert!(!attempted("launch"));
        if edit_at == "prepare" {
            let run = failure.run.unwrap();
            let prepared = wait(Job::act(run.clone(), Action::Refresh)).unwrap();
            let launch = || Job::act(run.clone(), Action::Launch(prepared.pin.clone().unwrap()));
            // The separate "launch prepared worker" key must recheck the same source boundary.
            assert!(wait(launch()).unwrap_err().message.contains("Local source"));
            assert!(!attempted("launch"));
            fs::remove_file(run.root.join("extra-context.txt")).unwrap();
            assert_eq!(
                wait(launch()).unwrap().state,
                State::Active {
                    turn: Turn::Running
                }
            );
        }
    }
}

#[test]
fn source_commit_changed_during_intake_refuses_before_worktree_or_admission() {
    let fixture = Fixture::new();
    fixture.edit(|value| value["advance_head"] = true.into());
    let failure = wait(Job::start(fixture.request.clone())).unwrap_err();
    assert!(failure.message.contains("source commit changed"));
    assert!(failure.run.is_some());
    assert!(
        !fixture
            .commands()
            .iter()
            .any(|args| args[0] == "step" || (args[0] == "stage" && args[2] == "3"))
    );
}

#[test]
fn starts_once_through_exact_argv_and_source_owned_isolated_checkout() {
    let fixture = Fixture::new();
    let snapshot = fixture.start();
    assert_eq!(snapshot.model, "test/exact:low");
    assert_eq!(snapshot.responses, 3);
    assert_eq!(
        snapshot.state,
        State::Active {
            turn: Turn::Running
        }
    );
    assert_ne!(snapshot.run.root, fixture.root);
    let commands = fixture.commands();
    let writes: Vec<_> = commands
        .iter()
        .filter(|args| {
            !matches!(args[0].as_str(), "status") && args.last().is_none_or(|s| s != "show")
        })
        .collect();
    assert_eq!(
        writes
            .iter()
            .map(|args| args[0].as_str())
            .collect::<Vec<_>>(),
        ["new", "stage", "stage", "step", "step"]
    );
    assert_eq!(writes[1][2], "1");
    assert_eq!(
        writes[2],
        &vec!["stage".to_owned(), "test".into(), "3".into()]
    );
    let prepared = writes[3];
    assert_eq!(&prepared[..4], ["step", "test", "build", "prepare"]);
    assert_eq!(
        prepared[5],
        snapshot
            .run
            .root
            .join("runs/test/worker-report.md")
            .to_str()
            .unwrap()
    );
    assert_eq!(prepared[7], "test/exact:low");
    assert_eq!(
        prepared[9],
        snapshot
            .run
            .root
            .join("_system/scripts/pi-launcher")
            .to_str()
            .unwrap()
    );
    assert_eq!(prepared[11], "3");
    assert!(
        wait(Job::start(fixture.request.clone()))
            .unwrap_err()
            .message
            .contains("already has a run")
    );
    assert_eq!(
        fixture
            .commands()
            .iter()
            .filter(|args| args.last().is_some_and(|arg| arg == "launch"))
            .count(),
        1
    );
}

#[test]
fn source_change_after_intake_stops_before_stage_three_or_provider() {
    let fixture = Fixture::new();
    fixture.edit(|value| value["change_seed"] = true.into());
    let failure = wait(Job::start(fixture.request.clone())).unwrap_err();
    assert!(failure.run.is_some());
    assert!(
        !fixture
            .commands()
            .iter()
            .any(|args| args[0] == "step" || (args[0] == "stage" && args[2] == "3"))
    );
}

#[test]
fn refused_admission_keeps_known_run_and_never_retries() {
    for refusal in ["gate", "uncertain"] {
        let fixture = Fixture::new();
        fixture.edit(|value| value["refusal"] = refusal.into());
        let failure = wait(Job::start(fixture.request.clone())).unwrap_err();
        let run = failure.run.unwrap();
        let refreshed = wait(Job::act(run, Action::Refresh)).unwrap();
        assert_eq!(
            refreshed.state,
            if refusal == "uncertain" {
                State::Uncertain
            } else {
                State::Prepared
            }
        );
        assert_eq!(
            fixture
                .commands()
                .iter()
                .filter(|args| args.last().is_some_and(|arg| arg == "launch"))
                .count(),
            1
        );
    }
}

#[test]
fn explicit_launch_recovers_prepared_step_but_never_uncertain_or_changed_contract() {
    for refusal in ["gate", "uncertain"] {
        let fixture = Fixture::new();
        fixture.edit(|value| value["refusal"] = refusal.into());
        let run = wait(Job::start(fixture.request.clone()))
            .unwrap_err()
            .run
            .unwrap();
        let observed = wait(Job::act(run.clone(), Action::Refresh)).unwrap();
        fixture.edit(|value| {
            value.as_object_mut().unwrap().remove("refusal");
        });
        fixture.edit(|value| value["step"]["max_responses"] = 99.into());
        assert!(
            wait(Job::act(
                run.clone(),
                Action::Launch(observed.pin.clone().unwrap())
            ))
            .unwrap_err()
            .message
            .contains("changed")
        );
        fixture.edit(|value| value["step"]["max_responses"] = 3.into());
        let result = wait(Job::act(run, Action::Launch(observed.pin.unwrap())));
        if refusal == "gate" {
            assert_eq!(
                result.unwrap().state,
                State::Active {
                    turn: Turn::Running
                }
            );
        } else {
            assert!(result.unwrap_err().message.contains("prepared"));
            assert_eq!(
                fixture
                    .commands()
                    .iter()
                    .filter(|args| args.last().is_some_and(|arg| arg == "launch"))
                    .count(),
                1
            );
        }
    }
}

#[test]
fn run_ids_preserve_canonical_uuid_v4_contract() {
    let fixture = Fixture::new();
    for id in [
        "01234567-89ab-4cde-8fab-0123456789ab",
        "01234567-89ab-4cde-9fab-0123456789ab",
        "01234567-89ab-4cde-afab-0123456789ab",
        "01234567-89ab-4cde-bfab-0123456789ab",
    ] {
        fixture.edit(|value| value["id"] = id.into());
        let run = Run {
            root: fixture.root.clone(),
            slug: "test".into(),
            id: id.into(),
        };
        let snapshot = wait(Job::act(run.clone(), Action::Refresh)).unwrap();
        assert_eq!(snapshot.run, run);
        assert_eq!(snapshot.state, State::NotStarted);
    }
    for id in [
        "",
        "01234567-89AB-4CDE-8FAB-0123456789AB",
        "01234567-89ab-1cde-8fab-0123456789ab",
        "01234567-89ab-7cde-8fab-0123456789ab",
        "01234567-89ab-4cde-7fab-0123456789ab",
        "01234567-89ab-4cde-cfab-0123456789ab",
        "01234567-89ab-4cde-8fab-0123456789a",
        "01234567-89ab-4cde-8fab-0123456789abc",
        "0123456789ab4cde8fab0123456789ab",
        "0123456-789ab-4cde-8fab-0123456789ab",
        "01234567_89ab-4cde-8fab-0123456789ab",
        "01234567-89ab-4cde-8fab-0123456789ag",
        "01234567-89ab-4cde-8fab-0123456789ab\n",
        "01234567-89ab-4cde-8fab-0123456789é",
    ] {
        fixture.edit(|value| value["id"] = id.into());
        let failure = wait(Job::act(
            Run {
                root: fixture.root.clone(),
                slug: "test".into(),
                id: id.into(),
            },
            Action::Refresh,
        ))
        .unwrap_err();
        assert_eq!(failure.message, "The run has no stable identity.", "{id:?}");
    }
    assert_eq!(fixture.commands().len(), 22);
    assert!(
        fixture
            .commands()
            .iter()
            .all(|args| args == &["status", "test", "--json"])
    );
}

#[test]
fn current_run_identity_is_required_before_resume_or_verify() {
    let fixture = Fixture::new();
    let initial = fixture.start();
    let run = initial.run;
    fixture.edit(|value| value["id"] = "22222222-2222-4222-8222-222222222222".into());
    let before = fixture.commands().len();
    for action in [
        Action::Resume(initial.pin.clone().unwrap()),
        Action::Verify,
        Action::StopOperator,
    ] {
        assert!(
            wait(Job::act(run.clone(), action))
                .unwrap_err()
                .message
                .contains("changed")
        );
    }
    assert!(
        fixture.commands()[before..]
            .iter()
            .all(|args| args[0] == "status")
    );
}

#[test]
fn resume_uses_recorded_contract_and_preserves_cumulative_cap() {
    let fixture = Fixture::new();
    let initial = fixture.start();
    let run = initial.run;
    fixture.edit(|value| value["turn"] = "ended".into());
    for _ in 0..2 {
        let snapshot = wait(Job::act(
            run.clone(),
            Action::Resume(initial.pin.clone().unwrap()),
        ))
        .unwrap();
        assert_eq!(snapshot.model, "test/exact:low");
        assert_eq!(snapshot.responses, 3);
    }
    assert!(
        wait(Job::act(run, Action::Resume(initial.pin.clone().unwrap())))
            .unwrap_err()
            .message
            .contains("cap")
    );
    for args in fixture
        .commands()
        .iter()
        .filter(|args| args.last().is_some_and(|arg| arg == "resume"))
    {
        assert_eq!(
            args,
            &vec![
                "step".to_owned(),
                "test".into(),
                "build".into(),
                "resume".into()
            ]
        );
    }
}

#[test]
fn running_turn_and_changed_disclosed_contract_cannot_resume() {
    let fixture = Fixture::new();
    let initial = fixture.start();
    assert!(
        wait(Job::act(
            initial.run.clone(),
            Action::Resume(initial.pin.clone().unwrap())
        ))
        .unwrap_err()
        .message
        .contains("ended turn")
    );
    fixture.edit(|value| value["turn"] = "ended".into());
    let fields = [
        ("model", json!("test/other:low")),
        ("max_responses", json!(99)),
        ("session", json!("TEST-substituted-session")),
        ("report", json!("/TEST/other-report.md")),
        ("launcher_sha256", json!("b".repeat(64))),
    ];
    for (field, replacement) in fields {
        let original = serde_json::from_slice::<Value>(&fs::read(&fixture.state).unwrap()).unwrap()
            ["step"][field]
            .clone();
        fixture.edit(|value| value["step"][field] = replacement);
        assert!(
            wait(Job::act(
                initial.run.clone(),
                Action::Resume(initial.pin.clone().unwrap())
            ))
            .unwrap_err()
            .message
            .contains("changed")
        );
        fixture.edit(|value| value["step"][field] = original);
    }
    assert!(
        !fixture
            .commands()
            .iter()
            .any(|args| args.last().is_some_and(|arg| arg == "resume"))
    );
}

#[test]
fn explicit_refresh_follows_only_the_same_run_to_its_reported_build_checkout() {
    let fixture = Fixture::new();
    let initial = fixture.start();
    let source = Run {
        root: fixture.root.clone(),
        ..initial.run.clone()
    };
    let refreshed = wait(Job::act(source.clone(), Action::Refresh)).unwrap();
    assert_eq!(refreshed.run, initial.run);
    assert!(
        wait(Job::act(source, Action::Verify))
            .unwrap_err()
            .message
            .contains("changed")
    );
}

#[test]
fn finish_requires_current_passing_checks_and_a_finished_report() {
    let fixture = Fixture::new();
    let initial = fixture.start();
    let run = initial.run;
    fixture.edit(|value| value["turn"] = "ended".into());
    assert!(
        wait(Job::act(run.clone(), Action::Finish))
            .unwrap_err()
            .message
            .contains("Verify first")
    );
    let verified = wait(Job::act(run.clone(), Action::Verify)).unwrap();
    assert_eq!(verified.verification, "current");
    assert!(
        wait(Job::act(run.clone(), Action::Finish))
            .unwrap_err()
            .message
            .contains("report")
    );
    fs::write(
        run.root.join("runs/test/worker-report.md"),
        "TEST real fixture report",
    )
    .unwrap();
    for (state, passed) in [("current", false), ("stale", true)] {
        fixture.edit(|value| {
            value["verification"] = state.into();
            value["passed"] = passed.into();
        });
        assert!(
            wait(Job::act(run.clone(), Action::Finish))
                .unwrap_err()
                .message
                .contains("Verify first")
        );
    }
    fixture.edit(|value| {
        value["verification"] = "current".into();
        value["passed"] = true.into();
    });
    let done = wait(Job::act(run.clone(), Action::Finish)).unwrap();
    assert_eq!(done.state, State::Completed);
    assert_eq!(done.verification, "current");
}

#[test]
fn failed_verification_keeps_ended_worker_available_for_same_session_repair() {
    let fixture = Fixture::new();
    let initial = fixture.start();
    let run = initial.run;
    fs::write(
        run.root.join("runs/test/worker-report.md"),
        "TEST finished report",
    )
    .unwrap();
    fixture.edit(|value| {
        value["turn"] = "ended".into();
        value["fail_verify"] = true.into();
    });
    assert!(
        wait(Job::act(run.clone(), Action::Verify))
            .unwrap_err()
            .message
            .contains("verification failed")
    );
    assert!(
        wait(Job::act(run.clone(), Action::Finish))
            .unwrap_err()
            .message
            .contains("Verify first")
    );
    let observed = wait(Job::act(run.clone(), Action::Refresh)).unwrap();
    assert_eq!(observed.state, State::Active { turn: Turn::Ended });
    assert_eq!(observed.verification, "failed");
    assert_eq!(observed.pin, initial.pin);
    let resumed = wait(Job::act(run.clone(), Action::Resume(observed.pin.unwrap()))).unwrap();
    assert_eq!(resumed.model, initial.model);
    assert_eq!(resumed.responses, initial.responses);
    assert!(
        !fixture
            .commands()
            .iter()
            .any(|args| args.last().is_some_and(|arg| arg == "complete"))
    );
    fixture.edit(|value| {
        value["fail_verify"] = false.into();
    });
    wait(Job::act(run.clone(), Action::Verify)).unwrap();
    assert_eq!(
        wait(Job::act(run, Action::Finish)).unwrap().state,
        State::Completed
    );
}

#[test]
fn stopped_turn_is_never_completed_and_changed_seed_does_not_block_stop() {
    for (action, cap) in [(Action::StopOperator, false), (Action::StopCap, true)] {
        let fixture = Fixture::new();
        let initial = fixture.start();
        let run = initial.run;
        fs::write(
            ready::path(&fixture.request.target.session),
            "TEST replaced seed",
        )
        .unwrap();
        assert!(
            wait(Job::act(
                run.clone(),
                Action::Resume(initial.pin.clone().unwrap())
            ))
            .is_err()
        );
        let snapshot = wait(Job::act(run, action)).unwrap();
        assert_eq!(snapshot.state, State::Stopped { cap });
        assert_ne!(snapshot.verification, "current");
    }
}

#[test]
fn drop_waits_for_admission_receipt_without_killing_or_retrying_it() {
    let fixture = Fixture::new();
    fixture.edit(|value| value["slow_launch"] = true.into());
    let job = Job::start(fixture.request.clone());
    drop(job);
    let value: Value = serde_json::from_slice(&fs::read(&fixture.state).unwrap()).unwrap();
    assert_eq!(value["step"]["state"], "active");
    assert_eq!(value["calls"], 1);
}

const SCRIPT: &str = r#"import hashlib,json,pathlib,shutil,subprocess,sys,time
state=pathlib.Path(@STATE@)
data=json.loads(state.read_text())
args=sys.argv[1:]
with pathlib.Path(data['log']).open('a') as log: log.write(json.dumps(args)+'\n')
root=pathlib.Path.cwd().resolve()
def save(): state.write_text(json.dumps(data))
def fail(text):
 print(text,file=sys.stderr);save();sys.exit(1)
def view():
 run={'slug':data.get('slug','test'),'run_id':data['id'],'seed_contract':data.get('seed'),'steps':[],
      'verification_status':data['verification'],
      'verification':{'passed':data.get('passed',data['verification']=='current')},'next_action':'TEST next action',
      'actions':{'verify':{'allowed':True,'reason':'','timeout_seconds':5}}}
 if data.get('built') and root==pathlib.Path(data['source']): run['checkout']=data['build']
 if data.get('step'):
  record=data['step']; step={'name':'build','state':record['state']}
  if step['state']=='active': step['turn']=data['turn']
  if step['state']=='stopped': step['stopped_by']=record['stopped_by']
  run['steps']=[step]
 return run
if args[0]=='status':
 out={'protocol':'tink-sdlc','api_version':1}
 out.update({'runs':[view()] if data['exists'] else []} if args==['status','--json'] else {'run':view()})
 print(json.dumps(out));sys.exit()
if args[0]=='new':
 if data['exists']:fail('TEST duplicate run')
 data['slug']=args[1];data['exists']=True;(root/'runs'/args[1]).mkdir(parents=True)
elif args[0]=='stage' and args[2]=='1':
 seed=pathlib.Path(args[args.index('--seed-contract')+1]);content=seed.read_bytes()
 data['seed']={'path':'runs/test/seed-contract.md','source':str(seed),'sha256':hashlib.sha256(content).hexdigest()}
 if data.get('change_seed'): seed.write_text('TEST changed source')
 if data.get('generated_rules'): (root/'AGENTS.md').write_text('TEST generated stage rules\n')
 if data.get('advance_head'):
  subprocess.run(['git','-c','user.name=TEST Worker','-c','user.email=test@localhost','-c','commit.gpgsign=false',
                  '-c','core.hooksPath=/dev/null','commit','--allow-empty','-qm','TEST concurrent commit'],check=True)
elif args[0]=='stage' and args[2]=='3':
 build=pathlib.Path(data['build'])
 if 'retry_stage' in data:
  # SDLC commits the run's evidence before forking its build. A project edit in that commit is outside the pin.
  git=['git','-c','user.name=TEST Worker','-c','user.email=test@localhost','-c','commit.gpgsign=false','-c','core.hooksPath=/dev/null']
  (root/'runs'/args[1]/'TEST-evidence').write_text('TEST only')
  if data['retry_stage']: (root/'TEST-other-source').write_text('TEST concurrent change')
  subprocess.run([*git,'add','-A'],check=True)
  subprocess.run([*git,'commit','-qm','TEST stage 3'],check=True)
  subprocess.run([*git,'worktree','add','-q','--detach',str(build),'HEAD'],check=True)
  # Stage 3 compiles its rules after checkout and can leave run evidence outside the commit.
  with (build/'AGENTS.md').open('a') as out: out.write('\n<!-- tink:rules begin -->\nTEST rules\n<!-- tink:rules end -->\n')
  (build/'runs'/args[1]/'TEST-stage-log').write_text('TEST runtime evidence')
  if data.get('edit_at') == 'checkout': (build/'context.txt').write_text('TEST post-checkout change')
 else:
  (build/'.git').mkdir(parents=True)
  (build/'_system/scripts').mkdir(parents=True);(build/'runs/test').mkdir(parents=True)
  shutil.copy(root/'_system/scripts/sdlc.py',build/'_system/scripts/sdlc.py')
 (build/'_system/scripts/pi-launcher').write_text('TEST adapter')
 data['built']=True
elif args[0]=='step':
 op=args[3]
 if op=='prepare':
  if data.get('step'):fail('TEST step exists')
  if data.get('edit_at') == 'prepare': (root/'extra-context.txt').write_text('TEST file created after prepare')
  data['step']={'schema':1,'run_id':data['id'],'checkout':str(root),'state':'prepared',
   'model':args[args.index('--model')+1],'max_responses':int(args[args.index('--max-responses')+1]),
   'report':args[args.index('--report')+1],'launcher':args[args.index('--launcher')+1],
   'launcher_sha256':'a'*64,'session':None}
 elif op in ('launch','resume'):
  if data.get('refusal')=='gate':fail('TEST current gate refuses admission')
  if data.get('refusal')=='uncertain':data['step']['state']='uncertain';fail('TEST uncertain admission')
  if data['calls']>=data['step']['max_responses']:fail('TEST response cap reached')
  if data.get('slow_launch'):time.sleep(.15)
  data['step'].update(state='active',session='TEST-exact-session');data['calls']+=1
 elif op=='complete':
  if '--stopped-by' in args:
   data['step'].update(state='stopped',stopped_by=args[-1])
  elif data['turn']!='ended' or not pathlib.Path(data['step']['report']).is_file():fail('TEST missing finished report')
  else:data['step']['state']='completed'
 elif op!='show':fail('TEST unknown step operation')
 print(json.dumps(data['step']))
elif args[0]=='verify':
 if data.get('fail_verify'):data['verification']='failed';fail('TEST verification failed')
 data['verification']='current'
else:fail('TEST unexpected command')
save()
"#;
