//! Ledger rules and publication: preserve originals, refuse invalid states and stale transitions,
//! and derive reviews without changing answers. Seed confirmation binds bytes, decisions and checks;
//! ending verifies that binding. No operation opens or refreshes a browser viewer.

mod teach_back;
pub use teach_back::{
    TEACH_BACK_SKIPPED, TeachBackComparison, TeachBackRecord, completion_report, teach_back_report,
};
mod closing;
pub use closing::{CompletionReview, FailureCase, confirmation_summary};
mod answers;
pub use answers::{
    AnswerAction, DeferredPrerequisite, LEAVES_OPEN, can_revise, deferred_prerequisite, leaves_open,
};
mod checks;
mod unknowns;
pub use unknowns::{resolve_unknowns, resolved_unknown};
mod questions;
pub use questions::{next_question, queue_question, waiting_on};
#[cfg(test)]
pub(crate) mod fixtures;
mod seed;
mod storage;
pub(crate) use checks::{FULL, prose_headings};
pub use checks::{
    checks_block, document_headings, read_checks, readable_acceptance, validate_checks,
};
pub(crate) use checks::{commandless_check, goal_criteria, keeps_criterion, validate_goal_checks};
pub(crate) use seed::status_from_ledger;
pub use seed::{decisions_block, seed_summary, status, verify_handoff};
pub use storage::{
    PublishError, SaveState, confirm_as, confirm_displayed_as, confirm_seed,
    confirm_with_teach_back, end, initialize, publish, record_teach_back, repair_check, respond,
    revalidate, set_checks,
};
pub(crate) use storage::{initialize_for, save_draft};
mod python;
mod value;
pub use value::Value;

use python::text;
use std::{
    collections::{BTreeSet, VecDeque},
    path::Path,
};
use value::{array, int, object};

type Result<T> = std::result::Result<T, String>;
const STATE: &[&str] = &[
    "status",
    "draft",
    "goal",
    "origin",
    "current_question",
    "nodes",
];
const OPTIONAL: &[&str] = &["assumed", "operator"];
const HELPER: &[&str] = &["revision", "premise_version", "reviewed_version"];
const NODE: &[&str] = &[
    "id",
    "kind",
    "status",
    "prerequisites",
    "predicate",
    "evidence",
    "answer",
    "authority",
    "authority_source",
    "label",
    "question",
    "owner",
    "gate",
    "recommendation",
    "defer_reason",
    "revisit_condition",
    "reopen_reason",
    "contradicts",
    "claim",
    "supported_by",
    "rationale",
    "delegation",
    "resolved_by",
];
const EVIDENCE: &[&str] = &[
    "status",
    "answer",
    "evidence",
    "claim",
    "supported_by",
    "contradicts",
];
fn require(condition: bool, message: impl Into<String>) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}
fn subset(value: &Value, fields: &[&str]) -> bool {
    value
        .object()
        .is_some_and(|v| v.iter().all(|(k, _)| fields.contains(&k.as_str())))
}
fn exact(value: &Value, fields: &[&str]) -> bool {
    subset(value, fields) && value.fields().len() == fields.len()
}
fn one_of(value: &Value, choices: &[&str]) -> bool {
    value.string().is_some_and(|s| choices.contains(&s))
}
fn strings(values: &[String]) -> Value {
    Value::Array(values.iter().map(|s| Value::from(s.as_str())).collect())
}
fn ids(nodes: &Value) -> Vec<String> {
    nodes
        .items()
        .iter()
        .filter_map(|n| n["id"].string().map(str::to_owned))
        .collect()
}
fn find<'a>(nodes: &'a Value, id: &str) -> &'a Value {
    nodes
        .items()
        .iter()
        .find(|n| n["id"].is(id))
        .unwrap_or(&Value::Null)
}
fn operator(value: &Value) -> Value {
    value.get("operator").cloned().unwrap_or("human".into())
}

/// Refuse malformed or secret-bearing receipts; never execute a check or open an artifact.
pub fn validate_receipt(receipt: &Value) -> Result<()> {
    require(
        subset(receipt, &["checked", "at", "observed", "check", "artifact"])
            && ["checked", "at", "observed"].iter().all(|k| receipt.has(k))
            && receipt.fields().iter().all(|(_, v)| text(v)),
        "invalid receipt: checked, at, and observed are required; check and artifact are optional; all non-blank strings",
    )?;
    require(
        python::timestamp(receipt["at"].string().unwrap()),
        "invalid receipt time: an ISO 8601 timestamp with a UTC offset",
    )?;
    require(
        !receipt
            .fields()
            .iter()
            .any(|(_, v)| python::secret(v.string().unwrap())),
        "receipt appears to contain a secret: record where it lives, never its value",
    )
}

