//! Seed bytes, decisions and confirmation receipts share one verifier. Reads never upgrade old receipts.

use super::{
    Result, Value, checks, editable, load,
    python::{text, whitespace},
    require, review_flags,
    value::{int, object},
};
use regex::Regex;
use sha2::{Digest, Sha256};
use std::path::Path;

pub(super) const FILE: &str = "seed-contract.md";
pub(super) const CONFIRMATION: &str = "seed-confirmation.json";
pub(super) const DRAFT: &str = "status: draft";
pub(super) const CONFIRMED: &str = "status: confirmed for intake";
pub(super) const SIMULATED: &str = "status: simulated";

pub(super) fn sha256(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}
pub(super) fn split_terminator(line: &str) -> (&str, &str) {
    let body = if let Some(body) = line.strip_suffix('\n') {
        body.strip_suffix('\r').unwrap_or(body)
    } else {
        line
    };
    (body, &line[body.len()..])
}
pub(super) fn seed_lines(directory: &Path) -> Result<Vec<String>> {
    let path = directory.join(FILE);
    require(
        path.is_file(),
        format!(
            "no seed to use: {} is missing; save the seed there first",
            path.display()
        ),
    )?;
    Ok(super::super::read_seed_text(&path)
        .map_err(|e| e.to_string())?
        .split_inclusive('\n')
        .map(str::to_owned)
        .collect())
}
fn revision(line: &str) -> Option<&str> {
    let rest = line
        .strip_prefix("revision:")?
        .trim_start_matches([' ', '\t']);
    rest.split(whitespace).next().filter(|s| !s.is_empty())
}
pub(super) fn repr(value: &str) -> String {
    let quote = if value.contains('\'') && !value.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = quote.to_string();
    for c in value.chars() {
        match c {
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\\' => out.push_str("\\\\"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c if !super::python::printable(c) => {
                let code = c as u32;
                if code <= 0xff {
                    out.push_str(&format!("\\x{code:02x}"));
                } else if code <= 0xffff {
                    out.push_str(&format!("\\u{code:04x}"));
                } else {
                    out.push_str(&format!("\\U{code:08x}"));
                }
            }
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}

fn require_seed_body(directory: &Path, refusal: &str) -> Result<()> {
    let lines = seed_lines(directory)?;
    let lines: Vec<_> = lines.iter().map(|s| split_terminator(s).0).collect();
    let fence = Regex::new(r"^ {0,3}(```|~~~)").unwrap();
    let mut inside = false;
    let fenced: Vec<_> = lines
        .iter()
        .map(|s| {
            if fence.is_match(s) {
                inside = !inside;
                false
            } else {
                inside
            }
        })
        .collect();
    let index = lines.iter().enumerate().position(|(i, s)| {
        i > 0 && !fenced[i] && s.trim_end_matches(whitespace) == "## You are confirming"
    });

    let valid = index.is_some_and(|index| {
        (1..index).any(|i| !fenced[i] && revision(lines[i]).is_some())
            && (index + 1..lines.len())
                .take_while(|i| !lines[*i].starts_with('#') || fenced[*i])
                .any(|i| {
                    !lines[i].trim_matches(whitespace).is_empty() && revision(lines[i]).is_none()
                })
    });
    require(
        valid,
        format!(
            "{refusal}: {} is not a seed contract; after its status line it needs a 'revision:' line and a '## You are confirming' section with content",
            directory.join(FILE).display()
        ),
    )
}
pub(super) fn require_seed_line(directory: &Path, expected: &str) -> Result<()> {
    let path = directory.join(FILE);
    require(
        path.is_file(),
        format!(
            "cannot complete: {} is missing; save the seed there with line 1 {}",
            path.display(),
            repr(expected)
        ),
    )?;
    let lines = seed_lines(directory)?;
    let first = lines.first().map_or("", |s| split_terminator(s).0);
    require(
        first == expected,
        format!(
            "cannot complete: line 1 of {} must be exactly {} for this session, found {}",
            path.display(),
            repr(expected),
            repr(first)
        ),
    )?;
    require_seed_body(directory, "cannot complete")
}

fn decisions_digest(ledger: &Value) -> Result<String> {
    let state = editable(ledger)?;
    let covered = object(
        &["draft", "goal", "origin", "nodes", "assumed", "operator"].map(|k| (k, state[k].clone())),
    );
    Ok(sha256(covered.sorted().compact(false)?.as_bytes()))
}
fn one_line(s: &str) -> String {
    s.split(whitespace)
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}
fn decisions_lines(ledger: &Value) -> Result<Vec<(String, String)>> {
    let mut rows = vec![("heading".into(), "## Decisions (from the ledger)".into())];
    for node in ledger["nodes"].items() {
        if super::resolved_unknown(node) {
            continue;
        }
        let after = if node["prerequisites"].truth() {
            format!(
                " (after: {})",
                node["prerequisites"]
                    .items()
                    .iter()
                    .map(|s| s.string().unwrap())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        } else {
            String::new()
        };
        let id = node["id"].string().unwrap();
        if node["status"].is("settled") {
            rows.push((
                id.into(),
                format!(
                    "- {id} [{}] {} (source: {}){after}",
                    node["authority"].string().unwrap(),
                    one_line(node["answer"].string().unwrap()),
                    one_line(node["authority_source"].string().unwrap())
                ),
            ));
            if let Some(d) = node.get("delegation") {
                rows.push((
                    id.into(),
                    format!(
                        "  Delegation (caller-declared, not authenticated): {}",
                        d.sorted().compact(true)?
                    ),
                ));
            }
        } else if node["status"].is("deferred") {
            rows.push((
                id.into(),
                format!(
                    "- {id} deferred: {} (revisit if: {}){after}",
                    one_line(node["defer_reason"].string().unwrap()),
                    one_line(node["revisit_condition"].string().unwrap())
                ),
            ));
        }
    }
    if let Some(base) = crate::seed_me::retry::base(ledger)? {
        rows.push((
            "starting commit".into(),
            format!("- Starting commit from Taste: {}", base.head),
        ));
    }
    rows.extend([
        ("heading".into(), "".into()),
        ("heading".into(), "## Assumed (not confirmed by you)".into()),
    ]);
    if !ledger["assumed"].truth() {
        rows.push(("assumed".into(), "- none".into()));
    }
    for a in ledger["assumed"].items() {
        let line = one_line(a["text"].string().unwrap());
        rows.push((
            format!(
                "assumed {}",
                repr(&line.chars().take(40).collect::<String>())
            ),
            format!("- {line} (why: {})", one_line(a["why"].string().unwrap())),
        ));
    }
    Ok(rows)
}

/// Only complete goal projections are literal: their originals remain unchanged.
pub(super) fn goal_lines(ledger: &Value) -> Result<Vec<String>> {
    let title = ledger["goal"]
        .string()
        .unwrap_or("")
        .lines()
        .next()
        .unwrap_or("");
    let mut lines = vec![format!("# Seed: {title}")];
    lines.extend(
        decisions_lines(ledger)?
            .into_iter()
            .filter(|(id, _)| Some(id.as_str()) == ledger["origin"].string())
            .map(|(_, line)| line),
    );
    Ok(lines)
}

/// The ledger-owned Decisions and Assumed sections, verbatim and read-only.
pub fn decisions_block(ledger: &Value) -> Result<String> {
    Ok(decisions_lines(ledger)?
        .into_iter()
        .map(|(_, s)| s + "\n")
        .collect())
}
pub(super) fn require_seed_decisions(
    directory: &Path,
    ledger: &Value,
    refusal: &str,
) -> Result<()> {
    let lines = seed_lines(directory)?;
    let lines: Vec<_> = lines.iter().map(|s| split_terminator(s).0).collect();
    let normalized = format!("\n{}\n", lines.join("\n"));
    if normalized.contains(&format!("\n{}", decisions_block(ledger)?)) {
        return Ok(());
    }
    let stale: Vec<_> = decisions_lines(ledger)?
        .into_iter()
        .filter(|(label, line)| label != "heading" && !lines.contains(&line.as_str()))
        .map(|(label, _)| label)
        .collect();
    let detail = if stale.is_empty() {
        "the block is incomplete or out of order".into()
    } else {
        format!("not current: {}", stale.join(", "))
    };
    Err(format!(
        "{refusal}: the seed's decisions do not match the ledger ({detail}); reopen Shape and draft again from the current ledger"
    ))
}

/// Summarize the saved bytes without claiming confirmation or repairing invalid text.
pub fn seed_summary(directory: &Path) -> Result<Value> {
    let path = directory.join(FILE);
    if !path.is_file() {
        return Ok(Value::Null);
    }
    let data = super::super::read_seed(&path).map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&data);
    let lines: Vec<_> = text.split('\n').collect();
    let label = lines.iter().skip(1).find_map(|s| revision(s));
    Ok(object(&[
        ("line_1", lines[0].trim_end_matches('\r').into()),
        ("revision", label.map_or(Value::Null, Value::from)),
        ("sha256", sha256(&data).as_str().into()),
    ]))
}
fn confirmation_record(directory: &Path) -> Option<Value> {
    let bytes =
        super::super::read_regular(&directory.join(CONFIRMATION), "confirmation record").ok()?;
    let record = Value::parse(std::str::from_utf8(&bytes).ok()?).ok()?;
    let fields = [
        "schema_version",
        "checks_sha256",
        "session_id",
        "operator",
        "revision",
        "source",
        "confirmed_at",
        "seed_sha256",
        "decisions_sha256",
    ];
    let agent = record["schema_version"] == int(3)
        && record["operator"].is("agent")
        && record.get("actor").is_some_and(|actor| {
            actor
                .compact(false)
                .ok()
                .and_then(|json| {
                    serde_json::from_str::<crate::think_record::GoalConfirmation>(&json).ok()
                })
                .is_some_and(|actor| {
                    matches!(actor.actor(), Ok(crate::think_record::Actor::Agent(_)))
                })
        });
    let mut allowed = fields.to_vec();
    if agent {
        allowed.push("actor");
    }
    (super::exact(&record, &allowed)
        && (agent || record["schema_version"] == int(2))
        && fields
            .iter()
            .filter(|k| **k != "schema_version")
            .all(|k| text(&record[k])))
    .then_some(record)
}

/// Verify matching operator or named-agent receipts, seed bytes, decisions and acceptance records.
/// Legacy or interrupted confirmation stays unverified; this proves content, not identity or truth.
pub fn verify_handoff(directory: &Path, ledger: &Value) -> Result<Value> {
    let refused = |reason: String| {
        Ok(object(&[
            ("verified", Value::Bool(false)),
            ("reason", reason.as_str().into()),
        ]))
    };
    if !super::one_of(&super::operator(ledger), &["human", "agent"]) {
        return refused("a simulated session is a practice run that no person confirmed".into());
    }
    let seed = seed_summary(directory)?;
    if seed == Value::Null {
        return refused("no seed is saved".into());
    }
    if !seed["line_1"].is(CONFIRMED) {
        return refused(format!(
            "the seed is not confirmed (line 1 is {})",
            repr(seed["line_1"].string().ok_or("Invalid seed status line.")?)
        ));
    }
    let Some(record) = confirmation_record(directory) else {
        return refused(
            "the seed has no confirmation record from `seed confirm` in this session".into(),
        );
    };
    if record["session_id"] != ledger["session_id"]
        || !(record["operator"].is("human")
            || (record["schema_version"] == int(3) && record["operator"].is("agent")))
    {
        return refused("the confirmation record belongs to another session".into());
    }
    let Some(label) = record["revision"].string() else {
        return refused("the confirmation revision is missing".into());
    };
    if record["schema_version"] == int(3) {
        let Ok(actor) = serde_json::from_str::<crate::think_record::GoalConfirmation>(
            &record["actor"].compact(false)?,
        ) else {
            return refused("the seed actor is invalid".into());
        };
        let (Some(source), Some(timestamp)) =
            (record["source"].string(), record["confirmed_at"].string())
        else {
            return refused("the confirmation source or date is missing".into());
        };
        let expected = confirmation_note(source, label, timestamp, actor.actor()?)?;
        if !seed_lines(directory)?
            .iter()
            .any(|line| split_terminator(line).0 == expected)
        {
            return refused("the agent attribution does not match the confirmed seed".into());
        }
    }
    if record["seed_sha256"] != seed["sha256"] {
        return refused(format!(
            "the seed changed after revision {label} was confirmed"
        ));
    }
    if record["decisions_sha256"].string() != Some(&decisions_digest(ledger)?) {
        return refused(format!(
            "the decisions changed after revision {label} was confirmed"
        ));
    }
    let Ok(records) = checks::require_seed_checks(directory, ledger, true) else {
        return refused("checks are missing, invalid or differ from the rendered seed".into());
    };
    if record["checks_sha256"].string() != Some(&checks::digest(&records)?) {
        return refused(format!(
            "the checks changed after revision {label} was confirmed"
        ));
    }
    Ok(object(&[
        ("verified", Value::Bool(true)),
        ("operator", record["operator"].clone()),
        (
            "reason",
            format!("revision {label} as confirmed").as_str().into(),
        ),
    ]))
}

/// Read today's intake JSON, deliberately omitting viewer state and never opening a network connection.
pub fn status(directory: &Path) -> Result<Value> {
    let ledger = load(directory)?;
    status_from_ledger(directory, &ledger)
}

/// Derive status from one validated ledger while rechecking seed and handoff artifacts.
pub(crate) fn status_from_ledger(directory: &Path, ledger: &Value) -> Result<Value> {
    let nodes = ledger["nodes"].items();
    Ok(object(&[
        ("status", ledger["status"].clone()),
        ("revision", ledger["revision"].clone()),
        ("version", ledger["version"].clone()),
        (
            "open",
            int(nodes
                .iter()
                .filter(|n| n["status"].is("unresolved"))
                .count() as u64),
        ),
        (
            "settled",
            int(nodes.iter().filter(|n| n["status"].is("settled")).count() as u64),
        ),
        ("current_question", ledger["current_question"].clone()),
        ("seed", seed_summary(directory)?),
        ("handoff", verify_handoff(directory, ledger)?),
    ]))
}

pub(super) fn confirmed_as(
    directory: &Path,
    ledger: &Value,
    revision_label: &str,
    source: &str,
    timestamp: &str,
    actor: crate::think_record::Actor<'_>,
) -> Result<(String, Value, Value)> {
    require(
        super::one_of(&super::operator(ledger), &["human", "agent"]),
        format!(
            "a simulated session cannot be confirmed: its seed stays {}",
            repr(SIMULATED)
        ),
    )?;
    require(
        ledger["status"].is("active"),
        format!(
            "session is not active ({}): confirm before ending it",
            ledger["status"].string().ok_or("Invalid session status.")?
        ),
    )?;
    require(
        ledger["origin"] != Value::Null,
        "the goal is not confirmed yet",
    )?;
    let unresolved: Vec<_> = ledger["nodes"]
        .items()
        .iter()
        .filter(|n| n["status"].is("unresolved"))
        .map(|n| {
            n["id"]
                .string()
                .ok_or_else(|| "Invalid decision identity.".to_owned())
        })
        .collect::<Result<Vec<_>>>()?;
    require(
        unresolved.is_empty(),
        format!("unresolved nodes remain: {}", unresolved.join(", ")),
    )?;
    let flags = review_flags(&ledger["nodes"]);
    let mut flagged: Vec<_> = flags.fields().iter().map(|(k, _)| k.as_str()).collect();
    flagged.sort();
    require(
        flagged.is_empty(),
        format!("nodes flagged for review remain: {}", flagged.join(", ")),
    )?;
    let mut lines = seed_lines(directory)?;
    let first = lines.first().map_or("", |s| split_terminator(s).0);
    require(
        first != CONFIRMED,
        format!(
            "the seed is already confirmed (line 1 is {}); to confirm a change, save a new revision with line 1 {} and no 'Confirmed by:' line, show it, and confirm that",
            repr(first),
            repr(DRAFT)
        ),
    )?;
    require(
        first == DRAFT,
        format!(
            "line 1 of {FILE} must be exactly {} to confirm it, found {}",
            repr(DRAFT),
            repr(first)
        ),
    )?;
    let stale: Vec<_> = lines
        .iter()
        .enumerate()
        .filter(|(i, s)| *i > 0 && s.starts_with("Confirmed by:"))
        .map(|(i, _)| (i + 1).to_string())
        .collect();
    require(
        stale.is_empty(),
        format!(
            "{FILE} still has an earlier 'Confirmed by:' line (line {}); remove it from the new draft",
            stale.join(", ")
        ),
    )?;
    let found = lines
        .iter()
        .enumerate()
        .skip(1)
        .find_map(|(i, s)| revision(s).map(|r| (i, r.to_owned())));
    let (index, label) = found.ok_or_else(|| {
        format!("{FILE} has no 'revision:' line, so the revision cannot be matched")
    })?;
    require(
        label == revision_label.trim_matches(whitespace),
        format!(
            "revision {} does not match the file's revision line ({})",
            repr(revision_label.trim_matches(whitespace)),
            repr(&label)
        ),
    )?;
    require_seed_body(directory, "cannot confirm")?;
    require_seed_decisions(directory, ledger, "cannot confirm")?;
    let records = checks::require_seed_checks(directory, ledger, false)?;
    if let Some(statement) = checks::commandless_check(&records) {
        return Err(format!(
            "Cannot confirm: \"{statement}\" has no command. F1: Supply a check command, or Make it a judgment with your how-to-judge words. Nothing was confirmed."
        ));
    }

    let first_end = split_terminator(&lines[0]).1;
    let first_end = if first_end.is_empty() {
        "\n"
    } else {
        first_end
    }
    .to_owned();
    let (body, terminator) = split_terminator(&lines[index]);
    let note = confirmation_note(source, &label, timestamp, actor)?;
    let next = if !terminator.is_empty() {
        note + terminator
    } else {
        let body = body.to_owned();
        lines[index] = body + &first_end;
        note
    };
    lines[0] = CONFIRMED.to_owned() + &first_end;
    lines.insert(index + 1, next);
    let confirmed = lines.concat();
    let digest = sha256(confirmed.as_bytes());
    let mut record = object(&[
        ("schema_version", int(2)),
        ("checks_sha256", checks::digest(&records)?.as_str().into()),
        ("session_id", ledger["session_id"].clone()),
        ("operator", actor.session_operator().into()),
        ("revision", label.as_str().into()),
        ("source", source.into()),
        ("confirmed_at", timestamp.into()),
        ("seed_sha256", digest.as_str().into()),
        (
            "decisions_sha256",
            decisions_digest(ledger)?.as_str().into(),
        ),
    ]);
    if let Some(context) = actor.context() {
        record.set("schema_version", int(3));
        record.set(
            "actor",
            Value::from(
                serde_json::to_value(context).map_err(|_| "Cannot encode seed attribution.")?,
            ),
        );
    }
    let result = object(&[
        (
            "seed",
            directory.join(FILE).to_string_lossy().as_ref().into(),
        ),
        ("line_1", CONFIRMED.into()),
        ("revision", label.as_str().into()),
        ("sha256", digest.as_str().into()),
    ]);
    Ok((confirmed, record, result))
}

fn confirmation_note(
    source: &str,
    label: &str,
    timestamp: &str,
    actor: crate::think_record::Actor<'_>,
) -> Result<String> {
    let date = timestamp.get(..10).ok_or("Invalid confirmation date.")?;
    let attribution = actor.context().map_or(String::new(), |context| {
        format!("; caller-declared {}", one_line(&context.details()))
    });
    Ok(format!(
        "Confirmed by: \"{}{attribution}\" (revision {label}, {date}). Not approved for implementation.",
        source
    ))
}

#[cfg(test)]
mod tests;
