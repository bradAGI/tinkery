"""Closed-run postmortems in a real terminal: a fake sdlc.py, real Git, no gh, Pi or Node."""
import json
import os
from pathlib import Path
import shutil
import sys
import tempfile
import time

import test_launch as subprocess
from measure_terminal import SIZES
from terminal_process import TerminalProcess, wait_output

REPO = Path(__file__).resolve().parents[1]
DAY = 86_400 * 10**9
FAKE = r'''import json, pathlib, sys
root = pathlib.Path(__file__).resolve().parents[2]
with open(root / 'sdlc-calls', 'a') as log:
    log.write(' '.join(sys.argv[1:]) + '\n')
fixture = json.loads((root / 'fixture.json').read_text())
body = {'runs': fixture['list']} if len(sys.argv) == 3 else {'run': fixture['runs'][sys.argv[2]]}
print(json.dumps({'protocol': 'tink-sdlc', 'api_version': 1, **body}))
'''


def git(root, *args):
    return subprocess.run(['git', '-c', 'user.name=TEST', '-c', 'user.email=test@example.invalid', *args],
                          cwd=root, check=True, capture_output=True, text=True,
                          env=subprocess.environment()).stdout.strip()


def commit(root, message):
    git(root, 'commit', '--allow-empty', '-q', '-m', message)
    return git(root, 'rev-parse', 'HEAD')


def checkout(root):
    """Two closed runs: one held with Pi steps and corrections, one reverted without steps."""
    git(root, 'init', '-q')
    kept, dropped = commit(root, 'kept change'), commit(root, 'reverted change')
    commit(root, f'Revert\n\nThis reverts commit {dropped}.')
    (root / '_system/scripts').mkdir(parents=True)
    (root / '_system/scripts/sdlc.py').write_text(FAKE)
    now = time.time_ns()
    steps = root / 'runs/TEST-held/steps'
    steps.mkdir(parents=True)
    (steps / 'build.json').write_text(json.dumps({'session': 's-build'}))
    events = [{'type': 'session', 'id': 's-build'}] + [{'type': 'message_end', 'message': {'role': 'assistant'}}] * 4
    (steps / 'build.launch-1.pi.log').write_text(''.join(json.dumps(e) + '\n' for e in events))
    labels = [{'kind': 'correction', 'words': 'TEST it edited the locked test'}, {'kind': 'accepted', 'words': 'TEST looks right'}]
    (root / 'runs/TEST-held/labels.jsonl').write_text(''.join(json.dumps(l) + '\n' for l in labels))
    (root / 'runs/TEST-reverted').mkdir()
    (root / 'runs/TEST-reverted/labels.jsonl').write_text(json.dumps({'kind': 'none', 'words': 'fine'}) + '\n')
    runs = {
        'TEST-held': {'slug': 'TEST-held', 'closure': {'outcome': 'merged', 'merge': kept, 'time_ns': now - 30 * DAY},
                      'labels': {'count': 2}, 'pr': {'url': 'https://github.com/TEST/repo/pull/1'},
                      'decisions': [{'stage': 1, 'decision': 'approved', 'reviewer': 'TEST-op'},
                                    {'stage': 2, 'decision': 'approved', 'reviewer': 'pi', 'attribution': 'agent-recorded'}],
                      'steps': [{'name': 'build'}]},
        'TEST-reverted': {'slug': 'TEST-reverted', 'closure': {'outcome': 'merged', 'merge': dropped, 'time_ns': now - DAY},
                          'labels': {'count': 1}, 'pr': None, 'decisions': [], 'steps': []},
    }
    listed = [{'slug': s, 'closure': r['closure']} for s, r in runs.items()]
    (root / 'fixture.json').write_text(json.dumps({'list': listed, 'runs': runs}))


