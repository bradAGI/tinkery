# Taste

Taste shows one exact result beside its unchanged saved goal. Its one write is the human
operator's [label](#labels), pass or fail in their own words,
saved by tink-sdlc only on F2. The label is the operator's verdict on the result; it still
approves, merges and signs nothing. Taste never posts a review, merges, signs a gate or
launches an agent. Merging remains a deliberate external decision, not an interpretation of
green checks. How the run went (where it stalled, who stepped in) belongs to Measure, not
Taste. An explicit report of external acceptance can reveal an ephemeral next-dump guess.

## Sources and selection

Menu place 6 uses the named `--sdlc-root`, never checkout discovery. Opening only
explains the read. `r` runs that checkout's API 1 `status --json`, then each valid
run's `status <slug> --json`. The shared [SDLC boundary](architecture.md#modules-today)
owns bounded execution, cancellation and the exact saved-goal join used by Watch.
Unsupported interfaces or states refuse the reading. No source writes or polling.
Without a current result, the footer keeps help and exit, plus `r read results` only
when the named checkout is available and no read/open helper is running. Result
actions return when a result is selected; this changes guidance, not key behavior.

Each run records its own PR with tink-sdlc's `sdlc.py pr` (1.50.0 and later), and
API 1 per-run `status --json` reports it as `pr.url`. That record is the only link:
handoff prose is not a machine interface, and neither a slug, branch, title nor handoff
file can supply it. Taste reads `pr` only from the per-run status whose run ID matches
the list. A run without a recorded PR keeps an unknown-result row; the run's own status
says how to record one. tink-sdlc reports a malformed record as an invalid run, and
Taste also refuses any URL that is not a full `https://github.com/OWNER/REPO/pull/NUMBER`.
If GitHub names a different PR (owner and repo compare case-insensitively), the row
stays unknown. Tinkery stores no mapping and launches take no PR input.

Only `r` reads result metadata from GitHub, via `gh pr view <recorded URL> --json` using the operator's
login. Read the title, current head, state and PR-head checks. The title is display text only;
a missing one falls back to the recorded URL. Per-run status also reports how many labels
the run has; a checkout that doesn't say leaves the list silent rather than guessing. Recorded open and
merged PRs stay listed, including green PRs with no source gates owed: checks never
remove the need for human review. Merged results are marked “merged” in the list
and review; neither merge state nor green checks imply acceptance. Closed-unmerged
PRs stay hidden. Source gaps stay visible.
Each bounded helper has a 30-second deadline and 4 MiB output limit; helpers run
serially off paint/input. Leaving cancels and reaps the owned process group; late
results cannot reopen a view. Refresh removes old rows and copy requests first.

Keep source order, select the first result initially, and preserve selection across
refresh by run-instance ID plus PR URL. A vanished identity returns to the list
with a reason. No complete empty claim survives a failed read.

## One result

The list gives each result a compact two-line block: a one-row goal preview with
an explicit ellipsis when needed and muted open/merged/unknown context, then glyphs
and counts for passed, waiting, stale, failed and unknown observations, plus
“You owe N” when gates are owed and “awaiting human verdict” when the run has none. No check sentences, PR URLs, paths, hashes, run IDs
or commands appear in the list. Jade marks selection; the goal remains ink.
Counts are observations, never an aggregate done/approved verdict.

Enter opens the review. The PR title, merge state and preview offer come first so the
command and URL stay visible before `v`. **What to judge** shows the first active human
check the run reports as pending for the known result. It uses the source's exact
wording. Only an explicit `operator_attestation: true` identifies a human check in
API 1. A manual check with the flag absent or false stays in Checks and Evidence,
and can inform Next. If several human checks are pending, the review gives the total
and `e` shows them all.
With no such check, the review asks **Is this good?** A missing receipt set, an
unknown revision, or a previous human mark of unknown, failed or passed with a
recheck does not supply a pending criterion.

A human sees `w` to write a verdict. An agent sees who can record it. **Your goal**
shows the goal's first line. Source gaps and unknown revisions carry a rust `!`.
A read older than ten minutes says “may be out of date, press r”. The review scrolls
when source text needs more room. Only `l` reveals **Next**.

