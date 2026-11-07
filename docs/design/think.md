# Think

## Shipped experience

The blank writing box accepts typing or paste without fields, kinds or cards.
Only F2 interprets; Enter is a newline. Originals stay intact on the left.
Below 80×24, the size warning holds F2 for typed and dictated drafts. Resizing back
retains the words and requires a fresh F2 press before sending.
One “I think you mean…” reading and at most one focused question sit on the right.
Competing interpretations get a disambiguating question, not a second reading.
Model-host construction and every model call require an explicit budget. The app
shares one launch allowance across Think and Shape, defaulting to zero;
[terminal setup](../terminal-setup.md#launch) defines what it counts.

### Questions

Use the [goal-block contract](goal-block.md). Ask the most consequential unresolved
operator-only question that would change the goal: missing intent, target, scope,
boundary or protected things, never discoverable facts. Missing fields do not
impose question order; a missing beneficiary or reason alone need not prompt a
question. Explicit starting scope stays chosen, not a new choice against the wider
ambition. Ask only when the answer matters to the next useful step; remaining
unknowns may travel to the seed. Recording observations or declining to define a
goal does not manufacture a goal or another question. Originals remain intact.
Decisions contain a part, text, at least two answers and an optional question link.
Ignore invalid parts or answer counts. Open decisions keep readiness pending
and travel to review and the seed. Review shows a linked
question once; otherwise it shows an Open decision note. Alternatives survive
handoff even when a question replaces the note.

If the selected missing-success question is bounded by proposed alternatives, the
board opens it to “What would tell you this worked?” without repeating the headline
or offering an exhaustive menu. Other specific questions retain their wording.
It retains the selected question's identity and updates only its linked decision.
Other questions and choices keep their meaning and priority.
A missing success field, an unasked decision or a short answer creates no follow-up.
Alternatives, missing intent, skips and the two-ask limit still travel to review and
the seed; readiness is not a completeness judgment. The
[frozen Stacker replay](../../tests/fixtures/stacker-226-notes.md) preserves the
recorded observations without imposing an answer-length rule.

After shaping, a separate review sees only words, answers, draft parts and shaping
decisions, never the template. It finds silently settled choices about scope,
irreversible actions, protected things, persistence or whose rules apply. The
reading decoder joins decisions; raw recorded replies stay unchanged. Only explicit,
valid shaping indices for the same part let review replace decisions. Uncovered
choices remain, even on the same part. Neither text similarity nor a shared part
justifies deduplication. After cleanup, review choices lead when required parts
exist; otherwise the top shaping question keeps its priority. Questions about
filled, non-open parts are dropped. A missing unrelated field does not replace a
valid question. Failed or unreadable review leaves shaping intact.

Stop when the target and irreversible boundaries are settled. Ask no already-covered
edge cases or implementation questions. Answers belong only to their part. Notes
hold open intent, not evidence or status disclaimers. Evidence authorship, test
history and unreproduced failures stay in originals, not invented requirements.
Explicit requirements and undecided success checks or permissions still belong in
goals or questions.

Answers and added dumps remain separate exact sources. Sending refines even a ready
reading, never starts another dump. Skips persist; F1 brings the last one back. Separate
goals stay aside, not in each other's constraints; deferred items remain seed context.
Constraint-polarity errors remain possible; decision coverage is a model
judgment, not proof that choices mean the same thing. See [#64 evidence](../../findings/README.md#goal-history-and-review-regression-check-64).
Originals are plain ink. Exact cited spans validate invisibly; uncited words are
not implicitly rejected. Decimal-string source IDs are decoded as numeric IDs and
noted in Why; the source must still exist and the quote and occurrence must match
an exact, grapheme-safe span. No other source-ID coercion or citation inference runs;
recorded raw replies and operator originals stay unchanged.

### Suggested answers

The current question shows one provisional answer and its reason, or says no suggestion
is available. Shaping and review use original words and submitted answers to propose it;
insufficient context must yield no proposal. Scope questions use the unavailable state.
F1's “Use the suggested answer” copies a suggestion into an empty writing box; F2 alone sends it.
Existing drafts are preserved. Changed questions discard their old suggestion, including
questions rewritten by board validation. Missing or malformed proposals leave questions usable.

Suggestions are omitted from subsequent request and handoff context. A separate `suggested`
history event preserves what was shown; only a submitted answer becomes a source. Schema 3
readers here accept older records; older binaries may refuse the new event rather than guess.
This stays within the existing two-call ceiling. [#330](https://github.com/jon-devlapaz/tinkery/issues/330)
holds the research and deferred live comparison; zero-call checks establish mechanics, not
recommendation quality or saved operator effort.

### Taste drafts

[Taste](taste.md#after-external-acceptance) may offer a next dump after an explicit
report of external acceptance. The menu opens a fresh Think visit with that draft,
never submits it. Before F2 it stays italic and labelled “Tinkery's guess”; edits
remain unsent. The usual editing, copying, leaving and preflight rules apply.
F2 commits exactly the submitted text as the sender's words; the launch declaration
still owns confirmation attribution. Merely opening or abandoning the draft writes
nothing and calls no provider.

A separate `draft_suggested` in-memory turn retains the exact offered text and
run/seed/PR/head provenance, including its deterministic basis and unverified
acceptance-report limit. It never enters model context; only submitted words do.
On later typed confirmation the ordinary schema-3 record retains both events.
Existing records remain readable; older readers may refuse the new event rather
than mistake a proposal for original words. Tend history labels the offered draft
separately. The prior goal remains unchanged; no suggestion confirms a goal or seed.

### Drafting runs

Each live send uses at most two drafting runs: shaping, then decision review. While a run
is in flight, the initial reading or refinement status says `Thinking… (run 1 of up
to 2)`, then `Reviewing decisions… (run 2 of up to 2)`. These name phases, not a
claim that both runs ran; unreadable shaping skips review. Practice claims no
drafting runs: its status says `Simulating… (practice: no model)` (#614). Scope questions may use none. No runs follow display, and nothing
retries automatically. With a model, Right now help and a failed reading say
`Drafting runs left this launch: N of M.`, the launch allowance Shape shares (#435). The
header label shows the runs left; F1's Model and runs row opens the
[Model panel](menu.md#model-setup), where Now adds runs for this launch only. Exhaustion
keeps the words and starts nothing; the refusal says to add runs. The
[Model panel contract](menu.md#model-setup) defines what a run counts and why it does
not cap spending.
Practice and verification use fakes; [historical evals](#decision-review-evidence-34)
promise no general quality.

`src/think/brain_dump/prompts/` holds one rule per line and a finding or eval note
per tuned rule. Notes never reach models. Tests pin sent Pi bytes and decoder
field names. Prompt changes require deliberate evaluation and an operator budget.

### Readiness and review

With no focused question (none left, skipped or a part asked twice), show one muted line:

| State | Line |
| --- | --- |
| No composed goal | No goal yet. |
| Ready, review available | Ready for your review. F1, then 1: Review the goal. |
| Ready, review unavailable | Ready for your review. |
| Required part missing | Still missing: Done when. Review when you're ready. (Name the first missing part: who, outcome, why, done when.) |
| Required parts present, other intent open | Some parts remain open for the seed. Review when you're ready. |

Ready describes the recorded parts and open questions, not complete coverage of the
person's intent. The [#401 replay](../../tests/fixtures/think-delivery-401-notes.md)
preserves a reading that moved explicit delivery constraints from the main goal to
“also in your dump,” and the correction that restored them to the goal. It checks
review and recovery mechanics, not model quality;
[#401](https://github.com/jon-devlapaz/tinkery/issues/401) retains that evaluation work.

Without a composed goal, omit the review invitation. F1 uses the same first sentence
and names “1: Review the goal” only with a saveable current goal: no unsent words, held reply or unfinished
reading. Listing and entry share these prerequisites; direct entry keeps its refusal messages.
In practice or without a sessions folder, help says
saving needs real mode and a sessions folder. With no goal, offer adding words then F2,
without a missing-field interview. If the first reading was unapplied or cancelled,
offer an F2 retry. Readiness text depends only on goal
content, including sent-but-unapplied words and unanswered skips, never recording
or unsent text. Review lists all open intent.

Choosing “Review the goal” from F1 deliberately reviews. Typed `confirm` plus Enter freezes the displayed goal and,
after successful saving, shows the [receipt](shape-and-handoff.md). Missing parts
and questions may travel to the seed. Readiness is not required to save; saving
is neither seed nor implementation approval.
Review reserves the confirmation input row and normal instruction height, so typing
or correcting `confirm` does not move an already-read end. Unread content still prevents saving.
Until the current review reaches its end, ordinary instructions, the key bar and review help
name PgDn before confirmation. Once it fits or reaches the end, they offer the existing typed confirmation.
Below 80×24, the size warning holds review input and scrolling; no new save can start.
Resize is painted before queued keys and paste resume; mouse events from the old layout are ignored.
Existing input, goal and attribution stay intact;
a save already started finishes normally, including a requested return to the menu.

## Confirmation attribution (#185)

The operator accepted option (c) in [#185](https://github.com/jon-devlapaz/tinkery/issues/185#issuecomment-6237481270):
an agent's confirmation is recorded as the agent's, never the operator's. Under
[#349](https://github.com/jon-devlapaz/tinkery/issues/349) the launch declaration alone
attributes an agent's writes; the context file with door and blast-radius claims is gone.
Tinkery's confirmation never grants implementation permission: tink-sdlc's human-only
gates and the operator's merge hold that line. Goal confirmation, seed confirmation and
implementation approval remain separate.

`BrainDump::set_goal_confirmation` supplies caller-declared attribution before review.
Refused updates preserve the current words, review and attribution. Neither keypresses,
login names nor the model producing the reading identify the confirmer. Without explicit
context, identity is `unrecorded`, never operator. API callers may still supply named
operator or agent context; a named agent needs a two-way/small class with reasons, and
incomplete or ineligible declarations refuse. Named context freezes with the displayed
goal, stays outside model requests and never carries into a fresh Think visit.
Original words and the confirmed goal remain unchanged.

Think's review shows supplied attribution outside the goal. The save source describes
the actual confirmation action without claiming the operator typed an agent's words.
`write_with_confirmation` records the same frozen attribution; ordinary `write` records
unrecorded identity. Readers disclose attribution as caller-declared technical detail,
never as authentication or implementation permission.

The CLI accepts `--operator human|agent` as an explicit launch declaration, frozen
across Think visits in that terminal session. Header and confirmation review show
`declared: human`, `declared: agent` or `not declared`; none is verified identity.
The record retains that exact declaration, `typed confirm at this terminal`, and
`declaration is not verified identity`, never an operator identity inferred from
keys. No declaration stays unknown; existing records are not backfilled. Launch
context cannot be replaced by named attribution through the API once installed. It never enters
model requests and grants neither seed confirmation nor implementation permission.

**Agent operation:** `--operator agent` is the whole agent context. Think saves, Shape
answers and F2 seed confirmation all write as the agent: `operator: agent` ledgers,
delegated authority, a goal origin delegated to `agent`, and action reasons that begin
`The agent`. Nothing an agent types becomes a human save. Shape writes need
`--shape-pi`; without it an agent's Shape only reads, since its writes would carry no
agent attribution. The declaration grants no model budget, private-data access or
implementation approval.

One shared authorization boundary resolves a typed actor before goal saving or Shape
writes. Tinkery's own Rust ledger saves and reads back delegated goal origins; no Seed Me
checkout or helper is used. Version-3 seed confirmation receipts carry the actor (the
terminal declaration, or named context from older receipts), bound to confirmed seed
bytes and checked against the seed's attribution line. Missing, malformed or changed
attribution refuses verification. Human version-2 receipts, named agent receipts and
legacy histories remain valid. New ledgers derive `operator: human|agent` from the same
actor as goal and answer attribution. Historical agent sessions stored as
`operator: human` remain readable; they are not rewritten.

Shape and Approve offer manual intake commands for verified agent confirmations, labeled
`Agent-confirmed seed`, with a tink-sdlc 1.38.0-or-newer requirement (1.36.0 accepts only
named receipts). That consumer accepts v3 receipts as agent, never human. Tinkery only
prepares and copies text: it does not check the installed consumer version, run intake or
grant permission. Missing or invalid attribution remains ineligible. The consumer's
human-only gates and merge remain unchanged; seed confirmation is neither the operator's
confirmation nor implementation approval.

## Think record

Typed `confirm` also writes one pretty JSON `<UTC confirm time>_<session folder>.json`
in `~/.local/share/tinkery/think/`, overridden by `--record-root`. Without an explicit
root, unset, empty or relative HOME refuses `--shape-pi` startup and asks for a root,
rather than writing under the current directory. The review and bar label
`type confirm` → `save goal and record` discloses the write. Unconfirmed exits,
including crashes, leave no record.

Schema 3 has an additive optional `goal_confirmation` object: `by` is `unrecorded`,
`terminal`, `operator` or `agent`. `terminal` retains `declaration`, `action` and
`identity` as the exact strings above, not a named actor or delegation.
Known actors carry their name, exact source and class (door,
blast radius and a reason for each), and an agent also carries `authorizing_operator`.
This is separate from the reading's `origin`. Missing metadata in existing records
means unrecorded identity; no old record is rewritten or backfilled. Existing schema-2
and schema-3 histories remain readable and matching schema-1 records remain unsafe
to attribute, as before. Readers predating #209 ignore the added field; #209 readers
refuse the new `terminal` variant rather than guess an identity. Malformed
present attribution refuses history and batch repo attribution rather than choosing
an operator or silently discarding it. Record/session identity, exact-goal checks,
duplicate refusals and per-file limits are unchanged.

Schema 3 retains the version, origin (`live` for a model host, otherwise `simulated`),
harness (`pi` or `claude-code`; older records lack it and read as not recorded), model,
thinking level, both prompts' hash, first-turn and confirm times, session folder,
canonical path, source ID, launch Git root `repo` (or null), exact confirmed goal,
every ordered, timed, verbatim turn (`words`, `asked`, `answered`, `skipped`,
`restored`, `scope_asked`, `scope_answered`) and serialized Why checks.
Rejected checks retain `raw`; passing checks use decoded `candidate` and `raw: null`.
Each Think visit gets separate model diagnostic history. Reused settings start no
process or file read and cannot mix goals. Standalone return and reentry preserve
the record folder.

`repo` means made in, not about. Review shows `made in <folder>` before confirmation.
It never enters the goal, session ledger or model requests. Observations stay
separate from operator words.

Shared `src/think_record.rs` owns the format; Conversation owns turns. Write only
after ledger saving and readback succeed. Record failure leaves the goal saved
and adds a receipt notice; the reason and path live in How it works. Think never
reads records. This is not the session's `ledger.json`. Tend's deliberate
[history](shape-and-handoff.md#think-history) adds no model context.
Eval record reading and gap flags are separate slices.

### Presented readings and subsequent words

Optional `reading_context.events` keeps terminal presentations and submitted-source
references in one ordered list. Each `presented` entry holds a UTC `at` and the frozen
verified `board` associated with that frame, including its composed framing text.
Each `submitted` entry names the existing `source`; its words and timestamp stay in
`turns`. Its context is the preceding presentation, or none before the first.
List order establishes sequence even when wall clocks move backward. This records
what preceded submitted words, not a claim that they corrected it or why.

A presentation is acknowledged only after frame output and ending synchronized output
succeed, with nonblank reading content visible in the reading pane or goal review.
Help, other covering panels, originals, minimum-size notices and failed output do not
qualify. Repainting an acknowledged board adds nothing; a newly applied board can
qualify even when its wording is identical. A pending or applied-but-unpainted reply
cannot replace the last recorded presentation. The snapshot can include offscreen
material: this is neither full-screen replay nor proof that someone read it.

Collection stays in memory until existing typed confirmation writes the record.
Unconfirmed exits and crashes still leave no record. Invalid optional context is saved as
`reading_context: null` with a sibling `reading_context_error` explaining why. The writer
returns that nonfatal warning for the receipt; the confirmed goal and originals still save.
Missing context, rejected context and malformed saved context remain distinct in
[history](shape-and-handoff.md#think-history), without invalidating independently valid
originals or goal attribution. Unrepresentable UTC conversions reject context, never panic.
Technical details retain exact recorded timestamps, full frozen boards and the same submission
relationships as the sequence.
References must cover committed sources exactly once in order. Historical boards validate
required framing-text, other-goal and deferred lists into typed text once, requiring at least
one nonblank string; they never pass through today's model decoder. Writer and reader share
the 16 MiB complete-record limit; an oversized record fails visibly without truncating originals.

`tinkery_source` holds optional compile-time supplied build information. Set
`TINKERY_BUILD_COMMIT` to the full 40-hex Tinkery commit and `TINKERY_BUILD_DIRTY`
to `true` or `false` when building; Cargo tracks changes to these inputs. Missing or
malformed inputs record null, shown as unknown. This is supplied information, not
authenticated provenance; a modified build identifies only its base commit. Ordinary
unstamped builds remain unknown. Saving never reads Git from the application's folder.

## Interaction contracts

### Reading

Why and the copied provisional board label the Done when part **Success criteria**;
when absent, they say **Not yet captured**, not that the desired experience was
unsupplied. Rejected-reply summaries label raw `parts.done_when` as unaccepted
success criteria; the original diagnostic evidence stays unchanged.

Overflow keeps muted `PgUp/PgDn scroll reading` below the reading. The active
question grows to fit above the answer while retaining reading and input space.
If it still overflows, `PgUp/PgDn scroll question` names the same scroll route;
keys scroll both overflowing regions, and the wheel works over either. Keys and
the wheel reveal all before review; How it works names the keyboard route. Hide each
hint when its content fits, including after resize. Scrolling changes no goal,
original or unsent answer text.

### Help

Think and Shape share one key map ([#440](https://github.com/jon-devlapaz/tinkery/issues/440)):
F2 runs the one action the key bar names, F1 lists every other action allowed now, and Esc
goes back. F6 (or Ctrl+Alt+Z) speaks; editing keys stay. No other function key or Ctrl
alias acts. A row appears only when its action is allowed, so nothing in the list refuses:
review needs real mode with a sessions folder. Empty help says “Type, then F2 sends it.”
F1 opens bounded reading-side help without changing originals or drafts. Right now leads
with the numbered rows (a digit runs its row once and closes help), then the state; `h`
opens How it works and F1 closes. Labels fit one line of an 80-column panel; a short
panel drops its blank rows and packs its keys so the rows stay above the writing box.
Lists longer than nine actions show a fixed page hint: Left/Right changes pages;
1–9 chooses from the displayed page. Reopening F1 starts at the first page.
The layer title is jade, headings and keys bold ink, and labels and borders muted.
Short pages fit; long pages scroll under a fixed title and navigation. Show the
scroll hint only on overflow. Empty technical sections add no gap before Last result.

Esc first cancels voice, otherwise closes or backs from technical help. Reading
cancellation remains after help closes. Help blocks typing, paste and hidden send,
review or voice actions. Ctrl-C cancellation stays, and `b` changes the preference without revealing the hidden bar. `?` remains
text while writing. Once the review reaches its end, its bar says `type confirm save goal and record` when recording.

### Editing and focus

- Tab or Shift-Tab toggles writing and the original once present. Typing returns
  to writing and clears original selection. Letters never command; skipping and copying are F1 rows.
- Shift with arrows, Home or End, and mouse drag select logical whole graphemes.
  Ctrl-A selects only the active surface. Writing input, Enter, deletion or paste
  replaces selection as one edit. There is no numeric editor-size cap. Allocation
  refusal preserves text and selection. Finite fixtures promise neither unlimited
  memory nor responsiveness.
- Ctrl-E extracts; F1 shows or hides the cards. Covered originals have no selection
  and cannot be copied.
- Ctrl-C copies a nonempty active selection, otherwise cancels a request or does
  nothing; it never exits. Dictation never deletes selection.

### Original bytes and display

Writing, sources and cards retain received UTF-8, including CRLF, CR, tabs and controls.
Copy uses base64 OSC 52, not executable source. The host owns clipboard support and
permission. LF starts rows; CR displays as ␍, tab as ⇥, ASCII controls as control
pictures, and C1 as �. CRLF is one grapheme. Display-only marks map cells to raw
grapheme offsets for motion, selection, highlighting and voice preview, never
substring reconstruction. Rows use machine-sized integers; only the visible viewport
narrows to terminal coordinates.

Literal ESC `[201~` ends bracketed paste and cannot occur inside its envelope.
Crossterm replaces invalid UTF-8 before Paste delivery. Retain received strings,
not unavailable predecode bytes. Valid-UTF-8 complete-envelope tests include OSC
sentinels: bytes survive sending and copying, never execute on display.

### Sending and leaving

- The encoded request limit for Think and Shape is 128 KiB (`pi::REQUEST_LIMIT`), including
  metadata, history and the waiting reply's refreshed reading. It was 32 KiB, about 8K
  tokens, until a long goal's Shape step was refused at 59,795 bytes (#593). 128 KiB is about
  32K tokens, well inside current context windows and at the low end of where long-context
  quality is reported to fall off. Replies stay capped at 32 KiB (`pi::LIMIT`). One shared preview measures the final payload
  as it grows or shrinks. Idle frames reuse it until input, extraction, voice or
  a reply arrives. Unapplicable replies make size unknown; F2 preserves the old
  reading and sends one retry. Transactional preflight refusal starts no host and
  commits neither words nor the waiting reply. Copy a fitting replacement with
  Ctrl-A then Ctrl-C, deliberately replace it and send once. If retained context
  alone exceeds the limit, new words cannot fix it. F1 explains copying local words
  and originals using Tab, Ctrl-PgUp or Ctrl-PgDn, then Ctrl-A and Ctrl-C.
  Copying never prunes history or restarts.
- Distinguish provider input or context rejection, rate limits, timeout and overflow.
  Shaping failure preserves committed originals and the prior reading. Failed,
  unreadable or oversized review is best-effort: keep valid shaping and explain the
  skipped review in Why. Never automatically retry, chunk, summarize or send.
- With nothing to send or retry, F2 says “Write something, then F2 sends it.” on the
  initial or refining screen, without requesting or saving. The next input clears
  it; there is no permanent warning.
- One `refuse` route explains ineffective keys and recovery under writing or in the
  review's confirm slot. It takes at most two narrow-pane rows and overrides
  Thinking and receipts. The next key or paste clears display; Last result retains
  it. Review names unsent words, a waiting reply or an in-progress reading.
  Enter before the reading's end says PgDn. Confirmation saves only from the end.
- An unfinished save says “Handoff not complete. F1 then h shows why; Enter retries.”
  Last result retains the reason. Review messages grow, leaving Back its own row.
- Idle Esc guards unsaved work. `--home` returns to the menu; standalone
  exits before saving, opens the menu after saving. Esc first cancels voice or the
  reading, closes overlays, or leaves review. In the original, Esc clears a
  selection first; with no selection it returns to writing, preserving the draft.
  The next Esc asks before discarding; Esc at that prompt declines.
- Originals and Why are F1 rows. [Local voice](voice.md) inserts editable
  words, never sends. Answering keeps the original dump visible.

## Published proposals

Published proposal scope and current permission live in the linked GitHub issues:
[multiple-choice answer research](https://github.com/jon-devlapaz/tinkery/issues/426)
and [evidence on demand](https://github.com/jon-devlapaz/tinkery/issues/428).
Neither issue authorizes implementation.

## Decision-review evidence (#34)

See [historical evidence](../../findings/README.md#decision-review-evidence-34).

## Evidence-note regression check (#58)

See [historical evidence](../../findings/README.md#evidence-note-regression-check-58).

## Goal-history and review regression check (#64)

See [historical evidence](../../findings/README.md#goal-history-and-review-regression-check-64).