/// A fact's claim must name its scope; an inference also names limits and support.
pub fn validate_claim(node: &Value) -> Result<()> {
    let claim = &node["claim"];
    require(
        node["kind"].is("fact"),
        "claim is only allowed on a fact: a decision is settled by authority, not evidence",
    )?;
    require(
        subset(claim, &["type", "scope", "limits"])
            && one_of(&claim["type"], &["observation", "inference", "unknown"]),
        "invalid claim: type is observation, inference, or unknown, with scope and optional limits",
    )?;
    require(
        text(&claim["scope"]),
        "claim requires a scope: what was examined, or what the claim covers",
    )?;
    require(
        !claim.has("limits") || text(&claim["limits"]),
        "invalid claim limits",
    )?;
    if claim["type"].is("inference") {
        require(
            text(&claim["limits"]),
            "an inference must state its limits: what it does not establish",
        )?;
        require(
            node["supported_by"].truth(),
            "an inference must cite the observations that support it",
        )?;
    }
    Ok(())
}

/// Support names distinct other facts; inferences may cite observations only.
pub fn validate_support(node: &Value, nodes: &Value) -> Result<()> {
    let support = &node["supported_by"];
    require(
        support.array().is_some()
            && support.truth()
            && support.items().iter().all(text)
            && support
                .items()
                .iter()
                .filter_map(Value::string)
                .collect::<BTreeSet<_>>()
                .len()
                == support.items().len(),
        "invalid supported_by: distinct node IDs",
    )?;
    let claim = &node["claim"];
    require(
        node["kind"].is("decision") || claim["type"].is("inference"),
        concat!(
            "supported_by is only allowed on a decision or an inference: ",
            "an observation is checked directly and an unknown has no support"
        ),
    )?;
    for source in support.items() {
        let found = find(nodes, source.string().unwrap());
        require(
            found != &Value::Null && source != &node["id"],
            "supported_by must name another node",
        )?;
        require(
            found["kind"].is("fact") && !found["claim"]["type"].is("unknown"),
            "supported_by must name a fact that records evidence, not a decision or an unknown",
        )?;
        require(
            claim == &Value::Null || found["claim"]["type"].is("observation"),
            "an inference is supported by observations, not by other inferences",
        )?;
    }
    Ok(())
}

/// Only a decision may carry non-blank feasibility or intent rationale.
pub fn validate_rationale(node: &Value) -> Result<()> {
    let rationale = &node["rationale"];
    require(
        node["kind"].is("decision"),
        "rationale is only allowed on a decision",
    )?;
    require(
        subset(rationale, &["feasibility", "intent"])
            && rationale.truth()
            && rationale.fields().iter().all(|(_, v)| text(v)),
        concat!(
            "invalid rationale: feasibility (what the current architecture allows) and/or intent ",
            "(the user's outcome or threat model), non-blank"
        ),
    )
}

/// Copy editable fields, preserving node key order and removing helper-owned history/counters.
/// Missing fields, non-iterable nodes and malformed node objects refuse rather than crashing.
pub fn editable(ledger: &Value) -> Result<Value> {
    let nodes = &ledger["nodes"];
    let empty_iterable = (nodes.object().is_some() || nodes.string().is_some()) && !nodes.truth();
    require(
        STATE.iter().all(|k| ledger.has(k)) && (nodes.array().is_some() || empty_iterable),
        "malformed stored ledger",
    )?;
    let mut state = Value::Object(
        STATE
            .iter()
            .map(|k| ((*k).into(), ledger[k].clone()))
            .collect(),
    );
    state.set(
        "assumed",
        ledger.get("assumed").cloned().unwrap_or_else(array),
    );
    state.set("operator", operator(ledger));
    if let Value::Array(nodes) = &mut state["nodes"] {
        for node in nodes {
            require(node.object().is_some(), "malformed stored node")?;
            node.remove("history");
            for k in HELPER {
                node.remove(k);
            }
        }
    }
    Ok(state)
}