`e` opens details, one scrollable document in this order: **You owe**, **Checks**,
**Goal** (whole), **Evidence**, **Source notes**, **Result**. Blank space separates
sections. You owe retains the source's plain actions in jade. A human-attestation item already represented
there is not repeated under Checks; this is an exact-wording presentation rule,
not a change to owed-source derivation.

Checks begin with a glyph and plain status: “✓ passed,” “! failed,” “… stale,”
“… waiting: no record found,” or “? unknown: no CI reported,” with other honest waiting reasons retained. Then the
**verbatim source wording** wraps with a hanging indent. Check wording is never
shortened, abbreviated, clipped or given an ellipsis, in any view. Method-owner
tags move to Evidence rather than being repeated after each check.

The whole saved goal remains unchanged in ink in details, including completion conditions
and constraints. Result gives the PR URL, commit, run ID/locator, seed digest, read
time and preview declaration once, muted, last rather than in a competing fixed header.
The shared source/path-and-digest join supplies the goal; changed, moved, missing or
unmatchable originals stay unknown. Escape unsafe controls only for display, not
storage or copying. The list's goal preview is the only shortened source text.

Only failures use rust, always with `!` and words. Passed states stay ink; waiting,
stale and unknown cues are muted. Jade is selection or the current owed action,
never a generic success colour. NO_COLOR retains bold headings/selection, dim
metadata, underlined bold failure cues, glyphs and explicit status words.
Each source expectation retains its exact wording, method and observed state. A local pass needs both source-current verification and its receipt's exact
head matching the displayed PR head. A different head makes prior passes stale even
if SDLC still reports tree-equivalent verification as current. Missing result
identity stays pending/unknown. Provisional checks stay pending. Failed matching
observations say fail; absent evidence never passes. Passed human items with a
source `needs_recheck` retain “may need recheck,” not an invented invalidation.

GitHub checks concern the PR head, not the merge commit. No reported PR checks
means **unknown**, not failure or a repair instruction. Missing, skipped, neutral
or unfamiliar outcomes are not passes. Independent review remains pending for
external inspection because API 1 supplies no revision-bound independent review
receipt. This is separate from automated proof and operator acceptance.

When the run report has no verification receipt, no decisions, no checklist marks and
only default pending gates, its local checks and gate obligations say
**unknown: receipts not found here**, never owed, pending or waiting. This cannot
distinguish a never-verified run from receipts left in another checkout. The list
counts these checks as unknown, not as obligations; details and both copied requests
retain the unknown gate state. Raw source defaults remain in Evidence, not claims
that the operator owes another signature. No other checkout is searched. Relaunch
with `--sdlc-root` naming the receipt-holding checkout and read again; per-run roots
are not supported. Runs reporting receipt evidence retain normal derivation.
GitHub observations and independent review remain separate from these local gaps.

Owed gates use plain actions: “approve the plan” or “re-approve the plan,” plus
owed human expectations in their source words, never checklist IDs or seed jargon.
Evidence in details gives one item per block with indentation and blank space. It retains full check wording, raw reported states, method owners, human
marks and source diagnostics. No source fact is deleted. Evidence and the
unchanged copied request retain each automatic item's API 1
`run.log.path`, resolved lexically against the named checkout within the selected
run, its exact checking `argv` and `cwd` (with the API's default `.` labelled when
omitted), and optional `verification.log` SHA-256. Checking directories such as
`./`, `./src`, `src/` and repeated separators retain their original source spelling;
only their displayed resolved directory is normalized. Absolute, parent-traversing
and control-bearing cwd values remain unresolved; stricter log-pointer validation
is not loosened. That receipt digest is not a path.
No log is opened or dumped, no argv is executed, and no evidence URL is invented.
Pointers are source references, not existence claims. Missing/unsafe pointers or
methods leave matching pass claims pending; older heads/source-stale receipts still
stay stale. Human marks retain their own evidence separately.
There is no aggregate done/approved status. Read time means a snapshot, not an
unchanged-since guarantee. Preview commands and URLs come only from the explicit repo declaration below,
never these receipt sources or package scripts.

