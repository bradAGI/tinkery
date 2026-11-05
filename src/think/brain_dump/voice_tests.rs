use super::*;
use crate::voice::{Command, Event, Failure, VoiceHost};
use std::{cell::RefCell, collections::VecDeque, rc::Rc};

#[derive(Default)]
struct Fake {
    events: VecDeque<Event>,
    commands: Vec<Command>,
    closed: usize,
    restarted: usize,
}
impl VoiceHost for Rc<RefCell<Fake>> {
    fn send(&mut self, command: Command) -> Result<(), Failure> {
        self.borrow_mut().commands.push(command);
        Ok(())
    }
    fn poll(&mut self) -> Option<Event> {
        self.borrow_mut().events.pop_front()
    }
    fn shutdown(&mut self) {
        self.borrow_mut().closed += 1;
    }
    fn restart(&mut self) {
        self.borrow_mut().restarted += 1;
    }
}
fn attach(a: BrainDump) -> (BrainDump, Rc<RefCell<Fake>>) {
    let fake = Rc::new(RefCell::new(Fake::default()));
    fake.borrow_mut()
        .events
        .push_back(Event::Ready { version: 1 });
    let mut a = a.with_voice(Box::new(fake.clone()));
    a.tick();
    (a, fake)
}
fn key(a: &mut BrainDump, code: KeyCode) {
    a.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
}
fn event(a: &mut BrainDump, fake: &Rc<RefCell<Fake>>, event: Event) {
    fake.borrow_mut().events.push_back(event);
    a.tick();
}
fn start(a: &mut BrainDump, fake: &Rc<RefCell<Fake>>) {
    key(a, KeyCode::F(6));
    event(a, fake, Event::Listening { take: 1 });
}

#[test]
fn raw_display_mapping_styles_fake_voice_without_searching_transformed_rows() {
    let raw = "same\r\n\u{1b}[0m\tsame\r\nsame";
    let partial = "preview\t\u{1b}[31m\0中";
    for width in [2, 32, 72, 96, 156, 316] {
        let (mut a, fake) = attach(BrainDump::default());
        a.input = Note::new(raw);
        a.input.cursor = 6;
        a.screen.input_area.width = width;
        start(&mut a, &fake);
        event(
            &mut a,
            &fake,
            Event::Partial {
                take: 1,
                text: partial.into(),
            },
        );
        let palette = Palette::new(true);
        let (_, text) = voice_input::input_text(&a, palette);
        let expected = Note::new(&format!("{}{}{}", &raw[..6], partial, &raw[6..])).wrap(width);
        assert_eq!(text.to_string(), expected.lines.join("\n"));
        let muted = text
            .lines
            .iter()
            .flat_map(|l| &l.spans)
            .filter(|s| s.style == palette.muted)
            .map(|s| s.content.as_ref())
            .collect::<String>();
        assert_eq!(muted, Note::display(partial));
        assert_eq!(a.input.text, raw);
        assert!(
            !text
                .to_string()
                .chars()
                .any(|c| c.is_control() && c != '\n')
        );
        let before = a.encoded_bytes().unwrap();
        event(
            &mut a,
            &fake,
            Event::Final {
                take: 1,
                text: partial.into(),
            },
        );
        assert!(a.shaping.encoded_bytes.is_none());
        let fresh = a.preview_send(&a.input.text).unwrap().bytes;
        assert_eq!(a.encoded_bytes(), Some(fresh));
        assert!(fresh > before);
        assert_eq!(
            a.input.text,
            format!("{}{}{}", &raw[..6], partial, &raw[6..])
        );
        assert!(a.conversation.sources().is_empty());
    }
}

