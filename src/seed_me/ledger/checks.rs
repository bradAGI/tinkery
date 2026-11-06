//! Acceptance records own their rendered section; commands remain inert data, never executions.

use super::{
    Result, Value,
    python::{text, whitespace},
    require,
};
use regex::Regex;
use std::path::Path;

pub(super) const FILE: &str = "seed-checks.json";

/// Refuse unknown fields, blank values and non-relative working directories without coercion.
pub fn validate_checks(records: &Value) -> Result<()> {
    require(
        records.array().is_some() && records.truth(),
        "checks must be a non-empty JSON array",
    )?;
    for (index, record) in records.items().iter().enumerate() {
        let prefix = format!("check {}", index + 1);
        require(
            record.object().is_some(),
            format!("{prefix} must be an object"),
        )?;
        let (required, optional): (&[&str], &[&str]) = match record["kind"].string() {
            Some("cmd") => (
                &["statement", "kind", "source", "cmd", "expect", "cwd"],
                &[],
            ),
            Some("provisional") => (
                &["statement", "kind", "source", "expect", "cwd", "missing"],
                &["cmd"],
            ),
            Some("judge") => (&["statement", "kind", "source"], &["how_to_judge"]),
            _ => return Err(format!("{prefix} has unknown kind")),
        };
        let mut missing: Vec<_> = required
            .iter()
            .filter(|k| !record.has(k))
            .copied()
            .collect();
        missing.sort();
        require(
            missing.is_empty(),
            format!(
                "{prefix} missing fields: {}.{}",
                missing.join(", "),
                if missing.contains(&"source") {
                    " Source must name the confirmed goal, a settled answer or examined repository paths and lines; never invent evidence."
                } else {
                    " Supply the missing check details before drafting."
                }
            ),
        )?;
        let mut extra: Vec<_> = record
            .fields()
            .iter()
            .map(|(k, _)| k.as_str())
            .filter(|k| !required.contains(k) && !optional.contains(k))
            .collect();
        extra.sort();
        require(
            extra.is_empty(),
            format!("{prefix} has unknown fields: {}", extra.join(", ")),
        )?;
        for (field, value) in record.fields() {
            require(
                text(value) && !value.string().unwrap_or("").contains('\0'),
                format!("{prefix}.{field} must be a non-empty string without NUL"),
            )?;
        }
        if let Some(cwd) = record.get("cwd").and_then(Value::string) {
            let drive = cwd.as_bytes().first().is_some_and(u8::is_ascii_alphabetic)
                && cwd.as_bytes().get(1) == Some(&b':');
            require(
                !cwd.starts_with('/')
                    && !cwd.split('/').any(|s| s == "..")
                    && !cwd.contains(['\\', '\n', '\r'])
                    && !drive,
                format!(
                    "{prefix}.cwd must be repo-relative, without '..', absolute paths or backslashes"
                ),
            )?;
        }
    }
    Ok(())
}

/// The first provisional check needing an operator command or judgment before confirmation.
pub fn commandless_check(records: &Value) -> Option<&str> {
    records
        .items()
        .iter()
        .find(|r| r["kind"].is("provisional") && !r.has("cmd"))
        .and_then(|r| r["statement"].string())
}

/// The confirmed goal's explicit Done when lines, in order. Unstructured goals have none.
pub(crate) fn goal_criteria(goal: &str) -> Vec<&str> {
    let mut in_done_when = false;
    goal.lines()
        .filter_map(|line| {
            if let Some(value) = line.strip_prefix("Done when:") {
                in_done_when = true;
                Some(value.trim())
            } else if in_done_when {
                let value = line.strip_prefix("- ").map(str::trim);
                in_done_when = value.is_some();
                value
            } else {
                None
            }
        })
        .filter(|value| !value.is_empty())
        .collect()
}

/// Whether `statement` keeps `criterion` whole: the same words, or the clause form the goal block
/// joined. `ignore_case` widens only letter case, never words or order.
pub(crate) fn keeps_criterion(statement: &str, criterion: &str, ignore_case: bool) -> bool {
    let same = |a: &str| {
        if ignore_case {
            a.to_lowercase() == criterion.to_lowercase()
        } else {
            a == criterion
        }
    };
    same(statement) || same(&crate::goal::composed_line(statement))
}

