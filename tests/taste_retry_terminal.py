"""Continue a rejected, verified local build through Taste and Shape. All source is synthetic."""
import base64
import json
import os
from pathlib import Path
import re
import sys
import tempfile
import time

import session_fixture as ledger
import test_launch as subprocess
from shape_terminal import DECIDED, make
from taste_label_terminal import ESC, F2, F10, checkout, sdlc
from taste_terminal import REPO, tree
from terminal_process import TerminalProcess, wait_output
from worker_terminal import command_seed

GOAL = 'Keep the original goal.\nRead the sample as written.'
REJECTION = '  Ignores my sample — keep the "raw" lines.  '
RUN = 'test-label'


def checked_result(base, env, sessions):
    """Real seed intake and verification, with the rejected change only on the build branch."""
    root, git = checkout(base, env)
    git('config', 'user.name', 'TEST operator')
    git('config', 'user.email', 'test@example.invalid')
    (root / 'TEST-check.py').write_text("print('TEST checked candidate')\n")
    (root / '_system/verification.json').write_text(json.dumps(dict(
        require_tink=False, checks=[dict(argv=['python3', 'TEST-check.py'], timeout_seconds=10)])))
    git('add', 'TEST-check.py', '_system/verification.json')
    git('commit', '-qm', 'TEST setup')
    session = make(sessions, 'TEST original', goal=GOAL, question=False)
    command_seed(session)
    sdlc(root, env, 'stage', RUN, '1', '--here', '--seed-contract', session / 'seed-contract.md',
         '--tinkery', REPO / 'target/debug/tinkery')
    build = base / 'TEST-build'
    sdlc(root, env, 'stage', RUN, '3', '--worktree', str(build))
    (build / 'example.txt').write_text('Only the rejected build has this file.\n')
    git('-C', str(build), 'add', 'example.txt')
    git('-C', str(build), 'commit', '-qm', 'TEST candidate')
    candidate = git('-C', str(build), 'rev-parse', 'HEAD').stdout.decode().strip()
    sdlc(build, env, 'verify', RUN)
    # Evidence alone can advance the branch after verification. Retrying must use its checked hash.
    git('-C', str(build), 'add', 'runs')
    git('-C', str(build), 'commit', '-qm', 'TEST later evidence')
    return root, build, session, candidate, git


