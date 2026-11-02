#!/usr/bin/env python3
"""Say which parts of a first real task are ready, without model calls, installs or writes.

Promises: four readiness lines (Tinkery runs, the workflow is connected, drafting is set up,
a worker can launch) and one Next step for the first that is not ready; exit 0 only when all
four are. Reads local files and runs only `tinkery --snapshot` (in a throwaway HOME), `git var`
and tink-sdlc's read-only `capabilities`.
Refuses to claim more than it read: "set up" never means a model answered. The first real
send proves drafting; the first launch proves the worker.
"""
import argparse
import importlib.machinery
import importlib.util
import json
import os
import shutil
import subprocess
import tempfile
from pathlib import Path

GUIDE = 'docs/first-task.md'


def tinkery_runs(tinkery):
    if not tinkery:
        return False, f'Install Tinkery and put it on PATH ({GUIDE}, step 1).'
    with tempfile.TemporaryDirectory(prefix='tinkery-first-task-check-') as home:
        env = {**os.environ, 'HOME': home}
        try:
            result = subprocess.run([tinkery, '--snapshot', '--no-color'], env=env, capture_output=True,
                                    text=True, timeout=30)
        except (OSError, subprocess.TimeoutExpired) as error:
            return False, f'{tinkery} did not start: {error}'
    if result.returncode:
        return False, f'{tinkery} --snapshot failed: {(result.stderr or result.stdout).strip()[:200]}'
    return True, tinkery


def launcher(root):
    """tink-sdlc's bundled Pi adapter in this checkout, loaded for its offline checks only."""
    path = root / '_system/scripts/pi-launcher'
    loader = importlib.machinery.SourceFileLoader('tink_sdlc_pi_launcher', str(path))
    module = importlib.util.module_from_spec(importlib.util.spec_from_loader(loader.name, loader))
    loader.exec_module(module)
    return module


def pi_ready(pi, pin, root):
    """Pi is installed, and the pin's model is in its catalog with a stored login (offline)."""
    if not shutil.which(pi):
        return False, f'Install Pi and log in, so `{pi}` is on PATH.'
    if root is None:
        return True, None
    adapter = launcher(root)
    try:
        provider, model_id, thinking = adapter.parse_pin(pin)
        adapter.check_catalog(provider, model_id, thinking)
        adapter.check_auth(provider)
    except adapter.Refusal as refusal:
        return False, str(refusal)
    return True, None


def pi_logged_in(root):
    """Pi has a stored login for some provider (offline; credential values are never shown)."""
    adapter = launcher(root)
    try:
        store = adapter.read_json(adapter.agent_dir() / 'auth.json', "Pi's credential store")
    except adapter.Refusal:
        return False

    def stored(provider):
        try:
            adapter.check_auth(provider)
            return True
        except adapter.Refusal:
            return False
    return isinstance(store, dict) and any(stored(provider) for provider in store)


def drafting(root):
    path = Path.home() / '.config/tinkery/model.json'
    try:
        setup = json.loads(path.read_text())
    except FileNotFoundError:
        if not shutil.which('pi'):
            return False, f'Install Pi and log in, so `pi` is on PATH ({GUIDE}, step 3).'
        if root is not None and not pi_logged_in(root):
            return False, f'Log in to Pi: run `pi`, then /login ({GUIDE}, step 3).'
        return False, (f'Press m on the menu, choose a model and calls, then s to save ({GUIDE}, step 3). '
                       'An agent never uses a saved setup: the operator launches it with '
                       '--shape-pi --model MODEL --shape-budget CALLS.')
    except (OSError, ValueError) as error:
        return False, f'{path} is unreadable: {error}'
    if not setup.get('model') or not setup.get('calls'):
        return False, f'The saved setup is practice (no model or no calls). Press m on the menu ({GUIDE}, step 3).'
    if setup.get('harness') == 'claude-code':
        ok = shutil.which('claude') is not None
        return ok, (f"Claude Code {setup['model']}, {setup['calls']} calls each launch" if ok
                    else 'Install Claude Code and log in, so `claude` is on PATH.')
    ok, why = pi_ready('pi', f"{setup['model']}:{setup.get('thinking', 'low')}", root)
    return ok, (f"Pi {setup['model']}, {setup['calls']} calls each launch" if ok else why)


