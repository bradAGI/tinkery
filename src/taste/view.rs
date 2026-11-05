//! Taste's visual hierarchy: compact result counts, a review that shows what was built and asks
//! one question, and details on request that keep every source word. Presentation never changes
//! source facts or copy payloads.

use super::{
    goal,
    next::Pause,
    read::{Check, Item, ResultRow},
    safe,
};
use crate::{Palette, sdlc::Goal};
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};
use std::time::{Duration, Instant};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Passed,
    Failed,
    Stale,
    Waiting,
    Unknown,
}

fn state(item: &Item) -> State {
    match item.state {
        Check::Pass | Check::PassRecheck => State::Passed,
        Check::Fail => State::Failed,
        Check::Stale => State::Stale,
        Check::NoneReported | Check::ReceiptsMissing => State::Unknown,
        Check::Pending
        | Check::NotSuccessful
        | Check::AwaitingHuman
        | Check::MatchingEvidenceMissing
        | Check::AttestationEvidenceMissing
        | Check::InspectExternally
        | Check::NotActivated
        | Check::RevisionUnknown
        | Check::FailureRevisionDiffers
        | Check::EvidenceUnavailable => State::Waiting,
    }
}

fn style(state: State, p: Palette) -> Style {
    match state {
        State::Failed => p.rust,
        State::Passed => p.ink,
        _ => p.muted,
    }
}

fn status(item: &Item) -> String {
    match item.state {
        Check::Pass => "✓ passed".into(),
        Check::PassRecheck => "✓ passed; may need recheck".into(),
        Check::Fail => "! failed".into(),
        Check::Stale => "… stale".into(),
        Check::NoneReported => "? unknown: no CI reported".into(),
        Check::ReceiptsMissing => format!("? {}", super::read::MISSING_RECEIPTS),
        waiting => format!(
            "… waiting: {}",
            match waiting {
                Check::AwaitingHuman
                | Check::MatchingEvidenceMissing
                | Check::AttestationEvidenceMissing => {
                    "no record found"
                }
                Check::InspectExternally => "review externally",
                Check::NotActivated => "not activated",
                Check::RevisionUnknown => "revision unknown",
                Check::FailureRevisionDiffers => "failure revision differs or is unknown",
                Check::EvidenceUnavailable => "evidence unavailable",
                _ => "not finished",
            }
        ),
    }
}

fn owed_item(row: &ResultRow, item: &Item) -> bool {
    item.kind == "human attestation"
        && row.gates.iter().any(|g| {
            g == &item.label || g.strip_prefix("may need recheck: ") == Some(item.label.as_str())
        })
}

fn preview(text: &str, width: usize) -> String {
    if text.width() <= width {
        return text.into();
    }
    let mut result = String::new();
    let mut used = 0;
    for g in text.graphemes(true) {
        if used + g.width() + 1 > width {
            break;
        }
        result.push_str(g);
        used += g.width();
    }
    if width > 0 {
        result.push('…');
    }
    result
}

/// One compact block per result, with jade selection and no check wording or technical IDs.
pub(super) fn list(
    rows: &[ResultRow],
    selected: usize,
    width: u16,
    p: Palette,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        let suffix = if row.merged {
            " · merged"
        } else if row.local().is_some() {
            " · local"
        } else if row.head.is_some() {
            " · open"
        } else if row.url.as_ref().is_err_and(|gap| gap.known_absent) {
            " · no recorded PR"
        } else {
            " · result unknown"
        };
        let goal = safe(goal(&row.goal).split('\n').next().unwrap_or(""));
        let mut available = usize::from(width).saturating_sub(2 + suffix.width());
        let mut summary = vec![Span::styled(
            if index == selected { "› " } else { "  " },
            p.jade,
        )];
        let separator = " · Goal: ";
        if let Some(title) = &row.title
            && available >= separator.width() + 2
        {
            available -= separator.width();
            // Give the title priority and reserve a third of the line for a long goal.
            let goal_reserve = goal.width().min(available.div_ceil(3));
            let title = preview(&safe(title), available - goal_reserve);
            available -= title.width();
            summary.push(Span::styled(title, p.ink.add_modifier(Modifier::BOLD)));
            summary.push(Span::styled(separator, p.muted));
        }
        summary.push(Span::styled(preview(&goal, available), p.ink));
        summary.push(Span::styled(suffix, p.muted));
        lines.push(Line::from(summary));
        let mut counts = vec![Span::raw("  ")];
        for (s, glyph, word) in [
            (State::Passed, "✓", "passed"),
            (State::Waiting, "…", "waiting"),
            (State::Stale, "…", "stale"),
            (State::Failed, "!", "failed"),
            (State::Unknown, "?", "unknown"),
        ] {
            let count = row.items.iter().filter(|item| state(item) == s).count();
            if count > 0 {
                counts.push(Span::styled(
                    format!("{glyph} {count} {word}  "),
                    style(s, p),
                ));
            }
        }
        if row.items.is_empty() {
            counts.push(Span::styled("? no checks reported  ", p.muted));
        }
        if !matches!(row.goal, Goal::Matched(_)) {
            counts.push(Span::styled("? goal unknown  ", p.muted));
        }
        if !row.gates.is_empty() {
            counts.push(Span::styled(
                format!("You owe {}  ", row.gates.len()),
                p.jade,
            ));
        }
        if row.labelled == Some(false) {
            counts.push(Span::styled("awaiting human verdict", p.jade));
        }
        lines.push(Line::from(counts));
        lines.push(Line::default());
    }
    lines
}