#[test]
fn partial_and_final_follow_current_cursor_not_a_captured_anchor() {
    let (mut a, fake) = attach(BrainDump::default());
    a.input = Note::new("A🙂Z");
    a.input.cursor = 1;
    start(&mut a, &fake);
    event(
        &mut a,
        &fake,
        Event::Partial {
            take: 1,
            text: "ghost".into(),
        },
    );
    assert_eq!(a.input.text, "A🙂Z");
    key(&mut a, KeyCode::Right);
    a.paste("Typed ");
    assert!(
        snapshot(100, 30, &mut a, false)
            .unwrap()
            .contains("Typed ghostZ")
    );
    let (_, preview) = voice_input::input_text(&a, Palette::new(false));
    assert_eq!(preview.to_string(), "A🙂Typed ghostZ");
    assert!(!a.copy_text().contains("ghost"));
    assert!(a.conversation.sources().is_empty());
    assert!(a.conversation.fragments().is_empty());
    event(
        &mut a,
        &fake,
        Event::Final {
            take: 1,
            text: "said".into(),
        },
    );
    assert_eq!(a.input.text, "A🙂Typed saidZ");
    assert_eq!(a.input.cursor, "A🙂Typed said".len());
    assert!(a.conversation.sources().is_empty());
    assert!(a.conversation.board().is_none());
    assert!(!a.running());
}
#[test]
fn edit_delete_paste_and_replaced_partial_preserve_ordinary_text() {
    let (mut a, fake) = attach(BrainDump::default());
    a.input = Note::new("left right");
    a.input.cursor = 5;
    start(&mut a, &fake);
    event(
        &mut a,
        &fake,
        Event::Partial {
            take: 1,
            text: "old".into(),
        },
    );
    key(&mut a, KeyCode::Delete);
    key(&mut a, KeyCode::Backspace);
    a.paste(" ↔ ");
    event(
        &mut a,
        &fake,
        Event::Partial {
            take: 1,
            text: "new".into(),
        },
    );
    let text = snapshot(100, 30, &mut a, false).unwrap();
    assert!(text.contains("left ↔ newight"));
    assert!(!text.contains("old"));
    event(
        &mut a,
        &fake,
        Event::Final {
            take: 1,
            text: "final".into(),
        },
    );
    assert_eq!(a.input.text, "left ↔ finalight");
}
#[test]
fn actual_partial_cells_are_muted_and_final_cells_are_ink_in_color_and_no_color() {
    for no_color in [false, true] {
        for (w, h) in [(80, 24), (100, 30), (160, 40), (320, 40)] {
            let (mut a, fake) = attach(BrainDump::default());
            a.input = Note::new("ABCD");
            a.input.cursor = 2;
            start(&mut a, &fake);
            event(
                &mut a,
                &fake,
                Event::Partial {
                    take: 1,
                    text: "ghost".into(),
                },
            );
            let mut terminal =
                ratatui::Terminal::new(ratatui::backend::TestBackend::new(w, h)).unwrap();
            let p = Palette::new(no_color);
            terminal.draw(|f| render(f, &mut a, p)).unwrap();
            let b = terminal.backend().buffer();
            for x in 6..11 {
                assert_eq!(
                    b[(x, 8)].symbol(),
                    &"ghost"[(x - 6) as usize..(x - 5) as usize]
                );
                if no_color {
                    assert!(b[(x, 8)].modifier.contains(ratatui::style::Modifier::DIM));
                } else {
                    assert_eq!(b[(x, 8)].fg, p.muted.fg.unwrap());
                }
            }
            assert!(!b[(4, 8)].modifier.contains(ratatui::style::Modifier::DIM));
            assert!(!b[(11, 8)].modifier.contains(ratatui::style::Modifier::DIM));
            event(
                &mut a,
                &fake,
                Event::Final {
                    take: 1,
                    text: "ghost".into(),
                },
            );
            terminal.draw(|f| render(f, &mut a, p)).unwrap();
            for x in 6..11 {
                let cell = &terminal.backend().buffer()[(x, 8)];
                assert!(!cell.modifier.contains(ratatui::style::Modifier::DIM));
                if !no_color {
                    assert_eq!(cell.fg, p.ink.fg.unwrap());
                }
            }
        }
    }
}
#[test]
fn unicode_combining_partial_and_wrapping_do_not_modify_or_split_input() {
    let (mut a, fake) = attach(BrainDump::default());
    a.input = Note::new("e tail 👩‍💻");
    a.input.cursor = 1;
    start(&mut a, &fake);
    for partial in ["\u{301}", "\n你好 👩‍💻", "word ".repeat(100).as_str()] {
        event(
            &mut a,
            &fake,
            Event::Partial {
                take: 1,
                text: partial.into(),
            },
        );
        snapshot(80, 24, &mut a, true).unwrap();
        assert_eq!(a.input.text, "e tail 👩‍💻");
    }
    event(
        &mut a,
        &fake,
        Event::Final {
            take: 1,
            text: "\u{301}".into(),
        },
    );
    assert_eq!(a.input.text, "e\u{301} tail 👩‍💻");
    assert_eq!(a.input.cursor, 3);
}
#[test]
fn aliases_never_undo_skip_or_insert_omega_and_repeats_do_not_toggle() {
    for (code, mods) in [
        (
            KeyCode::Char('z'),
            KeyModifiers::CONTROL | KeyModifiers::ALT,
        ),
        (KeyCode::Char('Ω'), KeyModifiers::NONE),
        (
            KeyCode::Char('Ω'),
            KeyModifiers::CONTROL | KeyModifiers::ALT,
        ),
    ] {
        let (mut a, fake) = attach(BrainDump::default());
        a.conversation.skipped_mut().push(Question {
            suggestion: None,
            target: None,
            id: "keep".into(),
            text: "Keep this skipped?".into(),
        });
        a.handle_key(KeyEvent::new(code, mods));
        assert_eq!(a.conversation.skipped().len(), 1);
        assert!(a.input.text.is_empty());
        assert_eq!(fake.borrow().commands, vec![Command::Start { take: 1 }]);
        let mut repeat = KeyEvent::new(code, mods);
        repeat.kind = KeyEventKind::Repeat;
        a.handle_key(repeat);
        assert_eq!(fake.borrow().commands.len(), 1);
        assert_eq!(a.conversation.skipped().len(), 1);
        assert!(a.voice_active());
    }
}
#[test]
fn esc_cancels_voice_before_shaping_and_drops_all_late_callbacks() {
    let (mut a, fake) = attach(BrainDump::default());
    a.paste("typed");
    start(&mut a, &fake);
    key(&mut a, KeyCode::F(2));
    key(&mut a, KeyCode::Esc);
    event(
        &mut a,
        &fake,
        Event::Final {
            take: 1,
            text: "late".into(),
        },
    );
    assert_eq!(a.input.text, "typed");
    assert!(a.conversation.sources().is_empty());
    assert!(!a.running());
    assert_eq!(
        fake.borrow().commands,
        vec![
            Command::Start { take: 1 },
            Command::Stop { take: 1 },
            Command::Cancel { take: 1 }
        ]
    );
}
#[test]
fn f2_waits_for_final_then_submits_exactly_once_with_intact_source() {
    let (mut a, fake) = attach(BrainDump::default());
    a.paste("typed ");
    key(&mut a, KeyCode::F(6));
    key(&mut a, KeyCode::F(2));
    key(&mut a, KeyCode::F(2));
    assert!(a.conversation.sources().is_empty());
    assert!(!a.running());
    event(&mut a, &fake, Event::Listening { take: 1 });
    event(
        &mut a,
        &fake,
        Event::Partial {
            take: 1,
            text: "temporary".into(),
        },
    );
    event(
        &mut a,
        &fake,
        Event::Final {
            take: 1,
            text: "spoken".into(),
        },
    );
    assert_eq!(a.conversation.sources().len(), 1);
    assert_eq!(a.conversation.sources()[0].id, 1);
    assert_eq!(a.conversation.sources()[0].text, "typed spoken");
    assert!(a.conversation.sources()[0].in_reply_to.is_none());
    event(
        &mut a,
        &fake,
        Event::Final {
            take: 1,
            text: "duplicate".into(),
        },
    );
    assert_eq!(a.conversation.sources().len(), 1);
    assert!(a.input.text.is_empty());
    assert_eq!(
        fake.borrow().commands,
        vec![Command::Start { take: 1 }, Command::Stop { take: 1 }]
    );
    assert!(a.handoff.receipt().is_none());
    assert!(a.handoff.review().is_none());
}
#[test]
fn direct_submit_uses_the_same_wait_for_final_contract() {
    let (mut a, fake) = attach(BrainDump::default());
    start(&mut a, &fake);
    a.submit();
    assert!(a.conversation.sources().is_empty());
    event(
        &mut a,
        &fake,
        Event::Final {
            take: 1,
            text: "spoken".into(),
        },
    );
    assert_eq!(a.conversation.sources()[0].text, "spoken");
}
#[test]
fn every_failure_is_one_muted_actionable_line_and_does_not_send_typed_words() {
    for kind in [
        "no-node",
        "install",
        "download",
        "config",
        "no-model",
        "engine-load",
        "mic-permission",
        "mic-unavailable",
        "mic-busy",
    ] {
        let (mut a, fake) = attach(BrainDump::default());
        a.paste("typed");
        start(&mut a, &fake);
        key(&mut a, KeyCode::F(2));
        event(
            &mut a,
            &fake,
            Event::Error {
                take: Some(1),
                kind: kind.into(),
                message: "Action required.".into(),
            },
        );
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 30)).unwrap();
        terminal
            .draw(|f| render(f, &mut a, Palette::new(true)))
            .unwrap();
        let b = terminal.backend().buffer();
        let notice = find_on_screen(b, "Action required.");
        assert!(b[notice].modifier.contains(ratatui::style::Modifier::DIM));
        assert_eq!(a.voice.as_ref().unwrap().failure().unwrap().kind, kind);
        assert_eq!(a.input.text, "typed");
        assert!(a.conversation.sources().is_empty());
        event(
            &mut a,
            &fake,
            Event::Final {
                take: 1,
                text: "late".into(),
            },
        );
        assert_eq!(a.input.text, "typed");
        assert!(a.conversation.sources().is_empty());
    }
}
#[test]
fn empty_fillers_never_send_or_overwrite() {
    for final_text in ["", "thank you", "YOU.", "."] {
        let (mut a, fake) = attach(BrainDump::default());
        a.paste("typed");
        start(&mut a, &fake);
        key(&mut a, KeyCode::F(2));
        event(
            &mut a,
            &fake,
            Event::Final {
                take: 1,
                text: final_text.into(),
            },
        );
        assert_eq!(a.input.text, "typed");
        assert!(a.conversation.sources().is_empty());
        assert!(!a.running());
        let notice = a.voice.as_ref().unwrap().notice();
        assert_eq!(notice, "Didn't catch that.");
    }
}
#[test]
fn large_fake_voice_final_uses_editor_storage_not_request_policy() {
    let (mut a, fake) = attach(BrainDump::default());
    a.paste("typed ");
    let raw = "spoken 中\r\n\t\0".repeat(400);
    assert!(raw.len() > 4096);
    start(&mut a, &fake);
    event(
        &mut a,
        &fake,
        Event::Final {
            take: 1,
            text: raw.clone(),
        },
    );
    assert_eq!(a.input.text, format!("typed {raw}"));
    assert!(a.conversation.sources().is_empty() && !a.running());
    assert!(!a.voice.as_ref().unwrap().notice().contains("4096"));
}

