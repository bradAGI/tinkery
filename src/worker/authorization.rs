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
const REQUIRED: &str = "Agent worker admission requires --worker-authorization FILE with the operator's relayed authorization.";
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
}

/// One authorization door for agent starts, launches and resumes; other operators pass directly.
#[derive(Clone, Debug)]
pub(crate) struct Gate {
    agent: bool,
    relay: Result<Option<Relay>, String>,
}

impl Gate {
    /// Retain the launch-time declaration and relay snapshot without granting admission.
    pub(crate) fn new(agent: bool, relay: Result<Option<Relay>, String>) -> Self {
        Self { agent, relay }
    }

    fn bind(&self, target: Option<&SeedTarget>, config: &Config) -> Result<Option<&Relay>, String> {
        if !self.agent {
            return Ok(None);
        }
        let relay = self
            .relay
            .as_ref()
            .map_err(Clone::clone)?
            .as_ref()
            .ok_or(REQUIRED)?;
        let target = target.ok_or("Agent worker admission requires a readable confirmed seed.")?;
        if relay.seed_sha256 != target.sha256
            || relay.model != config.model
            || relay.responses != config.responses
        {
            return Err("Relayed authorization does not exactly match the reviewed seed, model and response cap.".into());
        }
        if target.retry.is_some() {
            return Err("Relayed authorization requires the seed's checkout; alternate worktrees are outside this flow.".into());
        }
        Ok(Some(relay))
    }

    /// Refuse agent work without an exact non-retry seed, model and allowance binding.
    pub(crate) fn check(&self, target: Option<&SeedTarget>, config: &Config) -> Result<(), String> {
        self.bind(target, config).map(|_| ())
    }

    /// Disclose verbatim relay words and attribution limits, including any known refusal.
    pub(crate) fn disclosure(&self, binding: Option<(&SeedTarget, &Config)>) -> String {
        if !self.agent {
            return String::new();
        }
        let input = match binding {
            Some((target, config)) => self.bind(Some(target), config),
            None => self
                .relay
                .as_ref()
                .map_err(Clone::clone)
                .and_then(|relay| relay.as_ref().map(Some).ok_or_else(|| REQUIRED.into())),
        };
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

    /// Bind and append each agent admission before work; recording failures refuse the action.
    pub(crate) fn record(
        &self,
        run: &Run,
        target: Option<&SeedTarget>,
        config: &Config,
        action: &str,
    ) -> Result<(), String> {
        let Some(relay) = self.bind(target, config)? else {
            return Ok(());
        };
        let value = serde_json::json!({
            "schema": 1, "operator": "agent", "authority": "operator, relayed",
            "provenance": PROVENANCE, "authorization": relay, "action": action,
            "run_id": run.id, "step": "build", "checkout": run.root,
        });
        let path = run
            .root
            .join("runs")
            .join(&run.slug)
            .join("worker-authorization.jsonl");
        let write = || -> Result<(), Box<dyn std::error::Error>> {
            let mut bytes = serde_json::to_vec(&value)?;
            bytes.push(b'\n');
            let mut file = OpenOptions::new().append(true).create(true).open(path)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            Ok(())
        };
        write().map_err(|e| {
            format!("Cannot record relayed worker authorization; no worker was launched: {e}")
        })
    }
}
