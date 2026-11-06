# Approve

Approve reads a shaped goal and offers a manual handoff or a worker-start review.
Reading, navigation and copying create no run and start no agent. Only the separate
worker screen's F2 authorizes implementation; seed confirmation and definition
approval remain separate.

## Source and verification

The menu supplies its existing session discovery result, without Tend's repository
filter. Approve reads across that configured session root, oldest update first.
The menu also supplies its Think record root. Once per visit, Approve's worker reads
origins through `think_record::read_origins`, using the existing live identity and
exact-goal checks. Each visit resolves the named checkout once. Approve uses that path
to find its enclosing repository, read run status and prepare the copied command.
Reopening Approve reads the current destination of a checkout symlink. Repository
comparison uses `Repo::enclosing` and never reads `.git` contents.
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
The acceptance section is shown without the Markdown escapes Tinkery writes into it, so
checks read as written; the stored bytes keep them.

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
Copying runs nothing and grants no approval. Missing checkout, a checkout without
`_system/scripts/sdlc.py` (#716), an empty slug or unsafe paths leave the verified seed readable with a reason and no copy action.
A known different repository does the same: a plain reason names both repository
folders and asks the operator to reopen with `--sdlc-root` naming the seed's origin.
If the folder names look the same, the message shows both full paths. It offers
no command, and `c` does nothing. Only root paths establish identity. Matching
origins keep the existing copy action.
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
| `b` | Review the selected worker start; execute nothing. |
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

## Start a worker

`b` opens a review of the selected seed and canonical checkout. It needs a verified
seed, a usable checkout and a successful start-status read; uncertainty offers no
start shortcut. The review reads nothing and runs nothing. It shows the exact goal,
provider/model/thinking pin, response cap, checkout, provider data and effects before
F2. Read the whole disclosure first. `--worker-model provider/model:thinking` and
`--worker-responses N` supply a separate allowance for this launch; its default is
zero and drafting calls never supply it. One response may already be in flight at
the cap. Configuration is not permission to start on navigation.

With a configured worker allowance, the existing Approve reader looks at up to five
fresh Pi launch logs from the three most recently updated run folders. It reads
only the checkout each run reports. A log must match that run's step contract,
session and checkout. The review names the source step and counts Pi responses up
to the first successful `edit` or `write` tool result. Shell commands can also change
files, so this measures only those two tools. The suggested minimum leaves one
response after the latest observed first edit. If the launch itself reached its
cap with no recorded edit, allow one response to edit and one to follow up.
The strongest of these observations sets the suggestion. Smaller caps show a
warning and F2 still uses the exact configured cap. Missing usable evidence leaves
the minimum unknown. Resumes and Pi's private session store are outside this read;
the reader keeps no logs and sends none to a provider.

F2 requests one bounded transaction through the installed SDLC runtime: recheck the
selected seed and origin, require committed project source, create the run, import
the seed, open stage 3 in its own
worktree, prepare a pinned Pi build step and launch it. The existing SDLC commands
own definition gates, worktree creation, model resolution, launcher hashing,
session identity and cumulative cap. No copied shell text is executed and no
approval or worker report is invented. A refusal stops the sequence; there is no
automatic retry. Partial progress retains the source-owned run for inspection.
A refusal before any run is known (such as uncommitted source) started nothing, so F2 is
offered again after the operator fixes the cause; a run created meanwhile makes that retry
refuse rather than start twice (#738). Interrupted admissions never offer F2 again.

The menu retains this finite request across navigation. Exiting during a request
waits for it to settle, rather than interrupting admission between records. SDLC
owns the detached worker, which can outlive Tinkery. Home's `b` reopens the current
worker screen; Watch's `b` opens the selected existing run, resolving a build
checkout only from SDLC's explicit report and matching run ID.

On that screen, `r` reads status, `l` launches an inspected prepared step, `u` resumes
an ended turn, `f` records a normally
ended worker, `v` verifies, `s` stops it, and `c` records a cap stop. These are separate
explicit actions. Verify before finishing: `f` refuses without current passing
verification, so a failed check leaves the ended worker resumable. Resume discloses and rechecks the original model, session and cap;
it cannot silently use changed settings or add allowance. Unknown admission never
becomes a successful launch or an automatic retry. The preflight identity read is
not an atomic lock against concurrent external record edits; SDLC rechecks its own
contracts when executing each action.

`w` opens Watch and `t` opens Taste; neither navigation starts a preview. Completion,
current verification and human acceptance remain separate. The shared `worker`
module owns transactions; the menu owns their screen and lifetime. No second
workflow ledger is created. Dirty project source refuses before intake; ignored
dependencies and `runs/` evidence do not require a source commit.

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

[Worker terminal journeys](../../tests/worker_terminal.py) cover explicit admission,
separate allowance, source changes, retained requests and truthful completion states
with temporary TEST storage and fake providers. Architecture diagnostics 2, 3, 6 and
7 apply: the screen requests typed actions while SDLC owns durable work records.
