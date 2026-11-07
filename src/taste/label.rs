//! The operator's label for one exact head: their answer to "Is this good?", pass or fail in their own
//! words, written only by tink-sdlc's own `sdlc.py label` after an explicit F2. The label is the
//! operator's verdict on the result; it approves, merges and gates nothing. The words are passed
//! through verbatim, never trimmed or rewritten, and signed with the name the form showed.

use super::read::{Check, commit};
use crate::sdlc::{self, DEADLINE};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use serde_json::Value;
use std::{
    fs, io::Read, os::unix::fs::OpenOptionsExt, path::Path, process::Command,
    sync::atomic::AtomicBool,
};

/// The operator's answer, on keys 1 and 2. Taste offers only these two of tink-sdlc's kinds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    Pass,
    Fail,
}

impl Kind {
    pub(super) const ALL: [Kind; 2] = [Kind::Pass, Kind::Fail];

    /// What the operator sees.
    pub(super) fn name(self) -> &'static str {
        match self {
            Kind::Pass => "pass",
            Kind::Fail => "fail",
        }
    }

    /// When to pick it, in one plain line.
    pub(super) fn when(self) -> &'static str {
        match self {
            Kind::Pass => "It does what you asked, and you'd keep it.",
            Kind::Fail => "Something is wrong or missing.",
        }
    }

    fn word(self) -> &'static str {
        match self {
            Kind::Pass => "accepted",
            Kind::Fail => "rejected",
        }
    }
}

/// Words of the latest rejection and whether that verdict recorded this commit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Verdict {
    pub(super) words: String,
    pub(super) pinned: bool,
}

/// Revision-bound verdict status and the latest usable rejection, from source-validated labels.
/// Unbound legacy labels remain readable but cannot cover a head; missing counts stay unknown.
pub(super) fn read(
    root: &Path,
    run: &Value,
    head: Option<&str>,
) -> Result<(Option<Check>, Option<Verdict>), String> {
    let kinds = &run["labels"]["kinds"];
    let Some((accepted, rejected)) = kinds["accepted"].as_u64().zip(kinds["rejected"].as_u64())
    else {
        return Ok((None, None));
    };
    if accepted == 0 && rejected == 0 {
        return Ok((Some(Check::AwaitingHuman), None));
    }
    let count = run["labels"]["count"]
        .as_u64()
        .ok_or("Unreadable label count.")?;
    let checkout = match run.get("checkout") {
        None => root,
        Some(value) => Path::new(value.as_str().ok_or("Verdict checkout is invalid.")?),
    };
    if !checkout.is_absolute() {
        return Err("Verdict checkout must be absolute.".into());
    }
    let path = checkout
        .join("runs")
        .join(run["slug"].as_str().ok_or("Run name missing.")?)
        .join("labels.jsonl");
    if path.canonicalize().ok().as_ref() != Some(&path) {
        return Err("Cannot read this run's verdict words in its checkout.".into());
    }
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW)
        .open(&path)
        .map_err(|error| error.to_string())?;
    if !file.metadata().is_ok_and(|m| m.is_file()) {
        return Err("The verdict words are not in a regular file.".into());
    }
    const LIMIT: u64 = 4 * 1024 * 1024;
    let mut bytes = Vec::new();
    file.take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > LIMIT || !bytes.ends_with(b"\n") {
        return Err("The verdict record is too large or incomplete.".into());
    }
    let mut status = Check::Stale;
    let mut latest = None;
    let mut total = 0;
    let (mut passes, mut failures) = (0, 0);
    for line in bytes[..bytes.len() - 1].split(|&byte| byte == b'\n') {
        let record: Value =
            serde_json::from_slice(line).map_err(|_| "Unreadable verdict record.")?;
        let judged = record["source"]
            .as_str()
            .and_then(|s| s.strip_prefix("taste@"));
        let judged = judged.and_then(commit);
        let exact = judged.is_some() && judged == head;
        match record["kind"].as_str() {
            Some("accepted" | "rejected") => {
                let pass = record["kind"] == "accepted";
                passes += u64::from(pass);
                failures += u64::from(!pass);
                if exact {
                    status = if pass { Check::Pass } else { Check::Fail };
                }
                latest = if !pass && (exact || record["source"] == "taste") {
                    Some(Verdict {
                        words: record["words"]
                            .as_str()
                            .filter(|s| !s.trim().is_empty())
                            .ok_or("Verdict words missing.")?
                            .to_owned(),
                        pinned: exact,
                    })
                } else {
                    None
                };
            }
            Some("correction" | "none") => {}
            _ => return Err("Unrecognized verdict record.".into()),
        }
        total += 1;
    }
    if total != count || passes != accepted || failures != rejected {
        return Err("The verdict changed while reading. Press r.".into());
    }
    Ok((Some(status), latest))
}

/// An unsent label. Dropping it records nothing.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Form {
    pub(super) kind: Option<Kind>,
    pub(super) words: String,
}

/// What a key asks of the form's owner.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Action {
    Stay,
    Discard,
    Save,
}