def git_identity(root):
    """Git can name the author and committer of step 2's commit, as `git commit` would."""
    cwd = root if root is not None and root.is_dir() else None
    for ident in ('GIT_AUTHOR_IDENT', 'GIT_COMMITTER_IDENT'):
        if subprocess.run(['git', 'var', ident], cwd=cwd, capture_output=True).returncode:
            return False
    return True


def workflow(root):
    if not git_identity(root):
        return False, ('Tell Git who you are so the install can be committed: git config --global user.name '
                       f'"Your Name" && git config --global user.email "you@example.com" ({GUIDE}, step 2).')
    if root is None:
        return False, f'Name your project with --sdlc-root after installing tink-sdlc there ({GUIDE}, step 2).'
    runtime = root / '_system/scripts/sdlc.py'
    if not runtime.is_file():
        return False, f'{root} has no tink-sdlc; install it there ({GUIDE}, step 2).'
    git = subprocess.run(['git', '-C', str(root), 'rev-parse', '--is-inside-work-tree'],
                         capture_output=True, text=True)
    if git.stdout.strip() != 'true':
        return False, f'{root} is not a Git checkout; run git init and commit the install.'
    tracked = subprocess.run(['git', '-C', str(root), 'ls-files', '_system/scripts/sdlc.py'],
                             capture_output=True, text=True)
    if not tracked.stdout.strip():
        return False, f'Commit the tink-sdlc install in {root} ({GUIDE}, step 2).'
    result = subprocess.run(['python3', str(runtime), 'capabilities'], cwd=root, capture_output=True,
                            text=True, timeout=30)
    if result.returncode:
        return False, f'tink-sdlc in {root} refused: {(result.stderr or result.stdout).strip()[:200]}'
    return True, str(root)


def worker(root, pin):
    if root is None:
        return False, 'Connect the workflow first.'
    if not pin:
        return False, f'Name the build model with --worker-model provider/model-id:thinking ({GUIDE}, step 8).'
    if not (root / '_system/scripts/pi-launcher').is_file():
        return False, f'{root} lacks the bundled Pi launcher; upgrade tink-sdlc there.'
    ok, why = pi_ready(os.environ.get('TINK_SDLC_PI', 'pi'), pin, root)
    return ok, (pin if ok else why)


def main():
    parser = argparse.ArgumentParser(description=__doc__.split('\n\n')[0])
    parser.add_argument('--tinkery', default=shutil.which('tinkery'), help='Tinkery executable (default: on PATH)')
    parser.add_argument('--sdlc-root', type=Path, help='your project, with tink-sdlc installed')
    parser.add_argument('--worker-model', help='build pin: provider/model-id:thinking')
    args = parser.parse_args()
    root = args.sdlc_root.resolve() if args.sdlc_root else None
    connected = workflow(root)
    lines = [('Tinkery runs', tinkery_runs(args.tinkery)),
             ('The workflow is connected', connected),
             ('Drafting is set up; the first real send proves it', drafting(root if connected[0] else None)),
             ('A worker can launch; the first launch proves it', worker(root if connected[0] else None,
                                                                       args.worker_model))]
    for label, (ok, detail) in lines:
        print(f"{'ready' if ok else 'not yet'}: {label}" + (f'. {detail}' if ok and detail else ''))
    waiting = [detail for _, (ok, detail) in lines if not ok]
    print(f'Next: {waiting[0]}' if waiting else f'Next: start your first task ({GUIDE}, step 4).')
    return 1 if waiting else 0


if __name__ == '__main__':
    raise SystemExit(main())
