---
name: retro
description: "Conduct a retrospective on a coding session."
disable-model-invocation: true
---

Suggest environment improvements for future agent runs; present candidates, not
unrequested changes. Tinkery's rules live in [AGENTS.md](../../../AGENTS.md) and
`docs/design/`, not a separate standards file or reviewer agent.

## Steps

1. Read [writing-for-agents](../writing-for-agents/SKILL.md).
2. Read primary sources for the specified session, including local logs when needed;
   default to this session. For a named tink-sdlc run, read its records in that checkout
   (`python3 _system/scripts/sdlc.py status <run> --json`, then `runs/<run>/`), the Shape
   session and Think record they bind to, and the worker session named by its build step.
3. Find improvements using the checks below.
4. Present candidates in severity order.
5. For a named run, end with one ready-to-run `sdlc.py label` command per correction, for
   the operator to run in that checkout. Never run `label` yourself.

## Checks

- **Navigation:** if discovery was slow or dependencies hidden, identify a missing pointer.
- **Automation:** read existing check commands and CI first. An unwired or broken check is
  the finding, not a reason to reinvent it. If neither a hook nor CI runs lint, types or tests,
  that is itself a gap. Mechanical violations (syntax, banned APIs, imports, file locations)
  require deterministic checks in the cheapest existing linter, hook or CI boundary,
  not prose rules. Reserve review guidance for genuine judgment calls.
- **Context:** review large project and global steering files for rules better enforced
  by automation or consulted during review. Delete no-ops; disclose task-specific detail.
- **Corrections:** for each operator correction, name the layer that holds its fix now
  and propose the leftmost one, per Yohaku's [move each correction left](../../../docs/yohaku.md#for-anyone-changing-tinkery-human-or-agent).
  Count corrections apart from required decisions (confirm, approve, merge): a fix may
  remove a correction, never a decision. As a label command:
  `python3 _system/scripts/sdlc.py label <run> --kind correction --words <their words> --reviewer <operator> --source <'retro DATE: fix in LAYER'> [--about <checkout-relative file>|outside]`.
  Shell-quote each value (as Python's `shlex.quote` does) so the words stay verbatim.
  With no corrections, propose `--kind none --words 'No corrections'`; a run closes only
  once labelled.
- **Tool economy:** identify expensive, repetitive or token-heavy calls, CLIs and MCP tools.
- **Access:** identify missing evidence sources, such as captured server logs or read-only
  service access; access changes still require permission.

## Reference

Implementation carries context from exploration and debugging; review starts from a diff.
Put judgment-oriented standards in review, keeping implementation's always-loaded
`AGENTS.md` and `CLAUDE.md` sparse and pointer-led. Prefer existing docs for reference;
add navigation if a standards file exceeds 1,000 lines. Use skills for discoverable
reference or user-invoked commands, following writing-for-agents invocation guidance.