#[test]
fn final_preserves_raw_words_and_does_not_grant_confirmation() {
    let (mut a, fake) = attach(BrainDump::default());
    start(&mut a, &fake);
    event(
        &mut a,
        &fake,
        Event::Final {
            take: 1,
            text: "a\r\nb\u{0}confirm\u{1b}".into(),
        },
    );
    assert_eq!(a.input.text, "a\r\nb\u{0}confirm\u{1b}");
    let shown = snapshot(100, 30, &mut a, true).unwrap();
    assert!(!shown.contains('\u{1b}') && !shown.contains('\u{0}'));
    assert!(a.conversation.sources().is_empty());
    assert!(a.handoff.receipt().is_none());
    assert!(a.handoff.review().is_none());
}
#[test]
fn leaving_closes_helper_preserves_unsaved_guard_and_can_reload_after_n() {
    let (mut a, fake) = attach(BrainDump::default());
    a.paste("typed");
    start(&mut a, &fake);
    key(&mut a, KeyCode::Esc);
    assert!(
        !a.voice_active() && !a.screen.leave_prompt,
        "Esc cancels the take first"
    );
    key(&mut a, KeyCode::Esc);
    assert!(a.screen.leave_prompt);
    assert!(!a.quitting());
    assert!(fake.borrow().closed > 0);
    assert!(!a.voice_active());
    event(
        &mut a,
        &fake,
        Event::Final {
            take: 1,
            text: "late".into(),
        },
    );
    key(&mut a, KeyCode::Char('n'));
    assert_eq!(a.input.text, "typed");
    key(&mut a, KeyCode::F(6));
    assert_eq!(fake.borrow().restarted, 1);
    assert!(!a.voice_active());
    event(&mut a, &fake, Event::Ready { version: 1 });
    key(&mut a, KeyCode::F(6));
    assert!(a.voice_active());
    drop(a);
    assert!(fake.borrow().closed >= 2);
}
#[test]
fn review_and_receipt_disable_voice_and_cannot_receive_final_or_confirm() {
    let (mut a, fake) = attach(super::readiness_tests::app(
        serde_json::json!({"parts": super::readiness_tests::parts()}),
    ));
    a.review_goal();
    assert!(a.handoff.review().is_some());
    for code in [KeyCode::F(6), KeyCode::Char('Ω')] {
        key(&mut a, code);
    }
    event(
        &mut a,
        &fake,
        Event::Final {
            take: 1,
            text: "confirm".into(),
        },
    );
    assert!(fake.borrow().commands.is_empty());
    assert!(a.handoff.review().unwrap().input.is_empty());
    assert!(!a.handoff.saving());
    assert!(a.input.text.is_empty());
    assert!(!a.voice_available());
    a.set_saved(seed_me::Receipt::test("/TEST/voice-receipt".into()));
    key(&mut a, KeyCode::F(6));
    assert!(fake.borrow().commands.is_empty());
    assert!(!layout::help_text(&a).contains("speak"));
    assert_eq!(
        layout::key_bar_items(&a),
        vec![("s", "open in tend"), ("esc", "menu")]
    );
}
#[test]
fn active_voice_blocks_readiness_and_review_even_without_typed_text() {
    let (mut a, fake) = attach(super::readiness_tests::app(
        serde_json::json!({"parts": super::readiness_tests::parts()}),
    ));
    assert!(a.goal_ready());
    start(&mut a, &fake);
    assert!(!a.goal_ready());
    a.review_goal();
    assert!(a.handoff.review().is_none());
    assert!(a.notice.contains("Stop or cancel voice"));
    key(&mut a, KeyCode::Esc);
    assert!(a.goal_ready());
}
#[test]
fn voice_help_keeps_the_alias_and_local_label_visible_and_escape_cancellation() {
    let (mut a, fake) = attach(BrainDump::default());
    a.paste("my local draft");
    key(&mut a, KeyCode::F(1));
    for (w, h) in [(80, 24), (100, 30), (160, 40), (320, 40)] {
        for mono in [false, true] {
            a.screen.original_scroll = 0;
            let view = snapshot(w, h, &mut a, mono).unwrap();
            assert!(
                view.contains("Ctrl+Alt+Z") && view.contains("speaks"),
                "{view}"
            );
        }
    }
    key(&mut a, KeyCode::F(6));
    assert!(fake.borrow().commands.is_empty());
    key(&mut a, KeyCode::Esc);
    start(&mut a, &fake);
    key(&mut a, KeyCode::F(1));
    assert!(
        snapshot(80, 24, &mut a, true)
            .unwrap()
            .contains("Cancel voice")
    );
    key(&mut a, KeyCode::Esc);
    assert!(!a.voice_active() && a.screen.help && !a.running());
    assert_eq!(a.input.text, "my local draft");
    key(&mut a, KeyCode::Esc);
    assert!(!a.screen.help);
}

