# Preparing a new project for a run

Research for [#737](https://github.com/jon-devlapaz/tinkery/issues/737), 2026-11-06.
The question is what still needs a coordinator before Think can start on a project that
has never used Tinkery. It also asks who owns each step and what the smallest useful slice is.
This note is research only; it ships nothing.

## What happened in the runs

- **Strudel, 2026-11-04 and 2026-11-05.** Before Think, a coordinator did four things:
  - installed tink-sdlc;
  - chose the checks (vitest run, lint, build);
  - installed the dependencies;
  - ran the baseline.

  This took zero calls, one Mac permission prompt and about 15 minutes. The build worktree
  then had no `node_modules`, which tink-sdlc's declared `setup` (tink-sdlc #161) now covers.
- **Vault links, 2026-11-06.** The project was greenfield: a README plus tink-sdlc, with
  `python3 -m unittest` declared even though no tests existed yet. Untracked bytecode then
  blocked the worker launch. The cause was Tinkery's first-task check (fixed in #741), not
  the scaffold. The baseline fails until the run writes tests, and that's intended.

## Steps and owners

| Step | Owner | Why |
| --- | --- | --- |
| Pick the checkout | Operator | Only they know which project and branch |
| Install the scaffold (`init.py TARGET`) | Mechanical, on the operator's word | It writes files into their repo |
| Choose the checks (`verification.json` argv and timeouts) | Operator | Tinkery never infers commands from package files (the Taste preview rule); a file name proves nothing |
| Choose `setup` (for example `pnpm install --frozen-lockfile`) | Operator | Same rule: it is a trusted command |
| Write those choices to `verification.json` | Mechanical | Today someone hand-edits JSON; the choices are already made |
| Commit the scaffold and config | Operator's word, mechanical | `stage` reads the committed file at HEAD; launch refuses uncommitted edits |
| Run setup and the checks once (baseline) | Mechanical | Reports pass or fail per check; never a verdict |
| Accept the baseline state | Operator | Green, or a known failure such as "no tests yet" on a greenfield repo |

Only three of these need judgment: choosing the checkout, the checks and setup, and accepting
the baseline. The rest is typing and waiting.

## Where it belongs

tink-sdlc owns `verification.json`, its validation and `setup`. Tinkery should not learn a
second way to write it. The first-task check already reads readiness without writing, so
it is the right place to say "this project isn't prepared" and print the command, not to run it.

## First slice

Filed as [tink-sdlc #165](https://github.com/jon-devlapaz/tink-sdlc/issues/165).
In tink-sdlc, `init.py TARGET` accepts the operator's typed commands. It takes repeatable
`--check-argv JSON` with `--timeout N`, and an optional `--setup-argv JSON`. With those flags
it writes them to `verification.json` through the existing validator instead of leaving it
empty. `--check` previews the file it would write. It still creates no Git history, and a
repeat install still preserves an existing `verification.json`.

This removes the hand-written JSON and keeps every command explicit.

Later, separately:
- a read-only `baseline` command that runs setup and checks once in a throwaway worktree
  of HEAD;
- a first-task check line that names the missing preparation.
