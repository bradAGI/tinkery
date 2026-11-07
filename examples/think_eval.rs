//! Explicit-data evaluation of think. Replies are scripted by default; `--live` sends the supplied
//! words to Pi only with operator data, a written permission and a positive call limit, all counted
//! by one budget at the Pi boundary. Never saves a session or confirms a goal.
#[path = "support/capture.rs"]
mod capture;

use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, atomic::AtomicBool},
    time::{Duration, Instant},
};
use tinkery::think::{
    brain_dump::{
        BoardHost, BoardRequest, BrainDump, DecisionReview, Guess, decision_review_input,
    },
    drafting::{CallBudget, ModelHost, Request},
};
use tinkery::think_record::hash;

const USAGE: &str = "Supply --dataset FILE --output NEW_DIRECTORY --max-requests N. Replies are scripted. \
Add --baseline REPORT to compare with an earlier run of the same dataset. Add --live --model provider/model-id [--pi-command PATH] [--thinking LEVEL] to call Pi; \
there is no default dataset, model or limit.";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Dataset {
    provenance: Provenance,
    /// Who allowed these words to go to a provider, in their own terms. Required for `--live`.
    #[serde(default)]
    permission: Option<String>,
    steps: Vec<Step>,
    #[serde(default)]
    replies: VecDeque<Reply>,
    #[serde(default)]
    expected: std::collections::BTreeMap<String, Value>,
    /// Operator-written key terms per field, found regardless of wording: see `key_terms_check`.
    #[serde(default)]
    key_terms:
        std::collections::BTreeMap<String, std::collections::BTreeMap<String, Vec<Vec<String>>>>,
}

