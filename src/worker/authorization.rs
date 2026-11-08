//! Relayed start words are caller-supplied, never authenticated consent. This boundary binds
//! them to one reviewed seed and allowance and records attribution before worker admission.

use super::{Config, Run};
use crate::seed_me::ready::SeedTarget;
use serde::{Deserialize, Serialize};
use std::{
    fs::OpenOptions,
    io::{Read, Write},
    os::unix::fs::OpenOptionsExt,
    path::Path,
};

const LIMIT: u64 = 16 * 1024;
const PROVENANCE: &str =
    "Provenance cannot be verified. These are relayed words, not verified operator identity.";

/// Verbatim caller-supplied words and their exact start binding; never inferred from a budget.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Relay {
    words: String,
    seed_sha256: String,
    model: String,
    responses: usize,
}

impl Relay {
    /// Read a small explicit file without creating or repairing any authorization.
    pub(crate) fn read(path: Option<&Path>) -> Result<Option<Self>, String> {
        let Some(path) = path else { return Ok(None) };
        let read = || {
            let mut bytes = Vec::new();
            let file = OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NONBLOCK)
                .open(path)
                .map_err(|e| e.to_string())?;
            if !file.metadata().map_err(|e| e.to_string())?.is_file() {
                return Err("Relayed authorization must be a regular file.".into());
            }
            file.take(LIMIT + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            if bytes.len() > LIMIT as usize {
                return Err("Relayed authorization exceeds 16 KiB.".to_string());
            }
            let relay: Self = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
            if relay.words.trim().is_empty() {
                return Err("Relayed authorization needs the operator's verbatim words.".into());
            }
            Ok(Some(relay))
        };
        read().map_err(|e: String| format!("Cannot read relayed authorization: {e}"))
    }

    /// Require an exact binding for agent F2; human and undeclared F2 keep direct consent.
    pub(crate) fn review(
        input: &Result<Option<Self>, String>,
        agent: bool,
        target: &SeedTarget,
        config: &Config,
    ) -> Result<Option<Self>, String> {
        if !agent {
            return Ok(None);
        }
        let relay = input.as_ref().map_err(Clone::clone)?.as_ref().ok_or(
            "Agent F2 requires --worker-authorization FILE with the operator's relayed authorization.",
        )?;
        if relay.seed_sha256 != target.sha256
            || relay.model != config.model
            || relay.responses != config.responses
        {
            return Err("Relayed authorization does not exactly match the reviewed seed, model and response cap.".into());
        }
        if target.retry.is_some() {
            return Err("Relayed authorization requires the seed's checkout; alternate worktrees are outside this flow.".into());
        }
        Ok(Some(relay.clone()))
    }

    /// Disclose original words and the limits of their attribution before the start action.
    pub(crate) fn disclosure(input: &Result<Option<Self>, String>) -> String {
        match input {
            Ok(Some(relay)) => format!(
                "\nAuthority: operator, relayed\nOperator declaration: agent\n{PROVENANCE}\n\
                 Relayed words (verbatim):\n{}\n\
                 Goal confirmation, seed confirmation and worker start are separate yeses.\n",
                relay.words
            ),
            Ok(None) => String::new(),
            Err(error) => format!("\nWorker authorization refused: {error}\n"),
        }
    }

    /// Persist a Tinkery start disclosure in the run, not an SDLC approval or authenticated receipt.
    pub(crate) fn record(&self, run: &Run) -> Result<(), String> {
        let value = serde_json::json!({
            "schema": 1, "operator": "agent", "authority": "operator, relayed",
            "provenance": PROVENANCE, "authorization": self,
            "run_id": run.id, "step": "build", "checkout": run.root,
        });
        let path = run
            .root
            .join("runs")
            .join(&run.slug)
            .join("worker-authorization.json");
        let write = || -> Result<(), Box<dyn std::error::Error>> {
            let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
            file.write_all(&serde_json::to_vec_pretty(&value)?)?;
            file.sync_all()?;
            Ok(())
        };
        write().map_err(|e| {
            format!("Cannot record relayed worker authorization; no worker was launched: {e}")
        })
    }
}
