"""Taste keeps its human-verdict reminder after notes, corrections and unbound verdicts (#714, #781).

Synthetic API 1 records exercise the public reader, including unavailable verdict counts.
Every app launch uses an isolated TEST home and an agent role; no providers or labels run.
"""
import json
import os
from pathlib import Path
import sys
import tempfile
import time

import test_launch as subprocess
from terminal_process import TerminalProcess, wait_output

REPO = Path(__file__).resolve().parents[1]
REMINDER = 'awaiting human verdict'
READER = '''import json, sys
from pathlib import Path
state = json.loads(Path(__file__).parents[2].joinpath('TEST-state.json').read_text())
assert sys.argv[1:] in (['status', '--json'], ['status', 'TEST-result', '--json']), sys.argv
print(json.dumps(state['list'] if len(sys.argv) == 3 else state['detail']))
'''


def cases():
    for kind in (None, 'none', 'correction', 'accepted', 'rejected'):
        counts = {name: int(name == kind) for name in ('none', 'correction', 'accepted', 'rejected')}
        yield kind or 'empty', {'count': sum(counts.values()), 'kinds': counts}, True
    yield 'labels unavailable', None, False
    yield 'older count-only report', {'count': 0}, False
    for kind in ('accepted', 'rejected'):
        other = 'rejected' if kind == 'accepted' else 'accepted'
        yield f'{kind} absent', {'count': 0, 'kinds': {other: 0}}, False
        for invalid in (None, -1, '0', False, 0.5):
            yield f'{kind} invalid {invalid!r}', {'count': 0, 'kinds': {kind: invalid, other: 0}}, False


def journey(width, height, no_color):
    with tempfile.TemporaryDirectory(prefix='tinkery-TEST-taste-verdict-') as directory:
        base = Path(directory).resolve()
        scripts = base / 'TEST-checkout/_system/scripts'
        scripts.mkdir(parents=True)
        (scripts / 'sdlc.py').write_text(READER)
        state = json.loads((REPO / 'tests/fixtures/taste-api1.json').read_text())
        state['detail']['run']['pr'] = None
        tools = base / 'bin'
        tools.mkdir()
        forbidden = base / 'TEST-unexpected-gh'
        (tools / 'gh').write_text(f'#!{sys.executable}\nfrom pathlib import Path\n'
                                 f'Path({str(forbidden)!r}).touch()\nraise SystemExit(1)\n')
        (tools / 'gh').chmod(0o755)
        env = subprocess.environment({**os.environ, 'PATH': f'{tools}:{os.environ["PATH"]}',
                                      'TERM': 'xterm-256color'}, home=base / 'TEST-home')
        env.pop('NO_COLOR', None)
        if no_color:
            env['NO_COLOR'] = '1'
        failures = []
        for name, labels, expected in cases():
            state['detail']['run']['labels'] = labels
            statefile = base / 'TEST-checkout/TEST-state.json'
            statefile.write_text(json.dumps(state))
            records = base / 'TEST-checkout/runs/TEST-result/labels.jsonl'
            records.parent.mkdir(parents=True, exist_ok=True)
            records.write_text(''.join(json.dumps(dict(kind=kind, source='taste', words='TEST legacy words')) + '\n'
                                       for kind, count in (labels or {}).get('kinds', {}).items()
                                       if count == 1 and kind in ('accepted', 'rejected')))
            before = statefile.read_bytes()
            terminal = TerminalProcess(
                [str(REPO / 'target/debug/tinkery'), '--operator', 'agent', '--home',
                 '--shape-budget', '0', '--sdlc-root', str(base / 'TEST-checkout'),
                 '--seed-session-root', str(base / 'TEST-sessions')],
                cwd=base, env=env, width=width, height=height)

            def wait(text):
                for returncode in wait_output(terminal.master, terminal.process, terminal.screen.feed,
                                              time.monotonic() + 10):
                    if text in terminal.screen.text() and terminal.screen.ready():
                        return
                    assert returncode is None, terminal.screen.text()
                raise AssertionError(f'{name}: {text!r} not shown\n{terminal.screen.text()}')

            try:
                wait('1  think')
                os.write(terminal.master, b'6')
                wait('Press r to read results')
                os.write(terminal.master, b'r')
                wait('No confirmed seed')
                screen = terminal.screen.text()
                observed = REMINDER in screen
                if observed != expected:
                    failures.append(f'{name}: expected reminder {expected}, saw {observed}\n{screen}')
                assert 'Saved your label.' not in screen, screen
                assert not forbidden.exists(), 'Reading an unrecorded result called GitHub.'
                assert statefile.read_bytes() == before, 'Taste changed source records.'
                print(json.dumps(dict(case=name, width=width, height=height, no_color=no_color,
                                      expected_reminder=expected, observed_reminder=observed,
                                      provider_calls=0, label_writes=0)))
            finally:
                terminal.close()
        assert not failures, '\n'.join(failures)


if __name__ == '__main__':
    journey(100, 30, False)
    journey(80, 24, True)
