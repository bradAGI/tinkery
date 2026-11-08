"""Watch and Taste read an older checklist through real SDLC status and mark (#782).

Synthetic approval and sign-off evidence in TEST roots, fake GitHub, no providers.
"""
import json
import os
from pathlib import Path
import sys
import tempfile
import time

import test_launch as subprocess
from taste_label_terminal import checkout, sdlc, F10
from terminal_process import TerminalProcess, wait_output

REPO = Path(__file__).resolve().parents[1]
URL = 'https://github.com/TEST/attestation/pull/1'
CRITERION = 'TEST inspect the amber border'


def journey(width, height, mono, resolution):
    with tempfile.TemporaryDirectory(prefix='tinkery-TEST-attestation-owed-') as directory:
        base = Path(directory).resolve()
        tools = base / 'bin'
        tools.mkdir()
        (tools / 'gh').write_text(f'''#!{sys.executable}
import json, sys
if sys.argv[1:3] == ['pr', 'view']:
    print(json.dumps(dict(url={URL!r}, title='TEST older result', state='OPEN',
                         headRefOid={'a' * 40!r}, statusCheckRollup=[])))
elif sys.argv[1:2] == ['api']:
    print('HTTP 404', file=sys.stderr)
    raise SystemExit(1)
else:
    raise SystemExit('Unexpected TEST GitHub command: ' + repr(sys.argv))
''')
        (tools / 'gh').chmod(0o755)
        env = subprocess.environment({**os.environ, 'PATH': f'{tools}:{os.environ["PATH"]}',
                                      'TERM': 'xterm-256color', 'GIT_CONFIG_NOSYSTEM': '1',
                                      'GIT_CONFIG_GLOBAL': str(base / 'TEST-gitconfig')},
                                     home=base / 'TEST-home')
        env.pop('NO_COLOR', None)
        if mono:
            env['NO_COLOR'] = '1'
        root, _ = checkout(base, env)
        run = root / 'runs/test-label'
        (run / 'checklist.json').write_text(json.dumps(dict(schema=1, items=[
            dict(id='test-manual', description='TEST older manual check', verify=CRITERION)])))
        (run / 'pr.json').write_text(json.dumps(dict(url=URL, candidate='a' * 40)))
        sdlc(root, env, 'decide', 'test-label', '3', 'approved',
             '--reviewer', 'TEST synthetic reviewer', '--source', 'TEST fixture only',
             '--reason', 'TEST definition, not operator approval')
        status = json.loads(sdlc(root, env, 'status', 'test-label', '--json').stdout)
        [item] = status['run']['checklist']
        assert 'operator_attestation' not in item and not item['automatic'], item
        original = {str(p.relative_to(run)): p.read_bytes() for p in run.rglob('*') if p.is_file()}
        terminal = TerminalProcess(
            [str(REPO / 'target/debug/tinkery'), '--operator', 'agent', '--home',
             '--shape-budget', '0', '--sdlc-root', str(root),
             '--seed-session-root', str(base / 'TEST-sessions')],
            cwd=base, env=env, width=width, height=height)

        def wait(text):
            for returncode in wait_output(terminal.master, terminal.process, terminal.screen.feed,
                                          time.monotonic() + 10):
                if text in terminal.screen.text() and terminal.screen.ready():
                    return
                assert returncode is None, terminal.screen.text()
            raise AssertionError(f'{text!r} not shown ({width}x{height}):\n{terminal.screen.text()}')

        def send(keys, text):
            os.write(terminal.master, keys)
            wait(text)

        def shown():
            return ' '.join(terminal.screen.text().split())

        try:
            wait('1  think')
            send(b'5', '1 attestations owed')
            send(b'\r', '1 attestations owed')
            assert CRITERION in shown(), terminal.screen.text()
            send(F10, '1  think')
            send(b'6', 'Press r to read results')
            send(b'r', 'TEST older result')
            assert 'You owe 1' in shown(), terminal.screen.text()
            send(b'\r', 'What to judge')
            assert CRITERION in shown(), terminal.screen.text()
            send(b'e', 'You owe')
            assert CRITERION in shown(), terminal.screen.text()
            assert 'No source gate owed.' not in shown(), terminal.screen.text()
            wait('older record, no sign-off recorded')
            assert {str(p.relative_to(run)): p.read_bytes() for p in run.rglob('*') if p.is_file()} == original
            sdlc(root, env, 'mark', 'test-label', 'test-manual', 'passed',
                 '--evidence', f'TEST {resolution} explicitly; synthetic fixture, not operator acceptance')
            send(F10, '1  think')
            send(b'5', 'Quiet')
            send(b'\r', '0 attestations owed')
            send(F10, '1  think')
            send(b'6', 'Press r to read results')
            send(b'r', 'TEST older result')
            send(b'\r', 'Is this good?')
            send(b'e', 'No source gate owed.')
            for name, content in original.items():
                assert (run / name).read_bytes() == content, name
            status = json.loads(sdlc(root, env, 'status', 'test-label', '--json').stdout)
            assert status['run']['checklist'][0]['status'] == 'passed', status
            assert 'operator_attestation' not in status['run']['checklist'][0]
            print(json.dumps(dict(journey='older attestation owed', width=width, height=height,
                                  no_color=mono, resolution=resolution, provider_calls=0,
                                  live_github_calls=0)))
        finally:
            terminal.close()


if __name__ == '__main__':
    journey(100, 30, False, 'signed off')
    journey(80, 24, True, 'waived')
