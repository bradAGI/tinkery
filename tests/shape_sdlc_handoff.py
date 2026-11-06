"""Shape's real writer → pinned SDLC intake/verify, with TEST-only confirmations.

No product model calls: one stub reply. All installs, Git commits, synthetic
attestations and the reversible consumer mutation live under a short /tmp TEST root.
Locked checks protect the agreement, not the strength of the agreed checks.
"""
from terminal_process import wait_output
import base64
import fcntl
import hashlib
import importlib.util
import json
import os
import re
from pathlib import Path
import select
import shlex
import shutil
import struct
import test_launch as subprocess
import sys
import tempfile
import termios
import time

from vt_screen import Screen
from shape_answer_terminal import completion_review, seed_fields

REPO = Path(__file__).resolve().parents[1]
# Current tink-sdlc main (scaffold 1.41.0): Tinkery-owned intake (#91), human-reviewed departures (#99).
SDLC_REF = '6eee1f1d3f2035983c01b3c61b51f0afb5dccf19'
assert os.environ.get('TINKERY_TEST_SDLC_REF', SDLC_REF) == SDLC_REF
import session_fixture as helper
CONFIRMATION = 'TEST confirmation of r1 only; not operator approval or permission to build.'
# Original copied verbatim from /tmp/tinkery-TEST-stacker-KZcCiq/sessions/
# 37bc32a6-e13d-4558-ab44-6d00fabbd54a/seed-contract.md; never edit that evidence.
ORIGINAL = (REPO / 'tests/fixtures/stacker-r1-seed-contract.md').read_text()
GOAL = ORIGINAL.split('- goal [user] ', 1)[1].split(' (source:', 1)[0]


def command(argv, cwd, env, expected=0):
    result = subprocess.run([str(a) for a in argv], cwd=cwd, env=subprocess.environment(env, home=(env)['HOME']),
                            capture_output=True, text=True, timeout=120)
    if expected is not None:
        assert result.returncode == expected, (argv, result.stdout, result.stderr)
    return result


