# Watch

Watch shows the runs in one tink-sdlc checkout, read-only, and puts the runs waiting on a
person first. It is a view, not a touch: it decides nothing, starts nothing and changes nothing.

## Source

The operator names the checkout with `--sdlc-root PATH`. Watch never discovers or scans for
checkouts: tink-sdlc's guide says opening a checkout trusts its executable code, so only an
explicitly named one is read. Without a root, Watch says how to name one and runs nothing. A
root without `_system/scripts/sdlc.py` is refused by a file check, without running anything.
An empty `--sdlc-root` is refused before startup; use `.` to explicitly choose the current folder.

Watch reads with that checkout's own `python3 _system/scripts/sdlc.py status --json`, from the
checkout's folder, when Watch opens and when the operator presses `r`. It then reads each valid
run's `status <slug> --json`, serially, before offering the list (except older runs below): the summary cannot identify owed
attestations or gates during a running verification. An unreadable detail refuses the list rather
than claiming that run is quiet. Enter reads the selected run's detail again. Both reads require the detail's slug and run ID
to match the original list row. A changed or missing identity refuses the facts and asks for
an explicit list refresh; it never borrows another run's status, goal or attention.
The Watch reader owns this comparison for both paths (architecture diagnostics 3 and 6);
callers cannot accidentally choose a weaker detail parser. The screen names the checkout and the exact command behind what it
shows, with how long ago it was read, to the minute. Watch never runs `verify`, `mark`, `decide`,
`stage` or any other command, and never polls, refreshes by itself, notifies or badges anything:
unchanged state makes no new demand on the operator's attention.

## Reading

Each read runs off the paint path with a 30-second deadline, at most 4 MiB of output and its
own process group. Leaving Watch, or closing a run's detail, stops and reaps the read; a late
result is never shown. Normal success and failure also stop remaining children in that group
before returning the result. Descendants that detach into another group are outside this ownership.
A refused read shows tink-sdlc's own `error.message`.

Completed reads wake the UI after publishing their result, rather than waiting for the
100-millisecond heartbeat. List and attention reads remain serial; a wake only collects
requested work, never refreshes or starts an extra read. The terminal owns ordered input
and waiting; the shared reader owns completion notification (architecture diagnostics 3 and 6).

