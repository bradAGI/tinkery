"""A stray folder in runs/ never hides valid results (#715); synthetic API 1 and fake gh, no network."""
import hashlib
import json
import os
from pathlib import Path
import sys
import tempfile
import time

import test_launch as subprocess
from taste_terminal import FAKE, REPO, confirmed_seed, fixture, tree
from terminal_process import TerminalProcess, wait_output

STRAY = 'notes backup'


def journey(strays):
    with tempfile.TemporaryDirectory(prefix='tinkery-TEST-taste-stray-') as directory:
        base = Path(directory).resolve()
        checkout = base / 'TEST-checkout'
        (checkout / '_system/scripts').mkdir(parents=True)
        (checkout / '_system/scripts/sdlc.py').write_text(FAKE)
        seed = confirmed_seed(base)
        state = fixture({'source': str(seed), 'path': 'runs/TEST-result/seed-contract.md',
                         'sha256': hashlib.sha256(seed.read_bytes()).hexdigest()})
        # tink-sdlc lists every runs/ folder; one it can't read is invalid, with no usable id.
        state['list']['runs'] += [{'slug': name, 'run_id': '', 'verification_status': 'invalid',
                                   'error': 'TEST not a run', 'seed_contract': None} for name in strays]
        statefile, log, pid = base / 'state.json', base / 'calls', base / 'pid'
        statefile.write_text(json.dumps(state))
        binary = base / 'bin'
        binary.mkdir()
        (binary / 'python3').symlink_to(sys.executable)
        (binary / 'gh').write_text(f'#!{sys.executable}\n' + FAKE)
        (binary / 'gh').chmod(0o755)
        env = {**os.environ, 'HOME': str(base / 'HOME'), 'PATH': f'{binary}:/usr/bin:/bin',
               'TERM': 'xterm-256color', 'TINKERY_NODE': '/tinkery-TEST-no-node', 'NO_COLOR': '1',
               'TEST_TASTE_STATE': str(statefile), 'TEST_TASTE_CALLS': str(log), 'TEST_TASTE_PID': str(pid)}
        original = tree(checkout)
        terminal = TerminalProcess([str(REPO / 'target/debug/tinkery'), '--operator', 'agent', '--home',
                                    '--seed-session-root', str(base / 'TEST-sessions'), '--sdlc-root', str(checkout)],
                                   cwd=base, env=subprocess.environment(env, home=env['HOME']), width=100, height=30)
        screen = terminal.screen

        def flat():
            return ' '.join(screen.text().split())

        def wait(text, seconds=8):
            deadline = time.monotonic() + seconds
            for returncode in wait_output(terminal.master, terminal.process, screen.feed, deadline):
                if text in flat() and not screen.synchronized:
                    return
                assert returncode is None, screen.text()
            raise AssertionError((text, screen.text()))

        try:
            wait('1 think')
            os.write(terminal.master, b'6')
            wait('Press r to read results')
            os.write(terminal.master, b'r')
            wait('TEST clinic allocation comparison')
            assert "Couldn't read results" not in flat(), screen.text()
            if strays:
                many = len(strays) > 1
                wait(f"Skipped {len(strays)} folder{'s' if many else ''} in runs/ whose name isn't a run name; "
                     f"nothing in {'them' if many else 'it'} was read.")
            else:
                assert 'Skipped' not in flat(), screen.text()
            calls = [json.loads(line) for line in log.read_text().splitlines()]
            assert not any(name in ' '.join(call) for call in calls for name in strays), calls
            assert tree(checkout) == original
            print(f'Taste with {len(strays)} stray runs/ folders: valid result shown, stray entries unread')
        finally:
            terminal.close()


if __name__ == '__main__':
    journey([])
    journey([STRAY])
    journey([STRAY, '../outside', 'TEST\x07bell'])
