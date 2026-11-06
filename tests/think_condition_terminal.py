"""Think keeps the operator's condition in the goal; fake Pi, temporary TEST roots, no providers."""
import json
import re
from pathlib import Path
import tempfile

from think_history_terminal import Terminal
from shape_answer_terminal import make as make_session

HOST = '''#!/usr/bin/env python3
import json, sys
request = json.load(sys.stdin)
if request.get('kind') == 'decision-review':
    print('{"decisions":[]}')
else:
    print(json.dumps(dict(parts=dict(who='I', outcome=OUTCOME, when='at work',
        why='TEST reason', done_when='TEST done'),
        open=['why'], questions=[dict(id='reason', target='why', text='Which TEST reason?')])))
'''


def journey(outcome, goal):
    with tempfile.TemporaryDirectory(prefix='tinkery-TEST-condition-') as directory:
        root = Path(directory)
        host = root / 'TEST-pi'
        host.write_text(HOST.replace('OUTCOME', json.dumps(outcome)))
        host.chmod(0o700)
        sessions = root / 'TEST-sessions'
        make_session(sessions)
        terminal = Terminal(root, 100, 30, [
            '--operator', 'agent', '--shape-pi', '--model', 'test/condition', '--pi-command', host,
            '--shape-budget', '1', '--seed-session-root', sessions, '--record-root', root / 'TEST-records'])
        try:
            terminal.wait(lambda screen: "What's on your mind?" in screen)
            terminal.send(b'TEST thermostat words', 'TEST thermostat words')
            screen = terminal.send(b'\x1bOQ', 'Which TEST reason?')
            text = ' '.join(screen.replace('│', ' ').split())
            assert goal in text, text
            assert len(re.findall(r'\bat work\b', text)) == 1, text
            print(f'Think condition: {goal!r} shown once')
        finally:
            terminal.close()


if __name__ == '__main__':
    # #717: "thermostat works" contains the letters of "at work" but does not state it.
    journey('see if my thermostat works', 'I see if my thermostat works, at work')
    # A condition the outcome already states as whole words is not repeated.
    journey('check the thermostat at work', 'I check the thermostat at work')