/// The fields each reading check observes, in report order.
const FIELDS: [(&str, &[&str]); 3] = [
    ("actor_outcome", &["who", "outcome"]),
    ("constraints", &["must", "must_not"]),
    (
        "unresolved_deferred",
        &["open", "questions", "notes", "deferred", "other_goals"],
    ),
];
#[derive(Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct Provenance {
    kind: InputKind,
    description: String,
}
#[derive(Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
enum InputKind {
    SyntheticTestData,
    OperatorSupplied,
}
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
enum Step {
    Send { text: String },
    Retry,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Reply {
    kind: Request,
    #[serde(flatten)]
    result: ReplyResult,
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum ReplyResult {
    Text(String),
    Failure(String),
}

struct Fake {
    budget: Arc<CallBudget>,
    replies: Mutex<VecDeque<Reply>>,
}
impl Fake {
    fn request(&self, kind: Request) -> Result<String, String> {
        // The same budget as Pi: charged before the scripted reply is read.
        let slot = self.budget.reserve(kind)?;
        let result = match self.replies.lock().unwrap().pop_front() {
            Some(reply) if reply.kind == kind => match reply.result {
                ReplyResult::Text(text) => Ok(text),
                ReplyResult::Failure(error) => Err(error),
            },
            Some(_) => Err("Fake reply kind does not match the next request.".into()),
            None => Err("No scripted fake reply remains.".into()),
        };
        self.budget.finish(slot, &result);
        result
    }
}
impl BoardHost for Fake {
    fn reshape(&self, request: BoardRequest, _: &AtomicBool) -> Result<Guess, String> {
        let raw = self.request(Request::Shaping)?;
        let mut changes = vec![];
        // Like ModelHost, readable invalid boards still get a review before decoding.
        let review = decision_review_input(&request, &raw).and_then(|_| {
            self.request(Request::DecisionReview)
                .ok()
                .and_then(|review| DecisionReview::decode(&review, &mut changes))
        });
        Guess::decode_reviewed(&raw, review.as_ref(), &request, &mut changes)
    }
}

/// Pi settings for a live run; absent for a scripted one.
#[derive(Debug)]
struct Live {
    pi_command: PathBuf,
    model: String,
    thinking: String,
}
#[derive(Debug)]
struct Options {
    dataset: PathBuf,
    output: PathBuf,
    cap: usize,
    live: Option<Live>,
    /// An earlier `report.json` of the same dataset to compare against.
    baseline: Option<PathBuf>,
}

fn options(args: impl Iterator<Item = String>) -> Result<Options, String> {
    let mut args = args;
    let mut live = false;
    let mut values = std::collections::BTreeMap::new();
    while let Some(arg) = args.next() {
        if arg == "--live" && !live {
            live = true;
            continue;
        }
        let known = [
            "--dataset",
            "--output",
            "--max-requests",
            "--model",
            "--pi-command",
            "--thinking",
            "--baseline",
        ];
        let value = args.next().ok_or(USAGE)?;
        if !known.contains(&arg.as_str()) || values.insert(arg, value).is_some() {
            return Err(USAGE.into());
        }
    }
    let mut take = |name: &str| values.remove(name);
    let (dataset, output) = (
        take("--dataset").ok_or(USAGE)?,
        take("--output").ok_or(USAGE)?,
    );
    let cap = take("--max-requests")
        .ok_or(USAGE)?
        .parse()
        .map_err(|_| USAGE)?;
    let baseline = take("--baseline").map(PathBuf::from);
    let live = if live {
        Some(Live {
            model: take("--model").ok_or(USAGE)?,
            pi_command: take("--pi-command").unwrap_or_else(|| "pi".into()).into(),
            thinking: take("--thinking").unwrap_or_else(|| "low".into()),
        })
    } else {
        None
    };
    if !values.is_empty() {
        return Err(format!("Pi options need --live. {USAGE}"));
    }
    Ok(Options {
        dataset: dataset.into(),
        output: output.into(),
        cap,
        live,
        baseline,
    })
}

fn validate(dataset: &Dataset, output: &Path, cap: usize, live: bool) -> Result<(), String> {
    if dataset.provenance.description.trim().is_empty() || dataset.steps.is_empty() {
        return Err("Supply provenance and at least one step; synthetic fixtures must say synthetic_test_data.".into());
    }
    if !matches!(dataset.steps.first(), Some(Step::Send { .. })) {
        return Err("Start the dataset with a send step.".into());
    }
    for step in &dataset.steps {
        if let Step::Send { text } = step
            && (text.trim().is_empty() || text.len() > 4096)
        {
            return Err(
                "Each send needs 1–4096 bytes of nonblank text; split larger inputs explicitly."
                    .into(),
            );
        }
    }
    if dataset
        .expected
        .keys()
        .any(|key| !["actor_outcome", "constraints", "unresolved_deferred"].contains(&key.as_str()))
    {
        return Err(
            "Expected checks may name actor_outcome, constraints or unresolved_deferred only."
                .into(),
        );
    }
    for (group, fields) in &dataset.key_terms {
        let Some((_, names)) = FIELDS.iter().find(|(name, _)| name == group) else {
            return Err(format!("Key terms may not name {group}."));
        };
        if fields.is_empty() {
            return Err(format!(
                "Key terms for {group} name no field; remove it or add one."
            ));
        }
        for (field, items) in fields {
            let mut seen = std::collections::BTreeSet::new();
            if !names.contains(&field.as_str())
                || items.is_empty()
                || items.iter().any(|item| {
                    item.is_empty()
                        || !seen.insert(item)
                        || item
                            .iter()
                            .any(|term| term.split('|').any(|word| word.trim().is_empty()))
                })
            {
                return Err(format!(
                    "Key terms {group}.{field} must name a known field and list distinct items of nonblank terms."
                ));
            }
        }
    }
    let parent = output.parent().and_then(|p| p.canonicalize().ok());
    if !output.is_absolute() || output.exists() {
        return Err("Output must be a new absolute directory with an existing parent.".into());
    }
    let Some(parent) = parent else {
        return Err("Output must be a new absolute directory with an existing parent.".into());
    };
    if !live {
        let temp = std::env::temp_dir()
            .canonicalize()
            .map_err(|e| e.to_string())?;
        if !parent.starts_with(temp) {
            return Err("A scripted run writes under the system temporary directory only.".into());
        }
        return Ok(());
    }
    if !matches!(dataset.provenance.kind, InputKind::OperatorSupplied) {
        return Err(
            "A live run sends operator_supplied words only; synthetic fixtures stay scripted."
                .into(),
        );
    }
    if dataset
        .permission
        .as_deref()
        .is_none_or(|p| p.trim().is_empty())
    {
        return Err("A live run needs the dataset's permission: who allowed these words to reach a provider.".into());
    }
    if !dataset.replies.is_empty() {
        return Err("A live run takes no scripted replies; remove them or drop --live.".into());
    }
    if cap == 0 {
        return Err("A live run needs a positive --max-requests; nothing was sent.".into());
    }
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let operator_data = std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join(".local/share"))
        .into_iter()
        .flat_map(|share| [share.join("seed-me"), share.join("tinkery")])
        .filter_map(|p| p.canonicalize().ok());
    if parent.starts_with(&repository) || operator_data.into_iter().any(|p| parent.starts_with(p)) {
        return Err(
            "Live output stays outside the repository and the operator's Seed Me and Tinkery data."
                .into(),
        );
    }
    Ok(())
}

fn settle(app: &mut BrainDump, limit: Duration) -> Result<(), String> {
    let deadline = Instant::now() + limit;
    while app.running() && Instant::now() < deadline {
        app.tick();
        std::thread::sleep(Duration::from_millis(1));
    }
    if app.running() {
        Err(format!(
            "The run did not settle within {} seconds.",
            limit.as_secs()
        ))
    } else {
        Ok(())
    }
}

fn originals_match(app: &BrainDump, supplied: &[&str]) -> bool {
    app.conversation()
        .sources()
        .iter()
        .map(|s| s.text.as_str())
        .eq(supplied.iter().copied())
}

fn run(options: &Options) -> Result<Value, String> {
    let (dataset_path, output, cap) = (&options.dataset, &options.output, options.cap);
    let input = std::fs::read_to_string(dataset_path)
        .map_err(|e| format!("Read the supplied dataset: {e}. {USAGE}"))?;
    let mut dataset: Dataset =
        serde_json::from_str(&input).map_err(|e| format!("Invalid dataset: {e}"))?;
    validate(&dataset, output, cap, options.live.is_some())?;
    // The words sent, apart from scripted replies, so a rerun with changed replies stays comparable.
    let words = hash(&serde_json::from_str::<Value>(&input).unwrap()["steps"].to_string());
    let baseline = match &options.baseline {
        Some(path) => Some(baseline(path, &words)?),
        None => None,
    };
    let budget = CallBudget::new(cap);
    let fake = Arc::new(Fake {
        budget: budget.clone(),
        replies: Mutex::new(std::mem::take(&mut dataset.replies)),
    });
    let (host, settle_limit): (Arc<dyn BoardHost>, _) = match &options.live {
        None => (fake.clone(), Duration::from_secs(5)),
        Some(live) => (
            Arc::new(
                ModelHost::new(
                    tinkery::pi::Harness::Pi,
                    live.pi_command.clone(),
                    live.model.clone(),
                    budget.clone(),
                )?
                .with_thinking(&live.thinking)?,
            ),
            // Two 90-second Pi calls per send, plus room to settle.
            Duration::from_secs(200),
        ),
    };
    let provider = options
        .live
        .as_ref()
        .map(|live| json!({"pi_command": live.pi_command, "origin": host.origin()}));
    std::fs::create_dir(output).map_err(|e| e.to_string())?;
    let mut app = BrainDump::with_host(host);
    capture::capture(&mut app, output, "00-entry");
    let mut supplied = vec![];
    let mut steps = vec![];
    for (index, step) in dataset.steps.iter().enumerate() {
        match step {
            Step::Send { text } => {
                app.paste(text);
                supplied.push(text.as_str());
            }
            Step::Retry => {
                if app.conversation().last_failure().is_none() {
                    return Err(
                        "A retry step requires a failed reading; use send for new words.".into(),
                    );
                }
            }
        }
        app.submit();
        settle(&mut app, settle_limit)?;
        let name = format!("{:02}-reading", index + 1);
        capture::capture(&mut app, output, &name);
        let accepted = originals_match(&app, &supplied);
        steps.push(json!({"capture": name, "notice": app.notice(),
            "input_accepted": accepted,
            "reading_applied": accepted && app.conversation().caught_up(),
            "failure": if accepted { app.conversation().last_failure() } else { Some(app.notice()) }}));
    }
    let exact = originals_match(&app, &supplied);
    let current = exact && app.conversation().caught_up();
    let board = app.conversation().board();
    let observed = json!({
        "actor_outcome": board.map(|b| json!({"who": b.parts.who, "outcome": b.parts.outcome})),
        "constraints": board.map(|b| json!({"must": b.parts.must, "must_not": b.parts.must_not})),
        "unresolved_deferred": board.map(|b| json!({"open": app.conversation().open_parts(),
            "questions": app.conversation().unresolved(), "notes": b.unresolved_notes,
            "deferred": b.deferred, "other_goals": b.other_goals})),
    });
    let mut checks = serde_json::Map::new();
    for name in ["actor_outcome", "constraints", "unresolved_deferred"] {
        let expected = dataset.expected.get(name);
        checks.insert(name.into(), json!({"status": match expected {
            Some(value) if board.is_some() && current => if *value == observed[name] { "pass" } else { "fail" },
            _ => "unknown",
        }, "basis": "Exact match to a scripted fixture; a live reading in other words fails it. Use key terms for live runs.",
            "expected": expected, "observed": observed[name]}));
    }
    checks.insert(
        "key_terms".into(),
        if dataset.key_terms.is_empty() || board.is_none() || !current {
            json!({"status": "unknown"})
        } else {
            key_terms_check(&dataset.key_terms, &observed)
        },
    );
    checks.insert(
        "originals_exact".into(),
        json!({"status": if exact { "pass" } else { "fail" }}),
    );
    for (name, reason) in [
        (
            "injection_handling",
            "Not judged automatically. Source preservation and decoder checks do not prove prompt-injection resistance.",
        ),
        (
            "exact_confirmation_handoff",
            "Not exercised: this runner never confirms or saves. Use the isolated goal_confirmation_terminal.py checks separately.",
        ),
    ] {
        checks.insert(name.into(), json!({"status": "unknown", "reason": reason}));
    }
    let attempts = budget.attempts();
    let (refused, calls): (Vec<_>, Vec<_>) = attempts.iter().partition(|a| a.outcome == "refused");
    let count = |kind| calls.iter().filter(|a| a.kind == kind).count();
    let live = options.live.is_some();
    let source = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned());
    let dirty = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| !o.stdout.is_empty());
    let report = json!({
        "execution": if live { "live" } else { "fake" },
        "input": {"path": dataset_path, "hash": hash(&input), "words_hash": words, "provenance": dataset.provenance,
            "permission": dataset.permission},
        "provider": provider,
        "revision": {"checkout_at_run": source, "working_tree_dirty": dirty,
            "compiled_library_revision": null,
            "limit": "Runtime Git metadata describes the checkout, not necessarily this executable. Compiled library revision is unknown; rebuild at the revision being evaluated.",
            "compiled_runner_hash": hash(include_str!("think_eval.rs")),
            "reading_prompt_file_hash": hash(include_str!("../src/think/brain_dump/prompts/reading.txt")),
            "review_prompt_file_hash": hash(include_str!("../src/think/brain_dump/prompts/decision_review.txt")),
            "prompts_executed": live},
        "requests": {"cap": budget.limit(), "used": calls.len(), "calls": calls,
            "refused_before_invocation": refused,
            "unused_replies": (!live).then(|| fake.replies.lock().unwrap().len()),
            "shaping": count(Request::Shaping), "decision_review": count(Request::DecisionReview)},
        "steps": steps, "reading_current": current, "automated_checks": checks,
        "human_baseline": {"recognition": null, "active_attention_seconds": null,
            "waiting_seconds": null, "corrections": null, "cost": null,
            "note": "Unknown until measured by a person. Fake timings and replies are not a human baseline."},
        "provider_cost": if live {
            json!({"amount": null, "currency": null, "real_calls": calls.len(),
                "note": "Charges are unknown here; read them from the provider account."})
        } else {
            json!({"amount": 0, "currency": "USD", "real_calls": 0})
        },
        "think_record": null, "session_confirmed": false,
    });
    let mut report = report;
    if let Some(baseline) = &baseline {
        report["comparison"] = compare(baseline, &report);
    }
    std::fs::write(
        output.join("report.json"),
        serde_json::to_string_pretty(&report).unwrap() + "\n",
    )
    .map_err(|e| e.to_string())?;
    Ok(report)
}

