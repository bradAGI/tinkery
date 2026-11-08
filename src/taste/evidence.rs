//! Actionable source evidence without opening a log or executing a check. Paths are lexical
//! references within the named checkout, not existence claims; receipt digests are never paths.

use serde_json::Value;
use std::path::{Path, PathBuf};

fn clean_path(s: &str) -> bool {
    !s.chars()
        .any(|c| c.is_control() || matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'))
}

fn resolve(root: &Path, relative: &str) -> Result<PathBuf, String> {
    if !root.is_absolute() || !root.to_str().is_some_and(clean_path) {
        return Err("Evidence checkout location is unsafe or unavailable.".into());
    }
    if relative == "." {
        return Ok(root.to_path_buf());
    }
    if relative.is_empty()
        || !clean_path(relative)
        || relative.contains(['\\', ':'])
        || relative
            .split('/')
            .any(|part| matches!(part, "" | "." | ".."))
        || Path::new(relative).is_absolute()
    {
        return Err("Evidence path must stay within the named checkout.".into());
    }
    Ok(root.join(relative))
}

fn checking_directory(root: &Path, cwd: &str) -> Result<PathBuf, String> {
    let mut resolved = resolve(root, ".")?;
    let drive_prefix = cwd.as_bytes().first().is_some_and(u8::is_ascii_alphabetic)
        && cwd.as_bytes().get(1) == Some(&b':');
    if cwd.trim().is_empty()
        || !clean_path(cwd)
        || Path::new(cwd).is_absolute()
        || cwd.contains('\\')
        || cwd.contains("://")
        || drive_prefix
        || cwd.split('/').any(|part| part == "..")
    {
        return Err("Checking cwd must be checkout-relative without traversal or controls.".into());
    }
    for part in cwd.split('/').filter(|part| !matches!(*part, "" | ".")) {
        resolved.push(part);
    }
    Ok(resolved)
}

/// Retain the API 1 log pointer, exact check argv/cwd and optional receipt SHA-256. No log text
/// is copied, paths are never opened, and missing or unsafe evidence cannot justify a pass.
pub(super) fn automated(
    root: &Path,
    slug: &str,
    report: &crate::sdlc::Run,
    condition: &crate::sdlc::CheckItem,
) -> Result<String, String> {
    let run = &report.data;
    let item = &condition.data;
    let relative = run["log"]["path"]
        .as_str()
        .ok_or("Source log pointer unavailable.")?;
    let log = resolve(root, relative)?;
    let run_root = root.join("runs").join(slug);
    if !log.starts_with(&run_root) || log == run_root {
        return Err("Source log pointer does not belong to this run.".into());
    }
    let argv = item["check"]["argv"]
        .as_array()
        .filter(|a| {
            a.first()
                .and_then(Value::as_str)
                .is_some_and(|s| !s.is_empty())
                && a.iter()
                    .all(|v| v.as_str().is_some_and(|s| !s.contains('\0')))
        })
        .ok_or("Automatic checking argv unavailable or malformed.")?;
    let (cwd, default) = match item["check"].get("cwd") {
        None => (".", " (API default)"),
        Some(value) => (value.as_str().ok_or("Checking cwd unavailable.")?, ""),
    };
    let resolved_cwd = checking_directory(root, cwd)?;
    let digest = match run["verification"].get("log") {
        None | Some(Value::Null) => String::new(),
        Some(value) => {
            let digest = value
                .as_str()
                .filter(|s| {
                    s.len() == 64
                        && s.bytes()
                            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                })
                .ok_or("Verification log digest is malformed; it is not a path.")?;
            format!("; receipt log SHA-256: {digest}")
        }
    };
    let argv = serde_json::to_string(argv).map_err(|_| "Checking argv unreadable.")?;
    let cwd = serde_json::to_string(cwd).map_err(|_| "Checking cwd unreadable.")?;
    Ok(format!(
        "Log pointer: {}; argv: {argv}; cwd: {cwd}{default} (resolved {}){digest}",
        log.display(),
        resolved_cwd.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn automated(root: &Path, slug: &str, run: &Value, item: &Value) -> Result<String, String> {
        let parsed = crate::sdlc::Run::parse(run);
        let condition = crate::sdlc::Run::parse(&json!({"checklist": [item]}));
        super::automated(
            root,
            slug,
            &parsed,
            &condition.checklist.as_ref().unwrap()[0],
        )
    }

    fn run() -> Value {
        serde_json::from_str::<Value>(include_str!("../../tests/fixtures/taste-api1.json")).unwrap()
            ["detail"]["run"]
            .clone()
    }

    #[test]
    fn api1_log_pointer_method_and_digest_survive_without_reading_or_dumping_the_log() {
        let run = run();
        let evidence = automated(
            Path::new("/TEST/unopened-checkout"),
            "TEST-result",
            &run,
            &run["checklist"][0],
        )
        .unwrap();
        assert!(
            evidence
                .contains("/TEST/unopened-checkout/runs/TEST-result/04-test/output/test-log.md"),
            "{evidence}"
        );
        assert!(
            evidence.contains(r#"argv: ["python3","TEST-allocation-check.py"]"#),
            "{evidence}"
        );
        assert!(
            evidence.contains(r#"cwd: ".""#)
                && evidence.contains("resolved /TEST/unopened-checkout"),
            "{evidence}"
        );
        assert!(evidence.contains(
            "receipt log SHA-256: 12b88a9685e1e46a893e9a3e59434d88c663774964346f41ccb1289aa3834d4b"
        ));
        assert!(!evidence.contains("TEST-checked-allocation-success"));
        assert!(!evidence.contains("https://"));
    }

    #[test]
    fn missing_unsafe_cross_run_paths_or_methods_never_supply_evidence() {
        for path in [
            json!(null),
            json!(""),
            json!("/etc/passwd"),
            json!("../outside"),
            json!("runs/TEST-result/../other/log"),
            json!("runs/other/04-test/output/test-log.md"),
            json!("https://example.test/log"),
            json!("runs/TEST-result/log\n"),
        ] {
            let mut run = run();
            run["log"]["path"] = path;
            assert!(
                automated(
                    Path::new("/TEST"),
                    "TEST-result",
                    &run,
                    &run["checklist"][0]
                )
                .is_err()
            );
        }
        for argv in [
            json!([]),
            json!(null),
            json!([1]),
            json!([""]),
            json!(["check\u{0000}"]),
        ] {
            let mut run = run();
            run["checklist"][0]["check"]["argv"] = argv;
            assert!(
                automated(
                    Path::new("/TEST"),
                    "TEST-result",
                    &run,
                    &run["checklist"][0]
                )
                .is_err()
            );
        }
        for cwd in [
            "..",
            "/outside",
            "sub/../../outside",
            "https://example.test",
        ] {
            let mut run = run();
            run["checklist"][0]["check"]["cwd"] = json!(cwd);
            assert!(
                automated(
                    Path::new("/TEST"),
                    "TEST-result",
                    &run,
                    &run["checklist"][0]
                )
                .is_err()
            );
        }
    }

    #[test]
    fn checking_cwd_normalizes_harmless_source_spellings_but_logs_remain_strict() {
        for (source, expected) in [
            (".", "/TEST"),
            ("./", "/TEST"),
            (".///.", "/TEST"),
            ("./src", "/TEST/src"),
            ("src/", "/TEST/src"),
            ("./src//checks/./", "/TEST/src/checks"),
            ("src:name/", "/TEST/src:name"),
        ] {
            assert_eq!(
                checking_directory(Path::new("/TEST"), source).unwrap(),
                Path::new(expected)
            );
            let mut run = run();
            run["checklist"][0]["check"]["cwd"] = json!(source);
            let evidence = automated(
                Path::new("/TEST"),
                "TEST-result",
                &run,
                &run["checklist"][0],
            )
            .unwrap();
            assert!(
                evidence.contains(&format!(
                    "cwd: {} (resolved {expected})",
                    serde_json::to_string(source).unwrap()
                )),
                "{evidence}"
            );
        }
        for source in [
            "",
            " ",
            "/src",
            "//src",
            "../src",
            "src/../checks",
            "src//..//checks",
            "C:/src",
            "C:src",
            "src\\checks",
            "src\n",
            "src\t",
            "src\u{202e}",
        ] {
            assert!(
                checking_directory(Path::new("/TEST"), source).is_err(),
                "{source:?}"
            );
        }
        for source in [
            "./runs/TEST-result/log",
            "runs//TEST-result/log",
            "runs/TEST-result/log/",
        ] {
            assert!(
                resolve(Path::new("/TEST"), source).is_err(),
                "log validation was loosened: {source}"
            );
        }
    }

    #[test]
    fn digest_is_optional_but_never_a_path_and_api_default_cwd_is_explicit() {
        let mut run = run();
        run["verification"].as_object_mut().unwrap().remove("log");
        run["checklist"][0]["check"]
            .as_object_mut()
            .unwrap()
            .remove("cwd");
        let evidence = automated(
            Path::new("/TEST"),
            "TEST-result",
            &run,
            &run["checklist"][0],
        )
        .unwrap();
        assert!(!evidence.contains("SHA-256") && evidence.contains("API default"));
        run["verification"]["log"] = json!("runs/TEST-result/04-test/output/test-log.md");
        assert!(
            automated(
                Path::new("/TEST"),
                "TEST-result",
                &run,
                &run["checklist"][0]
            )
            .is_err()
        );
    }
}
