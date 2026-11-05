"""Taste over synthetic API 1, explicit launch mappings and fake gh; no network or providers."""
import base64
import hashlib
import json
import os
from pathlib import Path
import re
import select
import sys
import tempfile
import time
import test_launch as subprocess
from shape_terminal import make as make_session, seed as make_seed
from terminal_process import wait_output, TerminalProcess
from support.seed_file_fixture import replaced_seed

REPO = Path(__file__).resolve().parents[1]
RUN_ID = '00000000-0000-4000-8000-000000000114'
HEAD = 'a' * 40
NEXT = 'b' * 40
URL = 'https://github.com/TEST/clinic-tycoon/pull/1'
GOAL = (' TEST café  \nWhy: Explore trade-offs.\n'
        'Done when: Allocation works; assumptions are inspectable.\nMust not: Claim clinical validation.  ')
FAKE = '''import json, os, sys, time
from pathlib import Path
if len(sys.argv) > 1 and sys.argv[1] == 'api':
    with open(os.environ['TEST_TASTE_CALLS'] + '.preview', 'a') as log:
        log.write(json.dumps(sys.argv[1:]) + '\\n')
    print('HTTP 404', file=sys.stderr)
    sys.exit(1)
pid = Path(os.environ['TEST_TASTE_PID'])
pid.with_suffix('.tmp').write_text(str(os.getpid()))
pid.with_suffix('.tmp').replace(pid)
with open(os.environ['TEST_TASTE_CALLS'], 'a') as log:
    log.write(json.dumps(sys.argv[1:]) + '\\n')
state = json.loads(Path(os.environ['TEST_TASTE_STATE']).read_text())
if state.get('slow'):
    time.sleep(30)
if '--web' in sys.argv:
    sys.exit(0)
if '--json' in sys.argv and 'pr' in sys.argv:
    if state.get('gh_fail'):
        sys.exit(1)
    print(json.dumps(state['pr']))
elif len(sys.argv) == 4:
    print(json.dumps(state['detail']))
else:
    print(json.dumps(state['list']))
'''


def envelope(**value):
    return {'protocol': 'tink-sdlc', 'api_version': 1, **value}


def fixture(record):
    state = json.loads((REPO / 'tests/fixtures/taste-api1.json').read_text())
    state['list']['runs'][0]['seed_contract'] = record
    state['detail']['run']['seed_contract'] = record
    return state


def tree(root):
    return {str(p.relative_to(root)): p.read_bytes() for p in root.rglob('*') if p.is_file()}


def confirmed_seed(base, goal=GOAL):
    """Bind the literal TEST goal to real ledger confirmation, without provider calls."""
    session = make_session(base / 'TEST-original-sessions', 'ready', goal=goal, question=False)
    make_seed(session, confirm=True)
    return session / 'seed-contract.md'