/// Validate in publication order and return ready node IDs; never settle or choose a node.
pub fn validate(state: &Value) -> Result<Vec<String>> {
    let mut allowed = STATE.to_vec();
    allowed.extend(OPTIONAL);
    require(
        subset(state, &allowed) && STATE.iter().all(|k| state.has(k)),
        "invalid session fields",
    )?;
    let operator = operator(state);
    require(
        one_of(&operator, &["human", "agent", "simulated"]),
        "invalid operator: human, agent or simulated",
    )?;
    let assumed = state.get("assumed").cloned().unwrap_or_else(array);
    require(
        assumed.array().is_some()
            && assumed
                .items()
                .iter()
                .all(|a| exact(a, &["text", "why"]) && text(&a["text"]) && text(&a["why"])),
        "invalid assumed: a list of {\"text\", \"why\"} entries with non-blank strings",
    )?;
    require(
        one_of(&state["status"], &["active", "stopped", "completed"]),
        "invalid session status",
    )?;
    let draft = &state["draft"];
    require(
        exact(draft, &["goal", "outcome", "options"]),
        "invalid draft",
    )?;
    require(
        draft["goal"].string().is_some() && draft["outcome"].string().is_some(),
        "invalid draft text",
    )?;
    require(
        draft["options"].array().is_some() && draft["options"].items().iter().all(text),
        "invalid draft options: each option must be a non-empty string such as 'Label — tradeoff'",
    )?;
    let nodes = &state["nodes"];
    require(nodes.array().is_some(), "nodes must be a list")?;
    let mut seen = BTreeSet::new();
    for node in nodes.items() {
        require(node.object().is_some(), "invalid node fields")?;
        let mut extra: Vec<_> = node
            .fields()
            .iter()
            .map(|(k, _)| k.as_str())
            .filter(|k| !NODE.contains(k))
            .collect();
        extra.sort();
        require(
            extra.is_empty(),
            format!(
                "invalid node fields: {} (helper-owned or unknown; start from `read --editable`)",
                extra.join(", ")
            ),
        )?;
        require(text(&node["id"]), "node requires an id")?;
        let id = node["id"].string().unwrap();
        require(
            seen.insert(id),
            format!("duplicate node id {id}: choose an id no other node uses"),
        )?;
        require(
            one_of(&node["kind"], &["fact", "decision"]),
            "invalid node kind",
        )?;
        require(
            one_of(
                &node["status"],
                &["unresolved", "settled", "deferred", "superseded"],
            ),
            "invalid node status",
        )?;
        let prerequisites = &node["prerequisites"];
        require(
            prerequisites.array().is_some() && prerequisites.items().iter().all(text),
            "invalid prerequisites",
        )?;
        require(
            prerequisites
                .items()
                .iter()
                .filter_map(Value::string)
                .collect::<BTreeSet<_>>()
                .len()
                == prerequisites.items().len(),
            "duplicate prerequisite",
        )?;
        require(
            matches!(
                node.get("predicate"),
                None | Some(Value::Null | Value::Bool(_))
            ),
            "predicate must be an observed boolean or unknown (null)",
        )?;
        let evidence = &node["evidence"];
        require(
            evidence.array().is_some()
                && evidence
                    .items()
                    .iter()
                    .all(|v| text(v) || v.object().is_some()),
            "invalid evidence",
        )?;
        for entry in evidence.items().iter().filter(|v| v.object().is_some()) {
            validate_receipt(entry)?;
            if teach_back::is_record(entry) {
                require(
                    node["id"] == state["origin"],
                    "Draft review and teach-back records belong only to the confirmed goal.",
                )?;
                require(
                    ["words", "mismatch", "reviewed", "skipped"]
                        .iter()
                        .any(|kind| entry["artifact"].is(&format!("tinkery:teach-back/{kind}")))
                        || entry["artifact"].is("tinkery:review/completed"),
                    "Unknown draft review or teach-back record kind.",
                )?;
            }
        }
        if node["kind"].is("decision") {
            require(
                text(&node["owner"]) && text(&node["gate"]),
                "decision requires owner and gate",
            )?;
        }
        if node["status"].is("settled") {
            require(text(&node["answer"]), "settled node requires answer")?;
            require(
                text(&node["authority_source"]),
                "settled node requires authority source",
            )?;
            let expected: &[&str] = if node["kind"].is("fact") {
                &["evidence"]
            } else if operator.is("simulated") {
                &["simulated"]
            } else {
                &["user", "delegated"]
            };
            require(
                one_of(&node["authority"], expected),
                concat!(
                    "authority does not match node kind or operator: ",
                    "a simulated operator can only settle decisions as 'simulated', a human session never as 'simulated'"
                ),
            )?;
            if one_of(&node["authority"], &["evidence", "delegated"]) {
                require(
                    evidence.truth(),
                    "evidence or delegated settlement requires evidence",
                )?;
            }
        } else {
            require(
                ["answer", "authority", "authority_source"]
                    .iter()
                    .all(|k| node[k] == Value::Null),
                "unsettled node cannot carry an active answer",
            )?;
        }
        if node.has("reopen_reason") {
            require(text(&node["reopen_reason"]), "invalid reopen reason")?;
        }
        if node["status"].is("deferred") {
            require(
                text(&node["defer_reason"]) && text(&node["revisit_condition"]),
                "deferral requires reason and revisit condition",
            )?;
        }
    }
    for node in nodes.items() {
        if node.has("delegation") {
            require(
                node["id"] == state["origin"]
                    && node["authority"].is("delegated")
                    && node["status"].is("settled")
                    && one_of(&operator, &["human", "agent"]),
                "delegation is only allowed on a delegated goal origin in a human or agent session",
            )?;
            let delegation = &node["delegation"];
            let named = [
                "agent",
                "authorizing_operator",
                "authorization_source",
                "door",
                "blast_radius",
            ];
            require(
                (exact(delegation, &["agent"]) || exact(delegation, &named))
                    && delegation.fields().iter().all(|(_, v)| text(v)),
                "delegated origin requires the agent, plus all named fields when any is given",
            )?;
            require(
                !delegation.has("door")
                    || (delegation["door"].is("two-way") && delegation["blast_radius"].is("small")),
                "delegated origin requires a two-way door and small blast radius",
            )?;
        }
        // #432: a retry must hear which node broke an evidence rule, not just the rule.
        let named = |e: String| format!("node {}: {e}", node["id"].string().unwrap_or_default());
        if node.has("claim") {
            validate_claim(node).map_err(named)?;
        }
        if node.has("resolved_by") {
            unknowns::validate_resolution(node, nodes)?;
        }
        if node.has("supported_by") {
            validate_support(node, nodes).map_err(named)?;
        }
        if node.has("rationale") {
            validate_rationale(node)?;
        }
    }
    for node in nodes.items() {
        if node.has("contradicts") {
            let target = &node["contradicts"];
            require(
                node["kind"].is("fact")
                    && text(target)
                    && target != &node["id"]
                    && seen.contains(target.string().unwrap_or("")),
                "contradicts must name another node and is only allowed on a fact",
            )?;
        }
    }
    let mut visiting = BTreeSet::new();
    let mut visited = BTreeSet::new();
    for id in ids(nodes) {
        visit(nodes, &id, &mut visiting, &mut visited)?;
    }
    for node in nodes.items().iter().filter(|n| n["status"].is("settled")) {
        require(
            predicate(node),
            "settled node has inactive or unknown predicate",
        )?;
        require(
            parents_settled(nodes, node),
            "settled node has unsettled prerequisites",
        )?;
    }
    let origin = &state["origin"];
    if origin == &Value::Null {
        require(
            state["goal"] == Value::Null && !nodes.truth(),
            "unconfirmed draft cannot contain ledger nodes",
        )?;
    } else {
        require(
            text(origin) && seen.contains(origin.string().unwrap_or("")) && text(&state["goal"]),
            "invalid confirmed goal",
        )?;
        let root = find(nodes, origin.string().unwrap());
        let authority: &[&str] = if operator.is("simulated") {
            &["simulated"]
        } else {
            &["user", "delegated"]
        };
        require(
            root["kind"].is("decision")
                && root["status"].is("settled")
                && one_of(&root["authority"], authority)
                && root["answer"] == state["goal"],
            "origin requires an explicitly confirmed goal",
        )?;
        require(
            !root["authority"].is("delegated") || root.has("delegation"),
            "delegated origin requires structured delegation",
        )?;
    }
    let ready: Vec<_> = nodes
        .items()
        .iter()
        .filter(|n| n["status"].is("unresolved") && predicate(n) && parents_settled(nodes, n))
        .map(|n| n["id"].string().unwrap().to_owned())
        .collect();
    let current = &state["current_question"];
    require(
        current == &Value::Null
            || (current
                .string()
                .is_some_and(|c| ready.iter().any(|r| r == c))
                && find(nodes, current.string().unwrap_or(""))["kind"].is("decision")),
        "current question must be a ready decision",
    )?;
    if !state["status"].is("active") {
        require(
            current == &Value::Null,
            "ended session cannot have a current question",
        )?;
    }
    if state["status"].is("completed") {
        require(
            origin != &Value::Null && nodes.items().iter().all(|n| !n["status"].is("unresolved")),
            "cannot complete with unresolved concerns",
        )?;
    }
    Ok(ready)
}
fn predicate(node: &Value) -> bool {
    matches!(node.get("predicate"), None | Some(Value::Bool(true)))
}
fn parents_settled(nodes: &Value, node: &Value) -> bool {
    waiting_on(nodes, node["id"].string().unwrap_or("")).is_empty()
}
fn visit(
    nodes: &Value,
    id: &str,
    visiting: &mut BTreeSet<String>,
    visited: &mut BTreeSet<String>,
) -> Result<()> {
    let node = find(nodes, id);
    require(node != &Value::Null, "missing prerequisite")?;
    require(!visiting.contains(id), "dependency cycle")?;
    if visited.contains(id) {
        return Ok(());
    }
    visiting.insert(id.into());
    for parent in node["prerequisites"].items() {
        visit(nodes, parent.string().unwrap(), visiting, visited)?;
    }
    visiting.remove(id);
    visited.insert(id.into());
    Ok(())
}