impl Form {
    /// Digits choose the kind until one is chosen; after that every character is the operator's
    /// words. Backspace on empty words returns to the kind. Only F2 saves; held F2 never repeats.
    pub(super) fn key(&mut self, key: KeyEvent) -> Action {
        // AltGr arrives as Ctrl+Alt on some terminals; it types, other chords never do.
        let altgr = KeyModifiers::CONTROL | KeyModifiers::ALT;
        let typing = matches!(key.modifiers, KeyModifiers::NONE | KeyModifiers::SHIFT)
            || matches!(key.code, KeyCode::Char(_)) && key.modifiers - KeyModifiers::SHIFT == altgr;
        if key.kind == KeyEventKind::Release || !typing {
            return Action::Stay;
        }
        match key.code {
            KeyCode::Esc => return Action::Discard,
            KeyCode::F(2) if key.kind == KeyEventKind::Press => return Action::Save,
            KeyCode::Char(c @ '1'..='2') if self.kind.is_none() => {
                self.kind = Some(Kind::ALL[c as usize - '1' as usize]);
            }
            KeyCode::Char(c) if self.kind.is_some() => self.words.push(c),
            KeyCode::Backspace if self.words.is_empty() => self.kind = None,
            KeyCode::Backspace => {
                self.words.pop();
            }
            _ => {}
        }
        Action::Stay
    }

    /// Pasted words, unchanged; ignored until a kind is chosen.
    pub(super) fn paste(&mut self, text: &str) {
        if self.kind.is_some() {
            self.words.push_str(text);
        }
    }

    /// The kind and words to record, or why the form isn't ready.
    pub(super) fn ready(&self) -> Result<(Kind, String), &'static str> {
        let kind = self.kind.ok_or("Choose first: 1 pass or 2 fail.")?;
        if self.words.trim().is_empty() {
            return Err("Type your words first; the label keeps them exactly as written.");
        }
        Ok((kind, self.words.clone()))
    }
}

/// The name a label from this checkout is signed with: its `git config user.name`.
pub(super) fn signer(root: &Path, stop: &AtomicBool) -> Result<String, String> {
    let mut name = Command::new("git");
    name.args(["config", "user.name"]);
    // git config exits 1 when the name is unset; any other failure is reported as itself.
    let unset = "No name to sign with. Set one in this checkout (git config user.name \"Your Name\"), then press F2.";
    match sdlc::execute(root, name, "git", stop, DEADLINE) {
        Ok(out) if !out.trim().is_empty() => Ok(out.trim().to_owned()),
        Ok(_) => Err(unset.into()),
        Err(why) if why.starts_with("git exited with exit status: 1") => Err(unset.into()),
        Err(why) => Err(why),
    }
}