def shape(root, env):
    """Draft and confirm through F2, never by writing a seed fixture."""
    sessions = root / 'TEST-sessions'
    session = helper.create(sessions)
    old = helper.load(session)
    state = helper.editable(old)
    state.update(goal=GOAL, origin='goal', current_question=None, nodes=[{
        'id': 'goal', 'kind': 'decision', 'prerequisites': [], 'evidence': [],
        'owner': 'TEST fixture', 'gate': 'TEST only', 'status': 'settled',
        'answer': GOAL, 'authority': 'user', 'authority_source': 'TEST fixture, not operator approval'}])
    for identifier, answer in [('stacker-fairness', 'skill'), ('stacker-checkpoint', 'aim for the top')]:
        state['nodes'].append({'id': identifier, 'kind': 'decision', 'prerequisites': [], 'evidence': [],
            'owner': 'TEST fixture', 'gate': 'TEST only', 'status': 'settled', 'answer': answer,
            'authority': 'user', 'authority_source': 'TEST replay of recorded Stacker decision, not new approval'})
    # Synthetic transport rows, not additional Stacker decisions or operator approval.
    state['nodes'].extend([
        {'id': 'test-deferred', 'kind': 'decision', 'prerequisites': [], 'evidence': [],
         'owner': 'TEST fixture', 'gate': 'TEST only', 'status': 'deferred',
         'defer_reason': 'TEST wait for scope', 'revisit_condition': 'TEST before implementation'},
        {'id': 'test-delegated', 'kind': 'decision', 'prerequisites': [],
         'evidence': ['TEST synthetic grounding'], 'owner': 'TEST fixture', 'gate': 'TEST only',
         'status': 'settled', 'answer': 'TEST suggestion', 'authority': 'delegated',
         'authority_source': "TEST fixture replay of 'Delegate to the suggestion', not operator approval"},
    ])
    reply = json.loads((REPO / 'tests/fixtures/stacker-r1-reply.json').read_text())
    reply['confirming'] = seed_fields(reply['confirming'])
    reply['review'] = completion_review(reply['checks'])
    # Historical receipts remain untyped; never guess claim types from mixed captured prose.
    legacy = [{'checked': f['checked'], 'observed': f['observed'], 'at': '2026-10-18T00:00:00+00:00',
               'check': 'TEST replay of recorded untyped receipts'} for f in reply['found']]
    state['nodes'][1]['evidence'] = legacy
    helper.publish(session, state, old['version'], 'TEST fixture only')
    (root / 'TEST-read.md').write_text('TEST synthetic observation.\n')
    reply['found'] = [
        {'id': 'test-read', 'claim': {'type': 'observation', 'scope': 'TEST fixture line'},
         'checked': 'TEST-read.md:1', 'observed': 'TEST synthetic observation.'},
        {'id': 'test-inferred', 'claim': {'type': 'inference', 'scope': 'TEST transport only',
          'limits': 'This establishes no Stacker mechanics or real investigation quality.'},
         'checked': 'TEST-read.md:1', 'observed': 'TEST bounded inference for intake transport.',
         'supported_by': ['test-read']},
    ]
    (root / 'TEST-reply.json').write_text(json.dumps(reply))
    host = root / 'TEST-pi'
    host.write_text(f'#!{sys.executable}\n'
                    'import json, os, pathlib, sys\n'
                    'request = json.load(sys.stdin)\n'
                    'assert "ledger" in request\n'
                    'root = pathlib.Path(os.environ["TINKERY_TEST_DIR"])\n'
                    'with (root / "TEST-calls").open("a") as calls: calls.write("stub\\n")\n'
                    'print((root / "TEST-reply.json").read_text())\n')
    host.chmod(0o755)
    master, slave = os.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', 30, 100, 0, 0))
    screen = Screen(100, 30)
    args = [REPO / 'target/debug/tinkery', '--home', '--shape-pi', '--model', 'test/model',
            '--shape-budget', '1', '--seed-session-root', sessions,
            '--record-root', root / 'TEST-records', '--link-root', root / 'TEST-links',
            '--pi-command', host]
    process = subprocess.Popen([str(a) for a in args], cwd=root, env=subprocess.environment(env, home=(env)['HOME']),
                               stdin=slave, stdout=slave, stderr=slave)

    def wait(text):
        deadline = time.monotonic() + 25
        for returncode in wait_output(master, process, screen.feed, deadline):
            if ' '.join(text.split()) in ' '.join(screen.text().split()) and not screen.synchronized:
                return
            if returncode is not None:
                break
        raise AssertionError(f'{text!r} not shown:\n{screen.text()}')

    def send(keys, text):
        os.write(master, keys)
        wait(text)

    try:
        wait('3  shape')
        send(b'3', 'enter open')
        send(b'\r', 'F2 look into the next question')
        # A real sample, then Seed Me's pre-mortem, come first, without a model call (#740).
        send(b'\x1bOQ', 'Paste one real example of what this will work on')
        send(b'no sample: TEST new game', 'no sample: TEST new game')
        send(b'\x1bOQ', 'Imagine this shipped and turned out wrong. What is the most likely reason?')
        send(b'TEST The sounds were missing.', 'TEST The sounds were missing.')
        send(b'\x1bOQ', 'Seed draft r1')
        wait('pgup/dn read to end')
        send(b'confirm', 'your words confirm')
        # F2 confirms only a draft read to the end (#250).
        send(b'\x1bOQ', "PgDn or Down to 'End of seed' first. Nothing was confirmed.")
        os.write(master, b'\x1b[6~' * 40)
        wait('F2 confirm seed r1')
        # #739: the recorded r1 has two commandless provisionals; each needs a TEST command first.
        for name in ('sounds', 'mechanics'):
            send(b'\x1bOQ', 'Supply a check command, or Make it a judgment')
            send(b'\x1bOP', 'Right now')
            wait('Supply a check command')
            send(screen.pick('Supply a check command'), 'your check command')
            typed = f'python3 -m unittest tests.test_TEST_stacker_{name}'
            send(b'\x1b[200~' + typed.encode() + b'\x1b[201~', typed)
            send(b'\x1bOQ', 'Check saved. Read the changed draft, then confirm. No model ran.')
            os.write(master, b'\x1b[6~' * 40)
            wait('F2 confirm seed r1')
        send(b'\x1bOQ', 'Seed ready: you confirmed r1, and it still matches.')
        status = helper.status(session)
        assert status['handoff']['verified'] is True
        for key in ('viewer', 'snapshot', 'snapshot_current'):
            assert key not in status
        before = {p.name: p.read_bytes() for p in session.iterdir() if p.is_file()}
        binary = REPO / 'target/debug/tinkery'
        assert json.loads(command([binary, '--operator', 'agent', 'seed', 'status', '--json', session], root, env).stdout) == status
        assert json.loads(command([binary, '--operator', 'agent', 'seed', 'checks', session], root, env).stdout) == helper.read_checks(session)
        assert {p.name: p.read_bytes() for p in session.iterdir() if p.is_file()} == before
        assert json.loads((session / 'seed-confirmation.json').read_text())['source'] == 'confirm'
        ledger = helper.load(session)
        assert ledger['goal'] == GOAL
        seed_text = (session / 'seed-contract.md').read_text()
        assert helper.decisions_block(ledger) in seed_text
        assert 'test-deferred deferred: TEST wait for scope (revisit if: TEST before implementation)' in seed_text
        assert 'test-delegated [delegated] TEST suggestion' in seed_text
        facts = [n for n in ledger['nodes'] if n['kind'] == 'fact']
        assert len(facts) == len(reply['found']) == 2, facts
        assert [n['claim']['type'] for n in facts] == ['observation', 'inference'], facts
        assert all(n['authority'] == 'evidence' for n in facts)
        assert next(n for n in ledger['nodes'] if n['id'] == 'stacker-fairness')['evidence'] == legacy
        evidence = (session / 'seed-contract.md').read_text().split('## Evidence and uncertainty\n', 1)[1]
        assert 'observation; scope:' in evidence and 'inference; scope:' in evidence, evidence
        assert 'limits: This establishes no Stacker mechanics' in evidence, evidence
        assert (root / 'TEST-calls').read_text().splitlines() == ['stub']
        print('Shape TEST confirmation: 100x30 PTY, real seed writer, 2 TEST-only typed facts, '
              '13 legacy receipts unchanged, 1 stub call, 0 product model calls')
        return session
    finally:
        if process.poll() is None:
            process.kill()
        process.wait()
        os.close(master)
        os.close(slave)


