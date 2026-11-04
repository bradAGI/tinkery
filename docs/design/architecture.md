# Architecture

Read [the constitution](../yohaku.md) first. This page owns module responsibilities,
dependency rules and the design score. [Architecture tests](../../tests/architecture.rs)
enforce the limits below; a limit may only get stricter.

## Phases and the dependency rule

Each built [place](menu.md) is a **phase**, with its own compartment:

- A phase uses only shared code, never another phase.
- Only the menu knows every phase.
- Shared code names no phase.

Each top-level module belongs to one area, listed in `MODULES` in the check. A new module must be listed
there, so a new phase can't count as shared code by default.

Published task scope and status live in GitHub Issues. Measure's former
[failure presentation](https://github.com/jon-devlapaz/tinkery/issues/119) and
[owned reader](https://github.com/jon-devlapaz/tinkery/issues/120) are retired.
`MODULES` owns area assignments; the map below describes responsibilities.

[Shape](shape-and-handoff.md#shape), [Approve](approve.md), [Watch](watch.md) and
[Taste](taste.md) have separate modules. There are no dependency exceptions: Think hands
saved goals to Tend only through `seed_me`. The menu alone connects Shape and Tend.

## Factory boundaries

[Factory principles](https://x.com/zachlloydtweets/status/2110505938604134737) are
context, not requirements or permission. Tinkery versions prompts, model boundaries,
tests and CI; external SDLC owns implementation orchestration. Tinkery neither
configures nor verifies whole-factory permissions, automations or sandboxes.

Pi uses an explicitly selected provider and model. [Manual continuation](shape-and-handoff.md#continuation-contract)
in Claude, Codex or Pi is not interchangeable execution inside Tinkery. Model
selection establishes neither open-weight compatibility nor comparative quality.

[Think records](think.md#think-record) retain confirmed-session evidence, not
unconfirmed sessions or complete factory telemetry. The read-only
[seed-to-run join](shape-and-handoff.md#sdlc-handoff) uses SDLC's source-owned record.

## Modules today

| Module | Owns |
| --- | --- |
| `main.rs`, `main/input.rs` | Command-line options, the terminal loop and bounded ordered input. Mouse input after resize waits for a source-level drain acknowledgement for that resize, never an empty transfer queue. Input and read completions wake the UI without shortening its heartbeat. Wires the menu or think to a real or practice host. |
| `lib.rs` | The palette (one job per colour) in light or dark, the header's light/dark switch and the model label's room, the minimum terminal size, paging and the clipped-view `more` cue. |
| `model_setup.rs`, `model_setup/panel.rs` | The saved model setup's one parser (file and flags), atomic save, header label and Model panel; the terminal loop owns when it shows (shared). |
| `pi.rs` | The one place a model host starts, Pi or Claude Code: which models and levels each accepts, a bounded, cancellable request with or without read-only tools held inside one folder ([`pi/folder.mjs`](../../pi/folder.mjs) for Pi, `dontAsk` for Claude Code), the call budget and prompt-file assembly (shared). |
| `repo.rs` | Finding the Git repository Tinkery was started in, by looking for `.git` only (shared). |
| `menu.rs` | The seven places and navigation, including Esc from Shape back to its original Tend view. |
| `measure.rs` | Recent PR list, details and evidence limits; explicit reads only, no writes or providers. |
| `measure/read.rs` | Bounded, cancellable `gh` reads scoped to the launch repo; no shell or retries. |
| `tend/mod.rs` | The saved-goal garden's discovery states, selection and detail, with navigation to Shape and saved Think history. |
| `tend/history.rs` | One owned history read at a time and a read-only conversation view with separate technical help. |
| `tend/linking.rs` | The source and target selection state, link refusals and derived needs wording, independent of garden drawing. |
| `tend/garden.rs` | The graph's framing: selected-line needs, read notes and explicit refusals. |
| `tend/graph.rs` | Graf adapter: path-keyed topology, deterministic layout, palette, labels, viewport and pointer geometry; no physics worker or writes. |
| `tend/discovery.rs` | Scanning for sessions off the paint path, with unknown, complete, incomplete and failed kept apart. |
| `tend::needs` | Tinkery-owned needs links, strict atomic persistence and readiness derived from readable session identities; never writes session files. |
| `seed_me/` | Session formats, goal saving and the one-time root migration; `answering.rs` adapts Shape actions, typed facts and operator review reasons to the shared ledger without duplicating its fact rules. Goal initialization uses the Rust ledger; exit receipts retain data until printing, when the shared verifier supplies the current next step. Contract guidance is bundled, and copied intake commands name this Tinkery executable (shared). |
| `seed_me/ledger` | Validation, transitions, explicit answer actions and cascading reopen, confirmation risks and completion-review coverage, byte-bound teach-back records, seed decisions and checks, confirmation receipts, verification and ending; shared writer lock and atomic saves. No phase policy, viewer or network connection. |
| `seed_me/shaping.rs` | Classifying ledger-owned status and review flags for display, with optional Tinkery-executable intake text; no subprocess or mutation. |
| `approve.rs` | One verified seed at a time, read-only commands and a retained verifier; no approval or launch. |
| `seed_me/ready.rs` | Consistent verified goal/acceptance snapshots and the one intake command; no mutation. |
| `shape.rs` | Goals awaiting seeds; each session's confirmed goal, stage, folder, words, draft reading and next step. Reads and steps stay off the paint path. |
| `shape/forms.rs` | Unsaved deferral and replacement-answer fields, navigation and cancellation; no validation or publication policy. |
| `shape/words.rs` | Shape's in-memory textarea adapter: editing, selection, cursor-following and original line-ending provenance; no save or agent policy. |
| `shape/step.rs`, `shape/prompt.txt` | The step's budget check, folder, model input, prompt, reply decoding and recording through the shared session boundary. Replies contain findings with a question or named seed prose fields; `seed_me` alone owns their layout and validation. |
| `goal/mod.rs` | Goal parts, constraint normalisation and composing the goal block (shared). |
| `goal/confirmed.rs` | Rendering a confirmed goal the same way in review, receipt and tend detail (shared). |
| `think/brain_dump.rs` | Think's `BrainDump`, which routes keys, mouse and ticks to its four parts, paints them, and reports one `Exit` to the menu. |
| `think/brain_dump/conversation.rs` | Words, extracts, readings, question rules, skips, scope, unresolved review questions and notes, requests and ordered record turns. Fields are private; methods hide these rules. It knows nothing about screens or jobs. |
| `think/brain_dump/reading.rs` | The model request, prompt, wire reply and lenient decoder, including open choices linked to questions. |
| `think/brain_dump/suggestion.rs` | Optional proposal decoding and copying into an empty draft; never submission or model context. |
| `think/brain_dump/review.rs` | The decision review: its prompt, what it sees, and how its decisions join the reading's. |
| `think/brain_dump/prompts/` | The two prompts as text, one rule per line, with a note naming the finding behind each tuned rule. Notes never reach the model. |
| `think/brain_dump/host.rs` | Pi shaping then review, or practice simulation. Stateful hosts start separate histories for each Think visit. |
| `think/brain_dump/board.rs` | A verified reading; `Board` exists only after verification. |
| `think/brain_dump/checks.rs` | The per-send record shown in "Why", from F1. |
| `think/brain_dump/goal_view.rs`, `intact_view.rs`, `voice_input.rs`, `layout.rs` | The review screen, original pane, dictation and screen layout, including help, key bar and notices. |
| `think/brain_dump/help_view.rs` | Think's bounded help overlay: visual hierarchy, scrolling body, fixed navigation and the boundary against hidden actions. |
| `think/drafting.rs` | Think's model host: its harness and model settings and bundled working-draft instructions, over the shared `pi.rs` boundary. |
| `think_record.rs`, `think_record/confirmation.rs`, `think_record/read.rs` | Confirmed-session format, caller-declared context, the shared authorization-to-actor boundary, exact-session attribution, formatted history and typed submissions for Shape; no phase behavior. |
| `think/canvas.rs`, `note.rs` | Extracted cards, and the raw text box with safe display, grapheme-offset mapping and machine-sized wrapped rows. Editor storage does not own send policy. |
| `voice/` | Local dictation: the host boundary, helper process, protocol and owned setup. |
| `watch.rs` | The runs in one named tink-sdlc checkout from its API 1 `status --json`: refusing other interfaces, waiting runs first, one run's detail. |
| `sdlc.rs`, `sdlc/run.rs` | Shared read-only API 1 status, exact original-seed matching, cancellable transactions and bounded command ownership; no store, checkout discovery or phase policy. |
| `taste.rs`, `taste/read.rs` | One result's unchanged goal, revision-bound observations, the one question and external copy handoff; explicitly mapped PRs, read-only status/GitHub reads and deliberate browser navigation, never an approval, merge or agent launch. |
| `taste/preview.rs` | Exact-head repo preview declarations, local HTTP readiness and explicitly launched temporary checkout/process ownership; no inferred commands, public URLs, agent or verdict. |
| `taste/next.rs` | Deterministic post-acceptance pause and next-dump proposal from snapshot evidence; no model, verdict, structural diagnosis or permission. The menu alone passes its unsent draft to Think. |
| `taste/label.rs` | The operator's pass/fail label, their verdict on the result, and its one write through the checkout's `sdlc.py label`, only on F2, words unchanged, signed with the name shown; never an approval, merge or agent. |
| `taste/view.rs` | Count-only result blocks, a one-question review, palette-bound headings and statuses, and details on request with verbatim hanging check wraps and evidence; no source reads, gate derivation or copy formatting. |
| `taste/links.rs`, `taste/evidence.rs` | The run's own recorded PR (API 1 `pr`), unknown when absent or malformed; source-owned log pointers and check methods as evidence references, never prose parsing, a mapping store, log reads or check execution. |

Vendored libraries are path dependencies, not phases: `vendor/pinstar/` owns
rendering and input primitives and `vendor/graf/` owns graph mechanics. Tend reaches graf
only through `tend/graph.rs`; linking and persistence do not depend on it. Graf knows
no Tinkery phase or Seed Me format. Each library's `UPSTREAM` records its source and
local edits; [Pinstar API](../../vendor/pinstar/API.md) and
[Graf API](../../vendor/graf/API.md) describe their boundaries.

## Design score

John Ousterhout's *A Philosophy of Software Design* supplies these eight questions.
Each pass scores 1.25. Structural PRs name the rows they move.

| # | Question | Recorded assessment |
| --- | --- | --- |
| 1 | Can each module be described in one sentence? | Yes: see the module map. |
| 2 | Are interfaces simpler than implementations? | Yes |
| 3 | Can an implementation change without touching callers? | Yes: model contracts live beside decoders; tests align field names. Only `seed_me/` owns Seed Me's format. Review decodes its reply; reading joins it. |
| 4 | Do interface comments describe the abstraction? | Yes: modules and `pub` items carry interface comments; the build denies `missing_docs` and `unreachable_pub`. |
| 5 | Is design discussion part of review? | Yes: [the working guide](../../AGENTS.md#guardrails) requires changeability review and naming affected rows. |
| 6 | Does each module hide an important decision? | Yes: Conversation hides question, skip and scope rules. |
| 7 | Can a newcomer see the boundaries without reading code? | Yes: this page |
| 8 | Is design improvement part of ordinary work? | Yes |

**Recorded score: 10/10** (2026-10-12, after #39). This is not a fresh measurement.
The remaining debt is `brain_dump.rs`'s narrow interface over substantial routing.

The module map and phase contracts own current boundaries; this score is a dated
assessment, not proof of current quality.

Design reasons remain with [theme ownership](https://github.com/jon-devlapaz/tinkery/pull/430),
[presentation receipts](https://github.com/jon-devlapaz/tinkery/pull/451),
[history sequence](https://github.com/jon-devlapaz/tinkery/pull/458) and
[typed history](https://github.com/jon-devlapaz/tinkery/pull/462).
For Shape's typed context, read [Think history](shape-and-handoff.md#think-history).

## Limits enforced by `tests/architecture.rs`

| Limit | Today |
| --- | --- |
| Phase reaches across phases | none allowed |
| `BrainDump` fields / `pub` fields | 13 / 0 |
| Long non-test source lines | See `LONG_LINE` and `LONG_LINES` in [the owning check](../../tests/architecture.rs) |
| Test calls to `Conversation`'s test-only accessors | 77 |
| Library files without a `//!` comment, `pub` items without a doc comment | none allowed |

Checks fail whenever code differs from a limit, even by improving it. Lower improved
limits in the same PR.
