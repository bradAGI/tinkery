//! The reading: think's contract with the model for each shaping call. It holds the request
//! think sends ([`BoardRequest`]), the prompt, the reply's wire shape ([`Guess`]) and the
//! lenient decoder that reads a reply into it. [`board`](super::board) then verifies it.
use super::*;

/// The reading prompt as written: one rule per line, with notes on why. See its header.
pub(super) const PROMPT: &str = include_str!("prompts/reading.txt");

/// The system prompt for one shaping call, with the bundled shaping guidance in its place.
pub(super) fn prompt(seed_me_shaping: &str) -> String {
    assemble(PROMPT).replace("{seed_me_shaping}", seed_me_shaping)
}

pub(super) use crate::pi::assemble;

/// The fields of a reading the decoder reads. The prompt's schema line names the same fields, less
/// three that older replies carried (`uncertain`, `framings`, `outcome`); `prompt_tests` keeps the
/// two equal.
pub(super) const READING: &[&str] = &[
    "parts",
    "other_goals",
    "deferred",
    "open",
    "supports",
    "uncertain",
    "framings",
    "outcome",
    "misfits",
    "unresolved_notes",
    "questions",
    "alternatives",
    "decisions",
];
/// The fields of `parts`.
pub(super) const PARTS: &[&str] = &[
    "who",
    "outcome",
    "when",
    "why",
    "done_when",
    "must",
    "must_not",
];
/// The fields of one decision. The decision review adds a question to these.
pub(super) const DECISION: &[&str] = &["part", "text", "answers", "question", "suggestion"];
/// The fields of one question.
pub(super) const QUESTION: &[&str] = &["id", "text", "target", "suggestion"];
/// The fields of one possible approach. Replies have also keyed the label `option` (#89).
pub(super) const APPROACH: &[&str] = &["label", "benefit", "cost", "undo_cost"];
/// The fields of a claimed source span.
pub(super) const ANCHOR: &[&str] = &["source", "quote", "occurrence"];