// Preserve every displayed grapheme, including spaces and explicit newlines. Only layout wraps.
fn chunks(text: &str, width: usize) -> Vec<String> {
    let mut rows = Vec::new();
    for logical in text.split('\n') {
        let display = safe(logical);
        let mut rest = display.as_str();
        if rest.is_empty() {
            rows.push(String::new());
        }
        while !rest.is_empty() {
            let mut end = 0;
            let mut used = 0;
            let mut space = None;
            for (at, g) in rest.grapheme_indices(true) {
                if used + g.width() > width.max(1) && end > 0 {
                    break;
                }
                end = at + g.len();
                used += g.width();
                if g.chars().all(char::is_whitespace) {
                    space = Some(end);
                }
            }
            if end < rest.len()
                && let Some(boundary) = space
            {
                end = boundary;
            }
            rows.push(rest[..end].into());
            rest = &rest[end..];
        }
    }
    rows
}

fn block(
    lines: &mut Vec<Line<'static>>,
    prefix: Span<'static>,
    text: &str,
    width: u16,
    ink: Style,
) {
    let indent = prefix.width();
    for (index, chunk) in chunks(text, usize::from(width).saturating_sub(indent))
        .into_iter()
        .enumerate()
    {
        lines.push(Line::from(vec![
            if index == 0 {
                prefix.clone()
            } else {
                Span::raw(" ".repeat(indent))
            },
            Span::styled(chunk, ink),
        ]));
    }
}

fn heading(lines: &mut Vec<Line<'static>>, name: &str, style: Style) {
    if !lines.is_empty() {
        lines.push(Line::default());
    }
    lines.push(Line::styled(
        name.to_owned(),
        style.add_modifier(Modifier::BOLD),
    ));
}

fn check(lines: &mut Vec<Line<'static>>, item: &Item, width: u16, p: Palette) {
    block(
        lines,
        Span::styled(format!("{}  ", status(item)), style(state(item), p)),
        &item.label,
        width,
        p.ink,
    );
}

/// Complete preview disclosure and lifecycle status remain scrollable, even for long shell lines.
pub(super) fn running_preview(
    row: &ResultRow,
    status: &str,
    width: u16,
    p: Palette,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if let Ok(Some(spec)) = &row.preview {
        block(
            &mut lines,
            Span::raw(""),
            &spec.disclosure(row.head.as_deref().unwrap_or("unknown")),
            width,
            p.muted,
        );
    }
    lines.push(Line::default());
    block(&mut lines, Span::raw(""), status, width, p.ink);
    lines
}

/// A snapshot this old may no longer match GitHub or the checkout.
const STALE: Duration = Duration::from_secs(10 * 60);

/// What was built: the PR title, or its link while the title is unknown.
fn built(row: &ResultRow) -> String {
    if row.local().is_some() {
        return "Local checked result".into();
    }
    safe(
        row.title
            .as_deref()
            .unwrap_or_else(|| row.url.as_deref().unwrap_or_else(|gap| gap.title)),
    )
}

fn preview_offer(row: &ResultRow) -> String {
    match row.preview() {
        Ok(Some(spec)) => spec.offer(),
        Ok(None) => super::preview::NO_DECLARATION.into(),
        Err(why) => why.into(),
    }
}