/// Keep each explicit confirmed success line in one whole statement; never infer semantic coverage.
/// A statement may keep the line's words in the clause form the goal block joined.
/// Unstructured goals and missing criteria add no requirements. This does not prove a check works.
/// A new draft's provisional never proposes a command another check runs in the same `cwd`
/// (#469); stored seeds keep reading.
pub(crate) fn validate_goal_checks(goal: &str, records: &Value) -> Result<()> {
    validate_checks(records)?;
    for criterion in goal_criteria(goal) {
        require(
            records.items().iter().any(|record| {
                record["statement"]
                    .string()
                    .is_some_and(|statement| keeps_criterion(statement, criterion, false))
            }),
            format!(
                "No seed was drafted: keep this complete Done when line unchanged in one check statement: {criterion} Separate checks may supplement it, not replace its sequence or qualifiers."
            ),
        )?;
    }
    let commands: Vec<_> = records
        .items()
        .iter()
        .map(|record| {
            let field = |key| record.get(key).and_then(Value::string);
            field("cmd").map(|cmd| (cmd, field("cwd")))
        })
        .collect();
    for (index, record) in records.items().iter().enumerate() {
        let Some((cmd, cwd)) =
            commands[index].filter(|_| record["kind"].string() == Some("provisional"))
        else {
            continue;
        };
        if let Some(other) =
            (0..commands.len()).find(|&i| i != index && commands[i] == Some((cmd, cwd)))
        {
            return Err(format!(
                "No seed was drafted: check {} proposes `{cmd}`, which check {} also uses in cwd `{}`. \
                 A provisional's proposed command must select its missing regression by name; otherwise leave cmd out.",
                index + 1,
                other + 1,
                cwd.unwrap_or(".")
            ));
        }
    }
    Ok(())
}

/// Read strict check records from an opened regular file without writing or running any of them.
pub fn read_checks(directory: &Path) -> Result<Value> {
    let path = directory.join(FILE);
    require(
        path.is_file(),
        "checks are missing: use `seed checks set <session> <input.json>`",
    )?;
    let bytes = super::super::read_regular(&path, "checks file").map_err(|e| e.to_string())?;
    let input =
        std::str::from_utf8(&bytes).map_err(|_| "stream did not contain valid UTF-8".to_owned())?;
    let records = Value::parse(input)?;
    validate_checks(&records)?;
    Ok(records)
}

pub(super) fn digest(records: &Value) -> Result<String> {
    Ok(super::seed::sha256(
        records.sorted().compact(false)?.as_bytes(),
    ))
}

fn prose(value: &str) -> String {
    let leading = value.bytes().take_while(|c| *c == b' ').count();
    let mut out = "&#32;".repeat(leading);
    let numeric = Regex::new(r"^\d+[.)]")
        .unwrap()
        .find(value)
        .map(|m| m.end() - 1);
    for (i, c) in value.char_indices().skip_while(|(i, _)| *i < leading) {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            c if whitespace(c) && c != ' ' => out.push_str(&format!("&#{};", c as u32)),
            c => {
                if "\\`*_[]#~".contains(c) || (i == 0 && "+-=".contains(c)) || numeric == Some(i) {
                    out.push('\\');
                }
                out.push(c);
            }
        }
    }
    out
}
fn command(value: &str) -> String {
    if value.contains("<!--")
        || value
            .chars()
            .any(|c| c < ' ' || (whitespace(c) && c != ' '))
    {
        return prose(value);
    }
    let longest = value.split(|c| c != '`').map(str::len).max().unwrap_or(0);
    let fence = "`".repeat(longest + 1);
    let pad = if value.starts_with(['`', ' ']) || value.ends_with(['`', ' ']) {
        " "
    } else {
        ""
    };
    format!("{fence}{pad}{value}{pad}{fence}")
}

