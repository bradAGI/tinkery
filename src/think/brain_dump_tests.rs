use super::*;
fn anchor(quote: &str) -> Anchor {
    Anchor {
        source: 1,
        quote: quote.into(),
        occurrence: 0,
    }
}
fn anchors(r: &BoardRequest) -> Vec<Anchor> {
    r.sources
        .iter()
        .map(|s| Anchor {
            source: s.id,
            quote: s.text.clone(),
            occurrence: 0,
        })
        .collect()
}
use std::{
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
#[derive(Default)]
pub(super) struct Recording {
    requests: Mutex<Vec<BoardRequest>>,
    fail: AtomicBool,
}
fn guess(r: &BoardRequest) -> Guess {
    Guess {
        parts: GoalParts {
            who: Some("I".into()),
            when: Some("Here".into()),
            outcome: Some(format!(
                "Model reading {} originals: make reading comfortable",
                r.sources.len()
            )),
            why: Some("I can read comfortably".into()),
            done_when: Some("reading is comfortable".into()),
            must: vec![],
            must_not: vec!["no confirmation".into()],
        },
        open: vec![],
        other_goals: vec![],
        deferred: vec![],
        unresolved_notes: vec![],
        decisions: vec![],
        uncertain: false,
        framings: vec![Framing {
            text: format!(
                "Model reading {} originals: make reading comfortable.",
                r.sources.len()
            ),
            supports: anchors(r),
        }],
        outcome: "Read comfortably in the blog and RSS reader.".into(),
        misfits: vec![],
        questions: if r.answered.contains(&"reader".into())
            || r.skipped.iter().any(|q| q.id == "reader")
        {
            vec![Question {
                suggestion: None,
                target: None,
                id: "rss-control".into(),
                text: "Who controls RSS appearance?".into(),
            }]
        } else {
            vec![
                Question {
                    suggestion: None,
                    target: None,
                    id: "reader".into(),
                    text: "What is Hamster?".into(),
                },
                Question {
                    suggestion: None,
                    target: None,
                    id: "audience".into(),
                    text: "Who needs comfortable reading?".into(),
                },
            ]
        },
        alternatives: [
            "A manual control / candidate",
            "Follow the system setting / candidate",
        ]
        .into_iter()
        .map(|label| Approach {
            label: label.into(),
            benefit: "Comfortable reading.".into(),
            cost: "Check the host.".into(),
            undo_cost: "Unverified.".into(),
        })
        .collect(),
    }
}
impl BoardHost for Recording {
    fn reshape(&self, r: BoardRequest, _: &AtomicBool) -> Result<Guess, String> {
        self.requests.lock().unwrap().push(r.clone());
        if self.fail.load(Ordering::Relaxed) {
            Err("injected failure".into())
        } else {
            Ok(guess(&r))
        }
    }
}
pub(super) fn amend_board(a: &mut BrainDump, edit: impl FnOnce(&mut Guess)) {
    let mut g = a.conversation.board().unwrap().wire();
    edit(&mut g);
    let r = BoardRequest {
        ask_counts: Default::default(),
        sources: a.conversation.sources().to_vec(),
        fragments: a.conversation.fragments().to_vec(),
        previous: None,
        skipped: a.conversation.skipped().to_vec(),
        answered: a
            .conversation
            .sources()
            .iter()
            .filter_map(|s| s.in_reply_to.clone())
            .collect(),
        settled: a.conversation.settled().to_vec(),
        layout: vec![],
    };
    a.conversation
        .set_board(Some(board::Board::verify(g, &r).unwrap()));
}
fn key(a: &mut BrainDump, code: KeyCode) {
    a.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
}
/// Runs the F1 row whose words contain `label`, as the operator does: F1, then its digit.
/// Panics, showing help, when no such row is offered right now.
pub(super) fn choose(a: &mut BrainDump, label: &str) {
    if !a.screen.help {
        key(a, KeyCode::F(1));
    }
    let help = layout::help_text(a);
    let digit = help
        .lines()
        .find_map(|line| {
            let (n, words) = line.split_once("  ")?;
            (n.len() == 1 && words.contains(label)).then(|| n.chars().next().unwrap())
        })
        .unwrap_or_else(|| panic!("no {label:?} row in:\n{help}"));
    key(a, KeyCode::Char(digit));
}
/// Whether F1 offers a row containing `label` right now.
pub(super) fn offers(a: &BrainDump, label: &str) -> bool {
    a.rows().iter().any(|row| row.label.contains(label))
}
pub(super) fn settle(a: &mut BrainDump) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while a.running() && Instant::now() < deadline {
        a.tick();
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(!a.running());
}
#[test]
fn silent_entry_intact_sources_and_deliberate_extractions_survive_answer_and_layout() {
    for (w, h) in [(80, 24), (100, 30), (160, 40), (320, 40)] {
        let host = Arc::new(Recording::default());
        let mut a = BrainDump::with_host(host.clone());
        let first = snapshot(w, h, &mut a, false).unwrap();
        assert!(first.contains("What's on your mind?"));
        assert!(!first.contains("I think this is about"));
        assert!(a.conversation.fragments().is_empty());
        let dump = "  I want a toggle.\nHamster is unfamiliar. Café 👩‍💻 matters!  ";
        a.paste(dump);
        a.tick();
        assert!(host.requests.lock().unwrap().is_empty());
        key(&mut a, KeyCode::F(2));
        settle(&mut a);
        assert_eq!(a.conversation.sources()[0].text, dump);
        assert!(
            snapshot(w, h, &mut a, false)
                .unwrap()
                .contains("Model reading 1 originals")
        );
        amend_board(&mut a, |g| {
            g.questions[0].text = "Should scratch thinking remain entirely separate unless someone explicitly chooses to carry it into the PR?".into()
        });
        assert!(
            snapshot(w, h, &mut a, false).unwrap().contains("PR?"),
            "Focused live question clipped at supported size"
        );
        assert!(
            a.conversation.fragments().is_empty(),
            "Sentence punctuation automatically created cards"
        );
        a.extract_range(1, 0, dump.find('\n').unwrap()).unwrap();
        for f in a.conversation.fragments() {
            assert_eq!(&dump[f.start..f.end], f.text);
        }
        assert!(a.copy_text().contains("Hamster"));
        assert!(
            a.copy_text()
                .contains("One question in focus\n\nShould scratch thinking")
        );
        if let CanvasNode::Text(n) = &mut a.screen.canvas.state.data.nodes[0] {
            n.x = 1234.;
            n.y = -456.;
        }
        let old = a.card_positions();
        a.paste("Hamster is my RSS reader; following the system setting is fine.");
        key(&mut a, KeyCode::F(2));
        settle(&mut a);
        assert_eq!(&a.card_positions()[..old.len()], old.as_slice());
        assert_eq!(a.conversation.focused_question().unwrap().id, "rss-control");
        let updated = snapshot(w, h, &mut a, false).unwrap();
        assert!(
            updated.contains("Model reading 2 originals"),
            "Answer did not visibly update centre"
        );
        assert!(!updated.contains("Model reading 1 originals"));
        assert_eq!(
            a.conversation.sources()[1].in_reply_to.as_deref(),
            Some("reader")
        );
        assert_eq!(a.conversation.sources()[0].text, dump);
        assert!(a.copy_text().contains("Hamster is my RSS reader"));
        assert_eq!(host.requests.lock().unwrap().len(), 2);
        assert_eq!(host.requests.lock().unwrap()[1].layout[..old.len()], old);
        a.screen.board_focus = !a.screen.board_focus;
        choose(&mut a, "Copy the reading");
        assert_eq!(a.take_copy_request(), Some(a.copy_text()));
        choose(&mut a, "Show your originals");
        assert!(
            snapshot(w, h, &mut a, false)
                .unwrap()
                .contains("Original 1")
        );
        key(&mut a, KeyCode::Esc);
        assert!(!a.screen.original);
    }
}
#[test]
fn failure_retry_skip_no_implicit_confirmation_or_new_note_edit_trap() {
    let host = Arc::new(Recording::default());
    let mut a = BrainDump::with_host(host.clone());
    snapshot(100, 30, &mut a, false).unwrap();
    a.submit();
    assert!(host.requests.lock().unwrap().is_empty());
    key(&mut a, KeyCode::Char('n'));
    key(&mut a, KeyCode::Char('e'));
    assert_eq!(a.input.text, "ne");
    a.submit();
    settle(&mut a);
    let before = a.copy_text();
    a.screen.board_focus = !a.screen.board_focus;
    choose(&mut a, "Skip this question");
    assert!(
        a.conversation.focused_question().is_none(),
        "only one model question"
    );
    a.undo_skip();
    assert_eq!(a.conversation.focused_question().unwrap().id, "reader");
    a.screen.board_focus = !a.screen.board_focus;
    host.fail.store(true, Ordering::Relaxed);
    a.paste("A second dump.");
    a.submit();
    settle(&mut a);
    assert!(a.notice.contains("injected failure"));
    assert_eq!(a.conversation.sources().len(), 2);
    assert_eq!(
        a.conversation.board().unwrap().outcome.as_deref().unwrap(),
        "reading is comfortable"
    );
    assert!(a.originals().contains("A second dump."));
    assert!(a.copy_text().contains("Nothing confirmed"));
    assert_ne!(a.copy_text(), before);
    host.fail.store(false, Ordering::Relaxed);
    a.submit();
    settle(&mut a);
    assert_eq!(a.conversation.sources().len(), 2);
    assert_eq!(a.conversation.focused_question().unwrap().id, "rss-control");
    assert!(
        host.requests
            .lock()
            .unwrap()
            .last()
            .unwrap()
            .settled
            .iter()
            .any(|s| s.question.id == "reader")
    );
}
#[test]
fn boundary_keeps_fact_and_history_guards_but_accepts_uncited_and_extra_readings() {
    let host = Arc::new(Recording::default());
    let mut a = BrainDump::with_host(host.clone());
    snapshot(100, 30, &mut a, false).unwrap();
    a.paste("Exact words. More words.");
    a.submit();
    settle(&mut a);
    let r = host.requests.lock().unwrap()[0].clone();
    let g = guess(&r);
    let mut bad = g.clone();
    bad.framings[0].supports = vec![anchor("invented")];
    assert!(bad.validate(&r).is_err());
    let mut bad = g.clone();
    bad.framings[0].supports.pop();
    assert!(bad.validate(&r).is_ok());
    let mut bad = g.clone();
    bad.questions[0]
        .text
        .push_str(" This is an unresolved note, without a question mark.");
    assert!(bad.validate(&r).is_ok());
    let mut bad = g.clone();
    bad.uncertain = true;
    assert!(
        bad.validate(&r).is_ok(),
        "Uncertainty is a hint, not a reason to force an invented second reading"
    );
    bad.framings.push(Framing {
        text: "Another possible meaning".into(),
        supports: anchors(&r),
    });
    assert!(bad.validate(&r).is_ok());
    bad.framings.push(bad.framings[0].clone());
    assert!(bad.validate(&r).is_ok());
    bad.framings.clear();
    assert!(
        bad.validate(&r).is_ok(),
        "missing parts are displayable, not invented"
    );
    let mut bad = g.clone();
    bad.outcome = "\x1b]52;c;payload".into();
    assert!(bad.validate(&r).is_err());
    let mut bad = g.clone();
    bad.alternatives.pop();
    assert!(bad.validate(&r).is_ok());
    let mut skipped = r.clone();
    skipped.skipped.push(g.questions[0].clone());
    assert!(g.validate(&skipped).is_err());
    let json = serde_json::to_string(&g).unwrap();
    let narrative = Guess::decode(
        &json.replace(
            "\"misfits\":[]",
            "\"misfits\":[\"an inferred relationship\"]",
        ),
        &r,
        &mut vec![],
    )
    .unwrap();
    assert_eq!(narrative.unresolved_notes, ["an inferred relationship"]);
    assert!(narrative.misfits.is_empty());
    assert!(serde_json::from_str::<Guess>(&(json.clone() + " extra")).is_err());
    let ignored = Guess::decode(
        &json.replace(
            "\"uncertain\":false",
            "\"uncertain\":false,\"confirmed\":true",
        ),
        &r,
        &mut vec![],
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(ignored).unwrap(),
        serde_json::to_value(Guess::decode(&json, &r, &mut vec![]).unwrap()).unwrap()
    );
    assert!(a.handoff.receipt().is_none());
}
#[test]
fn typing_during_request_is_never_replaced_by_the_result() {
    let host = Arc::new(Recording::default());
    let mut a = BrainDump::with_host(host.clone());
    snapshot(100, 30, &mut a, false).unwrap();
    a.paste("First dump.");
    a.submit();
    a.paste("Local text while thinking.");
    settle(&mut a);
    assert_eq!(a.input.text, "Local text while thinking.");
    assert_eq!(a.conversation.sources().len(), 1);
    assert!(
        a.conversation.board().is_none(),
        "Agent interrupted new typing with a framing"
    );
    assert!(a.shaping.ready.is_some());
    a.submit();
    settle(&mut a);
    assert!(a.conversation.board().is_some());
    assert_eq!(a.conversation.sources().len(), 2);
    let requests = host.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[1].sources[1].text, "Local text while thinking.");
    assert!(requests[1].previous.is_some());
}
#[test]
fn applying_a_held_reply_with_cleared_input_does_not_send_again() {
    let host = Arc::new(Recording::default());
    let mut a = BrainDump::with_host(host.clone());
    snapshot(100, 30, &mut a, false).unwrap();
    a.paste("First dump.");
    key(&mut a, KeyCode::F(2));
    a.paste("Local text while thinking.");
    settle(&mut a);
    assert!(a.shaping.ready.is_some());
    assert!(a.conversation.board().is_none());

    a.handle_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    key(&mut a, KeyCode::F(2));
    assert!(a.conversation.board().is_some());
    assert!(a.conversation.caught_up());
    assert!(a.shaping.ready.is_none());
    assert!(!a.running(), "applying the reply started another request");
    assert_eq!(a.conversation.sources().len(), 1);
    assert_eq!(a.conversation.sources()[0].text, "First dump.");
    key(&mut a, KeyCode::F(2));
    assert!(!a.running());
    assert_eq!(host.requests.lock().unwrap().len(), 1);
}
#[test]
fn a_failed_held_reply_with_cleared_input_still_retries() {
    let host = Arc::new(Recording::default());
    host.fail.store(true, Ordering::Relaxed);
    let mut a = BrainDump::with_host(host.clone());
    snapshot(100, 30, &mut a, false).unwrap();
    a.paste("First dump.");
    key(&mut a, KeyCode::F(2));
    a.paste("Local text while thinking.");
    settle(&mut a);
    assert!(a.shaping.ready.as_ref().unwrap().1.is_err());

    a.handle_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    host.fail.store(false, Ordering::Relaxed);
    key(&mut a, KeyCode::F(2));
    settle(&mut a);
    assert!(a.conversation.board().is_some());
    assert!(a.conversation.caught_up());
    assert!(a.conversation.last_failure().is_none());
    assert!(a.shaping.ready.is_none());
    let requests = host.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    for request in requests.iter() {
        assert_eq!(request.sources.len(), 1);
        assert_eq!(request.sources[0].text, "First dump.");
    }
}
#[test]
fn simulated_is_explicit_and_never_an_automatic_real_failure_fallback() {
    let mut a = BrainDump::default();
    snapshot(100, 30, &mut a, false).unwrap();
    a.paste("My own thought.");
    a.submit();
    settle(&mut a);
    assert!(
        a.conversation
            .board()
            .unwrap()
            .goal()
            .unwrap()
            .contains("Simulated")
    );
}