/// One source-pending human check with the preview offer, or a general verdict when none is known.
/// Other checks remain in details. Only human launches offer the verdict form.
pub(super) fn review(
    row: &ResultRow,
    pause: Option<&Pause>,
    at: Option<Instant>,
    width: u16,
    p: Palette,
    agent: bool,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let (prefix, ink) = if row.url.is_err() && row.local().is_none() {
        (Span::styled("! ", p.rust), p.rust)
    } else {
        (Span::raw(""), p.ink.add_modifier(Modifier::BOLD))
    };
    block(&mut lines, prefix, &built(row), width, ink);
    if row.merged {
        lines.push(Line::styled("merged", p.muted));
    }
    block(
        &mut lines,
        Span::raw(""),
        &preview_offer(row),
        width,
        p.muted,
    );
    if let Some(pause) = pause {
        heading(&mut lines, "Next", p.ink);
        let guess = p.ink.add_modifier(Modifier::ITALIC);
        block(
            &mut lines,
            Span::styled("  ", guess),
            &pause.next,
            width,
            guess,
        );
    }
    let mut pending = row
        .items
        .iter()
        .filter(|item| item.state == Check::AwaitingHuman && !item.label.trim().is_empty());
    if let Some(first) = pending.next() {
        heading(&mut lines, "What to judge", p.jade);
        block(&mut lines, Span::raw("  "), &first.label, width, p.ink);
        let others = pending.count();
        if others > 0 {
            lines.push(Line::styled(
                format!("  {} pending human checks · e shows all", others + 1),
                p.muted,
            ));
        }
    } else {
        heading(&mut lines, "Is this good?", p.jade);
    }
    lines.push(Line::styled(
        if agent {
            super::HUMAN_VERDICT
        } else {
            "  w  answer pass or fail, in your own words"
        },
        if agent { p.muted } else { p.jade },
    ));
    heading(&mut lines, "Your goal", p.ink);
    block(
        &mut lines,
        Span::raw("  "),
        goal(&row.goal).split('\n').next().unwrap_or(""),
        width,
        p.ink,
    );
    if !row.gaps.is_empty() || (row.url.is_ok() && row.head.is_none()) {
        lines.push(Line::default());
        for gap in &row.gaps {
            block(&mut lines, Span::styled("! ", p.rust), gap, width, p.rust);
        }
        if row.gaps.is_empty() {
            block(
                &mut lines,
                Span::styled("! ", p.rust),
                "Result revision unknown; press r.",
                width,
                p.rust,
            );
        }
    }
    if let Some(age) = at.map(|at| at.elapsed()).filter(|age| *age >= STALE) {
        lines.push(Line::default());
        lines.push(Line::styled(
            format!(
                "Read {} min ago; may be out of date, press r.",
                age.as_secs() / 60
            ),
            p.muted,
        ));
    }
    lines
}

/// Details on request: owed gates, every check in its source words, the whole goal, the evidence
/// behind each check, then result identity last. Nothing is shortened or dropped.
pub(super) fn details(
    row: &ResultRow,
    at: Option<Instant>,
    width: u16,
    p: Palette,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    heading(&mut lines, "You owe", p.jade);
    if row.receipts_missing {
        block(
            &mut lines,
            Span::raw("  "),
            super::read::MISSING_RECEIPTS,
            width,
            p.muted,
        );
    } else if row.gates.is_empty() {
        lines.push(Line::styled("  No source gate owed.", p.muted));
    }
    for gate in &row.gates {
        block(&mut lines, Span::raw("  "), gate, width, p.jade);
    }
    heading(&mut lines, "Checks", p.ink);
    for item in row.items.iter().filter(|item| !owed_item(row, item)) {
        check(&mut lines, item, width, p);
    }
    heading(&mut lines, "Goal", p.ink);
    block(&mut lines, Span::raw("  "), goal(&row.goal), width, p.ink);
    heading(&mut lines, "Evidence", p.ink);
    for item in &row.items {
        lines.push(Line::default());
        check(&mut lines, item, width, p);
        block(
            &mut lines,
            Span::raw("  "),
            &format!("Source: {}\nReported state: {}", item.kind, item.state),
            width,
            p.muted,
        );
        let receipt = if item.kind == "automated" {
            item.evidence
                .replace("; argv:", ";\nargv:")
                .replace("; cwd:", ";\ncwd:")
                .replace("; receipt log SHA-256:", ";\nreceipt log SHA-256:")
        } else {
            item.evidence.clone()
        };
        block(&mut lines, Span::raw("  "), &receipt, width, p.ink);
    }
    if !row.gaps.is_empty() {
        heading(&mut lines, "Source notes", p.ink);
        for gap in &row.gaps {
            lines.push(Line::default());
            block(&mut lines, Span::raw("  "), gap, width, p.ink);
        }
    }
    heading(&mut lines, "Result", p.ink);
    let mut result = vec![
        row.local().map_or_else(
            || {
                format!(
                    "PR: {}{}",
                    row.url.as_deref().unwrap_or_else(|gap| gap.title),
                    if row.merged { " · merged" } else { "" }
                )
            },
            |local| format!("Local checkout: {}", local.checkout().display()),
        ),
        format!("Commit: {}", row.head.as_deref().unwrap_or("unknown")),
        format!("Run ID: {}", row.id),
        format!("Run: {}", row.slug),
        format!("Seed digest: {}", row.seed),
        format!(
            "Read {} min ago (snapshot)",
            at.map_or(0, |at| at.elapsed().as_secs() / 60)
        ),
    ];
    result.push(preview_offer(row));
    for text in result {
        block(&mut lines, Span::raw("  "), &text, width, p.muted);
    }
    lines
}

