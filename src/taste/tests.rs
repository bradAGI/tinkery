use super::*;
use read::{Check, Item};

fn row() -> ResultRow {
    ResultRow {
        id: "TEST-run-id".into(),
        slug: "TEST-run".into(),
        goal: Goal::Matched(
            " TEST café  \nWhy: TEST reason.\nDone when: TEST check.\nMust not: TEST claims.  "
                .into(),
        ),
        url: Some("https://github.com/TEST/example/pull/1".into()),
        title: Some("TEST clinic allocation comparison".into()),
        head: Some("a".repeat(40)),
        merged: false,
        preview: Err("No preview declared".into()),
        receipts_missing: false,
        seed: "TEST-digest".into(),
        items: vec![Item {
            label: "TEST check".into(),
            state: Check::Pending,
            kind: "automated".into(),
            evidence: "TEST evidence".into(),
        }],
        gates: vec![
            "re-approve the plan".into(),
            "confirm no clinical claims".into(),
        ],
        gaps: Vec::new(),
        labelled: Some(false),
    }
}

fn ready() -> Taste {
    let mut taste = Taste::open(Some(PathBuf::from("/TEST")), false);
    taste.rows = vec![row()];
    taste.at = Some(Instant::now());
    taste
}

fn screen(taste: &mut Taste, width: u16, height: u16) -> String {
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|f| taste.render(f, f.area(), Palette::new(true)))
        .unwrap();
    terminal
        .backend()
        .buffer()
        .content()
        .chunks(width as usize)
        .map(|row| row.iter().map(|c| c.symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn preview_disclosure_says_each_fact_once_without_running_on_navigation() {
    for (width, height) in [(100, 30), (80, 24)] {
        let mut taste = ready();
        taste.rows[0].preview = Ok(serde_json::from_value(serde_json::json!({
            "command": "TEST preview",
            "url": "http://127.0.0.1:5173/"
        }))
        .unwrap());
        assert!(screen(&mut taste, width, height).contains("Enter open result"));
        taste.key(KeyEvent::from(KeyCode::Enter));
        let review = screen(&mut taste, width, height);
        assert!(
            review.contains(r#"v runs "TEST preview", then opens http://127.0.0.1:5173/"#),
            "{review}"
        );
        assert!(!review.contains(&"a".repeat(40)), "commit stays in details");
        let detail = result_lines(&taste.rows[0]).join("\n");
        assert_eq!(detail.matches(&"a".repeat(40)).count(), 1);
        assert!(taste.preview.is_none(), "navigation ran the command");
        taste.rows[0].preview = Err("No preview for this project.".into());
        taste.key(KeyEvent::from(KeyCode::Char('v')));
        let shown = screen(&mut taste, width, height);
        assert_eq!(
            shown.matches("No preview for this project.").count(),
            2,
            "review line and notice: {shown}"
        );
        assert!(shown.contains("? says how to add one") && taste.preview.is_none());
        assert!(
            taste
                .help_text()
                .contains("commit tinkery.preview at the project root")
        );
    }
}

#[test]
fn exact_goal_survives_display_and_both_handoffs_without_a_verdict() {
    let row = row();
    let Goal::Matched(goal) = &row.goal else {
        panic!()
    };
    let visible = result_lines(&row).join("\n");
    for line in goal.split('\n') {
        assert!(visible.contains(line));
    }
    for action in ['a', 'c'] {
        let copy = handoff(&row, action).unwrap();
        assert!(copy.contains(goal) && copy.contains(&"a".repeat(40)));
        assert!(copy.contains("Recheck current PR head, inputs and receipts"));
        assert!(
            copy.contains("re-approve the plan") && copy.contains("confirm no clinical claims")
        );
        assert!(copy.contains("No verdict recorded"));
        assert!(!copy.contains("gh pr review") && !copy.contains("gh pr merge"));
    }
}

#[test]
fn unknown_goal_or_result_refuses_copy_and_navigation_starts_nothing() {
    let mut taste = ready();
    for code in [
        KeyCode::Enter,
        KeyCode::Down,
        KeyCode::PageDown,
        KeyCode::Home,
        KeyCode::End,
        KeyCode::Char('n'),
        KeyCode::Char('p'),
    ] {
        taste.key(KeyEvent::from(code));
        assert!(
            taste.reading.is_none()
                && taste.opening.is_none()
                && taste.take_copy_request().is_none()
        );
    }
    for goal in [Goal::Unknown, Goal::NoSeed] {
        taste.rows[0].goal = goal;
        taste.key(KeyEvent::from(KeyCode::Char('a')));
        assert!(taste.take_copy_request().is_none());
    }
    taste.rows[0] = row();
    taste.rows[0].head = None;
    assert!(handoff(&taste.rows[0], 'c').is_err());
    taste.rows[0].head = Some("a".repeat(40));
    taste.rows[0].url = None;
    assert!(handoff(&taste.rows[0], 'a').is_err());
}

#[test]
fn repeated_or_modified_keys_never_copy_open_or_read() {
    let mut taste = ready();
    taste.detail = true;
    for code in ['a', 'c', 'r', 'o', 'l', 't', 'e', 'v'] {
        let mut key = KeyEvent::from(KeyCode::Char(code));
        key.kind = KeyEventKind::Repeat;
        taste.key(key);
        key.kind = KeyEventKind::Press;
        key.modifiers = KeyModifiers::ALT;
        taste.key(key);
        assert!(taste.pause.is_none() && taste.take_draft().is_none() && !taste.evidence);
        assert!(
            taste.reading.is_none()
                && taste.opening.is_none()
                && taste.take_copy_request().is_none()
        );
    }
    taste.key(KeyEvent::from(KeyCode::Char('a')));
    assert!(
        taste
            .take_copy_request()
            .unwrap()
            .contains("Acceptance review requested")
    );
    assert!(taste.take_copy_request().is_none());
    taste.copy_result(true);
    assert!(taste.notice.contains("No verdict recorded"));
    taste.copy_result(false);
    assert!(taste.notice.contains("Clipboard unavailable"));
    taste.rows.clear();
    let shown = screen(&mut taste, 80, 24);
    assert!(shown.contains("r read results"));
    assert!(!shown.contains("run preview") && !shown.contains("accepted externally"));
    taste.key(KeyEvent::from(KeyCode::Char('e')));
    assert!(
        !taste.evidence,
        "a refresh with no current result cannot queue an evidence view"
    );
}

#[test]
fn list_uses_counts_and_detail_keeps_every_source_word_with_evidence_behind_e() {
    for (w, h) in [(100, 30), (80, 24)] {
        let mut taste = ready();
        let mut evidence = taste.rows[0].items[0].clone();
        evidence.evidence = "/TEST/log; argv: TEST-command; receipt TEST-hash".into();
        taste.rows[0].items = [
            Check::Pass,
            Check::Fail,
            Check::Pending,
            Check::Stale,
            Check::NoneReported,
        ]
        .into_iter()
        .map(|state| Item {
            label: format!("TEST exact {state} wording"),
            state,
            ..evidence.clone()
        })
        .collect();
        taste.rows[0].gaps = vec!["TEST gap /TEST/source; argv: TEST-gap-command".into()];
        let full = result_lines(&taste.rows[0]).join("\n");
        let copies = ['a', 'c'].map(|action| handoff(&taste.rows[0], action).unwrap());
        let list = screen(&mut taste, w, h);
        for visible in [
            "TEST café",
            "✓ 1 passed",
            "! 1 failed",
            "… 1 waiting",
            "… 1 stale",
            "? 1 unknown",
            "You owe 2",
        ] {
            assert!(list.contains(visible), "{visible}: {list}");
        }
        for hidden in [
            "/TEST",
            "TEST-hash",
            "TEST-command",
            "TEST-run",
            "TEST exact",
            "github.com",
            "Commit:",
            "Why: TEST reason",
        ] {
            assert!(!list.contains(hidden), "{hidden}: {list}");
            assert!(full.contains(hidden), "{hidden}: {full}");
        }
        assert!(
            list.split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .contains("awaiting human verdict"),
            "{list}"
        );
        taste.key(KeyEvent::from(KeyCode::Enter));
        assert!(taste.detail && !taste.evidence);
        let review = screen(&mut taste, w, h);
        for visible in [
            "TEST clinic allocation comparison",
            "Your goal",
            " TEST café",
            "Is this good?",
            "w  answer pass or fail",
            "! TEST gap /TEST/source",
            "w write verdict",
        ] {
            assert!(review.contains(visible), "{visible}: {review}");
        }
        for hidden in [
            "TEST exact",
            "Why: TEST reason",
            "github.com",
            "Commit:",
            "Run ID:",
            "TEST-digest",
            "accepted externally",
            "a/c copy",
        ] {
            assert!(!review.contains(hidden), "{hidden}: {review}");
        }
        let document = view::details(&taste.rows[0], taste.at, u16::MAX, Palette::new(true))
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        for heading in ["You owe", "Checks", "Goal", "Result", "Evidence"] {
            assert!(document.contains(heading));
        }
        for item in &taste.rows[0].items {
            assert!(document.contains(&item.label));
        }
        assert!(document.contains("/TEST/log") && document.contains("argv:"));
        assert!(document.contains("waiting: not finished"));
        taste.key(KeyEvent::from(KeyCode::Char('l')));
        assert!(screen(&mut taste, w, h).contains("Guess · Next:"));
        taste.key(KeyEvent::from(KeyCode::Char('e')));
        assert!(taste.evidence);
        taste.key(KeyEvent::from(KeyCode::End));
        assert!(screen(&mut taste, w, h).contains("TEST-gap-command"));
        for (index, action) in ['a', 'c'].into_iter().enumerate() {
            taste.key(KeyEvent::from(KeyCode::Char(action)));
            assert_eq!(taste.take_copy_request().unwrap(), copies[index]);
        }
        taste.key(KeyEvent::from(KeyCode::Esc));
        assert!(taste.detail && !taste.evidence);
        taste.key(KeyEvent::from(KeyCode::Esc));
        assert!(!taste.detail);
        assert!(taste.reading.is_none() && taste.opening.is_none());
    }
}

#[test]
fn both_sizes_scroll_to_full_evidence_and_keep_navigation() {
    for (w, h) in [(100, 30), (80, 24)] {
        let mut taste = ready();
        taste.rows[0].items[0].evidence =
            format!("{} TEST-LAST-EVIDENCE", "TEST evidence ".repeat(500));
        taste.key(KeyEvent::from(KeyCode::Enter));
        let first = screen(&mut taste, w, h);
        for visible in ["TEST clinic", "TEST café", "Is this good?", "e details"] {
            assert!(first.contains(visible), "{visible}: {first}");
        }
        assert!(!first.contains("TEST-LAST-EVIDENCE"));
        taste.key(KeyEvent::from(KeyCode::Char('e')));
        let details = screen(&mut taste, w, h);
        assert!(details.contains("You owe") && details.contains("e back to the result"));
        assert!(!details.contains("TEST-LAST-EVIDENCE"));
        taste.key(KeyEvent::from(KeyCode::End));
        let tail = screen(&mut taste, w, h);
        assert!(tail.contains("TEST-LAST-EVIDENCE"), "{tail}");
        assert!(tail.contains(&"a".repeat(40)), "{tail}");
        assert_eq!(tail.matches("Commit:").count(), 1);
        taste.key(KeyEvent::from(KeyCode::Esc));
        assert!(taste.detail && !taste.evidence);
        assert!(screen(&mut taste, w, h).contains("Is this good?"));
        taste.key(KeyEvent::from(KeyCode::Esc));
        assert!(!taste.detail);
        assert!(taste.key(KeyEvent::from(KeyCode::Esc)));
    }
}

#[test]
fn selection_is_retained_by_run_and_pr_not_index_and_missing_selection_returns_to_list() {
    let mut taste = ready();
    taste.rows.push(ResultRow {
        id: "TEST-other".into(),
        ..row()
    });
    taste.selected = 1;
    taste.detail = true;
    let key = ("TEST-other".to_owned(), taste.rows[1].url.clone());
    taste.selection = Some(key);
    let mut rows = taste.rows.clone();
    rows.reverse();
    taste.reading = Some(Read::task(move |_| Ok(rows)));
    settle(&mut taste);
    assert_eq!(taste.rows[taste.selected].id, "TEST-other");
    taste.reading = Some(Read::task(|_| Ok(vec![row()])));
    settle(&mut taste);
    assert!(!taste.detail && taste.notice.contains("no longer"));
}

fn settle(taste: &mut Taste) {
    let until = Instant::now() + std::time::Duration::from_secs(5);
    while taste.reading.is_some() || taste.opening.is_some() {
        assert!(Instant::now() < until);
        taste.poll();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

#[test]
fn unsafe_terminal_controls_are_only_escaped_for_display() {
    assert_eq!(safe("TEST\x1b[31m\u{202e}"), "TEST\\u{1b}[31m\\u{202e}");
    assert_eq!(safe(" TEST café  "), " TEST café  ");
}

#[test]
fn next_dump_requires_explicit_external_acceptance_not_copy_or_green() {
    let mut taste = ready();
    taste.key(KeyEvent::from(KeyCode::Enter));
    for action in ['t', 'a', 'c'] {
        taste.key(KeyEvent::from(KeyCode::Char(action)));
        assert!(taste.pause.is_none() && taste.take_draft().is_none());
    }
    let original = goal(&taste.rows[0].goal).to_owned();
    taste.key(KeyEvent::from(KeyCode::Char('l')));
    assert!(taste.pause.is_some());
    assert!(taste.reading.is_none() && taste.opening.is_none());
    assert_eq!(goal(&taste.rows[0].goal), original);
    for (w, h) in [(100, 30), (80, 24)] {
        let shown = screen(&mut taste, w, h);
        assert!(
            shown.contains("Guess · Next: Resolve TEST check."),
            "{shown}"
        );
        assert_eq!(shown.matches("Next:").count(), 1, "{shown}");
        for label in [
            "Outcome:",
            "Evidence: snapshot",
            "Learning:",
            "Futures:",
            "Decision:",
        ] {
            assert!(!shown.contains(label), "{shown}");
        }
        let help = taste.help_text();
        for label in [
            "Outcome:",
            "Evidence:",
            "Learning:",
            "Futures:",
            "Decision:",
        ] {
            assert_eq!(help.matches(label).count(), 1, "{help}");
        }
        assert!(!help.contains("Next:"));
    }
    taste.key(KeyEvent::from(KeyCode::Char('t')));
    let draft = taste.take_draft().unwrap();
    assert!(draft.text.contains("Resolve TEST check"));
    assert!(draft.text.ends_with(&original));
    assert!(draft.provenance.contains(&"a".repeat(40)));
    taste.key(KeyEvent::from(KeyCode::Char('c')));
    assert!(taste.pause.is_none() && taste.take_draft().is_none());
    taste.rows[0].goal = Goal::Unknown;
    taste.key(KeyEvent::from(KeyCode::Char('l')));
    assert!(taste.pause.is_none());
}

#[test]
fn merged_result_is_labelled_but_needs_explicit_acceptance_before_the_note() {
    for (w, h) in [(100, 30), (80, 24)] {
        let mut taste = ready();
        taste.rows[0].merged = true;
        let list = screen(&mut taste, w, h);
        assert!(list.contains(" · merged"), "{list}");
        assert!(taste.pause.is_none() && taste.take_draft().is_none());
        taste.key(KeyEvent::from(KeyCode::Enter));
        taste.key(KeyEvent::from(KeyCode::End));
        let detail = screen(&mut taste, w, h);
        assert!(
            detail.lines().any(|line| line.trim() == "merged") && !detail.contains("Guess · Next:"),
            "{detail}"
        );
        taste.key(KeyEvent::from(KeyCode::Char('t')));
        assert!(taste.take_draft().is_none());
        taste.key(KeyEvent::from(KeyCode::Char('l')));
        assert!(screen(&mut taste, w, h).contains("Guess · Next:"));
        assert!(taste.reading.is_none() && taste.opening.is_none());
    }
}

#[test]
fn next_suggestion_preserves_source_punctuation_without_adding_another_stop() {
    for (label, sentence) in [
        ("TEST check", "TEST check."),
        ("TEST check.", "TEST check."),
        ("TEST check?", "TEST check?"),
        ("TEST check!", "TEST check!"),
        ("TEST check…", "TEST check…"),
    ] {
        let mut r = row();
        r.gates.clear();
        r.items[0].label = label.into();
        for (state, verb) in [(Check::Stale, "Resolve"), (Check::Pending, "Check")] {
            r.items[0].state = state;
            let pause = next::suggest(&r);
            assert_eq!(pause.next, format!("Guess · Next: {verb} {sentence}"));
            assert_eq!(
                pause.draft.text.lines().next().unwrap(),
                format!("{verb} {sentence}")
            );
            assert_eq!(r.items[0].label, label);
            assert!(pause.draft.text.ends_with(goal(&r.goal)));
        }
        r.items[0].state = Check::Pass;
        r.gates.push(label.into());
        let pause = next::suggest(&r);
        assert_eq!(
            pause.next,
            format!("Guess · Next: Resolve the owed action: {sentence}")
        );
        assert_eq!(
            pause.draft.text.lines().next().unwrap(),
            format!("Resolve the owed action: {sentence}")
        );
        assert_eq!(r.gates[0], label);
    }
}

#[test]
fn pause_prefers_failed_or_stale_checks_and_invents_no_next_feature() {
    let mut r = row();
    r.gates.clear();
    for state in [Check::Fail, Check::Stale] {
        r.items[0].state = state;
        let pause = next::suggest(&r);
        assert!(pause.next.contains("Resolve TEST check"));
        assert!(pause.lines.join(" ").contains("Decision: stop and repair"));
    }
    r.items[0].state = Check::Pass;
    r.gates.clear();
    let pause = next::suggest(&r);
    assert!(pause.lines.join(" ").contains("Decision: proceed"));
    assert!(pause.next.contains("Observe “TEST café”"));
    assert!(pause.next.contains("choose the next slice from this goal"));
    assert!(!pause.draft.text.contains("Add a feature"));
    r.items[0].state = Check::Pending;
    let pending = next::suggest(&r);
    assert_eq!(pending.next, "Guess · Next: Check TEST check.");
    assert!(pending.lines.join(" ").contains("Decision: proceed"));
}

#[test]
fn absent_ci_is_unknown_and_never_stop_when_source_checks_pass_and_nothing_is_owed() {
    let mut r = row();
    r.items[0].state = Check::Pass;
    r.gates.clear();
    r.items.push(Item {
        label: "PR checks".into(),
        state: Check::NoneReported,
        kind: "GitHub".into(),
        evidence: "No PR-head checks reported".into(),
    });
    let pause = next::suggest(&r);
    assert!(!pause.lines.join(" ").contains("stop and repair"));
    assert!(pause.lines.join(" ").contains("Decision: proceed"));
    assert!(pause.next.starts_with("Guess · Next: Observe “TEST café”"));
    assert!(!pause.next.contains("PR checks"));
    assert!(pause.draft.text.ends_with(goal(&r.goal)));
    r.gaps.push("Source evidence unknown".into());
    assert!(
        !next::suggest(&r)
            .lines
            .join(" ")
            .contains("stop and repair")
    );
}

#[test]
fn next_line_stays_single_and_italic_in_both_palettes() {
    for (w, h) in [(100, 30), (80, 24)] {
        for mono in [false, true] {
            let mut taste = ready();
            taste.detail = true;
            taste.rows[0].items[0].label = "TEST long completion condition ".repeat(50);
            taste.key(KeyEvent::from(KeyCode::Char('l')));
            let mut terminal =
                ratatui::Terminal::new(ratatui::backend::TestBackend::new(w, h)).unwrap();
            terminal
                .draw(|f| taste.render(f, f.area(), Palette::new(mono)))
                .unwrap();
            let rows = terminal
                .backend()
                .buffer()
                .content()
                .chunks(w as usize)
                .collect::<Vec<_>>();
            let next_rows = rows
                .iter()
                .filter(|row| {
                    row.iter()
                        .map(|c| c.symbol())
                        .collect::<String>()
                        .contains("Guess · Next:")
                })
                .collect::<Vec<_>>();
            assert_eq!(next_rows.len(), 1);
            assert!(
                next_rows[0][0]
                    .modifier
                    .contains(ratatui::style::Modifier::ITALIC)
            );
        }
    }
}

#[test]
fn hanging_check_words_and_final_metadata_remain_reachable_past_u16_rows() {
    let mut taste = ready();
    taste.detail = true;
    taste.evidence = true;
    taste.rows[0].items[0].state = Check::FailureRevisionDiffers;
    taste.rows[0].items[0].label = format!("{}TEST-LAST-CHECK-WORD", "word ".repeat(500_000));
    let rows = view::details(&taste.rows[0], taste.at, 80, Palette::new(true));
    assert!(rows.len() > u16::MAX as usize);
    let metadata = rows
        .iter()
        .position(|line| line.to_string().contains("Run ID:"))
        .unwrap();
    assert!(
        metadata > u16::MAX as usize,
        "identity follows the long check"
    );
    taste.key(KeyEvent::from(KeyCode::End));
    let tail = screen(&mut taste, 80, 24);
    assert!(taste.scroll > u16::MAX as usize);
    assert!(
        tail.contains("Commit:") && tail.contains("Run ID:"),
        "{tail}"
    );
}

#[test]
fn pause_and_draft_do_not_survive_selection_or_refresh() {
    let mut taste = ready();
    taste.detail = true;
    taste.key(KeyEvent::from(KeyCode::Char('l')));
    taste.key(KeyEvent::from(KeyCode::Char('n')));
    assert!(taste.pause.is_none());
    taste.key(KeyEvent::from(KeyCode::Char('l')));
    taste.refresh();
    assert!(taste.pause.is_none() && taste.take_draft().is_none());
}
