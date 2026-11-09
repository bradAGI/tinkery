"""One relay door for agent F2, launch and resume; stub admissions and durable attribution."""
import hashlib
import json
import os
from pathlib import Path
import sys
import tempfile

import test_launch as subprocess

REPO = Path(__file__).resolve().parents[1]
ENV = subprocess.environment()
if not os.environ.get('TINKERY_TEST_SEED_FIXTURE'):
    subprocess.run(['cargo', '+stable', 'build', '--locked', '--bin', 'tinkery',
                    '--example', 'seed_fixture'], cwd=REPO, env=ENV, check=True)
    os.environ['TINKERY_TEST_SEED_FIXTURE'] = str(REPO / 'target/debug/examples/seed_fixture')

from worker_terminal import (CAP, END, F2, F10, MODEL, Fixture, Terminal,
                             open_review, real_runtime_journey)

WORDS = 'TEST operator words:  budget OK: 200\nStart only this reviewed builder.  '


def authorization(fixture, case):
    if case == 'absent':
        return None
    path = fixture.root / 'TEST-operator-relay.json'
    if case == 'unreadable':
        return path
    if case == 'fifo':
        os.mkfifo(path)
        return path
    if case == 'directory':
        path.mkdir()
        return path
    value = dict(words=WORDS, seed_sha256=hashlib.sha256(
        (fixture.session / 'seed-contract.md').read_bytes()).hexdigest(), model=MODEL, responses=CAP)
    changes = {'seed': ('seed_sha256', '0' * 64), 'model': ('model', 'test/other:off'),
               'cap': ('responses', CAP + 1), 'empty': ('words', '  \n'),
               'type': ('responses', str(CAP)), 'unknown': ('budget', 200),
               'oversized': ('words', 'TEST' * 5000)}
    if case in changes:
        key, value[key] = changes[case]
    path.write_text('not JSON' if case == 'malformed' else json.dumps(value))
    return path


def journey(case, operator='agent', size=(100, 30), mono=True):
    with tempfile.TemporaryDirectory(prefix='tinkery-TEST-worker-relay-') as directory:
        fixture = Fixture(Path(directory).resolve())
        path = authorization(fixture, case)
        originals = {name: (fixture.session / name).read_bytes()
                     for name in ('seed-contract.md', 'ledger.json', 'seed-confirmation.json')}
        expected_relay = json.loads(path.read_text()) if case in ('matching', 'snapshot') else None
        terminal = Terminal(fixture, *size, operator=operator, authorization=path, mono=mono)
        try:
            open_review(terminal)
            refused = operator == 'agent' and case not in ('matching', 'snapshot')
            if not refused and operator == 'agent':
                assert 'Authority: operator, relayed' in terminal.flat(), terminal.screen.text()
                assert 'Operator declaration: agent' in terminal.flat()
                assert 'Provenance cannot be verified.' in terminal.flat()
                assert 'TEST operator words:  budget OK: 200' in terminal.screen.text()
                assert 'Start only this reviewed builder.' in terminal.flat()
                assert 'separate yeses' in terminal.flat()
            if case == 'snapshot':
                path.write_text(json.dumps(dict(words='TEST replaced after launch', model='test/other:off',
                                               responses=CAP + 1, seed_sha256='0' * 64)))
            terminal.send(F10, '1 think')
            terminal.send(b'b', 'F2 start worker')
            terminal.send(END, 'esc back')
            assert not fixture.mutations(), 'navigation or confirmed seed created work'
            os.write(terminal.master, F2)
            terminal.wait(lambda: 'Request refused or uncertain:' in terminal.flat()
                          or 'Worker result unavailable' in terminal.flat()
                          or 'Worker: turn ended;' in terminal.flat())
            if refused:
                assert not fixture.mutations(), 'unauthorized admission: agent F2 created work'
                assert not fixture.admissions(), 'unauthorized admission: worker launched'
                assert not (fixture.project / 'runs').exists()
                terminal.send(END, 'F2 tries again')
            else:
                assert len(fixture.admissions()) == 1, fixture.calls()
                build = Path(fixture.read()['run']['checkout'])
                record = build / 'runs/test-worker/worker-authorization.jsonl'
                if operator == 'agent':
                    lines = record.read_text().splitlines()
                    assert len(lines) == 1
                    receipt = json.loads(lines[0])
                    assert receipt['action'] == 'start'
                    assert receipt['operator'] == 'agent'
                    assert receipt['authority'] == 'operator, relayed'
                    assert 'cannot be verified' in receipt['provenance']
                    assert receipt['authorization'] == expected_relay
                    assert receipt['authorization']['words'] == WORDS
                    assert receipt['authorization']['responses'] == CAP, 'words inferred a budget'
                    assert receipt['run_id'] == fixture.read()['run']['run_id']
                    assert receipt['step'] == 'build' and receipt['checkout'] == str(build)
                else:
                    assert not record.exists(), 'human or undeclared F2 became relayed consent'
                terminal.send(END + F2 * 2, 'v verify')
                assert len(fixture.admissions()) == 1, 'one start action admitted multiple workers'
            for name, original in originals.items():
                assert (fixture.session / name).read_bytes() == original, 'start changed goal/seed consent'
            assert not (fixture.root / 'unexpected-provider').exists()
            print(f'PASS relay {case}: operator={operator}, {size[0]}x{size[1]}, mono={mono}')
        finally:
            terminal.close()


