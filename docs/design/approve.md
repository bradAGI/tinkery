# Approve

Approve prepares the manual start of a shaped goal in tink-sdlc. It creates no run,
records no approval and starts no agent. Seed confirmation, definition approval and
implementation permission remain separate.

## Source and verification

The menu supplies its existing session discovery result, without Tend's repository
filter. Approve reads across that configured session root, oldest update first.
The menu also supplies its Think record root. Once per visit, Approve's worker reads
origins through `think_record::read_origins`, using the existing live identity and
exact-goal checks. It compares recorded repository roots with `Repo::enclosing` of
the canonicalized named checkout; `.git` contents are never read. Relative segments
and symlink aliases resolve for comparison only, not as a new handoff destination.
Different worktrees count as different repositories, even with the same folder name.
No skill or model configuration is needed. Only Tinkery-verified ready seeds not already started appear, including
completed sessions whose seed remains verified. Drafts, stopped sessions and unverifiable confirmations do not.
Failed discovery and unreadable seeds stay unavailable;
an incomplete scan cannot claim there are no waiting seeds.

`seed_me::ready` owns the handoff reading: exact confirmed goal, revision and the
seed's Acceptance criteria section, including original whitespace. It uses the
same ledger verification as Shape and owns the one canonical, shell-quoted intake command.
Seed and ledger bytes are checked before and after verification; observed changes
refuse the reading. This is a snapshot, not a live freshness claim. Return to the
menu and reopen to read again. Unsafe terminal controls are escaped only for display.

One menu-owned verifier survives visits. Reads run off input and paint paths through
disk reads. Leaving stops publication and further seed
reads; the current origin scan or file read finishes before the verifier is reused. Reopening while it
finishes refuses a replacement worker and asks the operator to reopen later. Late
results never reopen a view or populate a different visit.

## From a confirmed Shape seed

Ready Shape offers `F1`, then `1`: Open this seed in Approve. F1 alone only opens
its actions. The menu carries one ephemeral checked target: canonical session path,
session ID, ledger version and confirmed seed hash. These come from the existing
verified reading; no input or paint path reads files. This navigation is available
without model runs and neither copies nor grants approval.

The normal verifier checks the target's current source, eligibility and repository
origin. A targeted visit offers only that exact seed. Changed, missing, unavailable
or already-started targets explain why; another eligible seed is never substituted.
Repository mismatch still leaves the goal readable with no command or copy action.
There is no new command builder, registry, worker or durable routing record.

Esc returns to the originating Shape session through its existing asynchronous
read, preserving local words and its Tend return context. Changed or missing source
therefore does not retain a stale Ready hint. F10 returns Home and drops that origin.
Ordinary Approve visits retain their existing menu return. Copy remains a separate
explicit action with the same exact command and approval boundary.

## One seed at a time

The goal and acceptance checks precede the manual handoff explanation and commands:

```text
cd '<named checkout>' && python3 _system/scripts/sdlc.py new seed-title && python3 _system/scripts/sdlc.py stage seed-title 1 --seed-contract '<seed path>' --tinkery '<Tinkery executable path>'
```

One shell line changes to the explicitly named `--sdlc-root`, creates a run and
imports its seed and checks for definition review. The default slug comes from the
seed's H1 title (without `Seed:`): whole lowercase ASCII words joined by hyphens,
at most 40 characters, so folder, Watch and Taste rows stay scannable; only a first
word longer than that is cut. Paths use the shared canonical
shell quoting; no placeholder needs replacing. Existing run names still refuse creation.
Copying runs nothing and grants no approval. Missing checkout, an empty slug or
unsafe paths leave the verified seed readable with a reason and no copy action.
A known different repository does the same: a plain reason names both repository
folders and asks the operator to reopen with `--sdlc-root` naming the seed's origin.
It offers no command, and `c` does nothing. Names explain the refusal; only root paths
establish identity. Matching origins keep the existing copy action.
Older records, missing or unreadable Think records, and an unresolvable checkout
leave the origin unknown; a muted note says so without adding a copy refusal.
A known origin outside any repository likewise keeps copy with a muted note.
These readings establish where Think ran, not what the goal concerns or whether it
has started elsewhere. Without `--sdlc-root`, origin handling changes nothing.
Copy uses the existing OSC 52 transport. Success says “Copied. Run it in a terminal;
after it succeeds, open Watch.” Failure keeps its retry notice; selecting another
seed clears feedback. For
[agent-confirmed seeds](think.md#confirmation-attribution-185), show
`Agent-confirmed seed` and the tink-sdlc 1.38.0-or-newer intake requirement alongside
the same copy action. This prepares text without checking the consumer version or
running intake; older consumers may refuse. Agent confirmation is not the operator's
confirmation, definition approval or implementation permission. Missing or invalid
agent attribution still refuses verification.

| Key | Action |
| --- | --- |
| `n` / `p`, Right / Left | Next / previous seed, clamped. |
| Up / Down, `k` / `j`, PgUp / PgDn, Home / End | Read wrapped content. |
| `c` | Copy the combined command, only when available. |
| F1 / `?` | Help and bounded unreadable-session details; no underlying actions. |
| Esc | Close help, or return to the originating Shape session for a targeted visit; otherwise return to the menu. |
| F10 | Return to the menu. |

The footer always retains navigation. When the reading doesn't fit, its last row says
`↓ more`, `↕ more` or `↑ more`, so the command below the goal is never mistaken for absent.
Empty state points to Shape or Watch.
[SDLC handoff](shape-and-handoff.md#sdlc-handoff) owns the exact join: matching
started seeds are hidden. Changed or unavailable originals get an unknown-start
notice and a Shape recovery step, not a waiting claim. Without `--sdlc-root`, or
if the runs cannot be read, say `Can't tell which seeds have started; none are hidden.`
and how to check. Opening reads only that checkout's status; it discovers no checkout.
Help retains the bounded reasons. Unknown start classification never conceals a
verified seed or justifies a complete empty-waiting claim.

## Verification

Shared-boundary tests prove verified-only selection, exact bytes, canonical intake
quoting and refusal on changed sources. View tests cover copying, help isolation,
scrolling and abandoned reads. [PTY journeys](../../tests/approve_terminal.py)
use the actual Rust ledger and temporary TEST roots at 80×24 and 100×30,
with and without colour. They cover ordering, excluded drafts/stale seeds, the
combined command, empty/incomplete/unreadable roots, matched/edited/moved seeds,
no checkout, unreadable runs, different-repository refusal and unchanged source trees.
Rust view tests also cover matching and canonicalized checkout roots, older, missing
and unreadable records, outside-repository origins and unresolved checkout paths.
Reversion probes remove the mismatch refusal and block unknown-origin copying;
both must fail the corresponding public-interface regressions. The
[real handoff journey](../../tests/shape_sdlc_handoff.py) proves the move to Watch.
These prove mechanics, not operator acceptance or real-work benefit.