def approve_watch(root, env, session, project):
    """Copy real commands, execute outside Tinkery, then read both places again."""
    master, slave = os.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', 30, 100, 0, 0))
    screen, output = Screen(100, 30), bytearray()
    args = [REPO / 'target/debug/tinkery', '--home', '--operator', 'agent',
            '--seed-session-root', session.parent,
            '--record-root', root / 'TEST-records', '--link-root', root / 'TEST-links',
            '--sdlc-root', project]
    process = subprocess.Popen([str(a) for a in args], cwd=root, env=subprocess.environment(env, home=(env)['HOME']),
                               stdin=slave, stdout=slave, stderr=slave)
    originals = {p: p.read_bytes() for p in session.iterdir() if p.is_file()}

    def wait(text):
        deadline = time.monotonic() + 25
        def consume_wait_output(chunk):
            data = chunk
            screen.feed(chunk)
            output.extend(chunk)
        for returncode in wait_output(master, process, consume_wait_output, deadline):
            if ' '.join(text.split()) in ' '.join(screen.text().split()) and not screen.synchronized:
                return
            assert returncode is None, screen.text()
        raise AssertionError(f'{text!r} not shown:\n{screen.text()}')

    def send(keys, text):
        os.write(master, keys)
        wait(text)

    try:
        wait('4  approve')
        send(b'4', 'Seed 1 of 1')
        wait(GOAL)
        send(b'c', 'Copied.')
        copies = re.findall(rb'\x1b\]52;[^;]*;([^\x07\x1b]*)(?:\x07|\x1b\\)', output)
        assert len(copies) == 1
        commands = base64.b64decode(copies[0]).decode().splitlines()
        assert len(commands) == 1 and '<run>' not in commands[0], commands
        words = shlex.split(commands[0])
        copied_slug = words[6]
        title = (session / 'seed-contract.md').read_text().split('# Seed: ', 1)[1].splitlines()[0]
        expected = re.sub('[^a-z0-9]+', '-', title.lower()).strip('-')
        assert len(copied_slug) <= 40 and (expected + '-').startswith(copied_slug + '-'), words
        # Paste byte-for-byte from outside the checkout: no slug replacement or split shell.
        command(['bash', '-c', commands[0]], root, env)
        runtime = project / '_system/scripts/sdlc.py'
        summary = json.loads(command([sys.executable, runtime, 'status', '--json'], project, env).stdout)
        detail = json.loads(command([sys.executable, runtime, 'status', copied_slug, '--json'], project, env).stdout)
        seed = session / 'seed-contract.md'
        for row in [summary['runs'][0], detail['run']]:
            assert row['seed_contract']['source'] == str(seed.resolve())
            assert row['seed_contract']['sha256'] == hashlib.sha256(seed.read_bytes()).hexdigest()
        send(b'\x1b', '4 approve')
        send(b'4', 'No verified seeds are waiting.')
        assert 'Seed 1 of' not in screen.text(), screen.text()
        command([sys.executable, runtime, 'new', 'test-no-seed'], project, env)
        no_seed = json.loads(command([sys.executable, runtime, 'status', 'test-no-seed', '--json'], project, env).stdout)
        assert no_seed['run']['seed_contract'] is None
        send(b'\x1b', '4 approve')
        send(b'5', 'Needs you')
        wait(GOAL)
        assert copied_slug[:30] in screen.text(), screen.text()
        assert 'Goal unknown' not in screen.text() and 'No confirmed seed' not in screen.text(), screen.text()
        copied_first = f'› {copied_slug[:30]}' in screen.text()
        if not copied_first:
            send(b'\x1b[B', f'› {copied_slug[:30]}')
        send(b'\r', 'Stage 3 gate:')
        wait(GOAL)
        assert 'Goal unknown' not in screen.text(), screen.text()
        send(b'\x1b', 'Needs you')
        send(b'\x1b[B' if copied_first else b'\x1b[A', '› test-no-seed')
        send(b'\r', 'No confirmed seed was recorded for this run.')
        assert 'Goal unknown' not in screen.text(), screen.text()
        send(b'?', 'What Watch does')
        send(b'\x1b[21~', '5 watch')
        assert process.poll() is None, 'F10 from Watch help quit Tinkery'
        assert {p: p.read_bytes() for p in originals} == originals
        assert (root / 'TEST-calls').read_text().splitlines() == ['stub']
        print('Shape → copied Approve new/stage → Approve hidden → Watch exact goal in list/detail: '
              'no-seed run honest in list/detail; 100x30 PTY, real Tinkery + tink-sdlc, '
              '1 stub reply, 0 product model calls', flush=True)
    finally:
        if process.poll() is None:
            process.kill()
        process.wait()
        os.close(master)
        os.close(slave)


