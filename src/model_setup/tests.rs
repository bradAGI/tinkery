use super::*;
use crate::pi::{CallBudget, Request};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn setup(model: &str, thinking: &str, calls: usize) -> Setup {
    Setup::new(Harness::Pi, model.into(), thinking.into(), calls).unwrap()
}

fn press(controls: &mut Controls, code: KeyCode) {
    assert!(controls.key(KeyEvent::new(code, KeyModifiers::NONE)));
}

fn typed(controls: &mut Controls, text: &str) {
    for c in text.chars() {
        press(controls, KeyCode::Char(c));
    }
}

fn screen(controls: &Controls) -> String {
    screen_at(controls, 100, 30)
}

fn screen_at(controls: &Controls, width: u16, height: u16) -> String {
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| controls.render(frame, crate::Palette::new(true)))
        .unwrap();
    let buffer = terminal.backend().buffer();
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn one_parser_refuses_what_neither_flags_nor_file_may_hold() {
    use Harness::{ClaudeCode, Pi};
    assert!(Setup::new(Pi, "anthropic/model".into(), "low".into(), 10).is_ok());
    assert!(Setup::new(Pi, String::new(), "off".into(), 0).is_ok());
    assert!(Setup::new(ClaudeCode, "claude-opus-5-5".into(), "max".into(), 5).is_ok());
    for (harness, model, thinking, calls, why) in [
        (Pi, "model", "low", 5, "provider/model-id"),
        (Pi, "a/b c", "low", 5, "provider/model-id"),
        (Pi, "a/b", "automatic", 5, "Use thinking off|minimal|low"),
        (Pi, "", "low", 5, "Choose a model"),
        (ClaudeCode, "sonnet", "low", 5, "claude-sonnet-5-5"),
        (
            ClaudeCode,
            "anthropic/claude-opus-5-5",
            "low",
            5,
            "Claude Code model id",
        ),
        (ClaudeCode, "claude-", "low", 5, "Claude Code model id"),
        (
            ClaudeCode,
            "claude-opus-5-5",
            "off",
            5,
            "Use thinking low|medium|high|xhigh|max with Claude Code",
        ),
        (
            ClaudeCode,
            "claude-opus-5-5",
            "minimal",
            5,
            "with Claude Code",
        ),
    ] {
        let refused = Setup::new(harness, model.into(), thinking.into(), calls).unwrap_err();
        assert!(refused.contains(why), "{model} {thinking}: {refused}");
    }
}

#[test]
fn launch_drafts_only_with_saved_calls_or_shape_pi_and_flags_win_for_one_launch() {
    let saved = setup("p/saved", "high", 10);
    let practice_saved = setup("p/saved", "high", 0);
    assert_eq!(Setup::launch(false, None, None, None, None, None), Ok(None));
    assert_eq!(
        Setup::launch(false, None, None, None, None, Some(&practice_saved)),
        Ok(None)
    );
    assert_eq!(
        Setup::launch(false, None, None, None, None, Some(&saved)),
        Ok(Some(saved.clone()))
    );
    assert_eq!(
        Setup::launch(
            true,
            None,
            Some("p/flag".into()),
            Some("max".into()),
            Some(2),
            Some(&saved)
        ),
        Ok(Some(setup("p/flag", "max", 2)))
    );
    assert_eq!(
        Setup::launch(true, None, None, None, None, Some(&practice_saved)),
        Ok(Some(practice_saved.clone())),
        "--shape-pi drafts with the saved model even with zero calls"
    );
    assert_eq!(
        Setup::launch(true, None, Some("p/flag".into()), None, None, None),
        Ok(Some(setup("p/flag", "low", 0))),
        "without a saved file, flags keep today's defaults"
    );
    assert!(
        Setup::launch(true, None, None, None, None, None)
            .unwrap_err()
            .contains("requires an explicit --model")
    );
    assert!(
        Setup::launch(
            true,
            None,
            Some("p/m".into()),
            Some("lots".into()),
            None,
            None
        )
        .is_err()
    );
}

#[test]
fn explicit_zero_overrides_saved_calls_without_changing_the_saved_setup() {
    for saved in [
        setup("p/saved", "high", 5),
        Setup::new(
            Harness::ClaudeCode,
            "claude-opus-5-5".into(),
            "high".into(),
            5,
        )
        .unwrap(),
    ] {
        assert_eq!(
            Setup::launch(false, None, None, None, Some(0), Some(&saved)),
            Ok(None)
        );
        assert_eq!(
            Setup::launch(false, None, None, None, None, Some(&saved)),
            Ok(Some(saved.clone()))
        );
        assert_eq!(
            Setup::launch(true, None, None, None, Some(0), Some(&saved)),
            Ok(Some(Setup { calls: 0, ..saved }))
        );
    }
}