/// One thing the person sent: a dump, added words or an answer. Kept exactly as sent; nothing
/// rewrites a source.
#[derive(Clone, Debug, Serialize)]
pub struct Source {
    /// Its number in the conversation, from 1.
    pub id: usize,
    /// The words exactly as sent.
    pub text: String,
    /// The id of the question it answers, when it is an answer.
    pub in_reply_to: Option<String>,
}
/// An excerpt the person pulled out of a source into a card (Ctrl-E). Theirs, not the model's.
#[derive(Clone, Debug, Serialize)]
pub struct Fragment {
    /// The card's id.
    pub id: String,
    /// The source it came from.
    pub source: usize,
    /// Byte offset where it starts in that source.
    pub start: usize,
    /// Byte offset where it ends.
    pub end: usize,
    /// The excerpt, verbatim.
    pub text: String,
}
/// A span of the person's words the model claims to cite. It counts only if it is an exact
/// substring, so the model can point at words but never put words in the person's mouth.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Anchor {
    /// The source's id.
    pub source: usize,
    /// The exact words, character for character.
    pub quote: String,
    /// Which occurrence of `quote` in that source, from 0.
    pub occurrence: usize,
}
impl Anchor {
    /// Where the quote sits in its source, or why it isn't there exactly: an unknown source,
    /// an empty or inexact quote, or a span that would cut a character.
    pub fn range(&self, sources: &[Source]) -> Result<std::ops::Range<usize>, String> {
        let source = sources
            .iter()
            .find(|s| s.id == self.source)
            .ok_or("Unknown annotation source")?;
        if self.quote.trim().is_empty() {
            return Err("Empty annotation".into());
        }
        let start = source
            .text
            .match_indices(&self.quote)
            .nth(self.occurrence)
            .map(|(i, _)| i)
            .ok_or("Annotation is not an exact source substring at that occurrence")?;
        let end = start + self.quote.len();
        if !grapheme_boundary(&source.text, start) || !grapheme_boundary(&source.text, end) {
            return Err("Annotation cuts a grapheme; no highlight applied".into());
        }
        Ok(start..end)
    }
}
/// The reading's text with the spans it cites.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Framing {
    /// The reading, as shown under "I think you mean…".
    pub text: String,
    /// The person's words it rests on; may be empty.
    pub supports: Vec<Anchor>,
}
/// A question for the person about one part of the goal.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Question {
    /// A provisional answer, never sent back as operator context until submitted.
    #[serde(default, skip_serializing, deserialize_with = "suggestion::decode")]
    pub suggestion: Option<Suggestion>,
    /// The part an answer would change; `None` for a question about scope.
    #[serde(default)]
    pub target: Option<Part>,
    /// Identifies the question across turns, so an answered or skipped one isn't asked again.
    pub id: String,
    /// The question as shown.
    pub text: String,
}
/// An open choice proposed by a model, with its alternatives and the question that names it.
/// This is provisional context, never an operator decision or permission.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OpenDecision {
    /// A provisional answer, never sent back as operator context until submitted.
    #[serde(default, skip_serializing, deserialize_with = "suggestion::decode")]
    pub suggestion: Option<Suggestion>,
    /// The goal line an answer would change.
    pub part: Part,
    /// What is undecided.
    pub text: String,
    /// At least two alternatives, kept for review and handoff.
    pub answers: Vec<String>,
    /// The question about this choice, when supplied; absence never hides the choice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub question: Option<String>,
}
impl OpenDecision {
    /// The note used when this choice has no question represented in review.
    pub fn note(&self) -> String {
        format!(
            "Open decision: {} ({})",
            self.text,
            self.answers.join(" / ")
        )
    }
    pub(super) fn covered_by(&self, questions: &[Question]) -> bool {
        self.question.as_ref().is_some_and(|text| {
            let linked = Question {
                suggestion: None,
                target: Some(self.part),
                id: String::new(),
                text: text.clone(),
            };
            questions
                .iter()
                .any(|q| q.target == linked.target && same_question_text(&q.text, &linked.text))
        })
    }
}

/// One reading as decoded from the model's reply: unverified, so nothing shows it or trusts it
/// until [`Board::verify`](super::board::Board::verify) accepts it. The fields follow the
/// prompt's schema; see `prompts/reading.txt` for what each asks of the model.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Guess {
    /// The goal's parts as read.
    #[serde(default)]
    pub parts: GoalParts,
    /// Separate goals noticed in the dump, kept aside, never folded into this one.
    #[serde(default)]
    pub other_goals: Vec<String>,
    /// Things the person explicitly put off; carried to the seed.
    #[serde(default)]
    pub deferred: Vec<String>,
    /// Parts still consequentially unclear.
    #[serde(default)]
    pub open: Vec<Part>,
    /// Open choices and their question links; originals and answers remain separate sources.
    #[serde(default)]
    pub decisions: Vec<OpenDecision>,
    /// Open intent that can't become a question this turn.
    #[serde(default)]
    pub unresolved_notes: Vec<String>,
    /// Always false: an older reply's flag is noted and ignored, since every reading is
    /// provisional anyway.
    pub uncertain: bool,
    /// One reading, composed from the parts with every citation the reply made. Free text
    /// the model writes as a reading is ignored, so the goal comes only from its parts.
    pub framings: Vec<Framing>,
    /// The done-when part again, under the draft outcome field.
    pub outcome: String,
    /// The person's words that don't fit the reading, kept visible as open.
    pub misfits: Vec<Anchor>,
    /// Questions in priority order; at most one is shown.
    pub questions: Vec<Question>,
    /// Approaches the model proposes; proposals only.
    pub alternatives: Vec<Approach>,
}
/// A question the person answered, and the source holding their answer.
#[derive(Clone, Debug, Serialize)]
pub struct Settled {
    /// The question as it was asked.
    pub question: Question,
    /// The answer's source id.
    pub source: usize,
}
/// Everything one shaping call sends. Think owns the history here (answers, skips, ask counts),
/// so the model can't rewrite what was asked or answered; [`Guess::validate`] checks a reply
/// against it.
#[derive(Clone, Serialize)]
pub struct BoardRequest {
    /// How often each part has been asked about; a part asked twice is left open.
    pub ask_counts: std::collections::BTreeMap<Part, u8>,
    /// Every source sent so far, verbatim.
    pub sources: Vec<Source>,
    /// The person's extracted cards.
    pub fragments: Vec<Fragment>,
    /// The last verified reading, to refine rather than start over.
    pub previous: Option<Guess>,
    /// Questions the person skipped; their targets aren't asked again.
    pub skipped: Vec<Question>,
    /// Ids of questions already answered.
    pub answered: Vec<String>,
    /// The answered questions with their answers.
    pub settled: Vec<Settled>,
    /// Each card's id and position on the canvas.
    pub layout: Vec<(String, f64, f64)>,
}

