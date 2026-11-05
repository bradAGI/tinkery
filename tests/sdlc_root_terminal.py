"""An explicit checkout is required; empty CLI roots never select the working directory."""
import json
import tempfile
from pathlib import Path

import test_launch as subprocess
from think_history_terminal import BINARY, MENU, Terminal

FAKE = '''import json, sys
from pathlib import Path
with Path('TEST-calls.jsonl').open('a') as log:
    log.write(json.dumps(sys.argv[1:]) + '\\n')
print(json.dumps(dict(protocol='tink-sdlc', api_version=1, workspace=str(Path.cwd()), runs=[])))
'''


def checkout(root):
    script = root / '_system/scripts/sdlc.py'
    script.parent.mkdir(parents=True)
    script.write_text(FAKE)


def calls(root):
    log = root / 'TEST-calls.jsonl'
    return [json.loads(line) for line in log.read_text().splitlines()] if log.exists() else []


def refused_roots():
    with tempfile.TemporaryDirectory(prefix='tinkery-TEST-sdlc-root-refusal-') as directory:
        root = Path(directory).resolve()
        checkout(root)
        for arguments in [
            ['--sdlc-root'],
            ['--sdlc-root', ''],
            ['--sdlc-root', str(root), '--sdlc-root', ''],
            ['--sdlc-root', '', '--sdlc-root', str(root)],
        ]:
            result = subprocess.run([str(BINARY), '--operator', 'agent', '--home', '--snapshot',
                                     *arguments], cwd=root, env=subprocess.environment(),
                                    capture_output=True, timeout=5)
            assert result.returncode != 0, arguments
            assert b'--sdlc-root needs a tink-sdlc checkout path' in result.stderr, result.stderr
            assert result.stdout == b'', 'invalid root reached screen startup'
            assert calls(root) == [], 'invalid root executed the current checkout'
        print('Missing and empty roots refused, including repeated options; no script ran.')


def selected_root(width, height, kind):
    with tempfile.TemporaryDirectory(prefix='tinkery-TEST-sdlc-root-selection-') as directory:
        root = Path(directory).resolve()
        selected = root / 'TEST selected checkout'
        checkout(root)
        checkout(selected)
        argument = {'absolute': str(selected), 'relative': selected.name, 'current': '.'}[kind]
        expected = root if kind == 'current' else selected
        other = selected if kind == 'current' else root
        terminal = Terminal(root, width, height, ['--operator', 'agent', '--home',
            '--sdlc-root', argument, '--seed-session-root', root / 'TEST-sessions'])
        try:
            terminal.wait(lambda screen: MENU in screen)
            assert calls(root) == calls(selected) == [], 'a script ran before Watch opened'
            terminal.send(b'5', 'No runs in this checkout yet.')
            assert calls(expected) == [['status', '--json']], (argument, calls(expected))
            assert calls(other) == [], 'a different checkout ran'
            terminal.quit()
            print(f'{kind} root at {width}x{height}: only the selected checkout ran.')
        finally:
            terminal.close()


if __name__ == '__main__':
    refused_roots()
    for width, height in [(80, 24), (100, 30)]:
        for kind in ['absolute', 'relative', 'current']:
            selected_root(width, height, kind)