#[test]
fn a_harness_flag_keeps_the_saved_calls_but_never_another_harness_s_model_or_thinking() {
    use Harness::{ClaudeCode, Pi};
    let saved = setup("p/saved", "minimal", 10);
    let claude = |model: &str, thinking: &str, calls| {
        Setup::new(ClaudeCode, model.into(), thinking.into(), calls).unwrap()
    };
    assert!(
        Setup::launch(true, Some(ClaudeCode), None, None, None, Some(&saved))
            .unwrap_err()
            .contains("--model: a Claude Code model id"),
        "a Pi model is never sent to Claude Code"
    );
    assert_eq!(
        Setup::launch(
            true,
            Some(ClaudeCode),
            Some("claude-haiku-5-5".into()),
            None,
            None,
            Some(&saved)
        ),
        Ok(Some(claude("claude-haiku-5-5", "low", 10))),
        "Pi's minimal thinking stays with Pi"
    );
    let saved_claude = claude("claude-opus-5-5", "xhigh", 5);
    assert_eq!(
        Setup::launch(true, None, None, None, None, Some(&saved_claude)),
        Ok(Some(saved_claude.clone()))
    );
    assert!(
        Setup::launch(true, Some(Pi), None, None, None, Some(&saved_claude))
            .unwrap_err()
            .contains("provider/model-id")
    );
    assert_eq!(Harness::parse("claude-code"), Ok(ClaudeCode));
    assert!(Harness::parse("claude").is_err());
}

#[test]
fn saved_file_errors_name_the_path_without_claiming_a_launch_mode() {
    let home = tempfile::tempdir().unwrap();
    let path = path(Some(home.path().as_os_str())).unwrap();
    assert_eq!(path, home.path().join(".config/tinkery/model.json"));
    assert_eq!(load(&path), Ok(None));
    assert_eq!(super::path(Some("relative".as_ref())), None);
    assert_eq!(super::path(None), None);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    for bad in [
        "not json",
        r#"{"model":"p/m","thinking":"low"}"#,
        r#"{"model":"p/m","thinking":"low","calls":5,"key":"x"}"#,
        r#"{"model":"p/m","thinking":"lots","calls":5}"#,
        r#"{"model":"","thinking":"low","calls":5}"#,
        r#"{"model":"p/m","thinking":"low","calls":-1}"#,
    ] {
        std::fs::write(&path, bad).unwrap();
        let refused = load(&path).unwrap_err();
        assert!(
            refused.contains(&path.display().to_string()) && !refused.contains("practice"),
            "{bad}: {refused}"
        );
    }
    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
    let refused = load(&path).unwrap_err();
    assert!(refused.contains(&path.display().to_string()), "{refused}");
    assert!(
        refused.contains("Cannot read") && !refused.contains("practice"),
        "{refused}"
    );
}

#[test]
fn only_s_writes_the_file_and_esc_leaves_it_alone() {
    let home = tempfile::tempdir().unwrap();
    let file = path(Some(home.path().as_os_str())).unwrap();
    let mut controls = Controls::new(Some(file.clone()), Ok(None), None);
    assert_eq!(controls.label(), "practice");
    assert!(!controls.key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE)));
    controls.open();
    assert!(screen(&controls).contains("Harness   Pi"));
    assert!(screen(&controls).contains("Model     none"));
    assert!(
        !screen(&controls).contains("--shape-budget"),
        "only an agent sees the grant"
    );
    press(&mut controls, KeyCode::Down);
    press(&mut controls, KeyCode::Enter);
    typed(&mut controls, "anthropic/claude-test");
    press(&mut controls, KeyCode::Enter);
    press(&mut controls, KeyCode::Down);
    press(&mut controls, KeyCode::Enter);
    press(&mut controls, KeyCode::Down);
    press(&mut controls, KeyCode::Enter);
    press(&mut controls, KeyCode::Enter);
    let shown = screen(&controls);
    assert!(shown.contains("anthropic/claude-test"), "{shown}");
    assert!(
        shown.contains("medium") && shown.contains("10 each launch"),
        "{shown}"
    );
    press(&mut controls, KeyCode::Esc);
    assert!(!controls.is_open());
    assert!(!file.exists(), "esc wrote the file");

    controls.open();
    assert!(
        screen(&controls).contains("Model     none"),
        "esc kept the draft"
    );
    press(&mut controls, KeyCode::Down);
    press(&mut controls, KeyCode::Enter);
    typed(&mut controls, "anthropic/claude-test");
    press(&mut controls, KeyCode::Enter);
    press(&mut controls, KeyCode::Down);
    press(&mut controls, KeyCode::Down);
    press(&mut controls, KeyCode::Enter);
    press(&mut controls, KeyCode::Char('s'));
    assert_eq!(
        load(&file),
        Ok(Some(setup("anthropic/claude-test", "low", 5)))
    );
    let shown = screen(&controls);
    assert!(
        shown.contains("Saved. A new launch drafts with it."),
        "{shown}"
    );
    assert_eq!(
        controls.label(),
        "practice",
        "saving changes later launches only"
    );
}