The shared `sdlc` boundary owns the bounded status process and envelope validation;
Watch owns attention and presentation. Parsing and original-seed reads run on the
worker, never during painting or input. The [SDLC handoff](shape-and-handoff.md#sdlc-handoff)
owns matching and keeps three states distinct:
- A matched seed supplies the exact saved goal.
- An explicit null record means no confirmed seed was used, not a failed match.
  Show nothing in the list; detail says `No confirmed seed was recorded for this run.`
- An edited, moved, missing or unreadable original, or a missing/malformed record,
  says `Goal unknown: the original seed changed or can't be found.` in needs-you
  rows and detail. Never borrow a goal from the run name or another original.

Watch reads only tink-sdlc API 1: `protocol` must be `tink-sdlc` and `api_version` the integer 1.
Any other interface, an unknown `verification_status` or gate state, or a run missing a field
fails the whole read closed, with what was wrong, rather than display a guess.

An API 1 invalid summary whose exact reason is the canonical-run-ID requirement is
an `older run`, not `can't be read`. Show `This run has no canonical run ID. Its approvals
and verification can't be checked here.` under Needs you; never infer stale approval or
verified work. Its detail uses that summary, names the same gap and offers Esc, without
requesting the unsupported per-run status or inventing gates and checklist counts.
`r` from this detail rereads the summary and returns to the list. Other invalid reasons
stay invalid. Watch neither migrates nor edits run files; tink-sdlc is unchanged.
An agent saying it is done is never verification or acceptance evidence.

## Codex observation

`--codex-report RUN_UUID:CHAT_ID:PATH` binds one saved observation to one run and chat for
this launch. It requires `--home` and `--sdlc-root`; repeated bindings, empty parts and
noncanonical UUIDs are refused. The run ID must be UUIDv4. Colons after the second delimiter
belong to the path. There is no chat discovery, history reading or live connection.

The named UTF-8 JSON file has this format. `next_step` is advice from the reporting agent,
separate from Codex's observed status:

```json
{"schema_version":1,"run_id":"00000000-0000-4000-8000-000000000729","thread_id":"00000000-0000-7000-8000-000000000729","observed_at":"2026-11-04T12:00:00Z","status":{"type":"active","activeFlags":[]},"next_step":"Review the terminal result."}
```

Watch reads it on the existing background detail read, only for the bound run with explicit
`closure: null`. Closed runs and unknown closure skip it, even if the file is malformed.
The opened file must be regular and at most 8 KiB; Unix opens are nonblocking so a FIFO
cannot stall Watch. Both identities must match. Unknown fields, versions, statuses and flags,
invalid or future RFC3339 times, and empty advice or advice over 512 bytes are refused.
Unsafe controls are escaped for display. These guarantees belong to `watch::codex`, keeping
file and status rules behind one boundary, architecture diagnostics 3 and 6.

Accepted statuses are `active` with `activeFlags` containing only `waitingOnApproval` or
`waitingOnUserInput`, `idle`, `notLoaded` and `systemError`. Watch labels every observation
`Codex last reported`, keeps its original timestamp and age across refresh, and directs the
operator to the named chat for current status. Age alone does not invalidate an observation.
Idle is not proof of completion. Waiting, unknown or unavailable reports lead under Needs you,
without creating attestations or changing SDLC state. `SDLC next` stays separate from
`Codex suggests`; SDLC gates, failures and closure retain their own meaning.

## Needs you first

A run needs a person when tink-sdlc reports it:

| `verification_status` | Shown as | Why it needs a person |
| --- | --- | --- |
| `blocked` | waiting on a decision | tink-sdlc reports `blocked` only while a gate isn't approved |
| `failed` | verification failed | someone has to look at the failure |
| `interrupted` | verification interrupted | the last pass didn't finish |
| `invalid` | can't be read | the run is broken; its reason is shown |

These lead, as do runs whose detail has any unapproved gate, an owed manual attestation, or an
attestation flagged `needs_recheck`. A non-automatic item whose `status` is not `passed` is owed,
except provisional (`pending: true`) items awaiting activation. A passed attestation with
`needs_recheck` is shown as **may need recheck**, not declared invalid: upstream says to recheck
only if the change affects it. Automatic pending checks alone do not demand a person.

A worker whose turn ended also leads: tink-sdlc's `steps` report each pinned build step and,
for an active one, its `turn`, and its next action says to read the report. Rows add
`worker running`, `worker's turn ended` or `worker state unknown`; a run awaiting building
with no active step says `no active worker step`. This says nothing about work outside those
steps. Detail lists each step. A tink-sdlc without `steps` adds nothing; an unfamiliar
step state or turn refuses the read.

This interpretation follows API 1's `checklist_view` and `status_state` in
[tink-sdlc at 1c6ffd59](https://github.com/jon-devlapaz/tink-sdlc/blob/1c6ffd59902a742a2b3bb607ab75c41dc2e95e55/assets/_system/scripts/sdlc.py).
Watch requires the typed `automatic`, `proof`, `status`, `needs_recheck` and `mark` fields;
missing or unfamiliar fields are not quiet. Each group keeps tink-sdlc's order. Needs-you rows
retain tink-sdlc's `next_action` (or the invalid run's reason). Under them, show
only the goal's first line verbatim, or its unknown line, indented four spaces in ink.
A known no-seed run adds nothing. Quiet runs stay exactly one clipped line, without
a goal or seed-status sub-line, except the bound Codex row adds its SDLC next action and
saved observation. Selected-row scrolling accounts for wrapped needs-you
sub-lines and keeps the selected run visible.

A run's detail keeps the full matched goal, wrapped and untrimmed, escaping unsafe
controls only for display. Its state, next action, gates, checklist counts, individual
expectations and recorded evidence follow, plus tink-sdlc's errors. The checklist
summary separates passed results from checking methods: `Checklist: 0/4 passed.
How each is checked: 0 automated, 4 by you.` describes how checks are done,
not how many attestations have been given. Individual
items retain their reported status, including `pending (attested)`.
Complete detail text must be under 60,000 bytes, counting span text plus one byte per
line, including blank lines, labels, separators and appended errors. Larger detail refuses
before wrapping, with the external `status <slug> --json` command and Esc available;
source data and other runs stay unchanged. This conservative bound keeps accepted text
inside the existing scroll range (architecture diagnostics 2 and 6), without a second wrapper.
↑↓, PgUp/PgDn and Home/End scroll wrapped content,
including long evidence and error tails. Opening or rereading detail resets to the top. One run
at a time; Esc returns to the list. F10 returns to Home from Watch, including help,
without quitting Tinkery. Scrolling launches nothing.

## Verification

Unit tests in `src/watch/tests.rs` cover the ordering (a waiting run never sorts below a quiet
one), interface and state refusals, detail counts, the deadline, leaving mid-read (the process
is gone afterwards) and that opening and reading again leave the checkout byte-identical.
The fake reader atomically publishes its PID only after writing it; file creation alone is not
readiness. A regression holds the write open to prove cancellation cannot observe an empty PID.
This keeps process-lifetime mechanics behind the shared reader (architecture diagnostics 3 and 6);
no production interface or cancellation policy changes.
[The terminal journey](../../tests/watch_terminal.py) drives the real binary over a fake
`sdlc.py` at 100x30, 80x24 and with NO_COLOR, including owed-attestation ordering after successful
verification, every verification state, scrolling to long evidence/error tails, and
matched, edited, moved and missing originals, no-seed runs, one-line quiet rows and
older runs missing canonical IDs, summary refresh and no unsupported detail calls or file changes.
Unit tests cover all three goal states in list/detail, exact first-line indentation,
full detail words, wide quiet rows and selected-row scrolling. Stubs prove
mechanics, not operator dogfood.

[The Codex terminal journey](../../tests/watch_codex_terminal.py) checks the explicit binding,
reported states and advice, original age after refresh, invalid and unavailable files,
closed runs and unchanged source bytes at 100x30 and 80x24 with NO_COLOR. It uses saved
fake observations and no provider calls; it cannot establish current chat activity.

The three-way goal reading and compact list/detail split improve architecture diagnostics
2 and 6: absence cannot masquerade as a failed match, and display rules stay behind
Watch's existing interface. No cross-phase imports or relaxed limits are introduced.
