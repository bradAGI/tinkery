use super::*;
use serde_json::json;
use std::fs;

mod descendant {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/command_descendant.rs"
    ));
}

#[test]
fn successful_commands_stop_descendants_and_preserve_results() {
    completed_command(0);
}

#[test]
fn failed_commands_stop_descendants_and_preserve_results() {
    completed_command(7);
}

fn completed_command(exit: u8) {
    let fixture = descendant::Fixture::new(exit);
    let result = run::execute(
        fixture.root(),
        std::process::Command::new(fixture.program()),
        &AtomicBool::new(false),
        std::time::Duration::from_secs(10),
    );
    fixture.assert_stopped();
    assert!(fixture.input().is_empty(), "status reads receive no input");
    if exit == 0 {
        assert_eq!(result.unwrap(), " TEST reply café  \n");
    } else {
        let error = result.unwrap_err();
        assert!(
            error.starts_with(&fixture.program().to_string_lossy().to_string()),
            "{error}"
        );
        assert!(error.contains("exit status: 7"), "{error}");
        assert!(error.ends_with("TEST command failed"), "{error}");
    }
}

#[test]
fn completed_reads_wake_their_owner_with_success_or_failure() {
    for outcome in [Ok("TEST result"), Err("TEST refusal".to_owned())] {
        std::thread::spawn(move || {
            let expected = outcome.clone();
            let read = Read::task(move |_| {
                std::thread::sleep(std::time::Duration::from_millis(30));
                outcome
            });
            let started = std::time::Instant::now();
            std::thread::park_timeout(std::time::Duration::from_secs(2));
            assert!(started.elapsed() < std::time::Duration::from_secs(1));
            assert_eq!(read.poll(), Some(expected));
        })
        .join()
        .unwrap();
    }
}