/// Read an earlier report, refusing one of different words before anything starts.
fn baseline(path: &Path, words: &str) -> Result<Value, String> {
    let read =
        std::fs::read_to_string(path).map_err(|e| format!("Read the baseline report: {e}"))?;
    let report: Value =
        serde_json::from_str(&read).map_err(|e| format!("Invalid baseline report: {e}"))?;
    if report["input"]["words_hash"].is_null() {
        return Err(
            "The baseline report predates this comparison; rerun the old model first.".into(),
        );
    }
    if report["input"]["words_hash"] != words {
        return Err("The baseline sent different words; compare runs of the same dataset.".into());
    }
    serde_json::from_value::<BaselineShape>(report.clone())
        .map_err(|e| format!("The baseline is not a think_eval report: {e}"))?;
    Ok(report)
}

/// The parts of a report a comparison reads; unknown fields are allowed.
#[derive(Deserialize)]
#[allow(dead_code)]
struct BaselineShape {
    execution: String,
    #[serde(deserialize_with = "present")]
    provider: Option<serde_json::Map<String, Value>>,
    revision: BaselineRevision,
    automated_checks: BaselineChecks,
    steps: Vec<BaselineStep>,
}
#[derive(Deserialize)]
#[allow(dead_code)]
struct BaselineRevision {
    #[serde(deserialize_with = "present")]
    checkout_at_run: Option<String>,
}
#[derive(Deserialize)]
#[allow(dead_code)]
struct BaselineChecks {
    actor_outcome: Observed,
    constraints: Observed,
    unresolved_deferred: Observed,
}
/// A reading is an object, or null when no reading applied.
#[derive(Deserialize)]
#[allow(dead_code)]
struct Observed {
    #[serde(deserialize_with = "present")]
    observed: Option<serde_json::Map<String, Value>>,
}
#[derive(Deserialize)]
#[allow(dead_code)]
struct BaselineStep {
    input_accepted: bool,
    reading_applied: bool,
    #[serde(deserialize_with = "present")]
    failure: Option<String>,
}
/// A nullable field whose key must still be present: serde treats a missing `Option` as null.
fn present<'de, D, T>(value: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::deserialize(value)
}