## Local preview

At the result's exact PR head, the repository may declare a root `tinkery.preview`
file: one JSON object on one line, with exactly `command` and `url`:

```json
{"command":"npm ci --ignore-scripts && npm run dev -- --host 127.0.0.1 --port 5173 --strictPort","url":"http://127.0.0.1:5173/"}
```

The command is a trusted shell line, not sandboxed code. Declare installation/setup
explicitly if needed; Tinkery never guesses from package.json. Only HTTP URLs on
literal 127.0.0.1 with an explicit nonzero port are supported. No public URLs,
redirect following, deployment, agents or model calls. The command must listen on
127.0.0.1: on macOS a dev server bound to `localhost` may listen only on ::1 and never
answer. The URL restriction is not a network sandbox for arbitrary repo code.

The explicit `r` read also reads that file through `gh api` at the displayed head.
404 means “No preview for this project.”; the format above lives in help (`?`).
Authentication, invalid files and other failures remain read refusals, not invented
absence. The review shows the exact command and URL before execution; the commit it
runs at is checked at launch and shown once in details. `v` from the list opens the
review, just like Enter; on the review it runs the disclosed command, and without a
declaration it says where help explains adding one. Reading, navigation, help and copying never
execute it; repeated or modified v is ignored.

Execution fetches that exact head into a temporary Git checkout, checks HEAD and
rechecks the declaration against the snapshot, then runs with temporary HOME and
XDG roots. Private-repository fetches use gh's credential helper, without changing
operator Git configuration. No operator working tree is used. An already-listening port refuses the
launch. The first successful local HTTP response triggers the default browser opener;
readiness has a 30-second deadline. Each HTTP attempt retains one connection for up
to two seconds within that deadline, accumulating a complete status line bounded to
1 KiB. Short socket timeouts poll cancellation; they do not abandon a slow or
fragmented response. Output is discarded, not retained as evidence.
The view says the URL answered and was sent to the browser, not that the operator
saw it or accepted it. Process exit, checkout, readiness and opener failures say what
failed and how to retry. Esc stops the owned process group and reaps its direct child
before returning; leaving Taste, switching results or rereading does the same.
Temporary checkout and HOME are removed after the process stops. This is a preview,
not an acceptance record or implementation permission.

## Keys and handoff

- List arrows choose; Enter opens the review. `l` from the list opens the review with the
  explicitly reported acceptance's Next block. On the review, `e` opens details;
  `e` or Esc there returns to the review. Arrows, PgUp/PgDn and Home/End scroll each
  complete document. `n`/`p` or Right/Left change result, clamped. Esc from detail
  returns to the list and discards the ephemeral report, without rereading.
- `v` on the review runs the explicitly declared local preview above; Esc stops and reaps it.
- `o` alone uses `gh pr view <recorded URL> --web` to open the PR. Navigation records
  nothing and rereads no checks. Browser failure names the retry. Repeated or modified
  action keys never open, reread or copy.
- `a` / `c` copy acceptance-review / correction requests through the existing clipboard
  transport, not executable approval commands. Copy feedback claims transport only.
  Each request retains the exact goal, run ID, seed digest, PR and full commit,
  condition evidence, gaps and owed gates. Unknown goal/result identity refuses copy.
