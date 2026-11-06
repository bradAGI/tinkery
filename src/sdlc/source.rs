//! Compare what a worker or local reviewer can read with the checked commit. Run receipts
//! and SDLC's generated rules are allowed to change independently of the project source.

use std::{fs, io::Read, os::unix::fs::OpenOptionsExt, path::Path};

/// Refuse changed or untracked project files. The caller supplies bounded Git reads for
/// this checkout, including its cancellation and configuration policy.
pub(crate) fn check(
    checkout: &Path,
    head: &str,
    git: impl Fn(&[&str]) -> Result<String, String>,
) -> Result<(), String> {
    let paths = [".", ":(exclude)runs"];
    let diff = [
        "-c",
        "core.fileMode=true",
        "diff",
        "--no-ext-diff",
        "--no-textconv",
    ];
    let compare = |args: &[&str]| git(&[&diff[..], &["--quiet", head, "--"], args].concat());
    let differs = "Local source differs from the checked commit.";
    compare(&[&paths[..], &[":(exclude)AGENTS.md"]].concat()).map_err(|_| differs)?;
    if compare(&["AGENTS.md"]).is_err() {
        // A type or mode change isn't a generated text edit, even if the bytes match.
        let summary = git(&[&diff[..], &["--summary", head, "--", "AGENTS.md"]].concat())
            .map_err(|_| differs)?;
        if !summary.is_empty() {
            return Err(differs.into());
        }
        let committed = git(&["show", &format!("{head}:AGENTS.md")]).map_err(|_| differs)?;
        let mut current = Vec::new();
        let mut file = fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(checkout.join("AGENTS.md"))
            .map_err(|_| differs)?;
        if !file.metadata().is_ok_and(|meta| meta.is_file()) {
            return Err(differs.into());
        }
        file.read_to_end(&mut current).map_err(|_| differs)?;
        if super::without_generated_rules(committed.as_bytes())
            != super::without_generated_rules(&current)
        {
            return Err(differs.into());
        }
    }
    let mut untracked = vec!["ls-files", "--others", "--exclude-standard", "-z", "--"];
    untracked.extend(paths);
    if !git(&untracked)?.is_empty() {
        return Err("Local source has uncommitted files.".into());
    }
    Ok(())
}