def journey(width, height, forwarded, legacy=False):
    with tempfile.TemporaryDirectory(prefix='tinkery-TEST-taste-retry-') as directory:
        base = Path(directory).resolve()
        tools = base / 'bin'
        tools.mkdir()
        (tools / 'python3').symlink_to(sys.executable)
        forbidden = base / 'unexpected-launch'
        for name in ('pi', 'gh', 'claude'):
            tool = tools / name
            tool.write_text(f'#!{sys.executable}\nfrom pathlib import Path\n'
                            f'Path({str(forbidden)!r}).touch()\nraise SystemExit(99)\n')
            tool.chmod(0o755)
        env = subprocess.environment({**os.environ, 'TERM': 'xterm-256color',
            'PATH': f'{tools}:/usr/bin:/bin', 'TINKERY_NODE': '/tinkery-TEST-no-node',
            'GIT_CONFIG_NOSYSTEM': '1'}, home=base / 'TEST-home')
        if width == 80:
            env['NO_COLOR'] = '1'
        sessions = base / 'TEST-sessions'
        root, build, original, candidate, git = checked_result(base, env, sessions)
        if legacy:
            sdlc(build, env, 'label', RUN, '--kind', 'rejected', '--words', REJECTION,
                 '--source', 'taste', '--reviewer', 'TEST operator')
        before = tree(original)
        refs = git('show-ref', '--heads').stdout
        worktrees = git('worktree', 'list', '--porcelain').stdout

        terminal = TerminalProcess([str(REPO / 'target/debug/tinkery'), '--operator', 'human', '--home',
                '--sdlc-root', str(root if forwarded else build), '--shape-budget', '0',
                '--seed-session-root', str(sessions)], cwd=root, env=env, width=width, height=height)

        def wait(text):
            def consume(chunk):
                terminal.raw.extend(chunk)
                terminal.screen.feed(chunk)

            for code in wait_output(terminal.master, terminal.process, consume,
                                    time.monotonic() + 12):
                if text in ' '.join(terminal.screen.text().split()) and terminal.screen.ready():
                    return
                assert code is None, terminal.screen.text()
            raise AssertionError(f'Missing {text!r}\n{terminal.screen.text()}')

        def send(keys, text):
            os.write(terminal.master, keys)
            wait(text)

        def open_review():
            send(b'6', 'Press r to read results')
            send(b'r', 'Keep the original goal.')
            send(b'\r', 'e details')

        try:
            wait('1 think')
            open_review()
            if not legacy:
                send(b'w', 'Signed as TEST operator.')
                send(b'2', '› 2 fail')
                send(REJECTION.encode(), '"raw" lines.')
                send(F2, 'Saved your label.')
                wait('Shape the next attempt')
                # Leave Taste, then reread so a transient form cannot supply the retry.
                send(F10, '1 think')
                open_review()
            label, = (json.loads(line) for line in (build / f'runs/{RUN}/labels.jsonl').read_text().splitlines())
            assert label['kind'] == 'rejected' and label['words'] == REJECTION, label
            send(b's', 'Confirmed goal')
            if legacy:
                wait('Earlier rejection did not record a commit.')
                wait('Current checked commit')
            for text in (GOAL.splitlines()[0], GOAL.splitlines()[1], REJECTION.strip(), candidate, DECIDED):
                wait(text)
            assert git('worktree', 'list', '--porcelain').stdout == worktrees
            assert git('show-ref', '--heads').stdout == refs
            assert set(sessions.iterdir()) == {original}
            send(ESC, 'e details')
            send(b's', 'Confirmed goal')
            send(F2, 'F1 more')
            retry, = set(sessions.glob('session-*')) - {original}
            state = ledger.load(retry)
            assert state['goal'] == GOAL and not (retry / 'seed-confirmation.json').exists()
            origin = next(node for node in state['nodes'] if node['id'] == state['origin'])
            assert any(item['checked'] == 'Taste rejection' and item['observed'] == REJECTION
                       for item in origin['evidence'])
            assert DECIDED in str(state['assumed'])
            assert tree(original) == before and git('show-ref', '--heads').stdout == refs
            copies = [line.removeprefix('worktree ') for line in
                      git('worktree', 'list', '--porcelain').stdout.decode().splitlines()
                      if line.startswith('worktree ')]
            detached, = set(map(Path, copies)) - {root, build}
            assert git('-C', str(detached), 'rev-parse', 'HEAD').stdout.decode().strip() == candidate
            assert (detached / 'example.txt').read_text() == (build / 'example.txt').read_text()
            assert not forbidden.exists() and not (build / f'runs/{RUN}/steps').exists()
            if forwarded and not legacy:
                # Confirm a synthetic runnable seed through the same ledger writer as other handoff
                # fixtures. Navigate normally and execute the command actually copied by Approve.
                send(ESC, '1 think')
                command_seed(retry)
                send(b'4', 'Seed 1 of 1')
                send(b'c', 'Copied.')
                (encoded,) = re.findall(rb'\x1b\]52;[^;]*;([^\x07\x1b]*)(?:\x07|\x1b\\)', terminal.raw)
                intake = base64.b64decode(encoded).decode()
                assert str(retry / 'seed-contract.md') in intake and str(detached) in intake
                before_runs = set((detached / 'runs').iterdir())
                subprocess.run(['sh', '-ec', intake], cwd=root, env=env, check=True, capture_output=True)
                new_run, = (p for p in set((detached / 'runs').iterdir()) - before_runs if p.is_dir())
                assert (new_run / 'seed-contract.md').read_bytes() == (retry / 'seed-contract.md').read_bytes()
                new_build = base / 'TEST-second-build'
                sdlc(detached, env, 'stage', new_run.name, '3', '--worktree', str(new_build))
                assert git('-C', str(new_build), 'rev-parse', 'HEAD^').stdout.decode().strip() == candidate
                assert (new_build / 'example.txt').read_bytes() == (build / 'example.txt').read_bytes()
                assert git('show-ref', '--heads', 'main', RUN).stdout == refs
                assert not (new_build / 'runs' / new_run.name / 'steps').exists()
            assert not forbidden.exists()
            print(f'PASS Taste retry {width}×{height}, forwarded={forwarded}, legacy={legacy}, zero provider calls')
        finally:
            terminal.close()


if __name__ == '__main__':
    journey(100, 30, forwarded=True)
    journey(80, 24, forwarded=False)
    journey(100, 30, forwarded=False, legacy=True)