/// Read exact ledger bytes from an opened regular file without waiting for a FIFO writer.
/// Symlinks to regular files remain readable; the bytes are never normalized or rewritten.
pub(super) fn read_bytes(directory: &Path) -> std::io::Result<Vec<u8>> {
    super::read_regular(&directory.join("ledger.json"), "ledger")
}

/// Read today's regular ledger without rewriting it. Unknown schemas, inconsistent frontiers and
/// malformed helper history refuse; duplicate JSON keys keep their first position and last value.
pub fn load(directory: &Path) -> Result<Value> {
    super::migration::refuse_old_root(directory)?;
    let input = String::from_utf8(read_bytes(directory).map_err(|e| e.to_string())?)
        .map_err(|_| "stream did not contain valid UTF-8".to_owned())?;
    let input = input.replace("\r\n", "\n").replace('\r', "\n");
    let ledger = Value::parse(&input)?;
    validate_stored(&ledger)?;
    Ok(ledger)
}
fn validate_stored(ledger: &Value) -> Result<()> {
    require(
        ledger.object().is_some() && matches!(&ledger["schema_version"], Value::Int(v) if v == "1"),
        "unsupported ledger schema",
    )?;
    let mut fields = STATE.to_vec();
    fields.extend(OPTIONAL);
    fields.extend([
        "schema_version",
        "session_id",
        "version",
        "revision",
        "created_at",
        "updated_at",
        "frontier",
    ]);
    require(exact(ledger, &fields), "invalid stored ledger fields")?;
    require(text(&ledger["session_id"]), "missing session identity")?;
    for field in ["version", "revision"] {
        require(ledger[field].natural(), format!("invalid {field}"))?;
    }
    let nodes = &ledger["nodes"];
    let other_iterable = nodes.object().is_some() || nodes.string().is_some();
    require(!other_iterable || !nodes.truth(), "missing node history")?;
    require(
        nodes.array().is_some() || other_iterable,
        "malformed stored nodes",
    )?;
    for node in ledger["nodes"].items() {
        require(
            node.object().is_some() && node["history"].array().is_some(),
            "missing node history",
        )?;
        require(
            node["revision"].natural() && node["revision"].cmp_int(&ledger["revision"]).is_le(),
            "invalid node revision",
        )?;
        for field in &HELPER[1..] {
            require(
                !node.has(field)
                    || (node[field].natural()
                        && node[field]
                            .cmp_int(&ledger["version"].twice().add(1))
                            .is_le()),
                format!("invalid node {field}"),
            )?;
        }
        for entry in node["history"].items() {
            require(
                exact(entry, &["state", "superseded_at", "reason"])
                    && text(&entry["reason"])
                    && text(&entry["superseded_at"]),
                "invalid history entry",
            )?;
            let mut allowed = NODE.to_vec();
            allowed.extend(HELPER);
            require(
                subset(&entry["state"], &allowed)
                    && entry["state"]["id"] == node["id"]
                    && entry["state"]["kind"] == node["kind"],
                "invalid historical node",
            )?;
        }
    }
    require(
        strings(&validate(&editable(ledger)?)?) == ledger["frontier"],
        "stored frontier is inconsistent",
    )
}

