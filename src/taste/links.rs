//! A result's PR comes only from the run's own tink-sdlc record (`sdlc.py pr`). Prose, slugs,
//! branches and other runs' records cannot establish the link; a missing or malformed record
//! stays unknown.

use serde_json::Value;

/// Why the checkout cannot supply a PR and what the operator can do about it.
#[derive(Clone, Copy, Debug)]
pub(super) struct PrGap {
    /// What the source establishes about the result.
    pub(super) title: &'static str,
    /// The prerequisite for opening the PR or reading an exact-head preview.
    pub(super) next: &'static str,
}

/// The list could not identify a valid run whose own PR record we can read.
pub(super) const UNKNOWN_RUN: PrGap = PrGap {
    title: "Result unknown: run identity unavailable",
    next: "Repair the run externally, then press r.",
};

/// The PR the run recorded for itself, or why it is unknown. Never reads files or contacts GitHub.
pub(super) fn recorded(run: &Value) -> Result<String, PrGap> {
    let url = match &run["pr"] {
        Value::Null => {
            return Err(PrGap {
                title: "Result unknown: no recorded PR",
                next: "When this run has a PR, record it with tink-sdlc, then press r.",
            });
        }
        pr => pr["url"].as_str().unwrap_or(""),
    };
    if !canonical_pr_url(url) {
        return Err(PrGap {
            title: "Result unknown: invalid PR URL",
            next: "Inspect this run's PR record outside Tinkery. After repair, press r.",
        });
    }
    Ok(url.into())
}

fn canonical_pr_url(url: &str) -> bool {
    static PR: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"^https://github\.com/[A-Za-z0-9-]+/[A-Za-z0-9_.-]+/pull/[1-9][0-9]*$")
            .unwrap()
    });
    PR.is_match(url)
        && !url
            .split('/')
            .nth(4)
            .is_some_and(|repo| matches!(repo, "." | ".."))
}

/// Source run IDs must use the canonical lowercase UUIDv4 spelling.
pub(super) fn canonical_run_id(id: &str) -> bool {
    static ID: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$")
            .unwrap()
    });
    ID.is_match(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    const URL: &str = "https://github.com/TEST/clinic-tycoon/pull/1";

    #[test]
    fn only_the_runs_own_record_supplies_its_pr() {
        assert_eq!(
            recorded(&json!({"slug": "TEST-result", "pr": {"url": URL}})).unwrap(),
            URL
        );
        let missing = recorded(&json!({"slug": "TEST-result", "pr": null})).unwrap_err();
        assert_eq!(missing.title, "Result unknown: no recorded PR");
        assert!(
            recorded(&json!({"slug": "TEST-result"})).is_err(),
            "older tink-sdlc has no pr field"
        );
    }

    #[test]
    fn malformed_recorded_urls_stay_unknown() {
        for url in [
            "TEST/clinic-tycoon#1",
            "http://github.com/TEST/clinic-tycoon/pull/1",
            "https://evil.test/TEST/clinic-tycoon/pull/1",
            "https://github.com/TEST/clinic-tycoon/pull/0",
            "https://github.com/TEST/clinic-tycoon/pull/1/",
            "https://github.com/TEST/clinic-tycoon/pull/1?x=1",
            "https://github.com/TEST/clinic-tycoon/pull/1#review",
            "https://github.com/TEST/../pull/1",
            "https://github.com/TEST/clinic-tycoon/pull/1\n",
        ] {
            let error = recorded(&json!({"slug": "TEST-result", "pr": {"url": url}})).unwrap_err();
            assert_eq!(error.title, "Result unknown: invalid PR URL", "{url}");
        }
    }

    #[test]
    fn run_ids_must_be_canonical_uuid_v4() {
        assert!(canonical_run_id("00000000-0000-4000-8000-000000000114"));
        for id in [
            "TEST-result",
            "",
            "00000000-0000-1000-8000-000000000114",
            "00000000-0000-4000-7000-000000000114",
            "AAAAAAAA-0000-4000-8000-000000000114",
        ] {
            assert!(!canonical_run_id(id), "{id}");
        }
    }
}