/// Mechanical: passes a field when every operator-written item is found, that is all of its
/// terms (`a|b` accepts either) start a word, ignoring case, in one observed entry of the
/// field. Wording, order and extra entries do not fail it; it cannot see negation or tell an
/// entry means the same, so unmatched entries are listed for the operator to read.
fn key_terms_check(
    key_terms: &std::collections::BTreeMap<
        String,
        std::collections::BTreeMap<String, Vec<Vec<String>>>,
    >,
    observed: &Value,
) -> Value {
    let mut fields = serde_json::Map::new();
    for (group, named) in key_terms {
        for (field, items) in named {
            let entries: Vec<String> = match &observed[group][field] {
                Value::Array(entries) => entries.iter().map(text).collect(),
                Value::Null => vec![],
                one => vec![text(one)],
            };
            let found = |item: &Vec<String>, entry: &str| {
                item.iter().all(|term| {
                    term.split('|')
                        .any(|word| starts_a_word(entry, &word.trim().to_lowercase()))
                })
            };
            let missing: Vec<_> = items
                .iter()
                .filter(|item| !entries.iter().any(|entry| found(item, entry)))
                .collect();
            let unmatched: Vec<_> = entries
                .iter()
                .filter(|entry| !items.iter().any(|item| found(item, entry)))
                .collect();
            fields.insert(
                format!("{group}.{field}"),
                json!({"status": if missing.is_empty() { "pass" } else { "fail" },
                    "missing": missing, "unmatched": unmatched}),
            );
        }
    }
    let passed = fields.values().all(|field| field["status"] == "pass");
    json!({"status": if passed { "pass" } else { "fail" },
        "basis": "Mechanical: operator-written key terms found regardless of wording. Terms cannot see negation or prove an entry means the same; read the unmatched entries.",
        "fields": fields})
}

