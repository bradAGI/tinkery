//! One complete record scan attributes saved goals by unique live ledger identity, never by root.
//! Malformed records invalidate the scan; duplicate records and live IDs refuse attribution.

use super::*;
use std::collections::{HashMap, HashSet};

struct Attribution {
    goal: String,
    made_in: MadeIn,
}

/// Origins in input order, sharing history's identity, exact-goal and uniqueness rules.
/// Scans each current session root and the record root once before rechecking identities.
/// Retains attribution only, never turns; malformed files refuse the scan. Writes nothing.
pub(crate) fn read_origins(
    root: &Path,
    sessions: &[(&Path, &str)],
) -> Result<Vec<MadeIn>, Unavailable> {
    let selected: Vec<_> = sessions
        .iter()
        .map(|(path, goal)| current_session(path, goal))
        .collect();
    let identities = identity_index(sessions.iter().map(|(path, _)| *path))?;
    let mut records: HashMap<String, Option<Attribution>> = HashMap::new();
    let mut older = HashSet::new();
    for entry in record_files(root)? {
        let (path, record) = entry?;
        if record.schema == 1 {
            older.insert(record.seed_me_session);
            continue;
        }
        let identity = record_identity(&path, &record)?;
        let attribution = Attribution {
            goal: record.goal,
            made_in: origin(record.schema, record.repo),
        };
        records
            .entry(identity)
            .and_modify(|found| *found = None)
            .or_insert(Some(attribution));
    }
    let after = identity_index(sessions.iter().map(|(path, _)| *path))?;
    Ok(sessions
        .iter()
        .zip(selected)
        .map(|((path, goal), selected)| {
            let Ok(selected) = selected else {
                return MadeIn::Unknown;
            };
            let Some(Some(record)) = records.get(&selected.1) else {
                return MadeIn::Unknown;
            };
            if older.contains(&session_id(&selected.0))
                || require_unique(&identities, &selected).is_err()
                || require_unique(&after, &selected).is_err()
                || record.goal != *goal
                || current_session(path, goal).ok().as_ref() != Some(&selected)
            {
                return MadeIn::Unknown;
            }
            record.made_in.clone()
        })
        .collect())
}
