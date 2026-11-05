//! The operator's label for one run: their answer to "Is this good?", pass or fail in their own
//! words, written only by tink-sdlc's own `sdlc.py label` after an explicit F2. The label is the
//! operator's verdict on the result; it approves, merges and gates nothing. The words are passed
//! through verbatim, never trimmed or rewritten, and signed with the name the form showed.

use crate::sdlc::{self, DEADLINE};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use std::{path::Path, process::Command, sync::atomic::AtomicBool};

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

    /// tink-sdlc's existing label kind.
    pub(super) fn word(self) -> &'static str {
        match self {
            Kind::Pass => "accepted",
            Kind::Fail => "rejected",
        }
    }
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
    match sdlc::execute(root, name, stop, DEADLINE) {
        Ok(out) if !out.trim().is_empty() => Ok(out.trim().to_owned()),
        Ok(_) => Err(unset.into()),
        Err(why) if why.starts_with("git exited with exit status: 1") => Err(unset.into()),
        Err(why) => Err(why),
    }
}

/// Ask the checkout to label only the reviewed run, signed with the name the form showed.
/// A checkout that cannot enforce its identity refuses; there is no unchecked retry.
pub(super) fn write(
    root: &Path,
    slug: &str,
    id: &str,
    kind: Kind,
    words: &str,
    reviewer: &str,
    stop: &AtomicBool,
) -> Result<String, String> {
    // `--flag=value` keeps words such as `-v` or `--no-verify` from reading as options.
    let mut label = Command::new("python3");
    label.arg(sdlc::SCRIPT).args([
        "label".to_owned(),
        slug.to_owned(),
        format!("--expected-run-id={id}"),
        format!("--kind={}", kind.word()),
        format!("--words={words}"),
        format!("--reviewer={reviewer}"),
        "--source=taste".to_owned(),
    ]);
    sdlc::execute(root, label, stop, DEADLINE)
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
