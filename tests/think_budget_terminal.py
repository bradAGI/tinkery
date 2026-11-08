"""Launch budgets through real keys; fake Pi, temporary TEST roots, no providers."""
import json
from pathlib import Path
import tempfile

from think_history_terminal import Terminal
from shape_answer_terminal import make as make_session

HOST = '''#!/usr/bin/env python3
import json, pathlib, sys
request = json.load(sys.stdin)
with pathlib.Path('TEST-calls.jsonl').open('a') as log:
    log.write(json.dumps(request) + '\\n')
if request.get('kind') == 'decision-review':
    print('{"decisions":[]}')
else:
    print(json.dumps(dict(parts=dict(who='I', outcome='keep TEST budget words',
        why='the allowance matters', done_when='no extra request starts'),
        open=['why'], questions=[dict(id='reason', target='why', text='Which TEST reason?')])))
'''


def journey(limit, home, width, height):
    with tempfile.TemporaryDirectory(prefix='tinkery-TEST-budget-') as directory:
        root = Path(directory)
        host = root / 'TEST-pi'
        host.write_text(HOST)
        host.chmod(0o700)
        sessions = root / 'TEST-sessions'
        make_session(sessions)
        args = ['--operator', 'agent', '--shape-pi', '--model', 'test/budget',
                '--pi-command', host, '--seed-session-root', sessions,
                '--record-root', root / 'TEST-records']
        if home:
            args += ['--home']
        if limit is not None:
            args += ['--shape-budget', str(limit)]
        terminal = Terminal(root, width, height, args)

        def calls():
            path = root / 'TEST-calls.jsonl'
            return [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []

        try:
            if home:
                terminal.send(b'1', "What's on your mind?")
            else:
                terminal.wait(lambda screen: "What's on your mind?" in screen)
            assert calls() == []
            terminal.send(b'TEST budget words', 'TEST budget words')
            terminal.send(b'\x1bOQ', "Reading failed: Drafting run limit" if not limit else 'Which TEST reason?')
            assert len(calls()) == (limit or 0), calls()
            assert 'TEST budget words' in terminal.screen.text()
            # #435: the allowance is shown where a retry is decided, and in Right now help.
            left = f'Drafting runs left this launch: 0 of {limit or 0}.'

            def shows(text):
                return terminal.wait(lambda screen: text in ' '.join(screen.replace('│', ' ').split()))
            if not limit:
                shows(left)
            terminal.send(b'\x1bOP', 'Right now')
            terminal.send(b'\x1b[6~' * 5, 'Close help')  # Rows come first; the allowance follows.
            shows(left)
            terminal.send(b'\x1b', 'TEST budget words')
            # Typing before help closes would land in help (macOS CI race).
            terminal.wait(lambda screen: 'Close help' not in screen)

            # Another submission cannot borrow a fresh allowance.
            terminal.send(b'TEST another answer', 'TEST another answer')
            terminal.send(b'\x1bOQ', "Reading failed: Drafting run limit")
            assert len(calls()) == (limit or 0), calls()
            shows(left)

            if home:
                terminal.send(b'\x1b', 'lose this draft?')
                terminal.send(b'y', '1  think')
                terminal.send(b'1', "What's on your mind?")
                terminal.send(b'TEST new goal', 'TEST new goal')
                terminal.send(b'\x1bOQ', "Reading failed: Drafting run limit")
                assert len(calls()) == (limit or 0), 'New goal reset the launch allowance'
                terminal.send(b'\x1b', 'lose this draft?')
                terminal.send(b'y', '1  think')

                # Shape receives the same remaining allowance as Think.
                terminal.send(b'3', 'enter open   esc back')  # Home's bar also says "enter open".
                terminal.send(b'\r', 'Keep records local or synced?')
                terminal.send(b'Local TEST records', 'Local TEST records')
                terminal.send(b'\x1bOQ', 'No drafting runs are left')
                assert len(calls()) == (limit or 0), 'Shape reset the launch allowance'
            print(f'Think budget {limit!r}, home={home}, {width}x{height}: {len(calls())} fake Pi starts')
        finally:
            terminal.close()


if __name__ == '__main__':
    for size in [(80, 24), (100, 30)]:
        for home in [False, True]:
            for limit in [None, 0, 1, 2]:
                journey(limit, home, *size)
