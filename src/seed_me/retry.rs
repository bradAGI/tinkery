//! A fresh Shape session keeps the verified goal and its original attribution. Earlier
//! decisions become assumptions. Only the operator's explicit save creates a session.

use super::{answering, ledger};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

const PIN: &str = "tinkery:retry/base";

/// The isolated checkout and exact commit reviewed for this attempt.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Base {
    pub(crate) checkout: PathBuf,
    pub(crate) head: String,
}

/// The pin belongs to the goal's evidence, which the seed confirmation binds by digest.
pub(crate) fn base(ledger: &ledger::Value) -> Result<Option<Base>, String> {
    let origin = ledger["nodes"]
        .array()
        .unwrap_or_default()
        .iter()
        .find(|node| node["id"] == ledger["origin"]);
    let mut entries = origin
        .and_then(|n| n["evidence"].array())
        .unwrap_or_default()
        .iter()
        .filter(|entry| entry["artifact"].string() == Some(PIN));
    let Some(entry) = entries.next() else {
        return Ok(None);
    };
    let parsed: Option<Base> = entry["observed"]
        .string()
        .and_then(|s| serde_json::from_str(s).ok());
    parsed
        .filter(|pin| {
            entries.next().is_none()
                && pin.checkout.is_absolute()
                && pin
                    .checkout
                    .to_str()
                    .is_some_and(|s| !s.chars().any(char::is_control))
                && pin.head.len() == 40
                && pin
                    .head
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
        .map(Some)
        .ok_or_else(|| {
            "The rejected result's checkout or commit is unreadable. Inspect the original attempt."
                .into()
        })
}

/// Goal and source decisions as reviewed on the new-attempt screen.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Prior {
    pub(crate) goal: String,
    pub(crate) decisions: Vec<String>,
    origin: Value,
    draft: Value,
}

/// Refuse an unverified source or a goal that differs from the displayed one.
pub(crate) fn prior(seed: &Path, goal: &str) -> Result<Prior, String> {
    let session = seed.parent().ok_or("The prior seed has no session.")?;
    let ledger = ledger::load(session)?;
    if ledger["goal"].string() != Some(goal)
        || ledger::verify_handoff(session, &ledger)?["verified"] != ledger::Value::Bool(true)
    {
        return Err("The prior seed changed. Reread Taste.".into());
    }
    let state = ledger::editable(&ledger)?.to_json()?;
    let nodes = state["nodes"]
        .as_array()
        .ok_or("The prior decisions are missing.")?;
    let mut origin = nodes
        .iter()
        .find(|n| n["id"] == state["origin"])
        .ok_or("The prior goal has no origin.")?
        .clone();
    let evidence = origin["evidence"]
        .as_array_mut()
        .ok_or("The prior goal has no evidence.")?;
    // Draft reviews and checkout pins belong only to their own attempt.
    evidence.retain(|entry| {
        entry["artifact"] != PIN
            && !entry["artifact"].as_str().is_some_and(|s| {
                s.starts_with("tinkery:teach-back/") || s == "tinkery:review/completed"
            })
    });
    let decisions = nodes
        .iter()
        .filter(|n| {
            n["id"] != state["origin"] && n["kind"] == "decision" && n["status"] == "settled"
        })
        .map(|n| {
            format!(
                "{}: {}",
                n["label"]
                    .as_str()
                    .or(n["id"].as_str())
                    .unwrap_or("Decision"),
                n["answer"].as_str().unwrap_or_default()
            )
        })
        .collect();
    Ok(Prior {
        goal: goal.into(),
        decisions,
        origin,
        draft: state["draft"].clone(),
    })
}

impl Prior {
    /// Save the displayed words and reference in one publication to an empty session.
    pub(crate) fn save(
        &self,
        session: &Path,
        base: &Base,
        words: &str,
        run: &str,
    ) -> Result<(), String> {
        let stamp = answering::now();
        let mut origin = self.origin.clone();
        let evidence = origin["evidence"]
            .as_array_mut()
            .ok_or("The prior goal has no evidence.")?;
        evidence
            .push(json!({"checked":"Taste rejection","at":stamp,"observed":words,"artifact":run}));
        evidence.push(
            json!({"artifact":PIN, "checked":"Rejected result checkout","at":stamp,
            "check":"The operator chose this starting commit for the next attempt.",
            "observed":serde_json::to_string(base).map_err(|e| e.to_string())?}),
        );
        let assumed: Vec<_> = self.decisions.iter().map(|s| json!({"text":s,"why":"Carried from the prior seed for review in this attempt."})).collect();
        let state = ledger::Value::from(
            json!({"status":"active","draft":self.draft,"goal":self.goal,
            "origin":origin["id"],"nodes":[origin],"current_question":null,"operator":"human","assumed":assumed}),
        );
        ledger::publish(
            session,
            &state,
            &ledger::Value::Int("0".into()),
            &"Saved a new attempt from the reviewed Taste rejection.".into(),
            None,
            &stamp,
        )
        .map(|_| ())
        .map_err(|e| e.to_string())
    }
}