#[test]
fn typing_and_reading_show_the_missing_node_remedy_only_after_the_voice_key() {
    let fake = Rc::new(RefCell::new(Fake::default()));
    let mut app = BrainDump::default().with_voice(Box::new(fake.clone()));
    let remedy = "Voice needs Node 22 or later. Set TINKERY_NODE to its executable.";
    event(&mut app, &fake, Failure::new("no-node", remedy).event());

    let quiet_screen = |app: &mut BrainDump, expected: &str| {
        let view = snapshot(100, 30, app, true).unwrap();
        assert!(view.contains(expected) && view.contains(" speak"), "{view}");
        assert!(!view.contains(remedy), "{view}");
    };
    quiet_screen(&mut app, "What's on your mind?");
    app.paste("Keep these typed words.");
    quiet_screen(&mut app, "Keep these typed words.");
    key(&mut app, KeyCode::F(2));
    super::tests::settle(&mut app);
    quiet_screen(&mut app, "which part matters most to you?");

    key(&mut app, KeyCode::F(6));
    let view = snapshot(100, 30, &mut app, true).unwrap();
    assert!(view.contains(remedy), "{view}");
    assert_eq!(fake.borrow().restarted, 1);
    assert!(fake.borrow().commands.is_empty());
}
#[test]
fn conditional_help_and_key_bar_and_no_voice_in_library_snapshots() {
    let mut a = BrainDump::default();
    assert!(!layout::help_text(&a).contains("speak"));
    assert!(!snapshot(100, 30, &mut a, false).unwrap().contains("speak"));
    let (mut a, fake) = attach(a);
    assert!(
        layout::help_text(&a)
            .contains("F6 or Ctrl+Alt+Z (⌃⌥Z) speaks; your voice stays on this machine.")
    );
    assert!(
        layout::key_bar_items(&a)
            .contains(&(crate::voice::key_label(std::env::consts::OS), "speak"))
    );
    start(&mut a, &fake);
    assert_eq!(
        layout::key_bar_items(&a),
        vec![
            (crate::voice::key_label(std::env::consts::OS), "stop"),
            ("esc", "cancel")
        ]
    );
    event(
        &mut a,
        &fake,
        Event::Error {
            take: None,
            kind: "no-node".into(),
            message: "Set TINKERY_NODE.".into(),
        },
    );
    assert!(layout::help_text(&a).contains("speak"));
}
#[test]
fn shaping_results_defer_while_voice_is_active_even_with_empty_input() {
    let (mut a, fake) = attach(BrainDump::default());
    a.paste("ordinary original");
    a.submit();
    start(&mut a, &fake);
    for _ in 0..100 {
        a.tick();
        if a.shaping.ready.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert!(a.shaping.ready.is_some());
    assert!(a.conversation.board().is_none());
    assert_eq!(a.conversation.sources()[0].text, "ordinary original");
    assert!(!a.goal_ready());
}

#[test]
fn first_use_key_bar_and_muted_prompt_preserve_input_and_require_explicit_yes() {
    for no_color in [false, true] {
        let fake = Rc::new(RefCell::new(Fake::default()));
        let mut a = BrainDump::default().with_voice(Box::new(fake.clone()));
        a.paste("typed");
        assert!(
            layout::key_bar_items(&a)
                .contains(&(crate::voice::key_label(std::env::consts::OS), "speak"))
        );
        event(
            &mut a,
            &fake,
            Event::NeedsInstall {
                size_mb: 20,
                message: "Voice needs a one-time local install (about 20 MB). Install now? y / n"
                    .into(),
            },
        );
        assert!(a.voice.as_ref().unwrap().notice().is_empty());
        key(&mut a, KeyCode::F(6));
        assert!(a.voice.as_ref().unwrap().setup_prompt());
        let palette = Palette::new(no_color);
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 30)).unwrap();
        terminal.draw(|f| render(f, &mut a, palette)).unwrap();
        let buffer = terminal.backend().buffer();
        let cell = &buffer[find_on_screen(buffer, "Voice needs a one-time local install")];
        if no_color {
            assert!(cell.modifier.contains(ratatui::style::Modifier::DIM));
        } else {
            assert_eq!(cell.fg, palette.muted.fg.unwrap());
        }
        key(&mut a, KeyCode::Char('y'));
        assert_eq!(a.input.text, "typed");
        assert_eq!(fake.borrow().commands, [Command::Install]);
        assert_eq!(a.voice.as_ref().unwrap().notice(), "installing voice…");
        event(&mut a, &fake, Event::Loading);
        event(&mut a, &fake, Event::Ready { version: 1 });
        assert!(!a.voice_active());
        assert!(a.conversation.sources().is_empty());
        assert_eq!(a.input.text, "typed");
    }
}