pub(super) fn same_question(a: &Question, b: &Question) -> bool {
    a.id == b.id || same_question_text(&a.text, &b.text)
}
fn same_question_text(a: &str, b: &str) -> bool {
    let words = |s: &str| {
        s.split(|c: char| !c.is_alphanumeric())
            .filter(|s| !s.is_empty())
            .map(str::to_lowercase)
            .collect::<Vec<_>>()
    };
    words(a) == words(b)
}
pub(super) fn grapheme_boundary(text: &str, index: usize) -> bool {
    index == text.len() || text.grapheme_indices(true).any(|(i, _)| i == index)
}
fn safe(text: &str) -> bool {
    !text.trim().is_empty() && text.chars().all(|c| c == '\n' || !c.is_control())
}
impl Guess {
    /// Reads a reply leniently: tolerated slips (a code fence, a trailing comma, unknown fields)
    /// are fixed and noted in `normalizations`; anything that would change meaning is refused.
    pub fn decode(
        raw: &str,
        request: &BoardRequest,
        normalizations: &mut Vec<String>,
    ) -> Result<Self, String> {
        reviewed(raw, None, request, normalizations)
    }
    /// [`decode`](Self::decode), with a decision review joined in: decisions it covers are
    /// replaced, its own are added, and once the required parts are present its question comes
    /// first. The same checks then apply to the whole reading.
    pub fn decode_reviewed(
        raw: &str,
        review: Option<&Review>,
        request: &BoardRequest,
        normalizations: &mut Vec<String>,
    ) -> Result<Self, String> {
        reviewed(raw, review, request, normalizations)
    }
    /// Refuses a reading that could mislead: control characters in shown text, a cited span
    /// that isn't exact, or a question that reuses an answered, skipped or application-owned
    /// identity.
    pub fn validate(&self, request: &BoardRequest) -> Result<(), String> {
        if self
            .parts
            .must
            .iter()
            .chain(&self.parts.must_not)
            .chain(&self.other_goals)
            .chain(&self.deferred)
            .any(|s| s.chars().any(|c| c != '\n' && c.is_control()))
        {
            return Err("Unsafe display control".into());
        }
        for part in Part::ALL {
            if self
                .parts
                .get(part)
                .is_some_and(|s| s.chars().any(|c| c != '\n' && c.is_control()))
            {
                return Err("Unsafe display control".into());
            }
        }
        for frame in &self.framings {
            if frame.text.chars().any(|c| c != '\n' && c.is_control()) {
                return Err("Unsafe display control".into());
            }
            for span in &frame.supports {
                span.range(&request.sources)?;
            }
        }
        for span in &self.misfits {
            span.range(&request.sources)?;
        }
        for text in std::iter::once(&self.outcome)
            .chain(self.unresolved_notes.iter())
            .chain(self.decisions.iter().flat_map(|d| {
                std::iter::once(&d.text)
                    .chain(d.answers.iter())
                    .chain(d.question.iter())
            }))
            .chain(
                self.alternatives
                    .iter()
                    .flat_map(|a| [&a.label, &a.benefit, &a.cost, &a.undo_cost]),
            )
        {
            if text.chars().any(|c| c != '\n' && c.is_control()) {
                return Err("Unsafe display control".into());
            }
        }
        let mut ids = HashSet::new();
        for q in &self.questions {
            if q.id.starts_with("scope-addition-")
                || !safe(&q.id)
                || !safe(&q.text)
                || !ids.insert(&q.id)
                || request.answered.contains(&q.id)
                || request
                    .settled
                    .iter()
                    .any(|s| same_question(&s.question, q))
                || request.skipped.iter().any(|s| same_question(s, q))
            {
                return Err("Invalid application-owned question identity/history".into());
            }
        }
        Ok(())
    }
}

