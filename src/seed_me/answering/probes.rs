//! The fixed probes Shape puts to the operator itself, without a model call, before any seed:
//! a real sample of the input, then the pre-mortem. Each is asked once, never displaces a current
//! question, and is answered only in the operator's words; refuses a stale version.

use super::{Refusal, nodes, publish, read};
use serde_json::json;
use std::path::Path;

/// The probe the interview requires the operator to answer before the seed: its ledger id and wording.
pub const PROBE: (&str, &str) = (
    "probe-pre-mortem",
    "Imagine this shipped and turned out wrong. What is the most likely reason?",
);

/// The probe that asks for real input before any seed, so checks are drafted against what the
/// work will meet, not only invented fixtures (#740): its ledger id and wording.
pub const SAMPLE: (&str, &str) = (
    "probe-real-sample",
    "Paste one real example of what this will work on, or say where one lives. If there is none, type no sample and say why.",
);

/// With no current question, put the first probe never asked to the operator, from `expected`:
/// the real sample, then the pre-mortem. Each must be answered in the operator's words before any
/// seed. The pre-mortem says so plainly when findings were already shown, since the answer is then
/// not independent of them. Returns the new version, or `None` when both were asked or a question
/// is current.
pub fn probe(session: &Path, expected: u64) -> Result<Option<u64>, Refusal> {
    let mut ledger = read(session)?;
    if ledger.version != expected {
        return Err(Refusal::Stale);
    }
    if !ledger.state["current_question"].is_null() {
        return Ok(None);
    }
    let asked = |id: &str| nodes(&ledger.state).iter().any(|n| n["id"] == id);
    let (node, reason) = if !asked(SAMPLE.0) {
        let (id, question) = SAMPLE;
        let node = json!({
            "id": id, "kind": "decision", "status": "unresolved", "prerequisites": [],
            "evidence": [], "owner": "User",
            "gate": "A real example to check against, or your word that there is none",
            "label": "Probe: real sample", "question": question,
        });
        (node, "Shape asks the operator for a real sample.")
    } else if !asked(PROBE.0) {
        let (id, question) = PROBE;
        let question = if ledger.findings().is_empty() {
            question.to_owned()
        } else {
            format!("{question}\nNOT independent: asked after findings were shown.")
        };
        let node = json!({
            "id": id, "kind": "decision", "status": "unresolved", "prerequisites": [],
            "evidence": [], "owner": "User", "gate": "Your own answer to the pre-mortem, or skip it",
            "label": "Probe: pre-mortem", "question": question,
        });
        (node, "Shape puts the pre-mortem probe to the operator.")
    } else {
        return Ok(None);
    };
    let id = node["id"].clone();
    ledger.state["nodes"]
        .as_array_mut()
        .ok_or_else(|| Refusal::Refused("The session has no decisions list.".into()))?
        .push(node);
    ledger.state["current_question"] = id;
    publish(session, expected, reason, &ledger.state).map(Some)
}