/// All workflow descendants, excluding the starting IDs; support links are not workflow edges.
pub fn descendants(nodes: &Value, starts: &[String]) -> BTreeSet<String> {
    let mut reached: BTreeSet<_> = starts.iter().cloned().collect();
    loop {
        let added: Vec<_> = nodes
            .items()
            .iter()
            .filter(|n| {
                n["prerequisites"]
                    .items()
                    .iter()
                    .any(|p| reached.contains(p.string().unwrap_or("")))
            })
            .filter_map(|n| n["id"].string().map(str::to_owned))
            .collect();
        let before = reached.len();
        reached.extend(added);
        if reached.len() == before {
            return reached
                .difference(&starts.iter().cloned().collect())
                .cloned()
                .collect();
        }
    }
}

/// Legacy classification and interview receipts are not new evidence; support order has no
/// evidential meaning. Teach-back never changes a goal's premises.
pub fn evidence_differs(before: &Value, after: &Value) -> bool {
    EVIDENCE
        .iter()
        .filter(|f| !(**f == "claim" && before[f] == Value::Null))
        .any(|f| {
            let view = |node: &Value| {
                let mut value = if *f == "evidence" {
                    teach_back::evidence(node)
                } else {
                    node[f].clone()
                };
                if *f == "supported_by"
                    && let Value::Array(v) = &mut value
                {
                    v.sort_by(|a, b| a.string().cmp(&b.string()));
                }
                value
            };
            view(before) != view(after)
        })
}

