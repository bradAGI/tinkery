//! Dictation inside think: the voice key, the take's muted text at the caret, inserting final
//! text, and the one voice notice. Voice text lands in the writing box and is sent only by F2,
//! like typed words.

use super::*;
use crate::voice::{Failure, Voice, VoiceHost, silence_filler};
use ratatui::text::{Line, Span, Text};
use std::time::Instant;

impl BrainDump {
    /// Enables dictation through `host`. Without it voice is never offered.
    pub fn with_voice(mut self, host: Box<dyn VoiceHost>) -> Self {
        self.voice = Some(Voice::new(host));
        self
    }
    pub(super) fn voice_active(&self) -> bool {
        self.voice.as_ref().is_some_and(Voice::active)
    }
    pub(super) fn voice_allowed(&self) -> bool {
        !self.quitting() && !self.screen.leave_prompt && !self.handoff.started()
    }
    pub(super) fn voice_available(&self) -> bool {
        self.voice_allowed() && self.voice.is_some()
    }
    pub(super) fn voice_key(&mut self, key: KeyEvent) -> bool {
        if self.voice_allowed()
            && key.kind == KeyEventKind::Press
            && key.modifiers.is_empty()
            && self.voice.as_ref().is_some_and(Voice::setup_prompt)
        {
            match key.code {
                KeyCode::Char('y' | 'Y') if self.below_minimum() => return true,
                KeyCode::Char('y' | 'Y' | 'n' | 'N') | KeyCode::Esc => {
                    self.voice
                        .as_mut()
                        .unwrap()
                        .answer_setup(matches!(key.code, KeyCode::Char('y' | 'Y')), Instant::now());
                    return true;
                }
                _ => {}
            }
        }
        if key.code == KeyCode::Esc && self.voice_active() && !self.screen.leave_prompt {
            self.voice.as_mut().unwrap().cancel();
            return true;
        }
        if key.code == KeyCode::F(2) && key.modifiers.is_empty() && self.voice_active() {
            self.voice.as_mut().unwrap().stop(true, Instant::now());
            return true;
        }
        let speak = key.code == KeyCode::F(6)
            || key.code == KeyCode::Char('Ω')
            || (matches!(key.code, KeyCode::Char('z' | 'Z'))
                && key
                    .modifiers
                    .contains(KeyModifiers::CONTROL | KeyModifiers::ALT));
        if !speak {
            return false;
        }
        if key.kind != KeyEventKind::Press || !self.voice_allowed() {
            return true;
        }
        if let Some(voice) = &mut self.voice {
            voice.toggle(Instant::now());
            // Dictation adds at the caret; it never deletes a selection.
            self.input.collapse();
            self.focus_writing();
            self.screen.help = false;
            self.screen.original = false;
            self.screen.details = false;
        }
        true
    }
    pub(super) fn voice_tick(&mut self) {
        let Some(result) = self.voice.as_mut().and_then(|v| v.tick(Instant::now())) else {
            return;
        };
        if !self.voice_allowed() {
            return;
        }
        let text = result.text;
        if silence_filler(&text) {
            self.voice.as_mut().unwrap().missed();
            return;
        }
        self.shaping.encoded_bytes = None;
        self.input.collapse();
        if let Err(error) = self.input.insert(&text) {
            self.voice
                .as_mut()
                .unwrap()
                .reject(Failure::new("capture", error));
            return;
        }
        self.focus_writing();
        self.conversation.forget_changes();
        if result.send {
            self.submit();
        }
    }
}

/// True while dictation shows a live preview in the writing box.
pub(super) fn previewing(app: &BrainDump) -> bool {
    !app.voice.as_ref().map_or("", Voice::partial).is_empty()
}

pub(super) fn input_text(
    app: &BrainDump,
    palette: Palette,
) -> (super::super::note::Wrapped, Text<'static>) {
    let partial = app.voice.as_ref().map_or("", Voice::partial);
    if partial.is_empty() {
        let wrapped = app.input.wrap(app.screen.input_area.width);
        let text = Text::styled(wrapped.lines.join("\n"), palette.ink);
        return (wrapped, text);
    }
    let start = app.input.cursor;
    let end = start + partial.len();
    let mut preview = Note::new(&format!(
        "{}{}{}",
        &app.input.text[..start],
        partial,
        &app.input.text[start..]
    ));
    preview.cursor = preview
        .text
        .grapheme_indices(true)
        .map(|(i, _)| i)
        .take_while(|i| *i <= start)
        .last()
        .unwrap_or(0);
    let wrapped = preview.wrap(app.screen.input_area.width);
    let mut lines = vec![Line::default(); wrapped.lines.len()];
    for ((at, g), (_, row, _)) in preview.text.grapheme_indices(true).zip(&wrapped.positions) {
        let safe = Note::display(g);
        let safe = safe.trim_end_matches('\n');
        // Raw positions, not substring searches in transformed or repeated display rows.
        lines[*row].spans.push(Span::styled(
            safe.to_owned(),
            if at < end && at + g.len() > start {
                palette.muted
            } else {
                palette.ink
            },
        ));
    }
    (wrapped, Text::from(lines))
}

pub(super) fn render_notice(frame: &mut Frame, app: &BrainDump, palette: Palette) {
    if !app.voice_allowed() || app.below_minimum() {
        return;
    }
    if let Some(voice) = &app.voice {
        frame.render_widget(
            Paragraph::new(voice.notice()).style(palette.muted),
            Rect::new(2, app.screen.area.height - 2, app.screen.area.width - 4, 1),
        );
    }
}