#[test]
fn completion_before_parking_retains_the_wakeup() {
    std::thread::spawn(|| {
        let read = Read::task(|_| Ok("TEST ready"));
        let until = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while read.poll().is_none() {
            assert!(std::time::Instant::now() < until);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let started = std::time::Instant::now();
        std::thread::park_timeout(std::time::Duration::from_secs(2));
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
    })
    .join()
    .unwrap();
}

fn fixture() -> (tempfile::TempDir, PathBuf, Value) {
    let root = tempfile::Builder::new()
        .prefix("tinkery-TEST-join-")
        .tempdir()
        .unwrap();
    let source = root
        .path()
        .canonicalize()
        .unwrap()
        .join("session/seed-contract.md");
    fs::create_dir(source.parent().unwrap()).unwrap();
    crate::seed_me::ledger::fixtures::confirmed(
        source.parent().unwrap(),
        " TEST café\nexact goal  ",
        "r1",
    );
    let run = json!({"slug":"unrelated-name", "seed_contract":{
        "path":"runs/other/seed-contract.md", "source":source,
        "sha256": format!("{:x}", Sha256::digest(fs::read(&source).unwrap()))
    }});
    (root, source, run)
}

#[test]
fn canonical_source_and_verified_current_bytes_join_the_exact_goal() {
    let (_root, source, run) = fixture();
    let seed = Seed::parse(&run).unwrap().unwrap();
    assert!(started(&[seed], &source).unwrap());
    assert_eq!(goal(&run), Goal::Matched(" TEST café\nexact goal  ".into()));
    let symlink = source.with_file_name("TEST-alias.md");
    std::os::unix::fs::symlink(&source, &symlink).unwrap();
    assert!(started(&[Seed::parse(&run).unwrap().unwrap()], &symlink).unwrap());
    let mut alias = run.clone();
    alias["seed_contract"]["source"] = source
        .parent()
        .unwrap()
        .join("./seed-contract.md")
        .to_string_lossy()
        .into_owned()
        .into();
    assert_eq!(
        goal(&alias),
        Goal::Unknown,
        "a noncanonical record never joins"
    );
}

#[test]
fn unchanged_seed_bytes_cannot_lend_a_changed_ledger_goal() {
    let (_root, source, run) = fixture();
    let ledger_path = source.parent().unwrap().join("ledger.json");
    let original = fs::read(&ledger_path).unwrap();
    let seed = Seed::parse(&run).unwrap().unwrap();
    for matching_origin in [false, true] {
        let mut ledger: Value = serde_json::from_slice(&original).unwrap();
        ledger["goal"] = "TEST changed goal".into();
        if matching_origin {
            ledger["nodes"][0]["answer"] = ledger["goal"].clone();
        }
        fs::write(&ledger_path, ledger.to_string()).unwrap();
        assert!(
            seed.matches(&source).unwrap(),
            "the seed bytes stayed intact"
        );
        assert_eq!(
            goal(&run),
            Goal::Unknown,
            "an edited goal was called original"
        );
    }
    fs::write(ledger_path, original).unwrap();
    assert_eq!(goal(&run), Goal::Matched(" TEST café\nexact goal  ".into()));
}

#[test]
fn missing_or_changed_confirmation_cannot_lend_a_goal() {
    let (_root, source, run) = fixture();
    let receipt = source.parent().unwrap().join("seed-confirmation.json");
    let original = fs::read(&receipt).unwrap();
    let mut changed: Value = serde_json::from_slice(&original).unwrap();
    changed["decisions_sha256"] = "0".repeat(64).into();
    fs::write(&receipt, changed.to_string()).unwrap();
    assert_eq!(goal(&run), Goal::Unknown);
    fs::remove_file(&receipt).unwrap();
    assert_eq!(goal(&run), Goal::Unknown);
    fs::write(receipt, original).unwrap();
    assert_eq!(goal(&run), Goal::Matched(" TEST café\nexact goal  ".into()));
}

#[test]
fn another_file_beside_a_verified_seed_cannot_lend_its_goal() {
    let (_root, source, mut run) = fixture();
    let unrelated = source.with_file_name("unrelated.md");
    fs::copy(&source, &unrelated).unwrap();
    run["seed_contract"]["source"] = unrelated.to_string_lossy().into_owned().into();
    assert!(
        Seed::parse(&run)
            .unwrap()
            .unwrap()
            .matches(&unrelated)
            .unwrap()
    );
    assert_eq!(goal(&run), Goal::Unknown);
}

#[test]
fn edited_seed_is_unknown_not_started_and_never_lends_its_goal() {
    let (_root, source, run) = fixture();
    fs::write(&source, "TEST edited seed\n").unwrap();
    let seed = Seed::parse(&run).unwrap().unwrap();
    assert!(
        started(&[seed], &source).is_err(),
        "changed seed was called started"
    );
    assert_eq!(
        goal(&run),
        Goal::Unknown,
        "changed seed lent a goal to the run"
    );
}

#[test]
fn moved_and_missing_originals_never_join_even_with_identical_names_and_bytes() {
    let (root, source, run) = fixture();
    let moved = root.path().join("moved/seed-contract.md");
    fs::create_dir(moved.parent().unwrap()).unwrap();
    fs::rename(&source, &moved).unwrap();
    let seed = Seed::parse(&run).unwrap().unwrap();
    assert!(!started(std::slice::from_ref(&seed), &moved).unwrap());
    assert_eq!(goal(&run), Goal::Unknown);
    assert!(started(&[seed], &source).is_err());
    fs::remove_file(moved).unwrap();
    assert_eq!(goal(&run), Goal::Unknown);
    assert!(
        !started(&[], &source).unwrap(),
        "an ended session without a recorded seed isn't an unknown start"
    );
}

#[test]
fn missing_or_malformed_seed_records_cannot_establish_a_start() {
    let (_root, _source, run) = fixture();
    assert_eq!(goal(&json!({"seed_contract":null})), Goal::NoSeed);
    assert_eq!(goal(&json!({})), Goal::Unknown);
    for value in [json!({}), json!({"source":"relative", "sha256":"x"})] {
        let mut run = run.clone();
        run["seed_contract"] = value;
        assert_eq!(goal(&run), Goal::Unknown);
    }
    assert!(Seed::parse(&json!({})).is_err());
    for value in [
        json!({"protocol":"other","api_version":1}),
        json!({"protocol":"tink-sdlc","api_version":"1"}),
    ] {
        assert!(envelope(&value.to_string()).is_err());
    }
}

#[test]
fn recorded_but_unreadable_originals_stay_unknown_not_unseeded() {
    let (_root, source, run) = fixture();
    fs::write(
        source.parent().unwrap().join("ledger.json"),
        "TEST malformed ledger",
    )
    .unwrap();
    assert_eq!(goal(&run), Goal::Unknown);
    fs::remove_file(&source).unwrap();
    fs::create_dir(&source).unwrap();
    assert_eq!(goal(&run), Goal::Unknown);
}

#[test]
fn a_failed_reader_start_says_what_to_check_without_os_internals() {
    let root = tempfile::Builder::new()
        .prefix("tinkery-TEST-reader-")
        .tempdir()
        .unwrap();
    let why = run::run(
        &root.path().join("TEST-missing-checkout"),
        &[],
        &AtomicBool::new(false),
        DEADLINE,
    )
    .unwrap_err();
    assert_eq!(
        why,
        "Couldn't start reading runs. Check that Python 3 is installed and the tink-sdlc checkout is accessible, then reopen this view to read again."
    );
}

#[test]
fn other_commands_name_themselves_when_they_fail_never_python_or_status() {
    let root = tempfile::Builder::new()
        .prefix("tinkery-TEST-command-")
        .tempdir()
        .unwrap();
    let missing = std::process::Command::new("tinkery-TEST-missing-gh");
    let why = run::execute(root.path(), missing, &AtomicBool::new(false), DEADLINE).unwrap_err();
    assert_eq!(
        why,
        "Couldn't start tinkery-TEST-missing-gh. Check that it is installed and on PATH."
    );
    let mut failing = std::process::Command::new("sh");
    failing.args(["-c", "echo 'fatal: TEST remote ref' >&2; exit 3"]);
    let why = run::execute(root.path(), failing, &AtomicBool::new(false), DEADLINE).unwrap_err();
    assert!(
        why.starts_with("sh exited with") && why.ends_with("fatal: TEST remote ref"),
        "{why}"
    );
    assert!(!why.contains("Python"), "{why}");
}

#[test]
fn unreadable_runs_refuse_all_start_classification() {
    let (root, _source, run) = fixture();
    let scripts = root.path().join("_system/scripts");
    fs::create_dir_all(&scripts).unwrap();
    let cancel = AtomicBool::new(false);
    for row in [
        json!({"verification_status":"invalid", "seed_contract":null}),
        json!({"slug":"old-report"}),
    ] {
        let report =
            json!({"protocol":"tink-sdlc", "api_version":1, "runs":[run, row]}).to_string();
        fs::write(scripts.join("sdlc.py"), format!("print({report:?})\n")).unwrap();
        let why = seeds(root.path(), &cancel).unwrap_err();
        if row["verification_status"] == "invalid" {
            assert_eq!(
                why,
                "A run couldn't be read. Check it in tink-sdlc, then reopen this view to read again."
            );
        }
    }
}

#[test]
fn generated_rules_are_dropped_only_from_well_formed_blocks() {
    let authored = b"TEST authored\n".as_slice();
    let rules = b"TEST authored\n\n<!-- tink:rules begin skillset=TEST-skillset -->\nTEST rules\n<!-- tink:rules end -->\n";
    assert_eq!(without_generated_rules(rules), authored);
    let trailing = b"TEST authored\n\n<!-- tink:rules begin -->\nTEST rules\n<!-- tink:rules end -->\nTEST after\n";
    assert_eq!(
        without_generated_rules(trailing),
        b"TEST authored\nTEST after\n"
    );
    for unbalanced in [
        b"TEST\n<!-- tink:rules begin -->\nTEST rules\n".as_slice(),
        b"TEST\n<!-- tink:rules end -->\n",
        b"<!-- tink:rules begin -->\n<!-- tink:rules begin -->\n<!-- tink:rules end -->\n",
    ] {
        assert_eq!(without_generated_rules(unbalanced), unbalanced);
    }
    assert_eq!(without_generated_rules(authored), authored);
}