#[test]
fn decline_is_consumed_without_typing_or_repeated_prompt_and_plain_messages_are_muted() {
    let fake = Rc::new(RefCell::new(Fake::default()));
    let mut a = BrainDump::default().with_voice(Box::new(fake.clone()));
    a.paste("typed");
    event(
        &mut a,
        &fake,
        Event::NeedsInstall {
            size_mb: 20,
            message: "Install now? y / n".into(),
        },
    );
    key(&mut a, KeyCode::F(6));
    key(&mut a, KeyCode::Char('n'));
    key(&mut a, KeyCode::F(6));
    assert!(!a.voice.as_ref().unwrap().setup_prompt());
    assert!(fake.borrow().commands.is_empty());
    assert_eq!(a.input.text, "typed");
    for (kind, message) in [
        (
            "mic-permission",
            "Allow microphone access for your terminal in System Settings → Privacy & Security → Microphone.",
        ),
        ("mic-busy", "Microphone is in use by another app."),
        ("mic-unavailable", "No microphone found."),
        (
            "install",
            "Voice install failed: registry unavailable. Press the voice key to retry.",
        ),
    ] {
        let (mut a, fake) = attach(BrainDump::default());
        a.paste("typed");
        event(
            &mut a,
            &fake,
            Event::Error {
                take: None,
                kind: kind.into(),
                message: message.into(),
            },
        );
        assert_eq!(a.voice.as_ref().unwrap().notice(), message);
        assert_eq!(a.input.text, "typed");
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 30)).unwrap();
        terminal
            .draw(|f| render(f, &mut a, Palette::new(true)))
            .unwrap();
        let buffer = terminal.backend().buffer();
        let shown: String = message.chars().take(20).collect();
        let notice = find_on_screen(buffer, &shown);
        assert!(
            buffer[notice]
                .modifier
                .contains(ratatui::style::Modifier::DIM)
        );
    }
}
#[test]
fn recording_does_not_change_what_the_goal_line_says_is_open() {
    let (mut a, fake) = attach(super::intent_tests::constraints(
        &["must work offline"],
        &[],
    ));
    let ready = "Ready for your review.";
    let right = |a: &mut BrainDump| {
        super::goal_tests::right_text(&snapshot(100, 30, a, false).unwrap(), 100)
    };
    let before = right(&mut a);
    assert!(before.contains(ready), "{before}");
    start(&mut a, &fake);
    assert!(a.voice_active() && !a.goal_ready());
    let screen = right(&mut a);
    assert!(screen.contains(ready), "{screen}");
    assert!(!screen.contains("remain open"));
    a.screen.help = true;
    assert!(layout::help_text(&a).contains("Ready for your review."));
}

