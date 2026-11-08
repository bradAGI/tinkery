# Tinkery

## The big idea

Tinkery is a calm substrate where a messy thought becomes useful, checked agent work,
without losing your words or your control. Nothing runs without you: one next step, with a way out.

## The journey

**Think → Tend → Shape → Approve → work happens → Watch → Taste → Measure**

| Place | Purpose |
| --- | --- |
| **Think** | Clarify what you want: one reading, one question at a time. |
| **Tend** | Choose a saved goal and open its detail. |
| **Shape** | Investigate and turn the goal into a confirmed seed. |
| **Approve** | Understand the decision and exactly what you're authorizing. |
| **Watch** | See progress, blockers and decisions needing your attention. |
| **Taste** | See the result beside your original goal and say whether it is good. |
| **Measure** | Judge accepted, checked results against your effort. |

This is the product flow, not all shipped behavior. Originals remain verbatim;
confirmed goals stay unchanged through handoffs. Readings and suggestions are guesses.

## What Tinkery builds on

- **SDLC** organizes external implementation, checks and review.
- **GitHub** holds issues, PRs, checks and reviews.
- **You** authorize work and judge results.

Tinkery connects those records; it doesn't invent a competing set of approvals.

Goal confirmation, brief confirmation and implementation approval are separate.
Copying a prompt authorizes none of them and starts no agent. Voice is local dictation.

## What works today

- **Think:** clarify a dump and explicitly save the confirmed goal, unchanged, to
  `~/.local/share/tinkery/sessions/` with a record in `~/.local/share/tinkery/think/`.
  Questions offer a suggested answer when available: F1 can copy it for editing; F2 sends.
  When started in Git, the menu names the repository and the record keeps its origin.