/// A screen point inside the first extracted card.
fn first_card_point(a: &BrainDump) -> (u16, u16) {
    let rect = a.screen.canvas.area;
    let (nx, ny) = a.screen.canvas.state.data.nodes[0].pos();
    let x = ((nx - a.screen.canvas.state.viewport_x) * a.screen.canvas.state.zoom
        + f64::from(rect.width) / 2.0) as u16
        + rect.x
        + 4;
    let y = ((ny - a.screen.canvas.state.viewport_y) * a.screen.canvas.state.zoom
        + f64::from(rect.height) / 2.0) as u16
        + rect.y
        + 2;
    (x, y)
}
fn card_mouse(a: &mut BrainDump, kind: MouseEventKind, column: u16, row: u16) {
    let area = a.screen.area;
    a.handle_mouse(
        MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        },
        area,
    );
}
#[test]
fn hiding_cards_mid_drag_releases_the_drag_so_later_moves_change_nothing() {
    let mut a = BrainDump::with_host(Arc::new(Recording::default()));
    snapshot(100, 30, &mut a, false).unwrap();
    a.paste("Read comfortably. Hamster is unfamiliar.");
    a.submit();
    settle(&mut a);
    a.extract_range(1, 0, "Read comfortably.".len()).unwrap();
    a.screen.board_focus = !a.screen.board_focus;
    choose(&mut a, "extracted cards");
    snapshot(100, 30, &mut a, false).unwrap();
    let (x, y) = first_card_point(&a);
    card_mouse(&mut a, MouseEventKind::Down(MouseButton::Left), x, y);
    card_mouse(&mut a, MouseEventKind::Drag(MouseButton::Left), x + 2, y);
    // Hiding the cards ends the gesture; the button-up never reaches the canvas.
    choose(&mut a, "extracted cards");
    choose(&mut a, "extracted cards");
    snapshot(100, 30, &mut a, false).unwrap();
    let released = a.card_positions();
    let view = (
        a.screen.canvas.state.viewport_x,
        a.screen.canvas.state.viewport_y,
    );
    card_mouse(
        &mut a,
        MouseEventKind::Drag(MouseButton::Left),
        x + 12,
        y + 4,
    );
    assert_eq!(a.card_positions(), released, "a stale drag moved a card");
    assert_eq!(
        (
            a.screen.canvas.state.viewport_x,
            a.screen.canvas.state.viewport_y
        ),
        view,
        "a stale drag panned the cards"
    );
}
#[test]
fn actual_drag_keeps_exact_words_and_layout_on_the_next_submit() {
    let host = Arc::new(Recording::default());
    let mut a = BrainDump::with_host(host);
    snapshot(100, 30, &mut a, false).unwrap();
    a.paste("Read comfortably. Hamster is unfamiliar.");
    a.submit();
    settle(&mut a);
    a.extract_range(1, 0, "Read comfortably.".len()).unwrap();
    a.screen.board_focus = !a.screen.board_focus;
    choose(&mut a, "extracted cards");
    snapshot(100, 30, &mut a, false).unwrap();
    let (x, y) = first_card_point(&a);
    let before = a.card_positions();
    for (kind, dx, dy) in [
        (MouseEventKind::Down(MouseButton::Left), 0, 0),
        (MouseEventKind::Drag(MouseButton::Left), 4, 2),
        (MouseEventKind::Up(MouseButton::Left), 4, 2),
    ] {
        card_mouse(&mut a, kind, x + dx, y + dy);
    }
    assert_ne!(
        a.card_positions(),
        before,
        "Real mouse drag did not move a card"
    );
    let moved = a.card_positions();
    assert!(a.screen.canvas.state.floating_editor.is_none());
    for (n, f) in a
        .screen
        .canvas
        .state
        .data
        .nodes
        .iter()
        .zip(a.conversation.fragments())
    {
        assert_eq!(n.text(), f.text);
    }
    a.screen.board_focus = !a.screen.board_focus;
    a.paste("It is an RSS reader.");
    a.submit();
    settle(&mut a);
    assert_eq!(&a.card_positions()[..moved.len()], moved.as_slice());
}
#[test]
fn selective_sparse_reading_highlights_sources_without_renaming_or_recolouring_words() {
    for (w, h) in [(80, 24), (100, 30), (160, 40), (320, 40)] {
        let mut a = BrainDump::with_host(Arc::new(Recording::default()));
        snapshot(w, h, &mut a, false).unwrap();
        a.paste("My week has no clear priority. Meetings are too long. A dashboard worries me.");
        a.submit();
        settle(&mut a);
        let layout = a.card_positions();
        let fragments = a.conversation.fragments().to_vec();
        let mut g = a.conversation.board().unwrap().wire();
        g.uncertain = true;
        g.framings = vec![
            Framing {
                text: "PROVISIONAL: Clarify what matters this week.".into(),
                supports: vec![anchor("My week has no clear priority.")],
            },
            Framing {
                text: "PROVISIONAL: Reduce time in meetings.".into(),
                supports: vec![anchor("Meetings are too long.")],
            },
        ];
        g.misfits = vec![anchor("A dashboard worries me.")];
        amend_board(&mut a, |current| *current = g);
        let ordinary = snapshot(w, h, &mut a, false).unwrap();
        for noise in [
            "PROVISIONAL:",
            "Supports:",
            "Success criteria",
            "Candidates /",
            "Your f1",
            "Your f2",
        ] {
            assert!(!ordinary.contains(noise), "{noise} in sparse view");
        }
        assert!(!ordinary.contains("doesn’t fit yet"));
        assert!(a.details_text().contains("Doesn’t fit yet"));
        assert!(a.screen.canvas.state.data.nodes.iter().all(|n| match n {
            CanvasNode::Text(n) => n.title.is_none(),
            _ => false,
        }));
        assert_eq!(a.card_positions(), layout);
        assert!(!a.running(), "Highlighting sent a model request");
        assert_eq!(
            a.conversation.sources().len(),
            1,
            "Highlighting was recorded as an answer"
        );
        for (n, f) in a.screen.canvas.state.data.nodes.iter().zip(&fragments) {
            assert_eq!(n.text(), f.text);
        }
        a.screen.board_focus = true;
        choose(&mut a, "Why");
        let detail = snapshot(w, h, &mut a, false).unwrap();
        assert!(detail.contains("Success criteria"));
        assert!(!detail.contains("PROVISIONAL:"));
        assert!(a.details_text().contains("A dashboard worries me."));
        key(&mut a, KeyCode::Esc);
        assert!(
            !snapshot(w, h, &mut a, true)
                .unwrap()
                .contains("Success criteria")
        );
    }
}
#[test]
fn details_are_accessible_without_stealing_literal_answer_text_or_focus() {
    let mut a = BrainDump::with_host(Arc::new(Recording::default()));
    snapshot(100, 30, &mut a, false).unwrap();
    assert!(!offers(&a, "Why"), "no details before a submit");
    a.paste("A question worth understanding.");
    a.submit();
    settle(&mut a);
    a.paste("draft");
    let input = a.input.text.clone();
    key(&mut a, KeyCode::Char('d'));
    assert_eq!(a.input.text, input + "d");
    assert!(!a.screen.details);
    let input = a.input.text.clone();
    for mode in 0..4 {
        a.screen.board_focus = mode == 1;
        a.screen.original = mode == 2;
        a.screen.help = mode == 3;
        choose(&mut a, "Why");
        assert!(a.screen.details);
        assert!(!a.screen.original && !a.screen.help);
        assert_eq!(a.screen.board_focus, mode == 1);
        let frame = snapshot(100, 30, &mut a, false).unwrap();
        assert!(!frame.contains("provisional"));
        assert!(frame.contains("Success criteria"));
        key(&mut a, KeyCode::Esc);
        assert!(!a.screen.details);
        assert_eq!(a.input.text, input);
        assert!(!a.running());
    }
    a.screen.details = true;
    a.screen.original = true;
    choose(&mut a, "Why");
    assert!(
        a.screen.details && !a.screen.original,
        "Hidden details incorrectly toggled off"
    );
    a.screen.details = false;
    a.screen.original = true;
    let area = a.screen.area;
    a.handle_mouse(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: area.width - 3,
            row: 1,
            modifiers: KeyModifiers::NONE,
        },
        area,
    );
    assert!(a.screen.help && !a.screen.original);
    assert_eq!(a.input.text, input);
    key(&mut a, KeyCode::Esc);
    a.screen.board_focus = !a.screen.board_focus;
    choose(&mut a, "Why");
    assert!(a.screen.details);
    key(&mut a, KeyCode::Esc);
    assert!(!a.screen.details);
}
#[test]
fn clipping_is_visible_and_original_is_one_action_away() {
    let mut a = BrainDump::default();
    snapshot(80, 24, &mut a, false).unwrap();
    let source = "Management wants a dashboard that shows everything people do all week, which feels like surveillance.";
    let source = source.repeat(12);
    a.paste(&source);
    a.submit();
    settle(&mut a);
    let view = snapshot(80, 24, &mut a, false).unwrap();
    assert!(view.contains("more"), "No source scrolling cue");
    assert!(
        view.lines()
            .nth(usize::from(a.screen.source_area.y))
            .unwrap()
            .contains("Management"),
        "Source title was replaced with a clipping hint"
    );
    assert!(!view.contains("… Ctrl-O"));
    assert_eq!(a.conversation.sources()[0].text, source);
    assert!(a.conversation.fragments().is_empty());
    choose(&mut a, "Show your originals");
    let original = snapshot(80, 24, &mut a, false).unwrap();
    assert!(original.contains("surveillance."));
}
#[test]
fn unresolved_roles_can_overlap_and_long_readings_do_not_veto_a_board() {
    let host = Arc::new(Recording::default());
    let mut a = BrainDump::with_host(host.clone());
    a.paste("Meaning. A tangent.");
    a.submit();
    settle(&mut a);
    let r = host.requests.lock().unwrap()[0].clone();
    let mut g = guess(&r);
    g.framings[0].supports = vec![anchor("Meaning.")];
    g.misfits = vec![anchor("A tangent.")];
    assert!(g.validate(&r).is_ok());
    g.framings[0].supports.push(anchor("A tangent."));
    assert!(g.validate(&r).is_ok());
    g.framings[0].supports.pop();
    g.parts.outcome = Some(vec!["word"; 46].join(" "));
    assert!(g.validate(&r).is_ok());
    assert_eq!(
        agent_text("PROVISIONAL: uncertain meaning\nPROVISIONAL: conditional cost"),
        "Uncertain meaning\nConditional cost"
    );
}