/// Label only the reviewed run and head, signed with the name the form showed.
/// Unknown heads and checkouts that cannot enforce run identity refuse; no unchecked retry.
pub(super) fn write(
    root: &Path,
    row: &super::read::ResultRow,
    kind: Kind,
    words: &str,
    reviewer: &str,
    stop: &AtomicBool,
) -> Result<String, String> {
    // `--flag=value` keeps words such as `-v` or `--no-verify` from reading as options.
    let slug = &row.slug;
    let id = &row.id;
    let head = row
        .head
        .as_ref()
        .ok_or("Result revision unknown; press r before writing a verdict.")?;
    let source = format!("taste@{head}");
    let mut label = Command::new("python3");
    if let Some(local) = row.local() {
        local.recheck(stop)?;
        if local.root() != root {
            let read = |checkout: &Path| crate::seed_me::read_seed(&checkout.join(sdlc::SCRIPT));
            let named = read(local.root())
                .map_err(|_| "Cannot read the named checkout’s label command.")?;
            if !read(root).is_ok_and(|bytes| bytes == named) {
                return Err("The build checkout has a different label command. Inspect it and reopen Taste with --sdlc-root naming that checkout.".into());
            }
            // The candidate's module directory cannot replace Python's standard imports.
            label.arg("-I");
        }
    }
    label.arg(sdlc::SCRIPT).args([
        "label".to_owned(),
        slug.clone(),
        format!("--expected-run-id={id}"),
        format!("--kind={}", kind.word()),
        format!("--words={words}"),
        format!("--reviewer={reviewer}"),
        format!("--source={source}"),
    ]);
    sdlc::execute(root, label, "sdlc.py label", stop, DEADLINE)
        .map(|out| out.trim().to_owned())
        .map_err(|why| {
            if why.contains("unrecognized arguments: --expected-run-id") {
                return "This checkout needs a newer tink-sdlc to save a verdict safely. Update it, reread the result, and try again.".into();
            }
            match why.split_once(": Error: ") {
                Some((_, said)) => format!("tink-sdlc said: {said}"),
                None => why,
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn typed(form: &mut Form, text: &str) {
        for c in text.chars() {
            form.key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::SHIFT));
        }
    }

    fn recorded(records: &[Value], head: Option<&str>) -> (Option<Check>, Option<Verdict>) {
        let dir = tempfile::Builder::new()
            .prefix("tinkery-TEST-labels-")
            .tempdir()
            .unwrap();
        let root = dir.path().canonicalize().unwrap();
        let path = root.join("runs/TEST-label");
        fs::create_dir_all(&path).unwrap();
        fs::write(
            path.join("labels.jsonl"),
            records
                .iter()
                .map(|record| format!("{record}\n"))
                .collect::<String>(),
        )
        .unwrap();
        let run = serde_json::json!({
            "slug": "TEST-label", "labels": {"count": records.len(), "kinds": {
                "accepted": records.iter().filter(|r| r["kind"] == "accepted").count(),
                "rejected": records.iter().filter(|r| r["kind"] == "rejected").count()
            }}
        });
        read(&root, &run, head).unwrap()
    }

    #[test]
    fn only_exact_head_verdicts_count_and_legacy_words_stay_readable() {
        let head = "a".repeat(40);
        for kind in ["accepted", "rejected"] {
            for (source, current) in [
                ("taste".to_owned(), false),
                (format!("taste@{}", "b".repeat(40)), false),
                ("taste@aaaa".to_owned(), false),
                (format!("taste@{head}"), true),
            ] {
                let record =
                    serde_json::json!({"kind": kind, "source": source, "words": "  TEST Ω  "});
                let (status, rejection) = recorded(std::slice::from_ref(&record), Some(&head));
                let expected = match (current, kind) {
                    (true, "accepted") => Check::Pass,
                    (true, _) => Check::Fail,
                    (false, _) => Check::Stale,
                };
                assert_eq!(status, Some(expected), "{record}");
                if let Some(rejection) = rejection {
                    assert_eq!(rejection.words, "  TEST Ω  ");
                    assert_eq!(rejection.pinned, current);
                }
                assert_eq!(recorded(&[record], None).0, Some(Check::Stale));
            }
        }
        assert_eq!(
            recorded(&[], Some(&head)),
            (Some(Check::AwaitingHuman), None)
        );
    }

    #[test]
    fn unbound_and_other_head_labels_never_replace_a_current_verdict() {
        let head = "a".repeat(40);
        let bound = serde_json::json!({"kind": "rejected", "source": format!("taste@{head}"), "words": "TEST fail"});
        let note =
            serde_json::json!({"kind": "correction", "source": "TEST", "words": "TEST note"});
        assert_eq!(
            recorded(&[bound.clone(), note], Some(&head)),
            (
                Some(Check::Fail),
                Some(Verdict {
                    words: "TEST fail".into(),
                    pinned: true
                })
            )
        );
        for source in ["taste".to_owned(), format!("taste@{}", "b".repeat(40))] {
            let later = serde_json::json!({"kind": "accepted", "source": source, "words": "TEST other verdict"});
            assert_eq!(
                recorded(&[bound.clone(), later], Some(&head)),
                (Some(Check::Fail), None)
            );
        }
    }

    #[test]
    fn digits_choose_pass_or_fail_then_every_character_is_the_operators_words() {
        let mut form = Form::default();
        assert!(form.ready().is_err());
        form.key(press(KeyCode::Char('3')));
        assert_eq!(form.kind, None, "only pass and fail are offered");
        form.key(press(KeyCode::Char('1')));
        typed(&mut form, "Shape's check didn't run — 2 times? Ω");
        assert_eq!(
            form.ready().unwrap(),
            (Kind::Pass, "Shape's check didn't run — 2 times? Ω".into())
        );
        assert_eq!(
            (Kind::Pass.word(), Kind::Fail.word()),
            ("accepted", "rejected")
        );
    }

    #[test]
    fn empty_words_are_never_saved_and_backspace_on_empty_returns_to_the_choice() {
        let mut form = Form::default();
        form.key(press(KeyCode::Char('1')));
        assert!(form.ready().is_err(), "empty words are never saved");
        form.key(press(KeyCode::Backspace));
        assert!(form.ready().unwrap_err().contains("1 pass or 2 fail"));
        form.key(press(KeyCode::Char('2')));
        form.paste("pasted\n-v");
        assert_eq!(form.ready().unwrap(), (Kind::Fail, "pasted\n-v".into()));
    }

    #[test]
    fn only_a_pressed_f2_saves_and_esc_discards() {
        let mut form = Form::default();
        assert_eq!(form.key(press(KeyCode::Enter)), Action::Stay);
        let mut held = press(KeyCode::F(2));
        held.kind = KeyEventKind::Repeat;
        assert_eq!(form.key(held), Action::Stay);
        assert_eq!(form.key(press(KeyCode::F(2))), Action::Save);
        assert_eq!(form.key(press(KeyCode::Esc)), Action::Discard);
        form.key(press(KeyCode::Char('1')));
        form.key(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL));
        form.key(KeyEvent::new(
            KeyCode::Char('@'),
            KeyModifiers::CONTROL | KeyModifiers::ALT,
        ));
        assert_eq!(
            form.ready().unwrap(),
            (Kind::Pass, "@".into()),
            "AltGr types; Ctrl chords don't"
        );
    }
}
