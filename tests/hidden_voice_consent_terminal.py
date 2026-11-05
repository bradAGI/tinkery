"""Voice download consent requires a visible prompt; fake helper, no audio or downloads."""
import json
import os
import tempfile
from pathlib import Path

from hidden_review_terminal import pump, resize_with_input
from think_history_terminal import Terminal

F6 = b'\x1b[17~'
PROMPT = 'Download now? y / n'
NODE = '''#!/usr/bin/env python3
import json, sys
from pathlib import Path
if sys.argv[1:] == ['--version']:
    print('v26.0.0')
    sys.exit(0)
print(json.dumps(dict(event='needsmodel', size_mb=732,
    message='Voice model is missing (732 MB). Download now? y / n')), flush=True)
Path('TEST-helper-ready').touch()
for line in sys.stdin:
    command = json.loads(line)
    with Path('TEST-commands.jsonl').open('a') as log:
        log.write(json.dumps(command) + '\\n')
    assert command['command'] == 'download', command
    print(json.dumps(dict(event='loading')), flush=True)
    print(json.dumps(dict(event='ready', version=1)), flush=True)
'''


def journey(width, height, open_first, decline=None):
    with tempfile.TemporaryDirectory(prefix='tinkery-TEST-hidden-voice-consent-') as directory:
        root = Path(directory)
        node = root / 'TEST-node'
        node.write_text(NODE)
        node.chmod(0o700)
        terminal = Terminal(root, 100, 30, [
            '--operator', 'agent', '--shape-budget', '0',
            '--record-root', root / 'TEST-records', '--seed-session-root', root / 'TEST-sessions',
        ], extra={'TINKERY_NODE': str(node)})

        def commands():
            path = root / 'TEST-commands.jsonl'
            return [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []

        try:
            terminal.wait(lambda screen: "What's on your mind?" in screen
                          and (root / 'TEST-helper-ready').exists())
            pump(terminal)  # Let the UI receive the helper's setup requirement before F6.
            if open_first:
                terminal.send(F6, PROMPT)
            resize_with_input(terminal, width, height, b'y' if open_first else F6 + b'y')
            terminal.wait(lambda screen: 'Need 80×24.' in screen)
            if decline:
                os.write(terminal.master, decline)
                pump(terminal)
            resize_with_input(terminal, 100, 30)
            if decline:
                terminal.wait(lambda screen: "What's on your mind?" in screen)
                assert PROMPT not in terminal.screen.text()
                assert commands() == [], 'hidden consent or cancellation started a download'
            else:
                terminal.wait(lambda screen: PROMPT in screen or bool(commands()))
                assert commands() == [], 'hidden y or resizing started a download'
                assert PROMPT in terminal.screen.text()
                os.write(terminal.master, b'y')
                terminal.wait(lambda screen: PROMPT not in screen and len(commands()) == 1)
                assert commands() == [{'command': 'download'}]
            assert not (root / 'TEST-records').exists()
            print(f'voice consent {width}×{height}, open_first={open_first}, decline={decline!r}: '
                  'hidden yes consumed; visible consent or cancellation respected')
        finally:
            terminal.close()


if __name__ == '__main__':
    for size in [(60, 20), (79, 24), (100, 23)]:
        for open_first in [False, True]:
            journey(*size, open_first)
    journey(60, 20, True, b'n')
    journey(79, 24, False, b'\x1b')
