"""Long Think exclusions stay reachable before review; fake replies, isolated TEST roots."""
from terminal_process import wait_output
import fcntl, json, os, select, struct, tempfile, termios, time
import test_launch as subprocess
from pathlib import Path
from vt_screen import Screen

BINARY = Path(__file__).resolve().parents[1] / 'target/debug/tinkery'
HINT = 'PgUp/PgDn scroll reading'
EXCLUSIONS = [
    f'share café notes from folder {i} with an outside service or replace the protected original words belonging to reader {i}'
    for i in range(1, 7)
]
GOAL = '\n'.join([
    'I inspect the newsletter plan.',
    'Why: my boundaries matter.',
    'Done when: I see every exclusion.',
    *(f'Must not: {line}.' for line in EXCLUSIONS),
])
DUMP = '  Keep these exact words.\n' + GOAL + '\n  '
HOST = '''#!/usr/bin/env python3
import json,sys
r=json.load(sys.stdin)
with open('calls.jsonl','a') as f: f.write(json.dumps(r)+'\\n')
if r.get('kind')=='decision-review':
 print('{"decisions":[]}');sys.exit(0)
print(json.dumps(dict(parts=dict(who='I',outcome='inspect the newsletter plan',why='my boundaries matter',done_when='I see every exclusion',must_not=EXCLUSIONS),open=['why'],supports=[dict(source=s['id'],quote=s['text'],occurrence=0) for s in r['sources']],misfits=[],questions=[dict(target='why',id='boundary',text='Which boundary matters most?')],alternatives=[])))
'''.replace('EXCLUSIONS', repr(EXCLUSIONS))


