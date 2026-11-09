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

## From a rejected local result

A confirmed retry seed records its isolated checkout and the rejected commit. Approve
shows both. The named checkout must belong to the same Git repository, and the retry
checkout must still be detached at that exact commit. The copied command checks again
before creating the run there. A run name derived from the retry session keeps an
unchanged seed title from colliding with the earlier run. When Approve reopens from the
original project, it reads the matching retry intake record in the detached checkout
as data. A verified seed already bound there leaves the waiting list. Partial or
conflicting records keep the seed readable and disable copy and start until inspected.

The worker start checks the same pin on F2 and before stage 3. Before preparing the worker,
it checks that the build starts at that commit or its direct child containing only the
new run’s evidence. Before preparation and initial launch, including a later launch of
a prepared step, the working files must still match the candidate. Run records and
SDLC's generated rules block may change. Watch and Taste open on the resulting build checkout from the worker
screen. After restarting Tinkery, use `--sdlc-root` with the displayed retry checkout to
find the new run.

## Start a worker

`b` opens a review of the selected seed and canonical checkout. It needs a verified
seed, a usable checkout and a successful start-status read; uncertainty offers no
start shortcut. The review reads nothing and runs nothing. It shows the exact goal,
provider/model/thinking pin, response cap, checkout, provider data and effects before
F2. Read the whole disclosure first. `--worker-model provider/model:thinking` and
`--worker-responses N` supply a separate allowance for this launch; its default is
zero and drafting calls never supply it. One response may already be in flight at
the cap. Configuration is not permission to start on navigation.

With `--operator agent`, F2, `l` and `u` share one relayed-authorization door requiring
`--worker-authorization FILE`. The operator
supplies their explicit start authorization through the relay. The agent must not create
consent or infer a budget from the words. The file is a UTF-8 JSON object, at most 16 KiB,
with only these fields:

```json
{"words":"The operator's verbatim start authorization", "seed_sha256":"64 lowercase hex characters", "model":"provider/model:thinking", "responses":7}
```

The seed SHA-256, model pin and response cap must exactly match the reviewed admission.
Missing, unreadable, malformed or mismatched authorization refuses before run creation
for F2 and before any SDLC mutation for `l` and `u`. An unreadable run seed also refuses.
The review shows the verbatim words, the declared agent operator and authority
`operator, relayed`. Provenance cannot be verified. The file is a caller-supplied
claim, not verified operator identity or proof that an agent did not invent the words.
Tinkery never generates the file. It reads it at launch and binds that snapshot when
opening the review; replacing the file cannot change the displayed authorization.
Every admission appends that snapshot, the agent declaration, relayed authority, unverified
provenance, exact run/step/checkout and `action` (`start`, `launch` or `resume`) as one JSON
object to `runs/<slug>/worker-authorization.jsonl` in the build checkout. F2 records before
preparing; `l` and `u` record before their command. Records are never overwritten.
A recording failure refuses admission. This is Tinkery's disclosure, not an SDLC approval
or a second workflow ledger.
The existing exact-seed prior-start check prevents another start of the same seed.
The relay requires the seed's checkout; it adds no alternate worktree support.
Human and undeclared F2, `l` and `u` keep their existing direct consent and require no relay file.
Goal confirmation, seed confirmation and worker start remain separate yeses.
Navigation and copying never start a worker.

With a configured worker allowance, the existing Approve reader selects up to five
recent Pi launches by the start time in each log filename. Only runs in the named
checkout and their matching Git worktrees are considered. Empty run folders add
no samples. It reads log contents only in the checkout each run reports. A log must match that run's step contract,
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
Only a refusal before run creation is attempted (such as uncommitted source) offers F2
again after the operator fixes the cause; a run created meanwhile makes that retry
refuse rather than start twice (#738). Once creation is attempted, a failed command or
identity read cannot establish that nothing was saved. The transaction names the attempted
run and checkout, says creation may have happened, and directs the operator to read it
in Watch with that `--sdlc-root`. It never invents a stable identity or offers a blind F2 retry
(#780). Interrupted admissions never offer F2 again.

The menu retains this finite request across navigation. Exiting during a request
waits for it to settle, rather than interrupting admission between records. SDLC
owns the detached worker, which can outlive Tinkery. Home's `b` reopens the current
worker screen; Watch's `b` opens the selected existing run, resolving a build
checkout only from SDLC's explicit report and matching run ID.

On that screen, `r` reads status, `l` launches an inspected prepared step, `u` resumes
an ended turn, `f` records a normally
ended worker, `v` verifies, `s` stops it, and `c` records a cap stop. These are separate
explicit actions. Verify before finishing: `f` refuses without current passing
verification, so a failed check leaves the ended worker resumable after a successful reread.
An `awaiting-attestation` reading says `waiting on your judgment`, names the pending
IDs and keeps Finish refused. Taste can inspect its unaccepted candidate; only a
person outside Tinkery can record those judgments with tink-sdlc. Worker presentation
consumes typed verification rather than recognizing status words itself.
A refused request retains the last successful worker reading and tink-sdlc's own blocker,
including unmet human checks and their instructions. That reading is explicitly historical:
current worker and verification state are unknown, and only `r` and `s` can request anything
until a read succeeds. Stop stays available because it is the conservative action; it still
confirms the run first, so tink-sdlc status must answer for it to act. Hidden action keys obey the same admission rule as the footer. A failed reread
keeps the others disabled; neither failure nor rereading automatically launches or resumes work.
Resume discloses and rechecks the original model, session and cap;
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
with temporary TEST storage and fake providers.
The failed-Verify journeys retain ended
state and human-check instructions, press stale action keys after Verify and reread failures,
and count launch/resume calls at 100×30 and 80×24 with NO_COLOR. Run
`python3 tests/worker_verify_retention_terminal.py --revert` for assertion-specific red/green
proofs with automatic source and binary restoration. The same centralized admission guard
protects mutation and provider-capable resume, so both regressions revert that guard.
The [partial-start journeys](../../tests/worker_partial_start_terminal.py)
cover persisted creation followed by command, status and identity failures, read-only Watch
recovery, blocked blind retries and the unchanged pre-creation F2 retry.
The relay journeys run standalone with `python3 tests/worker_relay_authorization_terminal.py`
and through [Python discovery](../../scripts/python-tests.py).
They cover absent/mismatched authorization, verbatim disclosure and attribution,
separate consent, unchanged human/undeclared F2 and real SDLC persistence with a fake adapter.
Run `python3 tests/worker_relay_authorization_terminal.py --reversion-probe` to remove only
the admission guard temporarily, require failure at the unauthorized-admission assertion,
then restore and rebuild. These checks make no provider requests and authenticate nobody.
Architecture diagnostics 2, 3, 6 and 7 apply: the screen requests typed actions,
the worker authorization boundary owns relay parsing/binding/recording, and SDLC owns workflow records.
