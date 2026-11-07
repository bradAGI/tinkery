"""A Taste verdict covers its read head, even when the PR moves before F2 (#781).

Real bundled SDLC status and label writer, synthetic human-role input and GitHub metadata.
No actual human acceptance, providers or live GitHub calls.
"""
import json
import os
from pathlib import Path
import sys
import tempfile
import time

import test_launch as subprocess
from taste_label_terminal import checkout, labels, sdlc, F2, ESC
from terminal_process import TerminalProcess, wait_output

REPO = Path(__file__).resolve().parents[1]
A, B = 'a' * 40, 'b' * 40
URL = 'https://github.com/TEST/verdict/pull/1'
WORDS = '  TEST only: keep "these words" — Ω  '
REMINDER = 'awaiting human verdict'


def journey(width, height, mono, initial_kind):
    with tempfile.TemporaryDirectory(prefix='tinkery-TEST-verdict-head-') as directory:
        base = Path(directory).resolve()
        tools = base / 'bin'
        tools.mkdir()
        head = base / 'TEST-head'
        head.write_text(A)
        (tools / 'gh').write_text(f'''#!{sys.executable}
import json, sys
from pathlib import Path
if sys.argv[1:3] == ['pr', 'view']:
    print(json.dumps(dict(url={URL!r}, title='TEST moving result', state='OPEN',
                         headRefOid=Path({str(head)!r}).read_text(), statusCheckRollup=[])))
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
        root, git = checkout(base, env)
        git('config', 'user.name', 'TEST synthetic reviewer')
        (root / 'runs/test-label/pr.json').write_text(json.dumps(dict(url=URL, candidate=A)))

        def launch(role):
            return TerminalProcess(
                [str(REPO / 'target/debug/tinkery'), '--operator', role, '--home',
                 '--shape-budget', '0', '--sdlc-root', str(root),
                 '--seed-session-root', str(base / 'TEST-sessions')],
                cwd=base, env=env, width=width, height=height)

        terminal = launch('human')

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

        def read_list():
            wait('1  think')
            send(b'6', 'Press r to read results')
            send(b'r', 'TEST moving result')

        try:
            read_list()
            assert REMINDER in shown(), terminal.screen.text()
            send(b'\r', 'Is this good?')
            send(b'w', 'Signed as TEST synthetic reviewer.')
            send(initial_kind.encode(), '› 1 pass' if initial_kind == '1' else '› 2 fail')
            send(WORDS.encode(), 'Ω')
            head.write_text(B)
            send(F2, 'Saved your label.')
            send(b'r', 'Reading results from the named checkout')
            wait('TEST moving result')
            send(ESC, 'Enter open result')
            assert REMINDER in shown(), (
                'Verdict on A incorrectly covers B after refresh', labels(root), terminal.screen.text())
            send(b'\r', 'Is this good?')
            send(b'e', 'Human verdict')
            assert 'stale' in terminal.screen.text(), terminal.screen.text()
            [record] = labels(root)
            assert record['source'] == f'taste@{A}', record
            assert record['words'] == WORDS, record
            before = (root / 'runs/test-label/labels.jsonl').read_bytes()
            terminal.close()
            terminal = launch('agent')
            read_list()
            assert REMINDER in shown(), terminal.screen.text()
            assert (root / 'runs/test-label/labels.jsonl').read_bytes() == before
            terminal.close()
            terminal = launch('human')
            read_list()
            send(b'\r', 'Is this good?')
            send(b'w', 'Signed as TEST synthetic reviewer.')
            current_kind = '2' if initial_kind == '1' else '1'
            send(current_kind.encode(), '› 2 fail' if current_kind == '2' else '› 1 pass')
            send(b'--TEST-current', '--TEST-current')
            send(F2, 'Saved your label.')
            send(b'r', 'Reading results from the named checkout')
            wait('TEST moving result')
            send(ESC, 'Enter open result')
            assert REMINDER not in shown(), terminal.screen.text()
            assert [(r['source'], r['words']) for r in labels(root)] == [
                (f'taste@{A}', WORDS), (f'taste@{B}', '--TEST-current')]
            status = json.loads(sdlc(root, env, 'status', 'test-label', '--json').stdout)
            assert status['run']['labels']['count'] == 2, status
            terminal.close()
            terminal = launch('agent')
            read_list()
            assert REMINDER not in shown(), terminal.screen.text()
            print(json.dumps(dict(journey='Taste verdict head', width=width, height=height,
                                  no_color=mono, initial_kind=initial_kind, pty_sessions=4, stale_after_refresh=True,
                                  stale_after_restart=True, current_counts=True,
                                  verbatim=True, provider_calls=0, live_github_calls=0)))
        finally:
            terminal.close()


if __name__ == '__main__':
    for kind in ('1', '2'):
        journey(100, 30, False, kind)
        journey(80, 24, True, kind)
