"""Read one bound, saved Codex observation through Watch; no providers or operator data."""
import copy
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import re
import stat
import sys
import tempfile
import time

import test_launch as subprocess
from terminal_process import TerminalProcess, wait_output
from watch_terminal import DETAIL, FAKE, REPO, envelope, run, tree

RUN = '00000000-0000-4000-8000-000000000729'
OTHER = '00000000-0000-4000-8000-000000000730'
CHAT = '00000000-0000-7000-8000-000000000729'


def journey(width, height, no_color):
    with tempfile.TemporaryDirectory(prefix='tinkery-TEST-watch-codex-') as directory:
        root = Path(directory)
        checkout = root / 'TEST-checkout'
        scripts = checkout / '_system/scripts'
        scripts.mkdir(parents=True)
        (scripts / 'sdlc.py').write_text(FAKE)
        state_path, log, pid = (root / name for name in ('state.json', 'calls', 'pid'))
        report_path = root / 'TEST Codex:report.json'
        row = {**run('TEST-codex', 'not-run'), 'run_id': RUN}
        other = {**run('TEST-other', 'not-run'), 'run_id': OTHER}
        detail = copy.deepcopy(DETAIL)
        detail['run'].update(steps=[], closure=None)
        state = {'list': envelope(runs=[row, other]), 'detail': detail}
        state_path.write_text(json.dumps(state))
        report = {'schema_version': 1, 'run_id': RUN, 'thread_id': CHAT,
                  'observed_at': datetime.now(timezone.utc).isoformat(),
                  'status': {'type': 'active', 'activeFlags': []},
                  'next_step': 'TEST build the report reader'}
        report_path.write_text(json.dumps(report))
        tools = root / 'bin'
        tools.mkdir()
        (tools / 'python3').symlink_to(sys.executable)
        env = subprocess.environment({**os.environ, 'PATH': f'{tools}:/usr/bin:/bin',
            'TERM': 'xterm-256color', 'TINKERY_NODE': '/tinkery-TEST-no-node',
            'TEST_WATCH_LOG': str(log), 'TEST_WATCH_PID': str(pid),
            'TEST_WATCH_STATE': str(state_path)}, home=root / 'HOME')
        env.pop('NO_COLOR', None)
        if no_color:
            env['NO_COLOR'] = '1'
        terminal = TerminalProcess([str(REPO / 'target/debug/tinkery'), '--home',
            '--operator', 'agent', '--shape-budget', '0', '--sdlc-root', str(checkout),
            '--codex-report', f'{RUN}:{CHAT}:{report_path}'], cwd=root, env=env,
            width=width, height=height)
        before = tree(checkout)
        cases = 0

        def wait(predicate, after=0):
            for returncode in wait_output(terminal.master, terminal.process, terminal.screen.feed,
                                          time.monotonic() + 10):
                if predicate(terminal.screen.text()) and terminal.screen.ready(after=after):
                    return
                assert returncode is None, terminal.screen.text()
            raise AssertionError(terminal.screen.text())

        def send(keys, expected):
            frame = terminal.screen.frames
            os.write(terminal.master, keys)
            wait(lambda screen: expected in screen, after=frame)

        def shown(expected, absent=()):
            nonlocal cases
            wait(lambda screen: expected in screen)
            screen = terminal.screen.text()
            assert all(word not in screen for word in absent), (expected, screen)
            assert tree(checkout) == before, 'Watch changed the source checkout'
            cases += 1

        def refresh(value, expected, absent=()):
            report_path.write_text(json.dumps(value))
            saved = report_path.read_bytes()
            reread(expected)
            shown(expected, absent)
            assert report_path.read_bytes() == saved, 'Watch changed the Codex report'

        def reread(expected):
            calls = len(log.read_text().splitlines())
            frame = terminal.screen.frames
            os.write(terminal.master, b'r')
            wait(lambda screen: len(log.read_text().splitlines()) >= calls + 3
                 and 'Reading' not in screen and expected in screen, after=frame)

        try:
            wait(lambda screen: '1  think' in screen)
            send(b'5', 'Codex last reported: working')
            shown('TEST build the report reader', ('no worker running',))
            send(b'\x1b[B', 'TEST-other')
            send(b'\r', 'TEST next for TEST-other')
            shown('TEST next for TEST-other', ('Codex last reported', 'TEST build the report reader'))
            send(b'\x1b', 'TEST-codex')
            refresh({**report, 'next_step': 'TEST review the terminal result'},
                    'TEST review the terminal result', ('TEST build the report reader',))
            for flag, words in [('waitingOnApproval', 'waiting for approval'),
                                ('waitingOnUserInput', 'waiting for input')]:
                refresh({**report, 'status': {'type': 'active', 'activeFlags': [flag]}},
                        words, ('Nothing needs you right now.',))
            refresh({**report, 'status': {'type': 'active',
                     'activeFlags': ['waitingOnApproval', 'waitingOnUserInput']}},
                    'waiting for approval and input')
            refresh({**report, 'next_step': 'TEST line\nSDLC next: TEST injected\t\u202e'},
                    r'Codex suggests: TEST line\nSDLC next: TEST injected\t')
            refresh({**report, 'status': {'type': 'idle'}}, 'idle (not proof of completion)')
            for status in ('notLoaded', 'systemError'):
                refresh({**report, 'status': {'type': status}}, 'activity unknown',
                        ('Codex last reported: working',))
            refresh({**report, 'observed_at': '2000-01-01T00:00:00Z'},
                    '2000-01-01T00:00:00Z')
            reread('2000-01-01T00:00:00Z')
            shown('2000-01-01T00:00:00Z')
            age = re.search(r'\b(\d+)\s+min ago\b', terminal.screen.text())
            assert age and int(age[1]) > 0, terminal.screen.text()
            for changed in ({'run_id': OTHER}, {'thread_id': OTHER},
                            {'observed_at': '2999-01-01T00:00:00Z'},
                            {'observed_at': 'not a time'}, {'schema_version': 2},
                            {'status': {'type': 'active', 'activeFlags': ['invented']}},
                            {'status': {'type': 'active'}}, {'next_step': 'x' * 513},
                            {'status': {'type': 'invented'}}, {'next_step': 'x' * 9000}):
                refresh({**report, **changed}, 'Codex report unavailable',
                        ('TEST build the report reader', 'Codex last reported: working'))
            for kind in ('missing', 'malformed', 'fifo'):
                report_path.unlink()
                if kind == 'malformed':
                    report_path.write_text('{')
                elif kind == 'fifo':
                    os.mkfifo(report_path)
                reread('Codex report unavailable')
                shown('Codex report unavailable', ('TEST build the report reader',))
                if kind == 'missing':
                    assert not report_path.exists(), 'Watch created the missing report'
                elif kind == 'malformed':
                    assert report_path.read_text() == '{', 'Watch rewrote the malformed report'
                else:
                    assert stat.S_ISFIFO(report_path.stat().st_mode), 'Watch replaced the FIFO'
                if report_path.exists():
                    report_path.unlink()
                report_path.write_text(json.dumps(report))
            refresh(report, 'TEST build the report reader')
            row.update(verification_status='blocked', next_action='TEST operator must approve')
            state_path.write_text(json.dumps(state))
            reread('TEST operator must approve')
            shown('TEST operator must approve')
            row.update(verification_status='failed', next_action='TEST inspect failed verification')
            state_path.write_text(json.dumps(state))
            reread('TEST inspect failed verification')
            shown('TEST inspect failed verification')
            row.update(verification_status='current', next_action='TEST closed run')
            detail['run']['closure'] = {'outcome': 'merged', 'merge': 'a' * 40, 'time_ns': 1}
            state_path.write_text(json.dumps(state))
            report_path.write_text('{')
            reread('TEST-codex')
            send(b'\r', 'TEST closed run')
            shown('TEST closed run', ('Codex last reported', 'TEST build the report reader',
                                      'Codex report unavailable'))
            send(b'\x1b', 'TEST-codex')
            detail['run']['closure'] = {}
            state_path.write_text(json.dumps(state))
            report_path.write_text(json.dumps(report))
            reread('TEST-codex')
            send(b'\r', 'TEST closed run')
            shown('TEST closed run', ('Codex last reported', 'Codex report unavailable'))
            send(b'\x1b', 'TEST-codex')
            row.update(run_id='00000000-0000-4000-8000-000000000731',
                       verification_status='not-run')
            detail['run']['closure'] = None
            state_path.write_text(json.dumps(state))
            report_path.write_text(json.dumps(report))
            reread('TEST-codex')
            send(b'\r', 'TEST closed run')
            shown('TEST closed run', ('Codex last reported', 'TEST build the report reader'))
            assert all(line.startswith('status ') for line in log.read_text().splitlines())
            print(f'Watch Codex: {cases} cases at {width}x{height}, NO_COLOR={no_color}; '
                  'saved observations only, source bytes unchanged, no provider calls')
        finally:
            terminal.close()


if __name__ == '__main__':
    journey(100, 30, False)
    journey(80, 24, True)
