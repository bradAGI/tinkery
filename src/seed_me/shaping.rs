//! Where one session's seed stands, read only through Tinkery's ledger and seed verifier,
//! plus the seed draft's text when one waits for confirmation. Read-only: it never publishes,
//! confirms or ends anything, and keeps no copy of the ledger.

use super::answering::{AGAINST, OPTION_LABELS, UNFOUND, claim_suffix};
use serde_json::Value;
use std::path::Path;

/// One session as Shape shows it: the confirmed goal exactly as recorded, and where its seed stands.
#[derive(Debug, Clone, PartialEq)]
pub struct Shaping {
    /// Exact confirmed-seed identity from this verified read; navigation only, never permission.
    pub ready_target: Option<super::ready::SeedTarget>,
    /// The ledger's confirmed goal, byte for byte; `None` when the session records none.
    pub goal: Option<String>,
    /// The saved record explicitly lacks a success criterion; unknown older records are not absence.
    pub success_open: bool,
    /// Where the seed stands.
    pub stage: Shaped,
    /// The ledger version this was read at; a change made from what's shown must name it.
    pub version: Option<u64>,
    /// Each settled decision after the goal, in ledger order, exactly as recorded.
    pub decided: Vec<Decided>,
    /// Open questions in ledger order, naming the earlier answers each held question waits for.
    pub open: Vec<String>,
    /// Confirmation attribution and consumer requirements for a verified seed; not permission.
    pub confirmation_note: Option<String>,
    /// Ledger-computed risks and byte-bound drafting guesses, displayed without recomputing them.
    pub confirmation: Vec<String>,
    /// The ledger's latest completed teach-back for the exact draft, with display labels.
    pub teach_back: Vec<String>,
    /// The single eligible deferred prerequisite, offered only while Waiting.
    pub deferred: Option<super::ledger::DeferredPrerequisite>,
    /// First provisional requiring a command or operator judgment before confirmation.
    pub commandless_check: Option<String>,
}

/// One settled decision: what was decided, the answer as recorded, and who settled it.
#[derive(Debug, Clone, PartialEq)]
pub struct Decided {
    /// The node's label, or its id when it has none.
    pub title: String,
    /// The recorded answer, byte for byte.
    pub answer: String,
    /// The ledger's authority word: `user` means the operator's own words; anything else isn't.
    pub authority: String,
}

/// Where a session's seed stands. Each says only what the ledger's files show; nothing is guessed.
#[derive(Debug, Clone, PartialEq)]
pub enum Shaped {
    /// The current question, as the ledger records it. The ledger keeps no options.
    Question {
        /// The node's id, which an answer settles.
        id: String,
        /// The node's label, or its id when it has none.
        title: String,
        /// What this question settles, shown when the operator chooses deferral.
        gate: String,
        /// The question, or the decision to make when no question is recorded.
        ask: String,
        /// The node's evidence, one entry per line: what was checked, then what was observed.
        found: Vec<String>,
        /// The recorded suggestion, when one is recorded.
        suggestion: Option<String>,
        /// Each option's line, as recorded in the question: its choice, trade-off and undo cost.
        options: Vec<String>,
        /// The strongest case against the suggestion, when recorded.
        against: Option<String>,
        /// Whether an option is hard to undo, so the operator's instinct comes first.
        hard: bool,
    },
    /// A settled node flagged by the ledger, shown before any question or seed.
    Review {
        /// The node to keep or, for an operator-answered decision, revise.
        id: String,
        /// Ledger-owned eligibility: findings and the confirmed goal are keep-only.
        revisable: bool,
        /// The node's label or ID.
        title: String,
        /// The unchanged settled answer.
        answer: String,
        /// The ledger's reasons and their source facts.
        found: Vec<String>,
    },
    /// Active with no current question and no seed: no next question has been recorded.
    Waiting,
    /// A seed draft waiting for the operator to confirm it.
    Drafted {
        /// The draft's revision label, when it has one.
        revision: Option<String>,
        /// The draft exactly as saved, when it could be read.
        text: Option<String>,
    },
    /// A confirmed seed that `status` verifies against this session's confirmation record. It
    /// stays ready after the session is completed, since intake needs only the verified seed.
    Ready {
        /// The confirmed revision label, when it has one.
        revision: Option<String>,
        /// Whether the session is completed, so nothing more can be changed or ended.
        ended: bool,
    },
    /// A seed that says it is confirmed but that `status` can't verify, with Tinkery's reason.
    Unverified {
        /// Tinkery's reason for refusing verification.
        reason: String,
    },
    /// A practice session's seed, never a real one.
    Practice,
    /// Completed or stopped: nothing to shape.
    Ended {
        /// The ledger's status word.
        status: String,
    },
    /// The files say something this module doesn't recognise. Never guessed.
    Unknown {
        /// What wasn't recognised, in plain words.
        what: String,
    },
}