/// The unsent label: what was built, pass or fail with when to pick each, the words as typed,
/// and the name it will be signed with.
pub(super) fn label_form(
    row: &ResultRow,
    form: &super::label::Form,
    signer: Option<&Result<String, String>>,
    note: &str,
    width: u16,
    p: Palette,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    heading(&mut lines, "Is this good?", p.ink);
    block(&mut lines, Span::raw("  "), &built(row), width, p.muted);
    // tink-sdlc's answer sits above the form so a refusal is never scrolled out of sight.
    if !note.is_empty() {
        lines.push(Line::default());
        block(&mut lines, Span::raw("  "), &safe(note), width, p.ink);
    }
    lines.push(Line::default());
    for (number, kind) in super::label::Kind::ALL.iter().enumerate() {
        let chosen = form.kind == Some(*kind);
        block(
            &mut lines,
            Span::styled(
                format!(
                    "{} {} {:<6}",
                    if chosen { "  ›" } else { "   " },
                    number + 1,
                    kind.name()
                ),
                if chosen { p.jade } else { p.ink },
            ),
            kind.when(),
            width,
            if chosen { p.jade } else { p.muted },
        );
    }
    heading(&mut lines, "Your words", p.ink);
    block(
        &mut lines,
        Span::raw("  "),
        "What makes it good, or what is wrong with it?",
        width,
        p.muted,
    );
    if form.kind.is_some() {
        block(
            &mut lines,
            Span::raw("  "),
            &format!("{}▏", safe(&form.words)),
            width,
            p.ink,
        );
    }
    lines.push(Line::default());
    let signed = match signer {
        None => "Checking who signs this label…".to_owned(),
        Some(Ok(name)) => format!("Signed as {}.", safe(name)),
        Some(Err(why)) => safe(why),
    };
    block(
        &mut lines,
        Span::raw("  "),
        &format!(
            "{signed} Your label is your verdict on this result; it approves and merges nothing."
        ),
        width,
        p.muted,
    );
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row() -> ResultRow {
        let human =
            "Confirm the synthetic assumptions remain distinguishable from real observations.";
        ResultRow {
            id: "TEST-run-id".into(), slug: "TEST-run".into(),
            goal: Goal::Matched(format!("{}\nWhy: TEST reason.\nDone when: TEST result.", "TEST café 👩‍💻 goal ".repeat(12))),
            url: Ok("https://github.com/TEST/example/pull/1".into()), title: None, head: Some("a".repeat(40)),
            merged: true, local: Ok(None), preview: Err("No preview declared".into()), receipts_missing: false, seed: "TEST-seed".into(), gates: vec![human.into()], gaps: vec![], labelled: None,
            items: vec![
                Item { label: "Every original check word stays intact while this long sentence wraps with a hanging indent and no abbreviation.".into(),
                    state: Check::Fail, kind: "automated".into(), evidence: "/TEST/log; argv: [TEST]".into() },
                Item { label: human.into(), state: Check::AwaitingHuman,
                    kind: "human attestation".into(), evidence: "No operator evidence recorded".into() },
            ],
        }
    }

    #[test]
    fn long_preview_disclosure_and_failure_are_complete_wrapped_rows() {
        let mut row = row();
        row.preview = Ok(serde_json::from_value(serde_json::json!({
            "command": "echo TEST; ".repeat(300),
            "url": "http://127.0.0.1:5173/"
        }))
        .unwrap());
        let status = "Preview stopped: TEST failure. Esc returns; v retries.";
        for width in [76, 96] {
            let lines = running_preview(&row, status, width, Palette::new(true));
            assert!(lines.iter().all(|line| line.width() <= width as usize));
            let complete = lines.iter().map(Line::to_string).collect::<String>();
            let expected = row
                .preview()
                .unwrap()
                .unwrap()
                .disclosure(row.head.as_deref().unwrap())
                .replace('\n', "");
            assert_eq!(complete, format!("{expected}{status}"));
        }
    }

    #[test]
    fn untitled_list_result_uses_the_goal_and_counts() {
        for width in [76, 96] {
            let row = row();
            let lines = list(std::slice::from_ref(&row), 0, width, Palette::new(true));
            assert_eq!(lines.len(), 3);
            assert!(lines[0].to_string().contains('…'));
            assert!(lines[0].to_string().ends_with(" · merged"));
            assert!(lines[0].width() <= width as usize);
            let text = lines
                .iter()
                .map(Line::to_string)
                .collect::<Vec<_>>()
                .join("\n");
            assert!(
                text.contains("! 1 failed")
                    && text.contains("… 1 waiting")
                    && text.contains("You owe 1")
            );
            assert!(!text.contains(&row.items[0].label) && !text.contains("Confirm the synthetic"));
        }
    }

    #[test]
    fn long_titles_and_goals_share_one_line_and_keep_whole_graphemes() {
        let mut row = row();
        let title = "研究👩‍💻e\u{301}".repeat(30);
        row.title = Some(title.clone());
        for width in [76, 116] {
            let lines = list(std::slice::from_ref(&row), 0, width, Palette::new(true));
            let line = lines[0].to_string();
            let (shown, goal) = line
                .strip_prefix("› ")
                .unwrap()
                .split_once("… · Goal: ")
                .unwrap();
            assert_eq!(
                title
                    .graphemes(true)
                    .take(shown.graphemes(true).count())
                    .collect::<String>(),
                shown
            );
            assert!(
                goal.starts_with("TEST café") && goal.ends_with("… · merged"),
                "{line}"
            );
            assert!(lines[0].width() <= usize::from(width), "{line}");
        }
    }

    #[test]
    fn headings_palette_and_exact_owed_deduplication_do_not_change_sources() {
        let row = row();
        for mono in [false, true] {
            let p = Palette::new(mono);
            let lines = details(&row, None, 96, p);
            let full = lines
                .iter()
                .map(Line::to_string)
                .collect::<Vec<_>>()
                .join("\n");
            let order = ["You owe", "Checks", "\nGoal", "\nEvidence", "\nResult"]
                .map(|heading| full.find(heading).unwrap());
            assert!(order.is_sorted(), "{full}");
            let text = &full[..order[3]];
            assert_eq!(text.matches("Confirm the synthetic").count(), 1);
            assert!(
                !text.contains("(automated)")
                    && !text.contains("argv:")
                    && !text.contains("/TEST/log")
            );
            let failed = lines
                .iter()
                .find(|line| line.to_string().starts_with("! failed"))
                .unwrap();
            assert_eq!(failed.spans[0].style, p.rust);
            assert_eq!(failed.spans[1].style, p.ink);
            let owed = lines
                .iter()
                .find(|line| line.to_string().starts_with("  Confirm the synthetic"))
                .unwrap();
            assert_eq!(owed.spans[1].style, p.jade);
            let failed_row = lines
                .iter()
                .position(|line| line.to_string().starts_with("! failed"))
                .unwrap();
            let owed_row = lines
                .iter()
                .position(|line| line.to_string().starts_with("  Confirm the synthetic"))
                .unwrap();
            let mut terminal =
                ratatui::Terminal::new(ratatui::backend::TestBackend::new(96, 40)).unwrap();
            terminal
                .draw(|frame| {
                    frame.render_widget(
                        ratatui::widgets::Paragraph::new(lines.clone()).style(p.ink),
                        frame.area(),
                    )
                })
                .unwrap();
            let cells = terminal.backend().buffer();
            assert_eq!(
                cells[(0, failed_row as u16)].fg,
                p.rust.fg.unwrap_or(ratatui::style::Color::Reset)
            );
            assert!(
                cells[(0, failed_row as u16)]
                    .modifier
                    .contains(Modifier::BOLD)
            );
            if mono {
                assert!(
                    cells[(0, failed_row as u16)]
                        .modifier
                        .contains(Modifier::UNDERLINED)
                );
            }
            assert_eq!(
                cells[(2, owed_row as u16)].fg,
                p.jade.fg.unwrap_or(ratatui::style::Color::Reset)
            );
            let evidence = details(&row, None, 96, p)
                .iter()
                .map(Line::to_string)
                .collect::<Vec<_>>()
                .join("\n");
            assert!(
                evidence.contains("/TEST/log")
                    && evidence.contains("argv:")
                    && evidence.contains("human attestation")
            );
        }
    }

    #[test]
    fn review_shows_what_was_built_the_goal_line_and_one_question_in_order() {
        let mut row = row();
        row.title = Some("TEST clinic comparison".into());
        for width in [76, 96] {
            let text = review(&row, None, None, width, Palette::new(true), false)
                .iter()
                .map(Line::to_string)
                .collect::<Vec<_>>()
                .join("\n");
            let order = [
                "TEST clinic comparison",
                "No preview declared",
                "What to judge",
                "Confirm the synthetic",
                "Your goal",
            ]
            .map(|part| text.find(part).unwrap());
            assert!(order.is_sorted(), "{text}");
            assert!(text.lines().count() <= 20, "fits the first screen: {text}");
            for hidden in [
                "Why: TEST reason",
                "Done when",
                "Every original check",
                "TEST-run-id",
                "TEST-seed",
                "https://",
            ] {
                assert!(!text.contains(hidden), "{hidden}: {text}");
            }
        }
        let old = Instant::now().checked_sub(STALE).unwrap();
        let stale = review(&row, None, Some(old), 96, Palette::new(true), false)
            .iter()
            .map(Line::to_string)
            .collect::<String>();
        assert!(stale.contains("may be out of date, press r."), "{stale}");
        let fresh = review(
            &row,
            None,
            Some(Instant::now()),
            96,
            Palette::new(true),
            false,
        )
        .iter()
        .map(Line::to_string)
        .collect::<String>();
        assert!(!fresh.contains("min ago"), "{fresh}");
        row.title = None;
        let untitled = review(&row, None, None, 96, Palette::new(true), false)[0].to_string();
        assert_eq!(untitled, "https://github.com/TEST/example/pull/1");
        row.head = None;
        let unknown = review(&row, None, None, 96, Palette::new(true), false)
            .iter()
            .map(Line::to_string)
            .collect::<String>();
        assert!(
            unknown.contains("! Result revision unknown; press r."),
            "{unknown}"
        );
    }

    #[test]
    fn label_form_explains_pass_and_fail_and_names_the_signer() {
        let row = row();
        let mut form = super::super::label::Form::default();
        let text = |form: &super::super::label::Form, signer| {
            label_form(&row, form, signer, "", 76, Palette::new(true))
                .iter()
                .map(Line::to_string)
                .collect::<Vec<_>>()
                .join("\n")
        };
        let checking = text(&form, None);
        for visible in [
            "1 pass  It does what you asked, and you'd keep it.",
            "2 fail  Something is wrong or missing.",
            "What makes it good, or what is wrong with it?",
            "Checking who signs this label…",
        ] {
            assert!(checking.contains(visible), "{visible}: {checking}");
        }
        assert!(!checking.contains("tink-sdlc") && !checking.contains("correction"));
        form.kind = Some(super::super::label::Kind::Fail);
        let signed = text(&form, Some(&Ok("TEST operator".into())));
        assert!(signed.contains("› 2 fail") && signed.contains("Signed as TEST operator."));
    }

    #[test]
    fn check_words_are_never_shortened_and_hanging_rows_rejoin_exactly() {
        let text = "Keep every word,  including repeated spaces and café 👩‍💻, while a long condition wraps without ellipsis.";
        for width in [76, 96] {
            let item = Item {
                label: text.into(),
                state: Check::MatchingEvidenceMissing,
                kind: "automated".into(),
                evidence: String::new(),
            };
            let mut lines = Vec::new();
            check(&mut lines, &item, width, Palette::new(true));
            let joined = lines
                .iter()
                .map(|line| line.spans[1].content.as_ref())
                .collect::<String>();
            assert_eq!(joined, text);
            assert!(lines.len() > 1);
            let indent = lines[0].spans[0].width();
            assert!(
                lines[1..]
                    .iter()
                    .all(|line| line.spans[0].width() == indent)
            );
        }
    }
}