def journey(width, height, no_color, mapping_case='valid', next_case=None, empty_case=None):
    with tempfile.TemporaryDirectory(prefix='tinkery-TEST-taste-') as directory:
        base = Path(directory).resolve()
        checkout = base / 'TEST-checkout'
        scripts = checkout / '_system/scripts'
        scripts.mkdir(parents=True)
        (scripts / 'sdlc.py').write_text(FAKE)
        handoff = checkout / 'runs/TEST-result/handoff.md'
        handoff.parent.mkdir(parents=True)
        handoff.write_text(f'Human prose is not a machine interface.\nPR: {URL}\n')
        seed = confirmed_seed(base)
        original_seed = seed.read_bytes()
        record = {'source': str(seed), 'path': 'runs/TEST-result/seed-contract.md',
                  'sha256': hashlib.sha256(seed.read_bytes()).hexdigest()}
        state = fixture(record)
        if empty_case == 'no-results':
            state['list']['runs'] = []
        if next_case == 'merged':
            state['pr']['state'] = 'MERGED'
        state['detail']['run']['checklist'][0]['check']['cwd'] = './'
        elsewhere = base / 'TEST-build-elsewhere'
        if next_case == 'receipts elsewhere':
            elsewhere.mkdir()
            signed = json.loads(json.dumps(state['detail']['run']))
            signed['gates'][0]['status'] = 'approved'
            signed['checklist'][1].update(status='passed', mark={'evidence': 'TEST signed elsewhere'})
            (elsewhere / 'TEST-receipts.json').write_text(json.dumps(signed))
            state['detail']['run'].update(verification=None, verification_status='blocked', decisions=[])
            state['list']['runs'][0]['verification_status'] = 'blocked'
            state['detail']['run']['gates'][0]['status'] = 'pending'
            for item in state['detail']['run']['checklist']:
                item.update(status='pending', mark=None)
        if mapping_case == 'missing':  # not recorded, or a tink-sdlc older than 1.50.0
            state['detail']['run']['pr'] = None
        elif mapping_case == 'malformed':
            state['detail']['run']['pr']['url'] = 'TEST/clinic-tycoon#1'
        elif mapping_case == 'moved':  # GitHub answers for a different PR than the record
            state['pr']['url'] = 'https://github.com/TEST/other-repo/pull/1'
        elif mapping_case == 'case':  # owner/repo case differs only; the record still holds
            state['pr']['url'] = URL.replace('TEST/clinic-tycoon', 'test/Clinic-Tycoon')
        statefile, log, pid = base / 'state.json', base / 'calls', base / 'pid'
        statefile.write_text(json.dumps(state))
        binary = base / 'bin'
        binary.mkdir()
        (binary / 'python3').symlink_to(sys.executable)
        (binary / 'gh').write_text(f'#!{sys.executable}\n' + FAKE)
        (binary / 'gh').chmod(0o755)
        env = {**os.environ, 'HOME': str(base / 'HOME'), 'PATH': f'{binary}:/usr/bin:/bin',
               'TERM': 'xterm-256color', 'TINKERY_NODE': '/tinkery-TEST-no-node',
               'TEST_TASTE_STATE': str(statefile), 'TEST_TASTE_CALLS': str(log), 'TEST_TASTE_PID': str(pid)}
        env.pop('NO_COLOR', None)
        if no_color:
            env['NO_COLOR'] = '1'
        args = [str(REPO / 'target/debug/tinkery'), '--operator', 'agent', '--home',
                '--seed-session-root', str(base / 'TEST-sessions')]
        if empty_case != 'missing-root':
            args += ['--sdlc-root', str(checkout)]
        terminal = TerminalProcess(args, cwd=base, env=subprocess.environment(env, home=env['HOME']),
                                   width=width, height=height)
        process, master, screen, raw = terminal.process, terminal.master, terminal.screen, terminal.raw
        original = tree(checkout)
        original_elsewhere = tree(elsewhere)

        def wait(predicate, seconds=8):
            deadline = time.monotonic() + seconds
            def consume_wait_output(chunk):
                terminal.raw.extend(chunk)
                screen.feed(chunk)
            for returncode in wait_output(master, process, consume_wait_output, deadline):
                if predicate() and not screen.synchronized:
                    return
                assert returncode is None, screen.text()
            raise AssertionError((width, height, no_color, screen.text()))

        def send(key, expected):
            os.write(master, key)
            wait(lambda: expected in screen.text())

        def calls():
            return [json.loads(line) for line in log.read_text().splitlines()] if log.exists() else []

        def update():
            statefile.write_text(json.dumps(state))

        def reread():
            update()
            before = len(calls())
            os.write(master, b'r')
            wait(lambda: len(calls()) >= before + 2 and 'Your goal' in screen.text()
                 and 'Reading results' not in screen.text())

        def copied():
            payloads = re.findall(rb'\x1b\]52;[^;]*;([A-Za-z0-9+/=]+)(?:\x07|\x1b\\)', raw)
            return [base64.b64decode(p).decode() for p in payloads]

        def footer_text():
            return next(line for line in screen.text().splitlines() if '? help' in line and 'Esc back' in line)

        try:
            wait(lambda: '1  think' in screen.text())
            send(b'6', '--sdlc-root' if empty_case == 'missing-root' else 'Press r to read results')
            footer = footer_text()
            assert all(label not in footer for label in ['open result', 'run preview', 'accepted externally', 'a/c copy', 'open PR']), footer
            assert ('r read results' in footer) == (empty_case != 'missing-root'), footer
            if empty_case:
                if empty_case == 'no-results':
                    send(b'r', 'No open or merged results')
                    assert len(calls()) == 1
                before = len(calls())
                os.write(master, b'\rvlaco')
                time.sleep(.1)
                send(b'?', 'What Taste reads')
                send(b'\x1b', '? help')
                footer = footer_text()
                assert all(label not in footer for label in ['open result', 'run preview', 'accepted externally', 'a/c copy', 'open PR']), footer
                assert len(calls()) == before and not copied(), 'empty result actions launched or copied'
                # The existing hidden v key can still enter detail; F10 leaves either state.
                send(b'\x1b[21~', '1  think')
                os.write(master, b'q')
                process.wait(timeout=5)
                assert process.returncode == 0 and tree(checkout) == original and seed.read_bytes() == original_seed
                print(json.dumps(dict(journey='Taste empty controls', case=empty_case, width=width, height=height, no_color=no_color, provider_calls=0, verdicts=0)))
                return
            assert not calls(), 'opening Taste must disclose reads, not perform them'
            send(b'?', 'What Taste reads')
            assert 'Enter opens a result' not in screen.text(), screen.text()
            assert "l report a person's prior acceptance" in screen.text(), screen.text()
            assert 'a person already accepted this result elsewhere' in screen.text(), screen.text()
            assert 'unverified' in screen.text() and 'saves no verdict' in screen.text(), screen.text()
            assert 'never imply acceptance or a merge.' in screen.text(), screen.text()
            os.write(master, b'oacr')
            time.sleep(.1)
            assert not calls(), 'help leaked actions'
            send(b'\x1b', 'Press r to read results')
            send(b'r', 'TEST café')
            if mapping_case in ('valid', 'case'):
                assert '› TEST clinic allocation comparison' in screen.text(), screen.text()
            assert 'Enter open result' in screen.text() and 'a/c copy' not in screen.text()
            if mapping_case not in ('valid', 'case'):
                github = 1 if mapping_case == 'moved' else 0
                assert len(calls()) == 2 + github, calls()
                assert sum('pr' in call for call in calls()) == github, calls()
                send(b'\r', 'Is this good?')
                send(b'e', 'Checks')
                assert 'waiting: revision unknown  Allocation works' in screen.text(), screen.text()
                send(b'\x1b[F', 'Commit: unknown')
                send(b'a', 'Result identity unknown')
                assert not copied(), 'a result without a recorded PR was copied'
                send(b'\x1b', 'e details')
                send(b'\x1b', 'Enter open result')
                send(b'\x1b', '1  think')
                os.write(master, b'q')
                process.wait(timeout=5)
                assert process.returncode == 0 and tree(checkout) == original
                print(json.dumps(dict(journey='Taste mapping refusal', mapping=mapping_case,
                                      width=width, height=height, no_color=no_color, gh_calls=github)))
                return
            assert len(calls()) == 3, calls()
            assert all('--web' not in c for c in calls())
            if next_case == 'receipts elsewhere':
                assert 'You owe' not in screen.text(), screen.text()
                assert '? 2 unknown' in screen.text(), screen.text()
                send(b'\r', 'Is this good?')
                assert 'What to judge' not in screen.text() and 'receipts not found here' in screen.text(), screen.text()
                send(b'e', 'Checks')
                owed_and_checks = screen.text().split('Goal')[0]
                assert owed_and_checks.count('unknown: receipts not found here') == 3, screen.text()
                assert 'approve the plan' not in screen.text(), screen.text()
                assert 'No source gate owed' not in screen.text(), screen.text()
                for action in [b'a', b'c']:
                    before = len(copied())
                    os.write(master, action)
                    wait(lambda: len(copied()) > before)
                    payload = copied()[-1]
                    assert f'Gates owed: unknown: receipts not found here' in payload, payload
                    for label in ['Allocation works', 'confirm no clinical claims']:
                        assert f'{label}: unknown: receipts not found here' in payload, payload
                    assert GOAL in payload and HEAD in payload
                    assert 'pending (matching evidence missing)' not in payload
                assert len(calls()) == 3, calls()
                send(b'\x1b', 'e details')
                send(b'\x1b', 'Enter open result')
                send(b'\x1b', '1  think')
                os.write(master, b'q')
                process.wait(timeout=5)
                assert process.returncode == 0
                assert tree(checkout) == original and tree(elsewhere) == original_elsewhere
                print(json.dumps(dict(journey='Taste receipts elsewhere unknown', width=width,
                                      height=height, no_color=no_color, provider_calls=0, verdicts=0)))
                return
            if next_case == 'merged':
                assert ' · merged' in screen.text(), screen.text()
                assert 'Guess · Next:' not in screen.text(), 'merge is not acceptance'
                os.write(master, b't')
            send(b'\r', 'What to judge')
            assert 'confirm no clinical claims' in screen.text() and 'Is this good?' not in screen.text(), screen.text()
            assert 'Guess · Next:' not in screen.text(), 'review is not acceptance'
            assert 'TEST café' in screen.text() and 'TEST clinic allocation comparison' in screen.text(), screen.text()
            assert HEAD not in screen.text() and RUN_ID not in screen.text(), screen.text()
            send(b'e', 'Checks')
            send(b'\x1b[F', HEAD)
            if next_case == 'merged':
                assert ' · merged' in screen.text(), screen.text()
            assert HEAD in screen.text(), screen.text()
            # Reads and scrolling never launch anything; the whole goal is retained in the copy.
            read_count = len(calls())
            assert 'TEST-checked-allocation-success' not in screen.text(), 'log contents must not be dumped'
            wait(lambda: 'run API' in screen.text())
            send(b'a', 'Handoff copied; decide externally.')
            wait(lambda: bool(copied()))
            assert GOAL in copied()[-1] and HEAD in copied()[-1]
            expected_log = str(checkout / 'runs/TEST-result/04-test/output/test-log.md')
            assert expected_log in copied()[-1]
            assert 'argv: ["python3","TEST-allocation-check.py"]' in copied()[-1]
            assert f'cwd: "./" (resolved {checkout})' in copied()[-1]
            assert state['detail']['run']['verification']['log'] in copied()[-1]
            assert 'TEST-checked-allocation-success' not in copied()[-1]
            assert 'Acceptance review requested' in copied()[-1]
            assert 're-approve the plan' in copied()[-1] and 'confirm no clinical claims' in copied()[-1]
            assert 'No verdict recorded' in copied()[-1]
            count = len(copied())
            os.write(master, b'c')
            wait(lambda: len(copied()) > count)
            assert 'Correction requested' in copied()[-1] and expected_log in copied()[-1]
            assert len(calls()) == read_count
            send(b'o', 'PR sent to browser')
            assert calls()[-1] == ['pr', 'view', URL, '--web']
            if next_case:
                send(b'e', 'e details')
                assert 'Guess · Next:' not in screen.text(), 'copy/open is not acceptance'
                if next_case in ['nothing unmet', 'absent ci', 'failed']:
                    state['detail']['run']['gates'][0]['status'] = 'approved'
                    state['detail']['run']['checklist'][1].update(
                        status='passed', needs_recheck=False, mark={'evidence': 'TEST observation'})
                    if next_case == 'absent ci':
                        state['pr']['statusCheckRollup'] = []
                    if next_case == 'failed':
                        state['detail']['run']['checklist'][0]['status'] = 'failed'
                        state['detail']['run']['verification_status'] = 'failed'
                        state['detail']['run']['verification'].update(result='failed', passed=False)
                    update()
                    before = len(calls())
                    os.write(master, b'r')
                    wait(lambda: len(calls()) == before + 3 and 'Reading results' not in screen.text())
                send(b'\x1b[H', 'Your goal')
                before = len(calls())
                send(b'?', 'After external acceptance')
                os.write(master, b'lte')
                time.sleep(.1)
                os.write(master, b'\x1b')
                wait(lambda: 'What Taste reads' not in screen.text() and 'Your goal' in screen.text())
                assert 'Guess · Next:' not in screen.text(), 'help leaked acceptance'
                send(b'l', 'Guess · Next:')
                assert screen.text().count('Next:') == 1, screen.text()
                for label in ['Outcome:', 'Evidence: snapshot', 'Learning:', 'Futures:', 'Decision:']:
                    assert label not in screen.text(), screen.text()
                if next_case in ['nothing unmet', 'absent ci']:
                    assert 'Observe “TEST café”' in screen.text(), screen.text()
                elif next_case == 'failed':
                    assert 'Resolve Allocation works' in screen.text(), screen.text()
                else:
                    assert 'confirm no clinical claims' in screen.text(), screen.text()
                assert 'Human verdict only' in screen.text() and 'w write verdict' not in screen.text(), screen.text()
                send(b'?', 'Outcome: unobserved')
                assert 'Only a human can record the verdict.' in screen.text() and 'w opens' not in screen.text(), screen.text()
                expected_decision = 'proceed' if next_case in ['nothing unmet', 'absent ci'] else 'stop and repair'
                assert f'Decision: {expected_decision}' in screen.text(), screen.text()
                assert 'Next:' not in screen.text(), 'help repeats the next step'
                assert 'Evidence:' in screen.text() and 'Futures:' in screen.text()
                send(b'\x1b', 'Guess · Next:')
                assert len(calls()) == before
                send(b't', "Tinkery's guess · edit freely")
                assert 'F2 sends' in screen.text()
                assert not (base / 'TEST-sessions').exists()
                assert len(calls()) == before
                if next_case == 'correct':
                    os.write(master, b'\x01')  # Ctrl-A: replace draft, not an original
                    send(b'\x1b[200~TEST operator correction\x1b[201~', 'TEST operator correction')
                elif next_case in ['nothing unmet', 'absent ci']:
                    os.write(master, b'\x1b[1;5H')  # Ctrl-Home reveals beginning of long draft
                    wait(lambda: 'Observe “TEST café”' in screen.text())
                os.write(master, b'\x1bOQ')  # F2: practice host, zero provider calls
                wait(lambda: 'Simulated' in screen.text())
                if next_case == 'correct':
                    assert 'TEST operator correction' in screen.text()
                assert len(calls()) == before
                assert tree(checkout) == original, 'next draft changed original goal or checkout'
                assert not (base / 'TEST-sessions').exists()
                send(b'\x1b', 'Back to menu and lose this draft?')  # Esc loss guard
                send(b'y', '1  think')
                os.write(master, b'q')
                process.wait(timeout=5)
                assert process.returncode == 0
                assert not (base / 'TEST-sessions').exists()
                # The practice visit is deliberately abandoned; no durable record.
                print(json.dumps(dict(journey='Taste next dump', case=next_case,
                                      width=width, height=height, no_color=no_color,
                                      provider_calls=0, sends='F2 only', verdicts=0)))
                return
            # Among several human checks, show the first active, pending one.
            checklist = state['detail']['run']['checklist']
            automatic, human = checklist
            worker = {**human, 'id': 'TEST-worker', 'verify': 'TEST worker smoke check'}
            del worker['operator_attestation']
            checklist[:] = [automatic, worker,
                            {**human, 'id': 'TEST-passed', 'verify': 'TEST previous observation',
                             'status': 'passed', 'mark': {'evidence': 'TEST checked'}, 'needs_recheck': True},
                            {**human, 'id': 'TEST-inactive', 'verify': 'TEST provisional', 'pending': True},
                            {**human, 'id': 'TEST-unknown', 'verify': 'TEST marked unknown',
                             'status': 'unknown', 'mark': {'evidence': 'TEST uncertain'}},
                            human, {**human, 'id': 'TEST-second', 'verify': 'TEST second criterion'}]
            reread()
            assert '2 pending human checks' in screen.text() and human['verify'] in screen.text(), screen.text()
            for hidden in [worker['verify'], 'TEST previous observation', 'TEST provisional', 'TEST marked unknown',
                           'TEST second criterion', 'Is this good?']:
                assert hidden not in screen.text(), screen.text()
            send(b'e', 'TEST second criterion')
            owed, checks = screen.text().split('Checks', 1)
            assert worker['verify'] not in owed and worker['verify'] in checks, screen.text()
            send(b'e', 'What to judge')
            checklist[:] = [automatic, human]
            # Fully green open PRs still need external human review; never filter them away.
            state['detail']['run']['gates'][0]['status'] = 'approved'
            state['detail']['run']['checklist'][1].update(status='passed', mark={'evidence': 'TEST human observation'})
            update()
            before_green = len(calls())
            os.write(master, b'r')
            wait(lambda: len(calls()) == before_green + 3 and 'Is this good?' in screen.text()
                 and 'Reading results' not in screen.text())
            count = len(copied())
            os.write(master, b'a')
            wait(lambda: len(copied()) > count)
            assert 're-approve the plan' not in copied()[-1]
            assert 'Allocation works: pass' in copied()[-1] and 'No verdict recorded' in copied()[-1]
            # New commit, still-green old local receipt: the local condition must be stale.
            state = fixture(record)
            state['pr']['headRefOid'] = NEXT
            state['pr']['statusCheckRollup'] = []
            update()
            reread()
            send(b'e', '… stale  Allocation works')
            assert '✓ passed  Allocation works' not in screen.text()
            send(b'\x1b[F', NEXT)
            assert HEAD not in screen.text(), 'old revision repeated in detail'
            wait(lambda: 'run API' in screen.text())
            count = len(copied())
            os.write(master, b'c')
            wait(lambda: len(copied()) > count)
            assert NEXT in copied()[-1] and 'Allocation works: stale' in copied()[-1]
            # Matching failure and missing evidence remain separate from success.
            state['pr']['headRefOid'] = HEAD
            state['detail']['run']['verification']['result'] = 'failed'
            state['detail']['run']['verification']['passed'] = False
            state['detail']['run']['verification_status'] = 'failed'
            state['detail']['run']['checklist'][0]['status'] = 'failed'
            update()
            reread()
            send(b'e', '! failed  Allocation works')
            assert 'waiting' in screen.text()
            send(b'e', 'What to judge')
            # Unknown original refuses copying, without falling back to the old goal.
            seed.write_text('TEST changed original\n')
            send(b'r', 'Goal unknown')
            count = len(copied())
            send(b'a', 'Goal/run identity unknown')
            assert len(copied()) == count
            seed.write_bytes(original_seed)
            # The same seed bytes cannot authorize a replacement goal or missing receipt.
            ledger = seed.parent / 'ledger.json'
            original_ledger = ledger.read_bytes()
            changed = json.loads(original_ledger)
            changed['goal'] = 'TEST changed goal must never become the original'
            changed['nodes'][0]['answer'] = changed['goal']
            receipt = seed.parent / 'seed-confirmation.json'
            original_receipt = receipt.read_bytes()
            for source_case in ('changed-goal', 'missing-confirmation'):
                send(b'r', 'TEST café')
                if source_case == 'changed-goal':
                    ledger.write_text(json.dumps(changed))
                else:
                    receipt.unlink()
                send(b'r', 'Goal unknown')
                assert 'TEST changed goal' not in screen.text()
                count = len(copied())
                for action in (b'a', b'c'):
                    send(action, 'Goal/run identity unknown')
                    assert len(copied()) == count
                ledger.write_bytes(original_ledger)
                receipt.write_bytes(original_receipt)
            # A seed path replaced by a FIFO/directory becomes unknown without trapping Escape.
            for kind in ('fifo', 'directory'):
                send(b'r', 'TEST café')
                with replaced_seed(seed, kind):
                    started = time.monotonic()
                    send(b'r', 'Goal unknown')
                    send(b'\x1b', 'Enter open result')
                    assert time.monotonic() - started < 2, 'non-regular seed delayed Taste or Escape'
                    send(b'\r', 'Goal unknown')
                send(b'r', 'TEST café')
            # Handoff prose is irrelevant, including conflicting links: only launch inputs bind.
            handoff.write_text(f'{URL}\nhttps://github.com/TEST/decoy/pull/999\n')
            update()
            before_prose = len(calls())
            os.write(master, b'r')
            wait(lambda: len(calls()) == before_prose + 3 and 'What to judge' in screen.text()
                 and 'Reading results' not in screen.text())
            send(b'e', 'Checks')
            send(b'\x1b[F', HEAD)
            assert calls()[-1][2] == URL, 'handoff prose retargeted the explicit result'
            handoff.write_bytes(original['runs/TEST-result/handoff.md'])
            state['gh_fail'] = True
            update()
            reread()
            assert "! Couldn't read GitHub" in screen.text(), 'a mismatch must show on the review'
            send(b'e', 'waiting: revision unknown')
            send(b'\x1b[F', 'Commit: unknown')
            send(b'\x1b', 'e details')
            send(b'\x1b', 'Enter open result')
            assert "Couldn't read GitHub" not in screen.text(), 'source diagnostics leaked into the list'
            send(b'\r', 'Is this good?')
            send(b'e', 'Checks')
            send(b'\x1b[F', "Couldn't read GitHub")
            del state['gh_fail']
            update()
            send(b'\x1b', 'e details')
            send(b'\x1b', 'Enter open result')
            send(b'\x1b', '1  think')
            before_reopen = len(calls())
            send(b'6', 'Press r to read results')
            assert len(calls()) == before_reopen
            # Cancellation must stop an in-flight helper, with no late reopening.
            state['slow'] = True
            update()
            os.write(master, b'r')
            wait(lambda: len(calls()) > before_reopen and 'Reading results from' in screen.text())
            footer = footer_text()
            assert 'r read results' not in footer and 'open result' not in footer and 'a/c copy' not in footer, footer
            send(b'\x1b', '1  think')
            deadline = time.monotonic() + 3
            while time.monotonic() < deadline:
                try:
                    os.kill(int(pid.read_text()), 0)
                except ProcessLookupError:
                    break
                time.sleep(.02)
            else:
                raise AssertionError('Taste left its read process running')
            os.write(master, b'q')
            process.wait(timeout=5)
            assert process.returncode == 0
            assert tree(checkout) == original, 'Taste mutated the named checkout'
            assert not (base / 'TEST-sessions').exists()
            assert not any(any(x in c for x in ['verify', 'mark', 'decide', 'stage', 'merge', 'review']) for c in calls())
            print(json.dumps(dict(journey='Taste', width=width, height=height, no_color=no_color,
                                  provider_calls=0, navigation='explicit o only', verdicts=0)))
        finally:
            terminal.close()


if __name__ == '__main__':
    for dimensions in [(100, 30), (80, 24)]:
        for no_color in (False, True):
            journey(*dimensions, no_color)
    for dimensions in [(80, 24), (100, 30)]:
        for no_color in (False, True):
            for empty_case in ['missing-root', 'no-results']:
                journey(*dimensions, no_color, empty_case=empty_case)
    # Both palettes of the next line are covered in src/taste/tests.rs.
    for dimensions in [(80, 24)]:
        for no_color in (False,):
            for next_case in ['accept', 'correct', 'nothing unmet', 'merged', 'absent ci', 'failed', 'receipts elsewhere']:
                journey(*dimensions, no_color, next_case=next_case)
    for case in ['missing', 'malformed', 'moved', 'case']:
        journey(80, 24, True, case)