/// Evidence dependencies nearest first, visiting each named existing source once.
pub fn support_chain(nodes: &Value, id: &str) -> Vec<String> {
    let mut seen = vec![];
    let mut queue: VecDeque<_> = find(nodes, id)["supported_by"]
        .items()
        .iter()
        .filter_map(Value::string)
        .map(str::to_owned)
        .collect();
    while let Some(source) = queue.pop_front() {
        if !seen.contains(&source) && find(nodes, &source) != &Value::Null {
            seen.push(source.clone());
            queue.extend(
                find(nodes, &source)["supported_by"]
                    .items()
                    .iter()
                    .filter_map(Value::string)
                    .map(str::to_owned),
            );
        }
    }
    seen
}

/// Every settled contradicting fact, grouped by target in first-seen order.
pub fn contradictions(nodes: &Value) -> Value {
    let mut found = object(&[]);
    for fact in nodes
        .items()
        .iter()
        .filter(|n| n["status"].is("settled") && n["contradicts"].truth())
    {
        if let Some(target) = fact["contradicts"].string() {
            found.default(target, array());
            found[target].list_mut().push(fact.clone());
        }
    }
    found
}
fn counter(node: &Value, field: &str, default: u64) -> Value {
    node.get(field).cloned().unwrap_or_else(|| int(default))
}
fn flag(flags: &mut Value, id: &str, source: &str, why: &str) {
    flags.default(id, array());
    flags[id]
        .list_mut()
        .push(object(&[("because", source.into()), ("why", why.into())]));
}

/// Derive review requests against each node's own last review; never reopen or erase answers.
/// Input nodes are validated ledger nodes, with nonnegative helper counters.
pub fn review_flags(nodes: &Value) -> Value {
    let contradicted = contradictions(nodes);
    let mut flags = object(&[]);
    for node in nodes.items().iter().filter(|n| n["status"].is("settled")) {
        let id = node["id"].string().unwrap_or("");
        let reviewed = counter(node, "reviewed_version", 0);
        for source in support_chain(nodes, id) {
            let found = find(nodes, &source);
            let moved = counter(found, "premise_version", 1);
            let fact_moved = contradicted[&source]
                .items()
                .iter()
                .map(|f| counter(f, "premise_version", 1))
                .max_by(Value::cmp_int)
                .unwrap_or_else(|| int(0));
            let why = if moved.cmp_int(&reviewed).is_gt() {
                if found["status"].is("settled") {
                    "changed"
                } else {
                    "withdrawn"
                }
            } else if found["status"].is("settled")
                && moved.cmp_int(&fact_moved).is_le()
                && fact_moved.cmp_int(&reviewed).is_gt()
            {
                "contradicted"
            } else {
                continue;
            };
            flag(&mut flags, id, &source, why);
        }
        for fact in contradicted[id].items() {
            let moved = counter(fact, "premise_version", 1);
            if counter(node, "premise_version", 1).cmp_int(&moved).is_le()
                && moved.cmp_int(&reviewed).is_ge()
            {
                flag(
                    &mut flags,
                    id,
                    fact["id"].string().unwrap_or(""),
                    "contradicting",
                );
            }
        }
    }
    flags
}