class Terminal(TerminalProcess):
    def __init__(self, width, height, no_color):
        self.temporary = tempfile.TemporaryDirectory(prefix='tinkery-TEST-measure-runs-')
        self.root = Path(self.temporary.name).resolve()
        home, binary = self.root / 'HOME', self.root / 'bin'
        home.mkdir()
        binary.mkdir()
        checkout(self.root)
        self.calls = self.root / 'calls'
        for name in ('pi', 'node', 'gh'):
            stub = binary / name
            stub.write_text(f'#!/bin/sh\nprintf "%s\\n" "$0" >> "{self.calls}"\nexit 99\n')
            stub.chmod(0o755)
        for name in ('python3', 'git'):
            (binary / name).symlink_to(shutil.which(name))
        self.before = self.files()
        env = {'PATH': str(binary), 'TERM': 'xterm-256color', 'TINKERY_NODE': str(binary / 'node')}
        if no_color:
            env['NO_COLOR'] = '1'
        super().__init__([str(REPO / 'target/debug/tinkery'), '--operator', 'agent', '--home',
                          '--seed-session-root', str(self.root / 'TEST-sessions')],
                         cwd=self.root, env=subprocess.environment(env, home=home), width=width, height=height)
        self.wait(lambda: '1  think' in self.screen.text())

    def files(self):
        return {str(p.relative_to(self.root)): p.read_bytes() for p in self.root.rglob('*')
                if p.is_file() and '.git' not in p.relative_to(self.root).parts}

    def wait(self, predicate, seconds=10):
        deadline = time.monotonic() + seconds
        def consume(chunk):
            self.raw.extend(chunk)
            self.screen.feed(chunk)
        for returncode in wait_output(self.master, self.process, consume, deadline):
            if predicate() and not self.screen.synchronized:
                return
            if returncode is not None:
                break
        raise AssertionError(self.screen.text())

    def send(self, key, expected):
        os.write(self.master, key)
        self.wait(lambda: expected in self.screen.text())

    def sdlc_calls(self):
        path = self.root / 'sdlc-calls'
        return path.read_text().splitlines() if path.exists() else []

    def close(self):
        super().close()
        self.temporary.cleanup()


def journey(width, height, no_color):
    t = Terminal(width, height, no_color)
    try:
        t.send(b'7', 'p runs')
        t.send(b'?', 'p reads closed runs')
        t.send(b'\x1b', 'p runs')
        assert t.sdlc_calls() == [], 'opening or help read runs'
        t.send(b'p', '2 closed runs · 2 merged · 1 reverted')
        text = t.screen.text()
        rows = [' '.join(line.split()) for line in text.splitlines()]
        assert '> 0 0 ? reverted TEST-reverted' in rows, text
        assert '1 1 4 held TEST-held' in rows, text
        assert t.sdlc_calls() == ['status --json', 'status TEST-reverted --json', 'status TEST-held --json']
        t.send(b'?', 'never zero')
        t.send(b'\x1b', 'TEST-held')
        t.send(b'\x1b[B', '> 1')
        t.send(b'\r', 'Driving agent responses: not recorded')
        details = t.screen.text()
        for expected in ('Gates: 1 by you', 'agent-recorded, not counted', 'correction: TEST it edited the locked test',
                         'build: 4', 'Held: not reverted on HEAD'):
            if height >= 30:
                assert expected in details, (expected, details)
        t.send(b'\x1b', 'TEST-reverted')
        t.send(b'\x1b', 'r read GitHub')
        assert len(t.sdlc_calls()) == 3, 'navigation reread runs'
        os.write(t.master, b'q')
        t.wait(lambda: '1  think' in t.screen.text())
        os.write(t.master, b'\x1b[21~')
        t.process.wait(timeout=5)
        assert not t.calls.exists(), 'Measure launched gh, Pi or Node'
        after = t.files()
        after.pop('sdlc-calls')
        assert after == t.before, 'the postmortem wrote files'
        print(json.dumps({'journey': 'runs', 'width': width, 'height': height, 'no_color': no_color,
                          'sdlc_calls': 3, 'provider_calls': 0}))
    finally:
        t.close()


if __name__ == '__main__':
    for dimensions in SIZES:
        for no_color in (False, True):
            journey(*dimensions, no_color)
