//! Watch's verification wording and attention, from typed states only. No API-word recognition.

use crate::sdlc;

/// What tink-sdlc says about a run's verification, from API 1. Anything else is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum State {
    Blocked,
    Running,
    NotRun,
    Current,
    AwaitingAttestation,
    Stale,
    Failed,
    Interrupted,
    Invalid,
    Older,
}

impl State {
    /// Preserve the shared reader's recognized state without accepting unknown words.
    pub(super) fn from_status(status: sdlc::Verification) -> Self {
        use sdlc::Verification as V;
        match status {
            V::Blocked => Self::Blocked,
            V::Running => Self::Running,
            V::NotRun => Self::NotRun,
            V::Current => Self::Current,
            V::AwaitingAttestation => Self::AwaitingAttestation,
            V::Stale => Self::Stale,
            V::Failed => Self::Failed,
            V::Interrupted => Self::Interrupted,
            V::Invalid => Self::Invalid,
        }
    }

    /// Whether a person is owed something: a gate decision (tink-sdlc reports `blocked` only
    /// while a gate isn't approved), or a failed, interrupted or unreadable run. An older run
    /// owes nothing: there is no action to take on it.
    pub(super) fn needs_you(self) -> bool {
        matches!(
            self,
            Self::Blocked
                | Self::AwaitingAttestation
                | Self::Failed
                | Self::Interrupted
                | Self::Invalid
        )
    }

    /// Status words, never an acceptance claim.
    pub(super) fn words(self) -> &'static str {
        match self {
            Self::Blocked => "waiting on a decision",
            Self::Running => "running",
            Self::NotRun => "not verified yet",
            Self::Current => "verified",
            Self::AwaitingAttestation => "waiting on your judgment",
            Self::Stale => "verification out of date",
            Self::Failed => "verification failed",
            Self::Interrupted => "verification interrupted",
            Self::Invalid => "can't be read",
            Self::Older => "older run",
        }
    }
}