#[cfg(unix)]
#[test]
fn board_process_uses_only_shaping_guidance_and_disabled_resources() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let host = Arc::new(Recording::default());
    let mut a = BrainDump::with_host(host.clone());
    snapshot(100, 30, &mut a, false).unwrap();
    a.paste("My exact words.");
    a.submit();
    settle(&mut a);
    let r = host.requests.lock().unwrap()[0].clone();
    let response = serde_json::to_string(&guess(&r)).unwrap();
    let program = dir.path().join("pi");
    std::fs::write(&program,format!("#!/usr/bin/env python3\nimport sys,json,os\nr=json.load(sys.stdin)\nif r.get('kind')=='meaning-preservation': print('{{\"missing\":[],\"false_choice\":false}}');sys.exit(0)\np=sys.argv[sys.argv.index('--system-prompt')+1]\nassert 'Distill whatever arrived' in p\nassert 'FAKE_LEDGER_REPLY_RULE' not in p and 'DO_NOT_RUN_LATER_GATE' not in p\nassert 'Proposed mechanisms remain candidates' in p\nassert 'preserve compatible aims rather than forcing a choice' in p\nassert 'No research, tools, execution, approvals, ledger' in p\nassert 'Never invent properties of unknown names' in p\nassert 'only when its answer matters to the next useful step' in p\nassert 'Fill missing required parts FIRST' not in p\nassert 'never leaving it only in unresolved_notes' in p and 'that is when questions stop' in p\nassert 'decisions (open choices only the person can make' in p\nassert 'never status notes, disclaimers' in p\nassert 'Put each answer in the one part it answers' in p and 'edit that line instead of adding another' in p\nassert 'existing source ID' in p and 'whole Unicode graphemes' in p\nassert not any(example in p for example in ['went well','way off','usual week','no targets','no dashboard','five emails','phones','no gamification','Kirra','restaurateur'])\nassert 'never field definitions' in p and 'never facts an agent could find out' in p\nassert 'know, decide, or do' not in p\nassert 'two to four' not in p and 'EXACT keys' not in p and 'at most 45' not in p\nassert all(x in sys.argv for x in ['--no-tools','--no-session','--no-extensions','--no-mcp','--no-context-files'])\nassert 'PI_SESSION_FILE' not in os.environ\nassert r['sources'][0]['text']=='My exact words.'\nprint({response:?})\n")).unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let pi = ModelHost::new(crate::pi::Harness::Pi, program, "test/model".into()).unwrap();
    let actual = pi.reshape(r, &AtomicBool::new(false)).unwrap();
    assert_eq!(actual.parts, guess(&host.requests.lock().unwrap()[0]).parts);
}