def handoff(root, env, session):
    source = root / 'TEST-tink-sdlc'
    command(['git', 'init', '-q', source], root, env)
    command(['git', 'fetch', '--depth', '1', 'https://github.com/jon-devlapaz/tink-sdlc.git',
             SDLC_REF], source, env)
    command(['git', 'checkout', '--detach', 'FETCH_HEAD'], source, env)
    head = command(['git', 'rev-parse', 'HEAD'], source, env).stdout.strip()
    assert head == SDLC_REF, (head, SDLC_REF)
    print(f'Pinned tink-sdlc: {head}', flush=True)
    project = root / 'TEST-project'
    project.mkdir()
    command([sys.executable, source / 'scripts/init.py', project], root, env)
    command(['git', 'init', '-q', '-b', 'main'], project, env)
    check = project / 'TEST-check'
    check.write_text('#!/bin/sh\necho "TEST existing output"\ntest -f TEST-accepted\n')
    check.chmod(0o755)
    command(['git', 'add', '-A'], project, env)
    command(['git', 'commit', '-qm', 'TEST isolated baseline'], project, env)
    runtime = project / '_system/scripts/sdlc.py'
    approve_watch(root, env, session, project)

    def sdlc(*args, expected=0):
        return command([sys.executable, runtime, *args], project, env, expected)

    # Replay the recorded Stacker reply first: all three records, no invented executable checks.
    original_files = {name: (session / name).read_bytes() for name in (
        'seed-contract.md', 'ledger.json', 'seed-confirmation.json', 'seed-checks.json')}
    sdlc('new', 'stacker-r1')
    sdlc('stage', 'stacker-r1', '1', '--here', '--seed-contract', str(session / 'seed-contract.md'),
         '--tinkery', REPO / 'target/debug/tinkery')
    stacker_run = project / 'runs/stacker-r1'
    assert (stacker_run / 'seed-contract.md').read_bytes() == original_files['seed-contract.md']
    stacker_checks = json.loads((stacker_run / 'seed-checks.json').read_text())['items']
    assert [(i['kind'], i['locked']) for i in stacker_checks] == [
        ('judge', True), ('provisional', True), ('provisional', True)]
    records = json.loads(original_files['seed-checks.json'])
    assert [item['description'] for item in stacker_checks] == [record['statement'] for record in records]
    assert [{key: item[key] for key in record}
            for item, record in zip(stacker_checks, records)] == records
    assert {name: (session / name).read_bytes() for name in original_files} == original_files
    print('Real Stacker r1: judge + two provisional records imported and locked; no checks promoted')

    # Separate synthetic command records exercise verify mechanics, not Stacker quality.
    session = helper.create(root / 'TEST-command-sessions')
    old = helper.load(session)
    state = helper.editable(old)
    state.update(goal='TEST command enforcement', origin='goal', current_question=None, nodes=[{
        'id': 'goal', 'kind': 'decision', 'prerequisites': [], 'evidence': [],
        'owner': 'TEST fixture', 'gate': 'TEST only', 'status': 'settled',
        'answer': 'TEST command enforcement', 'authority': 'user', 'authority_source': 'TEST fixture only'}])
    helper.publish(session, state, old['version'], 'TEST command enforcement fixture')
    helper.set_checks(session, json.loads((REPO / 'tests/fixtures/shape-checks.json').read_text()))
    body = (REPO / 'tests/fixtures/shape-seed-reply.txt').read_text().replace(
        '## Affected users and systems', helper.checks_block(helper.read_checks(session)) + '## Affected users and systems', 1)
    body = body.replace('## Knowledge map', helper.decisions_block(helper.load(session)) + '\n## Knowledge map', 1)
    body = body.replace('## Suggested first slice', '## Evidence and uncertainty\nTEST command fixture; no investigation claimed.\n\n## Suggested first slice', 1)
    (session / 'seed-contract.md').write_text('status: draft\n# TEST commands\nrevision: r1\n\n## You are confirming\n' + body)
    helper.confirm_seed(session, 'r1', CONFIRMATION)
    sdlc('new', 'test-handoff')
    run = project / 'runs/test-handoff'
    worker = {'argv': [sys.executable, '-c', 'print("TEST worker check passes")'], 'timeout_seconds': 5}
    (project / '_system/verification.json').write_text(json.dumps({'require_tink': False, 'checks': [worker]}))
    (run / 'checklist.json').write_text(json.dumps({'schema': 1, 'items': [{
        'id': 'worker', 'description': 'TEST worker-written check', 'verify': 'TEST success', 'check': worker}]}))
    seed = session / 'seed-contract.md'
    confirmed = {name: (session / name).read_bytes()
                 for name in ('seed-contract.md', 'ledger.json', 'seed-confirmation.json', 'seed-checks.json')}
    # Stage 1 is the real intake. --here prevents worktrees; it prints a prompt, not an agent launch.
    sdlc('stage', 'test-handoff', '1', '--here', '--seed-contract', str(seed),
         '--tinkery', REPO / 'target/debug/tinkery')
    assert (run / 'seed-contract.md').read_bytes() == confirmed['seed-contract.md']
    definition_path = run / 'seed-checks.json'
    definitions = definition_path.read_bytes()
    items = json.loads(definitions)['items']
    assert [(i['id'], i['kind'], i['locked']) for i in items] == [
        ('seed-1', 'cmd', True), ('seed-2', 'provisional', True), ('seed-3', 'judge', True)]
    assert (items[0]['cmd'], items[0]['expect'], items[0]['cwd']) == (
        './TEST-check', 'TEST existing output', '.')

    def approve():
        sdlc('decide', 'test-handoff', '3', 'approved', '--reviewer', 'TEST synthetic reviewer',
             '--source', 'TEST fixture only', '--reason', 'TEST definition, not operator approval')

    def attest(item):
        sdlc('mark', 'test-handoff', item, 'passed', '--reviewer', 'TEST synthetic reviewer',
             '--source', 'TEST fixture only', '--evidence', f'TEST synthetic observation of {item}, not operator attestation')

    approve()
    for mutation in ('edit', 'drop'):
        changed = json.loads(definitions)
        if mutation == 'edit':
            changed['items'][0]['cmd'] = 'true'
        else:
            changed['items'] = []
        try:
            definition_path.write_text(json.dumps(changed))
            assert 'Stage 3: stale' in sdlc('status', 'test-handoff').stdout
            assert sdlc('verify', 'test-handoff', expected=None).returncode != 0
            # A departure from the confirmed seed is a human-only decision: an agent can't accept it.
            status = sdlc('status', 'test-handoff').stdout
            assert ('cmd changed' if mutation == 'edit' else 'seed-1 removed') in status, status
            assert sdlc('decide', 'test-handoff', '3', 'approved', '--reviewer', 'Agent TEST worker',
                        '--source', 'TEST only', '--reason', 'TEST', expected=None).returncode != 0
            assert 'Stage 3: stale' in sdlc('status', 'test-handoff').stdout
        finally:
            definition_path.write_bytes(definitions)
    print('Locked seed edit/drop: stale gate shows the departure; verify and agent reapproval refused')

    # No test fixture attestation may be mistaken for a real person's judgment.
    assert sdlc('mark', 'test-handoff', 'seed-3', 'passed', '--evidence', 'TEST worker guess',
                expected=None).returncode != 0
    accepted = project / 'TEST-accepted'
    accepted.touch()
    # Command success settles a cmd: check; its expect: is context, never an owed mark.
    # A you judge: check still needs a human mark.
    missing_judgment = sdlc('verify', 'test-handoff', expected=None)
    assert missing_judgment.returncode != 0, missing_judgment.stdout
    assert 'seed-3' in missing_judgment.stderr and 'seed-1-expect' not in missing_judgment.stderr
    attest('seed-3')
    accepted.unlink()
    receipt = run / '04-test/output/verification.json'

    def failing_seed_blocks():
        result = sdlc('verify', 'test-handoff', expected=None)
        assert result.returncode != 0, 'verify accepted a failing confirmed seed check'
        assert 'Seed check failed: seed-1' in result.stderr, result.stderr
        proof = json.loads(receipt.read_text())
        assert proof['result'] == 'failed', proof
        assert proof['seed_checks']['seed-1'] == 'failed', proof
        log = (run / '04-test/output/test-log.md').read_text()
        assert log.count('TEST worker check passes\n') == 2, log
        assert '# checklist item worker' in log and '# seed check seed-1' in log, log
        print('Settled seed FAIL blocks verify; every worker-written check PASSES')

    failing_seed_blocks()
    # Revert probe: drop imported seed enforcement in this TEST install only.
    # The exact same behavioral assertion must go red, then pass after restoration.
    original = runtime.read_bytes()
    needle = b'def seed_check_items(path):\n'
    assert original.count(needle) == 1
    try:
        runtime.write_bytes(original.replace(needle, needle + b'    return []  # TEST mutation: drop seed checks\n'))
        try:
            failing_seed_blocks()
        except AssertionError as error:
            assert str(error) == 'verify accepted a failing confirmed seed check', str(error)
            print(f'Revert probe RED: {error}')
        else:
            raise AssertionError('Dropping seed enforcement did not turn the regression red')
    finally:
        runtime.write_bytes(original)
    assert runtime.read_bytes() == original
    failing_seed_blocks()
    print('Revert probe RESTORED: failing seed blocks again')
    accepted.touch()
    result = sdlc('verify', 'test-handoff')
    assert 'Seed checks (operator-confirmed)' in result.stdout and 'Worker-written checks:' in result.stdout
    proof = json.loads(receipt.read_text())
    assert proof['result'] == 'passed', proof
    assert proof['checklist_items'] == {'worker': 'proven'}, proof
    assert proof['seed_checks'] == {'seed-1': 'proven', 'seed-2': 'pending', 'seed-3': 'attested'}, proof
    assert 'Verification: current' in sdlc('status', 'test-handoff').stdout
    assert {name: (session / name).read_bytes() for name in confirmed} == confirmed
    print('Settled seed PASS completes verify; provisional remains pending; manual judgments are TEST attestations')


