//! Citation warnings and reported paths through Shape's public ledger and seed interfaces.

use super::*;
use crate::seed_me::shaping::{self, Shaped};
use std::fs;

fn journey(checked: &str, warning: Option<&str>, paths: &[&str]) {
    let root = tempfile::Builder::new()
        .prefix("tinkery-TEST-citations-402-")
        .tempdir()
        .unwrap();
    let folder = root.path().join("TEST-folder");
    fs::create_dir_all(folder.join("docs")).unwrap();
    fs::write(folder.join("docs/recovery.md"), "TEST line\n".repeat(23)).unwrap();
    fs::write(folder.join("docs/research.md"), "TEST line\n".repeat(91)).unwrap();
    fs::write(folder.join("one.rs"), "TEST no trailing newline").unwrap();
    fs::write(folder.join("123.rs"), "TEST numeric filename\n").unwrap();
    fs::write(folder.join("empty.rs"), "").unwrap();
    fs::write(folder.join("report.md"), "TEST unrelated report\n").unwrap();
    fs::write(folder.join("docs/spaced name.md"), "TEST one\nTEST two\n").unwrap();
    fs::create_dir(folder.join("directory.rs")).unwrap();
    fs::write(root.path().join("outside.rs"), "TEST outside\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::{PermissionsExt, symlink};
        symlink(root.path().join("outside.rs"), folder.join("linked.rs")).unwrap();
        if checked == "unreadable.rs:1" {
            let unreadable = folder.join("unreadable.rs");
            fs::write(&unreadable, "TEST unreadable\n").unwrap();
            fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o000)).unwrap();
            if fs::read(unreadable).is_ok() {
                eprintln!("Unreadable-file case unavailable: this test user can read mode 000.");
                return;
            }
        }
    }
    let session = publication_tests::session(root.path());
    let observed = "TEST observation café\n  kept exactly";
    let version = ask(
        &session,
        read(&session).unwrap().version(),
        &Ask {
            id: "citation-choice".into(),
            label: "TEST next choice".into(),
            question: "TEST continue?".into(),
            gate: "TEST next step".into(),
            found: vec![Found {
                id: "citation-fact".into(),
                claim: json!({"type":"observation", "scope":"TEST cited files"}),
                checked: checked.into(),
                observed: observed.into(),
                supported_by: None,
                contradicts: None,
            }],
            resolves: None,
            supported_by: Some(json!(["citation-fact"])),
            prerequisites: None,
            suggestion: None,
            assumed: vec![],
            options: vec![],
            against: None,
            confidence: None,
        },
        &folder,
    )
    .unwrap();
    let original_receipt = read(&session).unwrap().findings()[0]["evidence"][0].clone();
    assert_eq!(original_receipt["checked"], checked);
    assert_eq!(original_receipt["observed"], observed);
    let shown = shaping::read(&session).unwrap();
    let Shaped::Question { found, .. } = shown.stage else {
        panic!("Expected the saved question: {shown:?}");
    };
    let shown = found.join("\n");
    assert_eq!(
        shown.contains("citation not found:"),
        warning.is_some(),
        "{checked}: {shown}"
    );
    if let Some(reason) = warning {
        assert!(shown.contains(reason), "{checked}: {shown}");
    }
    let version = answer(
        &session,
        version,
        "citation-choice",
        "TEST continue unchanged",
        None,
    )
    .unwrap();
    let checks =
        serde_json::from_str(include_str!("../../../tests/fixtures/shape-checks.json")).unwrap();
    draft_seed(
        &session,
        version,
        &ledger::fixtures::prose_reply(include_str!(
            "../../../tests/fixtures/shape-seed-reply.txt"
        )),
        &checks,
        &[],
        &folder,
        None,
    )
    .unwrap();
    let shown = shaping::read(&session).unwrap();
    let Shaped::Drafted {
        text: Some(seed), ..
    } = shown.stage
    else {
        panic!("Expected a saved draft, without confirmation: {shown:?}");
    };
    assert_eq!(
        seed,
        fs::read_to_string(session.join("seed-contract.md")).unwrap()
    );
    let evidence = seed
        .split("## Evidence and uncertainty\n")
        .nth(1)
        .unwrap()
        .split("\n## ")
        .next()
        .unwrap();
    let expected = if paths.is_empty() {
        "- No attributable read paths are known.\n".into()
    } else {
        paths
            .iter()
            .map(|path| format!("- {path}\n"))
            .collect::<String>()
    };
    assert!(
        evidence.contains(&format!(
            "Grounded in:\n{expected}\nProvenance unavailable:"
        )),
        "{checked}: {evidence}"
    );
    assert_eq!(
        evidence.contains("citation not found:"),
        warning.is_some(),
        "{checked}: {evidence}"
    );
    assert!(
        evidence.contains(&checked.split_whitespace().collect::<Vec<_>>().join(" ")),
        "{evidence}"
    );
    assert!(
        evidence.contains("TEST observation café kept exactly"),
        "{evidence}"
    );
    assert_eq!(
        read(&session).unwrap().findings()[0]["evidence"][0],
        original_receipt
    );
    assert_eq!(read(&session).unwrap().goal(), Some("TEST goal"));
    assert!(!session.join("seed-confirmation.json").exists());
}