#[test]
fn active_recipient_survives_editing_and_saving_later_settings() {
    for active in [
        setup("p/active", "low", 0),
        Setup::new(
            Harness::ClaudeCode,
            "claude-haiku-5-5".into(),
            "low".into(),
            0,
        )
        .unwrap(),
    ] {
        for (width, height) in [(80, 24), (100, 30)] {
            let home = tempfile::tempdir().unwrap();
            let file = home.path().join("TEST-model.json");
            let saved = setup("p/saved", "low", 5);
            save(&file, &saved).unwrap();
            let mut controls = Controls::new(
                Some(file.clone()),
                Ok(Some(saved.clone())),
                Some((active.clone(), CallBudget::new(0))),
            );
            let recipient = format!("F2 sends to {}: {}.", active.harness.label(), active.model);
            controls.open();
            assert!(screen_at(&controls, width, height).contains(&recipient));
            press(&mut controls, KeyCode::Enter); // Change only the later harness/model.
            assert_eq!(load(&file), Ok(Some(saved)));
            for save_changes in [false, true] {
                if save_changes {
                    press(&mut controls, KeyCode::Char('s'));
                    assert_eq!(load(&file).unwrap().unwrap().model, "claude-sonnet-5-5");
                }
                let shown = screen_at(&controls, width, height);
                let without_border = shown.replace('│', " ");
                let words = without_border
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ");
                for definition in [
                    "Runs count drafting attempts, including failures and cancellations.",
                    "One run may make several model responses;",
                    "nothing caps responses or spending.",
                ] {
                    assert!(words.contains(definition), "Missing {definition}:\n{shown}");
                }
                for text in [
                    &recipient,
                    "For later launches",
                    "claude-sonnet-5-5",
                    "s save",
                    "esc close",
                ] {
                    assert!(shown.contains(text), "Missing {text}:\n{shown}");
                }
                assert_eq!(controls.label(), "0 runs left");
            }
        }
    }
}

#[test]
fn long_active_model_keeps_controls_visible_at_minimum_size() {
    let active = setup(&format!("p/{}END", "a".repeat(120)), "low", 0);
    let mut controls = Controls::new(
        None,
        Ok(Some(active.clone())),
        Some((active, CallBudget::new(0))),
    );
    controls.open();
    let shown = screen_at(&controls, 80, 24);
    for text in ["F2 sends to Pi:", "END.", "s save", "esc close"] {
        assert!(shown.contains(text), "Missing {text}:\n{shown}");
    }
}

#[test]
fn practice_disclosure_never_names_the_saved_model_as_the_recipient() {
    let mut controls = Controls::new(None, Ok(Some(setup("p/saved", "high", 5))), None);
    controls.open();
    let shown = screen_at(&controls, 80, 24);
    assert!(
        shown.contains("This launch: practice; no model requests."),
        "{shown}"
    );
    assert!(
        shown.contains("For later launches") && shown.contains("p/saved"),
        "{shown}"
    );
    assert!(!shown.contains("F2 sends"), "{shown}");
}

