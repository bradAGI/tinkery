//! Result snapshots from API 1 status and each run's recorded PR. No handoff prose, branch or
//! title supplies a PR; missing identities never borrow a goal or old success. No writes or decisions.

use super::{
    evidence,
    links::{self, canonical_run_id},
};
use crate::sdlc::{self, DEADLINE, Goal};
use serde_json::Value;
use std::{
    path::Path,
    process::Command,
    sync::atomic::{AtomicBool, Ordering},
};

/// An absent receipt set cannot establish checks or gate obligations at this root.
pub(super) const MISSING_RECEIPTS: &str = "unknown: receipts not found here";

/// Where one condition stands. Decisions match on this; the words are only for display.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Check {
    Pass,
    PassRecheck,
    Fail,
    Stale,
    /// A GitHub check that hasn't finished.
    Pending,
    NotSuccessful,
    MatchingEvidenceMissing,
    AttestationEvidenceMissing,
    InspectExternally,
    NotActivated,
    RevisionUnknown,
    FailureRevisionDiffers,
    EvidenceUnavailable,
    NoneReported,
    ReceiptsMissing,
}

impl Check {
    /// The exact words shown and copied for this state.
    pub(super) fn word(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::PassRecheck => "pass (may need recheck)",
            Self::Fail => "fail",
            Self::Stale => "stale",
            Self::Pending => "pending",
            Self::NotSuccessful => "pending (not a successful check)",
            Self::MatchingEvidenceMissing => "pending (matching evidence missing)",
            Self::AttestationEvidenceMissing => "pending (attestation evidence missing)",
            Self::InspectExternally => "pending (inspect externally)",
            Self::NotActivated => "pending (not activated)",
            Self::RevisionUnknown => "pending (result revision unknown)",
            Self::FailureRevisionDiffers => "pending (failure revision unknown or different)",
            Self::EvidenceUnavailable => "pending (evidence unavailable)",
            Self::NoneReported => "unknown (none reported)",
            Self::ReceiptsMissing => MISSING_RECEIPTS,
        }
    }
}

impl std::fmt::Display for Check {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.word())
    }
}

/// One condition or observation, never an aggregate verdict.
#[derive(Clone, Debug)]
pub(super) struct Item {
    /// Exact source expectation or check name.
    pub(super) label: String,
    /// Where the condition stands, with any honest gap.
    pub(super) state: Check,
    /// Evidence owner, not an acceptance claim.
    pub(super) kind: String,
    /// Original evidence, available by scrolling and in the external handoff.
    pub(super) evidence: String,
}

/// One run/result relationship, frozen at a deliberate read.
#[derive(Clone, Debug)]
pub(super) struct ResultRow {
    /// Source-owned run instance, not its display slug.
    pub(super) id: String,
    /// Safe run locator inside the named checkout.
    pub(super) slug: String,
    /// Exact saved goal, only from the shared seed join.
    pub(super) goal: Goal,
    /// Canonical explicit PR URL; never inferred from a branch.
    pub(super) url: Option<String>,
    /// Current PR head from GitHub; absent means unknown.
    pub(super) head: Option<String>,
    /// GitHub explicitly reported a merged PR; never an acceptance claim.
    pub(super) merged: bool,
    /// Exact-head declaration, or an honest absence/read refusal.
    pub(super) preview: Result<super::preview::Spec, String>,
    /// Snapshot identity of the imported seed, not approval.
    pub(super) seed: String,
    /// Imported conditions and source observations, separately labelled.
    pub(super) items: Vec<Item>,
    /// No verification, decisions or human marks were reported at the read root.
    pub(super) receipts_missing: bool,
    /// Plain actions still owed, including human attestations.
    pub(super) gates: Vec<String>,
    /// Unresolved source problems, not concealed by other successes.
    pub(super) gaps: Vec<String>,
}

fn text<'a>(v: &'a Value, key: &str) -> Result<&'a str, String> {
    v[key]
        .as_str()
        .ok_or_else(|| format!("Run report missing {key}; read again after repairing the source."))
}

