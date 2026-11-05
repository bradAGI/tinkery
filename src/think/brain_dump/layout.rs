//! Where things go on think's screen, and the words around them: the panes, header, help and
//! "how it works", the key bar, the leave prompt and the status notices. It draws what the
//! conversation and screen state say; it decides nothing about the goal.

use super::*;
use ratatui::style::Modifier;

/// The refused edit. Short enough for one row of the narrowest (80-column) right-hand pane.
pub(super) const LIMIT_TEXT: &str = "Edit not accepted; words unchanged.";
/// Staging refuses a skipped scope question before sending or committing words.
pub(super) const SCOPE_REFUSAL: &str =
    "Answer the skipped scope question first: F1 brings it back.";
/// Empty sends need one next step, without a permanent warning on the blank screen.
pub(super) const EMPTY_SEND_TEXT: &str = "Write something, then F2 sends it.";
/// A review invitation, not a claim that the reading captured every requirement.
const READY_FOR_REVIEW: &str = "Ready for your review.";
/// Notices that report a finished action and stay on the status row until replaced. Other
/// notices that are not refusals live only under How it works, Last result.
const RECEIPTS: [&str; 5] = [
    "Copy",
    "Clipboard",
    "Question skipped",
    "Skip undone",
    "Added words",
];
/// Why practice, or real mode without a sessions folder, can't review or save a goal.
pub(super) const SAVE_NEEDS_REAL: &str = "Saving a goal needs real mode and a sessions folder.";
/// The status while practice prepares its simulated reading: no model runs.
pub(super) const PRACTICE_STATUS: &str = "Simulating… (practice: no model)";
/// Rows a refusal may take: every fixed refusal fits in two at the narrowest pane.
pub(super) const STATUS_ROWS: u16 = 2;

/// What a send in flight is doing. Practice says it simulates, so it never reads as a model call.
fn thinking_status(app: &BrainDump) -> &'static str {
    if !app.real {
        return PRACTICE_STATUS;
    }
    match app.host.call_in_flight() {
        Some(Request::Shaping) => "Thinking… (run 1 of up to 2)",
        Some(Request::DecisionReview) => "Reviewing decisions… (run 2 of up to 2)",
        _ => "Thinking…",
    }
}

/// The one status line under the writing box: why the last action did nothing, else whether
/// a reading is coming, else what the last action did. A refusal always wins, so a blocked
/// key is never silent.
fn status(app: &BrainDump) -> Option<&str> {
    let board = app.conversation.board().is_some();
    if app.screen.refused.is_some() {
        Some(&app.notice)
    } else if app.running() && board {
        Some(thinking_status(app))
    } else if RECEIPTS.iter().any(|p| app.notice.starts_with(p)) {
        Some(&app.notice)
    } else if !app.running()
        && !app.conversation.caught_up()
        && board
        && app.conversation.scope_pending().is_none()
    {
        Some(if app.shaping.ready.is_some() {
            "Reply ready."
        } else {
            "Reading couldn't be updated."
        })
    } else {
        None
    }
}

