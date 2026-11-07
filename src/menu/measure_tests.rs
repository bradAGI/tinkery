//! Measure's no-repo state, help ownership and quit routes; no implicit external reads.

use super::*;

fn home(root: &Path) -> Home {
    let mut home = Home::new(
        Box::new(BrainDump::default),
        Ok(root.join("TEST-sessions")),
        Ok(root.join("TEST-links")),
    );
    home.open(Place::Measure);
    home
}

fn press(home: &mut Home, code: KeyCode) {
    home.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
}

#[test]
fn measure_names_the_repo_once_in_its_heading() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("TEST-measure-header");
    std::fs::create_dir_all(repo.join(".git")).unwrap();
    let mut h = home(root.path()).with_repo(crate::repo::Repo::enclosing(&repo));
    h.open(Place::Measure);
    let text = snapshot(80, 24, &mut h, true).unwrap();
    assert!(text.contains("measure · TEST-measure-header"), "{text}");
    assert_eq!(text.matches("TEST-measure-header").count(), 1);
    assert!(!text.contains("tinkery › measure") && !text.contains("Pull requests ·"));
}

#[test]
fn measure_opens_unknown_at_supported_sizes_without_creating_records() {
    let root = tempfile::tempdir().unwrap();
    for (width, height) in [(80, 24), (100, 30), (160, 40), (320, 40)] {
        for no_color in [false, true] {
            let mut h = home(root.path());
            for _ in 0..3 {
                let text = snapshot(width, height, &mut h, no_color).unwrap();
                assert!(text.contains("No launch repository."), "{text}");
                assert!(text.contains("esc back   ? help"), "{text}");
                assert!(
                    !text.contains("unavailable") && !text.contains("Reading"),
                    "{text}"
                );
                h.open(Place::Measure);
            }
        }
    }
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}

#[test]
fn measure_help_closes_and_q_returns_to_a_closed_help_menu() {
    let root = tempfile::tempdir().unwrap();
    for help in [false, true] {
        let mut h = home(root.path());
        if help {
            press(&mut h, KeyCode::F(1));
            let text = snapshot(80, 24, &mut h, true).unwrap();
            assert!(text.contains("q goes back to the menu"), "{text}");
            assert!(!text.contains("q quits"), "{text}");
        }
        press(&mut h, KeyCode::Char('q'));
        assert!(!h.quit && !h.help && matches!(h.view, View::Menu));
        assert!(
            snapshot(80, 24, &mut h, true)
                .unwrap()
                .contains("? help   q quit")
        );
    }
    let mut h = home(root.path());
    press(&mut h, KeyCode::Char('?'));
    press(&mut h, KeyCode::Esc);
    assert!(
        snapshot(80, 24, &mut h, true)
            .unwrap()
            .contains("No launch repository.")
    );
    press(&mut h, KeyCode::Esc);
    assert!(!h.quit && !h.help && matches!(h.view, View::Menu));
}

#[test]
fn measure_keeps_f10_and_ctrl_c_quit_routes_with_or_without_help() {
    let root = tempfile::tempdir().unwrap();
    for help in [false, true] {
        for quit in [
            KeyEvent::new(KeyCode::F(10), KeyModifiers::NONE),
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
        ] {
            let mut h = home(root.path());
            if help {
                press(&mut h, KeyCode::Char('?'));
            }
            h.handle_key(quit);
            assert!(h.quit);
        }
    }
}

#[test]
fn measure_help_ignores_menu_shortcuts_and_releases() {
    let root = tempfile::tempdir().unwrap();
    let mut h = home(root.path());
    press(&mut h, KeyCode::F(1));
    let before = snapshot(100, 30, &mut h, false).unwrap();
    for code in [
        KeyCode::Char('1'),
        KeyCode::Char('7'),
        KeyCode::Enter,
        KeyCode::Char('h'),
    ] {
        press(&mut h, code);
        assert_eq!(snapshot(100, 30, &mut h, false).unwrap(), before);
    }
    let mut release = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
    release.kind = KeyEventKind::Release;
    h.handle_key(release);
    assert_eq!(snapshot(100, 30, &mut h, false).unwrap(), before);
}

#[test]
fn help_away_from_the_menu_says_q_goes_back() {
    let root = tempfile::tempdir().unwrap();
    let mut h = home(root.path());
    h.open(Place::Taste);
    press(&mut h, KeyCode::Char('?'));
    let help = snapshot(80, 24, &mut h, true).unwrap();
    assert!(help.contains("q goes back to the menu"), "{help}");
    assert!(!help.contains("q quits"), "{help}");
    press(&mut h, KeyCode::Char('q'));
    assert!(!h.quit && matches!(h.view, View::Menu));
}
