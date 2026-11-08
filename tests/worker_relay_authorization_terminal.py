"""Agent F2 relay admission, exact binding and real durable attribution. No providers."""
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
                          or 'Worker: turn ended;' in terminal.flat())
            if refused:
                assert not fixture.mutations(), 'unauthorized admission: agent F2 created work'
                assert not fixture.admissions(), 'unauthorized admission: worker launched'
                assert not (fixture.project / 'runs').exists()
                terminal.send(END, 'F2 tries again')
            else:
                assert len(fixture.admissions()) == 1, fixture.calls()
                build = Path(fixture.read()['run']['checkout'])
                record = build / 'runs/test-worker/worker-authorization.json'
                if operator == 'agent':
                    receipt = json.loads(record.read_text())
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
            "        (target / 'runs' / slug / 'worker-authorization.json').mkdir()\n"
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
                  'worker-authorization.json', 'exactly match', 'verbatim', 'not create'):
        assert field in design, field
    print('PASS automated README and Approve relay contracts')


def reversion_probe():
    source = REPO / 'src/worker.rs'
    original = source.read_text()
    guard = '        request.authorization.as_ref().map_err(Clone::clone)?;\n'
    assert original.count(guard) == 1
    try:
        source.write_text(original.replace(guard, ''))
        subprocess.run(['cargo', '+stable', 'build', '--locked', '--bin', 'tinkery'],
                       cwd=REPO, env=ENV, check=True)
        result = subprocess.run([sys.executable, __file__, '--unauthorized-only'], cwd=REPO,
                                env={**ENV, 'TINKERY_TEST_SEED_FIXTURE': str(
                                    REPO / 'target/debug/examples/seed_fixture')},
                                capture_output=True, text=True)
        print(result.stdout + result.stderr)
        assert result.returncode != 0 and 'unauthorized admission: agent F2 created work' in result.stderr
        print('PASS guard reversion detected unauthorized admission at the refusal assertion')
    finally:
        source.write_text(original)
        subprocess.run(['cargo', '+stable', 'build', '--locked', '--bin', 'tinkery'],
                       cwd=REPO, env=ENV, check=True)


if __name__ == '__main__':
    if sys.argv[1:] == ['--reversion-probe']:
        reversion_probe()
    elif sys.argv[1:] == ['--unauthorized-only']:
        journey('absent')
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
        real_runtime_journey()