#[test]
fn annotations_match_exact_occurrences_and_do_not_cut_unicode_graphemes() {
    let sources = vec![Source {
        id: 1,
        text: "Café 👩‍💻 same. Café 👩‍💻 same.".into(),
        in_reply_to: None,
    }];
    let second = Anchor {
        source: 1,
        quote: "Café 👩‍💻".into(),
        occurrence: 1,
    };
    let range = second.range(&sources).unwrap();
    assert_eq!(&sources[0].text[range.clone()], second.quote);
    assert_eq!(range.start, sources[0].text.rfind("Café").unwrap());
    for bad in [
        Anchor {
            source: 2,
            ..second.clone()
        },
        Anchor {
            quote: "café 👩‍💻".into(),
            ..second.clone()
        },
        Anchor {
            quote: "Cafe\u{301} 👩‍💻".into(),
            ..second.clone()
        },
        Anchor {
            occurrence: 2,
            ..second.clone()
        },
        Anchor {
            quote: "👩".into(),
            occurrence: 0,
            ..second.clone()
        },
        Anchor {
            quote: "".into(),
            ..second.clone()
        },
    ] {
        assert!(
            bad.range(&sources).is_err(),
            "{bad:?} was silently normalized/repaired"
        );
    }
}

#[test]
fn original_stays_plain_ink_even_with_cited_and_unplaced_words() {
    use ratatui::style::{Color, Modifier};
    for (w, h) in [(80, 24), (100, 30), (160, 40), (320, 40)] {
        for mono in [false, true] {
            let host = Arc::new(Recording::default());
            let mut a = BrainDump::with_host(host.clone());
            let original = "Café 👩‍💻 same. Café 👩‍💻 same.\nEnd.";
            a.paste(original);
            a.submit();
            settle(&mut a);
            let support = Anchor {
                source: 1,
                quote: "Café 👩‍💻".into(),
                occurrence: 1,
            };
            let supported = support.range(a.conversation.sources()).unwrap();
            let unresolved = anchor("End.").range(a.conversation.sources()).unwrap();
            amend_board(&mut a, |g| {
                g.framings[0].supports = vec![support];
                g.misfits = vec![anchor("End.")];
            });
            let mut terminal =
                ratatui::Terminal::new(ratatui::backend::TestBackend::new(w, h)).unwrap();
            terminal
                .draw(|f| render(f, &mut a, Palette::new(mono)))
                .unwrap();
            let area = a.screen.source_area;
            let wrapped = Note::new(original).wrap(area.width);
            for (i, g) in original
                .grapheme_indices(true)
                .filter(|(_, g)| !g.chars().all(char::is_whitespace))
            {
                let (_, row, col) = wrapped
                    .positions
                    .iter()
                    .find(|(index, _, _)| *index == i)
                    .unwrap();
                let cell =
                    &terminal.backend().buffer()[(area.x + *col as u16, area.y + *row as u16)];
                assert_eq!(cell.symbol(), g);
                // P10: cited and unplaced words are not painted; the original stays plain ink.
                assert!(
                    !cell
                        .modifier
                        .intersects(Modifier::BOLD | Modifier::UNDERLINED),
                    "painted cell at {i} (supported {}, unresolved {})",
                    supported.contains(&i),
                    unresolved.contains(&i)
                );
                assert_ne!(cell.bg, Color::Rgb(219, 235, 217), "green highlight at {i}");
                if !mono {
                    assert_eq!(
                        cell.fg,
                        Color::Rgb(16, 15, 15),
                        "Agent recoloured authored words"
                    );
                }
            }
            assert_eq!(a.conversation.sources()[0].text, original);
            assert!(a.conversation.fragments().is_empty());
        }
    }
}