- **Tend:** active goals form a needs graph. Link or unlink goals, open detail, or copy
  Shape without changing saved sessions.
  Press `r` in detail for [Think history](docs/design/shape-and-handoff.md#think-history)
  when its saved record verifies.
- **Shape:** press `s` in goal detail or choose menu place 3 to see the current question,
  waiting draft or verified ready seed. With real drafting set up
  ([terminal setup](docs/terminal-setup.md#launch)), the saved calls or `--shape-budget CALLS` (default zero)
  set the launch allowance shared by Think and Shape. `F2` saves an answer and lets an agent investigate the next with
  read-only tools, and confirms the seed when you type `confirm` after reading it to the end; F1 Revise the draft
  sends a correction back for a new draft.
  Agents operate Think and Shape with `--operator agent`; their saves are
  [recorded as the agent's](docs/design/think.md#confirmation-attribution-185). `F1` lists
  every other action allowed now by number: So far (the whole goal and decisions), objecting
  to an assumed default, optional one-call teach-back on a draft (mismatches never block
  confirming), the options on a hard-to-undo question after your instinct, and ending a
  session, which asks once more. Tinkery owns the ledger, seed checks,
  confirmation, verification and ending; no viewer or network read is involved.
  `tinkery seed status --json DIR` and `tinkery seed checks DIR` expose read-only intake JSON.
  Its draft guidance and artifact reference are bundled; no external skill is needed.
  The agent's step reads the repository where the goal was made, else the one named by
  `--sdlc-root`, else where Tinkery started. A ready seed points to Approve.
- **Approve:** menu place 4 shows verified seeds, oldest first, with their exact goal
  and acceptance checks. `c` copies run creation and intake commands; nothing runs.
  `b` reviews an explicit bounded Pi start; after the full disclosure, F2 creates and
  imports the run, prepares its isolated build checkout and launches the worker.
  Supply `--worker-model provider/model:thinking --worker-responses N`; the default
  worker allowance is zero, separate from drafting.
  With `--operator agent`, F2, `l` and `u` share one relayed-authorization door:
  supply `--worker-authorization FILE` containing the operator's verbatim words bound exactly
  to the seed hash, model pin and response cap. Missing or mismatched authorization refuses
  before work. The review shows the words and authority `operator, relayed`; provenance
  cannot be verified. Each admission appends to `runs/<slug>/worker-authorization.jsonl`.
  Human and undeclared F2, `l` and `u` keep their existing direct consent without a relay file.
  Goal confirmation, seed confirmation and worker start remain separate yeses.
  See the [relay file contract](docs/design/approve.md#start-a-worker). Navigation never starts work.
  Recent Pi launch logs show how many
  responses workers used before editing. The review warns when your cap leaves too little
  room for an edit and a follow-up. The worker screen retains
  requests across navigation and offers explicit refresh, resume, finish, stop and
  verification. Home `b` reopens it; Watch `b` selects an existing run.
  Intake rechecks the seed through this executable with `--tinkery PATH`.
  A seed known to come from another repository stays readable but cannot be copied
  into the named checkout; unknown origins keep copy with a note.
  Seeds already started move to Watch; without a named checkout, start status is unknown.
  See [Approve](docs/design/approve.md).
- **Watch:** with `--sdlc-root PATH`, reads that tink-sdlc checkout's runs through its own
  `status --json` when opened or on `r`, runs waiting on you first. Matched runs show their
  exact saved goal; changed or missing originals stay unknown. It runs only read-only
  status commands and changes no records. An optional `--codex-report RUN_UUID:CHAT_ID:PATH`
  adds one saved Codex observation and suggested next step, with its original time and age.
  It is not a live connection. See [Watch](docs/design/watch.md#codex-observation).
- **Taste:** menu place 6 uses `--sdlc-root PATH`. `r` explicitly reads runs and their
  recorded PRs or current checked local commits; handoff prose supplies no links.
  `v` runs the disclosed `tinkery.preview` command from an exact-commit temporary checkout
  and opens its local URL when it answers. Esc stops it. Local results need no GitHub PR.
  The list pairs each available result title with a goal excerpt. Open a result to see
  what was built, your goal and the first human check reported pending.
  With no such check, it asks “Is this good?” Only a human can press `w` to write a verdict.
  F2 saves pass or fail with their words and the checkout's git name. Saving requires
  tink-sdlc 1.53.0 or later to check that the reviewed run is still the one being labelled.
  After a failed local result, `s` opens Shape with the confirmed goal, rejection words,
  checked commit and prior decisions. F2 saves the new attempt in a separate checkout.
  `e` shows checks, owed gates, the whole goal, IDs and evidence. For PR results,
  `o` opens the PR and `a`/`c` copy external review handoffs. Once a person accepts
  the result elsewhere, `l` reports that decision for this view and
  reveals one italic next-dump guess; `t` opens it for editing in Think. Only F2 sends.
  No unmet conditions means observe the accepted behavior, not invent another feature.
  See [Taste](docs/design/taste.md).
- **Measure:** `r` explicitly reads this repo's PRs updated in the last 30 days from
  GitHub through `gh`. Lists all states; Enter shows dates, author, URL and reported
  PR-head checks. No metrics or acceptance claims. See [Measure](docs/design/menu.md#measure).
- **Light or dark:** Ctrl-T or the `dark` label beside `?` switches every screen to warm
  charcoal until you quit; each launch starts light. See [principles](docs/design/principles.md#visual-meaning).

On first default-root launch, old saved sessions move once to Tinkery's sessions folder,
with a visible notice and no Think-record writes. History matches unique ledger session IDs. Leftover old and new roots
refuse rather than merge; no old-root reader remains. Runs bound to moved seeds show unknown
in Watch, not a guessed match.

## What success means

**Useful, verified changes per hour of your attention**, not more agents or code.
Active effort is not elapsed time; passing checks is not your acceptance. Unknown
evidence stays unknown. Improvement needs a separately permitted baseline and trial;
it is not a measured claim.

## Try it

Fresh Debian or Ubuntu clone:

```bash
./scripts/cloud-setup.sh
export PATH="${CARGO_HOME:-$HOME/.cargo}/bin:$PWD/.cloud-tools/node/bin:$PATH"
cargo +stable run --locked -- --home
```

Think is practice: no model calls or saved goals. Tend writes needs only when you
explicitly link or unlink. F2 (or Cmd+Enter, Ctrl+Alt+S; see [Function keys](docs/terminal-setup.md#function-keys)) sends, Enter adds a line, F1 lists what else you can do, Esc returns to the menu.
Add `--snapshot` for text output.

Real drafting needs a model and an explicitly authorized call allowance; provider
charges may apply. Press `m` on the menu, or click the `practice` label, to choose the
harness (Pi or Claude Code), model, thinking level and calls each launch: `s` saves them in
`~/.config/tinkery/model.json` and later launches draft with them. `--shape-pi --model
provider/model-id` and `--shape-budget CALLS` override them for one launch and save
nothing. Out of calls, F1's Model and calls row adds more for this launch only.
`--help` lists options. See [the Model panel](docs/design/menu.md#model-setup).
[Terminal setup](docs/terminal-setup.md#launch) gives an isolated trial.
[First task](docs/first-task.md) is the one path from install to a running build worker, with a
readiness check that makes no model calls.

## Read next

- [Documentation index](docs/README.md): find the governing rules, task contracts,
  setup, proposals and recorded evidence.
- [Constitution](docs/yohaku.md): governing principles.
- [Working guide](AGENTS.md): macOS setup, verification and contribution rules.

GPL-3.0-only; see [LICENSE](LICENSE).