fn commit(v: &Value) -> Option<&str> {
    v.as_str()
        .filter(|s| s.len() == 40 && s.bytes().all(|b| b.is_ascii_hexdigit()))
}

fn gate_words(stage: u64, status: &str) -> String {
    let action = match stage {
        1 => "review the problem",
        2 => "review the specification",
        3 => "approve the plan",
        _ => "review the decision externally",
    };
    if status == "stale" && stage == 3 {
        "re-approve the plan".into()
    } else if status == "changes-requested" {
        format!("revise and {action}")
    } else {
        action.into()
    }
}

fn local(root: &Path, run: &Value, row: &mut ResultRow) -> Result<(), String> {
    let verification = run
        .get("verification")
        .ok_or("Verification report missing.")?;
    let gates = run["gates"].as_array().ok_or("Run report missing gates.")?;
    let checklist = run["checklist"]
        .as_array()
        .ok_or("Run report missing completion conditions.")?;
    let no_decisions = match run.get("decisions") {
        None => true,
        Some(value) => value
            .as_array()
            .ok_or("Run decisions unreadable.")?
            .is_empty(),
    };
    row.receipts_missing = verification.is_null()
        && no_decisions
        && checklist.iter().all(|item| item["mark"].is_null())
        && gates.iter().all(|gate| gate["status"] == "pending");
    if row.receipts_missing {
        row.gaps.push(format!("{MISSING_RECEIPTS}; name the checkout holding this run's receipts with --sdlc-root and read again. No other checkout was searched."));
    }
    let receipt_head = commit(&verification["candidate"]["head"]);
    let exact = receipt_head.is_some() && receipt_head == row.head.as_deref();
    let current = run["verification_status"] == "current" && verification["passed"] == true;
    let mut all_gates = Vec::new();
    for gate in gates {
        let status = text(gate, "status")?;
        if !matches!(
            status,
            "approved" | "pending" | "stale" | "changes-requested"
        ) {
            return Err("Unknown gate status.".into());
        }
        let stage = gate["stage"].as_u64().ok_or("Gate stage missing.")?;
        if row.receipts_missing {
            row.gaps.push(format!(
                "Gate source reported {status}: {}. Obligation {MISSING_RECEIPTS}.",
                gate_words(stage, status)
            ));
        } else if status != "approved" {
            all_gates.push(gate_words(stage, status));
        }
    }
    row.gates = all_gates;
    match run["verification_status"].as_str() {
        Some("failed") => row.gaps.push(
            "Verification failed; inspect the run log externally before reviewing again.".into(),
        ),
        Some("interrupted") => row.gaps.push(
            "Verification interrupted; inspect the log and finish verification externally.".into(),
        ),
        Some("running") => row
            .gaps
            .push("Verification is still running; no settled local pass is claimed.".into()),
        _ => {}
    }
    if run["checklist"].as_array().is_some_and(Vec::is_empty) {
        row.gaps
            .push("Completion evidence missing: no conditions reported by this run.".into());
    }
    for item in checklist {
        let automatic = item["automatic"].as_bool().ok_or("Check method missing.")?;
        let proof = text(item, "proof")?;
        if proof != if automatic { "automated" } else { "attested" } {
            return Err("Check method conflicts with proof.".into());
        }
        let label = text(item, "verify")?.to_owned();
        let status = text(item, "status")?;
        if !matches!(status, "passed" | "failed" | "pending" | "unknown") {
            return Err("Unknown completion condition status.".into());
        }
        let recheck = item["needs_recheck"]
            .as_bool()
            .ok_or("Check freshness missing.")?;
        let mark = item.get("mark").ok_or("Check evidence missing.")?;
        if !mark.is_null() && !mark.is_object() {
            return Err("Check evidence unreadable.".into());
        }
        let pending = match item.get("pending") {
            None => false,
            Some(v) => v.as_bool().ok_or("Provisional state unreadable.")?,
        };
        let automatic_evidence = automatic.then(|| evidence::automated(root, &row.slug, run, item));
        let old_pass = status == "passed" || (automatic && verification["result"] == "passed");
        let state = match status {
            _ if row.receipts_missing => Check::ReceiptsMissing,
            _ if pending => Check::NotActivated,
            "passed" if !automatic && mark["evidence"].as_str().is_none() => {
                Check::AttestationEvidenceMissing
            }
            _ if row.head.is_none() => Check::RevisionUnknown,
            _ if old_pass && (!exact || !current) => Check::Stale,
            "passed" if automatic_evidence.as_ref().is_some_and(Result::is_err) => {
                Check::EvidenceUnavailable
            }
            "passed" if !automatic && recheck => Check::PassRecheck,
            "passed" => Check::Pass,
            "failed" if exact => Check::Fail,
            "failed" => Check::FailureRevisionDiffers,
            "pending" | "unknown" => Check::MatchingEvidenceMissing,
            _ => return Err("Unknown completion condition status.".into()),
        };
        if !row.receipts_missing && !automatic && (!pending && status != "passed" || recheck) {
            row.gates.push(if recheck {
                format!("may need recheck: {label}")
            } else {
                label.clone()
            });
        }
        let evidence = if let Some(evidence) = automatic_evidence {
            match evidence {
                Ok(pointer) if old_pass && (!exact || !current) => {
                    format!("Earlier verification (history): {pointer}")
                }
                Ok(pointer) => pointer,
                Err(why) => format!("Evidence unresolved: {why}"),
            }
        } else {
            mark["evidence"]
                .as_str()
                .unwrap_or("No operator evidence recorded")
                .to_owned()
        };
        row.items.push(Item {
            label,
            state,
            kind: if automatic {
                "automated"
            } else {
                "human attestation"
            }
            .into(),
            evidence: if row.receipts_missing {
                format!("Source reported {status}; {evidence}")
            } else {
                evidence
            },
        });
    }
    for error in run["errors"].as_array().ok_or("Run errors missing.")? {
        row.gaps
            .push(error.as_str().ok_or("Run error unreadable.")?.to_owned());
    }
    Ok(())
}