pub(super) fn render(frame: &mut Frame, app: &mut BrainDump, palette: Palette) {
    let area = frame.area();
    app.screen.area = area;
    frame.render_widget(Block::default().style(palette.ink), area);
    if app.below_minimum() {
        frame.render_widget(
            Paragraph::new(if app.screen.help {
                "Help\nF1 or Esc returns. Esc again goes back to the menu.\nResize to 80×24. Words retained."
            } else {
                "Need 80×24. Words retained."
            })
            .style(palette.ink),
            Rect::new(0, 3, area.width, area.height.saturating_sub(3)).intersection(area),
        );
        return;
    }
    app.screen.goal_button = Rect::default();
    app.screen.back_button = Rect::default();
    if app.conversation.sources().is_empty() {
        // Before the first send, help sits beside the draft; the draft never leaves view.
        let width = if app.screen.help {
            area.width * 48 / 100 - 4
        } else {
            area.width - 8
        };
        frame.render_widget(
            Paragraph::new(if app.conversation.has_draft_suggestion() {
                "Tinkery's guess · edit freely · F2 sends"
            } else {
                "What's on your mind?"
            })
            .style(palette.muted),
            Rect::new(4, 5, width, 1),
        );
        app.screen.input_area = Rect::new(4, 8, width, area.height - 12);
        render_input(frame, app, palette);
        if app.screen.refused.is_some() {
            // Show only a refusal, never the initial status as permanent clutter.
            frame.render_widget(
                Paragraph::new(app.notice.as_str())
                    .style(palette.muted)
                    .wrap(Wrap { trim: false }),
                Rect::new(4, area.height - 4, width, 2),
            );
        }
        return;
    }
    let left = Rect::new(2, 4, area.width * 48 / 100 - 4, area.height - 6);
    let right = Rect::new(
        area.width * 48 / 100 + 2,
        4,
        area.width - area.width * 48 / 100 - 4,
        area.height - 6,
    );
    app.screen.agent_area = right;
    app.screen.canvas.area = left;
    if app.screen.show_extractions {
        draw_fragments(frame, app, left, palette);
    } else {
        if let Some(label) = app.source_label() {
            frame.render_widget(
                Paragraph::new(label).style(palette.muted),
                Rect::new(left.x, 3, left.width, 1),
            );
        }
        intact_view::render_source(frame, app, left, palette);
    }
    if app.screen.original || app.screen.details {
        let text = if app.screen.original {
            app.originals()
        } else {
            format!(
                "Why\nh How it works\n\n{}\n\n{}\n\n{}",
                app.details_text(),
                app.notice,
                checks::why(&app.diagnostics())
            )
        };
        let text = ratatui::text::Text::styled(Note::display(&text), palette.ink);
        let p = Paragraph::new(text)
            .style(palette.ink)
            .wrap(Wrap { trim: false });
        let max = p
            .line_count(right.width)
            .saturating_sub(right.height as usize) as u16;
        app.screen.original_scroll = app.screen.original_scroll.min(max);
        frame.render_widget(p.scroll((app.screen.original_scroll, 0)), right);
        return;
    }
    if app.handoff.review().is_some() || app.handoff.receipt().is_some() {
        goal_view::render_goal(frame, app, palette);
        return;
    }
    let question = match app.conversation.focused_question() {
        Some(q) => {
            let mut lines = vec![
                ratatui::text::Line::styled("One thing I'm not sure about", palette.muted),
                ratatui::text::Line::styled(q.text.clone(), palette.jade),
            ];
            for line in suggestion::description(q).lines() {
                lines.push(ratatui::text::Line::styled(
                    line.to_owned(),
                    palette.muted.add_modifier(ratatui::style::Modifier::ITALIC),
                ));
            }
            ratatui::text::Text::from(lines)
        }
        None => match (open_intent(app), app.conversation.board()) {
            (Some(line), Some(g)) if !app.running() => ratatui::text::Text::styled(
                if line == READY_FOR_REVIEW && app.can_review() {
                    format!("{line} F1, then 1: Review the goal.")
                } else if g.goal().is_none() || line == READY_FOR_REVIEW {
                    line
                } else {
                    format!("{line} Review when you're ready.")
                },
                palette.muted,
            ),
            _ => ratatui::text::Text::default(),
        },
    };
    let question = Paragraph::new(question).wrap(Wrap { trim: false });
    let question_lines = question.line_count(right.width);
    // Keep ordinary questions in view; larger ones share the reading's scroll route.
    let question_height = question_lines.clamp(6, usize::from(right.height - 11)) as u16;
    let rows = Layout::vertical([
        Constraint::Min(5),
        Constraint::Length(question_height),
        Constraint::Length(4),
        Constraint::Length(2),
    ])
    .split(right);
    app.screen.agent_area = rows[0].union(rows[1]);
    let failure = app.conversation.last_failure().filter(|_| !app.running());
    let content = if let Some(error) = failure {
        let retained = app
            .conversation
            .board()
            .and_then(|g| g.goal())
            .map(agent_text)
            .unwrap_or_default();
        let left = app
            .host
            .calls_left()
            .map_or_else(String::new, |left| format!(" {left}"));
        format!(
            "Reading failed: {error}{left}\n\nOriginals retained. Retry, inspect Why, or add clarification.\n\n{retained}"
        )
    } else if app.conversation.board().is_some() {
        String::new()
    } else if app.running() {
        thinking_status(app).into()
    } else if app.shaping.ready.is_some() {
        "Reply ready.".into()
    } else if !app.conversation.caught_up() {
        "I couldn't form a faithful reading.".into()
    } else {
        String::new()
    };
    let mut text = ratatui::text::Text::from(content);
    if let (Some(g), None) = (app.conversation.board(), failure) {
        // One voice, one reading: "I think you mean…".
        let mut lines = vec![];
        lines.push(ratatui::text::Line::styled(
            "I think you mean…",
            palette.muted,
        ));
        for line in g.goal().unwrap_or("").lines() {
            lines.push(ratatui::text::Line::styled(
                line.to_owned(),
                app.reading_line_style(line, palette)
                    .add_modifier(Modifier::ITALIC),
            ));
        }
        if !g.other_goals.is_empty() {
            lines.push(ratatui::text::Line::styled(
                format!("also in your dump: {}", g.other_goals.join("; ")),
                palette.muted,
            ));
        }
        if !g.deferred.is_empty() {
            lines.push(ratatui::text::Line::styled(
                format!("for the seed: {}", g.deferred.join("; ")),
                palette.muted,
            ));
        }
        text = ratatui::text::Text::from(lines);
    }
    if failure.is_some() {
        let retained_lines = app
            .conversation
            .board()
            .and_then(|g| g.goal())
            .map_or(0, |goal| agent_text(goal).lines().count());
        let known = text.lines.len().saturating_sub(retained_lines);
        for line in text.lines.iter_mut().take(known) {
            line.style = palette.ink.remove_modifier(Modifier::ITALIC);
        }
    }
    let p = Paragraph::new(text)
        .style(palette.ink.add_modifier(Modifier::ITALIC))
        .wrap(Wrap { trim: false });
    let mut reading = rows[0];
    let line_count = p.line_count(reading.width);
    if line_count > usize::from(reading.height) {
        reading.height -= 1;
        frame.render_widget(
            Paragraph::new("PgUp/PgDn scroll reading").style(palette.muted),
            Rect::new(reading.x, reading.bottom(), reading.width, 1),
        );
    }
    let mut question_area = rows[1];
    if question_lines > usize::from(question_area.height) {
        question_area.height -= 1;
        frame.render_widget(
            Paragraph::new("PgUp/PgDn scroll question").style(palette.muted),
            Rect::new(
                question_area.x,
                question_area.bottom(),
                question_area.width,
                1,
            ),
        );
    }
    let reading_max = line_count.saturating_sub(usize::from(reading.height));
    let question_max = question_lines.saturating_sub(usize::from(question_area.height));
    let max = reading_max.max(question_max).min(usize::from(u16::MAX)) as u16;
    app.screen.paper_scroll = app.screen.paper_scroll.min(max);
    frame.render_widget(
        p.scroll((
            usize::from(app.screen.paper_scroll).min(reading_max) as u16,
            0,
        )),
        reading,
    );
    // Inspect the clipped output, excluding the heading when it is in the viewport.
    let heading = u16::from(usize::from(app.screen.paper_scroll).min(reading_max) == 0);
    let content = Rect::new(
        reading.x,
        reading.y + heading,
        reading.width,
        reading.height.saturating_sub(heading),
    );
    app.screen.reading_visible =
        failure.is_none() && app.conversation.board().is_some() && visible_content(frame, content);
    frame.render_widget(
        question.scroll((
            usize::from(app.screen.paper_scroll).min(question_max) as u16,
            0,
        )),
        question_area,
    );
    let input = Block::default()
        .title(if app.add_more {
            "add more"
        } else if app.conversation.focused_question().is_none()
            && app.conversation.board().is_some()
        {
            "anything to add or change?"
        } else {
            "your answer"
        })
        .style(palette.ink);
    app.screen.input_area = input.inner(rows[2]);
    frame.render_widget(input, rows[2]);
    render_input(frame, app, palette);
    // A one-row status keeps its old row; a longer refusal takes both rows of the status area.
    let encoded_bytes = app.encoded_bytes();
    let status = status(app).map(|text| {
        let p = Paragraph::new(text)
            .style(palette.muted)
            .wrap(Wrap { trim: false });
        let lines = (p.line_count(rows[3].width) as u16).clamp(1, STATUS_ROWS);
        (p, lines)
    });
    if let Some(bytes) = encoded_bytes
        .filter(|b| *b >= 28 * 1024 && status.as_ref().is_none_or(|(_, n)| *n < STATUS_ROWS))
    {
        let offset = if app.screen.goal_button.width > 0 {
            14
        } else {
            0
        };
        frame.render_widget(
            Paragraph::new(format!("{bytes} / {REQUEST_LIMIT} bytes")).style(palette.muted),
            Rect::new(
                rows[3].x + offset,
                rows[3].y,
                rows[3].width.saturating_sub(offset),
                1,
            ),
        );
    }
    if let Some((p, lines)) = status {
        frame.render_widget(
            p,
            Rect::new(rows[3].x, rows[3].bottom() - lines, rows[3].width, lines),
        );
    }
}
/// With no question in focus: no goal yet, readiness, or intent still open for the seed.
/// Only goal content decides, including submitted words not yet applied; transient
/// states such as voice or unsent text do not.
/// Review lists everything still open; this names one thing.
pub(super) fn open_intent(app: &BrainDump) -> Option<String> {
    let g = app.conversation.board()?;
    Some(if g.goal().is_none() {
        "No goal yet.".into()
    } else if let Some(part) = g.parts.missing().next() {
        format!("Still missing: {}.", part.label())
    } else if app.conversation.open_parts().is_empty()
        && app.conversation.caught_up()
        && app.shaping.ready.is_none()
        && app.conversation.skipped().iter().all(|q| {
            app.conversation
                .settled()
                .iter()
                .any(|s| same_question(&s.question, q))
        })
        && g.unresolved_notes.is_empty()
        && g.misfits.is_empty()
        && app.conversation.scope_pending().is_none()
    {
        READY_FOR_REVIEW.into()
    } else {
        "Some parts remain open for the seed.".into()
    })
}
pub(super) fn help_text(app: &BrainDump) -> String {
    if app.handoff.receipt().is_some() {
        return "Right now\nYour goal is saved. Next: shape it into a seed in Shape.\ns open in tend · Esc back to the menu\n\nF1 close".into();
    }
    let open_now;
    let now = if matches!(
        app.screen.refused,
        Some(Refusal::DraftTooLong | Refusal::EarlierWordsTooLong)
    ) {
        if app.screen.refused == Some(Refusal::EarlierWordsTooLong) {
            concat!(
                "Earlier words are too long to send. Nothing was sent; all words stay here. Shortening new words cannot help.\n",
                "Ctrl-A then Ctrl-C copies local words. Tab selects originals; Ctrl-PgUp/PgDn switches them. ",
                "Ctrl-A then Ctrl-C copies each to take elsewhere. Copying starts nothing."
            )
        } else {
            concat!(
                "Nothing was sent; all words stay here.\n",
                "Ctrl-A then Ctrl-C copies the full draft. Deliberately replace it with fewer words, then F2 sends once. ",
                "The model may still refuse."
            )
        }
    } else if app.handoff.saving() {
        "Saving your goal. Wait a moment."
    } else if app
        .handoff
        .review()
        .is_some_and(|review| review.scroll < review.max)
    {
        "Esc closes help; then PgDn reads the rest.\nNothing is saved yet."
    } else if app.handoff.review().is_some() && app.records() {
        "Type confirm, then Enter, to save this goal and its think record.\nEsc goes back without saving."
    } else if app.handoff.review().is_some() {
        "Type confirm, then Enter, to save this goal.\nEsc goes back without saving."
    } else if app.running() {
        "Reading your words. Keep writing if you like.\nEsc cancels."
    } else if app.conversation.sources().is_empty() {
        "Type, then F2 sends it."
    } else if app.shaping.ready.is_some() {
        "A new reading is ready.\nF2 when you want it."
    } else if app
        .conversation
        .scope_pending()
        .is_some_and(|(_, q)| app.conversation.available(q))
    {
        "Answer the question about your added words.\nF2 sends."
    } else if app.conversation.last_failure().is_some() {
        "The reading failed; your words are safe.\nF2 tries again."
    } else if app.add_more {
        "Type a new dump.\nF2 sends it."
    } else if app.conversation.board().is_none()
        && app.conversation.applied() < app.conversation.sources().len()
    {
        "Your words haven't been read yet.\nF2 tries again."
    } else if app.conversation.focused_question().is_none() {
        let composed = app.conversation.board().is_some_and(|g| g.goal().is_some());
        open_now = format!(
            "{}\n{}",
            open_intent(app).unwrap_or_else(|| "No question left.".into()),
            if app.can_review() {
                "1: Review the goal."
            } else if composed && !app.can_save() {
                SAVE_NEEDS_REAL
            } else {
                "You can add words, then F2 sends."
            }
        );
        open_now.as_str()
    } else {
        "Type your answer.\nF2 sends."
    };
    let left = app.host.calls_left().map_or_else(String::new, |left| {
        format!("\n{left}\nOne send may use two drafting runs: reading and decision review.")
    });
    let voice = if app.voice_available() {
        "\nF6 or Ctrl+Alt+Z (⌃⌥Z) speaks; your voice stays on this machine."
    } else {
        ""
    };
    // The numbered actions come first, so even a short panel shows them without scrolling.
    let rows = app.screen.actions.text(&app.rows());
    let rows = if rows.is_empty() {
        rows
    } else {
        format!("{rows}\n\n")
    };
    format!("Right now\n{rows}{now}{left}{voice}\n\nF1 close")
}
pub(super) fn how_text(app: &BrainDump) -> String {
    if let Some(receipt) = app.handoff.receipt() {
        return format!(
            "How it works\nh or Esc returns to Help · PgUp/PgDn scroll\n\n{}{}\n\nCopy puts the prompt on your clipboard; your terminal may ask permission. Paste it into Claude, Codex or Pi yourself. Nothing starts on its own.",
            receipt.text(),
            app.record_line()
                .map_or(String::new(), |line| format!("\n{line}"))
        );
    }
    let diagnostics = app.diagnostics().join("\n\n");
    let saving = "Only typing confirm saves. It keeps your goal and a record of this conversation, and starts no work. Leave without confirming and nothing is kept.";
    format!(
        "How it works\nh or Esc returns to Help · PgUp/PgDn scroll\n\nSending\n{}\nNothing runs on its own and nothing retries. Ctrl-C cancels a send.\n\nSaving\n{saving}\n\nAfter saving\nOpen Shape from Tend.\n\nWords\nItalic is my guess at what you meant; upright is exactly what you typed.\n\nKeys\nF2 sends. F1 lists everything else you can do now; press its number. Esc goes back, or leaves and asks first if words would be lost. Enter starts a new line. Tab switches between writing and your original. Shift-arrows or drag select, Ctrl-A selects all, Ctrl-C copies, Ctrl-E extracts a selection to a card. PgUp/PgDn scroll the reading; Ctrl-PgUp/PgDn switch originals. b in help hides the key bar. Ctrl-T switches dark or light. Ctrl-L repaints. Mac F-keys may need fn.{}\n\nLast result\n{}",
        if app.real {
            "Real: your words go to the model you chose and may cost money."
        } else {
            "Practice: replies are simulated and nothing leaves your machine."
        },
        if diagnostics.is_empty() {
            String::new()
        } else {
            format!("\n\n{diagnostics}")
        },
        app.notice
    )
}
/// Only what's possible right now (C1): key in ink, word muted.
pub(super) fn key_bar_items(app: &BrainDump) -> Vec<(&'static str, &'static str)> {
    if app.screen.leave_prompt || app.handoff.saving() {
        vec![]
    } else if app.handoff.receipt().is_some() {
        vec![("s", "open in tend"), ("esc", "menu")]
    } else if let Some(review) = app.handoff.review() {
        if review.scroll < review.max {
            return vec![("PgDn", "read to end"), ("esc", "back")];
        }
        let save = if app.records() {
            "save goal and record"
        } else {
            "save"
        };
        vec![("type confirm", save), ("esc", "back")]
    } else if app.voice_active() {
        vec![
            (crate::voice::key_label(std::env::consts::OS), "stop"),
            ("esc", "cancel"),
        ]
    } else {
        let mut items = if app.running() {
            vec![("esc", "cancel")]
        } else {
            vec![("F2", "send")]
        };
        if !app.rows().is_empty() {
            items.push(("F1", "more"));
        }
        if app.voice_available() {
            items.push((crate::voice::key_label(std::env::consts::OS), "speak"));
        }
        if !app.running() {
            items.push(("esc", "menu"));
        }
        items
    }
}
pub(super) fn render_key_bar(frame: &mut Frame, app: &BrainDump, palette: Palette) {
    let area = frame.area();
    if app.screen.help
        || (!app.screen.key_bar && app.handoff.receipt().is_none())
        || area.width < MIN_WIDTH
        || area.height < MIN_HEIGHT
    {
        return;
    }
    let mut spans = vec![];
    for (i, (key, label)) in key_bar_items(app).into_iter().enumerate() {
        if i > 0 {
            spans.push(ratatui::text::Span::raw("   "));
        }
        spans.push(ratatui::text::Span::styled(key, palette.ink));
        spans.push(ratatui::text::Span::styled(
            format!(" {label}"),
            palette.muted,
        ));
    }
    frame.render_widget(
        Paragraph::new(ratatui::text::Line::from(spans)).style(palette.ink),
        Rect::new(2, area.height - 1, area.width - 4, 1),
    );
}
pub(super) fn render_header(frame: &mut Frame, palette: Palette) {
    let area = frame.area();
    frame.render_widget(
        Paragraph::new("tinkery").style(palette.muted),
        Rect::new(2, 1, 7, 1).intersection(area),
    );
    crate::render_theme_switch(frame, palette);
    frame.render_widget(
        Paragraph::new("?").style(palette.muted),
        Rect::new(area.width.saturating_sub(3), 1, 1, 1).intersection(area),
    );
}
pub(super) fn render_leave_prompt(frame: &mut Frame, app: &BrainDump, palette: Palette) {
    if !app.screen.leave_prompt {
        return;
    }
    let area = frame.area();
    let rect = Rect::new(
        2,
        area.height.saturating_sub(2),
        area.width.saturating_sub(4),
        1,
    )
    .intersection(area);
    frame.render_widget(Clear, rect);
    frame.render_widget(
        Paragraph::new(ratatui::text::Line::from(vec![
            ratatui::text::Span::styled("! ", palette.rust),
            ratatui::text::Span::styled("Back to menu and lose this draft? y / n", palette.ink),
        ]))
        .style(palette.ink),
        rect,
    );
}

// Qualify the actual viewport, not offscreen text or the reading's heading.
pub(super) fn visible_content(frame: &mut Frame, area: Rect) -> bool {
    let area = area.intersection(frame.area());
    let buffer = frame.buffer_mut();
    (area.y..area.bottom())
        .any(|y| (area.x..area.right()).any(|x| !buffer[(x, y)].symbol().trim().is_empty()))
}

#[cfg(test)]
mod presentation_viewport_tests {
    use super::*;
    #[test]
    fn blank_scrolled_viewport_does_not_qualify_offscreen_reading_text() {
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(40, 4)).unwrap();
        for (scroll, expected) in [(0, true), (1, false), (5, true)] {
            terminal
                .draw(|frame| {
                    let area = frame.area();
                    frame.render_widget(
                        Paragraph::new("TEST first\n\n\n\n\nTEST last").scroll((scroll, 0)),
                        area,
                    );
                    assert_eq!(visible_content(frame, area), expected);
                })
                .unwrap();
        }
    }
}
