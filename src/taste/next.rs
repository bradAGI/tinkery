//! One next step after a report of external acceptance. Missing evidence stays unknown;
//! only failed/stale checks or owed gates call for repair. No feature or permission is invented.

use super::{
    read::{Check, ResultRow},
    safe,
};
use crate::think_record::DraftSuggestion;

pub(super) struct Pause {
    pub(super) next: String,
    pub(super) lines: Vec<String>,
    pub(super) draft: DraftSuggestion,
}

pub(super) fn suggest(row: &ResultRow) -> Pause {
    let failed = row
        .items
        .iter()
        .find(|i| matches!(i.state, Check::Fail | Check::Stale));
    let unmet: Vec<_> = row
        .items
        .iter()
        .filter(|i| {
            matches!(
                i.kind.as_str(),
                "automated" | "human attestation" | "manual check"
            ) && i.state != Check::Pass
        })
        .collect();
    let repair = failed.is_some() || !row.gates.is_empty();
    let decision = if repair { "stop and repair" } else { "proceed" };
    let condition = unmet
        .iter()
        .copied()
        .find(|i| matches!(i.state, Check::Fail | Check::Stale))
        .or_else(|| unmet.first().copied())
        .or(failed);
    let (target, check) = if let Some(item) = condition {
        (
            format!(
                "{} {}",
                if repair { "Resolve" } else { "Check" },
                safe(&item.label)
            ),
            "Check this completion condition against the accepted behavior and current result."
                .to_owned(),
        )
    } else if let Some(gate) = row.gates.first() {
        (
            format!("Resolve the owed action: {}", safe(gate)),
            "Obtain the required external decision; a draft cannot grant permission.".into(),
        )
    } else {
        let goal = super::goal(&row.goal).lines().next().unwrap_or("").trim();
        (
            format!("Observe “{}”; choose the next slice from this goal", safe(goal)),
            "Try the accepted behavior against the goal's completion conditions and report a counterexample.".into(),
        )
    };
    let stop = if target.trim_end().ends_with(['.', '?', '!', '…']) {
        ""
    } else {
        "."
    };
    let mut text = format!(
        "{target}{stop}\nUncertainty: does the accepted result satisfy the unchanged goal?\n\
         Keep the confirmed goal and its constraints unchanged; do not invent another feature or bypass gates.\n\
         Disprove this slice: {check}"
    );
    if let crate::sdlc::Goal::Matched(goal) = &row.goal {
        text.push_str("\n\nConfirmed goal (unchanged source):\n");
        text.push_str(goal);
    }
    let provenance = format!(
        "Deterministic Taste guess after a report of external acceptance; not a verified verdict. Run {} ({}); seed {}; {} @{}. Decision: {decision}. Basis: {}. Gates owed: {}. Gaps: {}.",
        row.id,
        row.slug,
        row.seed,
        row.url.as_deref().unwrap_or("unknown"),
        row.head.as_deref().unwrap_or("unknown"),
        row.items
            .iter()
            .map(|i| format!("{}: {}", i.label, i.state))
            .collect::<Vec<_>>()
            .join("; "),
        row.gates.join("; "),
        row.gaps.join("; ")
    );
    Pause {
        next: format!("Guess · Next: {target}{stop}"),
        lines: vec![
            "Outcome: unobserved; acceptance alone is not user feedback.".into(),
            "Evidence: snapshot only; absent GitHub checks and missing receipts stay unknown."
                .into(),
            format!(
                "Learning: {} unmet source completion conditions.",
                unmet.len()
            ),
            "Futures: unknown; no code-level changeability evidence is supplied.".into(),
            format!("Decision: {decision}; only failed/stale checks or owed gates require repair."),
        ],
        draft: DraftSuggestion { text, provenance },
    }
}