def main():
    with tempfile.TemporaryDirectory(prefix='t194-TEST-', dir='/tmp') as directory:
        root = Path(directory).resolve()
        home, tools, tmp = (root / name for name in ('TEST-home', 'TEST-bin', 'tmp'))
        for folder in (home, tools, tmp):
            folder.mkdir()
        # Allow only known tools: no operator tink/tink-route, provider or launcher can leak in.
        for name in ('git', 'bash', 'python3'):
            (tools / name).symlink_to(shutil.which(name))
        env = {'PATH': f'{tools}:/usr/bin:/bin', 'HOME': str(home), 'TMPDIR': str(tmp),
               'TERM': 'xterm-256color', 'TINKERY_NODE': '/tinkery-TEST-no-node',
               'TINKERY_TEST_DIR': str(root), 'GIT_CONFIG_GLOBAL': '/dev/null', 'GIT_CONFIG_NOSYSTEM': '1',
               'GIT_TERMINAL_PROMPT': '0', 'GIT_AUTHOR_NAME': 'TEST fixture',
               'GIT_AUTHOR_EMAIL': 'test@example.invalid', 'GIT_COMMITTER_NAME': 'TEST fixture',
               'GIT_COMMITTER_EMAIL': 'test@example.invalid', 'PYTHONDONTWRITEBYTECODE': '1'}
        handoff(root, env, shape(root, env))


if __name__ == '__main__':
    main()