/// Goal parts with the reading's display cleanup and duplicate-part rules applied.
/// Refuses unreadable Done when shapes rather than silently losing success checks.
pub(super) fn read_parts(
    value: Option<&serde_json::Value>,
    changes: &mut Vec<String>,
) -> Result<GoalParts, String> {
    let mut parts = GoalParts::default();
    let object = value.and_then(serde_json::Value::as_object);
    if let Some(object) = object {
        extras(object, PARTS, "goal parts", changes);
    }
    parts.who = display(object.and_then(|o| o.get("who")), "who", changes);
    parts.outcome = display(object.and_then(|o| o.get("outcome")), "outcome", changes);
    parts.when = display(object.and_then(|o| o.get("when")), "when", changes);
    parts.why = display(object.and_then(|o| o.get("why")), "why", changes);
    let done_when = object.and_then(|o| o.get("done_when"));
    let joined;
    let done_when = match done_when {
        Some(serde_json::Value::Array(values)) => {
            let checks = values
                .iter()
                .map(|v| {
                    v.as_str()
                        .ok_or("Unreadable done_when: expected text or an array of strings")
                })
                .collect::<Result<Vec<_>, _>>()?;
            joined = serde_json::Value::String(checks.join("\n"));
            changes
                .push("Joined done_when list with newlines; wording and raw reply retained".into());
            Some(&joined)
        }
        None | Some(serde_json::Value::Null | serde_json::Value::String(_)) => done_when,
        Some(_) => return Err("Unreadable done_when: expected text or an array of strings".into()),
    };
    parts.done_when = display(done_when, "done_when", changes);
    parts.must = items(object.and_then(|o| o.get("must")), "must", changes)
        .iter()
        .filter_map(|v| display(Some(v), "must", changes))
        .collect();
    parts.must_not = items(object.and_then(|o| o.get("must_not")), "must not", changes)
        .iter()
        .filter_map(|v| display(Some(v), "must not", changes))
        .collect();
    for value in parts
        .must
        .iter()
        .chain(&parts.must_not)
        .filter(|v| crate::goal::looks_compound(v))
    {
        changes.push(format!(
            "A constraint item looks like more than one constraint; shown as one line as written: {value}"
        ));
    }
    parts.distinct_check();
    Ok(parts)
}

/// What the decision review sees of a reply before it is decoded: its parts and decisions as
/// written, so a readable but invalid reading is still reviewed. `None` when unreadable.
pub(super) fn draft(raw: &str) -> Option<(serde_json::Value, serde_json::Value)> {
    let reading = read(raw, &mut vec![]).ok()?;
    let parts = reading.get("parts").cloned().unwrap_or_default();
    let decisions = reading
        .get("decisions")
        .cloned()
        .unwrap_or_else(|| serde_json::json!([]));
    Some((parts, decisions))
}