#[test]
fn deliberate_keyboard_extraction_is_exact_linked_and_has_no_provider_side_effect() {
    let host = Arc::new(Recording::default());
    let mut a = BrainDump::with_host(host.clone());
    let text = "Café 👩‍💻 is the whole thought. That is my true goal.";
    a.paste(text);
    a.submit();
    settle(&mut a);
    snapshot(100, 30, &mut a, false).unwrap();
    assert!(a.conversation.fragments().is_empty());
    a.screen.board_focus = !a.screen.board_focus;
    for _ in "Café 👩‍💻".graphemes(true) {
        a.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::SHIFT));
    }
    a.handle_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::CONTROL));
    assert_eq!(a.conversation.fragments().len(), 1);
    let f = &a.conversation.fragments()[0];
    assert_eq!(f.source, 1);
    assert_eq!(f.start, 0);
    assert_eq!(f.end, "Café 👩‍💻".len());
    assert_eq!(f.text, "Café 👩‍💻");
    assert_eq!(host.requests.lock().unwrap().len(), 1);
    assert!(!a.running());
    assert_eq!(a.conversation.sources()[0].text, text);
    choose(&mut a, "extracted cards");
    let view = snapshot(100, 30, &mut a, false).unwrap();
    let (_, ny) = a.screen.canvas.state.data.nodes[0].pos();
    let y = ((ny - a.screen.canvas.state.viewport_y) * a.screen.canvas.state.zoom
        + f64::from(a.screen.canvas.area.y)
        + f64::from(a.screen.canvas.area.height) / 2.0)
        .round() as usize;
    assert!(
        view.lines().nth(y).unwrap().contains("Café"),
        "Card title was replaced with metadata"
    );
    assert!(!view.contains("Enter: source"));
    assert!(!view.contains("… Ctrl-O"));
    a.screen.canvas.state.selection.select_only("f1".into());
    key(&mut a, KeyCode::Enter);
    assert!(!a.screen.show_extractions);
    assert_eq!(a.screen.selection, Some((0, "Café 👩‍💻".len())));
    assert!(a.extract_range(1, 0, 4).is_err(), "UTF-8 cut accepted");
    let emoji = text.find('👩').unwrap();
    assert!(
        a.extract_range(1, emoji, emoji + "👩".len()).is_err(),
        "Grapheme cut accepted"
    );
    assert!(a.extract_range(99, 0, 4).is_err());
    assert!(a.extract_range(1, 0, "Café 👩‍💻".len()).is_err());
    assert_eq!(a.conversation.fragments().len(), 1);
}