#[test]
fn saving_refuses_calls_without_a_model_and_typing_refuses_a_loose_model() {
    let home = tempfile::tempdir().unwrap();
    let file = path(Some(home.path().as_os_str())).unwrap();
    let mut controls = Controls::new(Some(file.clone()), Ok(None), None);
    controls.open();
    press(&mut controls, KeyCode::Down);
    press(&mut controls, KeyCode::Enter);
    typed(&mut controls, "no-provider s");
    press(&mut controls, KeyCode::Enter);
    assert!(screen(&controls).contains("Use an explicit model: provider/model-id"));
    press(&mut controls, KeyCode::Esc);
    assert!(
        controls.is_open(),
        "esc while typing only undoes the typing"
    );
    press(&mut controls, KeyCode::Down);
    press(&mut controls, KeyCode::Down);
    press(&mut controls, KeyCode::Enter);
    press(&mut controls, KeyCode::Char('s'));
    assert!(screen(&controls).contains("Choose a model before allowing drafting runs."));
    assert!(!file.exists());
}

#[test]
fn an_invalid_file_is_explained_in_the_panel_and_saving_replaces_it() {
    let refused = "/TEST/model.json is not a model setup: bad. Save again".to_owned();
    let mut controls = Controls::new(None, Err(refused.clone()), None);
    assert_eq!(controls.label(), "practice");
    controls.open();
    assert!(screen(&controls).contains("/TEST/model.json is not a model setup"));
    press(&mut controls, KeyCode::Char('s'));
    assert!(screen(&controls).contains("Saving needs an absolute HOME"));
}

#[test]
fn adding_calls_raises_this_launch_only_and_a_launch_never_exceeds_its_cap() {
    let budget = CallBudget::new(1);
    assert!(budget.reserve(Request::Shaping).is_ok());
    let refused = budget.reserve(Request::Shaping).unwrap_err();
    assert!(refused.contains("Add runs to keep going."), "{refused}");
    let home = tempfile::tempdir().unwrap();
    let file = path(Some(home.path().as_os_str())).unwrap();
    let mut controls = Controls::new(
        Some(file.clone()),
        Ok(None),
        Some((setup("p/active", "low", 1), budget.clone())),
    );
    assert_eq!(controls.label(), "0 runs left");
    controls.open();
    for _ in 0..4 {
        press(&mut controls, KeyCode::Down);
    }
    press(&mut controls, KeyCode::Enter);
    assert_eq!(budget.remaining(), 5);
    assert_eq!(controls.label(), "5 runs left");
    assert!(screen(&controls).contains("5 left · enter adds 5"));
    press(&mut controls, KeyCode::Esc);
    assert!(!file.exists(), "adding calls saved nothing");
    for _ in 0..5 {
        assert!(budget.reserve(Request::SeedStep).is_ok());
    }
    assert!(budget.reserve(Request::SeedStep).is_err(), "cap exceeded");
    assert_eq!(budget.used(), 6);
}

#[test]
fn an_agent_sees_the_setup_but_cannot_save_it_or_add_calls() {
    let home = tempfile::tempdir().unwrap();
    let file = path(Some(home.path().as_os_str())).unwrap();
    let budget = CallBudget::new(0);
    let mut controls = Controls::new(
        Some(file.clone()),
        Ok(None),
        Some((setup("p/active", "low", 0), budget.clone())),
    )
    .for_agent();
    controls.open();
    assert!(
        screen(&controls).contains("--shape-budget"),
        "agent panel names the grant"
    );
    press(&mut controls, KeyCode::Char('s'));
    assert!(screen(&controls).contains("An agent can't save a setup or add runs"));
    for _ in 0..4 {
        press(&mut controls, KeyCode::Down);
    }
    press(&mut controls, KeyCode::Enter);
    assert_eq!(budget.limit(), 0);
    assert!(!file.exists());
}

#[test]
fn practice_has_no_now_row_and_clicking_the_label_opens_the_panel() {
    use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    let mut controls = Controls::new(None, Ok(None), None);
    let area = ratatui::layout::Rect::new(0, 0, 100, 30);
    let at = |column| MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row: 1,
        modifiers: KeyModifiers::NONE,
    };
    let line = screen(&controls).lines().nth(1).unwrap().to_owned();
    let start = line.find("practice").unwrap() as u16;
    assert!(!controls.click(at(start - 1), area));
    assert!(controls.click(at(start), area));
    assert!(controls.is_open());
    assert!(!screen(&controls).contains("Now "));
    for _ in 0..4 {
        press(&mut controls, KeyCode::Down);
    }
    assert!(screen(&controls).contains("› Harness"), "four rows wrap");
}