/// An acceptance section as its checks were written, for display only: undoes the Markdown
/// escapes and entities `checks_block` adds to prose (#670). Commands, which are code spans, and
/// legacy JSON sections stay as stored; control characters stay as entities.
pub fn readable_acceptance(section: &str) -> String {
    if section.lines().any(|line| line.starts_with("```")) {
        return section.to_owned();
    }
    section
        .split('\n')
        .map(|line| match line.strip_prefix("check: ") {
            Some(code) if code.starts_with('`') => line.to_owned(),
            _ => unprose(line),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn unprose(line: &str) -> String {
    let entity = Regex::new(r"^&(?:amp|lt|gt|#(\d+));").unwrap();
    let mut out = String::new();
    let mut rest = line;
    while let Some(c) = rest.chars().next() {
        if c == '\\'
            && let Some(next) = rest[1..]
                .chars()
                .next()
                .filter(|n| "\\`*_[]#~+-=.)".contains(*n))
        {
            out.push(next);
            rest = &rest[1 + next.len_utf8()..];
            continue;
        }
        if let Some(m) = entity.captures(rest) {
            let decoded = match (&m[0], m.get(1)) {
                ("&amp;", _) => Some('&'),
                ("&lt;", _) => Some('<'),
                ("&gt;", _) => Some('>'),
                (_, Some(n)) => n.as_str().parse().ok().and_then(char::from_u32),
                _ => None,
            };
            if let Some(d) = decoded.filter(|d| !d.is_control()) {
                out.push(d);
                rest = &rest[m[0].len()..];
                continue;
            }
        }
        out.push(c);
        rest = &rest[c.len_utf8()..];
    }
    out
}

/// Render literal, one-line acceptance cards from records, never from model Markdown.
pub fn checks_block(records: &Value) -> Result<String> {
    validate_checks(records)?;
    let blocks: Vec<_> = records
        .items()
        .iter()
        .map(|r| {
            let statement = r["statement"].string().unwrap();
            let (label, detail) = match r["kind"].string().unwrap() {
                "cmd" => ("check", command(r["cmd"].string().unwrap())),
                "judge" => (
                    "you judge",
                    prose(
                        r.get("how_to_judge")
                            .and_then(Value::string)
                            .unwrap_or(statement),
                    ),
                ),
                _ => ("not yet runnable", prose(r["missing"].string().unwrap())),
            };
            format!("{}\n{label}: {detail}", prose(statement))
        })
        .collect();
    Ok(format!(
        "## Acceptance criteria\n\n{}\n\n",
        blocks.join("\n\n")
    ))
}

pub(super) fn legacy_block(records: &Value) -> Result<String> {
    validate_checks(records)?;
    Ok(format!(
        "## Acceptance criteria\n\n```json\n{}```\n\n",
        records.sorted().pretty()?
    ))
}

/// The canonical Full artifact sequence; hosts derive their prose fields from this layout.
pub(crate) const FULL: &[&str] = &[
    "## You are confirming",
    "## Problem statement",
    "## Proposed outcome",
    "## Acceptance criteria",
    "## Affected users and systems",
    "## Constraints and boundaries",
    "## Decisions (from the ledger)",
    "## Assumed (not confirmed by you)",
    "## Knowledge map",
    "## Evidence and uncertainty",
    "## Suggested first slice",
    "## Risks and verification",
    "## Open questions and deferrals",
    "## Owners and next steps",
    "## Downstream handoff",
];
pub(super) const LEAN: &[&str] = &[
    "## You are confirming",
    "## Now  (the agent keeps these current)",
    "## Goal",
    "## Decisions (from the ledger)",
    "## Assumed (not confirmed by you)",
    "## Constraints and exclusions",
    "## Acceptance criteria",
    "## Open questions",
    "## Knowledge map",
    "## Evidence",
];
fn without_comments(raw: &str, inside: &mut bool) -> String {
    let (mut out, mut rest) = (String::new(), raw);
    while !rest.is_empty() {
        if *inside {
            let Some(end) = rest.find("-->") else { break };
            rest = &rest[end + 3..];
            *inside = false;
        } else if let Some(start) = rest.find("<!--") {
            out.push_str(&rest[..start]);
            rest = &rest[start + 4..];
            *inside = true;
        } else {
            out.push_str(rest);
            break;
        }
    }
    out
}

struct HeadingError {
    rule: &'static str,
    prose_rule: &'static str,
    line: usize,
    text: String,
}

/// Locate literal artifact headings, retaining the helper-compatible refusal wording.
pub fn document_headings(body: &str) -> Result<Vec<(usize, String)>> {
    headings(body, &[]).map_err(|error| error.rule.into())
}

/// The same structural parser with offending lines for model prose-field refusals.
pub(crate) fn prose_headings(body: &str) -> Result<Vec<(usize, String)>> {
    headings(body, &[]).map_err(|error| {
        format!(
            "{} at line {}: {}",
            error.prose_rule,
            error.line,
            super::seed::repr(&error.text)
        )
    })
}

fn headings(
    body: &str,
    goal_lines: &[String],
) -> std::result::Result<Vec<(usize, String)>, HeadingError> {
    let opening = Regex::new(r"^ {0,3}(`{3,}|~{3,})(.*)$").unwrap();
    let setext = Regex::new(r"^(?:=+|-+)[ \t]*$").unwrap();
    let container = Regex::new(r"^(?:>[ \t]*|(?:[-+*]|\d+[.)])[ \t]+)").unwrap();
    let heading = Regex::new(r"^#{1,6}(?:[ \t]+|$)").unwrap();
    let (mut headings, mut fence, mut comment, mut offset) = (vec![], None::<Regex>, false, 0);
    for (index, line) in body.split_inclusive('\n').enumerate() {
        let raw = super::seed::split_terminator(line).0;
        let refusal = |rule, prose_rule| HeadingError {
            rule,
            prose_rule,
            line: index + 1,
            text: raw.into(),
        };
        let start = offset;
        offset += line.len();
        if let Some(ref close) = fence {
            if close.is_match(raw) {
                fence = None;
            }
            continue;
        }
        if !comment
            && let Some(c) = opening.captures(raw)
            && (c[1].starts_with('~') || !c[2].contains('`'))
        {
            let f = &c[1];
            let close = format!(r"^ {{0,3}}{}{{{},}}[ \t]*$", &f[..1], f.len());
            fence = Some(Regex::new(&close).unwrap());
            continue;
        }
        let visible = if !comment && goal_lines.iter().any(|line| line == raw) {
            raw.into()
        } else {
            without_comments(raw, &mut comment)
        };
        let mut candidate = visible.trim_start_matches(whitespace);
        loop {
            if setext.is_match(candidate) {
                return Err(refusal(
                    "checks refuse injected Setext heading syntax; body prose cannot contain headings",
                    "Lines of dashes or equals signs create headings; Tinkery writes every heading",
                ));
            }
            let Some(m) = container.find(candidate) else {
                break;
            };
            candidate = candidate[m.end()..].trim_start_matches(whitespace);
        }
        if heading.is_match(candidate) {
            if raw != candidate || visible != raw || line != format!("{raw}\n") {
                return Err(refusal(
                    "checks refuse injected, indented, commented or non-literal heading lines",
                    "Prose fields cannot contain heading syntax; Tinkery writes every heading",
                ));
            }
            headings.push((start, raw.into()));
        }
    }
    Ok(headings)
}

pub(super) fn require_seed_checks(
    directory: &Path,
    ledger: &Value,
    allow_legacy: bool,
) -> Result<Value> {
    let records = read_checks(directory)?;
    let body = super::seed::seed_lines(directory)?.concat();
    require(
        !body
            .as_bytes()
            .iter()
            .enumerate()
            .any(|(i, c)| *c == b'\r' && body.as_bytes().get(i + 1) != Some(&b'\n')),
        "checks refuse lone CR: seed line endings must be LF or CRLF",
    )?;
    let headings = headings(&body, &super::seed::goal_lines(ledger)?)
        .map_err(|error| error.rule.to_string())?;
    require(
        headings.first().is_some_and(|(_, s)| {
            s.strip_prefix("# ").is_some_and(|title| {
                !title.contains(['\r', '\n'])
                    && title.chars().next_back().is_some_and(|c| !whitespace(c))
            })
        }),
        "checks need one leading H1 title and canonical Full or Lean section headings",
    )?;
    let titles: Vec<_> = headings.iter().skip(1).map(|(_, s)| s.as_str()).collect();
    require(
        titles == FULL || titles == LEAN,
        "checks refuse injected, missing, reordered or non-canonical headings; use the Full or Lean template",
    )?;
    let index = headings
        .iter()
        .position(|(_, s)| s == "## Acceptance criteria")
        .unwrap();
    let section = &body[headings[index].0..headings.get(index + 1).map_or(body.len(), |h| h.0)];
    require(
        section == checks_block(&records)? || (allow_legacy && section == legacy_block(&records)?),
        "checks do not match the rendered Acceptance criteria section; use `seed checks render`",
    )?;
    Ok(records)
}
