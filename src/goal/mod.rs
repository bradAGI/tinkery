//! The goal block: seven typed parts and the one way they compose into text.
//!
//! Think fills [`GoalParts`]; review, receipt, tend and Shape all show the text
//! [`GoalParts::compose`] makes, so a goal reads the same everywhere it appears. Composition
//! rewrites only what `docs/design/goal-block.md` allows (a leading `must`/`must not` prefix,
//! clause joins and standalone-outcome beneficiaries); it never invents a part, and originals live elsewhere,
//! untouched.

use serde::{Deserialize, Serialize};

pub mod confirmed;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
/// One slot of the goal block. Its serde name is the field name the model's contract uses.
pub enum Part {
    /// Who benefits; "I" only for the operator's own goal.
    Who,
    /// What is true for them when it works, not how it gets built.
    Outcome,
    /// The situation it applies in, when that matters. Never a made-up deadline.
    When,
    /// What changes, or what is wrong now.
    Why,
    /// A check distinct from the outcome, in the person's words.
    DoneWhen,
    /// Positive constraints, one per item.
    Must,
    /// Exclusions and negative constraints, one per item.
    MustNot,
}
impl Part {
    /// Every part, in the order the block shows them.
    pub const ALL: [Self; 7] = [
        Self::Who,
        Self::Outcome,
        Self::When,
        Self::Why,
        Self::DoneWhen,
        Self::Must,
        Self::MustNot,
    ];
    /// The parts a goal needs before it's ready. When and the constraints are optional and are
    /// never filled just to complete a form.
    pub const REQUIRED: [Self; 4] = [Self::Who, Self::Outcome, Self::Why, Self::DoneWhen];
    /// The part's wire name, as the model and saved files spell it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Who => "who",
            Self::Outcome => "outcome",
            Self::When => "when",
            Self::Why => "why",
            Self::DoneWhen => "done_when",
            Self::Must => "must",
            Self::MustNot => "must_not",
        }
    }
    /// The part's label for people, as the goal block and questions show it.
    pub fn label(self) -> &'static str {
        match self {
            Self::Who => "Who",
            Self::Outcome => "Outcome",
            Self::When => "When",
            Self::Why => "Why",
            Self::DoneWhen => "Done when",
            Self::Must => "Must",
            Self::MustNot => "Must not",
        }
    }
}
fn capitalise(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().collect::<String>() + chars.as_str()
    })
}
pub(super) fn identity(text: &str) -> String {
    text.trim()
        .trim_end_matches(['.', ',', ';', '!', '?'])
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}
/// The goal already says `phrase` as whole words, so "thermostat works" never states "at work".
fn states_whole_words(goal: &str, phrase: &str) -> bool {
    let words = |text: &str| -> Vec<String> {
        identity(text)
            .split(' ')
            .map(|word| word.trim_matches(|c: char| !c.is_alphanumeric()).to_owned())
            .filter(|word| !word.is_empty())
            .collect()
    };
    let (goal, phrase) = (words(goal), words(phrase));
    !phrase.is_empty()
        && goal
            .windows(phrase.len())
            .any(|window| window == phrase.as_slice())
}
/// Constraint identity preserves internal whitespace and case after the first letter.
/// "Upload logs" matches "upload logs" while "use IT" stays distinct from "use it".
fn constraint_identity(text: &str) -> String {
    let trimmed = text.trim().trim_end_matches(['.', ',', ';', '!', '?']);
    let mut chars = trimmed.chars();
    chars.next().map_or_else(String::new, |c| {
        c.to_lowercase().collect::<String>() + chars.as_str()
    })
}
/// Clauses of prose (why, done when, the goal line, when). Constraint items are never
/// split: each is one line as the model gave it.
fn clauses(text: &str) -> Vec<String> {
    let chars = text
        .trim()
        .trim_end_matches(['.', ',', ';'])
        .chars()
        .collect::<Vec<_>>();
    // A single quote (straight or curly) closes only at the last one of its kind that
    // isn't an apostrophe inside a word ("don’t"), so a possessive ("users' choices")
    // inside a quotation doesn't end it early.
    let closes = |kind: char| {
        (0..chars.len())
            .rev()
            .find(|&n| chars[n] == kind && !chars.get(n + 1).is_some_and(|c| c.is_alphanumeric()))
    };
    let (last_straight, last_curly) = (closes('\''), closes('’'));
    let mut quote = None;
    let mut parts = vec![];
    let mut current = String::new();
    for (i, &c) in chars.iter().enumerate() {
        let previous = i.checked_sub(1).and_then(|n| chars.get(n)).copied();
        let next = chars.get(i + 1).copied();
        let closing = match c {
            '\'' => last_straight == Some(i),
            '’' => last_curly == Some(i),
            _ => true,
        };
        if quote == Some(c) && closing {
            quote = None;
        } else if quote.is_none()
            && (c == '"'
                || c == '“'
                || (matches!(c, '\'' | '‘') && !previous.is_some_and(char::is_alphanumeric)))
        {
            quote = Some(match c {
                '“' => '”',
                '‘' => '’',
                c => c,
            });
        }
        let decimal = c == '.'
            && previous.is_some_and(|c| c.is_ascii_digit())
            && next.is_some_and(|c| c.is_ascii_digit());
        // A period after a lone letter or a common abbreviation ("U.S. data", "e.g. maps",
        // "Dr. Smith") is not a clause end.
        let word = if c == '.' {
            chars[..i]
                .iter()
                .rev()
                .take_while(|c| c.is_alphabetic())
                .count()
        } else {
            0
        };
        let initial = word > 0
            && (word == 1 || {
                let w = chars[i - word..i].iter().collect::<String>();
                ABBREVIATIONS.iter().any(|a| a.eq_ignore_ascii_case(&w))
            })
            && (i - word)
                .checked_sub(1)
                .and_then(|n| chars.get(n))
                .is_none_or(|c| !c.is_alphanumeric());
        if quote.is_none()
            && (matches!(c, ';' | '\n')
                || (matches!(c, '.' | '!' | '?')
                    && !decimal
                    && !initial
                    && next.is_some_and(char::is_whitespace)))
        {
            parts.push(current.trim().to_owned());
            current.clear();
        } else {
            current.push(if c == '\n' { ' ' } else { c });
        }
    }
    parts.push(current.trim().to_owned());
    parts.into_iter().filter(|s| !s.is_empty()).collect()
}
/// Abbreviations whose period is not a clause end (see `clauses`).
const ABBREVIATIONS: [&str; 14] = [
    "co", "dr", "etc", "inc", "jr", "ltd", "mr", "mrs", "ms", "mt", "prof", "sr", "st", "vs",
];
/// Common words lowered when they start a later clause. Any other first word (a name,
/// acronym such as "IT", quote or number) keeps its case.
const LOWERED: [&str; 34] = [
    "a", "all", "also", "an", "and", "both", "but", "each", "every", "he", "her", "his", "if",
    "it", "its", "no", "not", "or", "our", "she", "so", "some", "that", "the", "their", "then",
    "there", "these", "they", "this", "those", "we", "you", "your",
];
fn prose(text: &str) -> String {
    clauses(text)
        .into_iter()
        .enumerate()
        .map(|(i, s)| {
            let first = s.split_whitespace().next().unwrap_or_default();
            let mut chars = s.chars();
            match chars.next() {
                Some(c)
                    if i > 0
                        && LOWERED.contains(&first.to_lowercase().as_str())
                        && !first.chars().skip(1).any(char::is_uppercase) =>
                {
                    c.to_lowercase().collect::<String>() + chars.as_str()
                }
                _ => s,
            }
        })
        .collect::<Vec<_>>()
        .join("; ")
}
fn finish(s: String) -> String {
    if s.ends_with(['.', '!', '?']) {
        s
    } else {
        s + "."
    }
}
/// Text as the goal block would show it after a `Done when:` label: the same words, with
/// clauses joined and finished as composition does. Equal results mean the same words in
/// the same order, never similar meaning.
pub fn composed_line(text: &str) -> String {
    finish(prose(text))
}
/// Explicit leading negative syntax, matched case-insensitively; see goal-block.md.
const NEGATIVE: [&str; 4] = ["must not", "do not", "don't", "don\u{2019}t"];
/// Words after a negative prefix that make the clause something other than an exclusion.
const NOT_AN_EXCLUSION: [&str; 6] = ["only", "just", "necessarily", "always", "care", "mind"];
fn after_prefix<'a>(clause: &'a str, prefix: &str) -> Option<&'a str> {
    let rest = clause
        .get(..prefix.len())
        .filter(|head| head.eq_ignore_ascii_case(prefix))
        .and_then(|_| clause[prefix.len()..].strip_prefix(char::is_whitespace))?
        .trim_start();
    (!rest.is_empty()).then_some(rest)
}
/// Whether a constraint item seems to hold more than one constraint: a later clause starts
/// with syntax that rewriting accepts. Only ever logged; the item is shown as written.
pub(super) fn looks_compound(item: &str) -> bool {
    clauses(item)
        .iter()
        .skip(1)
        .any(|c| constraint_syntax(c, false).is_some())
}
/// Where an unambiguous leading prefix puts this clause: (is an exclusion, remainder).
fn constraint_syntax(clause: &str, negative: bool) -> Option<(bool, &str)> {
    if let Some(rest) = NEGATIVE.iter().find_map(|p| after_prefix(clause, p)) {
        // The qualifier is the leading letters/digits, so "necessarily—at" still reads
        // as "necessarily".
        let first = rest
            .split(|c: char| !c.is_alphanumeric())
            .find(|w| !w.is_empty())?
            .to_lowercase();
        return (!NOT_AN_EXCLUSION.contains(&first.as_str())).then_some((true, rest));
    }
    (!negative)
        .then(|| after_prefix(clause, "must"))
        .flatten()
        // "must not" followed by anything but a plain space ("must not:", "must not—…")
        // is unsupported syntax: leave it whole rather than strip only "must".
        .filter(|rest| {
            !rest.get(..3).is_some_and(|w| w.eq_ignore_ascii_case("not"))
                || rest[3..].starts_with(char::is_alphanumeric)
        })
        .map(|rest| (false, rest))
}
/// A goal as seven typed parts, each in the model's words for the person's intent. A missing
/// part is `None` (or empty), never a placeholder, so "still open" can't be mistaken for an
/// answer. Each field is the [`Part`] of the same name.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct GoalParts {
    /// See [`Part::Who`].
    pub who: Option<String>,
    /// See [`Part::Outcome`].
    pub outcome: Option<String>,
    /// See [`Part::When`].
    pub when: Option<String>,
    /// See [`Part::Why`].
    pub why: Option<String>,
    /// See [`Part::DoneWhen`].
    pub done_when: Option<String>,
    /// See [`Part::Must`]. Each item is shown as one line, never split.
    #[serde(default)]
    pub must: Vec<String>,
    /// See [`Part::MustNot`]. Each item is shown as one line, never split.
    #[serde(default)]
    pub must_not: Vec<String>,
}
impl GoalParts {
    /// The part's text, trimmed; `None` when it is missing or blank. A constraint list answers
    /// with its first item, which is enough to tell whether it is filled.
    pub fn get(&self, part: Part) -> Option<&str> {
        let value = match part {
            Part::Who => self.who.as_ref(),
            Part::Outcome => self.outcome.as_ref(),
            Part::When => self.when.as_ref(),
            Part::Why => self.why.as_ref(),
            Part::DoneWhen => self.done_when.as_ref(),
            Part::Must => self.must.first(),
            Part::MustNot => self.must_not.first(),
        };
        value
            .map(String::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
    }
    /// The required parts still missing, in block order.
    pub fn missing(&self) -> impl Iterator<Item = Part> + '_ {
        Part::REQUIRED
            .into_iter()
            .filter(|p| self.get(*p).is_none())
    }
    /// The known intent, with its beneficiary and situation when supplied, without its final
    /// period. A missing beneficiary stays unknown, never invented; only a missing outcome
    /// prevents a headline. Repeated subjects and situations aren't said twice. An uppercase
    /// standalone outcome leads, with the beneficiary after it ("I" becomes "me"). An uppercase
    /// situation starts its own sentence; a lowercase situation keeps the comma join.
    pub fn goal_line(&self) -> Option<String> {
        let outcome = self.get(Part::Outcome)?;
        let raw_who = self.get(Part::Who).map(prose).unwrap_or_default();
        let who = capitalise(&raw_who);
        let mut outcome = prose(outcome);
        let mut separator = if who.is_empty() { "" } else { " " };
        let repeated_subject = outcome
            .get(..raw_who.len())
            .is_some_and(|prefix| identity(prefix) == identity(&raw_who))
            && outcome.get(raw_who.len()..).is_some_and(|rest| {
                rest.starts_with(char::is_whitespace)
                    || ["'s", "’s"].iter().any(|suffix| {
                        rest.strip_prefix(suffix)
                            .is_some_and(|tail| tail.starts_with(char::is_whitespace))
                    })
            });
        if repeated_subject {
            let rest = &outcome[raw_who.len()..];
            if !rest.starts_with(char::is_whitespace) {
                separator = "";
            }
            outcome = rest.trim_start().to_owned();
        }
        while outcome.starts_with("can can ") {
            outcome = outcome[4..].to_owned();
        }
        let mut goal = if !repeated_subject
            && !raw_who.is_empty()
            && outcome.starts_with(char::is_uppercase)
        {
            let beneficiary = if raw_who == "I" { "me" } else { &raw_who };
            format!("{outcome}, for {beneficiary}")
        } else {
            capitalise(&format!("{who}{separator}{outcome}"))
        };
        if let Some(when) = self.get(Part::When).map(prose)
            && !states_whole_words(&goal, &when)
        {
            let join = if when.starts_with(char::is_uppercase) {
                ". "
            } else {
                ", "
            };
            goal.push_str(join);
            goal.push_str(&when);
        }
        Some(goal)
    }
    /// Rewrites only an explicit leading `must`/`must not`/`do not`/`don't` on each whole
    /// constraint item; anything else, and every original source, stays as written.
    pub(super) fn normalize_constraints(&mut self) {
        let mut lists: [Vec<(String, bool)>; 2] = Default::default();
        for (negative, values) in [
            (false, std::mem::take(&mut self.must)),
            (true, std::mem::take(&mut self.must_not)),
        ] {
            for value in values {
                match constraint_syntax(value.trim(), negative) {
                    Some((to, rest)) => lists[usize::from(to)].push((rest.to_owned(), true)),
                    None => lists[usize::from(negative)].push((value, false)),
                }
            }
        }
        // A rewritten item already stated in its category is not repeated.
        let [must, must_not] = lists.map(|list| {
            let mut stated = list
                .iter()
                .filter(|(_, rewritten)| !rewritten)
                .map(|(v, _)| constraint_identity(v))
                .collect::<std::collections::HashSet<_>>();
            list.into_iter()
                .filter_map(|(value, rewritten)| {
                    (!rewritten || stated.insert(constraint_identity(&value))).then_some(value)
                })
                .collect()
        });
        self.must = must;
        self.must_not = must_not;
    }
    pub(super) fn distinct_check(&mut self) {
        if let Some(why) = self.get(Part::Why)
            && self
                .get(Part::Outcome)
                .is_some_and(|outcome| identity(why) == identity(outcome))
        {
            self.why = None;
        }
        if let Some(done) = self.get(Part::DoneWhen)
            && (self
                .goal_line()
                .is_some_and(|line| identity(&line) == identity(done))
                || self
                    .get(Part::Outcome)
                    .is_some_and(|outcome| identity(outcome) == identity(done)))
        {
            self.done_when = None;
        }
    }
    /// The whole goal block as people see and save it: the goal line, then Why, Done when,
    /// and one line per distinct constraint. `None` when there is nothing to show yet.
    pub fn compose(&self) -> Option<String> {
        let mut lines = vec![];
        if let Some(goal) = self.goal_line() {
            lines.push(finish(goal));
        }
        for part in [Part::Why, Part::DoneWhen] {
            if let Some(value) = self.get(part) {
                lines.push(finish(format!("{}: {}", part.label(), prose(value))));
            }
        }
        for (label, values) in [("Must", &self.must), ("Must not", &self.must_not)] {
            let mut seen = std::collections::BTreeSet::new();
            for value in values {
                let line = value
                    .trim()
                    .trim_end_matches(['.', ',', ';'])
                    .replace('\n', " ");
                if !line.is_empty() && seen.insert(constraint_identity(&line)) {
                    lines.push(finish(format!("{label}: {line}")));
                }
            }
        }
        (!lines.is_empty()).then(|| lines.join("\n"))
    }
}