fn reviewed(
    raw: &str,
    review: Option<&Review>,
    request: &BoardRequest,
    changes: &mut Vec<String>,
) -> Result<Guess, String> {
    let value = read(raw, changes)?;
    let obj = value.as_object().ok_or("No readable board object")?;
    extras(obj, READING, "board", changes);
    if obj.contains_key("uncertain") {
        changes.push("Legacy uncertainty flag ignored as a count/authority signal; all readings remain provisional".into());
    }
    let parts = read_parts(obj.get("parts"), changes)?;
    let other_goals = items(obj.get("other_goals"), "other goals", changes)
        .iter()
        .filter_map(|v| display(Some(v), "other goal", changes))
        .collect();
    let deferred = items(obj.get("deferred"), "deferred", changes)
        .iter()
        .filter_map(|v| display(Some(v), "deferred", changes))
        .collect();
    let mut open = vec![];
    for value in items(obj.get("open"), "open parts", changes) {
        if let Ok(part) = serde_json::from_value::<Part>(value.clone()) {
            if !open.contains(&part) {
                open.push(part);
            }
        } else {
            changes.push(format!("Ignored unknown open part {value}"));
        }
    }
    // Typed decisions: a choice only the person can make, with answers that would change
    // a goal line. Each keeps its part open (so it can be asked and blocks readiness) and
    // travels to review and the seed as a note.
    let mut shaping = vec![];
    for (i, v) in items(obj.get("decisions"), "decisions", changes)
        .iter()
        .enumerate()
    {
        let Some(o) = v.as_object() else {
            changes.push(format!("Ignored decision {i}: not an object"));
            continue;
        };
        extras(o, DECISION, &format!("decision {i}"), changes);
        let part = o
            .get("part")
            .and_then(|v| serde_json::from_value::<Part>(v.clone()).ok());
        let text = display(o.get("text"), &format!("decision {i}"), changes);
        let mut answers: Vec<String> = vec![];
        for a in items(o.get("answers"), &format!("decision {i} answers"), changes) {
            if let Some(a) = display(Some(&a), &format!("decision {i} answer"), changes)
                && !answers.contains(&a)
            {
                answers.push(a);
            }
        }
        let (Some(part), Some(text)) = (part, text) else {
            changes.push(format!("Ignored decision {i}: needs a known part and text"));
            continue;
        };
        if answers.len() < 2 {
            changes.push(format!(
                "Ignored decision {i}: needs two answers that change the goal"
            ));
            continue;
        }
        let question = display(
            o.get("question"),
            &format!("decision {i} question"),
            changes,
        );
        shaping.push((
            i,
            OpenDecision {
                part,
                text,
                answers,
                question,
                suggestion: suggestion::read(o.get("suggestion")),
            },
        ));
    }
    let decisions = match review {
        Some(review) => review.join(shaping, changes),
        None => shaping.into_iter().map(|(_, d)| d).collect(),
    };
    let mut decision_notes = vec![];
    for decision in &decisions {
        if !open.contains(&decision.part) {
            open.push(decision.part);
        }
        decision_notes.push(decision.note());
    }
    let mut framings = vec![];
    for (i, v) in items(obj.get("framings"), "framings", changes)
        .iter()
        .enumerate()
    {
        let (text, supports) = if let Some(o) = v.as_object() {
            extras(o, &["text", "supports"], &format!("reading {i}"), changes);
            (
                display(o.get("text"), &format!("reading {i}"), changes),
                anchors(o.get("supports"), &format!("reading {i} supports"), changes)?,
            )
        } else {
            (display(Some(v), &format!("reading {i}"), changes), vec![])
        };
        if supports.is_empty() {
            changes.push(format!("Reading {i} is uncited; no source highlight"));
        }
        framings.push(Framing {
            text: text.unwrap_or_default(),
            supports,
        });
    }
    if obj.contains_key("framings") {
        changes.push("Ignored freeform reading text; goal comes only from parts".into());
    }
    let mut supports = anchors(obj.get("supports"), "goal supports", changes)?;
    supports.extend(framings.into_iter().flat_map(|f| f.supports));
    let framings = vec![Framing {
        text: parts.compose().unwrap_or_default(),
        supports,
    }];
    let outcome = parts.done_when.clone().unwrap_or_default();
    let mut questions = vec![];
    let mut question_values = items(obj.get("questions"), "questions", changes);
    if let Some(review) = review {
        // A missing required part is still asked before any review decision.
        let asks = review
            .questions()
            .map(|d| serde_json::json!({"target": d.part, "text": d.question, "suggestion": d.suggestion}));
        if parts.missing().next().is_some() {
            question_values.extend(asks);
        } else {
            question_values.splice(0..0, asks);
        }
    }
    question_values.extend(decisions.iter().filter_map(|d| {
        d.question.as_ref().map(
            |text| serde_json::json!({"target": d.part, "text": text, "suggestion": d.suggestion}),
        )
    }));
    for (i, v) in question_values.iter().enumerate() {
        let o = v.as_object();
        if let Some(o) = o {
            extras(o, QUESTION, &format!("question {i}"), changes);
        }
        let Some(text) = display(
            o.and_then(|o| o.get("text")).or(Some(v)),
            &format!("question {i}"),
            changes,
        ) else {
            continue;
        };
        let id = display(
            o.and_then(|o| o.get("id")),
            &format!("question {i} ID"),
            changes,
        )
        .unwrap_or_else(|| {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            text.hash(&mut h);
            let id = format!("model-{:016x}", h.finish());
            changes.push(format!("Allocated question ID {id}"));
            id
        });
        let target = o
            .and_then(|o| o.get("target"))
            .and_then(|v| serde_json::from_value::<Part>(v.clone()).ok());
        let target = target
            .or_else(|| open.first().copied())
            .or_else(|| parts.missing().next());
        if o.and_then(|o| o.get("target")).is_none() {
            changes.push(format!(
                "Question {id} target assigned from first open/missing part"
            ));
        }
        if let Some(part) = target {
            if parts.get(part).is_some() && !open.contains(&part) {
                changes.push(format!("Ignored question {id}: no missing/open target"));
                continue;
            }
        } else {
            changes.push(format!(
                "Ignored untargeted question {id}: no open/missing part"
            ));
            continue;
        }
        let q = Question {
            target,
            id,
            text,
            suggestion: suggestion::read(o.and_then(|o| o.get("suggestion"))),
        };
        if q.id.starts_with("scope-addition-") || q.id.starts_with("goal-refinement-") {
            return Err("Model attempted an application-owned question ID".into());
        }
        if questions.iter().any(|old| same_question(old, &q))
            || request.answered.contains(&q.id)
            || request
                .settled
                .iter()
                .any(|s| same_question(&s.question, &q))
            || request.skipped.iter().any(|s| same_question(s, &q))
        {
            changes.push(format!(
                "Excluded duplicate/answered/skipped question {}",
                q.id
            ));
        } else if q.target.is_some_and(|p| {
            request.skipped.iter().any(|s| s.target == Some(p))
                || (request.ask_counts.get(&p).copied().unwrap_or(0) >= 2
                    && !request
                        .previous
                        .as_ref()
                        .is_some_and(|old| old.questions.iter().any(|s| same_question(s, &q))))
        }) {
            changes.push(format!(
                "Part question {} left open for seed: skipped or asked twice",
                q.id
            ));
        } else {
            questions.push(q);
        }
    }
    if questions.len() > 1 {
        changes.push("Extra questions dropped; one targeted question".into());
        questions.truncate(1);
    }
    let mut alternatives = vec![];
    for (i, v) in items(obj.get("alternatives"), "candidates", changes)
        .iter()
        .enumerate()
    {
        let o = v.as_object();
        // A live reply keyed each approach `option`; reading only `label` dropped them all.
        let option = o
            .filter(|o| !o.contains_key("label"))
            .and_then(|o| o.get("option"));
        if let Some(o) = o {
            let known = if option.is_some() {
                [APPROACH, &["option"]].concat()
            } else {
                APPROACH.to_vec()
            };
            extras(o, &known, &format!("candidate {i}"), changes);
        }
        if option.is_some() {
            changes.push(format!("Read candidate {i} label from its option field"));
        }
        let Some(label) = display(
            o.and_then(|o| o.get("label")).or(option).or(Some(v)),
            &format!("candidate {i} label"),
            changes,
        ) else {
            continue;
        };
        let mut optional = |key: &str| {
            display(
                o.and_then(|o| o.get(key)),
                &format!("candidate {i} {key}"),
                changes,
            )
            .unwrap_or_default()
        };
        alternatives.push(Approach {
            label,
            benefit: optional("benefit"),
            cost: optional("cost"),
            undo_cost: optional("undo_cost"),
        });
    }
    let mut misfits = vec![];
    let mut unresolved_notes = vec![];
    for value in items(obj.get("supports"), "uncited supports", changes)
        .into_iter()
        .chain(
            items(obj.get("open"), "open prose", changes)
                .into_iter()
                .filter(|v| v.as_str().is_some_and(|s| s.split_whitespace().count() > 1)),
        )
    {
        if !value
            .as_object()
            .is_some_and(|o| ANCHOR.iter().any(|key| o.contains_key(*key)))
        {
            let text = value
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| value.to_string());
            if let Some(text) = display(
                Some(&serde_json::Value::String(text)),
                "uncited note",
                changes,
            ) {
                unresolved_notes.push(text);
                changes.push(
                    "Retained narrative support/open prose as uncited note; no highlight".into(),
                );
            }
        }
    }
    for (i, v) in items(obj.get("misfits"), "unresolved", changes)
        .into_iter()
        .chain(items(
            obj.get("unresolved_notes"),
            "unresolved notes",
            changes,
        ))
        .enumerate()
    {
        let object = v.as_object();
        if let Some(o) = object
            && !ANCHOR.iter().any(|k| o.contains_key(*k))
        {
            extras(o, &["text"], &format!("unresolved note {i}"), changes);
        }
        if object.is_some_and(|o| ANCHOR.iter().any(|k| o.contains_key(*k))) {
            misfits.push(anchor(&v, &format!("unresolved {i}"), changes)?);
        } else if let Some(note) = display(
            object.and_then(|o| o.get("text")).or(Some(&v)),
            &format!("unresolved note {i}"),
            changes,
        ) {
            unresolved_notes.push(note);
            changes.push(format!(
                "Retained unresolved {i} as an uncited note; no source highlight"
            ));
        }
    }
    unresolved_notes.extend(decision_notes);
    let guess = Guess {
        parts,
        other_goals,
        deferred,
        open,
        uncertain: false,
        framings,
        outcome,
        misfits,
        unresolved_notes,
        decisions,
        questions,
        alternatives,
    };
    guess.validate(request)?;
    if guess.questions.len() > 1 {
        changes.push(format!(
            "{} questions retained; up to 1 presented, remainder available in details",
            guess.questions.len()
        ));
    }
    Ok(guess)
}
pub(super) fn read(raw: &str, changes: &mut Vec<String>) -> Result<serde_json::Value, String> {
    let text = raw.trim();
    let text = if text.starts_with("```") && text.ends_with("```") {
        changes.push("Removed surrounding JSON code fence".into());
        text.split_once('\n')
            .map(|(_, s)| s[..s.len() - 3].trim())
            .unwrap_or(text)
    } else {
        text
    };
    serde_json::from_str(text)
        .or_else(|original| {
            let normalized = without_trailing_commas(text);
            if normalized == text {
                return Err(original);
            }
            let value = serde_json::from_str(&normalized)?;
            changes.push(
                "Removed trailing JSON collection comma outside strings; raw retained".into(),
            );
            Ok(value)
        })
        .map_err(|e| format!("Unreadable board response: {e}"))
}
/// `text` without commas that close a JSON array or object; commas inside strings stay.
fn without_trailing_commas(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut quoted = false;
    let mut escaped = false;
    let mut out = Vec::with_capacity(bytes.len());
    for (i, &byte) in bytes.iter().enumerate() {
        if !quoted
            && byte == b','
            && bytes[i + 1..]
                .iter()
                .find(|b| !b.is_ascii_whitespace())
                .is_some_and(|b| matches!(b, b'}' | b']'))
        {
            continue;
        }
        out.push(byte);
        if quoted {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                quoted = false;
            }
        } else if byte == b'"' {
            quoted = true;
        }
    }
    String::from_utf8(out).expect("Removing ASCII commas preserves UTF-8")
}
fn extras(
    obj: &serde_json::Map<String, serde_json::Value>,
    known: &[&str],
    where_: &str,
    changes: &mut Vec<String>,
) {
    for key in obj.keys().filter(|k| !known.contains(&k.as_str())) {
        changes.push(format!(
            "Ignored extra {where_} field {key:?}; retained in raw evidence; never authority"
        ));
    }
}
pub(super) fn items(
    value: Option<&serde_json::Value>,
    where_: &str,
    changes: &mut Vec<String>,
) -> Vec<serde_json::Value> {
    match value {
        Some(serde_json::Value::Array(v)) => v.clone(),
        None | Some(serde_json::Value::Null) => {
            changes.push(format!("Absent {where_}: empty collection"));
            vec![]
        }
        Some(v) => {
            changes.push(format!("Accepted singleton {where_}"));
            vec![v.clone()]
        }
    }
}
fn display(
    value: Option<&serde_json::Value>,
    where_: &str,
    changes: &mut Vec<String>,
) -> Option<String> {
    let Some(s) = value
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
    else {
        changes.push(format!("Absent/unreadable optional {where_}"));
        return None;
    };
    let escaped = s
        .chars()
        .map(|c| {
            if c != '\n' && c.is_control() {
                changes.push(format!("Escaped display control in {where_}"));
                c.escape_default().to_string()
            } else {
                c.to_string()
            }
        })
        .collect::<String>();
    Some(escaped)
}
fn anchors(
    value: Option<&serde_json::Value>,
    where_: &str,
    changes: &mut Vec<String>,
) -> Result<Vec<Anchor>, String> {
    items(value, where_, changes)
        .iter()
        .enumerate()
        .filter(|(_, v)| {
            v.as_object()
                .is_some_and(|o| ANCHOR.iter().any(|key| o.contains_key(*key)))
        })
        .map(|(i, v)| anchor(v, &format!("{where_} {i}"), changes))
        .collect()
}
fn anchor(
    v: &serde_json::Value,
    where_: &str,
    changes: &mut Vec<String>,
) -> Result<Anchor, String> {
    let o = v
        .as_object()
        .ok_or_else(|| format!("Invalid claimed source span at {where_}"))?;
    extras(o, ANCHOR, where_, changes);
    let source = o
        .get("source")
        .and_then(|s| {
            s.as_u64().or_else(|| {
                let text = s.as_str()?;
                if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
                    return None;
                }
                let source = text.parse::<u64>().ok()?;
                changes.push(format!(
                    "Accepted decimal-string source reference at {where_}"
                ));
                Some(source)
            })
        })
        .and_then(|s| usize::try_from(s).ok())
        .ok_or_else(|| format!("Invalid source reference at {where_}"))?;
    let quote = o
        .get("quote")
        .and_then(|s| s.as_str())
        .ok_or_else(|| format!("Missing exact source quote at {where_}"))?
        .to_owned();
    let occurrence = match o.get("occurrence") {
        None => {
            changes.push(format!("Defaulted {where_} occurrence to zero"));
            0
        }
        Some(v) => v
            .as_u64()
            .and_then(|s| usize::try_from(s).ok())
            .ok_or_else(|| format!("Invalid source occurrence at {where_}"))?,
    };
    Ok(Anchor {
        source,
        quote,
        occurrence,
    })
}
