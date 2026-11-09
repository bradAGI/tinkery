//! Read-only status and exact seed identity in one explicitly named checkout. Source paths and
//! current bytes are the only join; names and goal text confer no identity. No store or discovery.

mod run;
pub(crate) mod source;
#[cfg(test)]
mod tests;

pub(crate) use run::{DEADLINE, Read, SCRIPT, execute, run as status};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

/// Read bounded Git metadata without updating the index, on a background worker.
pub(crate) fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let mut command = std::process::Command::new("git");
    command.arg("--no-optional-locks").args(args);
    execute(root, command, "git", &AtomicBool::new(false), DEADLINE)
}

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

/// Read only the expected detached retry's intake record; never invoke its runtime or steps.
pub(crate) fn retry_seed(directory: &Path, slug: &str) -> Option<Seed> {
    use std::{fs::OpenOptions, io::Read as _, os::unix::fs::OpenOptionsExt};
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW)
        .open(directory.join("run.json"))
        .ok()?;
    let limit = 4 * 1024 * 1024;
    let metadata = file.metadata().ok()?;
    if !metadata.is_file() || metadata.len() > limit {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes).ok()?;
    if bytes.len() as u64 > limit {
        return None;
    }
    let run: Value = serde_json::from_slice(&bytes).ok()?;
    let bound = &run["seed_contract"];
    if run["schema"] != 1
        || !canonical_run_id(run["run_id"].as_str()?)
        || bound["path"].as_str()? != format!("runs/{slug}/seed-contract.md")
        || bound.get("definitions_sha256").is_none()
    {
        return None;
    }
    Seed::parse(&run).ok().flatten()
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

