//! Escape clears original selection, returns to writing, then guards unsaved work.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use tinkery::think::brain_dump::{BoardHost, BoardRequest, BrainDump, Exit, Guess, snapshot};

const ORIGINAL: &str = "TEST original café 👩‍💻\r\nKeep these exact words.";
const DRAFT: &str = "TEST unsent answer 中\r\nStill local.";
const LOSS: &str = "Back to menu and lose this draft? y / n";

#[derive(Default)]
struct Host(AtomicUsize);

impl BoardHost for Host {
    fn reshape(&self, request: BoardRequest, _: &AtomicBool) -> Result<Guess, String> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Guess::decode(
            r#"{"parts":{"who":"I","outcome":"keep exact words","why":"copies must match","done_when":"bytes match"}}"#,
            &request,
            &mut vec![],
        )
    }
}

fn key(app: &mut BrainDump, code: KeyCode) {
    app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn control(app: &mut BrainDump, code: char) {
    app.handle_key(KeyEvent::new(KeyCode::Char(code), KeyModifiers::CONTROL));
}

fn copy(app: &mut BrainDump) -> Option<String> {
    control(app, 'c');
    app.take_copy_request()
}

#[test]
fn escape_from_original_preserves_words_and_guards_discard_without_calls_or_saves() {
    for (width, height) in [(80, 24), (100, 30), (160, 40), (320, 40)] {
        for selection in ["none", "empty", "text"] {
            let root = tempfile::Builder::new()
                .prefix("tinkery-TEST-escape-")
                .tempdir()
                .unwrap();
            let host = Arc::new(Host::default());
            let mut app = BrainDump::with_host(host.clone())
                .with_sessions(Some(root.path().join("TEST-sessions")))
                .with_record_root(root.path().join("TEST-records"));
            snapshot(width, height, &mut app, false).unwrap();
            app.paste(ORIGINAL);
            key(&mut app, KeyCode::F(2));
            let deadline = Instant::now() + Duration::from_secs(3);
            while app.running() && Instant::now() < deadline {
                app.tick();
                std::thread::sleep(Duration::from_millis(1));
            }
            assert!(!app.running());
            assert_eq!(host.0.load(Ordering::SeqCst), 1);
            let originals = app.originals();
            app.paste(DRAFT);
            snapshot(width, height, &mut app, false).unwrap();
            key(&mut app, KeyCode::Tab);
            if selection == "empty" {
                app.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::SHIFT));
            }
            if selection == "text" {
                control(&mut app, 'a');
                assert_eq!(copy(&mut app).as_deref(), Some(ORIGINAL));
                key(&mut app, KeyCode::Esc);
                assert_eq!(copy(&mut app), None, "Escape must clear selection");
                // Selecting again still selects the original: the first Esc kept focus.
                control(&mut app, 'a');
                assert_eq!(copy(&mut app).as_deref(), Some(ORIGINAL));
                key(&mut app, KeyCode::Esc);
            }
            key(&mut app, KeyCode::Esc);
            assert_eq!(app.exit(), &Exit::Stay);
            assert!(
                !snapshot(width, height, &mut app, false)
                    .unwrap()
                    .contains(LOSS)
            );
            assert_eq!(app.take_copy_request(), None, "Escape must not copy");
            control(&mut app, 'a');
            assert_eq!(
                copy(&mut app).as_deref(),
                Some(DRAFT),
                "focus returned to writing"
            );

            for cancel in [KeyCode::Esc, KeyCode::Char('n')] {
                key(&mut app, KeyCode::Esc);
                assert!(
                    snapshot(width, height, &mut app, false)
                        .unwrap()
                        .contains(LOSS)
                );
                assert_eq!(app.exit(), &Exit::Stay);
                key(&mut app, cancel);
                assert!(
                    !snapshot(width, height, &mut app, false)
                        .unwrap()
                        .contains(LOSS)
                );
                assert_eq!(copy(&mut app).as_deref(), Some(DRAFT));
                assert_eq!(app.originals(), originals);
            }
            key(&mut app, KeyCode::Esc);
            key(&mut app, KeyCode::Char('y'));
            assert_eq!(app.exit(), &Exit::Menu);
            assert_eq!(app.originals(), originals);
            assert_eq!(
                host.0.load(Ordering::SeqCst),
                1,
                "Escape started another read"
            );
            assert!(app.saved().is_none() && !app.running());
            assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
        }
    }
}
