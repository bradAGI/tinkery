use super::*;
use crate::seed_me::{
    ledger::fixtures::prose_reply,
    shaping::{Decided, Shaped, Shaping},
};
use crossterm::event::KeyModifiers;
use std::sync::Arc;

const GOAL: &str = "TEST I can shape a goal calmly.\nWhy: I lose the thread between tools.\nDone when: I see the next question.\nMust: Keep my words.\nMust not: Run agents.";

fn done(stage: Shaped) -> Shape {
    let mut shape = Shape::session(Path::new("/tmp/TEST-shape/session"));
    let open = shape.open.as_mut().unwrap();
    open.reading = Reading::Done(Ok(Shaping {
        ready_target: None,
        goal: Some(GOAL.into()),
        success_open: false,
        stage,
        version: Some(3),
        confirmation_note: None,
        confirmation: vec![],
        teach_back: vec![],
        deferred: None,
        commandless_check: None,
        signoff: None,
        open: vec![],
        decided: vec![
            Decided {
                title: "TEST where records live".into(),
                answer: "Local for now.\nSynced later if I ask.".into(),
                authority: "user".into(),
            },
            Decided {
                title: "TEST format".into(),
                answer: "Markdown, as the README says.".into(),
                authority: "delegated".into(),
            },
        ],
    }));
    shape
}

fn screen(shape: &mut Shape, width: u16, height: u16, no_color: bool) -> String {
    let backend = ratatui::backend::TestBackend::new(width, height);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let area = Rect::new(2, 3, width - 4, height - 4);
            shape.render(frame, area, Palette::new(no_color))
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

/// Picks the F1 row whose words contain `label` the way the operator does: by its number.
fn choose(shape: &mut Shape, label: &str) {
    let help = shape.help_text();
    let digit = help
        .lines()
        .find_map(|line| {
            let (n, words) = line.split_once("  ")?;
            (n.len() == 1 && words.contains(label)).then(|| n.chars().next().unwrap())
        })
        .unwrap_or_else(|| panic!("F1 offers no {label:?}:\n{help}"));
    assert!(shape.pick(key(KeyCode::Char(digit))), "{help}");
}

/// Whether F1 offers a row whose words contain `label` right now.
fn offers(shape: &Shape, label: &str) -> bool {
    shape.help_text().lines().any(|line| {
        line.split_once("  ")
            .is_some_and(|(n, w)| n.len() == 1 && w.contains(label))
    })
}

#[test]
fn so_far_names_open_questions_and_the_answers_they_wait_for() {
    for (width, height) in [(80, 24), (100, 30)] {
        for no_color in [false, true] {
            let mut shape = done(question());
            let open = shape.open.as_mut().unwrap();
            let Reading::Done(Ok(shaping)) = &mut open.reading else {
                panic!()
            };
            shaping.goal = Some("TEST goal".into());
            shaping.decided.clear();
            shaping.open = vec!["TEST A".into(), "TEST B — waits for: TEST A, TEST C".into()];
            choose(&mut shape, "So far");
            let text = screen(&mut shape, width, height, no_color);
            assert!(text.contains("Open questions"), "{text}");
            assert!(
                text.contains("TEST B — waits for: TEST A, TEST C"),
                "{text}"
            );
            assert!(text.contains("esc close"), "{text}");
        }
    }
}

#[test]
fn new_answer_actions_cannot_escape_an_ending_or_objection() {
    for ending in [true, false] {
        let mut shape = done(question()).with_agent(Some(agent(1)));
        let open = shape.open.as_mut().unwrap();
        if ending {
            open.asking = Some(Ending::Stopped);
        } else {
            open.objecting = true;
        }
        assert!(!offers(&shape, "Defer") && !offers(&shape, "Delegate"));
        let open = shape.open.as_ref().unwrap();
        assert!(open.form.is_none() && open.running.is_none() && open.writing.is_none());
        assert!(open.notice.is_none());
        assert_eq!(open.asking.is_some(), ending);
        assert_eq!(open.objecting, !ending);
    }
}

#[test]
fn crowded_question_bars_keep_more_and_back_visible_and_f1_lists_every_action() {
    for (width, height) in [(80, 24), (100, 30)] {
        for hard in [true, false] {
            let mut stage = question();
            let Shaped::Question {
                found, hard: undo, ..
            } = &mut stage
            else {
                panic!()
            };
            *found = vec!["TEST long evidence ".repeat(300)];
            *undo = hard;
            let mut shape = done(stage).with_agent(Some(agent(0)));
            shape.open.as_mut().unwrap().assumed.push(Assumption {
                text: "TEST default".into(),
                why: "TEST equal".into(),
            });
            let text = screen(&mut shape, width, height, true);
            let bar = text.lines().last().unwrap();
            for key in ["F1 more", "esc back"] {
                assert!(bar.contains(key), "{text}");
            }
            for label in [
                "Defer",
                "Object",
                if hard { "Show the options" } else { "Delegate" },
            ] {
                assert!(offers(&shape, label), "{label}: {}", shape.help_text());
            }
        }
    }
}

#[test]
fn forms_do_not_retarget_after_a_stale_read_and_held_keys_do_nothing() {
    let mut shape = done(question()).with_agent(Some(agent(1)));
    for digit in ['1', '2'] {
        let mut held = key(KeyCode::Char(digit));
        held.kind = KeyEventKind::Repeat;
        assert!(!shape.pick(held));
        let open = shape.open.as_ref().unwrap();
        assert!(open.form.is_none() && open.running.is_none() && open.writing.is_none());
    }
    choose(&mut shape, "Defer");
    let open = shape.open.as_mut().unwrap();
    let Reading::Done(Ok(shaping)) = &mut open.reading else {
        panic!()
    };
    shaping.version = Some(4);
    let Shaped::Question { id, .. } = &mut shaping.stage else {
        panic!()
    };
    *id = "TEST-another-question".into();
    let Some(Typing {
        act: Act::Respond { id, expected, .. },
        ..
    }) = open.typing()
    else {
        panic!()
    };
    assert_eq!(id, "storage");
    assert_eq!(expected, 3);
}

#[test]
fn only_flagged_operator_decisions_offer_a_revision_form() {
    for revisable in [false, true] {
        for (width, height) in [(100, 30), (80, 24)] {
            let mut shape = done(Shaped::Review {
                id: "TEST-answer".into(),
                title: "TEST answer".into(),
                answer: "TEST old".into(),
                found: vec![],
                revisable,
            })
            .with_agent(Some(agent(0)));
            let text = screen(&mut shape, width, height, true);
            assert!(text.contains("F1 more"), "{text}");
            assert_eq!(offers(&shape, "Revise"), revisable);
            if revisable {
                choose(&mut shape, "Revise");
                assert!(shape.open.as_ref().unwrap().form.is_some());
                shape.paste("TEST replacement\r\nwords ");
                assert!(screen(&mut shape, width, height, true).contains("replacement answer"));
                shape.key(key(KeyCode::Esc));
                assert!(shape.open.as_ref().unwrap().form.is_none());
                assert!(shape.open.as_ref().unwrap().running.is_none());
            }
        }
    }
}

#[test]
fn deferral_form_keeps_fields_and_original_answer_without_writes() {
    let mut shape = done(question()).with_agent(Some(agent(0)));
    shape.paste("TEST original answer");
    choose(&mut shape, "Defer");
    shape.paste("TEST wait\r\nfor scope ");
    let text = screen(&mut shape, 80, 24, true);
    assert!(text.contains("Gate: TEST gate"), "{text}");
    shape.key(key(KeyCode::F(2)));
    shape.paste("TEST before implementation ");
    shape.key(key(KeyCode::Tab));
    assert_eq!(
        shape.open.as_ref().unwrap().form.as_ref().unwrap().action(),
        AnswerAction::Defer {
            reason: "TEST wait\r\nfor scope ".into(),
            revisit: "TEST before implementation ".into(),
        }
    );
    shape.key(key(KeyCode::Esc));
    let open = shape.open.as_ref().unwrap();
    assert_eq!(open.typed.text(), "TEST original answer");
    assert!(open.running.is_none() && open.writing.is_none());
    shape.held = true;
    assert!(!offers(&shape, "Defer") && !offers(&shape, "Delegate"));
    shape.key(key(KeyCode::F(2)));
    assert!(shape.open.as_ref().unwrap().form.is_none());
    assert!(shape.open.as_ref().unwrap().running.is_none());
}

#[test]
fn a_flagged_answer_has_a_read_only_route_and_an_explicit_budget_free_keep_key() {
    let stage = Shaped::Review {
        revisable: false,
        id: "storage".into(),
        title: "TEST storage".into(),
        answer: "TEST local".into(),
        found: vec!["TEST conflicting observation".into()],
    };
    let mut readonly = done(stage.clone());
    let text = screen(&mut readonly, 80, 24, true);
    assert!(
        text.contains("Current question: review flagged answer"),
        "{text}"
    );
    assert!(
        text.contains("Next: reopen this goal with Shape set up; F1 shows how."),
        "{text}"
    );
    assert!(!text.contains("F2 keep"), "{text}");
    let mut writable = done(stage).with_agent(Some(agent(0)));
    let text = screen(&mut writable, 80, 24, true);
    assert!(text.contains("F2 keep with reason"), "{text}");
    writable.key(key(KeyCode::F(2)));
    assert!(screen(&mut writable, 80, 24, true).contains(WORD_REFUSALS[2]));
    assert!(writable.open.as_ref().unwrap().writing.is_none());
    assert!(writable.open.as_ref().unwrap().running.is_none());
    assert_eq!(writable.agent.as_ref().unwrap().remaining(), 0);
}

#[test]
fn seed_ready_points_to_approve_and_copies_nothing() {
    for ended in [false, true] {
        let mut shape = done(Shaped::Ready {
            revision: Some("r2".into()),
            ended,
        });
        for (width, height) in [(80, 24), (100, 30)] {
            let text = screen(&mut shape, width, height, true);
            assert!(
                text.contains("open Approve from the menu to inspect this seed."),
                "{text}"
            );
            assert!(!text.contains("copy intake"), "{text}");
            assert!(!text.contains("<run>"), "{text}");
        }
        let before = screen(&mut shape, 80, 24, true);
        shape.key(key(KeyCode::Char('c')));
        assert_eq!(screen(&mut shape, 80, 24, true), before);
    }
}

#[test]
fn each_stage_shows_the_goal_as_confirmed_then_one_next_step() {
    let question = Shaped::Question {
        gate: "TEST gate".into(),
        id: "storage".into(),
        title: "Where records live".into(),
        ask: "Keep records local or synced?".into(),
        found: vec![
            "README.md:3 says local only".into(),
            "no sync client".into(),
        ],
        suggestion: Some("Local.".into()),
        options: vec![],
        against: None,
        hard: false,
    };
    let cases = [
        (question, "Where records live Keep records local or synced?"),
        (Shaped::Waiting, "Shape hasn't asked its next question yet."),
        (
            Shaped::Drafted {
                revision: Some("r2".into()),
                text: None,
            },
            "Seed draft r2 is waiting for you to confirm it.",
        ),
        (
            Shaped::Ready {
                revision: Some("r2".into()),
                ended: false,
            },
            "Seed ready: you confirmed r2, and it still matches.",
        ),
        (
            Shaped::Ready {
                revision: Some("r2".into()),
                ended: true,
            },
            "This session is completed, so nothing in it can change.",
        ),
        (
            Shaped::Unverified {
                reason: "the seed changed after revision r2 was confirmed".into(),
            },
            "but Tinkery can't verify it: the seed changed after revision r2",
        ),
        (
            Shaped::Ended {
                status: "completed".into(),
            },
            "This session has ended (completed).",
        ),
        (
            Shaped::Unknown {
                what: "This session has no confirmed goal.".into(),
            },
            "Shape can't tell where this seed stands.",
        ),
    ];
    for (stage, said) in cases {
        // Without an agent, ending is the only write: the stage decides whether it's offered.
        let ending = match stage {
            Shaped::Question { .. } | Shaped::Waiting | Shaped::Drafted { .. } => Some("Stop"),
            Shaped::Ready { ended: false, .. } => Some("End this session"),
            _ => None,
        };
        for (width, height) in [(100, 30), (80, 24)] {
            for no_color in [false, true] {
                let mut shape = done(stage.clone());
                let text = screen(&mut shape, width, height, no_color);
                let (first, rest) = GOAL.split_once('\n').unwrap();
                assert!(text.contains(first), "{first} missing:\n{text}");
                for line in rest.lines() {
                    assert!(!text.contains(line), "{line} on screen:\n{text}");
                }
                let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
                assert!(flat.contains(said), "{said} missing:\n{text}");
                assert_eq!(text.matches("Next:").count(), 1, "{text}");
                assert!(!text.contains("Option"), "{text}");
                assert!(!text.contains("/tmp/TEST-shape"), "{text}");
                assert_eq!(text.lines().last().unwrap().trim(), "F1 more   esc back");
                for label in ["Stop", "End this session"] {
                    assert_eq!(offers(&shape, label), ending == Some(label), "{label}");
                }
                assert_eq!(so_far(&mut shape, width, height, no_color), GOAL);
            }
        }
    }
}

/// Opens "So far" from F1 and returns the goal it shows, after checking the page around it and
/// that Esc closes it.
fn so_far(shape: &mut Shape, width: u16, height: u16, no_color: bool) -> String {
    choose(shape, "So far");
    assert!(!shape.takes_text(), "So far took words");
    let text = screen(shape, width, height, no_color);
    let rows: Vec<&str> = text.lines().map(str::trim_start).collect();
    let start = rows
        .iter()
        .position(|r| *r == "Your goal, as you confirmed it")
        .unwrap_or_else(|| panic!("no goal heading:\n{text}"));
    let end = start + rows[start..].iter().position(|r| r.is_empty()).unwrap();
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        flat.contains(
            "Decided TEST where records live: Local for now. Synced later if I ask. (you)"
        ),
        "{text}"
    );
    assert!(
        flat.contains("TEST format: Markdown, as the README says. (delegated)"),
        "a decision the operator didn't settle read as theirs:\n{text}"
    );
    assert!(text.contains("esc close"), "{text}");
    assert!(!text.contains("esc back"), "a key bar on So far:\n{text}");
    shape.key(key(KeyCode::Char('x')));
    shape.key(key(KeyCode::Esc));
    assert!(
        shape.open.as_ref().unwrap().so_far.is_none(),
        "Esc kept So far open"
    );
    assert!(
        shape.open.as_ref().unwrap().typed.is_empty(),
        "So far took a letter"
    );
    rows[start + 1..end].join("\n")
}

#[test]
fn the_question_is_jade_and_the_suggestion_reads_as_a_guess_without_colour_too() {
    for no_color in [false, true] {
        let mut shape = done(Shaped::Question {
            gate: "TEST gate".into(),
            id: "t".into(),
            title: "Label T".into(),
            ask: "Q?".into(),
            found: vec![],
            suggestion: Some("S".into()),
            options: vec![],
            against: None,
            hard: false,
        });
        let backend = ratatui::backend::TestBackend::new(100, 30);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|f| shape.render(f, Rect::new(2, 3, 96, 26), Palette::new(no_color)))
            .unwrap();
        let buffer = terminal.backend().buffer();
        let find = |needle: &str| {
            (0..30)
                .find_map(|y| {
                    let row: String = (0..100).map(|x| buffer[(x, y)].symbol()).collect();
                    row.find(needle).map(|x| buffer[(x as u16, y)].style())
                })
                .unwrap()
        };
        let palette = Palette::new(no_color);
        for (needle, style) in [("Q?", palette.jade), ("Label T", palette.muted)] {
            assert_eq!(find(needle).add_modifier, style.add_modifier, "{needle}");
            if !no_color {
                assert_eq!(find(needle).fg, style.fg, "{needle}");
            }
        }
        assert!(
            find("Suggestion: S")
                .add_modifier
                .contains(ratatui::style::Modifier::ITALIC)
        );
    }
}

#[test]
fn failures_and_missing_setup_are_said_plainly_with_the_reason_in_help() {
    let mut shape = Shape::session(Path::new("/tmp/TEST-shape/session"));
    let text = screen(&mut shape, 100, 30, true);
    assert!(text.contains("Reading where this seed stands."), "{text}");
    assert!(!text.contains("--seed-me"), "{text}");
    let mut failed = done(Shaped::Waiting);
    failed.open.as_mut().unwrap().reading = Reading::Done(Err("Tinkery refused: no ledger".into()));
    let text = screen(&mut failed, 100, 30, true);
    assert!(
        text.contains("Tinkery refused to read this session: no ledger"),
        "{text}"
    );
    assert!(text.contains("Next: Esc goes back"), "{text}");
    failed.open.as_mut().unwrap().reading =
        Reading::Done(Err("Cannot locate Shape instructions: x".into()));
    let text = screen(&mut failed, 100, 30, true);
    assert!(text.contains("Shape couldn't read this session."), "{text}");
    assert!(!text.contains("Cannot locate"), "{text}");
    assert!(
        failed
            .help_text()
            .contains("Cannot locate Shape instructions: x")
    );
    assert!(failed.help_text().contains("/tmp/TEST-shape/session"));
}

