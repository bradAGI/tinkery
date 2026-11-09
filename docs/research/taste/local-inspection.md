# Inspecting a local result before its human judgment

Research for [#787](https://github.com/jon-devlapaz/tinkery/issues/787), 2026-11-09, at
Tinkery ab23b20 and tink-sdlc 8848d0c. It ships nothing and relaxes no contract.

## The loop

Taste's [local preview](../../design/taste.md#local-preview) runs only for a current passed
verification. A seed judgment (`operator_attestation: true`) passes only after a person marks
it with `sdlc.py mark --reviewer --source`. That person needs to see the built result first.
So the preview waits on the judgment, and the judgment waits on seeing the result.

`taste_local_preview_terminal.py` case `pending-human` reproduces this with zero calls. It
uses tink-sdlc's real failed-receipt shape, checks that Taste refuses and says only a human can
record the verdict, and checks that `v` launches nothing. With the guard removed, the case fails.

## What tink-sdlc reports today

`verify` checks the checklist last, after every configured and seed check has passed and the
inputs are proven stable. When a judgment is still unmarked, it raises `Checklist incomplete`
and writes `{result: failed, error, inputs, seed_checks}`. The candidate snapshot it has just
checked is written only on a pass.

Status then reports `verification_status: failed` and `verification.candidate: null`.
Automatic checklist items read `pending`, because they read `passed` only when the receipt is
current. The `seed_checks` map stays inside the receipt and is not part of the API 1 view.

| State | API 1 words | Candidate | Can Taste tell it apart? | Preview |
| --- | --- | --- | --- | --- |
| Everything passed and marked | `current`, `passed: true` | head and tree | yes | runs (shipped) |
| Machine checks passed, a person's judgment pending | `failed` | null | only from `error` prose | refused |
| A machine check failed | `failed` | null | only from `error` prose | refused |
| Stale identity: pass no longer matches, or source/declaration/run changed | `stale`, or Tinkery's source and reread checks | old or none | yes | refused, asks for `r` |
| Unknown candidate: `not-run`, `running`, `interrupted`, `blocked`, `invalid`, an unfamiliar word | as named | none | yes | refused |

Rows two and three differ only in the error text. Prose is not a machine interface, so Taste
cannot trust any candidate before full verification today. **The API cannot provide one.**

## The two options

**A. Disclosed "inspect unaccepted candidate".** Taste would offer the same preview, clearly
labelled as not yet accepted, for a source-owned candidate whose machine checks all passed and
whose only remaining items are human judgments. It needs one tink-sdlc change first. On the
`Checklist incomplete` path, `verify` would also record the candidate, policy and test lock it
had just proven, plus the ids still awaiting a person. Status would then report a distinct word,
for example `awaiting-attestation`. It would report that word only while the same inputs still
match, and before any later mark. Tinkery would then accept that one word besides `current`.
Every existing guard stays: exact commit, committed `tinkery.preview` blob, run, seed and goal
identity, rereads before preparation and launch, loopback URL, explicit `v`, process cleanup.
The view would never call the result checked or accepted.

**B. External preview handoff.** Taste would show the declared command and a commit, and the
operator would run it themselves. Without a candidate, the only commit Taste could name is the
checkout's HEAD, which it would be inferring. The handoff also gives up temporary checkouts,
loopback readiness and owned cleanup. An operator can already do this without Taste, as the
dogfood did. B adds a display, not a trusted identity.

## Recommendation

Choose A, in two separately approved slices:

1. **tink-sdlc:** record the proven candidate and the ids still awaiting a person on the
   `Checklist incomplete` receipt, and report a distinct, freshness-checked status word.
   Old Tinkery builds treat the unknown word as unknown and keep refusing.
2. **Tinkery:** accept that word in `Local::read` under an "unaccepted candidate" label that
   names the pending judgments and repeats that only a person can record them.

Until slice 1 ships, keep today's boundary. Without a source-owned candidate, any preview
would present an inferred commit as the result.

The chain stays bound end to end. A mark records the working-tree digest it was made against,
the same `tree` the receipt names. Status flags a later tree as `needs_recheck`. Recording a
mark changes `manual_marks`, so the person's verdict always needs a fresh `verify` before the
result reads `current`. Inspection gathers evidence for that verdict and never records one.

## Limits

One contained dogfood (2026-11-08) showed the loop. It gives no frequency or time-saving
estimate. Wording for the Shape choice that creates these judgments belongs to
[#790](https://github.com/jon-devlapaz/tinkery/issues/790) and does not change who signs off.
