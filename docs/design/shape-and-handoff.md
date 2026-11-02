# Tend and handoff

Tend holds saved goals; [Shape](#shape) asks its questions with an explicit
model and budget. There is no skill continuation prompt; continue the saved goal in Shape.

## Receipt

Explicit successful saving shows `Saved.`, the exact frozen goal block and
`Next: shape it into a seed in Shape.`

- `s open in tend` opens this session's detail, not the garden.
- Esc returns to the menu, including from standalone Think.

Nothing launches, opens a browser, pastes into a harness or starts implementation.
Paths belong in F1 then `h`, with the [think record](think.md#think-record) below
the session path. If recording fails, the goal stays saved; help and a muted receipt
notice disclose the gap. If only reading context was rejected, the successful receipt says
`Reading sequence wasn't saved; F1 then h shows why.` The path and full reason remain in
technical details and the plain-text exit receipt. Notices wrap without clipping in receipt and detail.
The plain-text exit receipt may include recovery locations. Its next step is read
from the session at exit, through the shared verifier: a verified confirmed seed
points to Approve, a draft to confirming in Shape, and a goal without a seed to
shaping. An unverifiable or unreadable session points to checking in Shape, never
claims readiness; other ended sessions are named as ended.

## Tend: garden and detail

Inside a repository, Tend starts with only active goals whose attributed Think
record names that exact repository root. A muted `this repo · a all` line offers
`a` to show every active goal; `all · a this repo` toggles back. Outside a repo,
all goals show without a toggle. Unknown, missing, older or conflicting records
never guess a repo; those goals remain under all. An empty repo view still offers
`a`. If any active goal lacks attributable repo information, say `N goals have no
readable repo record. Press a for all goals.`, not that this repo has no goals.
Explicit no-repo records are known non-matches; older records without repo
information are unknown. Only with no unknown active goals or unreadable sessions
may an empty repo view say `No active goals are recorded for this repo.` With
unreadable sessions, say `N saved records couldn't be read; this repo may have
active goals. Press a for all goals.` Ended counts remain visible. Reopening Tend
starts with this repo again; explicit receipt or Shape returns still open the
exact saved goal, switching to all if necessary.

Filtering reads existing records on discovery's worker, never during painting,
and writes nothing. Each discovery enumerates the Think record root once and
reads each JSON record at most once, rebuilding an in-memory attribution index;
refreshes never reuse an earlier index. It shares history's strict validation and
rechecks session identity and goal across the scan. Malformed or unsupported files
make every origin unknown; duplicate paths and matching schema-1 records refuse
attribution, never choose a winner. The index keeps paths, identities, exact goals
and origins, not turns or checks; the 16 MiB per-file limit remains, with no total
index-size cap. Links, identity checks and readiness still use every readable
session, including other repos and ended goals. Use all to link across repos;
`a` does nothing during linking, detail, history, help or pending/failed reads.

Tend displays the visible goals in one static graph, without paging or node cap.
Equal hollow Braille rings mean size is unknown; no sizes or merges are known.
Needs lines join only active, uniquely identified endpoints. Missing, ambiguous or
ended endpoints draw no line; the selected-line summary still names them. Straight
lines leave gaps around unrelated rings, never implied junctions or routed paths.
Saved sessions stay read-only; linking writes only Tend's store.

Ended sessions are omitted. `N ended not shown` and
`Links couldn't be read. F1 then h for details.` share the muted incomplete-read
row, joined by ` · `, even with no active goals, and survive navigation. An
unreadable-links refusal replaces, rather than repeats, the note. When an empty
view's body reports unreadable sessions, the footer keeps ended and link notes
without repeating that count. This applies to repo and all-goals views, including
outside a repository. An incomplete empty all-goals view says `N saved records
couldn't be read, so nothing can be listed. Esc goes back.`, never `No active goals`.
Only when every session was readable and ended, say `No active goals. N ended not
shown. Confirm a goal in think to start one.`, not that no sessions exist.

Labels use the first saved-goal line, including unstructured goals. They sit above
or below rings, clipped with `…` or omitted rather than overlapping or interleaving.
The selected label gets space first and always appears, prefixed `› `; its label
and ring are bold, jade with colour. Other labels are muted and rings use ink;
none is italic.

The selected-line footer shows the first goal line in ink, status and relative time
muted, then needs and readiness, e.g. `needs Local records · ready`.
Valid offsets determine age and order. Timestamp syntax stays unchanged; invalid
dates and pre-epoch values remain `time unknown`.
Unresolved ids say `1 need not found` or `N needs not found`; no needs adds nothing.
Short summaries retain status and time. If these crowd the headline, show the
headline, readiness and `F1 needs`; scrollable Help retains full titles. Ambiguous source
identity says `readiness unknown` and attributes no needs. Refusals use the blank
row before the key bar.

| Input | Action |
| --- | --- |
| Right, Down or `j`; Left, Up or `k` | Select next or previous in discovery's newest-first order, clamped; not graph neighbours. |
| Enter | Open detail. |
| `+` or `=`; `-` | Zoom in; zoom out. |
| `H J K L` | Pan left, down, up or right. |
| Left press on a ring or label | Select its exact session path; a second press within 500 ms opens it. |
| Drag a ring or label; drag empty space; scroll | Move the goal; pan; zoom. |

The key bar is `↑↓←→ move   enter open   l link   esc back` (fits 80 columns).
Selection pans offscreen goals into view without resetting zoom. Help, detail,
history, reading, empty and unavailable states ignore graph input.

Graf uses deterministic identity-ordered slots adapted to the canvas, initially
at least 12 columns × 6 rows apart, without a physics thread. Crowded gardens
require navigation, panning or zooming, not simultaneous labels. Moving, panning
and zooming write nothing. Refresh preserves positions and viewport for an unchanged
graph. Changed nodes, labels or links rebuild slots and reset manual positions but
keep zoom and pan. Resize reflows slots with relative zoom and pan retained.
Following selection may pan again. Session reads, scope changes and needs-link reads or
edits prepare the path-bound nodes and links; unchanged repaints reuse those inputs.
The adapter rebuilds Graf’s cached node colors and labels only for new graph data,
changed ink/jade colors or a changed marked source. Geometry still updates each frame.

Detail uses the receipt's shared renderer for the exact stored goal, without
`Saved.`. ↑↓, `j k`, PgUp or PgDn scroll (a page moves the view less one row); `s` opens Shape on permitted stages, and `r` opens history. Esc goes detail → garden →
menu; F10 goes to the menu. F1 opens help; `h` opens technical details. Returning from Shape
requests a fresh read and restores this goal's detail; next step and garden stage
update together. Ended or unreadable sessions disappear from active goals. Returning
writes nothing and calls no model.

| Stored state | Label | Detail next step |
| --- | --- | --- |
| Confirmed goal | goal saved | Shape it into a seed in Shape |
| Draft seed contract | seed drafted | Confirm the seed draft in Shape |
| Seed declares confirmation for intake | seed needs checking | Check the seed in Shape |
| Completed session | completed | None; copying unavailable |
| Stopped session | stopped | None; copying unavailable |

File-only discovery never verifies seed confirmation: even a legitimately confirmed,
unchanged seed says `seed needs checking`. Missing receipts or changed seeds cannot
claim readiness. Shape remains available; only Shape's Tinkery-owned
status verifier can say ready. Discovery invokes no helper or duplicate verification.
Unknown and practice stages stay distinct.

[Session discovery](menu.md#session-discovery) owns refresh and completeness.
Pending empty views say reading or still reading; failed reads say unavailable with F1 then
`h`. Only complete current empty reads say no sessions. Incomplete reads retain
valid goals with an unreadable-record count and missing-sessions warning. If none
are readable, say nothing can be listed, not “no sessions”. Reading and unavailable
views offer only `esc back`; an empty repo view also offers `a toggle goals`.

## Think history

Detail's `r history` starts with **Recorded sequence · 27 Oct 2026, UTC** (the first
event's UTC date). Rows show `HH:MM:SS` in UTC, converting recorded offsets, with a date
line whenever the date changes. List order stays the truth even when clocks go backward.
**Original words N** labels original-word events; **Submission N** labels recorded
answers and scope answers without claiming a question was asked. Source numbers match
the conversation headings below. Only a preceding presentation
adds ` · after reading R`. Presentations say **Reading R shown**. One notice says
`Shown means it reached the screen, not that it was read.` Missing context says
`Reading sequence was not recorded.` Rejected context says
`Reading sequence was not saved: <reason>` in both history and technical details. Malformed
saved context, including times outside the supported UTC range, says
`Reading sequence unavailable; saved words remain below.` None hides valid originals or the goal.
The shared reader owns these labels and validates context once for this summary, saved
reading text and technical details; the view parses neither JSON nor formatted details.
It infers no merged chronology with question/skip actions.

Original words, questions, answers, skips and restores follow in their existing recorded
order. Suggestions have their own label, never an answer label. **Readings · Tinkery's guesses**
then shows each presentation's saved main text and other-goal/deferred groupings, before the
unchanged **Confirmed goal**. Muted headings say **Reading R · shown HH:MM:SS**, appending
the UTC date only when it differs from the sequence heading's date. This includes identical
or partial readings. Main text and grouped items stay verbatim, in separate italic paragraphs,
under **Also in your dump** or **For the seed** where recorded. Originals and the confirmed
goal retain ink. Absent or whitespace-only main text says `No goal text recorded for this reading.`
and retains ancillary items. Missing or invalid context adds no empty readings section.

This section includes saved offscreen text, not the full original screen. Questions and
remaining snapshot fields stay in the conversation or technical details. It never rebuilds
readings from parts, sorts timestamps, merges event streams or infers corrections. The shared
reader supplies typed text lists and reading headings; Tend only lays them out and styles them.
Exact recorded timestamps and full frozen snapshots remain in technical details.
A recorded repository adds `Made in`
and its folder; outside repositories or schema-2 records, omit it. F1 then `h`
shows the full path, `none` or `not recorded`, plus model information, checks and
[recorded reading context](think.md#presented-readings-and-subsequent-words).
Labels are muted; operator words use ink. Display escapes unsafe terminal controls
without changing stored words. History keeps whitespace-delimited words together when
they fit the view; longer tokens wrap at grapheme boundaries. Explicit line breaks,
whitespace and tones remain, and scrolling reaches every row.

↑↓, `j k`, PgUp, PgDn (a view less one row), Home and End scroll width-wrapped history, including Unicode
beyond 65,535 rows. Help scrolls independently. Esc goes details → help → history
→ same saved goal; F10 goes to menu. A fixed footer keeps return instructions
visible. Opening, scrolling and help call no model, copy nothing and write nothing.

`--record-root` selects existing records without requiring `--shape-pi`, defaulting
to the saving folder. One background reader lasts the menu's lifetime. Pending
requests coalesce; leaving discards results. A stuck read occupies that worker,
but never blocks navigation or starts another. Reopening explicitly requests a read.

The shared reader requires one schema-2 or schema-3 record matching the ledger session ID
and exact confirmed goal, then rechecks the selected session and live-ID uniqueness.
Duplicate matches refuse selection. Matching schema-1 records lack exact attribution;
never migrate or guess from text. Unreadable or unsupported JSON may hide another
match, so it prevents selection. Each file is limited to 16 MiB; oversized records are
unavailable, never partial. Missing, unreadable and conflicting history explain
the gap without changing the goal. The one-time move rewrites no Think record. A recorded ledger ID remains the authority,
not the full path or the live folder name. For a path-only record whose old folder is missing,
compare its final folder name against live ledger IDs. Two live sessions with the same ID
refuse plainly rather than choose one; current-root identity scans happen off the paint path.
An existing path without a recorded ID cannot prove attribution. Unfinished-session recording
remains outside this feature.

Shape uses the same verified record read and full record parsing for its recorded
repository and exact answered question/answer pairs. It receives typed data and owns
the JSON format of its step prompt. History presentation is built only for history
readers. Invalid optional reading context still leaves valid attribution and Shape's
submitted words usable, while history retains its diagnostic notice.

## Linking needs

Garden `l` starts “this goal needs …”. The source retains `›`; movement marks a
target `→`. Enter or one left press adds or removes the need; picking the source does nothing.
The key bar becomes `↑↓←→ pick   enter link   esc cancel` (`enter unlink` for an
existing edge); the bottom line names both goals. Commit preserves the displayed
intent to add or remove despite concurrent changes. Already satisfied intent preserves
the file and says `Already linked.` or `Already unlinked.` Esc before commit writes
nothing. Committing or cancelling reselects the source; discovery refresh waits until picking
ends. Detail, history, help and reading states ignore `l`.

Missing or ambiguous identity, or unavailable links, changes nothing. Muted refusals are:
`This goal can't be linked yet.`,
`Links couldn't be read, so nothing changed. F1 then h for details.`, or
`Links couldn't be saved, so nothing changed. F1 then h for details.`
Technical help retains the exact reason and session path through pending refresh.
Lock waiting is synchronous, bounded to about two seconds. Timeout reports a save
failure with technical reason `another Tinkery is saving links`. Queued Esc or F10
cannot cancel an in-flight save that obtains the lock before deadline; navigation
follows it. Timeout refuses outright; later lock release never retries.

## Needs store

`--link-root PATH` selects `needs.json` (default `~/.local/share/tinkery/tend`,
requiring absolute HOME). Tend never writes session files. Navigation and copying
create no links; landed session reads and starting a pick refresh cached links.
Painting reads only the cache.

Version 1 is `{"version":1,"needs":[{"goal":"id","needs":"id"}]}`. IDs are nonempty,
control-free ledger `session_id`s, never titles or folders. Missing or invalid IDs
leave sessions readable but unlinkable. Sorting by goal id then needed id makes equivalent
sets byte-identical.

Missing files normally mean empty. After this view read nonempty links, disappearance
refuses reads and saves rather than losing known links. Bad JSON, unsupported
versions, invalid or duplicate entries and IO errors remain unreadable; edits cannot
overwrite them. Changes to read-only or unwritable existing files refuse without changing bytes
or permissions. Matching no-ops need no write access and preserve exact bytes,
permissions and timestamp.

Edits reject self-links and unreadable stores, acquire an exclusive sibling
`needs.lock` within the deadline, reread, apply intent and atomically replace
through a same-folder temporary file. The empty lock file remains so cooperating writers lock the same
file. Navigation creates neither file. This preserves concurrent saves, not changes
from external tools ignoring the lock.

Readiness uses links and all readable sessions, including ended ones. Found needs
use first goal lines; unresolved or duplicate IDs are counted without guessing.
Duplicate source IDs have no attributable needs or positive readiness. Ready means
at least one need, all uniquely found and completed. Stopped needs, no needs, missing
needs and unknown stages are not ready.

## Continuation contract

The former skill-based copy prompt is retired. Receipt and Tend detail open Shape;
navigation launches nothing and writes no session. Approve and verified Shape seeds
still copy manual intake commands through [OSC 52](../terminal-setup.md#clipboard-osc-52).

Default sessions live in `~/.local/share/tinkery/sessions`. On the first default-root
ordinary launch, move the old sessions folder once and show a notice.
Snapshots never move sessions: when only the old root exists, refuse before writes and explain
how to migrate with an ordinary launch before retrying. New-only or absent-root snapshots
render normally. If both roots exist, refuse rather than merge or read the old root.
Explicit roots never trigger the move. Startup chooses whether a move is allowed; migration
retains its root checks (architecture diagnostic 6).
The migration renames only the sessions folder, never parses its ledgers and never reads or
writes Think records. Old-format session folders move byte-for-byte too. History uses the
identity rule above; no old-root session reader remains.

### SDLC handoff

Approve and Watch share a read-only seed-to-run join, not a binding store. API 1's
`seed_contract.source` must equal the seed's canonical path and `sha256` must
match its current bytes. Names, slugs and goal text never establish a connection.
Edited originals are unknown, not started; moved or missing originals cannot lend
a goal to a run. Before a match supplies the session's exact saved goal
([Intent](../yohaku.md#1-intent-remains-sovereign)),
the shared seed verifier must also confirm that the ledger decisions and seed still
match their confirmation receipt. The seed and ledger must remain byte-identical
through that read; missing or invalid confirmation keeps the goal unknown.
These seed, ledger, confirmation-receipt and check reads open without waiting for a FIFO writer,
then require the opened file to be
regular before reading. This includes repeat reads during confirmation; a path replacement
cannot bypass the file-type check. Symlink resolution and exact path/hash matching stay
unchanged. This rule lives in
`seed_me`, shared by the run join and verifier (architecture diagnostics 3, 4 and 6).
[Watch](watch.md#reading) owns its compact list and full-detail presentation.
An explicit null record means no confirmed seed was used, distinct from an
unmatchable or malformed record ([Evidence](../yohaku.md#3-claims-stop-where-evidence-stops)).
This is a read-time snapshot, not
permission or a live freshness claim.

Only an explicitly named `--sdlc-root` is read. Missing configuration, unreadable
runs or absent/unsupported seed records mean Approve cannot tell which seeds have
started and hides none ([Evidence](../yohaku.md#3-claims-stop-where-evidence-stops)).
A readable null record establishes no join.
The shared `sdlc` boundary owns status reads and matching; `seed_me` alone reads
the session's files. There is no checkout discovery, source mutation or new store.

Goal and seed confirmation, SDLC definition approval and implementation permission
are separate decisions. External tink-sdlc intake preserves the confirmed seed and
imports its acceptance checks as locked definitions, reported separately from
worker-written supporting checks. Editing or dropping an imported check is a departure:
it stales the definition gate until a human, never an agent, accepts it. Settled commands
must succeed; their `expect:` is context, not an owed mark. Provisional checks stay pending until activated with prerequisite
evidence and renewed definition approval; `you judge:` stays manual.

Locked checks protect the agreement but don't prove the checks are strong.
Hash-bound local receipts detect changed definitions; they are not authenticated
operator approval.

The [handoff regression](../../tests/shape_sdlc_handoff.py) carries Shape's real
seed writer through confirmation, a fresh TEST-only SDLC install and intake:
Approve's combined command, pasted unchanged from outside the checkout, starts a real run, Approve hides the matched seed, and
Watch shows its goal's first line under needs-you runs and the full goal in detail,
while a run with no seed stays distinct from an unknown match. A failing confirmed command blocks
verification despite passing worker-written checks, and a passing command completes it.
Dropping seed enforcement turns the regression red; restoring it restores blocking. Its confirmations, approvals and
attestations are TEST fixtures, never the operator's.

## Shape

A verified Ready seed offers the deliberate [Approve route](approve.md#from-a-confirmed-shape-seed)
in F1's first row. Opening the list starts nothing; choosing the row navigates
through the menu and normal verification, with copying still separate.

Think settles intent; Shape investigates facts, then asks one decision at a time
with evidence and a suggestion, ending in an operator-confirmed seed.

- Tinkery owns Shape's ledger, seed checks, confirmation, verification and ending;
  [the accepted design](shape-ledger.md) records the completed migration. Draft instructions
  and the seed-contract reference are bundled, with their pinned source recorded in code.
- Each agent step needs an explicitly labelled key and budget (default zero).
- Shape never reasks settled Think intent or ledger answers, including delegated pre-mortem
  choices, or changes the confirmed goal. Seed drafting treats these choices as settled, not unknown.
- Failed reads, unknown stages and stale confirmations get honest reasons and one
  next step.

### Reading where a seed stands

Each session read derives ledger fields and status from the same validated snapshot.
Seed and confirmation artifacts are still checked separately.

Tend detail offers `s shape` on goal-saved, seed-drafted and
seed-needs-checking stages; Esc returns there. Menu place 3 lists only goal-saved
or seed-drafted sessions. Enter opens one; Esc returns to the list. Held `s` or Enter
opens nothing.

The menu passes discovery's unreadable-record count with its readable sessions.
Both empty and populated incomplete lists show the count and `F1 shows why`.
F1 lists up to eight actual failed paths and their read/parse reasons, then says
how many more unreadable records are not listed. Reasons retain at most 512
characters plus an ellipsis; display escapes controls without changing paths.
PgUp/PgDn scroll the wrapped help, with close keys always visible. Inspect the
listed files, then return to the menu and reopen Shape to read again. Fresh or
failed reads discard earlier details. Seed-read failures still mean an unknown
stage, not an unreadable session. Partial empty reads never say no goals are waiting.
A complete empty read keeps `No goals are waiting for a seed.` Session eligibility
and refresh rules are unchanged; list navigation writes nothing and calls no helper or model.

The screen starts with the confirmed goal's first line in ink:

| Tinkery reads | Shape shows |
| --- | --- |
| Current question | Show a muted label, jade question, `What I found:` for only the first supported finding (or legacy question receipt), and italic `Suggestion:`. So far holds all findings for this question in recorded order. Options and the case against show above the suggestion; see [options](#options-and-instinct-first). |
| No question or seed | If the saved record explicitly lacks a success criterion: `The saved goal leaves Done when open.` Otherwise: `Shape hasn't asked its next question yet.` |
| Draft seed | Draft replaces goal under muted `Seed draft rN`; see [screen](#the-screen) |
| Confirmed seed verified by `status`, active or completed | `Seed ready` and revision, then `Next: Approve copies the command that starts the build.`; completed adds a muted note that nothing can change |
| Unverifiable confirmation | Tinkery's reason, e.g. changed seed |
| Practice, stopped, unrecognised | Name that state plainly. |

The missing-success line is read from the saved record before any model call. Missing
older metadata stays unknown, and an explicit `Done when:` in the saved goal takes
precedence over an empty legacy outcome field. It changes no goal, question, save
permission or call policy. The session reader owns this interpretation; rendering
only chooses the wording (architecture diagnostics 3 and 6).

Ledger review flags precede open questions and older drafts: Shape shows the first
flagged node as the current question, its unchanged answer and the ledger's reasons.
`F2 keep with reason` publishes the operator's typed justification without a model call.
`F2 revise` opens replacement words only for an operator-answered decision; see
[answering](#answering-and-confirming). Each remaining flag holds confirmation and completion.
Open questions precede older drafts. Without configured drafting, one `Next:` points to
F1 for [Model setup](menu.md#model-setup). Configure a model and an explicitly authorized
call allowance (default zero), restart, then reopen the same goal through Tend,
retaining any explicit session root.
Unverifiable or unknown seeds point to inspecting the session files at the path in F1.
With a model, the key bar names the available action.
Ledger refusals say `Tinkery refused` with the first reason line; a rule the model's reply
broke says the model's reply was refused and names the rule without ledger node ids. Other failures
are explicit, with reasons and paths in F1. Reads stay off paint paths.
Opening, reading and scrolling write or copy nothing, call no model and leave prompts
unchanged.
`tinkery seed status --json DIR` prints the intake status without `viewer`, `snapshot` or
`snapshot_current`; `tinkery seed checks DIR` prints the strict check records. Both are read-only.

Shape copies no intake command. [Approve](approve.md) owns the one command that
creates a run in the named checkout and imports the seed, so a seed has one handoff
line and no placeholder to replace (#466). Seed ready points there; choosing or
creating an SDLC run stays outside Shape.

### The screen

- Under `tinkery › shape` and `?`, content begins at the body top, at most 76 columns
  wide. The goal headline or draft, then stage, precedes words, notices and `Next:`.
  Overflow scrolls while the words box and cursor, or running status, stay visible,
  occupying at most half the body. Cursor following survives long or wrapped
  words and resize. Readings (seed plus review or feedback, and So far) must stay under 60,000 bytes; otherwise Shape refuses without confirming or changing anything.
  An empty reading viewport cannot mark a draft read.
  Only overflow offers `pgup/dn read` while editing or `↑↓ scroll`
  otherwise; overflowing questions shorten the save key to `F2 save + look`.
  Seed `Nothing found` stays beside words. Crowded bars reduce gaps to two spaces,
  never cut Esc.
- **So far** (from F1) is a full help-like page without a key bar: the entire confirmed
  goal unchanged, then settled `label: answer` decisions in ledger order. Only
  `user` authority shows `(you)`; others show their authority, e.g. `(delegated)`.
  Open questions follow in ledger order; held questions say `waits for:` with the earlier
  answer labels they need. All current-question findings follow, unchanged and in recorded order. Esc
  closes it; ↑↓ scrolls. The fixed last row says `↑↓ scroll · esc close`.
  It accepts no words, appears after a read, never during a step; F1 lists it.
- Say the next step once: omit `Next:` when the key bar names the stage action;
  other states and failure notices keep exactly one.
- Words boxes use ratatui-textarea, titled `your answer`, `your words`, `your teach-back`, `your change` or `Folder:`.
  Arrows, Home, End, Ctrl+Left and Ctrl+Right move the cursor; Shift selects.
  Ctrl+W or Alt+Backspace deletes a word; Enter inserts a newline except at seed confirmation; Ctrl+U undoes
  and Ctrl+R redoes. PgUp and PgDn scroll only the reading, a view less one row. Pasted CR and CRLF
  display as breaks; edits preserve untouched raw endings. Under seed words, say seed confirmation is not approval to build.
- Running steps keep label, question and typed words muted, with one status
  (`Saving your answer.`, then `Answer saved. Looking in NAME for the next question.`),
  elapsed seconds and `esc stop`. F1 says a step is running.
- Draft display replaces file `status:` and `revision:` lines with `Seed draft rN` and
  shows `#` headings bold. All other lines remain as written; display never edits
  `seed-contract.md`.

### Answering and confirming

Configured drafting enables these actions; see [Model setup](menu.md#model-setup).
Shape uses Think's
[one key map](think.md#help): F2 runs the action in the key bar, F1 numbers the rest.

| Stage | Words | Key/action |
| --- | --- | --- |
| Flagged node | `your reason` | `F2 keep with reason`: keep the answer unchanged and revalidate, no model call |
| Flagged operator-answered decision | `replacement answer` after F1 **Revise** | `F2 save revision and look`: replace it, reopen settled dependents, then look |
| Current question | `your answer` | `F2 save + look`: save then one step |
| Current question | Reason, then revisit condition after F1 **Defer** | `F2 next field`, then `F2 save deferral`: publish deferred, then one step |
| Current question | None required | F1 **Delegate**: accept the grounded suggestion, then one step |
| No question or seed | None | `F2 look into the next question`: one step |
| Draft replacing goal | `your words` | `F2 confirm seed rN`: confirm displayed revision when the words are exactly `confirm` |
| Draft replacing goal | `your change` after F1 **Revise the draft** | `F2 save + redraft`: save the change as your settled decision, then one step |
| Draft replacing goal | `your teach-back` after F1 **Teach back** | `F2 save + compare`: capture words and compare with the draft, one call; Esc restores confirmation words |

- **Before confirmation:** the ledger computes the risk summary above confirming, including
  reversals detected from earlier settled answers, delegated and assumed choices,
  exact acceptance of suggestions, and accepted-versus-chosen-otherwise counts.
  Shape displays these words without its own checks or formatter.
- **Teach-back:** on a draft, F1's **Teach back** opens optional words about what will and won't be built,
  prefilled with any confirmation words already typed, editable in a separate box.
  F2 saves them verbatim, then compares in one call behind F2's shared budget,
  with no tools or repository access;
  opening, typing, help and cancelling do not save or call. Confirmation words remain
  untouched. The latest completed comparison for these draft bytes appears as guesses
  above the risk summary; mismatches are recorded verbatim and never block confirming.
  Failed comparisons retain captured words. Confirming without a completed comparison records
  `Skipped teach-back.` verbatim, after ordinary confirmation refusals. These append-only
  confirmed-goal receipts are bound to the revision and SHA-256 of draft bytes;
  confirmation digests cover them. They retain the existing receipt format and are
  interview records, not evidence changes or permission.
  F1 offers teach-back only on a draft.
- **Revise the draft (#614):** on a draft, F1's **Revise the draft** opens `your change`,
  prefilled with the confirmation words unless they are `confirm`, so a correction typed where
  `confirm` goes is never lost. F2 saves the words verbatim as a settled `change-to-rN` decision
  with the operator's authority, never a confirmation, then takes one drafting step behind F2's
  budget gate; the redraft lists the change under Decisions. Empty words or words that leave the
  change open are refused, saving nothing. Esc keeps the confirmation words untouched.
- **Defer:** F1's **Defer** opens the reason; F2 advances to the revisit condition; Tab switches
  between them. The form shows the question's gate: the operator alone decides it can wait.
  F2 on the condition saves both fields verbatim as `deferred`, never confirmed or settled.
  An answer, revision, revisit or delegated suggestion that is empty or only says the
  decision is open ("not decided.", "TBD") is refused, saving nothing and keeping the typed
  words, and points here (#594).
- **Answer deferred prerequisite:** an active Waiting session may offer one named deferred
  blocker in F1, selected by the ledger in its existing order. Its own prerequisites and
  predicate must be ready; the goal and flagged sessions never qualify. The form shows the
  original question, exact deferral reason and revisit condition. Opening, typing and Esc
  save nothing. F2 saves the exact answer with existing authorization, locking and stale-state
  checks, without a model call. Existing history preserves the prior deferral unchanged;
  dependent questions stay unanswered and follow ordinary ready ordering. No picker or
  automatic repair is provided. This keeps dependency selection and transitions in the
  ledger (architecture diagnostic 6).
- **Delegate:** F1's **Delegate** row is one choice, quoting that row as the authority source, saving the
  recorded suggestion unchanged with `delegated` authority and its supporting facts as evidence.
  Reply confidence is not appended to the recommendation or answer; the ledger never parses
  suggestion text.
  Missing facts or suggestion refuses without writing; type an answer instead.
- **Revise:** only flagged decisions answered by the operator (including delegated answers)
  offer F1 **Revise**. Flagged findings (facts and inferences) and the confirmed goal offer only
  keep-with-reason; Revise is not shown. There is no settled-answer picker: So far remains read-only.
  F2 saves exact replacement words with user authority; the ledger reopens every settled
  dependent with a reason naming the changed answer. Shape asks ready dependents one at a time
  before a new model question. Saving a revision uses F2's budget gate and next-step path:
  an already-recorded dependent is asked without a model call; otherwise one step investigates
  or replaces the older draft.
- Esc cancels a form without changing the original answer editor. Ledger refusals keep the
  question and both typed fields, show the first reason line and allow correction and retry.
  Required fields, delegated evidence, revision eligibility and cascading reopen belong only to
  the ledger; Shape adds no parallel rules. F1 lists only the actions allowed now.
- F2 saves exact answer bytes with `user` authority. Typing, paste, scrolling and
  help write nothing; held action keys never repeat. Letters, capitals and `?` remain words.
- The chosen harness runs one explicitly selected model with read-only tools (Pi's `read`,
  `grep`, `find`, `ls`; Claude Code's `Read`, `Grep`, `Glob` under `dontAsk`), no commands, in the [shown folder](#grounding-in-the-repository). Input includes
  the confirmed goal, attributable Think answers, every `found_so_far` finding and
  a model-only ledger view: findings appear only in `found_so_far`, not again as
  ledger fact nodes or copied decision evidence. Settled pre-mortem questions and answers stay
  once in their ledger decision nodes, not a second answer list. Goal provenance, including Think's
  original context, stays unchanged; stored ledgers are never trimmed. Replies require typed `found` entries (see
  [grounding](#grounding-in-the-repository)) plus a next question and suggestion, or seed summary, prose sections
  and separate typed `review` and final `checks` fields;
  anything else is a model error. Either reply may include optional `resolves`, a list of
  `{"unknown":"FACT-ID","resolved_by":"DECISION-ID"}` links to an already-settled answer.
  Absence means no links; malformed or invalid links refuse the reply plainly, saving no
  findings, question or resolution. An answer saved before that step stays saved.
  [The ledger](shape-ledger.md#2-what-shape-writes) owns eligibility, history and reopening;
  model coverage judgments are not semantic proofs. A question may include optional `prerequisites`, a list
  of existing decision IDs, preserved unchanged (absent means `[]`). Facts belong in `supported_by`,
  not prerequisites. The ledger alone refuses unknown IDs, cycles, invalid lists and deferred
  prerequisites (a deferral is not a settled answer), saving no
  findings, question or assumptions from a refused reply. A held question stays open without
  becoming current; the ledger selects the next ready decision. Once its prerequisites settle,
  the held question is promoted before any model call. No reply can settle, skip or reorder
  earlier decisions. `src/shape/prompt.txt` has one rule per line.
  The decoder tolerates redundant closing braces only after one complete JSON object;
  it never reconstructs missing fields or chooses between multiple objects. Invalid
  JSON, a missing question and an unsupported reply kind have specific reasons.
  The step finds the blast radius and the ways to meet the goal before choosing to ask or draft;
  a small live trial on #161 favored that order, which is evidence, not an eval.
- The launch allowance (saved calls or `--shape-budget CALLS`) defaults to zero; each step counts one before the model starts.
  Exhaustion refuses answer, defer, delegate, revision and teach-back before saving anything and explains how to allow calls.
- Tinkery, never the model, publishes answers and questions with the displayed version
  or the version at step start. Stale versions refuse, retain
  typed words and trigger a fresh read, never overwrite.
- Tinkery's bundled copy of the [seed contract source](https://github.com/jon-devlapaz/seed-me/blob/c9d766760dbeac0c05c92108165f9e968bec0a66/skills/seed-me/references/seed-contract.md)
  defines the artifact. Tinkery's ledger owns its canonical Full heading sequence;
  the reply fields and writer derive from that one layout. The model supplies
  `confirming` as an object: `You are confirming` contains exactly five non-empty
  summary lines, and each model-written prose section has its own string field named
  for the section, without heading syntax. Field order is immaterial. The summary
  covers goal, what gets built, what does not, the proposed first slice, and unknowns.
  Counts and reversal indicators belong to the ledger-computed confirmation summary.
  Drafting instructions retain names supplied
  in the confirmed goal or Think answers, including in the summary and knowledge
  map; a supplied tool name does not settle its executable path or invocation.
  Missing, blank, non-string or foreign fields
  refuse; prose cannot inject headings, fences, metadata or confirmation lines.
  Refusals name the rule and field, plus the offending line for structural text.
  A historical Markdown reply beginning `## You are confirming` is refused naming
  `confirming`, never with a generic format error.
  Shape attaches the bundled reference's section content guidance and check schema,
  not its instructions to write or paste artifact headings. The prompt defers to the
  generated field instructions, so there is no competing model-written layout.
  Tinkery writes every heading, `status: draft`, the goal-headline title,
  `revision: rN` one past the previous draft, and a separate `date:` line with the
  operator's local date (receipts stay UTC). An older writer can still increment the
  revision after rollback. Never replace a confirmed seed or draft with an open decision.
  Acceptance criteria, Decisions, Assumed and Evidence and uncertainty come only from
  recorded checks, decisions, assumptions and findings, in their canonical positions.
  Host-owned headings use LF; prose keeps its line endings. Ledger-owned blocks remain
  verbatim, with no extra wrappers. The exact generated title and origin-decision
  line treat goal comment markers as literal data during seed verification, only
  outside an existing comment. Their text and the confirmed goal remain unchanged;
  general heading and prose validation still refuse injected structure.
  Findings and question transport are unchanged.
- **Completion review runs within drafting**, with no extra call. Before producing a seed,
  the step compares settled decisions for contradictions and separates choices (`decided`)
  from observed completed work (`done`). Each model check carries its own non-empty
  `failure_case` string, including provisional and judge checks. The decoder removes this
  model-only field before canonical check validation and pairs it with that check's exact
  final statement; the model need not count a second list or copy statements (#469).
  A new blocker returns as a question, not a seed. The typed `review` object requires
  `contradictions`, `decided` and `done`. Older replies with ordered `review.failure_cases`
  strings still work strictly; mixed formats, missing cases and malformed cases refuse.
  Tinkery never duplicates, drops or invents cases to make coverage fit. Recorded clinic
  count mismatches and statement/case-object replies remain refusals.
  Saved check and review formats are unchanged. The ledger's `CompletionReview::validate`
  remains the validation boundary: it refuses nonempty contradictions, blank review
  entries and failure-case counts that differ from the exact check statements. Statements
  are descriptions, not unique IDs; repeated statements require the same number of cases,
  and a foreign case cannot replace a required occurrence. This keeps transport adaptation
  separate from saved coverage rules (architecture diagnostics 3 and 6), without new UI
  states or persistent fields. It checks structure and coverage, not semantic adequacy.
  The validated review is preserved verbatim in append-only confirmed-goal receipts
  (`tinkery:review/completed`), using teach-back's revision and draft SHA-256 binding
  and recording path. These receipts are interview records, not changed goal evidence.
  With the risk summary above confirming, Shape marks the review as `Drafting step guess`:
  settled choices, observed done (or `Nothing observed done.`), and each failure
  case as `Must fail: <case>` followed by an indented `Check: <statement>` line.
  Both strings stay verbatim; the label above them marks them as drafting guesses.
  Only the review for these exact draft bytes is shown. No check is run and F2's
  existing call budget is unchanged.
- The separate final `checks` field carries every confirmed `Done when` line with
  its qualifiers as records in the bundled contract, not model-formatted
  criteria. Before publishing a draft, each explicit `Done when:` line and its following
  bullet lines must each match one complete check statement exactly, without the heading
  or bullet marker. A statement that keeps a line's words in the clause form the goal
  block joined with `; ` matches too, once composed the same way; reworded, reordered or
  dropped clauses do not (#381). Splitting a sequence into weaker checks refuses the draft and names
  the missing statement; supplemental checks remain allowed. Unstructured goals and
  missing criteria add no requirements. This preserves words, not proof that a command
  exercises them; test adequacy still needs review. Existing seeds are not rewritten.
  This keeps the rule at the ledger boundary (architecture diagnostics 3 and 6), without
  new schema fields, calls or UI states. Tinkery's ledger stores those records unchanged and
  validates required fields, kinds, unknown fields and repository-relative
  `cwd`. Invalid records refuse drafting. There is no Markdown-criteria writing
  path. Existing legacy JSON acceptance sections remain verifiable but cannot be newly confirmed.
  Tinkery inserts its ledger's rendered acceptance block
  verbatim before Affected users and systems, then displays that saved card.
  No check executes during the interview.
  Copy settled commands exactly. Repository-grounded proposals name source paths
  and lines in `source`; behavior-specific regressions name the assertion expected
  to fail before the change or under targeted reversion. Inspection is not observed
  red or green. Routine setup or an unperformed reversion probe does not make an
  existing check provisional; missing assertions, tests or invocations do, with
  `missing` naming the gap. Never invent a runner or substitute a generic suite
  for a missing regression. For missing behavior, read the runner configuration and
  existing test names, then prefer a new behavior-named test file whose path does not exist yet.
  A provisional's proposed `cmd` runs only that file with the repository's runner; cite the
  runner configuration in `source` and name the absent test and its assertion in `missing`.
  An exact new test id is also suitable when the runner fails on an unknown id.
  Use only forms established to fail when absent; name-pattern filters can pass on no match,
  so do not rely on them.
  If no such form is known or available, omit `cmd` and explain why in `missing`.
  Keep suite runs as separate `cmd` checks. A new draft whose provisional
  proposes a command another check also uses in the same `cwd` is refused with both check numbers (#469);
  stored seeds are read unchanged. Taste stays `judge` with what the operator inspects.
  If requested behavior appears present, propose verification and repairs only for
  demonstrated gaps. Settled and explicitly deferred decisions stay settled;
  deferrals remain operator prerequisites, not chosen alternatives.
  Seed confirmation v2 binds canonical records, rendered seed bytes and decisions.
  Missing, changed or stale records invalidate readiness; renewed displayed drafting
  and explicit confirmation are required. A failure after storing records can leave
  unconfirmed records, never a verified handoff.
  These are drafting instructions, not a semantic completeness validator: stubs prove
  transport and preservation, not model quality or operator dogfood.
- Agent actions require `--operator agent` and [are recorded as the agent's](think.md#confirmation-attribution-185);
  seed confirmation never claims a human keypress.
- Confirmation uses Tinkery's ledger with the displayed revision and typed words as its source.
  As in Think, only the exact word `confirm` confirms (#614): anything else may be a correction,
  so F2 says `Type confirm, or F1 Revise the draft. Nothing was confirmed.`, keeping the words.
  The receipt and its `Confirmed by:` line record the word. Existing receipts keep the words
  they recorded; reads never rewrite them.
  Under its writer lock, the ledger rechecks the exact displayed draft bytes and ledger version
  before recording anything. A change keeps the typed words, refreshes the reading and requires
  confirmation again. Model replies cannot confirm; Tinkery refuses practice confirmation. Until the
  draft revision's last line has been on screen during this Shape visit, F2 says
  `PgDn or Down to 'End of seed' first. Nothing was confirmed.`; reaching the display-only
  `End of seed` marker after the last section clears that in the same frame. The saved seed is unchanged.
  Confirmation words are one line, so Up and Down scroll the draft while they are typed, for
  terminals or multiplexers that take PgUp/PgDn (#614).
  Before that marker is reached, the key bar says `pgup/dn read to end` instead of
  offering to confirm; the confirmation hint appears on the same frame that reaches the end.
  Notices keep their own recovery instructions without repeating the key bar's action.
  The exact revision and draft bytes retain that satisfaction through typing, box growth,
  resize and rereads, including teach-back. A new revision or changed bytes require reading again.
  Changing the words clears a refusal about them.
  A refusal retains the words and neither writes nor calls. The empty `your words` field shows
  the muted hint `type confirm, then F2` once the seed is read; typing hides it. Enter says
  `F2 confirms the seed; Enter does not add a line.` without changing the words.
  Tinkery owns validation and confirmation.
  Confirming is not approval to build.
- Failures name what happened and one `Next:`. A model failure, refusal or stop also
  says `Drafting runs left this launch: N of M.`, Think's wording, so a retry is chosen knowing
  its cost (#435). Budget exhaustion points to
  F1's Model and runs row, which adds runs for this launch; a model error or stop offers F2 retry, or F1's teach-back again
  (saved words stay saved);
  a stale version offers the fresh stage action; a ledger refusal shows Tinkery's first line.
  Review refusals name the mismatched check or case/check counts; a reply that also splits a
  Done when line hears both faults in one refusal. A check that keeps a Done when line whole but
  changes only its letter case is saved in the goal's own words; other wording still refuses.
  A missing check source explains what can establish it without inventing evidence. F2 retries explicitly,
  retaining saved answers; nothing retries automatically. During the same opened visit,
  the next explicit F2 retry receives the last reply-decoding, review or acceptance-check
  rejection, or ledger rule the reply's proposal broke (such as a reused node id), as
  `previous_refusal`, separate from the goal, answers and findings. Only the
  first diagnostic line is kept, at most 2,048 UTF-8 bytes, with truncation disclosed.
  The diagnostic is not saved. It must match the session, ledger version and working
  folder; a changed context, successful step, stop or transport/storage failure clears it.
  Storage failures never become model feedback. These checks prove transport and
  preservation, not that a model will correct its next draft.
  An oversized step sends nothing and keeps the words, asking the operator to start a
  smaller goal with this session as a reference. Esc stops a step; an
  already-started save finishes, then rereads. Stopped model calls still count,
  disclosed in the notice.

### Options and instinct first

The [drafting prompt](../../src/shape/prompt.txt) offers options only for a genuine choice
with supported trade-offs and undo costs. Compatible properties or unknown costs use the
existing question without options; the unresolved operator decision still gets asked.
These instructions do not validate model judgment; improvement needs a separately budgeted comparison.

A question reply may give exactly two options, each with its trade-off and undo cost (`cheap`,
`moderate` or `hard`), the strongest case against the suggestion, and a confidence. Anything
else gives no options. The ledger has no options field, so Tinkery records them as
`Option A:`/`Option B:` lines, then `Against my suggestion:`, under the question text, and reads them back from there.
The ledger has no separate confidence field. Reply confidence is not stored or shown;
Shape renders the recorded suggestion, not the transient reply. No schema field is added.
None of the options or case against reaches the seed.

- When either option is `hard`, The instinct-first rule applies: the question is recorded
  without its recommendation, and Shape shows the question and its finding with
  `This is hard to undo, so what's your instinct? F1 shows the options and my suggestion.`
- F1's **Show the options and suggestion** records the withheld suggestion through `publish`, then shows the options, the case
  against and the suggestion. The withheld suggestion lives only in this run; after a restart it
  shows the options and says no suggestion was kept.
- Cheap and moderate questions show everything at once.

### Probes

Drafting requires at least three probes before a seed, with at least one put to the operator
before any finding is shown. Shape puts the pre-mortem to the operator itself.

- With no current question and no `probe-pre-mortem` node, a step records `Probe: pre-mortem`
  (`Imagine this shipped and turned out wrong. What is the most likely reason?`) as the current
  question, without a model call or budget. The operator answers it with F2 like any question,
  in their own words, or types `skip`, as the screen says. Asked after findings were already recorded, it carries
  `NOT independent: asked after findings were shown.`
- The step investigates counter-example and outside-view probes as typed inference or unknown
  findings, asks instead of drafting when either raises an operator-only decision, and records
  consequences and a `Where we did not look:` line in Risks and verification. Only asked and
  answered ledger questions appear as operator answers in Decisions. Seed prose refuses the old
  `Probe:` / `— answer:` template, including inline in Knowledge map; unasked probes are readings,
  not answers. This format guard is not a semantic verifier of arbitrary model prose.
- These are instructions and transport; whether probes improve real seeds needs a separately
  approved budgeted trial.

### Assumptions

A step may list up to five defaults it chose instead of asking, where either answer leaves the
work, risk, cost, reversibility and behavior about the same. Tinkery adds them to the ledger's
`assumed` list with the question (once per text) and shows them under that question only, headed
`I'll assume these unless you object:`, each with its why. An assumption is never an answer.

- F1's **Object** then the assumption's number takes it off `assumed` and records an open decision,
  `Instead of assuming`, which blocks drafting. Esc says `Nothing was changed.`; other keys do
  nothing while asked. A refusal shows Tinkery's first line and keeps the list.
- The ledger's entries are exactly `{text, why}`, so "shown" lives with the question in this run,
  not in the ledger; the seed's Assumed block always lists what remains.
- With no current question, a step first makes the first ready open decision current, without a
  model call or budget, so an objection (or any recorded open decision) is asked before a new one.

### Ending a session

Ending is for good: Tinkery makes the session read-only and Tend stops listing it,
which Tend's readiness needs. It needs no model or budget.

- F1's **End this session** on an active Seed ready completes it; **Stop this session** on a question,
  waiting or draft stops it, keeping open questions recorded and typed words unsaved.
- Choosing it asks and says what ending costs; Approve still offers a completed seed;
  F2 then ends, Esc says `Nothing was ended.` Other keys and a held F2 do nothing while asked.
- Tinkery ends through its ledger with a stated reason, the shared writer lock and atomic save.
  Completion needs the verified seed, not a viewer or `--no-viewer`. Tinkery refuses an unverified
  seed or a conflicting ended status; an identical ending is a no-op. Shape shows the ledger's
  refusal or uncertain-save message without claiming nothing changed. Every ending is followed by a fresh read.
- A completed session's verified seed stays Seed ready, and Approve still offers it, since intake
  checks only the verified handoff. Tend doesn't list ended sessions.

### Not adopted from Seed Me

[#235](https://github.com/jon-devlapaz/tinkery/issues/235) compared Shape with Seed Me's interview.
Delegation (#239), deferral (#240) and typed findings (#241) are shipped through
[the ledger](shape-ledger.md). These stay out until their revisit condition holds:

| Seed Me feature | Why Shape leaves it out | Revisit when |
| --- | --- | --- |
| Conditional answers | Shape records "only if X" as a settled answer, not a condition to investigate; supporting it needs predicate tracking and a re-ask loop. | Dogfood shows conditional answers. |
| Browser viewer | Shape and Tend already show the ledger; status never opens a connection. | A separately approved viewing need cannot be met in Tinkery. |
| Lean path | Lean is one Markdown file co-edited by hand; Shape has a ledger and Think already shapes the goal. | Full seeds prove too heavy for small goals. |
| Agent mode | It tests the skill; Tinkery's stubs cover that, and a simulated seed never hands off. Agent work uses `--operator agent`. | A simulated run is needed in Tinkery. |
| Frontier progress | So far lists open and held questions; a progress count adds no next action. | Dogfood shows a need beyond the open-question list. |

### Grounding in the repository

- Before F2 is offered, the session read finds the folder: recorded Think repo,
  else the repository enclosing `--sdlc-root` when one is named (#467), else launch repo,
  else launch folder. Show `The step reads files in NAME. F1 picks
  another folder.` above words; F1 has the path. F1's **Choose the folder** opens `Folder:` with the current
  path. Enter inserts newline; F2 accepts an existing absolute folder or `~/…`.
  Try exact bytes first, including breaks and trailing whitespace; only if absent,
  try trimming surrounding whitespace. Esc keeps the previous choice; missing folders are refused. Selection lasts only this Shape visit, writes nowhere; step receipts name it.
- Each `found` becomes a settled `fact` on `evidence` authority, with a new `id`,
  `claim` and a receipt (`checked`, `at`, `observed`, `check: read-only file tools in FOLDER`).
  `claim.type` is `observation`, `inference` or `unknown`, and `claim.scope` names what
  was examined. Inferences cite observations through `supported_by` and state `limits`.
  Unknowns retain [the ledger's meaning and resolution rules](shape-ledger.md#2-what-shape-writes).
  A question cites its facts through `supported_by`; a fact may name a settled node it
  `contradicts`. The ledger alone validates these rules and receipts; a refused reply
  saves no findings or question and shows its reason. An answer saved before the step stays saved.
- Question findings display `What I found: PATH:LINE: what was seen`; So far holds all.
  Seed-step findings also become facts, including blast radius, and later steps receive
  them with IDs and claims in `found_so_far`. Model replies cannot revalidate, confirm
  or settle a decision. Untyped `checked`/`observed` replies are refused, not guessed
  into types; existing sessions' untyped receipts still read and render unchanged.
- `found_so_far` excludes confirmed-goal provenance, superseded facts and resolved unknowns;
  recheck relied-on active findings. Resolved unknowns remain in ledger history, not active
  context or the seed's Decisions and Evidence blocks, including copied receipts.
- `Grounded in:` lists deduplicated paths cited by recorded read-only findings.
  Missing read methods, absent citations and failed citation checks establish no
  read path; say when none are known. These are reported reads, not verified
  filesystem origins or an inventory of unrecorded reads; name those provenance
  gaps rather than inventing them.
- The evidence section groups findings by check method, with claim type, scope,
  inference limits and support IDs for typed facts. Legacy receipts retain
  `- PATH:LINE: what was seen (observed DATE)`. Each finding appears once in this section.
  Collapse internal whitespace to
  prevent heading injection. With none:
  `- Nothing was recorded: no step said what it checked, so this seed isn't grounded in a repository.`
- File citations use `PATH:LINE` or `PATH:FIRST-LAST`, with `-`, `–` or `—` and optional
  spaces around the dash. Commas can add ranges for the same file (`PATH:1–3, 7–9`)
  or another file citation. Check every range, not a valid prefix of a malformed token.
  Invalid, zero, reversed or overflowing bounds, missing/outside/unreadable files and
  too few lines are flagged, not refused: the check ends `; citation not found: WHY`,
  questions show `(citation not found: WHY)` and the seed groups it under that check.
  Flagged receipts establish no Grounded in path. Plain prose establishes no citation;
  receipt words stay unchanged. Valid bounds prove line existence, not what the lines say.
- Questions or seeds with no findings display `Nothing found: this question records nothing
  checked in the repository.` (seed wording for seeds), flagged rather than refused.

## Rationale and verification

[Tests](../../tests) cover these contracts with stubs and temporary TEST roots.
Shape checks saved drafts against the bundled contract and a [pinned consumer section list](../../tests/fixtures/tink-sdlc-intake-sections.txt),
never an operator's consumer checkout. PTYs use Tinkery's real Rust ledger in temporary TEST roots and cover
80×24, 100×30, 160×40 and 320×40 where applicable, including NO_COLOR. These establish
mechanics, not operator acceptance or the quality of real goals, questions or seeds. Lock timeout
coverage does not establish pre-deadline cancellation; matching read-only no-op
preservation has no dedicated reversion probe. Static graphs are a design choice,
not a comparative user-study result.
