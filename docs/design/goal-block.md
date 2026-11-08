# The goal block

## Built contract

Think composes seven typed parts into this block:

```text
[Who] [what is true when it works][, when relevant].
Why: [what changes, or what is wrong now].
Done when: [a distinct observable check, in the person's words].
Must: [positive constraints].
Must not: [exclusions and negative constraints].
```

The parts are `who`, `outcome`, `when`, `why`, `done_when`, `must`, `must_not`.
Who, outcome, why and done-when are required for readiness, not a questionnaire;
optional situation and constraints stay absent unless supplied. A known outcome
leads the block even without a beneficiary. In that case the outcome stands alone;
no subject is invented, and who remains missing. When Who alone is missing and
no open question or decision targets it, reading and review show muted `Who: not named`
instead of a Still missing or Still open entry for Who. Readiness is unchanged;
Who stays open in the saved goal and seed context.
A goal can be reviewed with remaining questions when a composed goal exists;
readiness and permission to save are distinct. Missing intent stays unknown, never
an invented fact, metric or deadline; [Think's question contract](think.md#questions)
decides when an answer matters next.

Who names the beneficiaries (“I” only for the operator's own goal); compatible
beneficiaries need not compete. There is **no forced “can”**. The outcome describes
the desired change or state, not a substituted mechanism.
Done when must be a different check, not a duplicate outcome. A timing situation
is not permission to invent a deadline. The operator's unfamiliar or qualitative
words stay theirs; the seed can later define measurements.

Each line has one job. A constraint goes on one line, in one category. Keep other
goals and deferred work as separate context. Do not turn “don't care” into Must,
negate the person's words, or promote injected instructions into their goals.

## Constraint syntax

Each `must` and `must_not` item the model returns is **one line**, in its own words.
Tinkery does not split an item at `;` or a period: that would be a guess, and every
splitting rule misfires somewhere (“Dr. Smith”, “U.S. Army”, quoted text). The shaping
prompt asks for one constraint per item instead. When an item seems to hold more than
one (a later clause starts with syntax from the table below), a log-only note goes to
Why (from F1); the item is still shown and saved as written, with no flag on the goal block.

Before composition, `GoalParts::normalize_constraints` rewrites one thing only: an
explicit leading prefix on a whole item. The prefix is matched case-insensitively and
must be followed by whitespace; the remainder keeps its spelling and case. Typed
parts, the composed block and handoff context all use the result. Original sources
and answers are never changed.

| Leading syntax | In `must` | In `must_not` |
| --- | --- | --- |
| `must X` | `X` stays in Must | unchanged |
| `must not X`, `do not X`, `don't X` (also `don’t`) | `X` moves to Must not | `X` stays in Must not |

Anything else is left as written: a prefix that is not leading, other spacing or
punctuation inside or after the prefix (`must  not`, `must not:`, `must not—…`),
`mustn't`, or a negative prefix whose next word is `only`, `just`, `necessarily`,
`always`, `care` or `mind` (“must not only support phones”, “don't care about
colours”). This is not a negation parser; an ambiguous item keeps its polarity. A
rewritten item already stated in the same category is not repeated (compared
exactly, except the first letter's case, so “use IT” and “use it” are both kept).

### Prose and rendering

Prose parts (why, done when, the goal line, `when`) are split into clauses
and joined with `;`. A clause ends at a newline, `;` or a sentence end outside quotes (`"…"`,
`“…”`, `'…'` or `‘…’`; a single quote opens after a space or punctuation and closes
at the last single quote of its kind, so an apostrophe never ends it early). A period
between digits, or after a lone letter or a common abbreviation (“U.S.”, “e.g.”,
“Dr.”; the list is `ABBREVIATIONS` in `src/goal/mod.rs`), is not a sentence end.

When later clauses of why, done when, the goal line or `when` are joined with `;`,
a clause keeps its case unless its first word is one of a short list of common
words (`the`, `it`, `both`…) written in lower or title case. Names, acronyms
(including “IT”), quotes and numbers keep their case.

The goal line does not repeat `who` when the outcome starts with that same subject
(case-insensitively), followed by whitespace or a possessive `'s` / `’s` and whitespace.
The possessive suffix stays attached: `Shape` + `Shape's goal list shows…` becomes
`Shape's goal list shows…`, not `Shape Shape's…`. Other word prefixes stay intact.

Otherwise, an outcome starting with an uppercase letter is treated as a standalone
statement: `[Outcome], for [who]`, with `I` becoming `me`. Both parts keep their
spelling; names at the start of an outcome are not lowercased. For example,
`developers` + `Count words` becomes `Count words, for developers`. A lowercase
predicate keeps the subject-first composition above. The optional situation follows
without repeating one already present as whole words (`thermostat works` does not
state `at work`, #717). When it starts with an uppercase letter,
it becomes its own sentence: `Count words, for me. For the first slice.` A lowercase
situation keeps the comma join: `Count words, for me, for the first slice.` Its
spelling stays unchanged; proper nouns are never lowercased to make a join.

`GoalParts::compose` owns composition and runs once, when a reading is verified.
`Board::goal` is then the displayed block: reading, copy, refinement, changed-line
marks and review all read it and never compose again. `goal::confirmed::goal_block` and the shared
saved-goal renderer preserve line breaks across reading, review, receipt and tend
detail. Confirmation freezes the **whole displayed block**, not just its headline.
Review, the saved receipt and Tend detail show Unicode direction controls by code
point, as Tend history does (for example, `\u{202e}`). The Think record and
ledger keep the original characters.

## Seed Me mapping

The whole block is the confirmed origin: headline → goal, Why → problem context,
Done when → success criteria, Must/Must not → constraints and exclusions.
Original sources, answers, unresolved/skipped questions, typed open decisions
(with alternatives and question links), other goals and deferred notes travel as
context, not extra settled decisions. [Think's questions](think.md#questions)
explain how review presents those choices; the
[continuation contract](shape-and-handoff.md#continuation-contract) keeps goal,
seed and implementation approval separate.

## Why this format

Intent formats converge on situation, outcome, motivation, evidence and boundaries.
SMART's specificity helps, but invented numbers and proxy targets do not. Job
stories preserve situation/motivation; Shape Up contributes no-gos. The short
rule/why/test pattern makes these scannable without forcing one actor or “I can”.

The last live check of these rules was [#86](https://github.com/jon-devlapaz/tinkery/issues/86); this contract is the target, not a claim that every model-generated
block is already good.