#[test]
fn the_list_holds_only_goals_waiting_for_a_seed_and_esc_steps_back_one_level() {
    let session = |name: &str, stage| Session {
        id: Some(name.into()),
        path: PathBuf::from(format!("/tmp/TEST-shape/{name}")),
        goal: format!("{name} goal\nWhy: test."),
        stage,
        updated: String::new(),
    };
    let mut shape = Shape::list();
    assert!(shape.wants_sessions());
    assert!(screen(&mut shape, 100, 30, true).contains("Reading your sessions."));
    let sessions = [
        session("saved", Stage::GoalSaved),
        session("ready", Stage::SeedUnchecked),
        session("drafted", Stage::SeedDrafted),
        session("ended", Stage::Completed),
        session("unknown", Stage::Unknown),
    ];
    shape.offer(Ok(&sessions), 0);
    let text = screen(&mut shape, 100, 30, true);
    assert!(
        text.contains("› saved goal") && text.contains("goal saved"),
        "{text}"
    );
    assert!(
        text.contains("drafted goal") && text.contains("seed drafted"),
        "{text}"
    );
    for hidden in ["ready goal", "ended goal", "unknown goal"] {
        assert!(!text.contains(hidden), "{text}");
    }
    assert_eq!(shape.key(key(KeyCode::Down)), Leave::Stay);
    assert_eq!(shape.key(key(KeyCode::Enter)), Leave::Stay);
    let text = screen(&mut shape, 100, 30, true);
    assert!(text.contains("Reading where this seed stands."), "{text}");
    assert!(shape.help_text().contains("/tmp/TEST-shape/drafted"));
    assert_eq!(shape.key(key(KeyCode::Esc)), Leave::Stay);
    // Back at the list, Shape reads again: a goal whose seed was confirmed meanwhile leaves it.
    assert!(shape.wants_sessions());
    shape.offer(Ok(&sessions), 0);
    assert!(screen(&mut shape, 100, 30, true).contains("› drafted goal"));
    let mut moved = sessions.clone();
    moved[2].stage = Stage::SeedUnchecked;
    assert_eq!(shape.key(key(KeyCode::Enter)), Leave::Stay);
    assert_eq!(shape.key(key(KeyCode::Esc)), Leave::Stay);
    shape.offer(Ok(&moved), 0);
    let text = screen(&mut shape, 100, 30, true);
    assert!(
        text.contains("› saved goal") && !text.contains("drafted goal"),
        "{text}"
    );
    assert_eq!(shape.key(key(KeyCode::Esc)), Leave::Back);
    let mut empty = Shape::list();
    empty.offer(Ok(&sessions[1..2]), 0);
    assert!(screen(&mut empty, 100, 30, true).contains("No goals are waiting for a seed."));
    let mut failed = Shape::list();
    failed.offer(Err("TEST read failed".into()), 0);
    assert!(screen(&mut failed, 100, 30, true).contains("couldn't be read"));
    assert!(failed.help_text().contains("TEST read failed"));
    let mut direct = done(Shaped::Waiting);
    assert_eq!(direct.key(key(KeyCode::Esc)), Leave::Back);
}

#[test]
fn incomplete_lists_keep_the_count_and_f1_route() {
    for (width, height) in [(100, 30), (80, 24)] {
        for no_color in [false, true] {
            let mut shape = Shape::list();
            shape.offer(Ok(&[]), 1);
            let text = screen(&mut shape, width, height, no_color);
            let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
            assert!(
                flat.contains("1 record couldn't be read. F1 shows why."),
                "{text}"
            );
            assert!(!flat.contains("No goals are waiting"), "{text}");
            assert!(
                shape
                    .help_text()
                    .contains("Some session ledgers could not be read")
            );
            shape.offer(Ok(&[]), 2);
            assert!(
                screen(&mut shape, width, height, no_color).contains("2 records couldn't be read.")
            );
            shape.offer(Err("TEST root failure".into()), 2);
            let failed = screen(&mut shape, width, height, no_color);
            assert!(!failed.contains("2 records"), "{failed}");
            assert!(shape.help_text().contains("TEST root failure"));
        }
    }
}

fn agent(budget: usize) -> Arc<Agent> {
    Arc::new(
        Agent::new(
            crate::pi::Harness::Pi,
            "/tmp/TEST-shape/never-run-pi".into(),
            "test/model".into(),
            "low".into(),
            None,
            crate::pi::CallBudget::new(budget),
            None,
        )
        .unwrap(),
    )
}

fn question() -> Shaped {
    Shaped::Question {
        gate: "TEST gate".into(),
        id: "storage".into(),
        title: "Where records live".into(),
        ask: "Keep records local or synced?".into(),
        found: vec![],
        suggestion: None,
        options: vec![],
        against: None,
        hard: false,
    }
}

#[test]
fn typing_writes_nothing_and_f2_refuses_empty_words_or_an_empty_budget_before_starting() {
    let mut shape = done(question()).with_agent(Some(agent(0)));
    assert!(shape.takes_text());
    assert_eq!(shape.key(key(KeyCode::F(2))), Leave::Stay);
    let text = screen(&mut shape, 100, 30, true);
    assert!(
        text.contains("Type your answer first; nothing was saved."),
        "{text}"
    );
    for c in "Local? Yes".chars() {
        shape.key(key(KeyCode::Char(c)));
    }
    shape.key(key(KeyCode::Backspace));
    shape.key(key(KeyCode::Char('s')));
    shape.paste("\nkept");
    shape.key(key(KeyCode::F(2)));
    let open = shape.open.as_ref().unwrap();
    assert!(open.running.is_none(), "a step started with no budget");
    assert_eq!(open.typed.text(), "Local? Yes\nkept");
    let text = screen(&mut shape, 100, 30, true);
    assert!(
        text.contains("  your answer\n  Local? Yes\n  kept"),
        "{text}"
    );
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        flat.contains("No drafting runs are left: this launch's allowance is 0."),
        "{text}"
    );
    assert!(
        flat.contains("Next: add runs from F1, Model and runs, then send again."),
        "{text}"
    );
    choose(&mut shape, "Model and runs");
    assert!(
        shape.take_model_request() && !shape.take_model_request(),
        "the row asks the launch for the panel once"
    );
    assert_eq!(text.matches("Next:").count(), 1, "{text}");
    assert_eq!(
        text.lines().last().unwrap().trim(),
        "F2 save + look   F1 more   esc back"
    );
}

#[test]
fn a_correction_typed_where_confirm_goes_carries_into_revise_the_draft() {
    let dir = tempfile::tempdir().unwrap();
    let words = "TEST keep the cache local, not shared";
    let mut shape = reading(
        done(Shaped::Drafted {
            revision: Some("r1".into()),
            text: Some("status: draft\n# TEST seed\nrevision: r1\n".into()),
        })
        .with_agent(Some(agent(1))),
        dir.path(),
    );
    screen(&mut shape, 80, 24, true);
    shape.paste(words);
    shape.key(key(KeyCode::F(2)));
    assert!(shape.open.as_ref().unwrap().running.is_none());
    assert!(screen(&mut shape, 80, 24, true).contains(WORD_REFUSALS[1]));
    choose(&mut shape, "Revise the draft");
    let text = screen(&mut shape, 80, 24, true);
    assert!(
        text.contains("your change") && text.contains(words),
        "{text}"
    );
    assert!(
        text.lines().last().unwrap().contains("F2 save + redraft"),
        "{text}"
    );
    shape.key(key(KeyCode::Esc));
    assert!(screen(&mut shape, 80, 24, true).contains("Change cancelled. Nothing was saved."));
    assert_eq!(shape.open.as_ref().unwrap().typed.text(), words);
    choose(&mut shape, "Revise the draft");
    shape.key(key(KeyCode::F(2)));
    assert!(
        shape.open.as_ref().unwrap().running.is_some(),
        "F2 did not save the change and redraft"
    );
    // The word that confirms is never carried in as a change.
    let mut shape = reading(
        done(Shaped::Drafted {
            revision: Some("r1".into()),
            text: Some("status: draft\n# TEST seed\nrevision: r1\n".into()),
        })
        .with_agent(Some(agent(1))),
        dir.path(),
    );
    shape.paste("confirm");
    choose(&mut shape, "Revise the draft");
    shape.key(key(KeyCode::F(2)));
    assert!(shape.open.as_ref().unwrap().running.is_none());
    assert!(
        screen(&mut shape, 80, 24, true)
            .contains("Type what should change first; nothing was saved.")
    );
}

#[test]
fn teach_back_prefills_exact_confirmation_words_without_sharing_edits() {
    let dir = tempfile::tempdir().unwrap();
    for words in ["", "  TEST café\r\nkeep these words.  "] {
        let mut shape = reading(
            done(Shaped::Drafted {
                revision: Some("r1".into()),
                text: Some("status: draft\n# TEST seed\nrevision: r1\n".into()),
            })
            .with_agent(Some(agent(0))),
            dir.path(),
        );
        shape.paste(words);
        choose(&mut shape, "Teach back");
        let open = shape.open.as_ref().unwrap();
        assert_eq!(open.aside.as_ref().unwrap().1.text(), words);
        assert_eq!(open.typed.text(), words);
        shape.paste("TEST changed teach-back");
        let open = shape.open.as_ref().unwrap();
        assert!(
            open.aside
                .as_ref()
                .unwrap()
                .1
                .text()
                .contains("TEST changed teach-back")
        );
        assert_eq!(open.typed.text(), words);
        shape.key(key(KeyCode::Esc));
        let open = shape.open.as_ref().unwrap();
        assert!(open.aside.is_none() && open.running.is_none());
        assert_eq!(open.typed.text(), words);
    }
}

#[test]
fn no_next_line_repeats_the_one_action_the_key_bar_names() {
    let dir = tempfile::tempdir().unwrap();
    let seed = Shaped::Drafted {
        revision: Some("r2".into()),
        text: Some("status: draft\n# Seed: TEST\nrevision: r2\n".into()),
    };
    for (stage, keys) in [
        (question(), "F2 save + look   F1 more   esc back"),
        (
            Shaped::Waiting,
            "F2 look into the next question   F1 more   esc back",
        ),
        (seed, "F2 confirm seed r2   F1 more   esc back"),
    ] {
        for (width, height) in [(100, 30), (80, 24)] {
            let mut shape = reading(done(stage.clone()).with_agent(Some(agent(1))), dir.path());
            let text = screen(&mut shape, width, height, true);
            assert_eq!(text.matches("Next:").count(), 0, "{text}");
            // A bar too wide for 80 columns closes up rather than lose Esc.
            let bar = text.lines().last().unwrap().trim();
            assert!(bar == keys || bar == keys.replace("   ", "  "), "{text}");
        }
    }
}

#[test]
fn while_a_step_runs_the_question_and_the_answer_stay_in_view() {
    let dir = tempfile::tempdir().unwrap();
    let mut shape = reading(done(question()).with_agent(Some(agent(1))), dir.path());
    shape.paste("TEST keep it local");
    shape.key(key(KeyCode::F(2)));
    assert!(
        shape.open.as_ref().unwrap().running.is_some(),
        "no step started"
    );
    for (width, height) in [(100, 30), (80, 24)] {
        let text = screen(&mut shape, width, height, true);
        for shown in [
            "Where records live",
            "Keep records local or synced?",
            "your answer",
            "TEST keep it local",
            "Saving your answer.",
        ] {
            assert!(text.contains(shown), "{shown} missing:\n{text}");
        }
        assert!(!text.contains("Nothing found"), "{text}");
        assert_eq!(text.matches("Next:").count(), 0, "{text}");
        assert_eq!(text.lines().last().unwrap().trim(), "esc stop");
    }
    // So far waits until the step is done, and help says what Esc does now.
    assert!(!offers(&shape, "So far"));
    assert!(
        shape
            .help_text()
            .contains("A step is running. Esc stops it.\nEsc stop\n\nF1 close")
    );
}

#[test]
fn a_tall_question_scrolls_while_the_answer_and_every_key_stay_in_view() {
    let dir = tempfile::tempdir().unwrap();
    // One finding can still overflow; the others belong behind So far.
    let found = vec![format!(
        "src/TEST/1.rs:1: {}",
        "this path prints the helper's error verbatim, stderr included. ".repeat(18)
    )];
    let tall = Shaped::Question {
        gate: "TEST gate".into(),
        id: "storage".into(),
        title: "Where records live".into(),
        ask: "Keep records local or synced?".into(),
        found,
        suggestion: Some("Local, matching the README.".into()),
        options: vec![],
        against: None,
        hard: false,
    };
    let mut shape = reading(done(tall).with_agent(Some(agent(1))), dir.path());
    shape.paste("TEST first line\nTEST last line");
    let text = screen(&mut shape, 80, 24, true);
    assert!(text.contains("Keep records local or synced?"), "{text}");
    assert!(
        text.contains("  your answer\n  TEST first line\n  TEST last line"),
        "{text}"
    );
    let bar = text.lines().last().unwrap().trim();
    let keys = "pgup/dn read   F2 save + look   F1 more   esc back";
    assert!(bar == keys || bar == keys.replace("   ", "  "), "{text}");
    // Scrolling moves only the reading; the answer stays where it is.
    for _ in 0..30 {
        shape.key(key(KeyCode::PageDown));
    }
    let text = screen(&mut shape, 80, 24, true);
    assert!(text.contains("F1 picks another folder."), "{text}");
    assert!(text.contains("TEST last line"), "{text}");
}

#[test]
fn without_an_agent_shape_only_reads_and_letters_scroll() {
    let mut shape = done(question());
    assert!(!shape.takes_text());
    shape.key(key(KeyCode::Char('j')));
    assert!(shape.open.as_ref().unwrap().typed.is_empty());
    assert_eq!(shape.scroll, 1);
    shape.key(key(KeyCode::F(2)));
    assert!(shape.open.as_ref().unwrap().running.is_none());
    let text = screen(&mut shape, 100, 30, true);
    assert!(
        text.contains("reopen this goal with Shape set up; F1 shows how."),
        "{text}"
    );
}

#[test]
fn a_draft_shows_acceptance_checks_as_written_and_other_lines_unchanged() {
    let records = crate::seed_me::ledger::Value::parse(
        r#"[{"statement":"Visiting `/foo` returns 404","kind":"cmd","source":"TEST goal","cmd":"grep -c '\\*' TEST-file","expect":"1","cwd":"."}]"#,
    )
    .unwrap();
    let seed = format!(
        "status: draft\n# Seed: TEST\nrevision: r2\n\n## You are confirming\nTEST \\`kept\\`\n{}## Affected users and systems\nTEST \\`kept too\\`\n",
        crate::seed_me::ledger::checks_block(&records).unwrap()
    );
    let mut shape = done(Shaped::Drafted {
        revision: Some("r2".into()),
        text: Some(seed),
    })
    .with_agent(Some(agent(1)));
    let text = screen(&mut shape, 100, 30, true);
    assert!(
        text.contains("  Acceptance criteria\n\n  Visiting `/foo` returns 404\n  check: `grep -c '\\*' TEST-file`\n"),
        "{text}"
    );
    assert!(
        text.contains("TEST \\`kept\\`") && text.contains("TEST \\`kept too\\`"),
        "{text}"
    );
}

#[test]
fn a_draft_shows_the_seed_and_f2_needs_the_operators_own_words() {
    let seed =
        "status: draft\n# Seed: TEST\nrevision: r2\n\n## You are confirming\nTEST seed body.\n";
    let mut shape = done(Shaped::Drafted {
        revision: Some("r2".into()),
        text: Some(seed.into()),
    })
    .with_agent(Some(agent(1)));
    let text = screen(&mut shape, 100, 30, true);
    assert!(text.contains("TEST seed body."), "{text}");
    assert!(!text.contains("Must not: Run agents."), "{text}");
    assert!(
        text.contains("  Seed draft r2\n\n  Seed: TEST\n\n  You are confirming\n  TEST seed body."),
        "{text}"
    );
    for raw in ["status:", "revision:", "#"] {
        assert!(!text.contains(raw), "{raw} shown:\n{text}");
    }
    assert!(
        text.contains("  your words\n  type confirm, then F2\n\n  Confirming the seed is not approval to build."),
        "{text}"
    );
    shape.key(key(KeyCode::F(2)));
    assert!(shape.open.as_ref().unwrap().running.is_none());
    assert!(screen(&mut shape, 100, 30, true).contains(WORD_REFUSALS[1]));
}