def recording_refusal():
    with tempfile.TemporaryDirectory(prefix='tinkery-TEST-worker-relay-record-') as directory:
        fixture = Fixture(Path(directory).resolve())
        script = fixture.project / '_system/scripts/sdlc.py'
        script.write_text(script.read_text().replace(
            "        state['run']['checkout'] = str(target.resolve())",
            "        (target / 'runs' / slug / 'worker-authorization.jsonl').mkdir()\n"
            "        state['run']['checkout'] = str(target.resolve())"))
        fixture.git('add', '.')
        fixture.git('commit', '-qm', 'TEST refuse durable authorization recording')
        terminal = Terminal(fixture, 100, 30)
        try:
            open_review(terminal)
            terminal.send(F2, 'Worker result unavailable')
            terminal.send(END, 'Cannot record relayed worker authorization')
            assert not fixture.admissions()
            assert not any(call['argv'][:1] == ['step'] for call in fixture.mutations())
            print('PASS durable authorization write refusal prevents prepare and admission')
        finally:
            terminal.close()


def documentation():
    for name in ('README.md', 'docs/design/approve.md'):
        body = (REPO / name).read_text()
        for phrase in ('--worker-authorization', '--operator agent', 'seed', 'model', 'response cap',
                       'operator, relayed', 'cannot be verified', 'Human and undeclared F2',
                       'Goal confirmation, seed confirmation and worker start'):
            assert phrase in body, (name, phrase)
    design = (REPO / 'docs/design/approve.md').read_text()
    for field in ('"words"', '"seed_sha256"', '"model"', '"responses"',
                  'worker-authorization.jsonl', 'exactly match', 'verbatim', 'not create',
                  'F2, `l` and `u`', 'Records are never overwritten'):
        assert field in design, field
    print('PASS automated README and Approve relay contracts')


def existing_journey(action, case, operator='agent', agent_start=False, record_block=False):
    with tempfile.TemporaryDirectory(prefix='tinkery-TEST-worker-existing-relay-') as directory:
        fixture = Fixture(Path(directory).resolve(),
                          scenario='launch-refused' if action == 'launch' else 'normal')
        first = Terminal(fixture, 100, 30, operator='agent' if agent_start else 'human')
        try:
            open_review(first)
            first.send(F2, 'Worker result unavailable' if action == 'launch' else 'Worker: turn ended;')
        finally:
            first.close()
        build = Path(fixture.read()['run']['checkout'])
        record = build / 'runs/test-worker/worker-authorization.jsonl'
        before = record.read_bytes() if record.exists() else b''
        if record_block:
            assert not before
            record.mkdir()
        path = authorization(fixture, case)
        count = len(fixture.admissions())
        mutations = len(fixture.mutations())
        terminal = Terminal(fixture, 100, 30, operator=operator, authorization=path)
        try:
            terminal.wait(lambda: '4  approve' in terminal.screen.text())
            terminal.send(b'5', 'test-worker')
            terminal.send(b'b', 'Response cap:')
            terminal.send(END, 'esc back')
            if operator == 'agent' and case == 'matching':
                assert 'Authority: operator, relayed' in terminal.flat(), terminal.screen.text()
                assert 'Provenance cannot be verified.' in terminal.flat()
                assert 'TEST operator words:  budget OK: 200' in terminal.screen.text()
                assert 'Start only this reviewed builder.' in terminal.flat()
            refused = operator == 'agent' and (case != 'matching' or record_block)
            os.write(terminal.master, b'l' if action == 'launch' else b'u')
            terminal.wait(lambda: 'Request in progress' not in terminal.flat() and
                          ('Current worker and verification state unknown.' in terminal.flat()
                           or 'Request refused or uncertain:' in terminal.flat()
                           or (len(fixture.admissions()) > count and
                               ('Worker: turn ended;' in terminal.flat() if action == 'launch'
                                else 'Worker: running' in terminal.flat()))))
            if refused:
                assert len(fixture.admissions()) == count, 'unauthorized admission: agent existing action launched work'
                assert len(fixture.mutations()) == mutations, 'unauthorized admission: agent existing action mutated SDLC'
                if not record_block:
                    assert (record.read_bytes() if record.exists() else b'') == before
            else:
                assert len(fixture.admissions()) == count + 1, fixture.calls()
                if operator == 'agent':
                    lines = record.read_bytes().splitlines()
                    assert len(lines) == (2 if agent_start else 1)
                    assert record.read_bytes().startswith(before)
                    receipt = json.loads(lines[-1])
                    assert receipt['action'] == action
                    assert receipt['authorization'] == json.loads(path.read_text())
                    assert receipt['authority'] == 'operator, relayed'
                    assert receipt['run_id'] == fixture.read()['run']['run_id']
                    assert receipt['checkout'] == str(build) and receipt['step'] == 'build'
                else:
                    assert not record.exists()
            print(f'PASS existing {action}: relay={case}, operator={operator}, agent_start={agent_start}, record_block={record_block}')
        finally:
            terminal.close()


