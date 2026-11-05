"""Watch over a fake tink-sdlc checkout in TEST roots; no real checkout, providers or writes."""
from terminal_process import wait_output
import fcntl
import hashlib
import json
import os
from pathlib import Path
import select
import struct
import test_launch as subprocess
import sys
import tempfile
import termios
import time

from vt_screen import Screen
from support.seed_file_fixture import replaced_seed
from shape_terminal import make as make_session, seed as make_seed

REPO = Path(__file__).resolve().parents[1]
FAKE = '''import json, os, sys, time
from pathlib import Path
pid = Path(os.environ['TEST_WATCH_PID'])
pid.with_suffix('.tmp').write_text(str(os.getpid()))
pid.with_suffix('.tmp').replace(pid)
with open(os.environ['TEST_WATCH_LOG'], 'a') as log:
    log.write(' '.join(sys.argv[1:]) + '\\n')
state = json.loads(Path(os.environ['TEST_WATCH_STATE']).read_text())
if state.get('slow'):
    time.sleep(30)
if len(sys.argv) == 4:
    slug = sys.argv[2]
    row = next(r for r in state['list']['runs'] if r['slug'] == slug)
    detail = state.get('details', {}).get(slug, state['detail'])
    detail['run']['slug'] = row['slug']
    detail['run']['run_id'] = row.get('run_id')
    for field, value in state.get('identity', {}).items():
        detail['run'][field] = value
    detail['run']['verification_status'] = row['verification_status']
    detail['run']['next_action'] = row['next_action']
    detail['run']['seed_contract'] = row.get('seed_contract')
    if row['verification_status'] != 'blocked':
        detail['run']['gates'][0]['status'] = 'approved'
        if slug not in state.get('details', {}):
            detail['run']['checklist'][1].update(automatic=True, proof='automated')
    print(json.dumps(detail))
else:
    print(json.dumps(state['list']))
'''


def run(slug, status):
    return {'slug': slug, 'run_id': 'TEST', 'profile': 'light', 'kind': 'feature',
            'verification_status': status, 'next_action': f'TEST next for {slug}',
            'has_lock': False, 'checklist_summary': {'total': 3, 'passed': 1}, 'seed_contract': None}


def envelope(**value):
    return {'protocol': 'tink-sdlc', 'api_version': 1, 'workspace': '/TEST', **value}


DETAIL = envelope(run={'verification_status': 'blocked', 'next_action': 'TEST review the brief',
                       'gates': [{'stage': 3, 'status': 'pending', 'blocked': False}],
                       'checklist': [{'id': 'TEST-auto', 'verify': 'TEST executable check', 'status': 'passed',
                                      'proof': 'automated', 'automatic': True, 'needs_recheck': False, 'mark': None},
                                     {'id': 'TEST-manual', 'verify': 'TEST operator observation', 'status': 'pending',
                                      'proof': 'attested', 'automatic': False, 'needs_recheck': False, 'mark': None}],
                       'errors': []})


def tree(root):
    return {str(p): p.read_bytes() for p in sorted(root.rglob('*')) if p.is_file()}