#[test]
fn the_reply_is_a_question_or_a_seed_and_never_a_confirmation() {
    use step::{Next, decode};
    let asked = decode(
        r#"Here it is:
```json
{"next":"question","id":"Storage","question":"Local?","found":[{"id":"read-1","claim":{"type":"observation","scope":"README"},"checked":"README.md:3","observed":"local only"}],"suggestion":null}
```"#, "")
    .unwrap();
    let Next::Question(ask) = asked else {
        panic!("{asked:?}")
    };
    assert_eq!(
        (ask.id.as_str(), ask.label.as_str(), ask.gate.as_str()),
        ("Storage", "Local?", "Local?")
    );
    assert_eq!(ask.found.len(), 1);
    assert_eq!(ask.suggestion, None);
    assert_eq!(
        decode("{\"next\":\"seed\",\"review\":{\"contradictions\":[],\"decided\":[],\"done\":[],\"failure_cases\":[\"TEST wrong result must fail\"]},\"confirming\":\" Build it. \",\"checks\":[{\"statement\":\"TEST\",\"kind\":\"judge\",\"source\":\"TEST\"}],\"found\":[]}", "").unwrap(),
        Next::Seed {
            confirming: serde_json::json!(" Build it. "),
            review: Ok(serde_json::from_value(serde_json::json!({"contradictions":[],"decided":[],"done":[],"failure_cases":[{"statement":"TEST","case":"TEST wrong result must fail"}]})).unwrap()),
            checks: serde_json::json!([{"statement":"TEST","kind":"judge","source":"TEST"}]),
            found: vec![],
            resolves: None,
        }
    );
    let Next::Seed { found, .. } = decode(
        r#"{"next":"seed","review":{"contradictions":[],"decided":[],"done":[],"failure_cases":["TEST wrong result must fail"]},"confirming":"Build it.","checks":[{"statement":"TEST","kind":"judge","source":"TEST"}],"found":[{"id":"read-1","claim":{"type":"observation","scope":"src/lib.rs"},"checked":"src/lib.rs:9","observed":"the palette"}]}"#, "")
    .unwrap() else {
        panic!("not a seed")
    };
    assert_eq!(
        (found[0].checked.as_str(), found[0].observed.as_str()),
        ("src/lib.rs:9", "the palette")
    );
    for reply in [
        "{\"next\":\"confirm\",\"revision\":\"r1\"}",
        "{\"next\":\"question\"}",
        "{\"next\":\"seed\"}",
        "no json",
        "",
    ] {
        assert!(decode(reply, "").is_err(), "{reply}");
    }
}

#[test]
fn stacker_r1_draft_stores_records_instead_of_model_markdown() {
    use crate::seed_me::{Affirmed, Config, answering};
    let root = tempfile::Builder::new()
        .prefix("tinkery-TEST-214-")
        .tempdir()
        .unwrap();
    let receipt = Config::new(Some(root.path().join("TEST-sessions")))
        .confirm(Affirmed {
            goal: GOAL.into(),
            outcome: "TEST Stacker".into(),
            options: vec![],
            originals: vec![],
            context: "{}".into(),
            source: "TEST fixture, not approval".into(),
        })
        .unwrap();
    let original = include_str!("../../tests/fixtures/stacker-r1-seed-contract.md");
    let legacy = original
        .split_once("## You are confirming\n")
        .unwrap()
        .1
        .split_once("## Evidence and uncertainty\n")
        .unwrap()
        .0
        .trim();
    let version = answering::read(&receipt.session).unwrap().version();
    let mut reply: serde_json::Value =
        serde_json::from_str(include_str!("../../tests/fixtures/stacker-r1-reply.json")).unwrap();
    reply["confirming"] = prose_reply(reply["confirming"].as_str().unwrap());
    // The historical reply is reused with this test's goal; retain its explicit criterion too.
    reply["checks"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({
            "statement":"I see the next question.","kind":"judge","source":"TEST confirmed goal"
        }));
    assert!(
        answering::draft_seed(
            &receipt.session,
            version,
            &serde_json::json!(legacy),
            &reply["checks"],
            &[],
            root.path(),
            None,
        )
        .is_err()
    );
    answering::draft_seed(
        &receipt.session,
        version,
        &reply["confirming"],
        &reply["checks"],
        &[],
        root.path(),
        None,
    )
    .unwrap();
    let stored: serde_json::Value =
        serde_json::from_slice(&std::fs::read(receipt.session.join("seed-checks.json")).unwrap())
            .unwrap();
    assert_eq!(stored, reply["checks"]);
    let (before_criteria, criteria) = legacy.split_once("## Acceptance criteria\n").unwrap();
    let (_, after_criteria) = criteria
        .split_once("## Affected users and systems")
        .unwrap();
    assert_eq!(
        reply["confirming"],
        prose_reply(&format!(
            "{before_criteria}## Affected users and systems{after_criteria}"
        ))
    );
    assert_eq!(reply["found"].as_array().unwrap().len(), 13);
    let refused = step::decode(
        &format!(
            "My reading is provisional.\n{}\nNo checks were run.",
            include_str!("../../tests/fixtures/stacker-r1-reply.json")
        ),
        "",
    )
    .unwrap_err();
    assert!(
        refused.contains("findings are incomplete or use an unsupported format"),
        "{refused}"
    );
    let seed_before = std::fs::read(receipt.session.join("seed-contract.md")).unwrap();
    let records_before = std::fs::read(receipt.session.join("seed-checks.json")).unwrap();
    let mut invalid = vec![
        serde_json::json!([]),
        serde_json::json!({}),
        serde_json::Value::Null,
    ];
    for (key, value) in [
        ("kind", serde_json::json!("unknown")),
        ("source", serde_json::json!("")),
        ("statement", serde_json::json!(42)),
        ("extra", serde_json::json!("unknown field")),
        ("cmd", serde_json::json!("not allowed on judge")),
    ] {
        let mut bad = stored.clone();
        bad[0][key] = value;
        invalid.push(bad);
    }
    for cwd in [
        "/tmp/TEST",
        "../TEST",
        "a/../TEST",
        "C:/TEST",
        "a\\TEST",
        "a\nb",
        "a\rb",
    ] {
        let mut bad = stored.clone();
        bad[1]["cwd"] = cwd.into();
        invalid.push(bad);
    }
    for key in ["missing", "expect", "cwd", "statement", "source"] {
        let mut bad = stored.clone();
        bad[1].as_object_mut().unwrap().remove(key);
        invalid.push(bad);
    }
    for bad in invalid {
        assert!(
            answering::draft_seed(
                &receipt.session,
                version,
                &reply["confirming"],
                &bad,
                &[],
                root.path(),
                None,
            )
            .is_err(),
            "accepted invalid checks: {bad}"
        );
        assert_eq!(
            std::fs::read(receipt.session.join("seed-contract.md")).unwrap(),
            seed_before
        );
        assert_eq!(
            std::fs::read(receipt.session.join("seed-checks.json")).unwrap(),
            records_before
        );
    }
    let render = crate::seed_me::ledger::checks_block(
        &crate::seed_me::ledger::read_checks(&receipt.session).unwrap(),
    )
    .unwrap();
    // The host writes canonical LF headings even when the model's prose uses CRLF.
    let crlf = serde_json::Value::Object(
        reply["confirming"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(key, value)| {
                (
                    key.clone(),
                    value.as_str().unwrap().replace('\n', "\r\n").into(),
                )
            })
            .collect(),
    );
    answering::draft_seed(
        &receipt.session,
        version,
        &crlf,
        &reply["checks"],
        &[],
        root.path(),
        None,
    )
    .unwrap();
    let draft = std::fs::read_to_string(receipt.session.join("seed-contract.md")).unwrap();
    assert!(draft.contains(&render));
    assert!(draft.contains("## Problem statement\nYou played Stacker"));
    assert!(
        draft.contains("You are confirming the goal: create a software version of Stacker.\r\n")
    );
    while crate::seed_me::shaping::read(&receipt.session)
        .unwrap()
        .commandless_check
        .is_some()
    {
        crate::seed_me::ledger::repair_check(
            &receipt.session,
            version,
            &std::fs::read_to_string(receipt.session.join("seed-contract.md")).unwrap(),
            "TEST operator inspects this requirement.",
            true,
            None,
        )
        .unwrap();
    }
    answering::confirm_seed(
        &receipt.session,
        crate::seed_me::answering::read(&receipt.session)
            .unwrap()
            .version(),
        "r2",
        &std::fs::read_to_string(receipt.session.join("seed-contract.md")).unwrap(),
        "TEST fixture confirmation only",
        None,
    )
    .unwrap();
    assert!(matches!(
        crate::seed_me::shaping::read(&receipt.session)
            .unwrap()
            .stage,
        Shaped::Ready { .. }
    ));
    let confirmation: serde_json::Value = serde_json::from_slice(
        &std::fs::read(receipt.session.join("seed-confirmation.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(confirmation["schema_version"], 2);
    assert!(confirmation["checks_sha256"].as_str().is_some());
    std::fs::write(receipt.session.join("seed-checks.json"), "[]").unwrap();
    assert!(!matches!(
        crate::seed_me::shaping::read(&receipt.session)
            .unwrap()
            .stage,
        Shaped::Ready { .. }
    ));
    assert!(
        receipt.session.join("seed-checks.json").exists(),
        "Stacker r1 still uses handwritten criteria instead of bound records"
    );
}

#[test]
fn issue_299_review_keeps_failure_cases_separate_from_success_criteria() {
    use crate::seed_me::{Affirmed, Config, ledger, shaping};

    let cases = [
        (
            "A saved loan must not disappear after a real close and reopen.",
            "The list survives closing and reopening.",
        ),
        (
            "Attempting to save a loan with one required field missing must fail or be rejected.",
            "The tool records a title, borrower, and date.",
        ),
    ];
    let temp = tempfile::Builder::new()
        .prefix("tinkery-TEST-299-")
        .tempdir()
        .unwrap();
    let receipt = Config::new(Some(temp.path().join("TEST-sessions")))
        .confirm(Affirmed {
            goal: GOAL.into(),
            outcome: "TEST loan review".into(),
            options: vec![],
            originals: vec![],
            context: "{}".into(),
            source: "TEST fixture, not real approval".into(),
        })
        .unwrap();
    let seed = "status: draft\n# TEST loan review\nrevision: r1\n\nTEST draft.\n";
    std::fs::write(receipt.session.join("seed-contract.md"), seed).unwrap();
    let review = ledger::CompletionReview {
        contradictions: vec![],
        decided: vec![],
        done: vec![],
        failure_cases: cases
            .iter()
            .map(|(case, statement)| ledger::FailureCase {
                case: (*case).into(),
                statement: (*statement).into(),
            })
            .collect(),
    };
    ledger::record_teach_back(
        &receipt.session,
        1,
        "r1",
        seed,
        &ledger::TeachBackRecord::Review(&review),
        "2026-10-21T00:00:00+00:00",
        None,
    )
    .unwrap();
    let draft = shaping::read(&receipt.session).unwrap();
    assert_eq!(
        draft
            .confirmation
            .iter()
            .take(6)
            .map(String::as_str)
            .collect::<Vec<_>>(),
        [
            "Drafting step guess: Nothing observed done.",
            "Drafting step guess:",
            "Must fail: A saved loan must not disappear after a real close and reopen.",
            "  Check: The list survives closing and reopening.",
            "Must fail: Attempting to save a loan with one required field missing must fail or be rejected.",
            "  Check: The tool records a title, borrower, and date.",
        ]
    );
    for (width, height) in [(100, 30), (80, 24)] {
        for no_color in [false, true] {
            let mut view = done(draft.stage.clone()).with_agent(Some(agent(0)));
            view.open.as_mut().unwrap().reading = Reading::Done(Ok(draft.clone()));
            let mut seen = [false; 2];
            for _ in 0..200 {
                let rendered = screen(&mut view, width, height, no_color);
                let flat = rendered.split_whitespace().collect::<Vec<_>>().join(" ");
                for forbidden in [
                    "Drafting step guess: If ",
                    "., ",
                    ". must fail",
                    "Must fail: The list survives closing and reopening.",
                ] {
                    assert!(!flat.contains(forbidden), "{rendered}");
                }
                for (index, (case, statement)) in cases.iter().enumerate() {
                    if flat.contains(&format!("Must fail: {case} Check: {statement}")) {
                        assert!(
                            rendered
                                .lines()
                                .any(|l| l.trim_start().starts_with("Must fail:"))
                        );
                        assert!(
                            rendered
                                .lines()
                                .any(|l| l.trim_start().starts_with("Check:"))
                        );
                        assert!(flat.contains("esc back"), "{rendered}");
                        seen[index] = true;
                    }
                }
                // Down edits confirmation words; inspect each reading offset instead.
                view.scroll = view.scroll.saturating_add(1);
            }
            assert!(
                seen.iter().all(|s| *s),
                "Missing review at {width}x{height}: {seen:?}"
            );
        }
    }
}

// Pinned consumer expectations are deliberately independent of the producer's heading list.
fn missing_intake_sections(seed: &str, consumer: &str) -> Vec<String> {
    consumer
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter(|section| !seed.lines().any(|line| line == format!("## {section}")))
        .map(str::to_owned)
        .collect()
}

#[cfg(unix)]
#[test]
fn a_change_to_the_draft_is_the_operators_decision_and_the_redraft_carries_it() {
    use crate::{
        pi::CallBudget,
        seed_me::{Affirmed, Config, shaping},
    };
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        time::{Duration, Instant},
    };

    let temp = tempfile::Builder::new()
        .prefix("tinkery-TEST-614-")
        .tempdir()
        .unwrap();
    let goal = "TEST I can check a seed.\nWhy: Intake needs an eval.\nDone when: Existing output is correct.\n- New output is correct.\n- The result feels calm.\nMust: Keep my words; existing check is ./TEST-check with expected output TEST existing output in cwd .\nMust not: Build without approval.";
    let receipt = Config::new(Some(temp.path().join("TEST-sessions")))
        .confirm(Affirmed {
            goal: goal.into(),
            outcome: "TEST checks".into(),
            options: vec![],
            originals: vec![],
            context: "{}".into(),
            source: "TEST fixture, not real approval".into(),
        })
        .unwrap();
    let confirming = prose_reply(include_str!("../../tests/fixtures/shape-seed-reply.txt"));
    let confirming_json = serde_json::to_string(&confirming).unwrap();
    let checks = include_str!("../../tests/fixtures/shape-checks.json");
    fs::write(
        receipt.session.join("seed-contract.md"),
        "status: draft\n# TEST\nrevision: r1\n\n## You are confirming\nTEST old draft.\n",
    )
    .unwrap();
    let words = "  TEST keep the cache local, not shared café  ";
    let program = temp.path().join("TEST-pi");
    fs::write(
        &program,
        format!(
            r##"#!/usr/bin/env python3
import json, pathlib, sys
request = sys.stdin.read()
pathlib.Path('TEST-request.json').write_text(request)
print(json.dumps({{'next': 'seed', 'confirming': json.loads({confirming_json:?}), 'checks': json.loads({checks:?}), 'review': {{'contradictions': [], 'decided': [], 'done': [], 'failure_cases': ['TEST wrong result must fail' for _ in json.loads({checks:?})]}}, 'found': []}}))
"##
        ),
    )
    .unwrap();
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
    let version = answered_probe(&receipt.session);
    let agent = Arc::new(
        step::Agent::new(
            crate::pi::Harness::Pi,
            program,
            "test/stub".into(),
            String::new(),
            Some(temp.path().into()),
            CallBudget::new(1),
            None,
        )
        .unwrap(),
    );
    let running = step::Running::start(
        step::Work::Change {
            revision: "r1".into(),
            words: words.into(),
            expected: version,
            folder: temp.path().into(),
        },
        receipt.session.clone(),
        agent.clone(),
    );
    let deadline = Instant::now() + Duration::from_secs(20);
    let result = loop {
        if let Some(result) = running.poll() {
            break result;
        }
        assert!(Instant::now() < deadline, "stub step timed out");
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(result, Ok(step::Done::Drafted));
    assert_eq!(agent.remaining(), 0);
    let request = fs::read_to_string(temp.path().join("TEST-request.json")).unwrap();
    assert!(
        request.contains(&serde_json::to_string(words).unwrap()),
        "the drafting step never saw the change: {request}"
    );
    let Shaped::Drafted {
        revision,
        text: Some(text),
    } = shaping::read(&receipt.session).unwrap().stage
    else {
        panic!("not a draft")
    };
    assert_eq!(revision.as_deref(), Some("r2"));
    let decisions = text
        .split("## Decisions (from the ledger)")
        .nth(1)
        .unwrap_or_default();
    assert!(
        decisions.contains(&format!("- change-to-r1 [user] {}", words.trim())),
        "the redraft's decisions lack the operator's change:\n{text}"
    );
}

#[cfg(unix)]
#[test]
fn a_stub_step_preserves_a_check_or_operator_judgment_for_every_done_when() {
    use crate::{
        pi::CallBudget,
        seed_me::{Affirmed, Config, answering, shaping},
    };
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        time::{Duration, Instant},
    };

    let temp = tempfile::Builder::new()
        .prefix("tinkery-TEST-183-")
        .tempdir()
        .unwrap();
    let goal = "TEST I can check a seed.\nWhy: Intake needs an eval.\nDone when: Existing output is correct.\n- New output is correct.\n- The result feels calm.\nMust: Keep my words; existing check is ./TEST-check with expected output TEST existing output in cwd .\nMust not: Build without approval.";
    let receipt = Config::new(Some(temp.path().join("TEST-sessions")))
        .confirm(Affirmed {
            goal: goal.into(),
            outcome: "TEST checks".into(),
            options: vec![],
            originals: vec![],
            context: "{}".into(),
            source: "TEST fixture, not real approval".into(),
        })
        .unwrap();
    let confirming = prose_reply(include_str!("../../tests/fixtures/shape-seed-reply.txt"));
    let confirming_json = serde_json::to_string(&confirming).unwrap();
    let checks = include_str!("../../tests/fixtures/shape-checks.json");
    fs::write(temp.path().join("TEST-read.md"), "TEST observed content\n").unwrap();
    let local_today = || {
        let output = std::process::Command::new("date")
            .arg("+%F")
            .output()
            .unwrap();
        assert!(output.status.success());
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    };
    let date_before = local_today();
    // Upgrade an older installation's undated draft through the actual drafting path.
    fs::write(
        receipt.session.join("seed-contract.md"),
        "status: draft\n# TEST\nrevision: r7\n\n## You are confirming\nTEST old draft.\n",
    )
    .unwrap();
    // A stub can prove instructions and transport, not a real model's choice of checks.
    let program = temp.path().join("TEST-pi");
    fs::write(
        &program,
        format!(
            r##"#!/usr/bin/env python3
import json, pathlib, sys
request = json.load(sys.stdin)
prompt = sys.argv[sys.argv.index('--system-prompt') + 1]
rules = ['every Done when', 'Check records', 'checks', 'provisional', 'you judge', 'Do not run',
         'Problem statement', 'Proposed outcome', 'Acceptance criteria', 'Constraints and boundaries',
         'Seed reply fields', 'named string fields', 'Section content guidance']
missing = [rule for rule in rules if rule not in prompt]
pathlib.Path('TEST-missing-rules.json').write_text(json.dumps(missing))
assert not missing, 'missing check-drafting instructions: ' + str(missing)
assert request['goal'] == {goal:?}
assert sys.argv[sys.argv.index('--tools') + 1] == 'read,grep,find,ls'
print(json.dumps({{'next': 'seed', 'confirming': json.loads({confirming_json:?}), 'checks': json.loads({checks:?}), 'review': {{'contradictions': [], 'decided': [], 'done': [], 'failure_cases': ['TEST wrong result must fail' for _ in json.loads({checks:?})]}}, 'found': [
    {{'id': 'TEST-read-fact', 'claim': {{'type': 'observation', 'scope': 'TEST-read.md:1'}}, 'checked': 'TEST-read.md:1', 'observed': 'TEST observed content'}}]}}))
"##
        ),
    )
    .unwrap();
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
    for name in ["TEST-check", "TEST-new-check"] {
        let command = temp.path().join(name);
        fs::write(&command, "#!/bin/sh\ntouch TEST-CHECK-WAS-RUN\n").unwrap();
        fs::set_permissions(&command, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let version = answered_probe(&receipt.session);
    let agent = Arc::new(
        step::Agent::new(
            crate::pi::Harness::Pi,
            program,
            "test/stub".into(),
            String::new(),
            Some(temp.path().into()),
            CallBudget::new(1),
            None,
        )
        .unwrap(),
    );
    let running = step::Running::start(
        step::Work::Look {
            feedback: None,
            expected: version,
            folder: temp.path().into(),
        },
        receipt.session.clone(),
        agent.clone(),
    );
    let deadline = Instant::now() + Duration::from_secs(20);
    let result = loop {
        if let Some(result) = running.poll() {
            break result;
        }
        assert!(Instant::now() < deadline, "stub step timed out");
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(
        fs::read_to_string(temp.path().join("TEST-missing-rules.json")).unwrap(),
        "[]",
        "stub did not receive per-criterion check instructions"
    );
    assert_eq!(result, Ok(step::Done::Drafted));
    assert_eq!(agent.remaining(), 0);
    let draft = shaping::read(&receipt.session).unwrap();
    assert_eq!(draft.goal.as_deref(), Some(goal));
    let checks_value: serde_json::Value = serde_json::from_str(checks).unwrap();
    let mut expected_review = vec![
        "Drafting step guess: Nothing observed done.".to_owned(),
        "Must fail: TEST wrong result must fail".to_owned(),
    ];
    expected_review.extend(
        checks_value
            .as_array()
            .unwrap()
            .iter()
            .map(|c| format!("Check: {}", c["statement"].as_str().unwrap())),
    );
    for (width, height) in [(100, 30), (80, 24)] {
        let mut view = done(draft.stage.clone()).with_agent(Some(agent.clone()));
        view.open.as_mut().unwrap().reading = Reading::Done(Ok(draft.clone()));
        let mut seen = vec![false; expected_review.len()];
        for _ in 0..200 {
            let rendered = screen(&mut view, width, height, true);
            let flat = rendered.split_whitespace().collect::<Vec<_>>().join(" ");
            for (index, line) in expected_review.iter().enumerate() {
                if let Some(position) = flat.find(line) {
                    assert!(position < flat.find("esc back").unwrap(), "{rendered}");
                    seen[index] = true;
                }
            }
            view.scroll += 1;
        }
        assert!(
            seen.iter().all(|s| *s),
            "Review missing above F2 at {width}x{height}: {seen:?}"
        );
    }
    let Shaped::Drafted {
        revision,
        text: Some(text),
    } = draft.stage
    else {
        panic!("not a draft")
    };
    assert_eq!(revision.as_deref(), Some("r8"));
    for paragraph in confirming
        .as_object()
        .unwrap()
        .values()
        .map(|value| value.as_str().unwrap())
    {
        assert!(
            text.contains(paragraph),
            "model prose changed or dropped: {paragraph}"
        );
    }
    assert!(text.starts_with("status: draft\n"));
    // Shape's real drafting path must hand intake the upstream section names, not just prose.
    let contract = include_str!("../seed_me/seed-contract.txt");
    for section in [
        "Problem statement",
        "Proposed outcome",
        "Acceptance criteria",
        "Affected users and systems",
        "Constraints and boundaries",
        "Knowledge map",
        "Suggested first slice",
        "Risks and verification",
        "Open questions and deferrals",
        "Owners and next steps",
        "Downstream handoff",
        "Evidence and uncertainty",
        "Decisions (from the ledger)",
        "Assumed (not confirmed by you)",
    ] {
        let declaration = if section == "Knowledge map" {
            "- **Knowledge map**, in plain words, four parts:".to_owned()
        } else if matches!(
            section,
            "Decisions (from the ledger)" | "Assumed (not confirmed by you)"
        ) {
            format!("## {section}")
        } else {
            format!("- {section}:")
        };
        assert!(
            contract.lines().any(|line| line.starts_with(&declaration)),
            "upstream contract changed: {section}"
        );
        assert!(
            text.lines().any(|line| line == format!("## {section}")),
            "Shape seed lacks canonical section {section}: {text}"
        );
    }
    let consumer = include_str!("../../tests/fixtures/tink-sdlc-intake-sections.txt");
    assert!(
        missing_intake_sections(&text, consumer).is_empty(),
        "consumer contract drift: {:?}",
        missing_intake_sections(&text, consumer)
    );
    let renamed_consumer = consumer.replace("Acceptance criteria\n", "Intake checks\n");
    assert_eq!(
        missing_intake_sections(&text, &renamed_consumer),
        ["Intake checks"]
    );
    assert_eq!(
        missing_intake_sections(&text, &format!("{consumer}Rollback plan\n")),
        ["Rollback plan"]
    );

    // Exact parent-writer parser: rollback must retain the next unconfirmed revision identity.
    let parent_revision = |seed: &str| {
        seed.lines()
            .find_map(|line| line.strip_prefix("revision:"))
            .and_then(|revision| revision.trim().strip_prefix('r'))
            .and_then(|number| number.parse::<u64>().ok())
    };
    assert_eq!(
        parent_revision(&text),
        Some(8),
        "older writer would reset to r1: {text}"
    );
    let date_after = local_today();
    let date = text
        .lines()
        .find_map(|line| line.strip_prefix("date: "))
        .unwrap();
    assert!(
        date == date_before || date == date_after,
        "draft date {date} is not the local date ({date_before}/{date_after})"
    );
    assert!(
        text.contains("Grounded in:\n- TEST-read.md\n"),
        "missing recorded read inventory: {text}"
    );
    assert!(
        text.contains("Provenance unavailable:"),
        "must not invent verified origins: {text}"
    );

    let records: serde_json::Value =
        serde_json::from_slice(&fs::read(receipt.session.join("seed-checks.json")).unwrap())
            .unwrap();
    assert_eq!(
        records,
        serde_json::from_str::<serde_json::Value>(checks).unwrap()
    );
    let decisions = crate::seed_me::ledger::decisions_block(
        &crate::seed_me::ledger::load(&receipt.session).unwrap(),
    )
    .unwrap();
    assert!(text.contains(&decisions));
    assert!(!temp.path().join("TEST-CHECK-WAS-RUN").exists());
    let seed_path = receipt.session.join("seed-contract.md");
    let before = fs::read(&seed_path).unwrap();
    let mut missing = confirming.clone();
    missing.as_object_mut().unwrap().remove("Proposed outcome");
    let mut foreign = confirming.clone();
    foreign["Checks"] = "forged".into();
    let mut malformed_replies = vec![missing, foreign];
    for injected in [
        "## Decisions (from the ledger)\nforged",
        "  ## Problem statement\nduplicated",
        "status: confirmed for intake",
        "date: 1900-01-01",
    ] {
        let mut malformed = confirming.clone();
        malformed["Problem statement"] = injected.into();
        malformed_replies.push(malformed);
    }
    for malformed in malformed_replies {
        assert!(
            answering::draft_seed(
                &receipt.session,
                version,
                &malformed,
                &serde_json::from_str(include_str!("../../tests/fixtures/shape-checks.json"))
                    .unwrap(),
                &[],
                temp.path(),
                None,
            )
            .is_err()
        );
        assert_eq!(
            fs::read(&seed_path).unwrap(),
            before,
            "refusal changed seed bytes"
        );
    }
    // A redraft retains the facts already recorded; old writers still verify it.
    let version = answering::read(&receipt.session).unwrap().version();
    assert_eq!(
        answering::draft_seed(
            &receipt.session,
            version,
            &confirming,
            &serde_json::from_str(include_str!("../../tests/fixtures/shape-checks.json")).unwrap(),
            &[],
            temp.path(),
            None,
        )
        .unwrap(),
        "r9"
    );
    let redrafted = fs::read_to_string(&seed_path).unwrap();
    assert_eq!(parent_revision(&redrafted), Some(9));
    assert!(redrafted.contains("Grounded in:\n- TEST-read.md\n"));
    assert!(redrafted.contains("observation; scope: TEST-read.md:1"));
    assert!(redrafted.contains("Provenance unavailable:"));
    assert_eq!(
        answering::read(&receipt.session).unwrap().goal(),
        Some(goal)
    );
    answering::confirm_seed(
        &receipt.session,
        crate::seed_me::answering::read(&receipt.session)
            .unwrap()
            .version(),
        "r9",
        &std::fs::read_to_string(receipt.session.join("seed-contract.md")).unwrap(),
        "TEST yes to this seed only",
        None,
    )
    .unwrap();
    assert!(matches!(
        shaping::read(&receipt.session).unwrap().stage,
        Shaped::Ready { .. }
    ));
    assert!(!temp.path().join("TEST-CHECK-WAS-RUN").exists());
}

#[cfg(unix)]
#[test]
fn repository_checks_are_proposed_with_sources_but_never_executed() {
    use crate::{
        pi::CallBudget,
        seed_me::{Affirmed, Config, answering, shaping},
    };
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        time::{Duration, Instant},
    };

    let temp = tempfile::Builder::new()
        .prefix("tinkery-TEST-198-")
        .tempdir()
        .unwrap();
    let goal = "TEST I can check a seed.\nWhy: Checks need evidence.\nDone when: The parsed value is new.\n- The future checker validates the result.\n- The result feels calm.\n- The missing boundary assertion detects the defect.\nMust: Keep my words. The proposed ./TEST-future-check does not exist yet.\nMust not: Run checks or build without approval.";
    fs::create_dir(temp.path().join("tests")).unwrap();
    fs::write(
        temp.path().join("README.md"),
        "cmd: python3 tests/check_value.py\nexpect: new value\ncwd: .\n",
    )
    .unwrap();
    fs::write(temp.path().join("value.txt"), "old\n").unwrap();
    fs::write(
        temp.path().join("tests/runner_only.py"),
        "print('TEST runner without the required boundary check')\n",
    )
    .unwrap();
    fs::write(temp.path().join("tests/check_value.py"),
        "from pathlib import Path\nPath('TEST-CHECK-WAS-RUN').touch()\nassert Path('value.txt').read_text().strip() == 'new'\nprint('new value')\n").unwrap();
    let receipt = Config::new(Some(temp.path().join("TEST-sessions")))
        .confirm(Affirmed {
            goal: goal.into(),
            outcome: "TEST checks".into(),
            options: vec![],
            originals: vec![],
            context: "{}".into(),
            source: "TEST fixture, not real approval".into(),
        })
        .unwrap();
    let program = temp.path().join("TEST-pi");
    let template = prose_reply(include_str!("../../tests/fixtures/shape-seed-reply.txt"));
    let template = serde_json::to_string(&template).unwrap();
    fs::write(&program, format!(r##"#!/usr/bin/env python3
import json, pathlib, sys
request = json.load(sys.stdin)
assert request['goal'] == {goal:?}
assert 'python3 tests/check_value.py' not in json.dumps(request)
assert sys.argv[sys.argv.index('--tools') + 1] == 'read,grep,find,ls'
prompt = sys.argv[sys.argv.index('--system-prompt') + 1]
source = pathlib.Path('README.md').read_text().splitlines()
command, expected, cwd = [line.split(': ', 1)[1] for line in source]
assert pathlib.Path('tests/check_value.py').is_file()
assert not pathlib.Path('TEST-future-check').exists()
assert 'assert ' not in pathlib.Path('tests/runner_only.py').read_text()
# A policy-aware stub, not a claim about real-model reasoning or check quality.
can_propose = all(rule in prompt for rule in [
    'For a code-checkable criterion, propose', "cite the command's source", 'fail before the change'
]) and 'file inspection alone is not a settled' not in prompt
checks = [
    dict(statement='The parsed value is new.', kind='cmd' if can_propose else 'judge', source='README.md:1-3; proposed from repository inspection, not run.'),
    dict(statement='The future checker validates the result.', kind='provisional', source='TEST goal',
         missing='Once the named TEST-future-check exists.', cmd='./TEST-future-check', expect='result validated', cwd='.'),
    dict(statement='The result feels calm.', kind='judge', source='TEST goal', how_to_judge='The operator inspects calmness.'),
    dict(statement='The missing boundary assertion detects the defect.', kind='provisional', source='tests/runner_only.py:1',
         missing='The boundary assertion must be built; an existing runner is not that assertion.',
         expect='boundary checked after its assertion exists', cwd='.')]
if can_propose:
    checks[0].update(cmd=command, expect=expected, cwd=cwd)
    if 'not provisional merely because setup' not in prompt:
        checks[0].update(kind='provisional', missing='Build binary and prepare reversion probe first.')
else:
    checks[0]['how_to_judge'] = 'No invocation was settled.'
if 'Use provisional when the needed test/assertion or invocation is missing' not in prompt:
    checks[3].update(kind='cmd', cmd='python3 tests/runner_only.py')
    del checks[3]['missing']
body = json.loads({template:?})
body['Suggested first slice'] = 'Proposal only: add the missing behavior, then use the check above.'
body = {{key: value.replace('no repository facts were recorded.', 'reported reads are listed below.') for key, value in body.items()}}
print(json.dumps({{'next': 'seed', 'confirming': body, 'checks': checks, 'review': {{'contradictions': [], 'decided': [], 'done': [], 'failure_cases': ['TEST wrong result must fail' for _ in checks]}}, 'found': [
    {{'id': 'TEST-read-fact', 'claim': {{'type': 'observation', 'scope': 'README.md:1-3'}}, 'checked': 'README.md:1-3', 'observed': 'documents the focused behavior check'}},
    {{'id': 'TEST-check-fact', 'claim': {{'type': 'observation', 'scope': 'tests/check_value.py:1-4'}}, 'checked': 'tests/check_value.py:1-4', 'observed': 'asserts the new value; current value.txt is old'}}]}}))
"##)).unwrap();
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
    let agent = Arc::new(
        step::Agent::new(
            crate::pi::Harness::Pi,
            program,
            "test/stub".into(),
            String::new(),
            Some(temp.path().into()),
            CallBudget::new(1),
            None,
        )
        .unwrap(),
    );
    let version = answered_probe(&receipt.session);
    let running = step::Running::start(
        step::Work::Look {
            feedback: None,
            expected: version,
            folder: temp.path().into(),
        },
        receipt.session.clone(),
        agent.clone(),
    );
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(result) = running.poll() {
            assert_eq!(result, Ok(step::Done::Drafted));
            break;
        }
        assert!(Instant::now() < deadline, "stub drafting timed out");
        std::thread::sleep(Duration::from_millis(10));
    }
    let seed = fs::read_to_string(receipt.session.join("seed-contract.md")).unwrap();
    let checks: serde_json::Value =
        serde_json::from_slice(&fs::read(receipt.session.join("seed-checks.json")).unwrap())
            .unwrap();
    assert_eq!(
        checks[0]["kind"], "cmd",
        "repository evidence was replaced by judgment or routine setup: {checks}"
    );
    assert_eq!(checks[0]["cmd"], "python3 tests/check_value.py");
    assert_eq!(
        checks[0]["source"],
        "README.md:1-3; proposed from repository inspection, not run."
    );
    assert_eq!(checks[1]["kind"], "provisional");
    assert_eq!(checks[1]["cmd"], "./TEST-future-check");
    assert_eq!(checks[2]["kind"], "judge");
    assert_eq!(
        checks[3]["kind"], "provisional",
        "a runner without the assertion is not a check"
    );
    assert!(checks[3].get("cmd").is_none());
    assert!(seed.starts_with("status: draft\n"));
    assert!(!receipt.session.join("seed-confirmation.json").exists());
    assert_eq!(
        answering::read(&receipt.session).unwrap().goal(),
        Some(goal)
    );
    assert!(!temp.path().join("TEST-CHECK-WAS-RUN").exists());
    assert!(!temp.path().join("TEST-future-check").exists());
    answering::confirm_seed(
        &receipt.session,
        crate::seed_me::answering::read(&receipt.session)
            .unwrap()
            .version(),
        "r1",
        &std::fs::read_to_string(receipt.session.join("seed-contract.md")).unwrap(),
        "TEST yes to proposed checks, not implementation",
        None,
    )
    .unwrap_err();
    assert!(matches!(
        shaping::read(&receipt.session).unwrap().stage,
        Shaped::Drafted { .. }
    ));
    assert_eq!(agent.remaining(), 0);
    assert!(!temp.path().join("TEST-CHECK-WAS-RUN").exists());
}

#[test]
fn prompt_recovery_separates_existing_checks_from_missing_assertions() {
    let prompt = step::prompt();
    for rule in [
        "Keep a known repository invocation with existing assertions for the criterion",
        "not provisional merely because setup",
        "state those prerequisites and intended red separately",
        "Use provisional when the needed test/assertion or invocation is missing",
        "never invent a runner or substitute an existing generic suite for a missing regression",
        "under a targeted reversion of an already-present fix",
    ] {
        assert!(
            prompt.contains(rule),
            "missing check-availability distinction: {rule}"
        );
    }
    assert!(!prompt.contains("When no repository command exists or something must be built first"));
}

#[test]
fn prompt_recovery_does_not_turn_existing_behavior_into_a_new_goal_choice() {
    let prompt = step::prompt();
    assert!(prompt.contains("If the repo already appears to meet the confirmed goal, propose verification and repairs only for demonstrated gaps"));
    assert!(prompt.contains("do not ask whether to verify it or require extra changes"));
    assert!(prompt.contains("do not claim it is complete without evidence"));
}

#[cfg(unix)]
#[test]
fn prompt_recovery_existing_goal_drafts_without_reconfirmation() {
    assert_settled_intake_drafts(None);
}

#[test]
fn settled_intake_drafts_without_reopening_settled_or_deferred_decisions() {
    let prompt = step::prompt();
    for rule in [
        "settled or explicitly deferred decisions do not block it",
        "Never reopen what the goal",
        "a settled user or delegated ledger answer settles",
        "record deferred choices as operator prerequisites in Open questions and deferrals",
        "choose no alternative",
        "When the goal and recorded answers settle what this draft must say, supply confirming",
        "genuinely unresolved ledger decisions still block drafting",
    ] {
        assert!(prompt.contains(rule), "missing settled-intake rule: {rule}");
    }
    assert!(!prompt.contains("If a decision only the operator can make is still open, ask"));
}

#[cfg(unix)]
#[test]
fn settled_intake_answer_drafts_without_reopening() {
    assert_settled_intake_drafts(Some(
        "TEST Keep storage local; that choice is settled. Do not ask it again.",
    ));
}

#[cfg(unix)]
#[test]
fn settled_intake_deferral_drafts_without_choosing_or_confirming() {
    assert_settled_intake_drafts(Some(
        "TEST Draft only. Defer local versus synced until before implementation; keep it as an operator prerequisite. Do not choose either or approve implementation.",
    ));
}

#[cfg(unix)]
fn assert_settled_intake_drafts(words: Option<&str>) {
    use crate::{
        pi::CallBudget,
        seed_me::{Affirmed, Config, answering},
    };
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        time::{Duration, Instant},
    };

    let template = prose_reply(include_str!("../../tests/fixtures/shape-seed-reply.txt"));
    let template_json = serde_json::to_string(&template).unwrap();
    let temp = tempfile::Builder::new()
        .prefix("tinkery-TEST-settled-")
        .tempdir()
        .unwrap();
    let receipt = Config::new(Some(temp.path().join("TEST-sessions")))
        .confirm(Affirmed {
            goal: GOAL.into(),
            outcome: "TEST draft".into(),
            options: vec![],
            originals: vec![],
            context: "{}".into(),
            source: "TEST fixture, not real approval".into(),
        })
        .unwrap();
    let version = answered_probe(&receipt.session);
    let version = if words.is_some() {
        let version = answering::ask(
            &receipt.session,
            version,
            &answering::Ask {
                prerequisites: None,
                supported_by: None,
                id: "storage".into(),
                label: "TEST Storage".into(),
                question: "TEST Local or synced?".into(),
                gate: "TEST Storage disposition".into(),
                found: vec![],
                resolves: None,
                suggestion: None,
                assumed: vec![],
                options: vec![],
                against: None,
                confidence: None,
            },
            temp.path(),
        )
        .unwrap();
        // A genuinely open decision still refuses a draft before the answer is saved.
        assert!(
            answering::draft_seed(
                &receipt.session,
                version,
                &template,
                &serde_json::from_str(include_str!("../../tests/fixtures/shape-checks.json"))
                    .unwrap(),
                &[],
                temp.path(),
                None,
            )
            .is_err()
        );
        version
    } else {
        fs::write(
            temp.path().join("TEST-existing.txt"),
            "TEST Requested behavior already present.\n",
        )
        .unwrap();
        version
    };
    assert!(!receipt.session.join("seed-contract.md").exists());
    let words_json = serde_json::to_string(&words).unwrap();
    let checks = r#"[{"statement":"I see the next question.","kind":"judge","source":"TEST confirmed goal"}]"#;
    let program = temp.path().join("TEST-pi");
    fs::write(&program, format!(r##"#!/usr/bin/env python3
import json, pathlib, sys
request = json.load(sys.stdin)
words = json.loads({words_json:?})
assert request['goal'] == {GOAL:?}
assert request['ledger']['current_question'] is None
assert all(n['status'] != 'unresolved' for n in request['ledger']['nodes'])
if words is not None:
    answer = next(n for n in request['ledger']['nodes'] if n['id'] == 'storage')
    assert answer['status'] == 'settled' and answer['authority'] == 'user'
    assert answer['answer'] == words
else:
    # The goal, and the pre-mortem the operator already answered.
    assert [n['id'] for n in request['ledger']['nodes'] if not n['id'].startswith('probe-')] == ['goal']
    assert pathlib.Path('TEST-existing.txt').read_text() == 'TEST Requested behavior already present.\n'
assert sys.argv[sys.argv.index('--tools') + 1] == 'read,grep,find,ls'
prompt = sys.argv[sys.argv.index('--system-prompt') + 1]
# This stub proves routing instructions and helper transport, not model reasoning.
drafts = all(rule in prompt for rule in [
    'settled or explicitly deferred decisions do not block it',
    'Never reopen what the goal',
    'When the goal and recorded answers settle what this draft must say, supply confirming',
    'record deferred choices as operator prerequisites in Open questions and deferrals',
    'choose no alternative',
    'genuinely unresolved ledger decisions still block drafting',
])
if words is None:
    drafts = drafts and 'do not ask whether to verify it or require extra changes' in prompt
if not drafts:
    print(json.dumps({{'next': 'question', 'id': 'reopened', 'question': 'TEST Verify existing behavior or require extra changes?', 'found': []}}))
else:
    body = json.loads({template_json:?})
    recorded = words if words is not None else 'Existing behavior remains unverified; propose checks, not extra requirements.'
    body['Open questions and deferrals'] = 'Recorded disposition: ' + recorded
    print(json.dumps({{'next': 'seed', 'confirming': body, 'checks': json.loads({checks:?}), 'review': {{'contradictions': [], 'decided': [], 'done': [], 'failure_cases': ['TEST wrong result must fail' for _ in json.loads({checks:?})]}}, 'found': []}}))
"##)).unwrap();
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
    let agent = Arc::new(
        step::Agent::new(
            crate::pi::Harness::Pi,
            program,
            "test/stub".into(),
            String::new(),
            Some(temp.path().into()),
            CallBudget::new(1),
            None,
        )
        .unwrap(),
    );
    let work = match words {
        Some(words) => step::Work::Answer {
            id: "storage".into(),
            words: words.into(),
            expected: version,
            folder: temp.path().into(),
        },
        None => step::Work::Look {
            feedback: None,
            expected: version,
            folder: temp.path().into(),
        },
    };
    let running = step::Running::start(work, receipt.session.clone(), agent.clone());
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(result) = running.poll() {
            assert_eq!(
                result,
                Ok(step::Done::Drafted),
                "settled intake reopened: {words:?}"
            );
            break;
        }
        assert!(Instant::now() < deadline, "stub drafting timed out");
        std::thread::sleep(Duration::from_millis(10));
    }
    let ledger = answering::read(&receipt.session).unwrap();
    assert_eq!(ledger.goal(), Some(GOAL));
    assert!(ledger.open().is_empty());
    let context: serde_json::Value = serde_json::from_str(&ledger.context()).unwrap();
    if let Some(words) = words {
        let answer = context["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["id"] == "storage")
            .unwrap();
        assert_eq!(answer["answer"], words);
    } else {
        // Drafting added nothing beside the goal and the answered pre-mortem.
        let ids: Vec<&str> = context["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|n| n["id"].as_str())
            .collect();
        assert_eq!(ids, ["goal", answering::SAMPLE.0, answering::PROBE.0]);
    }
    let seed = fs::read_to_string(receipt.session.join("seed-contract.md")).unwrap();
    assert!(seed.starts_with("status: draft\n"));
    assert!(
        seed.split("## Open questions and deferrals\n")
            .nth(1)
            .unwrap()
            .split("\n## ")
            .next()
            .unwrap()
            .contains(words.unwrap_or("Existing behavior remains unverified"))
    );
    assert!(!receipt.session.join("seed-confirmation.json").exists());
    assert_eq!(agent.remaining(), 0);
}

/// Scripted drafting proves sent instructions and name transport, not model compliance.
#[cfg(unix)]
#[test]
fn supplied_tally_name_reaches_drafting_and_survives_the_stub_seed() {
    use crate::{
        pi::CallBudget,
        seed_me::{Affirmed, Config},
    };
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        time::{Duration, Instant},
    };
    const WORDS: &str = "Build a minimal local tally CLI for developers in this README-only scratch repo: count whitespace-separated words in one UTF-8 text file and print the count. The first version is done when readable and empty files count correctly and unreadable files have clear errors; no stdin, multiple files, network access, or extra counting modes.";
    const GOAL: &str = "Count whitespace-separated words in one UTF-8 text file and print the count, for developers, in this README-only scratch repo.\nMust: Provide a minimal local tally CLI.";
    let temp = tempfile::Builder::new()
        .prefix("tinkery-TEST-supplied-name-")
        .tempdir()
        .unwrap();
    let receipt = Config::new(Some(temp.path().join("TEST-sessions")))
        .confirm(Affirmed {
            goal: GOAL.into(),
            outcome: "TEST word count".into(),
            options: vec![],
            originals: vec![crate::seed_me::Original {
                id: 1,
                in_reply_to: None,
                text: WORDS.into(),
            }],
            context: "{}".into(),
            source: "TEST fixture, not real approval".into(),
        })
        .unwrap();
    let version = answered_probe(&receipt.session);
    let template = prose_reply(include_str!("../../tests/fixtures/shape-seed-reply.txt"));
    let template_json = serde_json::to_string(&template).unwrap();
    let checks = include_str!("../../tests/fixtures/shape-checks.json");
    let program = temp.path().join("TEST-pi");
    fs::write(&program, format!(r#"#!/usr/bin/env python3
import json, sys
request = json.load(sys.stdin)
assert request['goal'] == {GOAL:?}
assert {WORDS:?} in json.dumps(request['ledger'])
prompt = sys.argv[sys.argv.index('--system-prompt') + 1]
assert 'Keep names supplied in the confirmed goal or Think answers in the seed, including its summary and knowledge map' in prompt
assert 'never list a supplied name as unknown' in prompt
assert 'leave those unknown unless established separately' in prompt
# This is a chosen faithful reply, not a semantic correction or a live evaluation.
body = json.loads({template_json:?})
body['You are confirming'] = {GOAL:?}.split('\n')[0] + '\nWhat gets built: a minimal local tally CLI.\nWhat does not: extra counting modes.\nAccepted unchanged: supplied name tally; no new choices.\nUnknowns: executable invocation remains unknown.'
body['Knowledge map'] = 'The confirmed goal supplies the tool name tally. Its executable invocation remains unknown.'
print(json.dumps({{'next': 'seed', 'confirming': body, 'checks': json.loads({checks:?}), 'review': {{'contradictions': [], 'decided': [], 'done': [], 'failure_cases': ['TEST wrong result must fail' for _ in json.loads({checks:?})]}}, 'found': []}}))
"#)).unwrap();
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
    let agent = Arc::new(
        step::Agent::new(
            crate::pi::Harness::Pi,
            program,
            "test/stub".into(),
            String::new(),
            Some(temp.path().into()),
            CallBudget::new(1),
            None,
        )
        .unwrap(),
    );
    let running = step::Running::start(
        step::Work::Look {
            feedback: None,
            expected: version,
            folder: temp.path().into(),
        },
        receipt.session.clone(),
        agent.clone(),
    );
    let until = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(result) = running.poll() {
            assert_eq!(result, Ok(step::Done::Drafted));
            break;
        }
        assert!(Instant::now() < until, "stub drafting timed out");
        std::thread::sleep(Duration::from_millis(10));
    }
    let seed = fs::read_to_string(receipt.session.join("seed-contract.md")).unwrap();
    assert!(seed.contains("What gets built: a minimal local tally CLI."));
    assert!(seed.contains("The confirmed goal supplies the tool name tally."));
    assert!(seed.contains("executable invocation remains unknown"));
    assert!(!seed.contains("CLI name"));
    let ledger = crate::seed_me::answering::read(&receipt.session).unwrap();
    assert_eq!(ledger.goal(), Some(GOAL));
    assert!(ledger.context().contains(WORDS));
    assert_eq!(agent.remaining(), 0);
    assert!(!receipt.session.join("seed-confirmation.json").exists());
}

#[test]
fn the_prompt_names_exactly_the_fields_the_decoder_reads() {
    let prompt = step::prompt();
    let mut named = std::collections::BTreeSet::new();
    for piece in prompt.split('"').skip(1).step_by(2) {
        if prompt.contains(&format!("\"{piece}\":")) {
            named.insert(piece.to_owned());
        }
    }
    let decoded: std::collections::BTreeSet<_> =
        step::FIELDS.iter().map(|f| f.to_string()).collect();
    assert_eq!(named, decoded);
    for note in include_str!("prompt.txt")
        .lines()
        .filter_map(|line| line.strip_prefix("# "))
    {
        assert!(
            !prompt.contains(note),
            "prompt note reached the model: {note}"
        );
    }
}

/// `shape` with its next step's folder already found.
fn reading(mut shape: Shape, folder: &Path) -> Shape {
    shape.open.as_mut().unwrap().folder = Folder::Known(folder.to_path_buf());
    shape
}

#[test]
fn the_folder_a_step_reads_shows_before_f2_and_f1_picks_another() {
    let dir = tempfile::tempdir().unwrap();
    let (first, other) = (dir.path().join("TEST-first"), dir.path().join("TEST-other"));
    std::fs::create_dir_all(&first).unwrap();
    std::fs::create_dir_all(&other).unwrap();
    for stage in [question(), Shaped::Waiting] {
        let mut shape = reading(done(stage).with_agent(Some(agent(1))), &first);
        let text = screen(&mut shape, 80, 24, true);
        assert!(
            text.contains("The step reads files in TEST-first. F1 picks another folder."),
            "{text}"
        );
        assert!(shape.help_text().contains(&format!(
            "The next step reads files in: {}",
            first.display()
        )));
        choose(&mut shape, "Choose the folder");
        assert!(shape.takes_text());
        let text = screen(&mut shape, 80, 24, true);
        // A long folder path wraps, so compare without the wrapping whitespace.
        let squashed: String = text.split_whitespace().collect();
        assert!(
            squashed.contains(&format!("Folder:{}", first.display())),
            "{text}"
        );
        assert!(
            text.contains("Next: F2 uses this folder; Esc keeps TEST-first."),
            "{text}"
        );
        assert_eq!(text.matches("Next:").count(), 1, "{text}");
        assert_eq!(
            text.lines().last().unwrap().trim(),
            "enter newline   F2 use folder   esc cancel"
        );
        // Esc keeps the folder shown; the typed answer is untouched.
        shape.key(key(KeyCode::Char('x')));
        shape.key(key(KeyCode::Esc));
        assert!(shape.open.is_some(), "Esc while choosing left Shape");
        assert!(screen(&mut shape, 80, 24, true).contains("reads files in TEST-first."));
        assert!(shape.open.as_ref().unwrap().typed.is_empty());
        // A folder that isn't there is refused and the shown one stays.
        choose(&mut shape, "Choose the folder");
        shape.paste("-missing");
        shape.key(key(KeyCode::F(2)));
        let text = screen(&mut shape, 100, 30, true);
        let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(
            flat.contains("so the step still reads TEST-first."),
            "{text}"
        );
        assert!(text.contains("reads files in TEST-first."), "{text}");
        // An existing folder is used for the next step.
        choose(&mut shape, "Choose the folder");
        for _ in 0..first.display().to_string().chars().count() + 20 {
            shape.key(key(KeyCode::Backspace));
        }
        shape.paste(&other.display().to_string());
        shape.key(key(KeyCode::F(2)));
        let text = screen(&mut shape, 100, 30, true);
        assert!(
            text.contains("The step reads files in TEST-other."),
            "{text}"
        );
        assert!(matches!(
            &shape.open.as_ref().unwrap().folder,
            Folder::Known(f) if *f == other.canonicalize().unwrap()
        ));
    }
}

#[test]
fn no_folder_is_offered_without_an_agent_or_a_step_and_no_step_starts_before_it_is_known() {
    let dir = tempfile::tempdir().unwrap();
    let read_only = reading(done(question()), dir.path());
    assert!(!offers(&read_only, "Choose the folder"));
    let seed = Shaped::Drafted {
        revision: Some("r1".into()),
        text: Some("status: draft\n".into()),
    };
    let drafted = reading(done(seed).with_agent(Some(agent(1))), dir.path());
    assert!(!offers(&drafted, "Choose the folder"));
    let mut finding = done(Shaped::Waiting).with_agent(Some(agent(1)));
    assert!(screen(&mut finding, 100, 30, true).contains("Finding which folder the step reads."));
    finding.key(key(KeyCode::F(2)));
    assert!(finding.open.as_ref().unwrap().running.is_none());
    assert!(screen(&mut finding, 100, 30, true).contains("still finding which folder"));
}

#[test]
fn a_question_or_seed_with_nothing_found_is_flagged() {
    let mut shape = done(question());
    let text = screen(&mut shape, 100, 30, true);
    assert!(
        text.contains("Nothing found: this question records nothing checked in the repository."),
        "{text}"
    );
    let mut grounded = done(Shaped::Question {
        gate: "TEST gate".into(),
        id: "storage".into(),
        title: "Where records live".into(),
        ask: "Keep records local or synced?".into(),
        found: vec!["README.md:3 says local only".into()],
        suggestion: None,
        options: vec![],
        against: None,
        hard: false,
    });
    assert!(!screen(&mut grounded, 100, 30, true).contains("Nothing found"));
    let seed = format!(
        "status: draft\n# Seed: TEST\nrevision: r1\n\n## You are confirming\nTEST.\n\n## What's true today\n{}\n",
        crate::seed_me::answering::NOTHING_FOUND
    );
    let mut drafted = done(Shaped::Drafted {
        revision: Some("r1".into()),
        text: Some(seed),
    });
    let text = screen(&mut drafted, 80, 24, true);
    assert!(
        text.contains("Nothing found: this seed records nothing checked in the repository."),
        "{text}"
    );
}

#[test]
fn so_far_scrolls_on_a_short_screen_and_always_says_how_to_close() {
    let mut shape = done(Shaped::Waiting);
    choose(&mut shape, "So far");
    let text = screen(&mut shape, 80, 14, true);
    assert!(text.contains("Your goal, as you confirmed it"), "{text}");
    assert!(text.contains("↑↓ scroll · esc close"), "{text}");
    for _ in 0..20 {
        shape.key(key(KeyCode::Down));
    }
    let text = screen(&mut shape, 80, 14, true);
    assert!(text.contains("TEST format: Markdown"), "{text}");
    assert!(text.contains("↑↓ scroll · esc close"), "{text}");
    shape.key(key(KeyCode::Esc));
    assert!(
        shape.open.as_ref().unwrap().so_far.is_none(),
        "Esc didn't close So far"
    );
}

#[test]
fn a_long_answer_never_hides_the_question_or_what_a_running_step_is_doing() {
    let dir = tempfile::tempdir().unwrap();
    let mut shape = reading(done(question()).with_agent(Some(agent(1))), dir.path());
    let answer = (1..=20)
        .map(|i| format!("TEST line {i}"))
        .collect::<Vec<_>>()
        .join("\n");
    shape.paste(&answer);
    let text = screen(&mut shape, 80, 24, true);
    assert!(text.contains("Keep records local or synced?"), "{text}");
    assert!(text.contains("TEST line 20"), "{text}");
    shape.key(key(KeyCode::F(2)));
    assert!(
        shape.open.as_ref().unwrap().running.is_some(),
        "no step started"
    );
    let text = screen(&mut shape, 80, 24, true);
    assert!(text.contains("Keep records local or synced?"), "{text}");
    assert!(text.contains("Saving your answer."), "{text}");
    assert_eq!(text.lines().last().unwrap().trim(), "esc stop");
}

#[test]
fn a_pasted_cr_breaks_the_row_on_screen_and_the_words_stay_as_pasted() {
    let dir = tempfile::tempdir().unwrap();
    let mut shape = reading(done(question()).with_agent(Some(agent(1))), dir.path());
    shape.paste("TEST one\rTEST two\r\nTEST three");
    let text = screen(&mut shape, 80, 24, true);
    assert!(
        text.contains("  TEST one\n  TEST two\n  TEST three"),
        "{text}"
    );
    assert_eq!(
        shape.open.as_ref().unwrap().typed.text(),
        "TEST one\rTEST two\r\nTEST three"
    );
}

#[test]
fn enter_at_seed_confirmation_guides_without_editing_or_starting_a_step() {
    for words in ["", "TEST I confirm r1.", "TEST pasted\r\nwords"] {
        let mut shape = done(Shaped::Drafted {
            revision: Some("r1".into()),
            text: Some("TEST seed".into()),
        })
        .with_agent(Some(agent(0)));
        shape.paste(words);
        shape.key(key(KeyCode::Enter));
        let open = shape.open.as_ref().unwrap();
        assert_eq!(open.typed.text(), words);
        assert!(open.running.is_none());
        assert_eq!(shape.agent.as_ref().unwrap().remaining(), 0);
        assert!(
            screen(&mut shape, 80, 24, true)
                .contains("F2 confirms the seed; Enter does not add a line.")
        );
    }
}

#[test]
fn only_the_word_confirm_confirms_and_other_words_are_kept_unconfirmed() {
    for words in [
        "TEST yes\nrevision r1",
        "TEST yes\rrevision r1",
        "confirm\r\n",
        "\n",
        "",
        " confirm",
        "Confirm",
        "I confirm",
        "keep the cache local, not shared",
    ] {
        let mut shape = done(Shaped::Drafted {
            revision: Some("r1".into()),
            text: Some("TEST seed".into()),
        })
        .with_agent(Some(agent(0)));
        screen(&mut shape, 80, 24, true);
        shape.paste(words);
        shape.key(key(KeyCode::F(2)));
        let open = shape.open.as_ref().unwrap();
        assert!(open.running.is_none(), "{words:?} started a confirmation");
        assert_eq!(open.typed.text(), words);
        assert_eq!(shape.agent.as_ref().unwrap().remaining(), 0);
        let shown = screen(&mut shape, 80, 24, true);
        assert!(shown.contains(WORD_REFUSALS[1]), "{shown}");
    }
    let mut shape = done(Shaped::Drafted {
        revision: Some("r1".into()),
        text: Some("TEST seed".into()),
    })
    .with_agent(Some(agent(0)));
    screen(&mut shape, 80, 24, true);
    shape.paste("confirm");
    shape.key(key(KeyCode::F(2)));
    assert!(shape.open.as_ref().unwrap().running.is_some());
}

#[test]
fn folder_edits_keep_multiline_paths_including_trailing_whitespace() {
    let dir = tempfile::tempdir().unwrap();
    for name in ["TEST folder\nnext", "TEST CR\rnext", "TEST CRLF\r\nnext\n "] {
        let folder = dir.path().join(name);
        std::fs::create_dir(&folder).unwrap();
        let mut shape = reading(done(question()).with_agent(Some(agent(0))), dir.path());
        choose(&mut shape, "Choose the folder");
        shape.key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL));
        shape.key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL));
        shape.paste(folder.to_str().unwrap());
        shape.key(key(KeyCode::Enter));
        assert!(
            shape.open.as_ref().unwrap().choosing.is_some(),
            "Enter applied the folder"
        );
        shape.key(key(KeyCode::Backspace));
        shape.key(key(KeyCode::F(2)));
        let open = shape.open.as_ref().unwrap();
        assert!(matches!(&open.folder, Folder::Known(p) if *p == folder.canonicalize().unwrap()));
        assert!(open.notice.is_none(), "the exact folder was refused");
        assert!(open.running.is_none());
    }
}

#[test]
fn a_pasted_folder_with_a_trailing_line_break_is_used() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("TEST-pasted");
    std::fs::create_dir(&folder).unwrap();
    let mut shape = reading(done(question()).with_agent(Some(agent(0))), dir.path());
    choose(&mut shape, "Choose the folder");
    shape.key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL));
    shape.key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL));
    shape.paste(&format!("  {}\r\n", folder.display()));
    shape.key(key(KeyCode::F(2)));
    let open = shape.open.as_ref().unwrap();
    assert!(open.notice.is_none(), "the pasted folder was refused");
    assert!(matches!(&open.folder, Folder::Known(p) if *p == folder.canonicalize().unwrap()));
}