#[test]
fn bad_span_keeps_the_previous_reading_and_originals_without_fallback() {
    let host = Arc::new(Recording::default());
    let mut a = BrainDump::with_host(host.clone());
    a.paste("An intact goal. That is my true goal.");
    a.submit();
    settle(&mut a);
    let request = host.requests.lock().unwrap()[0].clone();
    let paper = a.copy_text();
    let mut bad = guess(&request);
    bad.framings[0].supports = vec![anchor("A paraphrased goal.")];
    a.apply_result(request, Ok(bad));
    assert!(a.notice.contains("exact source substring"));
    assert_eq!(a.copy_text(), paper);
    assert!(a.conversation.fragments().is_empty());
    assert_eq!(
        a.conversation.sources()[0].text,
        "An intact goal. That is my true goal."
    );
}

#[test]
fn originals_label_exists_once_only_after_source_submit_and_unmarked_words_are_neutral() {
    let host = Arc::new(Recording::default());
    let mut a = BrainDump::with_host(host.clone());
    assert!(
        !snapshot(80, 24, &mut a, false)
            .unwrap()
            .contains("originals")
    );
    assert!(!offers(&a, "Show your originals"));
    a.paste("Meaning. Unmarked words are not misfits.");
    a.submit();
    settle(&mut a);
    let mut g = guess(&host.requests.lock().unwrap()[0]);
    g.framings[0].supports = vec![anchor("Meaning.")];
    assert!(
        g.validate(&host.requests.lock().unwrap()[0]).is_ok(),
        "Forced whole-source coverage"
    );
    amend_board(&mut a, |current| *current = g);
    assert!(
        !snapshot(80, 24, &mut a, false)
            .unwrap()
            .contains("Ctrl-O originals")
    );
    a.handle_key(KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE));
    assert!(a.screen.help && !a.screen.details);
    snapshot(80, 24, &mut a, false).unwrap();
    key(&mut a, KeyCode::Esc);
    choose(&mut a, "Show your originals");
    assert!(a.screen.original && !a.screen.details);
    assert!(a.conversation.fragments().is_empty());
}