/// Whether `term` appears in `entry` where a word starts, so "sync" finds "sync" and
/// "syncing" but not "async", and the stem "delet" finds "deleting".
fn starts_a_word(entry: &str, term: &str) -> bool {
    entry.match_indices(term).any(|(at, _)| {
        !entry[..at]
            .chars()
            .next_back()
            .is_some_and(char::is_alphanumeric)
    })
}

/// An observed entry as lowercase text.
fn text(value: &Value) -> String {
    value
        .as_str()
        .map_or_else(|| value.to_string(), str::to_owned)
        .to_lowercase()
}

/// What changed from the baseline: observed readings and each step's outcome, plus which model,
/// harness and revision each side ran. Equal readings are not a quality verdict.
fn compare(baseline: &Value, report: &Value) -> Value {
    let mut changed = vec![];
    for name in ["actor_outcome", "constraints", "unresolved_deferred"] {
        let (old, new) = (
            &baseline["automated_checks"][name]["observed"],
            &report["automated_checks"][name]["observed"],
        );
        if old != new {
            changed.push(json!({"check": name, "baseline": old, "now": new}));
        }
    }
    let empty = vec![];
    let (old, new) = (
        baseline["steps"].as_array().unwrap_or(&empty),
        report["steps"].as_array().unwrap_or(&empty),
    );
    for index in 0..old.len().max(new.len()) {
        let outcome = |steps: &[Value]| {
            steps.get(index).map(|s| {
                json!({"input_accepted": s["input_accepted"],
                "reading_applied": s["reading_applied"], "failure": s["failure"]})
            })
        };
        if outcome(old) != outcome(new) {
            changed.push(json!({"step": index + 1, "baseline": outcome(old), "now": outcome(new)}));
        }
    }
    let side = |r: &Value| {
        json!({"execution": r["execution"], "provider": r["provider"],
        "revision": r["revision"]["checkout_at_run"]})
    };
    json!({"baseline": side(baseline), "now": side(report), "changed": changed})
}

fn main() {
    let result = options(std::env::args().skip(1)).and_then(|options| run(&options));
    match result {
        Ok(report) => println!("{}", serde_json::to_string_pretty(&report).unwrap()),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    }
}

#[cfg(test)]
#[path = "support/think_eval_tests.rs"]
mod tests;