/// GitHub owner and repo names are case-insensitive; anything else must match the record exactly.
fn same_pr(pr: &Value, row: &ResultRow) -> bool {
    matches!((pr["url"].as_str(), row.url.as_deref()), (Some(a), Some(b)) if a.eq_ignore_ascii_case(b))
}

fn remote(pr: &Value, row: &mut ResultRow) -> Result<(), String> {
    row.head = Some(
        commit(&pr["headRefOid"])
            .ok_or("PR revision unavailable.")?
            .to_owned(),
    );
    if !same_pr(pr, row) {
        return Err("PR identity disagrees with the run's record.".into());
    }
    for check in pr["statusCheckRollup"]
        .as_array()
        .ok_or("PR checks unavailable.")?
    {
        let (name, state) = match text(check, "__typename")? {
            "CheckRun" => (
                text(check, "name")?,
                if check["status"] == "COMPLETED" {
                    match text(check, "conclusion")? {
                        "SUCCESS" => Check::Pass,
                        "FAILURE" | "TIMED_OUT" | "CANCELLED" | "ACTION_REQUIRED"
                        | "STARTUP_FAILURE" => Check::Fail,
                        _ => Check::NotSuccessful,
                    }
                } else {
                    Check::Pending
                },
            ),
            "StatusContext" => (
                text(check, "context")?,
                match text(check, "state")? {
                    "SUCCESS" => Check::Pass,
                    "FAILURE" | "ERROR" => Check::Fail,
                    _ => Check::Pending,
                },
            ),
            _ => return Err("Unsupported GitHub check type.".into()),
        };
        row.items.push(Item {
            label: name.into(),
            state,
            kind: "GitHub check".into(),
            evidence: "GitHub PR-head check, not merge-commit evidence".into(),
        });
    }
    if pr["statusCheckRollup"]
        .as_array()
        .is_some_and(Vec::is_empty)
    {
        row.items.push(Item {
            label: "PR checks".into(),
            state: Check::NoneReported,
            kind: "GitHub".into(),
            evidence: "No PR-head checks reported".into(),
        });
    }
    row.items.push(Item {
        label: "Independent review".into(),
        state: Check::InspectExternally,
        kind: "review, not acceptance".into(),
        evidence: "No revision-bound independent review receipt is supplied by the run API".into(),
    });
    Ok(())
}