#[test]
fn citation_ranges_reach_questions_and_seeds_without_losing_their_endpoints() {
    for checked in [
        "docs/recovery.md:3–29",
        "docs/recovery.md:3—29",
        "docs/recovery.md:3-29",
        "docs/recovery.md:3 – 29",
        "docs/recovery.md:3- 29",
        "docs/recovery.md:3 —29",
    ] {
        journey(checked, Some("lines, not"), &[]);
    }
}

#[test]
fn abbreviated_citation_ranges_check_every_later_bound() {
    for checked in [
        "docs/research.md:3–9,82–93",
        "docs/research.md:3-9, 82-93",
        "docs/recovery.md:1, docs/research.md:82–93",
    ] {
        journey(checked, Some("lines, not"), &[]);
    }
}

#[test]
fn valid_citations_keep_grouped_receipts_and_deduplicated_reported_paths() {
    for checked in [
        "docs/recovery.md:23",
        "docs/recovery.md:3-23",
        "docs/recovery.md:3–23",
        "docs/recovery.md:3—23",
        "docs/recovery.md:3 – 23",
        "docs/recovery.md:3- 23",
        "docs/recovery.md:3 —23",
        "docs/recovery.md:1, 3–23",
        "Read `docs/recovery.md:1,3-9, 12—23`.",
        "(docs/recovery.md:1), docs/recovery.md:23.",
        "docs/recovery.md:1: checked by eye",
        "docs/recovery.md:1, then inspected it.",
        "docs/recovery.md:1 and more investigation remains.",
        "docs/recovery.md:1 — first line",
        "docs/recovery.md:1 – description",
        "docs/recovery.md:1 - still needs human review",
    ] {
        journey(checked, None, &["docs/recovery.md"]);
    }
    journey(
        "docs/recovery.md:1,docs/research.md:82–91, one.rs:1",
        None,
        &["docs/recovery.md", "docs/research.md", "one.rs"],
    );
    journey(
        "one.rs:1,123.rs:1;docs/recovery.md:1",
        None,
        &["123.rs", "docs/recovery.md", "one.rs"],
    );
    journey("The README, by eye; version 1.2, 3.4.", None, &[]);
}

#[test]
fn malformed_citation_tokens_never_establish_a_valid_prefix() {
    for checked in [
        "docs/recovery.md:0",
        "docs/recovery.md:-1",
        "docs/recovery.md:+1",
        "docs/recovery.md:1,+2",
        "docs/recovery.md:1-+2",
        "docs/recovery.md:3-1",
        "docs/recovery.md:18446744073709551616",
        "docs/recovery.md:1-",
        "docs/recovery.md:1–",
        "docs/recovery.md:1 –",
        "docs/recovery.md:1—broken",
        "docs/recovery.md:1 — 2oops",
        "docs/recovery.md:1-2-3",
        "docs/recovery.md:1abc",
        "docs/recovery.md:1.2",
        "docs/recovery.md:1:2",
        "docs/recovery.md:1/2",
        "docs/recovery.md:1,,3",
        "docs/recovery.md:1, 2oops",
    ] {
        journey(checked, Some("invalid line range"), &[]);
    }
}

#[test]
fn unavailable_citation_files_stay_visible_as_evidence_gaps() {
    for (checked, reason) in [
        ("missing.rs:1", "isn't in the folder"),
        ("../outside.rs:1", "is outside the folder"),
        ("directory.rs:1", "is not a readable file"),
        ("empty.rs:1", "has 0 lines, not 1"),
    ] {
        journey(checked, Some(reason), &[]);
    }
    #[cfg(unix)]
    for (checked, reason) in [
        ("linked.rs:1", "is outside the folder"),
        ("unreadable.rs:1", "is not a readable file"),
    ] {
        journey(checked, Some(reason), &[]);
    }
}

#[test]
fn a_spaced_path_credits_only_the_whole_file_it_names() {
    // #718: `report.md` exists, `docs/design report.md` does not.
    journey(
        "docs/design report.md:1",
        Some("may be the end of a longer path that contains a space"),
        &[],
    );
    journey(
        "`docs/design report.md:1`",
        Some("docs/design report.md isn't in the folder"),
        &[],
    );
    journey("`docs/spaced name.md:2`", None, &["docs/spaced name.md"]);
    journey(
        "`docs/spaced name.md:3`",
        Some("docs/spaced name.md has 2 lines, not 3"),
        &[],
    );
    journey("`report.md:1`", None, &["report.md"]);
    journey(
        "Read report.md:1 and docs/recovery.md:2.",
        None,
        &["docs/recovery.md", "report.md"],
    );
    journey(
        "docs/recovery.md:1 report.md:1",
        None,
        &["docs/recovery.md", "report.md"],
    );
    journey("docs/design report.md:1-x", Some("may be the end"), &[]);
}
