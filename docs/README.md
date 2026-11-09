# Documentation

Start with the [README](../README.md) for shipped behavior. The
[Yohaku constitution](yohaku.md) governs decisions; this index only routes you
to the smallest relevant reference.

## Choose by task

| If you are working on… | Read |
| --- | --- |
| A first real task, from install to a running worker | [First task](first-task.md) |
| Terminal setup or a real drafting trial | [Terminal setup](terminal-setup.md) |
| CLI, rendering, input, graph or presentation | [Principles](design/principles.md), plus the [Pinstar API](../vendor/pinstar/API.md) or [Graf API](../vendor/graf/API.md) when changing those libraries or adapters |
| Module boundaries or shared code | [Architecture](design/architecture.md) |
| Think interactions, records or model evaluation | [Think](design/think.md), [Think evaluation](design/think-evaluation.md) |
| Goal composition and rendering | [Goal block](design/goal-block.md) |
| Saving, history, Tend, Shape or continuation | The relevant section of [Tend and handoff](design/shape-and-handoff.md); [Shape ledger](design/shape-ledger.md) records its accepted design decisions |
| Approvals, external work, result review or measurement | [Approve](design/approve.md), [Watch](design/watch.md), [Taste](design/taste.md), [Menu and Measure](design/menu.md) |
| Local dictation | [Voice](design/voice.md) |

Read only the relevant sections. For a change, the applicable contract and
current shipped behavior outrank proposals, snapshots and old issue status.

## Published work and proposals

Issues own proposal scope and status. Publication is not design acceptance,
implementation permission, data permission or budget.

## Recorded evidence and retired proposals

These dated records are not current behavior or permission to implement:

| Material | Status and use |
| --- | --- |
| [Think interaction proposal](research/think/interaction-proposal.md) | Dated mockup and audit; unresolved redesign is not accepted. See [#427](https://github.com/jon-devlapaz/tinkery/issues/427) for its disposition |
| [Question-generation research](research/think/question-generation.md) | Research comparison, not a shipped behavior specification |
| [Human baseline](research/shared/baseline.md) | A recorded evaluation plan and partial results, not a code-quality score |
| [Project preparation](research/shared/project-preparation.md) | Research on what a new project needs before a run, and who owns each step; ships nothing |
| [Local inspection before a judgment](research/taste/local-inspection.md) | Research for #787 on previewing a result whose human judgment is pending; ships nothing |
| [Dogfood findings](../findings/README.md) | Historical observations and evidence; not a general outcome claim |

For shipped behavior, use the relevant contract above. For proposals, use the linked
issue. For historical claims, use the dated evidence and its stated limits.