- For a human, `w` on the review opens the [label](#labels) form. While it is open every key is its own:
  1 or 2 chooses, then all characters, `?` and `q` included, are the words.
- `?`/F1 help blocks underlying actions. Esc closes help, then details, then the review,
  then returns Home from the list. F10 returns Home. The review footer offers `w` only
  to a human. Help fits 80×24 without scrolling.

The copied request asks the external process to recheck current head, inputs and
receipts, resolve gates, inspect that exact result, and add the operator's requested
changes there before recording any decision. Copy remains available with failed or
missing checks but carries those gaps; it cannot approve around them. External
expected-input binding is still not guaranteed; changed material needs rereview.

## Labels

The label is the operator's answer to "Is this good?": their verdict on the result, pass or
fail plus their own words. Pass/fail with a written critique beats rating scales, whose
middle options blur ([research](https://github.com/jon-devlapaz/tinkery/issues/672)).
Recording one must take seconds, inside the review, or it won't happen. It approves,
merges and gates nothing.

`w` opens a form for the selected run: 1 pass (“It does what you asked, and you'd keep
it.”) or 2 fail (“Something is wrong or missing.”), saved as tink-sdlc's existing
`accepted` and `rejected` kinds. Taste doesn't offer `correction` or `none`; tink-sdlc keeps
them for its other callers. The hint “What makes it good, or what is wrong with it?” sits
above the words (typed or pasted; AltGr types, other chords don't). The form shows the name
it will sign with, read from the checkout's `git config user.name` when it opens; there is
no Tinkery setting. Only that shown name signs: with none, F2 reads it again and the form
shows the result before another F2 can save. Esc cancels and leaves the run waiting for its
label; leaving discards, but F10 waits while a save runs. Only a pressed F2 saves, by
running the checkout's own `sdlc.py label <slug> --kind … --words … --reviewer <shown name>
--source taste`. Words pass through unchanged. Success says “Saved your label.”; a refusal
from tink-sdlc (closed run, run building elsewhere, agent reviewer) shows word for word in
the form, which keeps the words. An `--operator agent` launch refuses `w`: labels are the
operator's. The label lives in `runs/<slug>/labels.jsonl`, owned by tink-sdlc and
append-only; Taste stores nothing.

## After external acceptance

From the list or detail, `l` means “I accepted this exact result externally.” It reports the
operator's decision for this snapshot only; it neither verifies acceptance nor
records a verdict. Copying `a`, browser navigation, green checks and merged/closed
PR states never imply acceptance. Unknown goal, run or result identity refuses the
report. A recorded merged result remains available after refresh, but the
next-step note still requires `l`; refreshing never restores an acceptance report.

The report reveals one italic **Guess · Next: …** proposal under Next. It appears
once; long wording wraps rather than shortening any check words it quotes. The
full wording also remains in the editable Think draft. There is no repeated Next
instruction in Evidence. `?` moves Outcome, Evidence,
Learning, Futures and Decision out of the review and into help, once each, without
repeating the next step. Outcome stays unobserved without real feedback;
changeability stays unknown without code-level evidence. No model calls, extra
reads, numerical score or structural obstacle is invented.

Stop and repair applies only to failed/stale checks or owed gates. Unknown checks,
source gaps and absent GitHub CI are not failures and do not independently require
repair. Next draws first from unmet source completion conditions (failed/stale
conditions first), then other failed/stale checks or owed actions. When source
checks pass and nothing is owed, it suggests observing the accepted behavior and
choosing the next slice in the goal's own words, not inventing a feature. Missing
CI and the API's unavailable independent-review receipt remain evidence limits,
never fabricated unmet completion conditions. Consolidation and re-scoping require
evidence these sources do not supply, so neither is guessed from check counts.

`t` opens the single Next suggestion in a fresh Think visit as an editable, unsent
draft. It names the behavior/uncertainty, unchanged boundaries and a disproof check,
including the exact original goal as labelled context. No host, source, save or run
starts from opening it. Only F2 submits the edited text through Think's usual budget
and preflight boundary. [Think](think.md#taste-drafts) owns separate provenance and
submitted-word attribution. Esc dismisses the guess; correction, result selection,
rereading and leaving discard it. Neither the suggestion nor the report is a new
gate or authorization. The operator still merges externally.

## Verification

`src/taste/tests.rs` and the reader's unit tests cover exact goal preservation,
unknown identity, missing/unsafe evidence, strict run-ID/URL inputs, statuses, source staleness,
compact count summaries, one source-pending human check on the review,
complete source words and evidence in details behind `e`, label form keys (pass or fail,
verbatim words, held F2, Esc, control chords), the form's kind lines, hint and signer,
selection identity, clipboard requests, key repeats/modifiers and small-size scrolling.
The changed-result regression must fail when the exact-head mismatch protection is
removed, then pass after restoration.

[Terminal journeys](../../tests/taste_terminal.py) use fake API 1 and `gh` in guarded
TEST homes at 100×30 and 80×24, each with and without NO_COLOR. They exercise
explicit reads/browser navigation, help isolation, both copied payloads, long evidence,
changed revisions, failed/pending checks, green open PRs, unknown originals,
missing/malformed recorded PRs, ignored handoff prose, usable API 1
log pointers/methods in both clipboard requests, remote failure, exit/return and cancellation. The checkout stays byte-identical;
no providers, verdicts or real browser launches run. The [API-shaped fixture](../../tests/fixtures/taste-api1.json)
preserves the reviewer's log/method/digest example without a private seed locator.
The [label journey](../../tests/taste_label_terminal.py) uses a real tink-sdlc checkout
(this repo's bundled scaffold) with a failing `gh` stub: the list says the run waits for a
label, the review asks the question, the form shows pass/fail lines and the hint, Esc writes
nothing, a missing name refuses before tink-sdlc and signs nothing until the form shows the
new name, F2 writes Unicode and quotes byte-identical as `accepted` or `rejected`, a closed
run's refusal shows word for word, and an agent sees the human-only verdict in the
review and help. Pressing `w` as an agent refuses and writes nothing.
Next-dump journeys additionally report acceptance, edit and send only with F2,
correct without acceptance, and handle nothing unmet, at both sizes with and without
NO_COLOR. Unit regressions distinguish copying from the explicit report, prioritize
failed/stale evidence, discard stale drafts and keep provenance out of requests.
They also require a single italic Next proposal, five rationale labels only in help,
unknown/non-stopping absent CI, a stopping failed check and a goal-bound next slice.
The absent-CI regression must fail when unknown evidence is made to require repair,
then pass after restoration.
Merged-result journeys additionally read a recorded merged PR, verify its
label and absence of a note, then report acceptance with `l` before the note appears.
The merged-result retention regression must fail when the open-only filter returns,
then pass after restoration. The report-boundary regression must fail with its
acceptance protection reverted.
The [quiet-review journeys](../../tests/taste_quiet_terminal.py) check compact lists,
the one-question review and headed details, at both sizes and both colour settings. They
pin both OSC 52 payloads byte-for-byte to the synthetic
[a61f719 baseline](../../tests/fixtures/taste-handoff-355.json), substituting only the
TEST root into expected strings. Retained parent binaries additionally compare raw
copy bytes in the same TEST root; frame equality is deliberately retired for this
approved layout change.
[Long-sentence journeys](../../tests/taste_structure_terminal.py) use synthetic
clinic-planning assumptions, not operator data, and the
[3118762 transport baseline](../../tests/fixtures/taste-structure-handoff-355.json).
They recover every source check word through hanging wraps and scrolls, keep owed
human items out of Checks, and retain all metadata behind `e`. Unit tests check
actual colour and NO_COLOR cells, glyphs, heading order, and exact reconstruction
of repeated spaces and Unicode graphemes. The check-word preservation regression
must fail when truncation returns, then pass after restoration. Next content,
source reads, copied payloads and owed-source derivation do not change.
Missing-receipt journeys place synthetic signed receipts in another TEST checkout
and read the receipt-free intake report, at both sizes and both colour settings.
They require unknown local checks and gates in details and both copied requests,
no owed count, no extra reads, and byte-identical source trees. The missing-receipt
unit regression must fail when the protection is reverted, then pass after restoration.
[Preview journeys](../../tests/taste_preview_terminal.py) use a real temporary Git
fixture, fake HTTP server and stub opener at both sizes and both colour settings.
They prove no execution on read/navigation/help, exact-head checkout (not newer HEAD),
readiness before opening, Esc and leaving stopping the process group, checkout cleanup,
and plain declaration guidance when absent. A single-threaded server delayed by
250 ms per response exercises readiness under load, with the journey waiting for
the product's 30-second deadline plus opener/paint slack. Product regressions cover
delayed and fragmented status lines, cancellation, the overall deadline and valid
2xx parsing. The delayed/fragmented regressions must fail with the original probe,
then pass after restoration. Wrong-head refusal must fail when its
checkout identity protection is reverted, then pass after restoration.
These prove mechanics, not operator
dogfood or improved attention; #109's comparison method remains separate.