#[test]
fn both_is_an_answer_with_a_visible_update_and_full_settled_question() {
    for (w, h) in [(80, 24), (100, 30), (160, 40), (320, 40)] {
        let host = Arc::new(Recording::default());
        let mut a = BrainDump::with_host(host.clone());
        a.paste("An intact system goal.");
        a.submit();
        settle(&mut a);
        let question = a.conversation.focused_question().unwrap().text.clone();
        a.paste("both");
        a.submit();
        settle(&mut a);
        let r = host.requests.lock().unwrap()[1].clone();
        assert_eq!(r.settled.len(), 1);
        assert_eq!(r.settled[0].question.text, question);
        assert_eq!(r.sources[r.settled[0].source - 1].text, "both");
        assert_eq!(r.sources[1].in_reply_to.as_deref(), Some("reader"));
        let frame = snapshot(w, h, &mut a, false).unwrap();
        assert!(
            !frame.contains("Reading updated from your answer"),
            "No redundant banner"
        );
        assert!(!frame.contains("Settled: both"));
        assert!(a.details_text().contains("both"));
        assert!(frame.contains("Who controls RSS appearance?"));
        // The answered question is no longer asked; it only labels your answer on the left.
        assert!(!frame.contains("One thing I'm not sure about\nWhat is Hamster?"));
        assert!(
            frame
                .lines()
                .filter(|l| l.contains("What is Hamster?"))
                .all(|l| l.contains("your answer to:"))
        );
        assert!(a.details_text().contains(&question));
        assert!(a.copy_text().contains("Answer / original 2: both"));
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(w, h)).unwrap();
        terminal
            .draw(|f| render(f, &mut a, Palette::new(false)))
            .unwrap();
        assert!(a.conversation.changed_lines().is_some());
        a.paste("new local typing");
        a.tick();
        assert!(a.conversation.changed_lines().is_none());
        assert_eq!(a.input.text, "new local typing");
        a.input = Note::new("Only final judgment.");
        a.submit();
        settle(&mut a);
        assert_eq!(a.conversation.settled().len(), 2);
        assert_eq!(a.conversation.settled()[1].question.id, "rss-control");
        assert!(a.conversation.focused_question().is_none());
        assert!(
            snapshot(w, h, &mut a, false)
                .unwrap()
                .contains("I think you mean…")
        );
        assert_eq!(
            a.conversation.sources()[2].in_reply_to.as_deref(),
            Some("rss-control")
        );
    }
}