def journey(width, height, no_color, configured=True):
    with tempfile.TemporaryDirectory(prefix='tinkery-TEST-watch-') as directory:
        base = Path(directory).resolve()
        checkout = base / 'TEST-checkout'
        (checkout / '_system/scripts').mkdir(parents=True)
        (checkout / '_system/scripts/sdlc.py').write_text(FAKE)
        legacy = checkout / 'runs/TEST-older'
        legacy.mkdir(parents=True)
        (legacy / 'meta.json').write_text('{"slug":"TEST-older"}\n')
        (legacy / 'approval.json').write_text('TEST older approval bytes\n')
        state, log, pid = base / 'state.json', base / 'calls', base / 'pid'
        binary = base / 'bin'
        binary.mkdir()
        (binary / 'python3').symlink_to(sys.executable)

        def set_state(**value):
            state.write_text(json.dumps({'detail': DETAIL, **value}))

        set_state(list=envelope(runs=[run('quiet-a', 'current'), run('waits', 'blocked')]))
        before = tree(checkout)
        master, slave = os.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', height, width, 0, 0))
        screen = Screen(width, height)
        env = {**os.environ, 'HOME': str(base / 'HOME'), 'PATH': f'{binary}:/usr/bin:/bin',
               'TERM': 'xterm-256color', 'TINKERY_NODE': '/tinkery-TEST-no-node',
               'TEST_WATCH_LOG': str(log), 'TEST_WATCH_PID': str(pid), 'TEST_WATCH_STATE': str(state)}
        env.pop('NO_COLOR', None)
        if no_color:
            env['NO_COLOR'] = '1'
        args = [str(REPO / 'target/debug/tinkery'), '--home', '--operator', 'agent', '--seed-session-root', str(base / 'TEST-sessions')]
        if configured:
            args += ['--sdlc-root', str(checkout)]
        process = subprocess.Popen(args, cwd=base, env=subprocess.environment(env, home=(env)['HOME']), stdin=slave, stdout=slave, stderr=slave)

        def wait(predicate, seconds=5):
            deadline = time.monotonic() + seconds
            for returncode in wait_output(master, process, screen.feed, deadline):
                if predicate() and not screen.synchronized:
                    return
                assert returncode is None, screen.text()
            raise AssertionError((width, height, no_color, screen.text()))

        def send(key, expected):
            os.write(master, key)
            wait(lambda: expected in screen.text())

        def calls():
            return log.read_text().splitlines() if log.exists() else []

        try:
            wait(lambda: '1  think' in screen.text())
            if not configured:
                send(b'5', 'Start Tinkery with --sdlc-root PATH')
                assert not log.exists(), 'nothing runs without a named checkout'
                send(b'\x1b', '1  think')
            else:
                send(b'5', 'Needs you')
                text = screen.text()
                # The waiting run leads, though tink-sdlc listed it second.
                assert text.index('waits') < text.index('quiet-a'), text
                assert 'waiting on a decision' in text and 'TEST next for waits' in text
                assert 'Quiet' in text and 'verified · 1/2 checks passed' in text
                assert 'Goal unknown' not in text and 'No confirmed seed' not in text
                assert 'From python3 _system/scripts/sdlc.py status --json' in text
                assert calls() == ['status --json', 'status quiet-a --json', 'status waits --json']
                set_state(list=envelope(runs=[run('quiet-a', 'failed'), run('waits', 'blocked')]))
                send(b'r', 'quiet-a  verification failed')
                assert calls() == ['status --json', 'status quiet-a --json', 'status waits --json'] * 2
                send(b'\r', 'Stage 3 gate: approved')
                assert 'Checklist: 1/2 passed. How each is checked: 2 automated, 0 by you.' in ' '.join(screen.text().split())
                assert 'No confirmed seed was recorded for this run.' in screen.text()
                assert 'Goal unknown' not in screen.text()
                assert calls()[-1] == 'status quiet-a --json', calls()
                send(b'\x1b', 'Needs you')
                send(b'?', 'What Watch does')
                send(b'?', 'Needs you')

                # Four pending manual checks describe the method, never four given attestations.
                pending = json.loads(json.dumps(DETAIL))
                pending['run']['checklist'] = [
                    {**pending['run']['checklist'][1], 'id': f'TEST-manual-{i}'} for i in range(4)]
                set_state(list=envelope(runs=[run('TEST-pending', 'blocked')]),
                          details={'TEST-pending': pending})
                send(b'r', 'TEST-pending')
                send(b'\r', 'Stage 3 gate: pending')
                assert ('Checklist: 0/4 passed. How each is checked: 0 automated, 4 by you.'
                        in ' '.join(screen.text().split())), screen.text()
                assert '4 by your attestation' not in screen.text()
                send(b'\x1b', 'Needs you')

                # Summary counts cannot identify a manual observation. Detail must decide order.
                long_detail = json.loads(json.dumps(DETAIL))
                long_detail['run']['checklist'][0]['mark'] = {'evidence': 'TEST evidence ' * 500 + 'TEST-LAST-EVIDENCE'}
                long_detail['run']['errors'] = ['TEST-FIRST-ERROR ' + 'TEST explanation ' * 500 + 'TEST-LAST-ERROR']
                set_state(list=envelope(runs=[run('quiet-a', 'current'), run('observe', 'current')]),
                          details={'observe': long_detail})
                send(b'r', '1 attestations owed')
                text = screen.text()
                assert text.index('observe') < text.index('quiet-a'), text
                before_detail = len(calls())
                send(b'\r', 'TEST executable check')
                assert 'TEST-LAST-ERROR' not in screen.text()
                send(b'\x1b[F', 'TEST-LAST-ERROR')
                send(b'\x1b[5~', 'TEST explanation')
                send(b'\x1b[H', 'Stage 3 gate: approved')
                send(b'\x1b[6~', 'TEST evidence')
                assert len(calls()) == before_detail + 1, 'scrolling launched a read'
                send(b'\x1b', 'Needs you')
                # Fixed near-cap bodies plus small metadata remain readable through End.
                for slug, fill in [('TEST-near-bound-inline', 'x'), ('TEST-near-bound-lines', '\n')]:
                    accepted = json.loads(json.dumps(DETAIL))
                    accepted['run']['checklist'][0]['mark'] = {
                        'evidence': fill * 59000 + '\nTEST-ACCEPTED-TAIL'}
                    set_state(list=envelope(runs=[run(slug, 'blocked')]),
                              details={slug: accepted})
                    send(b'r', slug)
                    send(b'\r', 'TEST executable check')
                    send(b'\x1b[F', 'TEST-ACCEPTED-TAIL')
                    assert 'too long' not in screen.text(), screen.text()
                    send(b'\x1b', 'Needs you')
                # Reader-sized payloads can still exceed the renderer's scroll range.
                for large in ('60000 source bytes', 'short evidence lines', 'empty evidence lines', 'appended error labels'):
                    slug = 'TEST-big-' + large.replace(' ', '-')
                    oversized = json.loads(json.dumps(DETAIL))
                    if large == 'appended error labels':
                        oversized['run']['errors'] = [''] * 21000
                    elif large == '60000 source bytes':
                        oversized['run']['checklist'][0]['mark'] = {'evidence': 'x' * 60000}
                    else:
                        oversized['run']['checklist'][0]['mark'] = {
                            'evidence': ('x\n' if large == 'short evidence lines' else '\n') * 70000 + 'TEST-UNREACHABLE-TAIL'}
                    set_state(list=envelope(runs=[run(slug, 'blocked'), run('TEST-readable', 'current')]),
                              details={slug: oversized})
                    send(b'r', slug)
                    send(b'\r', 'Detail is too long to show completely.')
                    assert 'status <slug> --json' in screen.text(), screen.text()
                    assert 'TEST-UNREACHABLE-TAIL' not in screen.text()
                    assert 'Checklist:' not in screen.text(), 'oversized detail looked complete'
                    send(b'\x1b', 'TEST-readable')
                    send(b'\x1b[B', '› TEST-readable')
                    send(b'\r', 'No confirmed seed was recorded for this run.')
                    send(b'\x1b', 'TEST-readable')
                for status in ['blocked', 'failed', 'interrupted', 'running', 'not-run', 'stale', 'invalid']:
                    row = run('TEST-state', status)
                    if status == 'invalid':
                        row['error'] = 'TEST broken run'
                    set_state(list=envelope(runs=[row]))
                    words = {'blocked': 'waiting on a decision', 'failed': 'verification failed',
                             'interrupted': 'verification interrupted', 'running': 'running',
                             'not-run': 'not verified yet', 'stale': 'verification out of date',
                             'invalid': "can't be read"}
                    send(b'r', words[status])
                # Both attention inspection and reopening detail retain the list's identity.
                for identity in ({'slug': 'TEST-another-run'}, {'run_id': 'TEST-changed'},
                                 {'run_id': None}, {'slug': None}):
                    row = run('TEST-selected', 'current')
                    set_state(list=envelope(runs=[row]))
                    send(b'r', 'TEST-selected')
                    set_state(list=envelope(runs=[row]), identity=identity)
                    send(b'\r', 'Run identity changed or is missing.')
                    assert 'Next:' not in screen.text() and 'Checklist:' not in screen.text(), screen.text()
                    send(b'\x1b', 'TEST-selected')
                    send(b'r', "Couldn't read the runs.")
                    assert 'Run identity changed or is missing.' in screen.text(), screen.text()
                    assert 'TEST next for' not in screen.text(), screen.text()
                # A missing list ID cannot establish attention either.
                row = run('TEST-selected', 'current')
                row.pop('run_id')
                set_state(list=envelope(runs=[row]))
                send(b'r', "Couldn't read the runs.")
                assert 'identity' in screen.text(), screen.text()
                # Scaffold upgrades can reject old metadata; Watch names that gap without migration.
                older = {'slug': 'TEST-older', 'verification_status': 'invalid',
                         'error': 'Run run_id must be a canonical UUIDv4 string; create runs with sdlc.py new.',
                         'checklist_summary': {'passed': 0, 'total': 0}}
                set_state(list=envelope(runs=[older]))
                send(b'r', 'older run')
                assert "can't be read" not in screen.text() and 'UUIDv4' not in screen.text(), screen.text()
                read_count = len(calls())
                send(b'\r', 'No run files were changed.')
                assert 'This run has no canonical run ID.' in screen.text(), screen.text()
                assert 'Checklist:' not in screen.text() and 'Stage' not in screen.text(), screen.text()
                send(b'r', 'Needs you')
                assert calls()[read_count:] == ['status --json'], 'older refresh tried the unsupported detail read'
                # Only a matching source with verified confirmation lends its goal to a run.
                goal = ' TEST café  \nTEST exact goal tail  '
                session = make_session(base / 'TEST-original-sessions', 'ready', goal=goal, question=False)
                make_seed(session, confirm=True)
                seed = session / 'seed-contract.md'
                original = seed.read_bytes()
                ledger = session / 'ledger.json'
                original_ledger = ledger.read_bytes()
                receipt = session / 'seed-confirmation.json'
                original_receipt = receipt.read_bytes()
                checks = session / 'seed-checks.json'
                original_checks = checks.read_bytes()
                record = {'path': 'runs/unrelated/seed-contract.md', 'source': str(seed.resolve()),
                          'sha256': hashlib.sha256(original).hexdigest()}
                for seed_case in ('matched', 'no-seed', 'edited', 'moved', 'missing',
                                  'changed-goal', 'changed-goal-and-origin', 'missing-confirmation',
                                  'invalid-confirmation', 'missing-checks', 'invalid-checks'):
                    seed.write_bytes(original)
                    ledger.write_bytes(original_ledger)
                    receipt.write_bytes(original_receipt)
                    checks.write_bytes(original_checks)
                    if seed_case == 'edited':
                        seed.write_bytes(original + b'TEST edited\n')
                    elif seed_case == 'moved':
                        seed.rename(base / 'TEST-moved-seed.md')
                    elif seed_case == 'missing':
                        seed.unlink()
                    elif seed_case in ('changed-goal', 'changed-goal-and-origin'):
                        changed = json.loads(original_ledger)
                        changed['goal'] = 'TEST changed goal must never become the original'
                        if seed_case == 'changed-goal-and-origin':
                            changed['nodes'][0]['answer'] = changed['goal']
                        ledger.write_text(json.dumps(changed))
                    elif seed_case == 'missing-confirmation':
                        receipt.unlink()
                    elif seed_case == 'invalid-confirmation':
                        receipt.write_bytes(b'TEST invalid \xff')
                    elif seed_case == 'missing-checks':
                        checks.unlink()
                    elif seed_case == 'invalid-checks':
                        checks.write_bytes(b'TEST invalid \xff')
                    row = {**run('TEST-unrelated-slug', 'blocked'),
                           'seed_contract': None if seed_case == 'no-seed' else record,
                           'next_action': f'TEST next for {seed_case} seed case'}
                    set_state(list=envelope(runs=[row]))
                    send(b'r', row['next_action'])
                    text = screen.text()
                    assert 'TEST changed goal' not in text, text
                    unknown = "Goal unknown: the original seed changed or can't be found."
                    absent = 'No confirmed seed was recorded for this run.'
                    if seed_case == 'matched':
                        assert any(line.startswith('       TEST café') for line in text.splitlines()), text
                    else:
                        assert 'TEST café' not in text, text
                    assert 'TEST exact goal tail' not in text, text
                    assert (unknown in text) == (seed_case not in ('matched', 'no-seed')), text
                    assert absent not in text, text
                    send(b'\r', 'Stage 3 gate: pending')
                    expected = {'matched': 'TEST exact goal tail', 'no-seed': absent}.get(seed_case, unknown)
                    assert expected in screen.text(), screen.text()
                    assert 'TEST changed goal' not in screen.text(), screen.text()
                    assert ('TEST exact goal tail' in screen.text()) == (seed_case == 'matched')
                    assert not any(word in screen.text() for word in ('seed_contract', 'sha256', 'API'))
                    send(b'\x1b', 'Needs you')
                # Invalid verified-goal inputs are unknown promptly, even without a FIFO writer.
                seed.write_bytes(original)
                ledger.write_bytes(original_ledger)
                receipt.write_bytes(original_receipt)
                checks.write_bytes(original_checks)
                row = {**run('TEST-replaced-seed', 'blocked'), 'seed_contract': record}
                set_state(list=envelope(runs=[row]))
                for file in (seed, ledger, receipt, checks):
                    for kind in ('fifo', 'directory'):
                        send(b'r', 'TEST café')
                        with replaced_seed(file, kind):
                            started = time.monotonic()
                            send(b'r', "Goal unknown: the original seed changed or can't be found.")
                            assert time.monotonic() - started < 2, f'non-regular {file.name} delayed Watch'
                            send(b'\r', 'Stage 3 gate: pending')
                            send(b'\x1b', 'Needs you')
                            send(b'\x1b', '1  think')
                            assert time.monotonic() - started < 2, f'non-regular {file.name} delayed Escape'
                            send(b'5', "Goal unknown: the original seed changed or can't be found.")
                            assert 'TEST café' not in screen.text()
                        send(b'r', 'TEST café')
                # Regular symlink targets retain the same exact goal and unchanged bytes.
                for file in (ledger, receipt, checks):
                    original_bytes = file.read_bytes()
                    target = base / ('TEST-target-' + file.name)
                    file.rename(target)
                    file.symlink_to(target)
                    try:
                        send(b'r', 'TEST café')
                        send(b'\r', 'TEST exact goal tail')
                        send(b'\x1b', 'Needs you')
                        assert target.read_bytes() == original_bytes
                    finally:
                        file.unlink()
                        target.rename(file)
                # A matched quiet run stays exactly one row, immediately followed by its neighbour.
                seed.write_bytes(original)
                ledger.write_bytes(original_ledger)
                receipt.write_bytes(original_receipt)
                quiet = {**run('TEST-quiet-seeded', 'current'), 'seed_contract': record}
                set_state(list=envelope(runs=[quiet, run('TEST-quiet-unseeded', 'current')]))
                send(b'r', 'TEST-quiet-unseeded')
                rows = screen.text().splitlines()
                at = next(i for i, line in enumerate(rows) if 'TEST-quiet-seeded' in line)
                assert 'TEST-quiet-unseeded' in rows[at + 1], screen.text()
                assert not any(word in screen.text() for word in ('TEST café', 'TEST exact goal tail', 'Goal unknown', 'No confirmed seed'))
                send(b'\r', 'TEST exact goal tail')
                send(b'\x1b', 'TEST-quiet-unseeded')
                send(b'\x1b[B', '› TEST-quiet-unseeded')
                send(b'\r', 'No confirmed seed was recorded for this run.')
                assert 'Goal unknown' not in screen.text(), screen.text()
                send(b'\x1b', 'TEST-quiet-unseeded')
                set_state(list={**envelope(runs=[]), 'protocol': 'other'})
                send(b'r', "Couldn't read the runs.")
                assert "isn't supported" in screen.text()
                slow_call_count = len(calls()) + 1
                set_state(slow=True, list=envelope(runs=[]))
                send(b'r', 'Reading runs')
                wait(lambda: pid.exists() and len(calls()) == slow_call_count)
                started = time.monotonic()
                send(b'\x1b', '1  think')
                assert time.monotonic() - started < 3, 'leaving waited on the read'
                deadline = time.monotonic() + 3
                while time.monotonic() < deadline:
                    try:
                        os.kill(int(pid.read_text()), 0)
                    except ProcessLookupError:
                        break
                    time.sleep(.02)
                else:
                    raise AssertionError('leaving left the read running')
            os.write(master, b'q')
            process.wait(timeout=5)
            assert process.returncode == 0
            assert tree(checkout) == before, 'Watch changed the checkout'
            assert not (base / 'TEST-sessions').exists()
            print(json.dumps(dict(width=width, height=height, no_color=no_color, configured=configured,
                                  reads=len(calls()), provider_calls=0, checkout='unchanged TEST fixture')))
        finally:
            if process.poll() is None:
                process.kill()
                process.wait()
            os.close(master)
            os.close(slave)


if __name__ == '__main__':
    for dimensions in [(100, 30), (80, 24)]:
        for no_color in (False, True):
            journey(*dimensions, no_color)
    journey(80, 24, False, configured=False)
