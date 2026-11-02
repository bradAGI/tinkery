# The menu

| Key | Place | Scope |
| --- | --- | --- |
| 1 | think | Refine, review and explicitly save a goal. |
| 2 | tend | Explore active goals, needs, history and opening Shape. |
| 3 | shape | Read seed status; with a model and budget, answer and confirm ([Shape](shape-and-handoff.md#shape)). |
| 4 | approve | Read verified seeds not yet started and copy one runnable manual intake command ([Approve](approve.md)). |
| 5 | watch | Read runs with their matched goals in one named tink-sdlc checkout, waiting runs first ([Watch](watch.md)). |
| 6 | taste | Review an exact result against its unchanged goal; open its PR or copy an external review handoff ([Taste](taste.md)). |
| 7 | measure | This repo's recent pull requests, read from GitHub only on explicit request; no metrics or acceptance claims. |

Each place shows its one-line purpose beside its name; [menu.rs](../../src/menu.rs) owns the copy.
Name the purpose without borrowing another place's name. Taste can explicitly open
one [unsent next-dump draft](taste.md#after-external-acceptance) in a fresh Think
visit; the menu alone connects those phases, and starts no request. [Tend](shape-and-handoff.md)
owns the garden and handoff. Unbuilt places authorize nothing. Goal confirmation,
intake approval and implementation permission remain separate. The menu also owns
the deliberate [Ready Shape-to-Approve route](approve.md#from-a-confirmed-shape-seed),
including its exact Shape/Tend Back context and F10 Home.

The muted key bar `↑↓ move   enter open   ? help   q quit` has one blank row below.
The latest receipt keeps its row two blank rows below the list, aligned with place
names; without a receipt that row stays blank. Numbers align with the title and bar.
Inline purposes stay muted after the reserved count column. Tend's muted count appears
by its name only when [discovery](#session-discovery) is complete and nonzero.
Digits and aliases still work without bar clutter. Open places and help hide this bar.

Help overlays the upper-right (38×14), leaving place choices visible. At 80 columns
it may cover part of the inline purposes; closing help restores them. It has a thin muted border, jade title, bold ink headings and
keys, and muted action labels. A blank row follows the title. Key and action columns
align, with `Esc Close help` separate below; `m Model and runs` sits under `q`, then with colour `Ctrl-T Dark or light`. There is no ambiguous Leave heading,
new colour or background. NO_COLOR keeps the hierarchy. Help blocks underlying
digits, Enter, movement and row clicks; paste still does nothing. Esc closes help;
?, F1 or the corner click toggles it. q, Ctrl-C and F10 retain quit routes.
Closing restores selection; other places own their help.

Inside a launch Git repository, the title shows muted `tinkery  in <folder>` once.
Outside, show nothing: no creation offer or warning. Find the root by looking upward
for a `.git` folder or worktree file; run no Git command or read its contents.
This names location, not the goal's subject or permission.

Digits or arrows followed by Enter open a place; home `q` quits.
[Think's exit guard](think.md#sending-and-leaving) protects unsaved work.
`--home` opens the menu; otherwise Think opens. The installed launcher passes `--home`.

## Model setup

The header's muted label, left of the light/dark switch (or `?` without colour), shows real
state: `practice`, or the drafting runs this launch has left. Clicking it, `m` on the menu, or the
Model and runs row in Think's or Shape's F1 list (offered when the launch drafts) opens the
Model panel in Help's upper-right spot and frame. [model_setup.rs](../../src/model_setup.rs)
owns the file, parser, label and panel; the terminal loop draws them over every screen.

| Row | Change with `enter` |
| --- | --- |
| Harness | Cycles Pi and Claude Code. Changing it resets Model and any level the new one lacks. |
| Model | Picks the next model. Pi: the first `enter` lists Pi's models once with `pi --list-models --offline` (no model request), then each `enter` picks the next; if Pi can't list, it types. Claude Code: cycles `claude-sonnet-5-5`, `claude-opus-5-5`, `claude-haiku-5-5`. `t` types any model, such as one an extension adds that Pi doesn't list. Tinkery can't confirm the account may use it. |
| Thinking | Cycles the harness's levels: Pi `off` to `max`, Claude Code's effort `low` to `max`. |
| Runs | Cycles 0, 5, 10, 25 drafting runs each launch, shared by Think and Shape; 0 is practice. |
| Now | Only when this launch drafts: adds 5 runs at once, for this launch only. |

`s` saves Harness, Model, Thinking and Runs to `~/.config/tinkery/model.json`; `esc` closes and drops
unsaved changes. Nothing else writes the file, and saving changes later launches, not this
one. Drafting runs need a model. The panel names this launch's actual harness and model, including
flag overrides, or says it is practice with no model requests. Editable settings are marked
for later launches; editing or saving them does not change the displayed recipient.
A run counts a drafting attempt, including failures and cancellations. Tools or retries
may make several model responses within one run; this allowance caps runs, not responses or
spending, and no Tinkery setting caps a build worker's responses
([#651](https://github.com/jon-devlapaz/tinkery/issues/651)).
Input too large to send, or cancellation before Tinkery accepts the run, uses none. Tinkery does not automatically
retry a failed run. Stored `calls` and CLI names remain unchanged. Nothing in the panel starts a request.

One parser checks the file and the `--harness`, `--model`, `--thinking` and `--shape-budget`
flags. A file without a harness is Pi. Claude Code takes only full `claude-` ids, never an alias
whose meaning changes, so records name the model used. A launch's saved model and thinking
apply only to the saved harness. A later plain launch drafts when the saved setup allows drafting runs;
`--shape-budget 0` alone selects practice without changing that file. `--shape-pi` drafts for one
launch, each flag replacing its saved value, and writes nothing. A missing file supplies no saved defaults.
An unreadable or invalid file is refused with its path in the panel; explicit launch flags
still apply. The panel describes this launch separately from saved-file errors.
An `--operator agent` launch never inherits saved runs,
and its panel refuses to save or add runs: an agent's allowance is `--shape-budget` alone,
given by the operator, and the agent's panel says so. No credentials and no Pi config are read; Claude Code uses its own
login. [pi.rs](../../src/pi.rs) starts either harness with the same isolation, caps and budget
([#449](https://github.com/jon-devlapaz/tinkery/issues/449)). There is no build harness row:
nothing in Tinkery starts a build
([#422](https://github.com/jon-devlapaz/tinkery/issues/422)).

## Measure

Opening names the launch repo and discloses `r read GitHub`, using `gh` and the
operator's login. Outside a repo, explain how to launch inside one; results stay unknown.
Navigation, reopening, details, help and resizing start no commands. Only `r` from
the list reads or refreshes; held or modified keys cannot repeat a read.

Read all PR states updated since the UTC date 30 days before the request, newest
updated first. The heading names the phase and repo once; a quiet context line gives
the visible range and fetched count. Keep at most seven rows around the selection,
with aligned, muted number/state columns and blank space below. The selected title
wraps; PgUp/PgDn reveal unusually long titles in a bounded viewport. Other truncated
titles end in an ellipsis. `closed` means closed without merge. Details and help
retain the exact window date and read time. At most 100 PRs remain navigable;
requesting 101 detects truncation, disclosed as “more on GitHub.” No stale list or
empty-result claim survives a failed refresh. Missing `gh`, access failures and
malformed output say how to retry; stderr is not displayed.

Arrows select, Enter opens in-memory details, and arrows scroll them. Details show
author, dates, URL and GitHub's reported PR-head checks with the head revision.
These are not merge-commit checks, acceptance or proof of usefulness; missing checks
stay unknown. No browser opens, no goal is inferred and no records are written.

F1 or ? toggles Measure help. Esc closes help, then details, then returns to the
menu; q returns to the menu with help closed. F10 or Ctrl-C quits Tinkery. Leaving
cancels and reaps the owned process group. One read runs at a time, with a 30-second
deadline and bounded output. Pipe collection stops on cancellation or deadline even when
an unowned detached writer keeps a pipe open; Measure closes its readers without signalling
that writer’s process group. This policy stays inside the existing reader (architecture diagnostics 3 and 6).
There are no watchers, provider calls or disk caches.

[Terminal tests](../../tests) use fake `gh` in TEST roots at 80×24, 100×30, 160×40
and 320×40, with and without NO_COLOR. They cover states, details, empty/partial
results, failures, cancellation and navigation. Layout journeys also cover 70×49,
long Unicode titles, all fetched rows and selection through help/details/resize.
Reversion probes protect explicit reads, partial-result honesty, bounded list size
and title wrapping. The PR browser is
[#321](https://github.com/jon-devlapaz/tinkery/issues/321), with visual refinement in
[#338](https://github.com/jon-devlapaz/tinkery/issues/338); the broader results and
effort proposals [#155](https://github.com/jon-devlapaz/tinkery/issues/155) and
[#115](https://github.com/jon-devlapaz/tinkery/issues/115) own their current scope and status.

## Direction, not built behavior

Approve prepares manual intake; Watch shows read-only progress. [Taste](taste.md) supports
evidence-based review without an in-app verdict. Use session, run, ledger and PR artifacts, not harness
self-report. Missing sources stay unknown. Add no run-now, skip-review or global
auto-merge switch. Future launching requires separate seed confirmation and
externally enforced gates; menu navigation launches no agents.

## Session discovery

Sessions default to `~/.local/share/tinkery/sessions`;
[the one-time move](shape-and-handoff.md#continuation-contract) owns retirement of the old root.
Default roots require absolute, nonempty HOME. Otherwise discovery is unavailable:
no worker, scan, directory creation or empty-list claim.
Technical help names `--seed-session-root PATH`. Practice and snapshots still work;
live drafting refuses unsafe defaults before any host or helper. Explicit roots
also work in practice. Capture relative paths against the launch directory, refuse
empty paths, and give saving and discovery the same absolute destination.

`--record-root` selects saved history, not sessions, even without live drafting;
[Tend owns reading](shape-and-handoff.md#think-history). `--link-root PATH` selects
needs without `--shape-pi`. Its default is `~/.local/share/tinkery/tend` under absolute
HOME. Without absolute HOME, linking is unavailable but discovery works. Capture
relative explicit paths and refuse empty ones. The menu and standalone receipt
retain the destination. Configuration creates nothing. Tend refreshes links when
sessions land and picking starts; only explicit commits write them.
See [linking](shape-and-handoff.md#linking-needs).

Painting, counts and movement use memory only. One owned read-only scanner starts
on the first tick, Think return, deliberate Tend reopen or Shape return to detail.
At most one scan runs; pending requests coalesce into one follow-up. There are no
watchers or index; a rescan is a new scan.

| Read | Meaning | Display |
| --- | --- | --- |
| Not known | Nothing landed, or a new read is running. | Hide the count; show reading unless a nonempty garden exists. |
| Complete | Every record was read; a missing root means nothing was saved. | Show the exact active count if nonzero. |
| Incomplete | Some records are unreadable. | Show valid goals under [Tend rules](shape-and-handoff.md#tend-garden-and-detail) and a quiet unreadable count, but no menu count. |
| Failed | The root is unsafe or listing failed. | Show unavailable, without a count. |

Unknown, incomplete or failed reads never mean zero or no sessions. Stray files
and directories without `ledger.json` aren't sessions or incomplete reads.
New reads hide stale counts and empty-list claims until current results land.

Late results retain selection by session path, never row, and never reopen exited
views. Detail and picking freeze replacements until return. Shape return waits
for fresh same-goal detail and garden stage together. Receipt `s` waits for a read
capable of containing the save; pre-save reads cannot imply unavailable.

After three seconds from read start, Tend says `Still reading your sessions.`
Only the wording changes; no extra worker, spinner or timer starts. Navigation and
Esc stay live. A stuck scan occupies the worker for the session. No replacement
starts, the count stays hidden, and switching roots cannot start another read.
Timeout replacement would leak threads.

## Measured scope

`scripts/bench-menu.py` owns a POSIX PTY benchmark with temporary HOME, outside
the zero-call suite. It measures from monotonic spawn to the finished menu frame,
then from Down to the finished selection frame. Warm-cache timings are not portable
guarantees or cold-disk claims; the disk label came from `stat -f`.
Deterministic blocked-source tests, not timings, gate responsiveness.