#[test]
fn dictation_never_deletes_a_selection_and_inserts_at_the_caret() {
    let (mut a, fake) = attach(BrainDump::default());
    a.input = Note::new("keep these words");
    a.input.cursor = 5;
    for _ in 0..5 {
        a.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::SHIFT));
    }
    assert_eq!(a.input.selected_text(), Some("these"));
    start(&mut a, &fake);
    assert_eq!(a.input.selection(), None, "recording drops the highlight");
    a.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::SHIFT));
    event(
        &mut a,
        &fake,
        Event::Final {
            take: 1,
            text: "said".into(),
        },
    );
    assert_eq!(a.input.selection(), None);
    assert_eq!(
        a.input.text.replacen("said", "", 1),
        "keep these words",
        "every original byte survives; dictation only adds"
    );
}

#[test]
fn a_voice_result_during_a_drag_ends_the_drag_and_release_cannot_replace_text() {
    for spoken in ["é", "VOICE"] {
        let (mut a, fake) = attach(BrainDump::default());
        a.input = Note::new("abc.");
        snapshot(100, 30, &mut a, false).unwrap();
        let (x, y) = (a.screen.input_area.x, a.screen.input_area.y);
        let mouse = |a: &mut BrainDump, kind, dx| {
            let area = a.screen.area;
            a.handle_mouse(
                MouseEvent {
                    kind,
                    column: x + dx,
                    row: y,
                    modifiers: KeyModifiers::NONE,
                },
                area,
            )
        };
        mouse(&mut a, MouseEventKind::Down(MouseButton::Left), 2);
        mouse(&mut a, MouseEventKind::Drag(MouseButton::Left), 1);
        assert_eq!(a.input.selected_text(), Some("b"));
        start(&mut a, &fake);
        key(&mut a, KeyCode::F(6));
        event(
            &mut a,
            &fake,
            Event::Final {
                take: 1,
                text: spoken.into(),
            },
        );
        assert_eq!(a.input.text, format!("a{spoken}bc."), "dictation only adds");
        mouse(&mut a, MouseEventKind::Up(MouseButton::Left), 4);
        assert_eq!(a.input.selection(), None);
        key(&mut a, KeyCode::Char('x'));
        assert_eq!(a.input.text, format!("a{spoken}xbc."), "{spoken}");
    }
}

/// Where `needle` first appears on screen, so styles are checked by what a cell says.
fn find_on_screen(buffer: &ratatui::buffer::Buffer, needle: &str) -> (u16, u16) {
    let want: Vec<String> = needle.chars().map(String::from).collect();
    (0..buffer.area.height)
        .find_map(|y| {
            let row: Vec<&str> = (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect();
            row.windows(want.len())
                .position(|w| w.iter().zip(&want).all(|(a, b)| a == b))
                .map(|x| (x as u16, y))
        })
        .unwrap_or_else(|| panic!("{needle:?} is not on screen"))
}
