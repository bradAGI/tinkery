//! Worker-step display and attention from typed status, without API-word recognition or execution.

use super::{Worker, sdlc};

/// tink-sdlc's pinned build steps as detail lines, and what they say about a worker.
/// An older tink-sdlc omits them; anything present but unfamiliar is refused, not guessed.
pub(super) fn render(run: &sdlc::Run) -> Result<Option<(Vec<String>, Worker)>, String> {
    use sdlc::{StepState as S, StopReason as R, TurnStatus as T};
    if !run.steps_present {
        return Ok(None);
    }
    let refuse = || "A worker step Watch doesn't know; nothing is shown rather than a guess.";
    let mut lines = Vec::new();
    let mut worker = Worker::Idle;
    for step in run.steps.as_ref().ok_or_else(refuse)? {
        let name = step.data["name"].as_str().ok_or_else(refuse)?;
        let state = step.state.as_str().ok_or_else(refuse)?;
        let seen = match (step.state.known(), step.turn.present()) {
            (Some(S::Active), true) => {
                let now = match step.turn.known() {
                    Some(T::Ended) => Worker::Ended,
                    Some(T::Running) => Worker::Running,
                    Some(T::Unknown) => Worker::Unknown,
                    _ => return Err(refuse().into()),
                };
                let words = step.data["observed_turn"].as_str().ok_or_else(refuse)?;
                if worker == Worker::Idle || now == Worker::Ended {
                    worker = now;
                }
                format!(" · {words}")
            }
            // Only the newest uncertain step owes reconciliation; an older tink-sdlc
            // doesn't say which is newest, so every uncertain step is treated as owed.
            (Some(S::Uncertain), false) => {
                let owed = match step.data.get("owed") {
                    None => true,
                    Some(owed) => owed.as_bool().ok_or_else(refuse)?,
                };
                if owed && worker != Worker::Ended {
                    worker = Worker::Uncertain;
                }
                String::new()
            }
            (Some(S::Prepared | S::Completed | S::Unreadable), false) => String::new(),
            // A worker stopped before its turn ended is never shown as a finished turn.
            (Some(S::Stopped), false) => match step.stopped_by.known() {
                Some(R::Operator) => " by you".into(),
                Some(R::Cap) => " at its response cap".into(),
                _ => return Err(refuse().into()),
            },
            _ => return Err(refuse().into()),
        };
        lines.push(format!("Worker step {name}: {state}{seen}"));
    }
    if lines.is_empty() {
        lines.push("No worker step has been started for this run.".into());
    }
    Ok(Some((lines, worker)))
}
