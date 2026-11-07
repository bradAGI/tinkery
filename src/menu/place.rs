//! The menu's closed set of places, their display copy and navigation order; no routing or actions.

/// One of the seven places the operator can open from Home.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    /// Clarify a goal.
    Think,
    /// Open saved goals.
    Tend,
    /// Prepare a brief.
    Shape,
    /// Prepare work for approval.
    Approve,
    /// See work status.
    Watch,
    /// Review a result.
    Taste,
    /// Browse recent pull requests.
    Measure,
}

/// Places in the order shown by Home and used by its navigation keys.
pub const PLACES: [Place; 7] = [
    Place::Think,
    Place::Tend,
    Place::Shape,
    Place::Approve,
    Place::Watch,
    Place::Taste,
    Place::Measure,
];

impl Place {
    /// The name and one-line purpose shown on Home, independent of navigation position.
    pub fn label(self) -> (&'static str, &'static str) {
        match self {
            Self::Think => ("think", "Clarify a goal"),
            Self::Tend => ("tend", "Open saved goals"),
            Self::Shape => ("shape", "Prepare a brief"),
            Self::Approve => ("approve", "Prepare work for approval"),
            Self::Watch => ("watch", "See work status"),
            Self::Taste => ("taste", "Review a result"),
            Self::Measure => ("measure", "Browse recent pull requests"),
        }
    }
}