fn validate_revalidation(revalidated: &Value) -> Result<()> {
    require(
        revalidated.object().is_some(),
        "invalid revalidation targets",
    )?;
    require(
        revalidated.fields().iter().all(|(_, v)| text(v)),
        "revalidation requires a justification",
    )
}

/// Propose the next ledger with a caller-supplied timestamp; never write it.
/// Refuse stale versions, attribution changes, deletions and unreviewed dependent settlements.
pub fn transition(
    ledger: &Value,
    proposed: &Value,
    expected_version: &Value,
    reason: &Value,
    revalidated: Option<&Value>,
    timestamp: &str,
) -> Result<Value> {
    let mut state = proposed.clone();
    state.default("assumed", array());
    state.default("operator", operator(ledger));
    unknowns::reopen_changed_resolutions(ledger, &mut state);
    let ready = validate(&state)?;
    unknowns::validate_new_resolutions(ledger, &state)?;
    require(
        state["operator"] == operator(ledger),
        "operator cannot change in a session",
    )?;
    let old_state = editable(ledger)?;
    require(expected_version.natural(), "invalid expected version")?;
    require(
        ledger["version"].natural() && ledger["revision"].natural(),
        "malformed stored counters",
    )?;
    if state == old_state
        && expected_version.cmp_int(&ledger["version"]).is_le()
        && !(revalidated.is_some_and(Value::truth) && expected_version == &ledger["version"])
    {
        return Ok(ledger.clone());
    }
    require(
        expected_version == &ledger["version"],
        "stale publication version",
    )?;
    require(ledger["status"].is("active"), "ended session is read-only")?;
    require(text(reason), "update requires a reason")?;
    if ledger["origin"] != Value::Null {
        require(
            state["origin"] == ledger["origin"] && state["goal"] == ledger["goal"],
            "confirmed origin and goal cannot be replaced in this session",
        )?;
    }
    let old = &old_state["nodes"];
    let new = &state["nodes"];
    if let Some(origin) = ledger["origin"].string() {
        let previous = find(old, origin);
        let proposed = find(new, origin);
        if previous["authority"].is("delegated") || proposed["authority"].is("delegated") {
            require(
                ["authority", "authority_source", "delegation"]
                    .iter()
                    .all(|k| previous[k] == proposed[k]),
                "confirmed origin attribution cannot be replaced; start a new session",
            )?;
        }
    }
    require(
        ids(old).iter().all(|id| find(new, id) != &Value::Null),
        "preserve nodes; supersede instead of deleting",
    )?;
    for node in old.items() {
        let next = find(new, node["id"].string().unwrap());
        require(node["kind"] == next["kind"], "node kind is immutable")?;
        let records = |n: &Value| {
            n["evidence"]
                .items()
                .iter()
                .filter(|r| teach_back::is_record(r))
                .cloned()
                .collect::<Vec<_>>()
        };
        require(
            records(next).starts_with(&records(node)),
            "Preserve teach-back words, mismatches and skips unchanged.",
        )?;
        if node["status"].is("settled") && next["status"].is("unresolved") {
            require(
                text(&next["reopen_reason"]),
                "reopened node requires a reopen reason",
            )?;
        }
    }
    let premise = &[
        "status",
        "answer",
        "authority",
        "authority_source",
        "evidence",
        "prerequisites",
        "predicate",
        "question",
        "owner",
        "gate",
        "claim",
        "supported_by",
    ];
    let changed: Vec<_> = old
        .items()
        .iter()
        .filter(|n| {
            n["status"].is("settled")
                && premise.iter().any(|f| {
                    !(**f == *"claim" && n[f] == Value::Null)
                        && if *f == "evidence" {
                            teach_back::evidence(n)
                                != teach_back::evidence(find(new, n["id"].string().unwrap()))
                        } else {
                            n[f] != find(new, n["id"].string().unwrap())[f]
                        }
                })
        })
        .map(|n| n["id"].string().unwrap().to_owned())
        .collect();
    let mut affected = BTreeSet::new();
    for id in &changed {
        affected.extend(descendants(old, std::slice::from_ref(id)));
        affected.extend(descendants(new, std::slice::from_ref(id)));
    }
    let revalidated = revalidated
        .filter(|v| **v != Value::Null)
        .cloned()
        .unwrap_or_else(|| object(&[]));
    validate_revalidation(&revalidated)?;
    for id in &affected {
        let next = find(new, id);
        if next["status"].is("settled") {
            require(
                text(&revalidated[id]) && next["evidence"].truth(),
                format!("affected settlement requires explicit revalidation with evidence: {id}"),
            )?;
        }
    }
    let revision = ledger["revision"].add(u8::from(
        !changed.is_empty() || state["assumed"] != old_state["assumed"],
    ));
    let version = ledger["version"].add(1);
    let mut result = ledger.clone();
    for (k, v) in state.fields() {
        result.set(k, v.clone());
    }
    result.set("version", version.clone());
    result.set("revision", revision.clone());
    result.set("updated_at", timestamp.into());
    result.set("frontier", strings(&ready));
    let mut added = object(&[]);
    let mut unreviewed = object(&[]);
    for node in result["nodes"].list_mut() {
        let id = node["id"].string().unwrap().to_owned();
        let previous = find(&ledger["nodes"], &id);
        let exists = previous != &Value::Null;
        let mut history = if exists {
            previous["history"].clone()
        } else {
            array()
        };
        require(history.array().is_some(), "malformed stored history")?;
        let material = exists && (find(old, &id) != node || revalidated.has(&id));
        if material {
            let mut prior = previous.clone();
            prior.remove("history");
            history.list_mut().push(object(&[
                ("state", prior),
                ("superseded_at", timestamp.into()),
                ("reason", revalidated.get(&id).unwrap_or(reason).clone()),
            ]));
        }
        node.set("history", history);
        node.set(
            "revision",
            if !exists || material {
                revision.clone()
            } else {
                previous["revision"].clone()
            },
        );
        let moved = !exists || evidence_differs(find(old, &id), node);
        let first_settled =
            node["status"].is("settled") && (!exists || !previous["status"].is("settled"));
        let chain = support_chain(new, &id);
        let sources: Vec<_> = if !node["status"].is("settled") {
            vec![]
        } else if first_settled {
            chain
        } else {
            chain
                .into_iter()
                .filter(|x| !support_chain(old, &id).contains(x))
                .collect()
        };
        let missing: Vec<_> = sources
            .iter()
            .filter(|x| !find(new, x)["status"].is("settled"))
            .cloned()
            .collect();
        added.set(&id, strings(&sources));
        require(
            missing.is_empty(),
            format!(
                "cannot settle on evidence that is not recorded: {}",
                missing.join(", ")
            ),
        )?;
        let position = version.twice();
        if moved {
            node.set("premise_version", position.clone());
        } else if previous.has("premise_version") {
            node.set("premise_version", previous["premise_version"].clone());
        }
        let before_review = if !exists || first_settled {
            position.clone()
        } else {
            counter(previous, "reviewed_version", 0)
        };
        node.set(
            "reviewed_version",
            if revalidated.has(&id) {
                position.add(1)
            } else {
                before_review.clone()
            },
        );
        unreviewed.set(&id, before_review);
    }
    let mut before = result["nodes"].clone();
    for node in before.list_mut() {
        node.set(
            "reviewed_version",
            unreviewed[node["id"].string().unwrap_or("")].clone(),
        );
    }
    let flags = review_flags(&before);
    require(
        revalidated
            .fields()
            .iter()
            .all(|(k, _)| affected.contains(k) || flags.has(k)),
        "invalid revalidation targets",
    )?;
    let after = review_flags(&result["nodes"]);
    for (_, sources) in added.fields() {
        let clashing: Vec<_> = sources
            .items()
            .iter()
            .filter_map(Value::string)
            .filter(|x| {
                after[x]
                    .items()
                    .iter()
                    .any(|f| f["why"].is("contradicting"))
            })
            .collect();
        require(
            clashing.is_empty(),
            format!(
                "cannot rely on evidence that is contradicted and not revisited: {}",
                clashing.join(", ")
            ),
        )?;
    }
    if state["status"].is("completed") {
        let mut flagged: Vec<_> = after.fields().iter().map(|(k, _)| k.as_str()).collect();
        flagged.sort();
        require(
            flagged.is_empty(),
            format!(
                concat!(
                    "cannot complete: evidence behind these settled nodes changed, was withdrawn, or was contradicted ",
                    "and they were not reviewed, revised or resolved (revalidate each with a reason; answers stay as they are): {}"
                ),
                flagged.join(", ")
            ),
        )?;
    }
    Ok(result)
}
