"""Approve's read-only handoff over official Seed Me TEST fixtures; no provider or real approval."""
from terminal_process import wait_output
import base64
import fcntl
import hashlib
import json
import os
import re
import select
import shlex
import struct
import test_launch as subprocess
import tempfile
import termios
import time
from pathlib import Path

from shape_terminal import make, seed, files
from vt_screen import Screen

REPO = Path(__file__).resolve().parents[1]


def journey(width, height, case, no_color):
    with tempfile.TemporaryDirectory(prefix="tinkery-TEST-approve-") as directory:
        root = Path(directory).resolve()
        sessions = root / "TEST sessions' folder"
        expected = []
        retarget = case == 'retarget-checkout'
        if retarget or case.startswith('target-') or case in ('ready', 'started', 'edited', 'moved', 'runs-unreadable', 'no-checkout', 'different-repo'):
            for name, date in [('TEST oldest café', '2026-01-01T00:00:00Z'),
                               ('TEST newest', '2026-02-01T00:00:00Z')]:
                # #434: a ready goal tall enough to push the command below the fold at both sizes.
                extra = ''.join(f'\nTEST goal line {i}' for i in range(20)) if case == 'ready' else ''
                session = make(sessions, name, goal=name + '\nTEST original goal.' + extra, question=False)
                seed(session, confirm=True, title=name)
                ledger = session / 'ledger.json'
                value = json.loads(ledger.read_text())
                value['updated_at'] = date
                ledger.write_text(json.dumps(value))
                expected.append(session)
            draft = make(sessions, 'draft', goal='TEST excluded draft', question=False)
            seed(draft)
            stale = make(sessions, 'stale', goal='TEST excluded stale', question=False)
            seed(stale, confirm=True)
            path = stale / 'seed-contract.md'
            path.write_text(path.read_text() + '\nTEST changed seed\n')
            make(sessions, 'question', goal='TEST excluded question')
        elif case == 'incomplete':
            broken = sessions / 'broken'
            broken.mkdir(parents=True)
            (broken / 'ledger.json').write_text('not JSON')
        elif case == 'unreadable':
            sessions.write_text('TEST not a directory')
        checkout = root / "TEST checkout's folder"
        if case == 'target-twin-checkouts':
            checkout = root / r'TEST\tparent' / checkout.name
        configured = retarget or case.startswith('target-') or case in ('ready', 'started', 'edited', 'moved', 'runs-unreadable', 'different-repo')
        if configured:
            script = checkout / '_system/scripts/sdlc.py'
            script.parent.mkdir(parents=True)
            original = expected[0] / 'seed-contract.md'
            record = {'path': 'runs/TEST-other/seed-contract.md', 'source': str(original.resolve()),
                      'sha256': hashlib.sha256(original.read_bytes()).hexdigest()}
            report = {'protocol': 'tink-sdlc', 'api_version': 1, 'runs': [
                {'slug': 'TEST-unrelated-name', 'seed_contract': record}] if not case.startswith('target-') and case not in ('ready', 'different-repo') else []}
            script.write_text('print(' + repr(json.dumps(report)) + ')\n'
                              if case != 'runs-unreadable' else 'raise SystemExit("TEST unreadable runs")\n')
            if case == 'edited':
                original.write_text(original.read_text() + '\nTEST edit after intake\n')
            elif case == 'moved':
                moved = sessions / 'TEST moved session'
                expected[0].rename(moved)
                expected[0] = moved
        named_checkout = checkout.relative_to(root) if case == 'target-twin-checkouts' else checkout
        if retarget or case in ('different-repo', 'target-twin-checkouts'):
            (checkout / '.git').mkdir()
            other = root / 'TEST-other-repo'
            if case == 'target-twin-checkouts':
                other = root / 'TEST\tparent' / checkout.name
            (other / '.git').mkdir(parents=True)
            if retarget:
                named_checkout = root / 'current'
                named_checkout.symlink_to(checkout)
                # Only A reports the older seed started. Switch the alias inside A's second
                # status read, after its cwd is set and before it returns the run report.
                script.write_text(
                    'from pathlib import Path\nimport sys\n'
                    f'assert Path.cwd() == Path({str(checkout)!r}) and sys.argv[1:] == ["status", "--json"]\n'
                    'next_checkout = Path("../next-checkout")\n'
                    'if next_checkout.exists(): next_checkout.replace("../current")\n'
                    + script.read_text())
                other_script = other / '_system/scripts/sdlc.py'
                other_script.parent.mkdir(parents=True)
                other_script.write_text('print(\'{"protocol":"tink-sdlc","api_version":1,"runs":[]}\')\n')
            records = root / 'TEST-records'
            records.mkdir()
            for index, session in enumerate(expected):
                ledger = json.loads((session / 'ledger.json').read_text())
                record = dict(schema=3, tinkery='TEST', origin='simulated',
                    started='2026-01-01T00:00:00Z', confirmed='2026-01-01T00:00:00Z',
                    seed_me_session=session.name, seed_me_session_path=str(session),
                    seed_me_session_id=ledger['session_id'], repo=str(checkout if retarget else other),
                    turns=[], checks=[], goal=ledger['goal'])
                (records / f'TEST-{index}.json').write_text(json.dumps(record))
        if case.startswith('target-'):
            for folder in sessions.iterdir():
                if folder not in expected:
                    ledger = folder / 'ledger.json'; value = json.loads(ledger.read_text())
                    value['updated_at'] = '2025-01-01T00:00:00Z'; ledger.write_text(json.dumps(value))
        host = root / 'pi'
        host.write_text('#!/bin/sh\ntouch TEST-provider-was-run\nexit 86\n')
        host.chmod(0o755)
        before = files(root)
        master, slave = os.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', height, width, 0, 0))
        env = {**os.environ, 'HOME': str(root), 'TERM': 'xterm-256color',
               'TINKERY_NODE': '/tinkery-TEST-no-node', 'PATH': str(root) + os.pathsep + os.environ['PATH']}
        if no_color:
            env['NO_COLOR'] = '1'
        else:
            env.pop('NO_COLOR', None)
        proc = subprocess.Popen([str(REPO / 'target/debug/tinkery'), '--operator', 'agent', '--home',
            '--seed-session-root', str(sessions),
            '--record-root', str(root / 'TEST-records'), '--link-root', str(root / 'TEST-links'),
            *(['--sdlc-root', str(named_checkout)] if configured else [])], stdin=slave, stdout=slave, stderr=slave, cwd=root, env=subprocess.environment(env, home=(env)['HOME']))
        screen = Screen(width, height)
        output = bytearray()

        def flat():
            return ' '.join(screen.text().split())

        def wait(predicate):
            until = time.monotonic() + 15
            def consume_wait_output(chunk):
                output.extend(chunk)
                screen.feed(chunk)
            for returncode in wait_output(master, proc, consume_wait_output, until):
                if predicate() and not screen.synchronized:
                    return
                if returncode is not None:
                    break
            raise AssertionError(screen.text())

        def send(keys, text):
            os.write(master, keys)
            wait(lambda: text in flat())

        def copies():
            return [base64.b64decode(data).decode() for data in
                    re.findall(rb'\x1b\]52;[^;]*;([^\x07\x1b]*)(?:\x07|\x1b\\)', output)]

        try:
            wait(lambda: '4  approve' in screen.text())
            if case.startswith('target-'):
                target = expected[1]
                send(b'2', '↑↓←→ move')
                send(b'\r', 's shape')
                assert 'TEST newest' in flat(), screen.text()
                send(b's', 'Seed ready:')
                assert 'F1, then 1: Open this seed in Approve.' in flat(), screen.text()
                assert files(root) == before
                # F1 opens only the actions list; the deliberate first choice navigates.
                send(b'\x1bOP', '1 Open this seed in Approve')
                assert 'tinkery › shape' in flat() and not copies(), screen.text()
                seedpath = target / 'seed-contract.md'
                ledgerpath = target / 'ledger.json'
                recordpath = target / 'seed-confirmation.json'
                if case == 'target-stale':
                    seedpath.write_text(seedpath.read_text() + '\nTEST changed after Shape read.\n')
                elif case == 'target-missing':
                    seedpath.unlink()
                elif case == 'target-started':
                    report['runs'] = [{'slug':'TEST-started', 'seed_contract':{'source':str(seedpath.resolve()), 'sha256':hashlib.sha256(seedpath.read_bytes()).hexdigest(), 'path':'runs/TEST-started/seed-contract.md'}}]
                    script.write_text('print(' + repr(json.dumps(report)) + ')\n')
                elif case == 'target-version':
                    ledger = json.loads(ledgerpath.read_text());ledger['version'] += 1
                    ledgerpath.write_text(json.dumps(ledger))
                elif case == 'target-recreated':
                    ledger = json.loads(ledgerpath.read_text());ledger['session_id'] = 'TEST-recreated-id'
                    ledgerpath.write_text(json.dumps(ledger))
                    record = json.loads(recordpath.read_text());record['session_id'] = ledger['session_id']
                    recordpath.write_text(json.dumps(record))
                elif case == 'target-hash':
                    seedpath.write_text(seedpath.read_text() + '\nTEST newly confirmed bytes at the same revision.\n')
                    record = json.loads(recordpath.read_text());record['seed_sha256'] = hashlib.sha256(seedpath.read_bytes()).hexdigest()
                    recordpath.write_text(json.dumps(record))
                before = files(root)  # Only the authored TEST mutation above is allowed.
                success = case in ('target-ready', 'target-twin-checkouts')
                send(b'1', 'Seed 1 of 1' if success else 'Ready seeds unavailable.')
                assert 'TEST oldest café' not in flat(), 'targeted visit substituted an unrelated seed'
                send(b'\x1bOP', 'Esc returns to Shape.')
                assert 'Esc returns to the menu.' not in flat(), screen.text()
                send(b'\x1b', 'Seed 1 of 1' if success else 'Ready seeds unavailable.')
                if success:
                    assert 'TEST newest' in flat() and 'TEST original goal.' in flat()
                    if case == 'target-twin-checkouts':
                        send(b'\x1b[F', '--sdlc-root')
                        # One parent contains a literal backslash followed by t, the other a tab.
                        named = json.dumps(str(checkout), ensure_ascii=False)
                        origin = json.dumps(str(other), ensure_ascii=False)
                        expected_note = (f'You named {named}. Reopen Tinkery with --sdlc-root naming '
                                         f'{origin}, where this seed was made.')
                        # The terminal can wrap paths in the middle of a word.
                        assert ''.join(expected_note.split()) in ''.join(flat().split()), screen.text()
                        assert 'c copy command' not in flat() and 'python3' not in flat()
                        os.write(master, b'c')
                    else:
                        send(b'c', 'Copied. Run it in a terminal; after it succeeds, open Watch.')
                        wait(lambda: len(copies()) == 1)
                        words = shlex.split(copies()[0])
                        assert words[6] == 'test-newest' and words[11] == 'test-newest', words
                        assert words[words.index('--seed-contract')+1] == str(seedpath.resolve())
                else:
                    if case == 'target-started':assert 'already started' in flat(), screen.text()
                    if case in ('target-version','target-recreated','target-hash'):assert 'changed since Shape read it' in flat(), screen.text()
                    os.write(master,b'cnp');time.sleep(.1);assert not copies()
                send(b'\x1b', 'tinkery › shape')
                if case == 'target-twin-checkouts':
                    assert not copies(), 'same-named checkout mismatch offered a copy'
                wait(lambda: 'TEST newest' in flat())
                if case in ('target-stale','target-missing'):assert 'Seed ready:' not in flat(), screen.text()
                else:wait(lambda: 'Seed ready:' in flat())
                if case in ('target-version','target-recreated','target-hash'):
                    # Back rereads the now-valid replacement identity, never reuses the old one.
                    send(b'\x1bOP','1 Open this seed in Approve');send(b'1','Seed 1 of 1')
                    assert 'TEST newest' in flat() and 'TEST oldest café' not in flat()
                    send(b'\x1b','Seed ready:')
                if case == 'target-ready':
                    send(b'\x1bOP','1 Open this seed in Approve');send(b'1','Seed 1 of 1')
                    send(b'\x1b[21~','4 approve')  # F10 drops the route-specific origin.
                    send(b'2','↑↓←→ move');send(b'\r','s shape');send(b's','Seed ready:')
                send(b'\x1b','s shape')
                wait(lambda: 'tinkery › tend' in flat() and 'TEST newest' in flat())
                send(b'\x1b[21~','4 approve')
                assert files(root) == before and not (root/'TEST-provider-was-run').exists()
                print(json.dumps(dict(width=width,height=height,no_color=no_color,journey='approve/'+case,provider_calls=0,session_writes=0,copy_actions=int(case=='target-ready'))))
                return
            os.write(master, b'4')
            if retarget:
                for visit in (1, 2):
                    wait(lambda: 'Seed 1 of 1' in flat())
                    assert 'TEST newest' in flat() and "Can't tell which seeds have started" not in flat()
                    assert named_checkout.resolve() == (checkout if visit == 1 else other)
                    send(b'c', 'Copied. Run it in a terminal; after it succeeds, open Watch.')
                    wait(lambda: len(copies()) == visit)
                    command = shlex.split(copies()[-1])
                    assert command[:2] == ['cd', str(checkout)], command
                    assert command[command.index('--seed-contract') + 1] == str(expected[1] / 'seed-contract.md')
                    send(b'\x1b', '4 approve')
                    if visit == 1:
                        (root / 'next-checkout').symlink_to(other)
                        os.write(master, b'4')
                send(b'4', 'Seed 1 of 2')
                assert 'TEST oldest café' in flat() and "Can't tell which seeds have started" not in flat()
                send(b'\x1b[F', 'Made in ' + checkout.name)
                assert 'c copy command' not in flat(), screen.text()
                os.write(master, b'c')
                send(b'\x1b', '4 approve')
                assert len(copies()) == 2, 'stable mismatched checkout offered a copy'
            elif case in ('started', 'edited'):
                wait(lambda: 'Seed 1 of 1' in flat())
                assert 'TEST newest' in flat() and 'TEST oldest café' not in flat(), screen.text()
                if case == 'edited':
                    assert 'unknown start' in flat(), screen.text()
                else:
                    assert 'unknown start' not in flat(), screen.text()
            elif case == 'different-repo':
                wait(lambda: 'Seed 1 of 2' in flat())
                assert 'TEST oldest café' in flat() and 'TEST original goal.' in flat(), screen.text()
                send(b'\x1b[F', 'Made in TEST-other-repo,')
                assert "not the checkout you named (TEST checkout's folder)." in flat(), screen.text()
                assert 'To start it, reopen Tinkery with --sdlc-root naming TEST-other-repo.' in flat(), screen.text()
                assert 'c copy command' not in flat() and 'python3' not in flat(), screen.text()
                os.write(master, b'c')
                send(b'\x1b', '4 approve')
                assert not copies(), 'mismatched repo offered a copy'
            elif case == 'no-checkout':
                wait(lambda: 'Seed 1 of 2' in flat())
                send(b'\x1b[F', 'Cannot copy:')
                assert 'c copy command' not in flat(), screen.text()
                os.write(master, b'c')
                send(b'\x1b', '4 approve')
                assert not copies()
            elif case in ('ready', 'moved', 'runs-unreadable'):
                wait(lambda: 'Seed 1 of 2' in flat())
                if case == 'runs-unreadable':
                    assert "Can't tell which seeds have started; none are hidden." in flat(), screen.text()
                assert 'TEST oldest café' in flat() and 'TEST newest' not in flat(), screen.text()
                assert 'TEST original goal.' in flat(), screen.text()
                if case == 'ready':
                    # The hidden command is announced, never mistaken for absent (#434).
                    assert '↓ more' in flat() and '--seed-contract' not in flat(), screen.text()
                    send(b'\x1b[F', '↑ more')
                    assert '--tinkery' in flat() and '↓ more' not in flat(), screen.text()
                acceptance = (expected[0] / 'seed-contract.md').read_text().split('## Acceptance criteria\n', 1)[1].split('\n## ', 1)[0]
                for line in acceptance.splitlines():
                    if line.strip():
                        assert ' '.join(line.split()) in flat(), (line, screen.text())
                assert not copies()
                send(b'c', 'Copied. Run it in a terminal; after it succeeds, open Watch.')
                wait(lambda: len(copies()) == 1)
                commands = copies()[0].splitlines()
                assert len(commands) == 1 and '<run>' not in commands[0], commands
                words = shlex.split(commands[0])
                assert words[:10] == ['cd', str(checkout.resolve()), '&&', 'python3',
                    '_system/scripts/sdlc.py', 'new', 'test-oldest-caf', '&&', 'python3', '_system/scripts/sdlc.py'], words
                assert words[10:] == ['stage', 'test-oldest-caf', '1',
                    '--seed-contract', str((expected[0] / 'seed-contract.md').resolve()),
                    '--tinkery', str((REPO / 'target/debug/tinkery').resolve())]
                send(b'\x1b[F', '--tinkery')
                assert 'esc back' in flat()
                send(b'?', 'Right now')
                os.write(master, b'cnp')
                send(b'\x1b', '--tinkery')
                assert len(copies()) == 1, 'help leaked a copy key'
                send(b'n', 'TEST newest')
                assert 'Copied.' not in flat(), 'selection kept stale copy feedback'
                send(b'p', 'TEST oldest café')
                send(b'\x1b', '4 approve')
                send(b'4', 'Seed 1 of 2')
            else:
                message = {'empty': 'No verified seeds could be listed.',
                           'incomplete': 'No verified seeds could be listed.',
                           'unreadable': 'Ready seeds unavailable.'}[case]
                wait(lambda: message in flat())
                if case == 'incomplete':
                    assert 'may be incomplete' in flat()
                    send(b'?', 'broken')
                    send(b'\x1b', message)
                os.write(master, b'c')
                send(b'\x1b', '4 approve')
                assert not copies()
            assert files(root) == before, 'Approve changed TEST files'
            assert not (root / 'TEST-provider-was-run').exists()
            print(json.dumps(dict(width=width, height=height, no_color=no_color,
                journey='approve/' + case, provider_calls=0, session_writes=0)))
        finally:
            proc.kill()
            proc.wait()
            os.close(master)
            os.close(slave)


if __name__ == '__main__':
    for size in [(80, 24), (100, 30)]:
        for no_color in [False, True]:
            for case in ['ready', 'empty', 'incomplete', 'unreadable', 'started', 'edited', 'moved', 'runs-unreadable', 'no-checkout', 'different-repo']:
                journey(*size, case, no_color)
            for case in ['target-ready','target-stale','target-missing','target-started','target-twin-checkouts']:
                journey(*size,case,no_color)

    for case in ['target-version','target-recreated','target-hash']:
        journey(80,24,case,True)
    journey(100, 30, 'retarget-checkout', True)
