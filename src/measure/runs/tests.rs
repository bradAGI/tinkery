//! Closed runs in TEST checkouts with a fake `sdlc.py` and real Git; no network or providers.

use crate::{measure::Measure, repo::Repo};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend};
use serde_json::{Value, json};
use std::{
    fs,
    path::Path,
    process::Command,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const FAKE: &str = r#"import json, pathlib, sys
root = pathlib.Path(__file__).resolve().parents[2]
with open(root / 'calls.log', 'a') as log:
    log.write(' '.join(sys.argv[1:]) + '\n')
fixture = json.loads((root / 'fixture.json').read_text())
body = {'runs': fixture['list']} if len(sys.argv) == 3 else {'run': fixture['runs'][sys.argv[2]]}
print(json.dumps({'protocol': 'tink-sdlc', 'api_version': 1, **body}))
"#;

const DAY: u128 = 86_400 * 1_000_000_000;

fn now() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos()
}

fn git(root: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args([
            "-c",
            "user.name=TEST",
            "-c",
            "user.email=test@example.invalid",
        ])
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
}

fn commit(root: &Path, message: &str) -> String {
    git(root, &["commit", "--allow-empty", "-q", "-m", message]);
    git(root, &["rev-parse", "HEAD"])
}

fn label(kind: &str, words: &str) -> String {
    json!({"kind": kind, "words": words}).to_string() + "\n"
}

/// One run's status record and its files under runs/<slug>.
fn run(root: &Path, slug: &str, merge: Option<&str>, age: u128, labels: &[String]) -> Value {
    let folder = root.join("runs").join(slug);
    fs::create_dir_all(folder.join("steps")).unwrap();
    fs::write(folder.join("labels.jsonl"), labels.concat()).unwrap();
    let closure = match merge {
        Some(commit) => {
            json!({"outcome": "merged", "merge": commit, "time_ns": (now() - age) as u64})
        }
        None => json!({"outcome": "abandoned", "time_ns": (now() - age) as u64}),
    };
    json!({"slug": slug, "closure": closure, "labels": {"count": labels.len()},
        "pr": {"url": "https://github.com/TEST/repo.x/pull/1000"},
        "decisions": [], "steps": []})
}

fn step(root: &Path, slug: &str, name: &str, logs: &[(&str, usize)]) {
    let steps = root.join("runs").join(slug).join("steps");
    fs::write(
        steps.join(format!("{name}.json")),
        json!({"session": format!("s-{name}")}).to_string(),
    )
    .unwrap();
    for (kind, answers) in logs {
        let mut text = json!({"type": "session", "id": format!("s-{name}")}).to_string() + "\n";
        for _ in 0..*answers {
            text += &(json!({"type": "message_end", "message": {"role": "assistant"}}).to_string()
                + "\n");
            text += &(json!({"type": "message_end", "message": {"role": "toolResult"}})
                .to_string()
                + "\n");
        }
        text += "pi-launcher: a note that is not an event\n";
        fs::write(steps.join(format!("{name}.{kind}.pi.log")), text).unwrap();
    }
}

fn checkout() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::create_dir_all(root.join("_system/scripts")).unwrap();
    fs::write(root.join("_system/scripts/sdlc.py"), FAKE).unwrap();
    dir
}

fn write(root: &Path, runs: Vec<Value>) {
    let list: Vec<_> = runs
        .iter()
        .map(|r| json!({"slug": r["slug"], "closure": r["closure"]}))
        .chain([
            json!({"slug": "broken", "error": "unreadable"}),
            json!({"slug": "open", "closure": null}),
        ])
        .collect();
    let by_slug: serde_json::Map<_, _> = runs
        .into_iter()
        .map(|r| (r["slug"].as_str().unwrap().to_owned(), r))
        .collect();
    fs::write(
        root.join("fixture.json"),
        json!({"list": list, "runs": by_slug}).to_string(),
    )
    .unwrap();
}

fn press(view: &mut Measure, code: KeyCode) -> bool {
    view.key(KeyEvent::new(code, KeyModifiers::NONE))
}