def reversion_probe():
    source = REPO / 'src/worker.rs'
    original = source.read_text()
    start_guard = '        request.authorization.check(Some(&request.target), &request.config)?;\n'
    begin = original.index('    gate.record(\n', original.index('fn admit('))
    end = original.index('    command(\n', begin)
    existing_guard = original[begin:end]
    retry_source = REPO / 'src/worker/authorization.rs'
    retry_original = retry_source.read_text()
    begin_retry = retry_original.index('        if target.retry.is_some() {')
    end_retry = retry_original.index('        Ok(Some(relay))', begin_retry)
    retry_guard = retry_original[begin_retry:end_retry]
    assert original.count(start_guard) == 1
    try:
        for guard, mode, assertion in (
                (start_guard, '--unauthorized-only', 'unauthorized admission: agent F2 created work'),
                (existing_guard, '--unauthorized-existing-only', 'unauthorized admission: agent existing action launched work')):
            source.write_text(original.replace(guard, ''))
            subprocess.run(['cargo', '+stable', 'build', '--locked', '--bin', 'tinkery'],
                           cwd=REPO, env=ENV, check=True)
            result = subprocess.run([sys.executable, __file__, mode], cwd=REPO,
                                    env={**ENV, 'TINKERY_TEST_SEED_FIXTURE': str(
                                        REPO / 'target/debug/examples/seed_fixture')},
                                    capture_output=True, text=True)
            print(result.stdout + result.stderr)
            assert result.returncode != 0 and assertion in result.stderr
            print(f'PASS guard reversion detected: {mode}')
        source.write_text(original)
        retry_source.write_text(retry_original.replace(retry_guard, ''))
        result = subprocess.run(['python3', 'scripts/rust-tests.py',
                                 'agent_retry_with_matching_relay_refuses_before_creation'],
                                cwd=REPO, env=ENV, capture_output=True, text=True)
        print(result.stdout + result.stderr)
        assert result.returncode != 0 and 'agent_retry_with_matching_relay_refuses_before_creation ... FAILED' in result.stdout
        print('PASS retry guard reversion detected')
    finally:
        source.write_text(original)
        retry_source.write_text(retry_original)
        subprocess.run(['cargo', '+stable', 'build', '--locked', '--bin', 'tinkery'],
                       cwd=REPO, env=ENV, check=True)


if __name__ == '__main__':
    if sys.argv[1:] == ['--reversion-probe']:
        reversion_probe()
    elif sys.argv[1:] == ['--unauthorized-only']:
        journey('absent')
    elif sys.argv[1:] == ['--unauthorized-existing-only']:
        existing_journey('launch', 'absent')
    else:
        assert not sys.argv[1:], sys.argv
        documentation()
        for case in ('absent', 'seed', 'model', 'cap', 'unreadable', 'empty', 'type', 'unknown',
                     'malformed', 'oversized', 'fifo', 'directory'):
            journey(case)
        journey('matching', size=(80, 24), mono=False)
        journey('matching')
        journey('snapshot')
        for operator in ('human', None):
            journey('absent', operator=operator)
            journey('cap', operator=operator)
        recording_refusal()
        for action in ('launch', 'resume'):
            for case in ('absent', 'seed', 'model', 'cap', 'unreadable', 'matching'):
                existing_journey(action, case)
            for operator in ('human', None):
                existing_journey(action, 'absent', operator=operator)
            existing_journey(action, 'matching', agent_start=True)
            existing_journey(action, 'matching', record_block=True)
        real_runtime_journey()