macro_rules! word_enum {
    ($name:ident, $fallback:expr, {$($variant:ident => $word:literal),+ $(,)?}) => {
        #[doc = "A recognized API 1 word; unknown values remain unrecognized."]
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub(crate) enum $name { $(#[doc = $word] $variant),+ }
        impl $name {
            fn read(value: &Value) -> Option<Self> {
                match value.as_str()? {
                    $($word => Some(Self::$variant),)+
                    _ => $fallback,
                }
            }
        }
    };
}

word_enum!(Verification, None, {
    Blocked => "blocked", Running => "running", NotRun => "not-run", Current => "current",
    Stale => "stale", Failed => "failed", Interrupted => "interrupted", Invalid => "invalid",
    AwaitingAttestation => "awaiting-attestation"
});
word_enum!(GateStatus, None, {
    Approved => "approved", Pending => "pending", Stale => "stale", ChangesRequested => "changes-requested"
});
word_enum!(CheckStatus, None, {
    Passed => "passed", Pending => "pending", Failed => "failed", Unknown => "unknown"
});
word_enum!(Proof, None, { Automated => "automated", Attested => "attested" });
word_enum!(ReceiptResult, None, {
    Passed => "passed", Failed => "failed", AwaitingAttestation => "awaiting-attestation"
});
word_enum!(StepState, None, {
    Prepared => "prepared", Active => "active", Uncertain => "uncertain", Completed => "completed",
    Stopped => "stopped", Unreadable => "unreadable"
});
word_enum!(TurnStatus, None, { Ended => "ended", Running => "not-ended", Unknown => "unknown" });
word_enum!(StopReason, None, { Cap => "cap", Operator => "operator" });

/// A boundary word retains missing, null, malformed and unfamiliar values for contextual refusals.
#[derive(Debug, Clone)]
pub(crate) struct Word<T> {
    value: Option<Value>,
    known: Option<T>,
}

impl<T: Copy + PartialEq> Word<T> {
    /// The recognized value, never a fallback for an unfamiliar word.
    pub(crate) fn known(&self) -> Option<T> {
        self.known
    }
    /// The original spelling for display and exact refusal text, not recognition.
    pub(crate) fn as_str(&self) -> Option<&str> {
        self.value.as_ref()?.as_str()
    }
    /// Whether the source supplied this field, including explicit null.
    pub(crate) fn present(&self) -> bool {
        self.value.is_some()
    }
    /// Compare only a recognized value.
    pub(crate) fn is(&self, value: T) -> bool {
        self.known == Some(value)
    }
}

/// One parsed API record. Metadata excludes word fields so readers cannot reinterpret them.
#[derive(Debug, Clone)]
pub(crate) struct Run {
    /// Non-word metadata, including source-owned evidence and identity.
    pub(crate) data: Value,
    /// Run verification, with unknowns retained until the consumer requires a known state.
    pub(crate) verification: Word<Verification>,
    /// Receipt results are separate from checklist outcomes; pending judgment is never a pass.
    pub(crate) receipt_result: Word<ReceiptResult>,
    /// Validated pending judgment IDs; absent older fields remain empty, malformed lists refuse.
    pub(crate) pending_attestations: Result<Vec<String>, &'static str>,
    /// Opaque receipt identity retains every original field for exact snapshot comparison.
    pub(crate) receipt: Receipt,
    /// A missing or malformed list cannot silently become an empty list.
    pub(crate) gates: Option<Vec<Gate>>,
    /// Typed completion conditions, absent for list summaries.
    pub(crate) checklist: Option<Vec<CheckItem>>,
    /// Older reports can omit steps; explicit null is not omission.
    pub(crate) steps: Option<Vec<Step>>,
    /// Whether the report supplied steps at all.
    pub(crate) steps_present: bool,
    /// Contract state when this record is a worker contract rather than run status.
    pub(crate) state: Word<StepState>,
    /// Contract stop reason, meaningful only for a stopped contract.
    pub(crate) stopped_by: Word<StopReason>,
}

/// Exact receipt equality without exposing its words for a second interpretation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Receipt(Value);

/// A gate's word and source metadata; stage requirements belong to the consuming operation.
#[derive(Debug, Clone)]
pub(crate) struct Gate {
    /// Source-owned stage and decision evidence.
    pub(crate) data: Value,
    /// Recognized gate state or preserved unfamiliar value.
    pub(crate) status: Word<GateStatus>,
}

/// A completion condition's recognized words and uninterpreted evidence metadata.
#[derive(Debug, Clone)]
pub(crate) struct CheckItem {
    /// Exact expectation, ownership flags, check and mark evidence.
    pub(crate) data: Value,
    /// Completion result, not an acceptance claim.
    pub(crate) status: Word<CheckStatus>,
    /// Proof method, not a passing result.
    pub(crate) proof: Word<Proof>,
    /// Missing means active; malformed provisional flags refuse interpretation.
    pub(crate) pending: Result<bool, &'static str>,
    /// Required freshness flag, not a failed sign-off.
    pub(crate) needs_recheck: Result<bool, &'static str>,
}

/// A step's recognized state, turn and stop reason, without orchestration policy.
#[derive(Debug, Clone)]
pub(crate) struct Step {
    /// Name, observed text and obligation metadata.
    pub(crate) data: Value,
    /// Persisted step state.
    pub(crate) state: Word<StepState>,
    /// Observed turn; explicit null differs from omission.
    pub(crate) turn: Word<TurnStatus>,
    /// Stop attribution, never inferred from observed turn text.
    pub(crate) stopped_by: Word<StopReason>,
}

impl Run {
    /// The sole API-word parse entry point. Optional detail fields and unknown words retain their
    /// distinctions so each operation can preserve its existing requirements and refusal wording.
    pub(crate) fn parse(value: &Value) -> Self {
        fn word<T>(value: Option<&Value>, read: fn(&Value) -> Option<T>) -> Word<T> {
            Word {
                known: value.and_then(read),
                value: value.cloned(),
            }
        }
        fn metadata(value: &Value, keys: &[&str]) -> Value {
            let mut value = value.clone();
            if let Some(object) = value.as_object_mut() {
                for key in keys {
                    object.remove(*key);
                }
            }
            value
        }
        let mut data = metadata(
            value,
            &[
                "verification_status",
                "gates",
                "checklist",
                "steps",
                "state",
                "stopped_by",
            ],
        );
        if let Some(receipt) = data.get_mut("verification").and_then(Value::as_object_mut) {
            receipt.remove("result");
            receipt.remove("pending_attestations");
        }
        Self {
            data,
            receipt: Receipt(value["verification"].clone()),
            verification: word(value.get("verification_status"), Verification::read),
            receipt_result: word(value["verification"].get("result"), ReceiptResult::read),
            pending_attestations: match value["verification"].get("pending_attestations") {
                None if Verification::read(&value["verification_status"])
                    != Some(Verification::AwaitingAttestation) =>
                {
                    Ok(Vec::new())
                }
                ids => (|| {
                    let invalid = "The run has no readable pending_attestations list.";
                    let ids = ids.and_then(Value::as_array).ok_or(invalid)?;
                    if ids.is_empty() {
                        return Err(invalid);
                    }
                    let mut pending = Vec::new();
                    for id in ids {
                        let id = id
                            .as_str()
                            .filter(|id| {
                                !id.is_empty()
                                    && id
                                        .bytes()
                                        .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
                            })
                            .ok_or(invalid)?;
                        if pending.iter().any(|other| other == id) {
                            return Err(invalid);
                        }
                        pending.push(id.to_owned());
                    }
                    Ok(pending)
                })(),
            },
            gates: value["gates"].as_array().map(|gates| {
                gates
                    .iter()
                    .map(|gate| Gate {
                        data: metadata(gate, &["status"]),
                        status: word(gate.get("status"), GateStatus::read),
                    })
                    .collect()
            }),
            checklist: value["checklist"].as_array().map(|items| {
                items
                    .iter()
                    .map(|item| CheckItem {
                        data: metadata(item, &["status", "proof", "pending", "needs_recheck"]),
                        status: word(item.get("status"), CheckStatus::read),
                        proof: word(item.get("proof"), Proof::read),
                        pending: match item.get("pending") {
                            None => Ok(false),
                            Some(value) => value
                                .as_bool()
                                .ok_or("A checklist item has an invalid pending flag."),
                        },
                        needs_recheck: item["needs_recheck"]
                            .as_bool()
                            .ok_or("A checklist item has no readable needs_recheck flag."),
                    })
                    .collect()
            }),
            steps: value["steps"].as_array().map(|steps| {
                steps
                    .iter()
                    .map(|step| Step {
                        data: metadata(step, &["state", "turn", "stopped_by"]),
                        state: word(step.get("state"), StepState::read),
                        turn: word(step.get("turn"), TurnStatus::read),
                        stopped_by: word(step.get("stopped_by"), StopReason::read),
                    })
                    .collect()
            }),
            steps_present: value.get("steps").is_some(),
            state: word(value.get("state"), StepState::read),
            stopped_by: word(value.get("stopped_by"), StopReason::read),
        }
    }
}

/// A checklist item's operator obligation, independent of result revision or screen state.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Attestation {
    /// Automatic or explicitly worker-owned checks owe the operator nothing.
    Worker,
    /// Missing ownership on an older manual item fails closed; recheck is not a failed sign-off.
    Operator { owed: bool, recheck: bool },
}

impl Attestation {
    /// Read API 1 ownership and obligation; malformed flags or states never mean nothing is owed.
    pub(crate) fn read(record: &CheckItem) -> Result<Self, String> {
        let item = &record.data;
        let flag = |key: &str| {
            item[key]
                .as_bool()
                .ok_or_else(|| format!("A checklist item has no readable {key} flag."))
        };
        let automatic = flag("automatic")?;
        let operator = match item.get("operator_attestation") {
            None => true,
            Some(value) => value
                .as_bool()
                .ok_or("A checklist item has an invalid operator_attestation flag.")?,
        };
        let pending = record.pending?;
        let recheck = record.needs_recheck?;
        let status = record
            .status
            .known()
            .ok_or("A checklist item has no readable status.")?;
        Ok(if automatic || !operator {
            Self::Worker
        } else {
            Self::Operator {
                owed: !pending && status != CheckStatus::Passed,
                recheck,
            }
        })
    }
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
            let run = Run::parse(run);
            if run.verification.is(Verification::Invalid) {
                return Err(
                    "A run couldn't be read. Check it in tink-sdlc, then reopen this view to read again."
                        .into(),
                );
            }
            Seed::parse(&run.data)
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
    confirmed(run).map_or_else(|gap| gap, |ready| Goal::Matched(ready.goal))
}

/// A run may supply the goal and retry base only from its own still-matching confirmed seed.
pub(crate) fn confirmed(run: &Value) -> Result<crate::seed_me::ready::ReadySeed, Goal> {
    let seed = Seed::parse(run)
        .map_err(|_| Goal::Unknown)?
        .ok_or(Goal::NoSeed)?;
    let matched = || {
        if !seed.matches(&seed.source).ok()? {
            return None;
        }
        let ready = crate::seed_me::ready::confirmed(&seed.source).ok()?;
        seed.matches(&seed.source).ok()?.then_some(ready)
    };
    matched().ok_or(Goal::Unknown)
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

/// Source run IDs must use the canonical lowercase UUIDv4 spelling.
pub(crate) fn canonical_run_id(id: &str) -> bool {
    static ID: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$")
            .unwrap()
    });
    ID.is_match(id)
}