fn screen(view: &mut Measure, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|f| view.render(f, f.area(), crate::Palette::new(true), false))
        .unwrap();
    let buffer = terminal.backend().buffer();
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn read(view: &mut Measure) -> String {
    assert!(!press(view, KeyCode::Char('p')));
    let until = Instant::now() + Duration::from_secs(20);
    loop {
        view.poll();
        let text = screen(view, 100, 30);
        if !text.contains("Reading closed runs") || Instant::now() > until {
            return text;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn row<'a>(text: &'a str, slug: &str) -> &'a str {
    text.lines()
        .find(|l| l.trim_end().ends_with(slug))
        .unwrap_or_else(|| panic!("no row for {slug} in\n{text}"))
}

#[test]
fn counts_come_from_records_and_unknown_is_never_zero() {
    let dir = checkout();
    let root = &dir.path().canonicalize().unwrap();
    let base = commit(root, "base");
    let reverted = commit(root, "reverted change");
    let by_pr = commit(root, "reverted through GitHub");
    let kept = commit(root, "kept change");
    let recent = commit(root, "recent change");
    commit(root, &format!("Revert\n\nThis reverts commit {reverted}."));
    // `by-pr` is PR #51 and GitHub reverted it; `gated-run` is PR #5 and must not match #51.
    commit(root, "Revert\n\nReverts TEST/repo.x#51");
    let mut gated = run(
        root,
        "gated-run",
        Some(&kept),
        30 * DAY,
        &[
            label("correction", "Stop: it edited the locked test"),
            label("correction", "Second: wrong file"),
            label("accepted", "Looks right"),
        ],
    );
    gated["decisions"] = json!([
        {"stage": 1, "decision": "approved", "reviewer": "op"},
        {"stage": 3, "decision": "approved", "reviewer": "seed", "basis": "confirmed-seed"},
        {"stage": 2, "decision": "changes-requested", "reviewer": "pi", "attribution": "agent-recorded"},
        {"stage": 2, "decision": "approved", "reviewer": "op"}
    ]);
    gated["pr"]["url"] = json!("https://github.com/TEST/repo.x/pull/5");
    gated["steps"] = json!([{"name": "build"}, {"name": "fix"}]);
    step(
        root,
        "gated-run",
        "build",
        &[("launch-100", 3), ("resume-200", 2)],
    );
    step(root, "gated-run", "fix", &[("launch-300", 1)]);
    let mut nolog = run(
        root,
        "no-log",
        Some(&recent),
        DAY,
        &[label("none", "No corrections")],
    );
    nolog["steps"] = json!([{"name": "build"}, {"name": "claude"}]);
    step(root, "no-log", "build", &[("launch-1", 4)]);
    step(root, "no-log", "claude", &[]);
    let mismatch = {
        let mut r = run(
            root,
            "mismatch",
            Some(&reverted),
            40 * DAY,
            &[label("correction", "x")],
        );
        r["labels"]["count"] = json!(2);
        r
    };
    let mut by_pr_run = run(
        root,
        "by-pr",
        Some(&by_pr),
        50 * DAY,
        &[label("none", "fine")],
    );
    by_pr_run["pr"]["url"] = json!("https://github.com/TEST/repo.x/pull/51");
    let foreign = run(
        root,
        "elsewhere",
        Some(&"f".repeat(40)),
        60 * DAY,
        &[label("none", "fine")],
    );
    let dropped = run(
        root,
        "dropped",
        None,
        70 * DAY,
        &[label("correction", "wrong goal")],
    );
    let _ = base;
    write(
        root,
        vec![gated, nolog, mismatch, by_pr_run, foreign, dropped],
    );

    let mut view = Measure::open(Repo::enclosing(root));
    let calls = || fs::read_to_string(root.join("calls.log")).unwrap_or_default();
    assert!(screen(&mut view, 100, 30).contains("p runs"));
    assert_eq!(calls(), "", "opening Measure read runs");
    let text = read(&mut view);
    assert!(
        text.contains("6 closed runs · 5 merged · 2 reverted · 1 unreadable"),
        "{text}"
    );
    let words = |slug| {
        row(&text, slug)
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    };
    assert_eq!(words("gated-run"), "2 2 6 held gated-run");
    assert!(
        row(&text, "by-pr").contains("?      reverted"),
        "no steps is unknown, not zero"
    );
    assert_eq!(
        words("no-log"),
        "> 0 0 ? too recent no-log",
        "a step without a log must read unknown"
    );
    assert_eq!(
        words("mismatch"),
        "0 ? ? reverted mismatch",
        "a label count mismatch is unknown"
    );
    assert!(row(&text, "by-pr").contains("reverted"));
    assert!(row(&text, "elsewhere").contains("?"));
    assert!(row(&text, "dropped").contains("abandoned"));
    assert_eq!(
        calls().lines().count(),
        7,
        "one list read and one read per closed run"
    );

    press(&mut view, KeyCode::Down);
    press(&mut view, KeyCode::Enter);
    let details = screen(&mut view, 100, 30);
    for expected in [
        "Merged as ",
        "Held: not reverted on HEAD, closed 14+ days ago",
        "Gates: 2 by you",
        "stage 3 approved · seed · automatic seed binding, not counted",
        "stage 2 changes-requested · pi · agent-recorded, not counted",
        "Corrections: 2 · all labels, newest first",
        "  build: 5",
        "  fix: 1",
        "Driving agent responses: not recorded",
    ] {
        assert!(
            details.contains(expected),
            "missing {expected:?} in\n{details}"
        );
    }
    let order = |needle| details.find(needle).unwrap();
    assert!(order("accepted: Looks right") < order("correction: Second: wrong file"));
    assert!(
        order("correction: Second: wrong file")
            < order("correction: Stop: it edited the locked test")
    );
    assert!(!press(&mut view, KeyCode::Esc), "Esc closes details first");
    assert!(
        !press(&mut view, KeyCode::Esc),
        "Esc returns to pull requests"
    );
    assert!(screen(&mut view, 100, 30).contains("r read GitHub"));
    assert!(press(&mut view, KeyCode::Esc), "Esc then leaves Measure");
    assert_eq!(calls().lines().count(), 7, "navigation started a read");
}

#[test]
fn only_a_plain_p_reads_and_failures_stay_unknown() {
    let dir = checkout();
    let root = &dir.path().canonicalize().unwrap();
    let mut view = Measure::open(Repo::enclosing(root));
    let mut repeat = KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE);
    repeat.kind = KeyEventKind::Repeat;
    view.key(repeat);
    view.key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL));
    assert!(!screen(&mut view, 80, 24).contains("Reading closed runs"));
    // No fixture: the fake fails, and nothing reads as a count.
    let text = read(&mut view);
    assert!(
        text.contains("Results unknown; press p to retry."),
        "{text}"
    );
    assert!(!text.contains("closed runs ·"));
}
