//! Read-only status and exact seed identity in one explicitly named checkout. Source paths and
//! current bytes are the only join; names and goal text confer no identity. No store or discovery.

mod run;
#[cfg(test)]
mod tests;

pub(crate) use run::{DEADLINE, Read, SCRIPT, execute, run as status};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

/// A source-owned intake record, not permission to launch or approve anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Seed {
    source: PathBuf,
    sha256: String,
}

impl Seed {
    /// Refuse malformed records. Older or absent records cannot establish a join.
    pub(crate) fn parse(run: &Value) -> Result<Option<Self>, String> {
        let record = run
            .get("seed_contract")
            .ok_or("The run doesn't report its original seed.")?;
        if record.is_null() {
            return Ok(None);
        }
        let source = record["source"]
            .as_str()
            .ok_or("The run has no original seed location.")?;
        let sha256 = record["sha256"]
            .as_str()
            .filter(|s| {
                s.len() == 64
                    && s.bytes()
                        .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
            })
            .ok_or("The run has no readable seed fingerprint.")?;
        if !Path::new(source).is_absolute() || record["path"].as_str().is_none() {
            return Err("The run has no readable seed location.".into());
        }
        Ok(Some(Self {
            source: source.into(),
            sha256: sha256.into(),
        }))
    }

    /// True only for the canonical source path and the bytes recorded at intake.
    pub(crate) fn matches(&self, path: &Path) -> Result<bool, String> {
        crate::seed_me::migration::refuse_old_root(path)?;
        let path = path
            .canonicalize()
            .map_err(|e| format!("Cannot locate the original seed: {e}"))?;
        if self.source.as_os_str() != path.as_os_str() {
            return Ok(false);
        }
        let bytes = crate::seed_me::read_seed(&path)
            .map_err(|e| format!("Cannot read the original seed: {e}"))?;
        Ok(self.sha256 == format!("{:x}", Sha256::digest(bytes)))
    }
}

/// Read only API 1's envelope; unknown interfaces never supply facts.
pub(crate) fn envelope(output: &str) -> Result<Value, String> {
    let value: Value =
        serde_json::from_str(output).map_err(|e| format!("Couldn't read the run report: {e}"))?;
    if value["protocol"].as_str() != Some("tink-sdlc") || value["api_version"].as_u64() != Some(1) {
        return Err(
            "This checkout's run report isn't supported. Update tink-sdlc and read again.".into(),
        );
    }
    Ok(value)
}

/// Read the named checkout once with the shared deadline and cancellation; missing records or
/// unreadable runs refuse the whole start classification, never conceal any seed.
pub(crate) fn seeds(root: &Path, cancel: &AtomicBool) -> Result<Vec<Seed>, String> {
    if !root.join(SCRIPT).is_file() {
        return Err("The named checkout has no run reader.".into());
    }
    let output = run::run(root, &["status".into(), "--json".into()], cancel, DEADLINE)?;
    let value = envelope(&output)?;
    value["runs"]
        .as_array()
        .ok_or("The checkout reported no runs list.")?
        .iter()
        .map(|run| {
            if run["verification_status"] == "invalid" {
                return Err(
                    "A run couldn't be read. Check it in tink-sdlc, then reopen this view to read again."
                        .into(),
                );
            }
            Seed::parse(run)
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|seeds| seeds.into_iter().flatten().collect())
}

/// A run's seed evidence distinguishes known absence from an unreadable or unmatched original.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Goal {
    /// The seed matches and its confirmation verifies, with the exact saved goal.
    Matched(String),
    /// The run explicitly records that no confirmed seed was used.
    NoSeed,
    /// A missing, malformed or unmatched record cannot supply a goal.
    Unknown,
}

/// Read the exact goal only for a still-matching, verified seed, on a worker. An explicit null
/// record means no seed; missing, changed or unverified original session files stay unknown.
pub(crate) fn goal(run: &Value) -> Goal {
    let seed = match Seed::parse(run) {
        Ok(Some(seed)) => seed,
        Ok(None) => return Goal::NoSeed,
        Err(_) => return Goal::Unknown,
    };
    let matched = || {
        if !seed.matches(&seed.source).ok()? {
            return None;
        }
        let goal = crate::seed_me::ready::goal(&seed.source).ok()?;
        seed.matches(&seed.source).ok()?.then_some(goal)
    };
    matched().map_or(Goal::Unknown, Goal::Matched)
}

/// Whether a session's current seed was recorded by any run. A changed same-path seed is an
/// error (unknown), not a waiting or started claim. A different path never joins.
pub(crate) fn started(seeds: &[Seed], path: &Path) -> Result<bool, String> {
    let path = match path.canonicalize() {
        Ok(path) => path,
        Err(error) => {
            if seeds
                .iter()
                .any(|seed| seed.source.as_os_str() == path.as_os_str())
            {
                return Err(format!("Cannot locate the seed: {error}"));
            }
            return Ok(false);
        }
    };
    let mut same_source = false;
    for seed in seeds
        .iter()
        .filter(|seed| seed.source.as_os_str() == path.as_os_str())
    {
        same_source = true;
        if seed.matches(&path)? {
            return Ok(true);
        }
    }
    if same_source {
        Err("The seed's contents differ from the run. Check it in Shape.".into())
    } else {
        Ok(false)
    }
}

/// AGENTS.md as tink-sdlc compares it: each well-formed generated `tink:rules` block and the
/// blank line before it are dropped, so text the launcher writes is not an authored change.
/// Unbalanced markers leave the bytes unchanged, never guessing where a block ends.
pub(crate) fn without_generated_rules(data: &[u8]) -> Vec<u8> {
    use regex::bytes::Regex;
    use std::sync::LazyLock;
    static BEGIN: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"<!--\s*tink:rules begin\b.*?-->").unwrap());
    static END: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"<!--\s*tink:rules end\s*-->").unwrap());
    let lines: Vec<&[u8]> = data.split_inclusive(|&b| b == b'\n').collect();
    let (mut blocks, mut open) = (Vec::new(), None);
    for (number, line) in lines.iter().enumerate() {
        if BEGIN.is_match(line) {
            if open.replace(number).is_some() {
                return data.to_vec();
            }
        } else if END.is_match(line) {
            let Some(begin) = open.take() else {
                return data.to_vec();
            };
            blocks.push((begin, number));
        }
    }
    if open.is_some() || blocks.is_empty() {
        return data.to_vec();
    }
    let (mut out, mut cursor) = (Vec::with_capacity(data.len()), 0);
    for (begin, end) in blocks {
        let separator = begin > cursor && lines[begin - 1].trim_ascii().is_empty();
        out.extend(lines[cursor..begin - usize::from(separator)].concat());
        cursor = end + 1;
    }
    out.extend(lines[cursor..].concat());
    out
}
