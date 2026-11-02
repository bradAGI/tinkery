# First task, start to finish

One supported path from a fresh install to a build worker running on your project.
Each step names what it saves, what it allows and what is running afterwards. Nothing
runs on its own: every model call and every launch is a key or command you choose.
`tests/first_task_path.py` runs the shell blocks below verbatim, with stand-in models.

Set these once in your shell:

```sh
tinkery_src=~/dev/tinkery
project=~/dev/my-project
notes=~/dev/run-notes
```

## 1. Install Tinkery

On Debian or Ubuntu (on macOS, provision Rust, Python 3 and Node first; see
[AGENTS.md](../AGENTS.md#cold-setup)):

```sh
git clone https://github.com/jon-devlapaz/tinkery "$tinkery_src"
cd "$tinkery_src" && ./scripts/cloud-setup.sh
export PATH="$HOME/.cargo/bin:$tinkery_src/.cloud-tools/node/bin:$tinkery_src/target/release:$PATH"
cargo +stable build --locked --release
```

The `export` line lasts for this shell; add it to `~/.profile` to keep it.

Check readiness instead of running the contributor test suite:

```sh
python3 "$tinkery_src/scripts/first-task-check.py"
```

It runs no model and installs nothing. `ready: Tinkery runs` is this step's result.
Saved: nothing. Allowed: nothing. Running: nothing.

## 2. Connect your project to tink-sdlc

Tinkery hands work to [tink-sdlc](https://github.com/jon-devlapaz/tink-sdlc), installed
into the Git checkout you want built. Commit the install, then choose real checks in
`_system/verification.json` (see its `SDLC.md`); until then, verification fails on purpose.

```sh
mkdir -p "$project" && git -C "$project" init -q
git clone https://github.com/jon-devlapaz/tink-sdlc ~/dev/tink-sdlc
python3 ~/dev/tink-sdlc/scripts/init.py "$project"
git -C "$project" add -A && git -C "$project" commit -qm "Install tink-sdlc"
python3 "$tinkery_src/scripts/first-task-check.py" --sdlc-root "$project"
```

`ready: The workflow is connected` is this step's result. From now on, launch Tinkery with
`--sdlc-root "$project"` so Shape reads that repository and Approve and Watch use its runs.
Saved: the tink-sdlc install, committed in your project. Allowed: nothing. Running: nothing.

## 3. Choose the drafting model

Install Pi, the coding agent that drafts, with the Node from step 1:
`npm install -g --ignore-scripts @earendil-works/pi-coding-agent`. Run `pi`, log in with its
`/login` command, and quit it. Then open Tinkery:

```sh
tinkery --home --sdlc-root "$project"
```

Until a model is chosen, Think is practice: its status says
`Simulating… (practice: no model)` and nothing leaves your machine. Press `m`, choose
Pi, a model, a thinking level and the calls each launch may spend, then `s`. Rerun the
check: `ready: Drafting is set up` means Pi has that model and a login on disk, not that
it answered. A model can be listed and still refuse your account, so the first real send in
step 4 is the proof. If it fails, choose another model with `m`.
Saved: the model setup. Allowed: that many drafting calls each launch. Running: nothing.

## 4. Think: say what you want

Choose Think, type your goal, then F2 sends it (one or two calls). Answer what it asks
and send again. When the reading is right, F1's first row opens the review: read it to
the end, type `confirm`, then Enter.
Saved: your goal. Allowed: nothing; saving is not seed confirmation or permission to build.
Running: nothing.

## 5. Shape: agree what will be built

Open the goal in Shape (`s` in Tend's goal detail, or menu place 3). F2 lets the model
read your repository and ask the next question; answer each with F2. When a seed draft
appears, read it to `End of seed` (PgDn, or Down while typing), type `confirm`, then F2.
To change the draft instead, F1 Revise the draft sends your words back for a new one.
Saved: the confirmed seed. Allowed: nothing to build yet. Running: nothing.

## 6. Approve: create the run

From the ready seed, F1 then `1` opens it in Approve. `c` copies one command; paste it in
a terminal. It creates a run in `$project` and imports the seed and its checks:

```sh
cd "$project" && python3 _system/scripts/sdlc.py new my-run && python3 _system/scripts/sdlc.py stage my-run 1 --seed-contract "$seed" --tinkery "$(command -v tinkery)"
```

Use the copied line, not this example; set `run` to its run name for the next steps
(`run=my-run`). Saved: the run and its seed copy. Allowed: nothing. Running: nothing.

## 7. Open the build stage

```sh
cd "$project" && python3 _system/scripts/sdlc.py status "$run"
python3 _system/scripts/sdlc.py stage "$run" 3 --here
```

When the checks still match the confirmed seed, status says `Stage 3: approved` with
`matches confirmed seed`; no further question is asked. Changed checks wait for your
decision, as status says. `--here` builds in this checkout; without it, stage 3 makes a
worktree under `~/dev/worktrees/`. Stage prints a prompt and starts nothing.
Saved: the stage. Allowed: a build on this run. Running: nothing.

## 8. Start the worker

This is the step that spends model calls on building. Set `worker_model` to a model that
`pi --list-models --offline` shows, with a thinking level (`worker_model=provider/model-id:low`).
Check the pin first, then prepare and launch one pinned build step:

```sh
python3 "$tinkery_src/scripts/first-task-check.py" --sdlc-root "$project" --worker-model "$worker_model"
mkdir -p "$notes"
cd "$project" && python3 _system/scripts/sdlc.py step "$run" build prepare \
  --report "$notes/$run-build.md" --model "$worker_model" \
  --launcher "$project/_system/scripts/pi-launcher"
python3 _system/scripts/sdlc.py step "$run" build launch
```

Run `launch` from a shell that stays open. A remote runner that ends everything a command
started when the command returns, such as `hc exec` or `hc run --no-parent`, stops the worker
within a second ([#650](https://github.com/jon-devlapaz/tinkery/issues/650)).
Saved: the step contract. Allowed: this one worker on this pin. Running: Pi, building.

## 9. Watch it, then finish

```sh
cd "$project" && python3 _system/scripts/sdlc.py step "$run" build show
```

`observed_turn` says whether the worker's turn ended (`ended: read the report, then
complete the step`) or not. `not ended` with no Pi process left means the worker was stopped,
not that it is still building. Watch (menu place 5) shows the run. When it ended, read the
report, then:

```sh
cd "$project" && python3 _system/scripts/sdlc.py step "$run" build complete
python3 _system/scripts/sdlc.py status "$run"
```

`complete` records the report; it proves nothing passed. `status` lists what is still yours:
checks the worker changed (`Stage 3: stale`, with the `decide` command to fill in), then each
`pending` item, recorded with `mark` and `--evidence` (plus `--reviewer` and `--source` when it
says `operator attestation required`).
Only you record these; an agent's record is never your approval. Then run the checks:

```sh
cd "$project" && python3 _system/scripts/sdlc.py verify "$run"
```

A green `verify` ends this guide. Review and merging follow tink-sdlc's stage 5
(`stages/05-deploy/CONTEXT.md` in your project). Saved: the report digest, then verification.
Running: nothing.