#[test]
fn answered_ids_and_equivalent_text_with_new_ids_never_regain_focus() {
    let host = Arc::new(Recording::default());
    let mut a = BrainDump::with_host(host.clone());
    a.paste("Goal.");
    a.submit();
    settle(&mut a);
    a.paste("both");
    a.submit();
    settle(&mut a);
    let r = host.requests.lock().unwrap()[1].clone();
    let mut g = guess(&r);
    g.questions.insert(
        0,
        Question {
            suggestion: None,
            target: None,
            id: "a-new-id".into(),
            text: "WHAT IS HAMSTER ?".into(),
        },
    );
    a.apply_result(r.clone(), Ok(g));
    assert_eq!(a.conversation.focused_question().unwrap().id, "rss-control");
    let mut g = guess(&r);
    g.questions = vec![Question {
        suggestion: None,
        target: None,
        id: "reader".into(),
        text: "What is Hamster?".into(),
    }];
    a.apply_result(r, Ok(g));
    assert!(a.conversation.focused_question().is_none());
    assert!(
        snapshot(100, 30, &mut a, false)
            .unwrap()
            .contains("Ready for your review")
    );
    assert_eq!(a.conversation.settled().len(), 1);
    assert_eq!(a.conversation.sources()[1].text, "both");
}

#[test]
fn reply_label_is_stable_and_empty_chrome_is_hidden() {
    let mut a = BrainDump::with_host(Arc::new(Recording::default()));
    a.paste("Short goal.");
    a.submit();
    settle(&mut a);
    amend_board(&mut a, |g| g.questions.truncate(1));
    let view = snapshot(100, 30, &mut a, false).unwrap();
    for clutter in [
        "1/1",
        "Ctrl-Pg/wheel",
        "wheel scroll",
        "0 quietly queued",
        "Later:",
        "highlight:support underline:unresolved",
        "Answer or add more",
    ] {
        assert!(!view.contains(clutter), "{clutter} in sparse view");
    }
    assert!(view.contains("your answer"));
    assert!(view.lines().rev().skip(1).all(|line| !line.contains("F2")));
    assert!(view.lines().last().unwrap().contains("F2 send"));
    assert!(!view.contains("Highlighted words support"));
    choose(&mut a, "new dump");
    assert!(a.notice.contains("not an answer"));
    assert!(
        snapshot(100, 30, &mut a, false)
            .unwrap()
            .contains("add more")
    );
    choose(&mut a, "new dump");
    a.paste("both");
    a.submit();
    settle(&mut a);
    assert!(a.conversation.sources()[1].in_reply_to.is_some());
    choose(&mut a, "new dump");
    a.paste("Separate new dump.");
    a.submit();
    settle(&mut a);
    assert!(a.conversation.sources()[2].in_reply_to.is_none());
    assert_eq!(a.conversation.settled().len(), 1);
    a.conversation.set_board(None);
    assert!(
        !snapshot(100, 30, &mut a, false)
            .unwrap()
            .contains("Highlighted words")
    );
}

#[test]
fn unchanged_reading_records_the_answer_without_claiming_a_wording_change() {
    let mut a = BrainDump::default();
    a.paste("Goal.");
    a.submit();
    settle(&mut a);
    a.paste("both");
    a.submit();
    settle(&mut a);
    assert!(
        a.conversation.changed_lines().is_none(),
        "No muting when the block did not change"
    );
    assert!(a.conversation.focused_question().is_none());
    assert_eq!(a.conversation.settled().len(), 1);
}
