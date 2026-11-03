//! The operator's label for one run, written only by tink-sdlc's own `sdlc.py label` after an
//! explicit F2. A label records what the operator said about a result; it is not a verdict,
//! approval, gate or merge. The words are passed through verbatim, never trimmed or rewritten.

use crate::sdlc::{self, DEADLINE};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use std::{path::Path, process::Command, sync::atomic::AtomicBool};

/// tink-sdlc's label kinds, on keys 1 to 4.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    Correction,
    None,
    Accepted,
    Rejected,
}

impl Kind {
    pub(super) const ALL: [Kind; 4] =
        [Kind::Correction, Kind::None, Kind::Accepted, Kind::Rejected];

    pub(super) fn word(self) -> &'static str {
        match self {
            Kind::Correction => "correction",
            Kind::None => "none",
            Kind::Accepted => "accepted",
            Kind::Rejected => "rejected",
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
        if key.kind == KeyEventKind::Release
            || !matches!(key.modifiers, KeyModifiers::NONE | KeyModifiers::SHIFT)
        {
            return Action::Stay;
        }
        match key.code {
            KeyCode::Esc => return Action::Discard,
            KeyCode::F(2) if key.kind == KeyEventKind::Press => return Action::Save,
            KeyCode::Char(c @ '1'..='4') if self.kind.is_none() => {
                let kind = Kind::ALL[c as usize - '1' as usize];
                self.kind = Some(kind);
                if kind == Kind::None && self.words.is_empty() {
                    self.words = "No corrections".into();
                }
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

    /// The kind and words to record, or why the form isn't ready.
    pub(super) fn ready(&self) -> Result<(Kind, String), &'static str> {
        let kind = self
            .kind
            .ok_or("Choose a kind first: 1 correction, 2 none, 3 accepted, 4 rejected.")?;
        if self.words.trim().is_empty() {
            return Err("Type your words first; the label keeps them exactly as written.");
        }
        Ok((kind, self.words.clone()))
    }
}

/// Run the checkout's own `sdlc.py label`, signed with the checkout's `git config user.name`.
/// tink-sdlc decides; its answer or refusal comes back word for word.
pub(super) fn write(
    root: &Path,
    slug: &str,
    kind: Kind,
    words: &str,
    stop: &AtomicBool,
) -> Result<String, String> {
    let mut name = Command::new("git");
    name.args(["config", "user.name"]);
    let reviewer = sdlc::execute(root, name, stop, DEADLINE)
        .ok()
        .map(|out| out.trim().to_owned())
        .filter(|name| !name.is_empty())
        .ok_or(
            "No reviewer name: set git config user.name in this checkout, then press F2 again.",
        )?;
    let mut label = Command::new("python3");
    label.arg(sdlc::SCRIPT).args([
        "label",
        slug,
        "--kind",
        kind.word(),
        "--words",
        words,
        "--reviewer",
        &reviewer,
        "--source",
        "taste",
    ]);
    sdlc::execute(root, label, stop, DEADLINE)
        .map(|out| out.trim().to_owned())
        .map_err(|why| match why.split_once(": Error: ") {
            Some((_, said)) => format!("tink-sdlc said: {said}"),
            None => why,
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
    fn digits_choose_the_kind_then_every_character_is_the_operators_words() {
        let mut form = Form::default();
        assert!(form.ready().is_err());
        form.key(press(KeyCode::Char('1')));
        typed(&mut form, "Shape's check didn't run — 2 times? Ω");
        assert_eq!(
            form.ready().unwrap(),
            (
                Kind::Correction,
                "Shape's check didn't run — 2 times? Ω".into()
            )
        );
    }

    #[test]
    fn none_offers_no_corrections_and_backspace_on_empty_returns_to_the_kind() {
        let mut form = Form::default();
        form.key(press(KeyCode::Char('2')));
        assert_eq!(form.ready().unwrap(), (Kind::None, "No corrections".into()));
        for _ in 0.."No corrections".len() {
            form.key(press(KeyCode::Backspace));
        }
        assert!(form.ready().is_err(), "empty words are never saved");
        form.key(press(KeyCode::Backspace));
        assert_eq!(form.kind, None);
        form.key(press(KeyCode::Char('4')));
        assert_eq!(form.kind, Some(Kind::Rejected));
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
        let ctrl = KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL);
        form.key(press(KeyCode::Char('3')));
        form.key(ctrl);
        assert_eq!(form.words, "", "control chords never type");
    }
}