def journey(width, height, no_color):
    with tempfile.TemporaryDirectory(prefix='tinkery-TEST-reading-scroll-') as directory:
        cwd = Path(directory)
        host = cwd / 'pi'; host.write_text(HOST); host.chmod(0o755)
        environment = {**os.environ, 'TERM': 'xterm-256color', 'HOME': str(cwd / 'TEST-home'), 'TINKERY_NODE': '/tinkery-TEST-no-node'}
        environment.pop('NO_COLOR', None)
        if no_color:
            environment['NO_COLOR'] = '1'
        master, slave = os.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', height, width, 0, 0))
        screen = Screen(width, height)
        process = subprocess.Popen(
            [str(BINARY), '--shape-pi', '--shape-budget', '64', '--model', 'test/model', '--pi-command', str(host), '--record-root', str(cwd / 'TEST-records'),
             '--seed-session-root', str(cwd / 'TEST-sessions')],
            stdin=slave, stdout=slave, stderr=slave, cwd=cwd, env=subprocess.environment(environment))

        def pump(seconds=0.08):
            deadline = time.monotonic() + seconds
            while time.monotonic() < deadline:
                if select.select([master], [], [], 0.01)[0]:
                    screen.feed(os.read(master, 65536))

        def wait_for(condition, description, after=0):
            deadline = time.monotonic() + 5
            for returncode in wait_output(master, process, screen.feed, deadline):
                if condition() and screen.ready(after):
                    return
                if returncode is not None:
                    break
            raise AssertionError(f'{description} at {width}x{height}, NO_COLOR={no_color}:\n{screen.text()}')

        def send(keys):
            os.write(master, keys); pump()

        def right():
            return [line[width * 48 // 100 + 2:].rstrip() for line in screen.text().splitlines()]

        def reading():
            lines = right()
            assert HINT in lines, f'Missing reading scroll hint:\n{screen.text()}'
            return lines[4:lines.index(HINT)]

        def copy(keys):
            before = len(screen.clipboards)
            os.write(master, keys)
            wait_for(lambda: len(screen.clipboards) == before + 1, 'Clipboard reply missing')
            return screen.clipboards[-1]

        def joined(lines):
            return ' '.join(' '.join(lines).split())

        try:
            wait_for(lambda: "What's on your mind?" in screen.text(), 'Initial screen missing')
            assert HINT not in screen.text()
            send(b'\x1b[200~' + DUMP.encode() + b'\x1b[201~')
            assert not (cwd / 'calls.jsonl').exists()
            send(b'\x1bOQ')
            wait_for(lambda: 'Which boundary matters most?' in screen.text(), 'Reading missing')
            initial = reading()
            assert 'reader 6.' not in joined(initial), 'Fixture no longer overflows'
            calls = (cwd / 'calls.jsonl').read_bytes()
            requests = [json.loads(line) for line in calls.splitlines()]
            assert len(requests) == 2 and requests[0]['sources'][0]['text'] == DUMP
            before = screen.frames
            os.write(master, b'\x1bOP')
            wait_for(lambda: 'Copy the reading' in screen.text(), 'Reading actions missing', before)
            copied = copy(screen.pick('Copy the reading'))
            assert '\n## I think this is about… / guess\n\n' + GOAL + '\n\n' in copied, copied
            assert DUMP in copied

            # Merge the overlapping pages to check every word, including wrapped Unicode.
            seen = list(initial)
            previous = initial
            for _ in range(24):
                send(b'\x1b[6~')
                current = reading()
                if current == previous:
                    break
                overlap = next((n for n in range(len(current), 0, -1) if seen[-n:] == current[:n]), 0)
                seen.extend(current[overlap:])
                previous = current
            else:
                raise AssertionError('Reading did not reach its end')
            for exclusion in EXCLUSIONS:
                assert 'Must not: ' + exclusion + '.' in joined(seen), (exclusion, seen)
            assert 'reader 6.' in joined(current)
            assert 'type confirm' not in screen.text(), 'Scrolling opened review'

            # Help names this route and leaves both the reading position and draft intact.
            draft = 'Keep this café answer.'
            send(draft.encode()); before_help = reading()
            send(b'\x1bOP')
            before = screen.frames
            os.write(master, b'h')
            wait_for(lambda: '┌ How it works ' in screen.text(), 'How it works missing', before)
            help_seen = []
            for _ in range(24):
                help_seen.append(' '.join(joined(right()).replace('│', '').split()))
                if any('PgUp/PgDn   scroll the reading' in line or 'PgUp/PgDn scroll the reading' in line for line in help_seen):
                    break
                before = screen.frames
                previous = right()
                os.write(master, b'\x1b[6~')
                wait_for(lambda: '┌ How it works ' in screen.text() and right() != previous,
                         'Help page did not advance to the reading instructions', before)
            else:
                raise AssertionError('How it works does not document reading scrolling')
            before = screen.frames
            os.write(master, b'\x1b')
            wait_for(lambda: '┌ Right now ' in screen.text(), 'Contextual help missing', before)
            before = screen.frames
            os.write(master, b'\x1b')
            wait_for(lambda: HINT in right() and '┌ Right now ' not in screen.text(),
                     'Reading did not return after Help', before)
            assert reading() == before_help
            send(b'\x01')
            assert copy(b'\x03') == draft
            send(b'\x1bOP')
            assert copy(screen.pick('Copy the reading')) == copied
            send(b'\x1b[5~' * 24)
            assert reading() == initial, 'PgUp did not return to the start'

            # Review still needs its own explicit action; exiting starts and saves nothing.
            send(b'\x7f'); send(b'\x1bOP'); send(screen.pick('Review the goal'))
            wait_for(lambda: 'You are confirming' in screen.text(), 'Review missing')
            send(b'\x1b'); send(b'\x1b')
            wait_for(lambda: 'Back to menu and lose this draft?' in screen.text(), 'Exit guard missing')
            send(b'n')
            wait_for(lambda: 'lose this draft?' not in screen.text(), 'Exit guard did not close')
            assert process.poll() is None
            send(b'\x1b'); send(b'y')
            process.wait(timeout=5)
            assert process.returncode == 0
            assert (cwd / 'calls.jsonl').read_bytes() == calls, 'Navigation or copying started a call'
            assert {p.name for p in cwd.iterdir()} == {'pi', 'calls.jsonl'}, 'Unexpected persistence'
            print(json.dumps(dict(width=width, height=height, NO_COLOR=no_color, exclusions=len(EXCLUSIONS), fake_calls=len(requests), journey='reading hint/full Unicode text/PgUp/help/copy/review/guarded exit', saved=False)))
        finally:
            if process.poll() is None:
                process.kill(); process.wait()
            os.close(master); os.close(slave)


for size in [(80, 24), (100, 30)]:
    for no_color in [False, True]:
        journey(*size, no_color)