#[test]
fn a_long_editor_follows_its_cursor_and_leaves_half_the_body_for_reading() {
    use ratatui::style::Modifier;
    for (width, height) in [(80, 24), (100, 30), (80, 14)] {
        for no_color in [false, true] {
            let mut stage = question();
            if let Shaped::Question { found, .. } = &mut stage {
                *found = vec![format!("TEST evidence: {}", "reading ".repeat(360))];
            }
            let mut shape = done(stage).with_agent(Some(agent(0)));
            shape.paste(
                &(0..40)
                    .map(|i| format!("TEST row {i}: 界 {}", "long ".repeat(20)))
                    .collect::<Vec<_>>()
                    .join("\n"),
            );
            // Resize down from a larger viewport, then return from the end to the beginning.
            screen(&mut shape, width + 20, height + 6, no_color);
            shape.key(KeyEvent::new(
                KeyCode::Up,
                KeyModifiers::CONTROL | KeyModifiers::ALT,
            ));
            shape.key(key(KeyCode::Home));
            let backend = ratatui::backend::TestBackend::new(width, height);
            let mut terminal = ratatui::Terminal::new(backend).unwrap();
            terminal
                .draw(|f| {
                    shape.render(
                        f,
                        Rect::new(2, 3, width - 4, height - 4),
                        Palette::new(no_color),
                    )
                })
                .unwrap();
            let buffer = terminal.backend().buffer();
            let cursor = (3..height - 1)
                .flat_map(|y| (2..width - 2).map(move |x| (x, y)))
                .find(|&(x, y)| buffer[(x, y)].modifier.contains(Modifier::REVERSED))
                .expect("the editor's cursor was hidden");
            assert_eq!(
                buffer[cursor].symbol(),
                "T",
                "cursor did not return to TEST row 0"
            );
            assert!(
                cursor.1 >= 3 + (height - 4) / 2,
                "the editor displaced more than half the reading"
            );
            assert_eq!(shape.scroll, 0, "arrows scrolled the reading");
            shape.key(key(KeyCode::PageDown));
            assert_eq!(
                shape.scroll,
                crate::page(shape.page),
                "PgDn did not scroll the reading a page"
            );
            let shown = screen(&mut shape, width, height, no_color);
            assert!(shown.contains("TEST row 0"), "{shown}");
            assert!(
                shown.lines().last().unwrap().contains("pgup/dn read"),
                "{shown}"
            );
            assert!(
                shown.lines().last().unwrap().contains("esc back"),
                "{shown}"
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn ending_completes_only_a_verified_seed_keeps_it_ready_and_leaves_it_read_only() {
    use crate::seed_me::{
        Affirmed, Config,
        answering::{self, Ending},
        shaping,
    };

    let temp = tempfile::Builder::new()
        .prefix("tinkery-TEST-231-")
        .tempdir()
        .unwrap();
    let save = || {
        let receipt = Config::new( Some(temp.path().join("TEST-sessions")))
            .confirm(Affirmed {
                goal: "TEST I can end a session.\nWhy: Tend needs it ended.\nDone when: It ends.\nMust: Keep my words.\nMust not: Build without approval.".into(),
                outcome: "TEST ending".into(),
                options: vec![],
                originals: vec![],
                context: "{}".into(),
                source: "TEST fixture, not real approval".into(),
            })
            .unwrap();
        receipt.session.clone()
    };
    let refused = |result: Result<(), answering::Refusal>| match result {
        Err(answering::Refusal::Refused(reason)) => reason,
        other => panic!("not refused: {other:?}"),
    };

    // Completing needs a verified seed; Tinkery refuses and the session stays open.
    let session = save();
    let reason = refused(answering::end(&session, Ending::Completed, None));
    assert!(reason.starts_with("Tinkery refused:"), "{reason}");
    assert!(matches!(
        shaping::read(&session).unwrap().stage,
        Shaped::Waiting
    ));
    let version = answering::read(&session).unwrap().version();
    let revision = answering::draft_seed(
        &session,
        version,
        &prose_reply(include_str!("../../tests/fixtures/shape-seed-reply.txt")),
        &serde_json::json!([{"statement":"It ends.","kind":"judge","source":"TEST confirmed goal"}]),
        &[],
        temp.path(),
        None,
    )
    .unwrap();
    answering::confirm_seed(
        &session,
        crate::seed_me::answering::read(&session).unwrap().version(),
        &revision,
        &std::fs::read_to_string(session.join("seed-contract.md")).unwrap(),
        "TEST yes to this seed only",
        None,
    )
    .unwrap();
    // Completion needs no viewer record.
    assert!(!session.join("viewer.json").exists());
    answering::end(&session, Ending::Completed, None).unwrap();
    let ended = shaping::read(&session).unwrap();
    assert_eq!(
        ended.stage,
        Shaped::Ready {
            revision: Some(revision),
            ended: true
        }
    );
    std::fs::create_dir_all(session.join("_system/scripts")).unwrap();
    std::fs::write(session.join(crate::sdlc::SCRIPT), "TEST script").unwrap();
    assert!(
        crate::seed_me::ready::read(&session, Some(&session))
            .unwrap()
            .is_some_and(|seed| seed.commands.is_some()),
        "Approve still offers a completed session's verified seed"
    );
    let crate::seed_me::Record::Session(read) = crate::seed_me::read(&session) else {
        panic!("not readable as a session")
    };
    assert_eq!(read.stage, Stage::Completed, "tend counts it ended");
    // Ended is for good: neither ending applies again.
    let reason = refused(answering::end(&session, Ending::Stopped, None));
    assert!(reason.contains("read-only"), "{reason}");

    // Stopping keeps what's open, and Shape then says plainly that it ended.
    let session = save();
    answering::end(&session, Ending::Stopped, None).unwrap();
    assert_eq!(
        shaping::read(&session).unwrap().stage,
        Shaped::Ended {
            status: "stopped".into()
        }
    );
}

#[test]
fn uncertain_endings_and_other_ledger_saves_never_claim_nothing_changed() {
    let reason = "This may or may not have saved (TEST read-back failed). Reopen this session before trying again.";
    for write in [
        Write::End(Ending::Completed),
        Write::Object("TEST default".into()),
        Write::Recommend {
            id: "TEST".into(),
            text: "TEST".into(),
        },
    ] {
        let mut shape = done(question());
        let (sender, receiver) = std::sync::mpsc::channel();
        shape.open.as_mut().unwrap().writing = Some((write, receiver));
        sender.send(Err(Refusal::Refused(reason.into()))).unwrap();
        shape.tick();
        assert_eq!(
            shape.open.as_ref().unwrap().notice.as_ref().unwrap().0,
            reason
        );
    }
}

#[test]
fn ending_asks_once_more_and_esc_or_a_held_key_ends_nothing() {
    let held = KeyEvent::new_with_kind(
        KeyCode::F(2),
        KeyModifiers::NONE,
        crossterm::event::KeyEventKind::Repeat,
    );
    for (stage, ending, other) in [
        (question(), "Stop this session", "End this session"),
        (
            Shaped::Ready {
                revision: Some("r2".into()),
                ended: false,
            },
            "End this session",
            "Stop this session",
        ),
    ] {
        let mut shape = done(stage);
        // Each stage offers only its own ending.
        assert!(!offers(&shape, other));
        choose(&mut shape, ending);
        assert!(shape.open.as_ref().unwrap().asking.is_some());
        let text = screen(&mut shape, 80, 24, true);
        assert!(text.contains("read-only for good"), "{text}");
        assert!(text.contains("esc keep it"), "{text}");
        // Esc keeps it open; nothing started.
        shape.key(key(KeyCode::Esc));
        let open = shape.open.as_ref().unwrap();
        assert!(open.asking.is_none() && open.writing.is_none());
        assert!(screen(&mut shape, 80, 24, true).contains("Nothing was ended."));
        // Asked, other keys and a held F2 do nothing; F2 ends it through the helper.
        choose(&mut shape, ending);
        assert!(!offers(&shape, ending), "F1 offered more while asking");
        shape.key(key(KeyCode::Char('x')));
        shape.key(key(KeyCode::Char('1')));
        shape.key(held);
        assert!(shape.open.as_ref().unwrap().writing.is_none());
        shape.key(key(KeyCode::F(2)));
        assert!(shape.open.as_ref().unwrap().writing.is_some());
        // This fixture's session path doesn't exist, so the ledger's refusal lands.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while shape.open.as_ref().unwrap().writing.is_some() {
            assert!(std::time::Instant::now() < deadline, "ending never landed");
            shape.tick();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let open = shape.open.as_ref().unwrap();
        let said = &open.notice.as_ref().unwrap().0;
        assert!(said.starts_with("Tinkery refused:"), "{said}");
    }
    // Ended, unknown and unverified sessions offer no ending.
    for stage in [
        Shaped::Ready {
            revision: None,
            ended: true,
        },
        Shaped::Ended {
            status: "stopped".into(),
        },
        Shaped::Unverified {
            reason: "TEST".into(),
        },
    ] {
        let shape = done(stage);
        assert!(!offers(&shape, "End this session") && !offers(&shape, "Stop this session"));
    }
}

#[test]
fn a_step_may_assume_at_most_five_complete_defaults() {
    let assumed: Vec<String> = (1..=7)
        .map(|i| format!("{{\"text\":\" TEST default {i} \",\"why\":\"TEST either way\"}}"))
        .chain([
            "{\"text\":\"\",\"why\":\"TEST blank\"}".into(),
            "{\"text\":\"TEST no why\"}".into(),
        ])
        .collect();
    let reply = format!(
        "{{\"next\":\"question\",\"question\":\"Local?\",\"found\":[],\"assumed\":[{}]}}",
        assumed.join(",")
    );
    let step::Next::Question(ask) = step::decode(&reply, "").unwrap() else {
        panic!("not a question")
    };
    assert_eq!(ask.assumed.len(), 5);
    assert_eq!(ask.assumed[0].text, "TEST default 1");
    assert!(ask.assumed.iter().all(|a| !a.why.is_empty()));
}

#[test]
fn assumptions_show_with_their_question_and_object_then_a_number_objects_to_one() {
    let mut shape = done(question());
    // No assumptions: F1 offers no objection.
    assert!(!offers(&shape, "Object"));
    shape.open.as_mut().unwrap().assumed = vec![
        Assumption {
            text: "TEST notes are Markdown".into(),
            why: "TEST either format works".into(),
        },
        Assumption {
            text: "TEST one machine".into(),
            why: "TEST nothing syncs yet".into(),
        },
    ];
    for (width, height) in [(100, 30), (80, 24)] {
        let text = screen(&mut shape, width, height, true);
        let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(
            flat.contains("I'll assume these unless you object:"),
            "{text}"
        );
        assert!(
            flat.contains("2. TEST one machine (why: TEST nothing syncs yet)"),
            "{text}"
        );
        assert!(text.lines().last().unwrap().contains("F1 more"), "{text}");
    }
    choose(&mut shape, "Object");
    assert!(shape.open.as_ref().unwrap().objecting);
    assert!(screen(&mut shape, 80, 24, true).contains("esc keep them all"));
    // Esc keeps them all; a number with no assumption and other keys do nothing.
    shape.key(key(KeyCode::Esc));
    assert!(!shape.open.as_ref().unwrap().objecting);
    assert!(screen(&mut shape, 80, 24, true).contains("Nothing was changed."));
    choose(&mut shape, "Object");
    for code in [
        KeyCode::Char('3'),
        KeyCode::Char('0'),
        KeyCode::Char('x'),
        KeyCode::F(2),
    ] {
        shape.key(key(code));
    }
    let open = shape.open.as_ref().unwrap();
    assert!(open.objecting && open.writing.is_none());
    shape.key(key(KeyCode::Char('2')));
    assert!(shape.open.as_ref().unwrap().writing.is_some());
    // This fixture's session path doesn't exist, so the ledger read refusal lands and both stay.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while shape.open.as_ref().unwrap().writing.is_some() {
        assert!(
            std::time::Instant::now() < deadline,
            "objection never landed"
        );
        shape.tick();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let open = shape.open.as_ref().unwrap();
    assert!(
        open.notice
            .as_ref()
            .unwrap()
            .0
            .starts_with("Cannot read this session:"),
        "{:?}",
        open.notice
    );
    assert_eq!(open.assumed.len(), 2);
}

#[cfg(unix)]
#[test]
fn an_objection_becomes_an_open_decision_asked_before_any_new_one() {
    use crate::seed_me::{Affirmed, Config, answering, shaping};

    let temp = tempfile::Builder::new()
        .prefix("tinkery-TEST-233-")
        .tempdir()
        .unwrap();
    let receipt = Config::new( Some(temp.path().join("TEST-sessions")))
        .confirm(Affirmed {
            goal: "TEST I can object.\nWhy: Defaults can be wrong.\nDone when: It asks.\nMust: Keep my words.\nMust not: Build without approval.".into(),
            outcome: "TEST objection".into(),
            options: vec![],
            originals: vec![],
            context: "{}".into(),
            source: "TEST fixture, not real approval".into(),
        })
        .unwrap();
    let session = receipt.session.clone();
    let assumption = |text: &str| answering::Assumption {
        text: text.into(),
        why: "TEST either way".into(),
    };
    let ask = answering::Ask {
        prerequisites: None,
        supported_by: None,
        id: "storage".into(),
        label: "TEST Storage".into(),
        question: "TEST Local or synced?".into(),
        gate: "TEST Storage disposition".into(),
        found: vec![],
        resolves: None,
        suggestion: None,
        assumed: vec![assumption("TEST Markdown"), assumption("TEST one machine")],
        options: vec![],
        against: None,
        confidence: None,
    };
    let version = answering::read(&session).unwrap().version();
    let version = answering::ask(&session, version, &ask, temp.path()).unwrap();
    // Both defaults are recorded.
    let state = || {
        serde_json::from_str::<serde_json::Value>(&answering::read(&session).unwrap().context())
            .unwrap()
    };
    assert_eq!(state()["assumed"].as_array().unwrap().len(), 2);
    // An unrecorded assumption refuses; a stale version refuses.
    assert!(answering::object(&session, version, "TEST never assumed", None).is_err());
    assert_eq!(
        answering::object(&session, version - 1, "TEST Markdown", None),
        Err(answering::Refusal::Stale)
    );
    let version = answering::object(&session, version, "TEST Markdown", None).unwrap();
    let after = state();
    assert_eq!(
        after["assumed"],
        serde_json::json!([{"text": "TEST one machine", "why": "TEST either way"}])
    );
    let objection = after["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["label"] == "Instead of assuming")
        .unwrap()
        .clone();
    assert_eq!(objection["status"], "unresolved");
    assert!(
        objection["question"]
            .as_str()
            .unwrap()
            .contains("TEST Markdown")
    );
    // The current question stays current; nothing is promoted while one is.
    assert_eq!(after["current_question"], "storage");
    assert_eq!(answering::promote(&session, version).unwrap(), None);
    // Once it's answered, the objection is asked next, with no model.
    let version = answering::answer(&session, version, "storage", "TEST local", None).unwrap();
    let version = answering::promote(&session, version).unwrap().unwrap();
    let Shaped::Question { id, .. } = shaping::read(&session).unwrap().stage else {
        panic!("the objection isn't the current question")
    };
    assert_eq!(serde_json::Value::from(id.as_str()), objection["id"]);
    assert_eq!(answering::promote(&session, version).unwrap(), None);
}

#[test]
fn asking_to_end_or_object_takes_no_words_so_a_paste_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    for ask in ["Stop this session", "Object"] {
        let mut shape = reading(done(question()).with_agent(Some(agent(1))), dir.path());
        shape.open.as_mut().unwrap().assumed = vec![Assumption {
            text: "TEST one machine".into(),
            why: "TEST nothing syncs".into(),
        }];
        shape.paste("TEST kept");
        assert!(shape.takes_text());
        choose(&mut shape, ask);
        assert!(!shape.takes_text(), "{ask} still takes words");
        shape.paste(" TEST pasted while asked");
        assert_eq!(shape.open.as_ref().unwrap().typed.text(), "TEST kept");
        shape.key(key(KeyCode::Esc));
        assert!(shape.takes_text());
    }
}

#[test]
fn a_step_gives_exactly_two_options_with_a_known_undo_cost_or_none() {
    let decode = |options: &str, confidence: &str| {
        let reply = format!(
            "{{\"next\":\"question\",\"question\":\"Local?\",\"found\":[],\"suggestion\":\"Local.\",\"options\":{options},\"against\":\" TEST sync helps later \",\"confidence\":\"{confidence}\"}}"
        );
        let step::Next::Question(ask) = step::decode(&reply, "").unwrap() else {
            panic!("not a question")
        };
        ask
    };
    let two = r#"[{"choice":"Local","tradeoff":"TEST simple","undo":"Cheap"},{"choice":"Synced","tradeoff":"TEST shared","undo":"hard"}]"#;
    let ask = decode(two, "medium");
    assert_eq!(ask.options.len(), 2);
    assert_eq!(ask.options[0].undo, "cheap");
    assert!(ask.hard());
    assert_eq!(ask.against.as_deref(), Some("TEST sync helps later"));
    assert_eq!(ask.recommendation().as_deref(), Some("Local."));
    for options in [
        "[]".to_owned(),
        r#"[{"choice":"Local","tradeoff":"TEST","undo":"cheap"}]"#.to_owned(),
        two.replace("\"hard\"", "\"never\""),
        two.replace("\"Synced\"", "\"\""),
    ] {
        let ask = decode(&options, "sure");
        assert!(ask.options.is_empty(), "{options}");
        assert!(!ask.hard());
        assert_eq!(ask.confidence, None);
    }
}

#[test]
fn a_hard_to_undo_question_asks_for_instinct_before_options_or_suggestion() {
    let hard = Shaped::Question {
        gate: "TEST gate".into(),
        id: "storage".into(),
        title: "Where records live".into(),
        ask: "Keep records local or synced?".into(),
        found: vec!["README.md:3: says local only".into()],
        suggestion: None,
        options: vec![
            "Option A: Local; TEST simple. Undo cost: cheap.".into(),
            "Option B: Synced; TEST shared. Undo cost: hard.".into(),
        ],
        against: Some("TEST sync helps later".into()),
        hard: true,
    };
    for withheld in [None, Some("Local.".to_owned())] {
        let mut shape = done(hard.clone());
        shape.open.as_mut().unwrap().withheld = withheld.clone();
        for (width, height) in [(100, 30), (80, 24)] {
            let text = screen(&mut shape, width, height, true);
            let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
            assert!(flat.contains("so what's your instinct?"), "{text}");
            for hidden in ["Option A", "Against my suggestion", "Suggestion:"] {
                assert!(!flat.contains(hidden), "{hidden} shown first:\n{text}");
            }
            assert!(text.lines().last().unwrap().contains("F1 more"), "{text}");
        }
        choose(&mut shape, "Show the options");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while shape.open.as_ref().unwrap().writing.is_some() {
            assert!(std::time::Instant::now() < deadline, "reveal never landed");
            shape.tick();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(shape.open.as_ref().unwrap().revealed);
        // Recording a withheld suggestion needs the helper; this fixture's path doesn't exist.
        let said = &shape.open.as_ref().unwrap().notice.as_ref().unwrap().0;
        match withheld {
            None => assert!(said.contains("No suggestion was kept"), "{said}"),
            Some(_) => assert!(said.starts_with("Cannot read this session:"), "{said}"),
        }
    }
    // Once revealed, the options and the case against show, and F1 no longer offers them.
    let mut shape = done(hard.clone());
    shape.open.as_mut().unwrap().revealed = true;
    let text = screen(&mut shape, 100, 30, true);
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        flat.contains("Option B: Synced; TEST shared. Undo cost: hard."),
        "{text}"
    );
    assert!(
        flat.contains("Against my suggestion: TEST sync helps later"),
        "{text}"
    );
    assert!(!flat.contains("so what's your instinct?"), "{text}");
    assert!(!offers(&shape, "Show the options"));
    // Cheap and moderate questions show everything at once and offer no reveal.
    let Shaped::Question {
        id,
        title,
        ask,
        found,
        options,
        against,
        ..
    } = hard
    else {
        unreachable!()
    };
    let mut shape = done(Shaped::Question {
        gate: "TEST gate".into(),
        id,
        title,
        ask,
        found,
        suggestion: Some("Local.".into()),
        options,
        against,
        hard: false,
    });
    let text = screen(&mut shape, 100, 30, true);
    assert!(
        text.contains("Option A: Local") && text.contains("Suggestion: Local."),
        "{text}"
    );
    assert!(
        !text.contains("instinct") && !offers(&shape, "Show the options"),
        "{text}"
    );
}

#[cfg(unix)]
#[test]
fn options_live_in_the_question_and_a_hard_suggestion_waits_until_asked_for() {
    use crate::seed_me::{Affirmed, Config, answering, shaping};

    let temp = tempfile::Builder::new()
        .prefix("tinkery-TEST-232-")
        .tempdir()
        .unwrap();
    for undo in ["moderate", "hard"] {
        let receipt = Config::new( Some(temp.path().join("TEST-sessions")))
            .confirm(Affirmed {
                goal: "TEST I can weigh options.\nWhy: Choices have costs.\nDone when: I see them.\nMust: Keep my words.\nMust not: Build without approval.".into(),
                outcome: "TEST options".into(),
                options: vec![],
                originals: vec![],
                context: "{}".into(),
                source: "TEST fixture, not real approval".into(),
            })
            .unwrap();
        let session = receipt.session.clone();
        let option = |choice: &str, undo: &str| answering::Alternative {
            choice: choice.into(),
            tradeoff: format!("TEST {choice} trade-off"),
            undo: undo.into(),
        };
        let ask = answering::Ask {
            prerequisites: None,
            supported_by: None,
            id: "storage".into(),
            label: "TEST Storage".into(),
            question: "TEST Local or synced?".into(),
            gate: "TEST Storage disposition".into(),
            found: vec![],
            resolves: None,
            suggestion: Some("Local.".into()),
            assumed: vec![],
            options: vec![option("Local", "cheap"), option("Synced", undo)],
            against: Some("TEST sync helps later".into()),
            confidence: Some("high".into()),
        };
        let version = answering::read(&session).unwrap().version();
        let version = answering::ask(&session, version, &ask, temp.path()).unwrap();
        let Shaped::Question {
            ask: shown,
            options,
            against,
            hard,
            suggestion,
            ..
        } = shaping::read(&session).unwrap().stage
        else {
            panic!("not a question")
        };
        assert_eq!(shown, "TEST Local or synced?");
        assert_eq!(
            options,
            vec![
                "Option A: Local; TEST Local trade-off. Undo cost: cheap.".to_owned(),
                format!("Option B: Synced; TEST Synced trade-off. Undo cost: {undo}."),
            ]
        );
        assert_eq!(against.as_deref(), Some("TEST sync helps later"));
        assert_eq!(hard, undo == "hard");
        if undo == "hard" {
            // Seed Me: not recorded until the operator answers or asks to see it.
            assert_eq!(suggestion, None);
            answering::recommend(&session, version, "storage", "Local.", None).unwrap();
            let Shaped::Question { suggestion, .. } = shaping::read(&session).unwrap().stage else {
                panic!("not a question")
            };
            assert_eq!(suggestion.as_deref(), Some("Local."));
        } else {
            assert_eq!(suggestion.as_deref(), Some("Local."));
        }
    }
}

/// A TEST session whose real-sample and pre-mortem probes the operator already answered, for
/// steps about drafting.
fn answered_probe(session: &Path) -> u64 {
    use crate::seed_me::answering;
    let mut version = answering::read(session).unwrap().version();
    for (id, words) in [
        (answering::SAMPLE.0, "TEST no sample: nothing real yet"),
        (answering::PROBE.0, "TEST pre-mortem answer"),
    ] {
        version = answering::probe(session, version).unwrap().unwrap();
        version = answering::answer(session, version, id, words, None).unwrap();
    }
    version
}

#[cfg(unix)]
#[test]
fn the_first_steps_put_the_sample_and_pre_mortem_to_the_operator_without_a_model() {
    use crate::{
        pi::CallBudget,
        seed_me::{Affirmed, Config, answering, shaping},
    };
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        time::{Duration, Instant},
    };

    let temp = tempfile::Builder::new()
        .prefix("tinkery-TEST-234-")
        .tempdir()
        .unwrap();
    let save = || {
        let receipt = Config::new( Some(temp.path().join("TEST-sessions")))
            .confirm(Affirmed {
                goal: "TEST I can probe.\nWhy: Blind spots.\nDone when: It asks.\nMust: Keep my words.\nMust not: Build without approval.".into(),
                outcome: "TEST probe".into(),
                options: vec![],
                originals: vec![],
                context: "{}".into(),
                source: "TEST fixture, not real approval".into(),
            })
            .unwrap();
        receipt.session.clone()
    };
    // A model that would draft at once; the step must not reach it.
    let program = temp.path().join("TEST-pi");
    fs::write(&program, "#!/bin/sh\ntouch TEST-MODEL-WAS-CALLED\nexit 1\n").unwrap();
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
    let agent = Arc::new(
        step::Agent::new(
            crate::pi::Harness::Pi,
            program,
            "test/stub".into(),
            String::new(),
            Some(temp.path().into()),
            CallBudget::new(1),
            None,
        )
        .unwrap(),
    );
    let session = save();
    let version = answering::read(&session).unwrap().version();
    let running = step::Running::start(
        step::Work::Look {
            feedback: None,
            expected: version,
            folder: temp.path().into(),
        },
        session.clone(),
        agent.clone(),
    );
    let deadline = Instant::now() + Duration::from_secs(20);
    let result = loop {
        if let Some(result) = running.poll() {
            break result;
        }
        assert!(Instant::now() < deadline, "step timed out");
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(result, Ok(step::Done::Asked(step::Asked::default())));
    assert_eq!(agent.remaining(), 1, "a probe used a model call");
    assert!(!temp.path().join("TEST-MODEL-WAS-CALLED").exists());
    // #740: a real sample comes first, so checks are drafted against real input.
    let Shaped::Question { id, ask, found, .. } = shaping::read(&session).unwrap().stage else {
        panic!("the real-sample probe isn't the current question")
    };
    assert_eq!((id.as_str(), ask.as_str()), answering::SAMPLE);
    assert!(found.is_empty());
    let version = answering::read(&session).unwrap().version();
    assert_eq!(answering::probe(&session, version).unwrap(), None);
    let version = answering::answer(
        &session,
        version,
        answering::SAMPLE.0,
        "TEST no sample",
        None,
    )
    .unwrap();
    // Then the pre-mortem, still without a model.
    let version = answering::probe(&session, version).unwrap().unwrap();
    let Shaped::Question { id, ask, found, .. } = shaping::read(&session).unwrap().stage else {
        panic!("the pre-mortem isn't the current question")
    };
    assert_eq!((id.as_str(), ask.as_str()), answering::PROBE);
    assert!(found.is_empty());
    // Asked once: answered, neither comes back.
    let version =
        answering::answer(&session, version, answering::PROBE.0, "TEST skipped", None).unwrap();
    assert_eq!(answering::probe(&session, version).unwrap(), None);

    // Asked after findings were shown, it says its answer isn't independent of them.
    let session = save();
    let version = answering::read(&session).unwrap().version();
    let ask = answering::Ask {
        prerequisites: None,
        supported_by: None,
        id: "storage".into(),
        label: "TEST Storage".into(),
        question: "TEST Local or synced?".into(),
        gate: "TEST Storage disposition".into(),
        found: vec![answering::Found {
            id: "TEST-fact".into(),
            claim: serde_json::json!({"type":"observation","scope":"TEST file"}),
            supported_by: None,
            contradicts: None,
            checked: "TEST-read.md:1".into(),
            observed: "TEST local only".into(),
        }],
        resolves: None,
        suggestion: None,
        assumed: vec![],
        options: vec![],
        against: None,
        confidence: None,
    };
    fs::write(temp.path().join("TEST-read.md"), "TEST local only\n").unwrap();
    let version = answering::ask(&session, version, &ask, temp.path()).unwrap();
    // A current question is never displaced.
    assert_eq!(answering::probe(&session, version).unwrap(), None);
    let version = answering::answer(&session, version, "storage", "TEST local", None).unwrap();
    let version = answering::probe(&session, version).unwrap().unwrap();
    let version = answering::answer(
        &session,
        version,
        answering::SAMPLE.0,
        "TEST no sample",
        None,
    )
    .unwrap();
    answering::probe(&session, version).unwrap().unwrap();
    let Shaped::Question { ask, .. } = shaping::read(&session).unwrap().stage else {
        panic!("the pre-mortem isn't the current question")
    };
    assert!(
        ask.ends_with("NOT independent: asked after findings were shown."),
        "{ask}"
    );
}

#[test]
fn the_step_answers_the_other_probes_from_evidence_and_records_every_probe() {
    let prompt = step::prompt();
    for rule in [
        "answer two probes from evidence: a counter-example",
        "an outside view",
        "if either raises an operator-only decision, ask it instead of drafting",
        "Only asked and answered ledger questions are operator answers",
        "Record counter-example and outside-view probe readings as typed inference or unknown findings",
        "`Where we did not look:`",
    ] {
        assert!(prompt.contains(rule), "missing probe rule: {rule}");
    }
}

fn long_draft() -> Shaped {
    let body: String = (0..60).map(|i| format!("TEST line {i}\n")).collect();
    Shaped::Drafted {
        revision: Some("r1".into()),
        text: Some(format!(
            "status: draft\nrevision: r1\n\n{body}LAST TEST LINE\n"
        )),
    }
}

#[test]
fn an_agent_declaration_holds_every_shape_write_and_says_why() {
    let ready = Shaped::Ready {
        revision: Some("r1".into()),
        ended: false,
    };
    for stage in [question(), Shaped::Waiting, long_draft(), ready] {
        for mut shape in [
            done(stage.clone()).with_agent(Some(agent(2))).held(true),
            done(stage.clone()).held(true).with_agent(Some(agent(2))),
        ] {
            assert!(shape.agent.is_none(), "a held Shape kept its agent");
            assert!(!shape.takes_text(), "a held Shape took words");
            let flat = |text: String| text.split_whitespace().collect::<Vec<_>>().join(" ");
            assert!(flat(screen(&mut shape, 100, 30, true)).contains(HELD));
            shape.key(key(KeyCode::F(2)));
            shape.key(key(KeyCode::F(2)));
            let open = shape.open.as_ref().unwrap();
            assert!(open.running.is_none() && open.writing.is_none() && open.asking.is_none());
            let text = screen(&mut shape, 100, 30, true);
            assert!(flat(text.clone()).contains(HELD), "{text}");
            let bar = text.lines().last().unwrap();
            assert!(!bar.contains("F2"), "held Shape offers F2: {bar}");
            for label in [
                "End",
                "Stop",
                "Defer",
                "Delegate",
                "Teach back",
                "Choose the folder",
            ] {
                assert!(!offers(&shape, label), "held Shape offers {label}");
            }
            assert!(shape.help_text().contains(HELD));
        }
    }
}

#[test]
fn seed_confirmation_hints_follow_the_current_drafts_reading_gate() {
    for (width, height) in [(80, 24), (100, 30)] {
        for no_color in [false, true] {
            let mut shape = done(long_draft()).with_agent(Some(agent(0)));
            let initial = screen(&mut shape, width, height, no_color);
            let bar = initial.lines().last().unwrap();
            assert!(bar.contains("pgup/dn read to end"), "{initial}");
            assert_eq!(initial.matches("pgup/dn read to end").count(), 1);
            assert!(!initial.contains("F2 confirm"), "{initial}");
            assert!(bar.contains("esc back"), "{initial}");
            assert!(!initial.contains("Next:"), "{initial}");
            // A refusal must not restore a premature confirmation instruction.
            for code in [KeyCode::F(2), KeyCode::Enter] {
                shape.key(key(code));
                let text = screen(&mut shape, width, height, no_color);
                assert!(
                    text.lines().last().unwrap().contains("pgup/dn read to end"),
                    "{text}"
                );
                assert!(!text.contains("Next: type your own words"), "{text}");
                assert!(!text.contains("type confirm, then F2"), "{text}");
                assert!(shape.open.as_ref().unwrap().running.is_none());
            }
            shape.paste("TEST exact words café  ");
            assert_eq!(
                shape.open.as_ref().unwrap().typed.text(),
                "TEST exact words café  "
            );
            shape.key(key(KeyCode::F(2)));
            let refusal = screen(&mut shape, width, height, no_color);
            assert!(refusal.contains(READ_FIRST), "{refusal}");
            let mut text = String::new();
            for _ in 0..30 {
                shape.key(key(KeyCode::PageDown));
                text = screen(&mut shape, width, height, no_color);
                if text.lines().any(|line| line.trim() == "End of seed") {
                    break;
                }
            }
            // The very first frame that reaches the end offers confirmation.
            assert!(text.contains("End of seed"), "{text}");
            assert!(
                text.lines().last().unwrap().contains("F2 confirm seed r1"),
                "{text}"
            );
            assert!(
                !text.contains(READ_FIRST),
                "first end frame kept the obsolete refusal: {text}"
            );
            assert!(!text.contains("read to end"), "{text}");
            shape.key(key(KeyCode::PageUp));
            shape.paste("more words ".repeat(30).as_str());
            for (w, h) in [(80, 24), (100, 30)] {
                let text = screen(&mut shape, w, h, no_color);
                assert!(
                    text.lines().last().unwrap().contains("F2 confirm seed r1"),
                    "{text}"
                );
                assert!(!text.contains("read to end"), "{text}");
                let open = shape.open.as_ref().unwrap();
                assert_eq!(
                    open.typed.text(),
                    format!("TEST exact words café  {}", "more words ".repeat(30))
                );
                let Reading::Done(Ok(reading)) = &open.reading else {
                    panic!()
                };
                assert_eq!(reading.goal.as_deref(), Some(GOAL));
                assert_eq!(reading.stage, long_draft());
                assert!(open.running.is_none() && open.writing.is_none());
            }
            // Empty words gain their instruction on the first end frame too.
            let mut empty = done(long_draft()).with_agent(Some(agent(0)));
            empty.scroll = u16::MAX;
            let text = screen(&mut empty, width, height, no_color);
            assert!(text.contains("type confirm, then F2"), "{text}");
            assert!(
                text.lines().last().unwrap().contains("F2 confirm seed r1"),
                "{text}"
            );
            empty.key(key(KeyCode::F(2)));
            let refusal = screen(&mut empty, width, height, no_color);
            assert!(
                refusal.contains(WORD_REFUSALS[1]),
                "unrelated refusal was cleared: {refusal}"
            );
        }
    }
}

#[test]
fn down_reads_a_draft_to_its_end_while_the_confirm_box_has_the_words() {
    for (width, height) in [(100, 30), (80, 24)] {
        let mut shape = done(long_draft()).with_agent(Some(agent(0)));
        screen(&mut shape, width, height, true);
        shape.paste("confirm");
        let mut text = String::new();
        for _ in 0..400 {
            shape.key(key(KeyCode::Down));
            text = screen(&mut shape, width, height, true);
            if text.contains("End of seed") {
                break;
            }
        }
        assert!(
            text.contains("End of seed"),
            "Down never reached the end:\n{text}"
        );
        assert!(
            text.lines().last().unwrap().contains("F2 confirm seed r1"),
            "{text}"
        );
        shape.key(key(KeyCode::Up));
        assert_eq!(shape.open.as_ref().unwrap().typed.text(), "confirm");
    }
}

#[test]
fn f2_waits_until_the_whole_seed_draft_has_been_on_screen() {
    for (width, height) in [(100, 30), (80, 24)] {
        let mut shape = done(long_draft()).with_agent(Some(agent(0)));
        screen(&mut shape, width, height, true);
        shape.paste("I confirm r1.");
        shape.key(key(KeyCode::F(2)));
        assert!(shape.open.as_ref().unwrap().running.is_none());
        let refusal = screen(&mut shape, width, height, true);
        assert!(
            refusal.contains("PgDn or Down to 'End of seed' first."),
            "{refusal}"
        );
        for _ in 0..30 {
            shape.key(key(KeyCode::PageDown));
            screen(&mut shape, width, height, true);
        }
        let text = screen(&mut shape, width, height, true);
        let last = text.find("LAST TEST LINE").expect(&text);
        let end = text.find("End of seed").expect(&text);
        assert!(last < end, "{text}");
        assert_eq!(text.matches("End of seed").count(), 1, "{text}");
        assert!(
            !text.contains(READ_FIRST),
            "reaching the end left the refusal: {text}"
        );
        assert!(shape.open.as_ref().unwrap().read_end.is_some());
        // A taller editor afterwards doesn't take back what was read.
        shape.paste("\nmore words");
        screen(&mut shape, width, height, true);
        assert!(shape.open.as_ref().unwrap().read_end.is_some());
    }
}

#[test]
fn read_to_end_survives_same_revision_rereads_but_not_a_new_revision() {
    for (width, height) in [(80, 24), (100, 30)] {
        for no_color in [false, true] {
            for change in ["same draft", "changed bytes", "new revision"] {
                let mut shape = done(long_draft()).with_agent(Some(agent(0)));
                shape.scroll = u16::MAX;
                screen(&mut shape, width, height, no_color);
                let changed = change != "same draft";
                let open = shape.open.as_mut().unwrap();
                let Reading::Done(Ok(mut reading)) =
                    std::mem::replace(&mut open.reading, Reading::NotStarted)
                else {
                    panic!()
                };
                if let Shaped::Drafted { revision, text } = &mut reading.stage {
                    if change == "changed bytes" {
                        text.as_mut()
                            .unwrap()
                            .push_str("TEST unread changed bytes, still r1.\n");
                    } else if change == "new revision" {
                        *revision = Some("r2".into());
                        *text = Some(
                            text.as_ref()
                                .unwrap()
                                .replace("revision: r1", "revision: r2"),
                        );
                    }
                }
                let stage = reading.stage.clone();
                let (tx, rx) = mpsc::channel();
                open.reading = Reading::Waiting(rx);
                tx.send((Ok(reading), None)).unwrap();
                shape.scroll = 0;
                shape.tick();
                assert_eq!(shape.open.as_ref().unwrap().read_end.is_none(), changed);
                shape.paste(
                    "TEST confirmation words that grow the box "
                        .repeat(15)
                        .as_str(),
                );
                let text = screen(&mut shape, width, height, no_color);
                assert_eq!(shape.open.as_ref().unwrap().read_end.is_none(), changed);
                let bar = text.lines().last().unwrap();
                assert_eq!(
                    bar.contains("pgup/dn read to end"),
                    changed,
                    "{change}: {text}"
                );
                assert_eq!(
                    bar.contains("F2 confirm seed r1"),
                    !changed,
                    "{change}: {text}"
                );
                if changed {
                    shape.key(key(KeyCode::F(2)));
                    assert!(shape.open.as_ref().unwrap().running.is_none());
                    assert!(
                        screen(&mut shape, width, height, no_color)
                            .contains("PgDn or Down to 'End of seed' first.")
                    );
                }
                let open = shape.open.as_ref().unwrap();
                assert_eq!(
                    open.typed.text(),
                    "TEST confirmation words that grow the box ".repeat(15)
                );
                let Reading::Done(Ok(reading)) = &open.reading else {
                    panic!()
                };
                assert_eq!(reading.stage, stage);
            }
        }
    }
}

#[test]
fn changing_the_words_clears_a_refusal_about_them_and_only_then() {
    let mut shape = done(Shaped::Drafted {
        revision: Some("r1".into()),
        text: Some("TEST seed".into()),
    })
    .with_agent(Some(agent(0)));
    screen(&mut shape, 80, 24, true);
    shape.paste("I confirm\nr1.");
    shape.key(key(KeyCode::F(2)));
    assert!(screen(&mut shape, 80, 24, true).contains(WORD_REFUSALS[1]));
    shape.key(key(KeyCode::Left));
    assert!(
        screen(&mut shape, 80, 24, true).contains(WORD_REFUSALS[1]),
        "moving the caret changed no words but cleared the refusal"
    );
    shape.key(key(KeyCode::Backspace));
    assert!(!screen(&mut shape, 80, 24, true).contains(WORD_REFUSALS[1]));
}

#[test]
fn page_keys_move_about_one_screen_of_reading() {
    for (width, height) in [(80, 24), (100, 30), (160, 40)] {
        let mut shape = done(long_draft());
        screen(&mut shape, width, height, true);
        assert!(shape.page > 5, "{width}x{height}: {} rows", shape.page);
        shape.key(key(KeyCode::PageDown));
        assert_eq!(shape.scroll, crate::page(shape.page));
        shape.key(key(KeyCode::PageUp));
        assert_eq!(shape.scroll, 0);
    }
}

#[test]
fn issue_314_captured_goal_names_missing_success_before_any_call() {
    let root = tempfile::Builder::new()
        .prefix("TEST-314-")
        .tempdir()
        .unwrap();
    let session = root.path().join("TEST-session");
    std::fs::create_dir(&session).unwrap();
    let original = include_str!("../../tests/fixtures/shape-296/ledger.json");
    std::fs::write(session.join("ledger.json"), original).unwrap();
    let before = std::fs::read(session.join("ledger.json")).unwrap();
    for (width, height) in [(80, 24), (100, 30)] {
        for no_color in [false, true] {
            let host = agent(1);
            let mut shape = Shape::session(&session).with_agent(Some(host.clone()));
            shape.open.as_mut().unwrap().reading =
                Reading::Done(Ok(crate::seed_me::shaping::read(&session).unwrap()));
            let shown = screen(&mut shape, width, height, no_color);
            assert!(
                shown.contains("The saved goal leaves Done when open."),
                "{shown}"
            );
            assert!(shown.contains("F2 look into the next question"), "{shown}");
            assert_eq!(host.remaining(), 1);
            assert!(shape.open.as_ref().unwrap().running.is_none());
            assert_eq!(std::fs::read(session.join("ledger.json")).unwrap(), before);
            assert!(!session.join("seed-contract.md").exists());
        }
    }
}

#[cfg(unix)]
#[test]
fn stopping_after_a_reply_arrives_discards_its_retry_feedback() {
    use crate::{
        pi::CallBudget,
        seed_me::{Affirmed, Config, answering},
    };
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        time::{Duration, Instant},
    };

    let temp = tempfile::Builder::new()
        .prefix("tinkery-TEST-400-stop-")
        .tempdir()
        .unwrap();
    let session = Config::new(Some(temp.path().join("TEST-sessions")))
        .confirm(Affirmed {
            goal: GOAL.into(),
            outcome: "TEST next question".into(),
            options: vec![],
            originals: vec![],
            context: "{}".into(),
            source: "TEST fixture, not real approval".into(),
        })
        .unwrap()
        .session;
    answered_probe(&session);
    let before = fs::read(session.join("ledger.json")).unwrap();
    let program = temp.path().join("TEST-pi");
    fs::write(
        &program,
        r#"#!/usr/bin/env python3
import json, pathlib, sys
request = json.load(sys.stdin)
with pathlib.Path('TEST-calls.jsonl').open('a') as output:
    output.write(json.dumps(request) + '\n')
print('TEST deliberately invalid reply')
"#,
    )
    .unwrap();
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
    let budget = CallBudget::new(2);
    let agent = Agent::new(
        crate::pi::Harness::Pi,
        program,
        "test/stub".into(),
        String::new(),
        Some(temp.path().into()),
        budget.clone(),
        None,
    )
    .unwrap();
    let mut shape = Shape::session(&session).with_agent(Some(Arc::new(agent)));
    let ready = |shape: &mut Shape| {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            shape.tick();
            let shown = screen(shape, 100, 30, true);
            if shown.contains("F2 look into the next question") {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "Shape did not offer an explicit retry: {shown}"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    };
    ready(&mut shape);
    shape.key(key(KeyCode::F(2)));
    let deadline = Instant::now() + Duration::from_secs(10);
    while !budget
        .attempts()
        .iter()
        .any(|attempt| attempt.outcome == "reply")
    {
        assert!(Instant::now() < deadline, "Stub did not return its reply");
        std::thread::sleep(Duration::from_millis(10));
    }
    // Simulate the terminal not ticking while the completed rejection waits for consumption.
    std::thread::sleep(Duration::from_millis(300));
    shape.key(key(KeyCode::Esc));
    ready(&mut shape);
    shape.key(key(KeyCode::F(2)));
    ready(&mut shape);
    let calls = fs::read_to_string(temp.path().join("TEST-calls.jsonl")).unwrap();
    let calls: Vec<serde_json::Value> = calls
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(
        calls.len(),
        2,
        "Only the two explicit F2 keys may call the stub"
    );
    assert!(
        calls
            .iter()
            .all(|call| call.get("previous_refusal").is_none()),
        "A stopped reply reached the next step: {calls:?}"
    );
    assert_eq!(fs::read(session.join("ledger.json")).unwrap(), before);
    assert_eq!(answering::read(&session).unwrap().goal(), Some(GOAL));
    assert!(!session.join("seed-contract.md").exists());
}

#[test]
fn retired_function_keys_do_nothing_and_f1_reaches_every_action() {
    let dir = tempfile::tempdir().unwrap();
    let mut shape = reading(done(question()).with_agent(Some(agent(1))), dir.path());
    shape.open.as_mut().unwrap().assumed.push(Assumption {
        text: "TEST default".into(),
        why: "TEST equal".into(),
    });
    for n in 3..=12 {
        shape.key(key(KeyCode::F(n)));
        let open = shape.open.as_ref().unwrap();
        assert!(
            open.form.is_none()
                && open.choosing.is_none()
                && open.aside.is_none()
                && open.asking.is_none()
                && open.so_far.is_none()
                && !open.objecting
                && open.running.is_none()
                && open.writing.is_none(),
            "F{n} still does something"
        );
    }
    for label in [
        "Defer",
        "Delegate",
        "Object",
        "Choose the folder",
        "So far",
        "Stop this session",
    ] {
        assert!(offers(&shape, label), "{label}: {}", shape.help_text());
    }
}