/// Read `session` through Tinkery's ledger and verifier; run disk reads off the paint path.
/// It offers no intake command: Approve owns the one that starts a build.
pub fn read(session: &Path) -> Result<Shaping, String> {
    read_with_base(session).map(|(shaping, _)| shaping)
}

/// Include the retry checkout in the same reading, so Shape uses the source it displays.
pub(crate) fn read_with_base(
    session: &Path,
) -> Result<(Shaping, Option<super::retry::Base>), String> {
    read_session(session).map_err(|reason| format!("Tinkery refused: {reason}"))
}

fn read_session(session: &Path) -> Result<(Shaping, Option<super::retry::Base>), String> {
    let ledger = super::ledger::load(session)?;
    let flags = super::ledger::review_flags(&ledger["nodes"]);
    let status = super::ledger::status_from_ledger(session, &ledger)?;
    let revisable: Vec<_> = ledger["nodes"]
        .array()
        .unwrap_or(&[])
        .iter()
        .filter_map(|n| n["id"].string())
        .filter(|id| super::ledger::can_revise(&ledger, id))
        .map(str::to_owned)
        .collect();
    let waits: serde_json::Map<String, Value> = ledger["nodes"]
        .array()
        .unwrap_or(&[])
        .iter()
        .filter_map(|node| {
            let id = node["id"].string()?;
            Some((
                id.to_owned(),
                serde_json::json!(super::ledger::waiting_on(&ledger["nodes"], id)),
            ))
        })
        .collect();
    let confirmation = super::ledger::confirmation_summary(&ledger);
    let saved = ledger;
    let ledger = saved.to_json()?;
    let mut status = status.to_json()?;
    status["review_flags"] = flags.to_json()?;
    status["revisable"] = serde_json::json!(revisable);
    status["waiting_on"] = Value::Object(waits);
    let mut shaping = classify(&ledger, &status);
    let retry = super::retry::base(&saved)?;
    if matches!(shaping.stage, Shaped::Ready { .. }) {
        shaping.ready_target = (|| {
            Some(super::ready::SeedTarget {
                session: session.canonicalize().ok()?,
                session_id: ledger["session_id"]
                    .as_str()
                    .filter(|id| !id.is_empty() && !id.chars().any(char::is_control))?
                    .to_owned(),
                retry: retry.clone(),
                version: shaping.version?,
                sha256: status["seed"]["sha256"].as_str()?.to_owned(),
            })
        })();
        if retry.is_some() && shaping.ready_target.is_none() {
            return Err("The retry has no verified session identity.".into());
        }
    }
    shaping.confirmation = confirmation;
    if matches!(shaping.stage, Shaped::Waiting) {
        shaping.deferred = super::ledger::deferred_prerequisite(&saved)?;
    }
    if matches!(shaping.stage, Shaped::Ready { .. }) && status["handoff"]["operator"] == "agent" {
        shaping.confirmation_note = Some("Agent-confirmed seed. Intake requires tink-sdlc 1.38.0 or newer; this is not operator confirmation or implementation approval.".into());
    }
    if let Shaped::Drafted { text, revision } = &mut shaping.stage {
        shaping.commandless_check = super::ledger::read_checks(session)
            .ok()
            .and_then(|records| super::ledger::commandless_check(&records).map(str::to_owned));
        *text = super::read_seed_text(&session.join("seed-contract.md")).ok();
        if let (Some(seed), Some(revision)) = (text.as_ref(), revision.as_ref()) {
            shaping.teach_back = super::ledger::teach_back_report(&saved, revision, seed);
            shaping.confirmation.splice(
                0..0,
                super::ledger::completion_report(&saved, revision, seed),
            );
        }
    }
    Ok((shaping, retry))
}