#[test]
fn claude_code_models_are_picked_not_typed_and_saving_keeps_the_harness() {
    let home = tempfile::tempdir().unwrap();
    let file = path(Some(home.path().as_os_str())).unwrap();
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, r#"{"model":"p/old","thinking":"minimal","calls":5}"#).unwrap();
    assert_eq!(
        load(&file),
        Ok(Some(setup("p/old", "minimal", 5))),
        "a file without a harness is Pi"
    );
    let mut controls = Controls::new(Some(file.clone()), load(&file), None);
    controls.open();
    press(&mut controls, KeyCode::Enter);
    let shown = screen(&controls);
    assert!(
        shown.contains("Harness   Claude Code") && shown.contains("claude-sonnet-5-5"),
        "{shown}"
    );
    assert!(
        shown.contains("Thinking  low"),
        "minimal is Pi's alone: {shown}"
    );
    press(&mut controls, KeyCode::Down);
    press(&mut controls, KeyCode::Enter);
    assert!(screen(&controls).contains("claude-opus-5-5"));
    assert!(
        screen(&controls).contains("↑↓ choose"),
        "picking never starts typing"
    );
    press(&mut controls, KeyCode::Down);
    for level in ["medium", "high", "xhigh", "max", "low"] {
        press(&mut controls, KeyCode::Enter);
        assert!(screen(&controls).contains(&format!("Thinking  {level}")));
    }
    press(&mut controls, KeyCode::Char('s'));
    assert_eq!(
        load(&file),
        Ok(Some(
            Setup::new(
                Harness::ClaudeCode,
                "claude-opus-5-5".into(),
                "low".into(),
                5
            )
            .unwrap()
        ))
    );
    assert!(
        std::fs::read_to_string(&file)
            .unwrap()
            .contains(r#""harness": "claude-code""#)
    );
    press(&mut controls, KeyCode::Up);
    press(&mut controls, KeyCode::Up);
    press(&mut controls, KeyCode::Enter);
    let shown = screen(&controls);
    assert!(
        shown.contains("Harness   Pi") && shown.contains("Model     none"),
        "a Claude Code model is never kept for Pi: {shown}"
    );
}

/// A fake Pi running `script` with `sys` imported.
fn lister(directory: &std::path::Path, script: &str) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let program = directory.join("pi");
    std::fs::write(
        &program,
        format!("#!/usr/bin/env python3\nimport sys\n{script}\n"),
    )
    .unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    program
}

fn shown_once(controls: &Controls, text: &str) -> String {
    let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let shown = screen(controls);
        if shown.contains(text) || std::time::Instant::now() > until {
            return shown;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

#[test]
fn enter_picks_pi_s_listed_models_and_t_still_types_one() {
    let directory = tempfile::tempdir().unwrap();
    let program = lister(
        directory.path(),
        "print('provider   model     context')\n\
         print('anthropic  claude-a  1M  ')\n\
         print('freellmapi auto:fast 128K')",
    );
    let mut controls = Controls::new(None, Ok(None), None).listing_pi(program);
    controls.open();
    press(&mut controls, KeyCode::Down);
    assert!(screen(&controls).contains("enter next   t type"));
    press(&mut controls, KeyCode::Enter);
    let shown = shown_once(&controls, "Pi lists 2 models; enter picks the next.");
    assert!(shown.contains("Pi lists 2 models"), "{shown}");
    assert!(
        shown.contains("Model     none"),
        "listing picks nothing on its own"
    );
    for model in [
        "anthropic/claude-a",
        "freellmapi/auto:fast",
        "anthropic/claude-a",
    ] {
        press(&mut controls, KeyCode::Enter);
        assert!(screen(&controls).contains(&format!("Model     {model}")));
    }
    press(&mut controls, KeyCode::Char('t'));
    for _ in 0.."anthropic/claude-a".len() {
        press(&mut controls, KeyCode::Backspace);
    }
    typed(&mut controls, "antigravity/unlisted");
    press(&mut controls, KeyCode::Enter);
    assert!(screen(&controls).contains("Model     antigravity/unlisted"));
}

#[test]
fn a_pi_that_cannot_list_falls_back_to_typing_and_says_why() {
    let directory = tempfile::tempdir().unwrap();
    let program = lister(directory.path(), "sys.exit(3)");
    let mut controls = Controls::new(None, Ok(None), None).listing_pi(program);
    controls.open();
    press(&mut controls, KeyCode::Down);
    press(&mut controls, KeyCode::Enter);
    let shown = shown_once(&controls, "Type the model instead.");
    assert!(shown.contains("couldn't list its models"), "{shown}");
    press(&mut controls, KeyCode::Enter);
    let shown = screen(&controls);
    assert!(shown.contains("enter done"), "{shown}");
}