fn listed(pr: &Value, row: &mut ResultRow) -> Result<bool, String> {
    match pr["state"].as_str() {
        Some("OPEN" | "MERGED") => {
            row.merged = pr["state"] == "MERGED";
            Ok(true)
        }
        Some("CLOSED") => Ok(false),
        _ => Err("Unknown PR state.".into()),
    }
}

fn gh(root: &Path, url: &str, stop: &AtomicBool) -> Result<Value, String> {
    let mut command = Command::new("gh");
    command.args([
        "pr",
        "view",
        url,
        "--json",
        "url,headRefOid,state,statusCheckRollup",
    ]);
    let out = sdlc::execute(root, command, stop, DEADLINE)?;
    serde_json::from_str(&out).map_err(|_| "GitHub report unreadable.".into())
}

/// Read API 1 plus each run's recorded PR, serially; cancellation never publishes a late row.
pub(super) fn collect(root: &Path, stop: &AtomicBool) -> Result<Vec<ResultRow>, String> {
    if !root.join(sdlc::SCRIPT).is_file() {
        return Err("Named checkout has no tink-sdlc status reader.".into());
    }
    let root = root
        .canonicalize()
        .map_err(|_| "Named checkout unavailable.".to_owned())?;
    let value = sdlc::envelope(&sdlc::status(
        &root,
        &["status".into(), "--json".into()],
        stop,
        DEADLINE,
    )?)?;
    let mut rows = Vec::new();
    for summary in value["runs"].as_array().ok_or("Run list unavailable.")? {
        if stop.load(Ordering::SeqCst) {
            return Err("Stopped.".into());
        }
        let slug = text(summary, "slug")?;
        if slug.is_empty()
            || slug.len() > 80
            || !slug
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
        {
            return Err("Unsafe run locator; no run files read.".into());
        }
        let mut row = ResultRow {
            id: summary["run_id"].as_str().unwrap_or("").into(),
            slug: slug.into(),
            goal: Goal::Unknown,
            url: None,
            head: None,
            merged: false,
            preview: Err("Preview identity unknown; press r.".into()),
            seed: String::new(),
            items: Vec::new(),
            receipts_missing: false,
            gates: Vec::new(),
            gaps: Vec::new(),
        };
        if summary["verification_status"] == "invalid" || !canonical_run_id(&row.id) {
            row.gaps
                .push("Run identity unavailable; repair the run externally, then press r.".into());
            rows.push(row);
            continue;
        }
        let out = sdlc::status(
            &root,
            &["status".into(), slug.into(), "--json".into()],
            stop,
            DEADLINE,
        )?;
        let value = sdlc::envelope(&out)?;
        let run = &value["run"];
        if text(run, "run_id")? != row.id || text(run, "slug")? != slug {
            return Err("Run identity changed while reading.".into());
        }
        if !matches!(
            text(run, "verification_status")?,
            "blocked" | "running" | "not-run" | "current" | "stale" | "failed" | "interrupted"
        ) {
            return Err("Unknown run verification state.".into());
        }
        row.goal = sdlc::goal(run);
        row.seed = run["seed_contract"]["sha256"]
            .as_str()
            .unwrap_or("unknown seed")
            .into();
        match links::recorded(run) {
            Err(why) => row.gaps.push(why),
            Ok(url) => {
                row.url = Some(url.clone());
                match gh(&root, &url, stop) {
                    Ok(pr) => {
                        if !same_pr(&pr, &row) {
                            row.gaps.push("GitHub names a different PR than the run recorded (renamed or moved?); result revision unknown.".into());
                        } else {
                            if !listed(&pr, &mut row)? { continue; }
                            if let Err(why) = remote(&pr, &mut row) { row.head = None; row.items.clear(); row.gaps.push(why); }
                        }
                    }
                    Err(_) => row.gaps.push("Couldn't read GitHub. Check gh authentication/access, then press r; result revision unknown.".into()),
                }
            }
        }
        if let (Some(url), Some(head)) = (&row.url, &row.head) {
            row.preview = super::preview::declaration(&root, url, head, stop);
        }
        let supporting_checks = std::mem::take(&mut row.items);
        local(&root, run, &mut row)?;
        row.items.extend(supporting_checks);
        if sdlc::goal(run) != row.goal {
            return Err("Original goal changed while reading; press r.".into());
        }
        rows.push(row);
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_check_keeps_its_exact_shown_and_copied_words() {
        use super::Check::*;
        for (check, words) in [
            (Pass, "pass"),
            (PassRecheck, "pass (may need recheck)"),
            (Fail, "fail"),
            (Stale, "stale"),
            (Pending, "pending"),
            (NotSuccessful, "pending (not a successful check)"),
            (
                MatchingEvidenceMissing,
                "pending (matching evidence missing)",
            ),
            (
                AttestationEvidenceMissing,
                "pending (attestation evidence missing)",
            ),
            (InspectExternally, "pending (inspect externally)"),
            (NotActivated, "pending (not activated)"),
            (RevisionUnknown, "pending (result revision unknown)"),
            (
                FailureRevisionDiffers,
                "pending (failure revision unknown or different)",
            ),
            (EvidenceUnavailable, "pending (evidence unavailable)"),
            (NoneReported, "unknown (none reported)"),
            (ReceiptsMissing, super::MISSING_RECEIPTS),
        ] {
            assert_eq!(check.to_string(), words);
        }
    }

    use super::*;
    use serde_json::json;

    fn row() -> ResultRow {
        ResultRow {
            id: "00000000-0000-4000-8000-000000000114".into(),
            slug: "TEST-result".into(),
            goal: Goal::Unknown,
            url: Some("https://github.com/TEST/example/pull/1".into()),
            head: Some("a".repeat(40)),
            merged: false,
            preview: Err("No preview declared".into()),
            seed: "TEST".into(),
            items: vec![],
            receipts_missing: false,
            gates: vec![],
            gaps: vec![],
        }
    }

    fn run() -> Value {
        serde_json::from_str::<Value>(include_str!("../../tests/fixtures/taste-api1.json")).unwrap()
            ["detail"]["run"]
            .clone()
    }

    fn without_receipts() -> Value {
        let mut source = run();
        source["verification"] = Value::Null;
        source["verification_status"] = json!("blocked");
        source["decisions"] = json!([]);
        source["gates"][0]["status"] = json!("pending");
        for item in source["checklist"].as_array_mut().unwrap() {
            item["status"] = json!("pending");
            item["mark"] = Value::Null;
        }
        source
    }

    #[test]
    fn missing_receipts_are_unknown_never_owed_or_pending() {
        let mut target = row();
        target.goal = Goal::Matched("TEST unchanged goal".into());
        local(Path::new("/TEST/intake"), &without_receipts(), &mut target).unwrap();
        assert!(target.gates.is_empty());
        assert!(target.receipts_missing);
        assert!(
            target
                .items
                .iter()
                .all(|item| item.state == Check::ReceiptsMissing)
        );
        for color in [false, true] {
            let detail = super::super::view::detail(
                &target,
                None,
                None,
                96,
                crate::Palette::new(color),
                false,
            )
            .iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
            assert_eq!(detail.matches(MISSING_RECEIPTS).count(), 3, "{detail}");
            assert!(!detail.contains("waiting") && !detail.contains("No source gate owed"));
        }
        for action in ['a', 'c'] {
            let copy = super::super::handoff(&target, action).unwrap();
            assert!(copy.contains(&format!("Gates owed: {MISSING_RECEIPTS}")));
            assert!(!copy.contains("pending (matching evidence missing)"));
            assert!(copy.contains("TEST unchanged goal"));
        }
        let pause = super::super::next::suggest(&target);
        assert!(pause.lines.join(" ").contains("Decision: proceed"));
    }

    #[test]
    fn existing_receipts_keep_source_gate_and_check_derivation() {
        for evidence in ["verification", "decision", "mark", "gate"] {
            let mut source = without_receipts();
            match evidence {
                "verification" => source["verification"] = run()["verification"].clone(),
                "decision" => source["decisions"] = json!([{"stage": 3, "decision": "approved"}]),
                "mark" => source["checklist"][1]["mark"] = json!({"evidence": "TEST observation"}),
                "gate" => source["gates"][0]["status"] = json!("stale"),
                _ => unreachable!(),
            }
            let mut target = row();
            local(Path::new("/TEST"), &source, &mut target).unwrap();
            assert!(!target.receipts_missing, "{evidence}");
            assert!(!target.gates.is_empty(), "{evidence}");
        }
    }

    #[test]
    fn a_new_result_commit_stales_earlier_pass_claims_even_if_source_says_current() {
        let mut current = row();
        local(Path::new("/TEST"), &run(), &mut current).unwrap();
        assert_eq!(current.items[0].state, Check::Pass);
        let mut changed = row();
        changed.head = Some("b".repeat(40));
        local(Path::new("/TEST"), &run(), &mut changed).unwrap();
        assert_eq!(changed.items[0].state, Check::Stale);
        assert!(changed.items[0].evidence.contains("history"));
        assert!(changed.gates.contains(&"re-approve the plan".into()));
        assert!(changed.gates.contains(&"confirm no clinical claims".into()));
        assert!(!changed.items.iter().any(|i| i.state == Check::Pass));
    }

    #[test]
    fn absent_identity_is_unknown_not_stale_and_source_staleness_removes_green() {
        let mut unknown = row();
        unknown.head = None;
        local(Path::new("/TEST"), &run(), &mut unknown).unwrap();
        assert!(unknown.items[0].state.word().starts_with("pending"));
        let mut source = run();
        source["verification_status"] = json!("stale");
        source["verification"]["passed"] = json!(false);
        let mut old = row();
        local(Path::new("/TEST"), &source, &mut old).unwrap();
        assert_eq!(old.items[0].state, Check::Stale);
    }

    #[test]
    fn pending_failed_provisional_and_unknown_source_fields_stay_honest() {
        let mut source = run();
        source["checklist"][0]["status"] = json!("failed");
        source["verification"]["result"] = json!("failed");
        let mut failed = row();
        local(Path::new("/TEST"), &source, &mut failed).unwrap();
        assert_eq!(failed.items[0].state, Check::Fail);
        source["checklist"][0]["pending"] = json!(true);
        let mut provisional = row();
        local(Path::new("/TEST"), &source, &mut provisional).unwrap();
        assert!(provisional.items[0].state.word().starts_with("pending"));
        for head in [Some("a".repeat(40)), None] {
            let mut unknown = run();
            unknown["checklist"][0]["status"] = json!("finished");
            unknown["checklist"][0]["pending"] = json!(true);
            let mut target = row();
            target.head = head;
            assert!(local(Path::new("/TEST"), &unknown, &mut target).is_err());
        }
        for field in [
            "status",
            "verify",
            "automatic",
            "proof",
            "mark",
            "needs_recheck",
        ] {
            let mut broken = run();
            broken["checklist"][0]
                .as_object_mut()
                .unwrap()
                .remove(field);
            assert!(
                local(Path::new("/TEST"), &broken, &mut row()).is_err(),
                "{field}"
            );
        }
    }

    #[test]
    fn api1_passing_condition_carries_usable_evidence_into_tail_and_both_handoffs() {
        let mut target = row();
        target.goal = Goal::Matched(" TEST original goal  \nDone when: Allocation works.".into());
        local(Path::new("/TEST/checkout"), &run(), &mut target).unwrap();
        assert_eq!(target.items[0].state, Check::Pass);
        let pointer = "/TEST/checkout/runs/TEST-result/04-test/output/test-log.md";
        let tail = super::super::result_lines(&target).join("\n");
        assert!(
            tail.contains(pointer) && tail.contains("TEST-allocation-check.py"),
            "{tail}"
        );
        for action in ['a', 'c'] {
            let copy = super::super::handoff(&target, action).unwrap();
            assert!(copy.contains(pointer), "{copy}");
            assert!(copy.contains(r#"argv: ["python3","TEST-allocation-check.py"]"#));
            assert!(copy.contains(r#"cwd: ".""#));
            assert!(
                copy.contains("12b88a9685e1e46a893e9a3e59434d88c663774964346f41ccb1289aa3834d4b")
            );
            assert!(!copy.contains("TEST-checked-allocation-success"));
        }
    }

    #[test]
    fn source_cwd_spellings_keep_matching_passes_and_survive_both_copied_handoffs() {
        for (cwd, resolved) in [
            ("./", "/TEST"),
            ("./src", "/TEST/src"),
            ("src/", "/TEST/src"),
            ("./src//checks/./", "/TEST/src/checks"),
        ] {
            let mut source = run();
            source["checklist"][0]["check"]["cwd"] = json!(cwd);
            let mut target = row();
            target.goal = Goal::Matched("TEST unchanged goal".into());
            local(Path::new("/TEST"), &source, &mut target).unwrap();
            assert_eq!(
                target.items[0].state,
                Check::Pass,
                "{cwd}: {}",
                target.items[0].evidence
            );
            for action in ['a', 'c'] {
                let copy = super::super::handoff(&target, action).unwrap();
                let method = format!(
                    "cwd: {} (resolved {resolved})",
                    serde_json::to_string(cwd).unwrap()
                );
                assert!(copy.contains(&method), "{copy}");
            }
        }
    }

    #[test]
    fn current_matching_pass_without_safe_actionable_evidence_stays_pending() {
        for path in [
            json!(null),
            json!("/outside/log"),
            json!("runs/other/04-test/output/test-log.md"),
        ] {
            let mut source = run();
            source["log"]["path"] = path;
            let mut target = row();
            local(Path::new("/TEST"), &source, &mut target).unwrap();
            assert_eq!(target.items[0].state, Check::EvidenceUnavailable);
            assert!(target.items[0].evidence.contains("unresolved"));
        }
        let mut source = run();
        source["checklist"][0]
            .as_object_mut()
            .unwrap()
            .remove("check");
        let mut target = row();
        local(Path::new("/TEST"), &source, &mut target).unwrap();
        assert_eq!(target.items[0].state, Check::EvidenceUnavailable);
    }

    #[test]
    fn explicitly_mapped_merged_results_stay_listed_but_closed_results_do_not() {
        for (state, visible, merged) in [
            ("OPEN", true, false),
            ("MERGED", true, true),
            ("CLOSED", false, false),
        ] {
            let mut target = row();
            assert_eq!(
                listed(&json!({"state": state}), &mut target).unwrap(),
                visible
            );
            assert_eq!(target.merged, merged);
        }
        assert!(listed(&json!({"state": "ACCEPTED"}), &mut row()).is_err());
    }

    #[test]
    fn remote_checks_are_separate_and_neutral_is_not_pass() {
        let mut row = row();
        let pr = json!({"url":row.url,"headRefOid":"a".repeat(40),"statusCheckRollup":[
            {"__typename":"CheckRun","name":"TEST CI","status":"COMPLETED","conclusion":"NEUTRAL"}]});
        remote(&pr, &mut row).unwrap();
        assert!(row.items[0].state.word().starts_with("pending"));
        assert_eq!(row.items[1].label, "Independent review");
        assert!(row.items[1].state.word().starts_with("pending"));
        let mut absent = pr.clone();
        absent["statusCheckRollup"] = json!([]);
        let mut no_checks = row.clone();
        no_checks.items.clear();
        remote(&absent, &mut no_checks).unwrap();
        assert_eq!(no_checks.items[0].state, Check::NoneReported);
        let mut conflict = pr;
        conflict["url"] = json!("https://github.com/TEST/example/pull/2");
        assert!(remote(&conflict, &mut row).is_err());
    }
}