/// Where the seed stands, from the ledger and its verified `status`. An open question comes
/// before a seed, so a reopened decision is never hidden behind an older draft.
fn classify(ledger: &Value, status: &Value) -> Shaping {
    let text = |value: &Value, key: &str| {
        value
            .get(key)
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty())
            .map(str::to_owned)
    };
    let goal = ledger
        .get("goal")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let version = ledger.get("version").and_then(Value::as_u64);
    let nodes = ledger
        .get("nodes")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let origin = text(ledger, "origin");
    let decided: Vec<Decided> = nodes
        .iter()
        .filter(|n| {
            text(n, "kind").as_deref() == Some("decision")
                && text(n, "status").as_deref() == Some("settled")
                && text(n, "id") != origin
                && n.get("answer").and_then(Value::as_str) != goal.as_deref()
        })
        .filter_map(|n| {
            Some(Decided {
                title: text(n, "label").or_else(|| text(n, "id"))?,
                answer: text(n, "answer")?,
                authority: text(n, "authority").unwrap_or_else(|| "unrecorded".into()),
            })
        })
        .collect();
    let success_open = ledger
        .pointer("/draft/outcome")
        .and_then(Value::as_str)
        .is_some_and(|value| value.trim().is_empty())
        && goal.as_deref().is_some_and(|goal| {
            !goal.lines().any(|line| {
                line.strip_prefix("Done when:")
                    .is_some_and(|value| !value.trim().is_empty())
            })
        });
    let shaping = |goal: Option<String>, stage| Shaping {
        ready_target: None,
        goal,
        success_open,
        stage,
        version,
        decided: decided.clone(),
        open: open_questions(nodes, &status["waiting_on"]),
        confirmation_note: None,
        confirmation: Vec::new(),
        teach_back: Vec::new(),
        deferred: None,
        commandless_check: None,
    };
    let unknown = |what: &str| shaping(goal.clone(), Shaped::Unknown { what: what.into() });
    let Some(state) = text(status, "status") else {
        return unknown("Tinkery read no session status.");
    };
    let ready = |status: &Value| {
        let seed = status.get("seed").filter(|s| !s.is_null())?;
        let confirmed = seed.get("line_1").and_then(Value::as_str)
            == Some("status: confirmed for intake")
            && status
                .get("handoff")
                .and_then(|h| h.get("verified"))
                .and_then(Value::as_bool)
                == Some(true);
        confirmed.then(|| text(seed, "revision"))
    };
    if state == "completed"
        && let Some(revision) = ready(status)
    {
        return shaping(
            goal,
            Shaped::Ready {
                revision,
                ended: true,
            },
        );
    }
    if state != "active" {
        return shaping(goal, Shaped::Ended { status: state });
    }
    if goal.as_deref().is_none_or(|g| g.trim().is_empty()) {
        return unknown("This session has no confirmed goal.");
    }
    if let Some(node) = nodes.iter().find(|n| {
        n["id"]
            .as_str()
            .is_some_and(|id| status["review_flags"].get(id).is_some())
    }) {
        let id = text(node, "id").unwrap();
        let found = status["review_flags"][&id]
            .as_array()
            .into_iter()
            .flatten()
            .map(|flag| {
                let source = flag["because"].as_str().unwrap_or("");
                let answer = nodes
                    .iter()
                    .find(|n| n["id"] == source)
                    .and_then(|n| n["answer"].as_str())
                    .unwrap_or("");
                format!(
                    "{}: {source}: {answer}",
                    flag["why"].as_str().unwrap_or("review")
                )
            })
            .collect();
        return shaping(
            goal,
            Shaped::Review {
                revisable: status["revisable"]
                    .as_array()
                    .is_some_and(|ids| ids.iter().any(|v| v == &id)),
                title: text(node, "label").unwrap_or_else(|| id.clone()),
                id,
                answer: text(node, "answer").unwrap_or_default(),
                found,
            },
        );
    }
    if let Some(id) = text(status, "current_question") {
        let node = nodes.iter().find(|n| text(n, "id").as_deref() == Some(&id));
        let Some(node) = node else {
            return unknown("The current question isn't in its ledger.");
        };
        let Some(asked) = text(node, "question").or_else(|| text(node, "gate")) else {
            return unknown("The current question has no wording.");
        };
        // Options and the case against are lines Tinkery adds under the ask; see `answering::Ask`.
        let (mut ask, mut options, mut against) = (Vec::new(), Vec::new(), None);
        for line in asked.lines() {
            if OPTION_LABELS.iter().any(|l| line.starts_with(l)) {
                options.push(line.to_owned());
            } else if let Some(case) = line.strip_prefix(AGAINST) {
                against = Some(case.to_owned());
            } else {
                ask.push(line);
            }
        }
        let ask = ask.join("\n");
        let hard = options.iter().any(|o| o.ends_with("Undo cost: hard."));
        let found = std::iter::once(node)
            .chain(
                node["supported_by"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|id| nodes.iter().find(|n| n["id"] == *id)),
            )
            .flat_map(|node| {
                node["evidence"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(move |entry| format_finding(node, entry))
            })
            .collect();
        return shaping(
            goal,
            Shaped::Question {
                gate: text(node, "gate").unwrap_or_default(),
                title: text(node, "label").unwrap_or_else(|| id.clone()),
                id,
                ask,
                found,
                suggestion: text(node, "recommendation"),
                options,
                against,
                hard,
            },
        );
    }
    let seed = status.get("seed").filter(|s| !s.is_null());
    let Some(seed) = seed else {
        return shaping(goal, Shaped::Waiting);
    };
    let revision = text(seed, "revision");
    let handoff = status.get("handoff");
    let stage = match seed.get("line_1").and_then(Value::as_str) {
        Some("status: draft") => Shaped::Drafted {
            revision,
            text: None,
        },
        Some("status: simulated") => Shaped::Practice,
        Some("status: confirmed for intake") => match handoff
            .and_then(|h| h.get("verified"))
            .and_then(Value::as_bool)
        {
            Some(true) => Shaped::Ready {
                revision,
                ended: false,
            },
            _ => Shaped::Unverified {
                reason: handoff
                    .and_then(|h| text(h, "reason"))
                    .unwrap_or_else(|| "Tinkery gave no reason.".into()),
            },
        },
        _ => Shaped::Unknown {
            what: "Tinkery doesn't recognise the seed's first line.".into(),
        },
    };
    shaping(goal, stage)
}

fn open_questions(nodes: &[Value], waits: &Value) -> Vec<String> {
    let title = |node: &Value| {
        node["label"]
            .as_str()
            .or_else(|| node["id"].as_str())
            .unwrap_or("")
            .to_owned()
    };
    nodes
        .iter()
        .filter(|node| node["kind"] == "decision" && node["status"] == "unresolved")
        .map(|node| {
            let parents: Vec<_> = node["id"]
                .as_str()
                .and_then(|id| waits[id].as_array())
                .into_iter()
                .flatten()
                .filter_map(|id| nodes.iter().find(|parent| parent["id"] == *id))
                .map(&title)
                .collect();
            if parents.is_empty() {
                title(node)
            } else {
                format!("{} — waits for: {}", title(node), parents.join(", "))
            }
        })
        .collect()
}

fn format_finding(node: &Value, entry: &Value) -> Option<String> {
    let mut text = format_receipt(entry)?;
    if node["claim"]["type"].is_string() {
        text.push_str(&claim_suffix(&node["claim"], None));
    }
    Some(text)
}

fn format_receipt(entry: &Value) -> Option<String> {
    let text = |value: &Value, key: &str| {
        value[key]
            .as_str()
            .filter(|s| !s.trim().is_empty())
            .map(str::to_owned)
    };
    match entry {
        Value::String(s) if !s.trim().is_empty() => Some(s.clone()),
        Value::Object(_) => {
            let unfound = text(entry, "check")
                .and_then(|c| c.split_once(UNFOUND).map(|(_, why)| why.to_owned()))
                .map(|why| format!(" (citation not found: {why})"))
                .unwrap_or_default();
            match (text(entry, "checked"), text(entry, "observed")) {
                (Some(checked), Some(observed)) => Some(format!("{checked}: {observed}{unfound}")),
                (None, observed) => observed,
                (Some(_), None) => None,
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ledger() -> Value {
        json!({"goal": "TEST goal\nWhy: it matters.", "nodes": [
            {"id": "goal", "kind": "decision", "status": "settled"},
            {"id": "storage", "kind": "decision", "status": "unresolved", "label": "Where records live",
             "question": "Keep records local or synced?", "gate": "storage choice",
             "evidence": ["README.md:3 says local only", {"observed": "no sync client found", "check": "rg sync"}, 5],
             "recommendation": "Local, matching the README."}
        ]})
    }

    fn status(current: Option<&str>, seed: Value, handoff: Value) -> Value {
        json!({"status": "active", "current_question": current, "seed": seed, "handoff": handoff})
    }

    #[test]
    fn missing_success_is_known_absence_not_a_guess_about_older_records() {
        let mut saved = ledger();
        let waiting = status(None, Value::Null, Value::Null);
        assert!(!classify(&saved, &waiting).success_open);
        saved["draft"] = json!({"outcome": null});
        assert!(!classify(&saved, &waiting).success_open);
        saved["draft"]["outcome"] = " ".into();
        assert!(classify(&saved, &waiting).success_open);
        saved["draft"]["outcome"] = "The complete scenario passes.".into();
        assert!(!classify(&saved, &waiting).success_open);
        saved["draft"]["outcome"] = "".into();
        saved["goal"] = "TEST goal\nDone when: The complete scenario passes.".into();
        assert!(!classify(&saved, &waiting).success_open);
    }

    #[test]
    fn the_current_question_shows_what_the_ledger_records_and_nothing_else() {
        let shaped = classify(
            &ledger(),
            &status(Some("storage"), Value::Null, Value::Null),
        );
        assert_eq!(shaped.goal.as_deref(), Some("TEST goal\nWhy: it matters."));
        assert_eq!(
            shaped.stage,
            Shaped::Question {
                id: "storage".into(),
                gate: "storage choice".into(),
                title: "Where records live".into(),
                ask: "Keep records local or synced?".into(),
                found: vec![
                    "README.md:3 says local only".into(),
                    "no sync client found".into()
                ],
                suggestion: Some("Local, matching the README.".into()),
                options: vec![],
                against: None,
                hard: false,
            }
        );
        let mut bare = ledger();
        for key in ["label", "question", "recommendation", "evidence"] {
            bare["nodes"][1].as_object_mut().unwrap().remove(key);
        }
        let shaped = classify(&bare, &status(Some("storage"), Value::Null, Value::Null));
        assert_eq!(
            shaped.stage,
            Shaped::Question {
                id: "storage".into(),
                gate: "storage choice".into(),
                title: "storage".into(),
                ask: "storage choice".into(),
                found: vec![],
                suggestion: None,
                options: vec![],
                against: None,
                hard: false,
            }
        );
    }

    #[test]
    fn settled_decisions_after_the_goal_are_read_exactly_with_who_settled_them() {
        let mut ledger = ledger();
        ledger["origin"] = "goal".into();
        let nodes = ledger["nodes"].as_array_mut().unwrap();
        nodes[0]["answer"] = "TEST goal\nWhy: it matters.".into();
        nodes.push(
            json!({"id": "format", "kind": "decision", "status": "settled",
            "label": "Note format", "answer": " Markdown,\nas written. ", "authority": "user"}),
        );
        nodes.push(
            json!({"id": "lint", "kind": "decision", "status": "settled",
            "answer": "Clippy.", "authority": "delegated"}),
        );
        nodes.push(json!({"id": "repo", "kind": "fact", "status": "settled",
            "answer": "A Rust crate.", "authority": "evidence"}));
        let shaped = classify(&ledger, &status(None, Value::Null, Value::Null));
        assert_eq!(
            shaped.decided,
            vec![
                Decided {
                    title: "Note format".into(),
                    answer: " Markdown,\nas written. ".into(),
                    authority: "user".into(),
                },
                Decided {
                    title: "lint".into(),
                    answer: "Clippy.".into(),
                    authority: "delegated".into(),
                },
            ]
        );
    }

    #[test]
    fn a_seed_is_ready_only_when_status_verifies_it() {
        let confirmed = json!({"line_1": "status: confirmed for intake", "revision": "r2"});
        let ready = classify(
            &ledger(),
            &status(None, confirmed.clone(), json!({"verified": true})),
        );
        assert_eq!(
            ready.stage,
            Shaped::Ready {
                revision: Some("r2".into()),
                ended: false,
            }
        );
        let stale = classify(
            &ledger(),
            &status(
                None,
                confirmed.clone(),
                json!({"verified": false, "reason": "the seed changed after revision r2 was confirmed"}),
            ),
        );
        assert_eq!(
            stale.stage,
            Shaped::Unverified {
                reason: "the seed changed after revision r2 was confirmed".into()
            }
        );
        let silent = classify(&ledger(), &status(None, confirmed, Value::Null));
        assert!(matches!(silent.stage, Shaped::Unverified { .. }));
    }

    #[test]
    fn every_other_stage_is_said_plainly_and_nothing_is_guessed() {
        let drafted = json!({"line_1": "status: draft", "revision": "r1"});
        let cases = [
            (status(None, Value::Null, Value::Null), Shaped::Waiting),
            (
                status(None, drafted, Value::Null),
                Shaped::Drafted {
                    revision: Some("r1".into()),
                    text: None,
                },
            ),
            (
                status(None, json!({"line_1": "status: simulated"}), Value::Null),
                Shaped::Practice,
            ),
            (
                json!({"status": "completed"}),
                Shaped::Ended {
                    status: "completed".into(),
                },
            ),
        ];
        for (status, expected) in cases {
            assert_eq!(classify(&ledger(), &status).stage, expected);
        }
        let unknowns = [
            (ledger(), json!({})),
            (ledger(), status(Some("missing"), Value::Null, Value::Null)),
            (
                json!({"goal": null}),
                status(None, Value::Null, Value::Null),
            ),
            (
                ledger(),
                status(None, json!({"line_1": "hello"}), Value::Null),
            ),
        ];
        for (ledger, status) in unknowns {
            assert!(
                matches!(classify(&ledger, &status).stage, Shaped::Unknown { .. }),
                "{status}"
            );
        }
    }
}
