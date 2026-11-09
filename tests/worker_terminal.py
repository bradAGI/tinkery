"""Worker actions through the real terminal and a fake SDLC boundary; no providers.

The seed uses the actual Rust ledger. The project and build worktree are real Git
repositories under TEST storage. A deliberately fake SDLC records external calls
and supplies controlled worker observations; it does not establish Pi quality or
the real runtime's approval enforcement.
"""
from terminal_process import wait_output
import base64
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import struct
import sys
import tempfile
import termios
import time

import test_launch as subprocess
import session_fixture as ledger
from shape_terminal import make, seed
from vt_screen import Screen

REPO = Path(__file__).resolve().parents[1]
RUN_ID = '3b398cce-3075-4e40-8bda-56b939d2a80c'
MODEL = 'test/model:off'
CAP = 7
F2 = b'\x1bOQ'
END = b'\x1b[F'
HOME = b'\x1b[H'
F10 = b'\x1b[21~'

FAKE = r'''import hashlib, json, os, shutil, subprocess, sys, time
from pathlib import Path
args = sys.argv[1:]
root = Path(__file__).resolve().parents[2]
statefile = Path(os.environ['TEST_WORKER_STATE'])
state = json.loads(statefile.read_text())
with open(os.environ['TEST_WORKER_CALLS'], 'a') as log:
    log.write(json.dumps({'argv': args, 'cwd': str(Path.cwd()), 'root': str(root)}) + '\n')

def save():
    temp = statefile.with_suffix('.tmp')
    temp.write_text(json.dumps(state))
    temp.replace(statefile)

def refuse(message):
    print(message, file=sys.stderr)
    sys.exit(1)

def envelope(**values):
    return {'protocol': 'tink-sdlc', 'api_version': 1, 'workspace': str(root), **values}

def run():
    row = dict(state['run'])
    row['steps'] = []
    if state.get('step'):
        step = state['step']
        fact = {'name': 'build', 'state': step['state']}
        if step['state'] == 'active':
            fact.update(turn=state.get('turn', 'ended'),
                        observed_turn=observed())
        if step['state'] == 'stopped':
            fact['stopped_by'] = step['stopped_by']
        row['steps'].append(fact)
    return row

def observed():
    return {'ended': 'ended: read the report, then complete the step',
            'not-ended': 'not ended: TEST worker is still running',
            'unknown': 'unknown: TEST cap interrupted the worker'}[state.get('turn', 'ended')]

def option(name):
    return args[args.index(name) + 1]

if args[0] == 'status':
    if len(args) == 2:
        rows = state.get('history', []) + ([run()] if state.get('run') else [])
        print(json.dumps(envelope(runs=rows)))
    elif prior := next((row for row in state.get('history', []) if row['slug'] == args[1]), None):
        print(json.dumps(envelope(run=prior)))
    else:
        assert args == ['status', state['run']['slug'], '--json'], args
        print(json.dumps(envelope(run=run())))
elif args[0] == 'new':
    if state.get('run'):
        refuse('TEST run already exists; no duplicate creation')
    slug = args[1]
    state['run'] = dict(slug=slug, run_id=state['id'], profile='light', kind='feature',
        meta={'profile': 'light', 'kind': 'feature'}, has_lock=False,
        verification_status='not-run', verification=None, seed_contract=None,
        gates=[{'stage': 3, 'status': 'approved', 'blocked': False}],
        checklist=[], checklist_state='empty', checklist_summary={'total': 0, 'passed': 0},
        errors=[], next_action='TEST worker has not started', decisions=[], artifacts={},
        actions={'verify': {'allowed': True, 'reason': '', 'timeout_seconds': 30}},
        pr=None, closure=None)
    folder = root / 'runs' / slug
    folder.mkdir(parents=True)
    (folder / 'brief.md').write_text('TEST brief, not real implementation permission.\n')
    (folder / 'run.json').write_text(json.dumps({'run_id': state['id']}))
    save()
    print('TEST created run')
elif args[0] == 'stage':
    slug, number = args[1:3]
    assert slug == state['run']['slug']
    if number == '1':
        source = Path(option('--seed-contract'))
        executable = option('--tinkery')
        verified = subprocess.run([executable, 'seed', 'status', '--json', str(source.parent)],
                                  capture_output=True, text=True, check=True)
        assert json.loads(verified.stdout)['handoff']['verified'] is True
        state['run']['seed_contract'] = {
            'source': str(source.resolve()), 'path': f'runs/{slug}/seed-contract.md',
            'sha256': hashlib.sha256(source.read_bytes()).hexdigest()}
        shutil.copyfile(source, root / 'runs' / slug / 'seed-contract.md')
        if state.get('scenario') == 'pending-gate':
            state['run']['gates'][0]['status'] = 'pending'
        save()
    elif number == '3':
        if state['run']['gates'][0]['status'] != 'approved':
            refuse('TEST definition approval is pending; no worker was launched')
        assert '--here' not in args, 'build must use an isolated checkout'
        target = Path(option('--worktree')) if '--worktree' in args else Path(state['build'])
        subprocess.run(['git', 'add', 'runs'], cwd=root, check=True)
        subprocess.run(['git', 'commit', '-qm', 'TEST intake records'], cwd=root, check=True)
        target.parent.mkdir(parents=True, exist_ok=True)
        subprocess.run(['git', 'worktree', 'add', '-qb', slug, str(target)], cwd=root, check=True)
        (target / 'runs' / slug / '03-build' / 'output').mkdir(parents=True, exist_ok=True)
        state['run']['checkout'] = str(target.resolve())
        save()
    else:
        refuse('Unexpected stage in TEST worker journey')
    print('TEST stage prepared; no provider')
elif args[0] == 'step':
    assert args[1:3] == [state['run']['slug'], 'build'], args
    action = args[3]
    if action == 'prepare':
        assert not state.get('step'), 'duplicate prepare'
        assert root == Path(state['run']['checkout'])
        report, launcher = Path(option('--report')), Path(option('--launcher'))
        assert report.is_absolute() and report.parent.is_dir()
        assert launcher == root / '_system/scripts/pi-launcher'
        assert launcher.is_file()
        assert option('--model') == state['model']
        assert int(option('--max-responses')) == state['cap']
        state['step'] = dict(schema=1, run_id=state['run']['run_id'], checkout=str(root),
            report=str(report), model=option('--model'), launcher=str(launcher),
            launcher_sha256=hashlib.sha256(launcher.read_bytes()).hexdigest(),
            state='prepared', session=None, prepared_ns=time.time_ns(),
            max_responses=int(option('--max-responses')))
        save()
    elif action in ('launch', 'resume'):
        assert len(args) == 4, 'pin, cap and report must not be overridden'
        if state['run']['gates'][0]['status'] != 'approved':
            refuse('TEST definition approval is pending; no worker was launched')
        expected = 'prepared' if action == 'launch' else 'active'
        assert state['step']['state'] == expected
        if state.get('scenario') == 'launch-refused' and not state.get('refused'):
            state['refused'] = True
            save()
            refuse('TEST offline model unavailable; prepared step retained')
        state['step']['state'] = 'uncertain'
        save()
        if state.get('scenario') == 'uncertain':
            refuse('TEST admission did not confirm; inspect the session before another launch')
        if state.get('hold'):
            until = time.monotonic() + 15
            while not Path(os.environ['TEST_WORKER_RELEASE']).exists():
                assert time.monotonic() < until, 'TEST release was not supplied'
                time.sleep(.01)
        state['step'].update(state='active', session='TEST-pinned-session')
        state['turn'] = 'ended' if action == 'launch' else 'not-ended'
        Path(state['step']['report']).write_text('TEST worker report; not verification.\n')
        state['run']['next_action'] = 'TEST inspect the worker report'
        save()
    elif action == 'complete':
        assert state['step']['state'] == 'active'
        if '--stopped-by' in args:
            state['step'].update(state='stopped', stopped_by=option('--stopped-by'))
        else:
            assert state['turn'] == 'ended'
            state['step']['state'] = 'completed'
        save()
    elif action != 'show':
        refuse('Unexpected step operation in TEST worker journey')
    record = dict(state['step'])
    if record['state'] == 'active':
        record['observed_turn'] = observed()
    print(json.dumps(record))
elif args[0] == 'verify':
    assert args == ['verify', state['run']['slug']]
    state['run'].update(verification_status='current',
        verification={'passed': True, 'result': 'passed', 'candidate': {
            'head': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip(),
            'tree': 'a' * 64}, 'log': 'b' * 64, 'error': None},
        next_action='TEST result ready for human inspection')
    save()
    print('TEST configured checks passed')
else:
    refuse('Unexpected operation in TEST worker journey: ' + repr(args))
'''

ADAPTER = r'''import argparse, json, os, subprocess
from pathlib import Path
p = argparse.ArgumentParser()
p.add_argument('operation')
for name in ('model', 'report', 'contract', 'session', 'for-operation', 'max-responses'):
    p.add_argument('--' + name)
args = p.parse_args()
with open(os.environ['TEST_WORKER_ADAPTER_CALLS'], 'a') as log:
    log.write(json.dumps(vars(args)) + '\n')
assert args.model == 'test/model:off'
reply = dict(protocol='tink-sdlc-launcher', api_version=1, operation=args.operation,
             model=args.model, session='TEST-real-runtime-fake-worker')
contract = json.loads(Path(args.contract).read_text())
assert contract['model'] == args.model and contract['max_responses'] == 7
assert contract['report'] == args.report
if args.operation == 'resolve':
    reply['operation'] = args.for_operation
elif args.operation in ('launch', 'resume'):
    assert args.max_responses == '7'
    if args.operation == 'resume':
        assert args.session == reply['session']
    root = Path(contract['checkout'])
    assert Path.cwd() == root
    failed = args.operation == 'launch' and os.environ.get('TEST_WORKER_FAIL_FIRST') == '1'
    (root / 'marker').write_text('TEST unfinished candidate\n' if failed else 'TEST built candidate\n')
    subprocess.run(['git', 'add', 'marker'], check=True, stdout=subprocess.DEVNULL)
    subprocess.run(['git', 'commit', '-qm', 'TEST fake worker candidate'], check=True,
                   stdout=subprocess.DEVNULL)
    Path(args.report).write_text('TEST fake worker finished; actual SDLC verification is still owed.\n')
elif args.operation == 'finished':
    assert args.session == reply['session']
    reply['finished'] = True
else:
    raise AssertionError('Unexpected fake adapter operation: ' + args.operation)
print(json.dumps(reply))
'''


class Fixture:
    def __init__(self, root, scenario='normal'):
        self.root = root
        self.project = root / "TEST project's checkout"
        scripts = self.project / '_system/scripts'
        scripts.mkdir(parents=True)
        (scripts / 'sdlc.py').write_text(FAKE)
        launcher = scripts / 'pi-launcher'
        launcher.write_text('#!/bin/sh\necho TEST unexpected direct adapter call >&2\nexit 97\n')
        launcher.chmod(0o755)
        self.sessions = root / 'TEST sessions'
        self.session = make(self.sessions, 'TEST worker', goal='TEST keep this exact goal.', question=False)
        seed(self.session, confirm=True, title='TEST worker')
        self.state = root / 'state.json'
        self.log = root / 'calls.jsonl'
        self.release = root / 'release'
        self.state.write_text(json.dumps(dict(id=RUN_ID, model=MODEL, cap=CAP, scenario=scenario,
                                             build=str(root / 'TEST-build'), run=None)))
        binpath = root / 'bin'
        binpath.mkdir()
        (binpath / 'python3').symlink_to(sys.executable)
        for name in ('pi', 'gh'):
            tool = binpath / name
            tool.write_text(f'#!{sys.executable}\nfrom pathlib import Path\n'
                            f'Path({str(root / "unexpected-provider")!r}).write_text({name!r})\n'
                            'raise SystemExit(98)\n')
            tool.chmod(0o755)
        self.env = subprocess.environment({**os.environ, 'TERM': 'xterm-256color',
            'HOME': str(root / 'HOME'), 'PATH': f'{binpath}:/usr/bin:/bin',
            'TINKERY_NODE': '/tinkery-TEST-no-node', 'TEST_WORKER_STATE': str(self.state),
            'TEST_WORKER_CALLS': str(self.log), 'TEST_WORKER_RELEASE': str(self.release)},
            home=root / 'HOME')
        self.env.pop('NO_COLOR', None)
        self.env.pop('TINK_SDLC_STEP_SESSION', None)
        self.git('init', '-q')
        self.git('config', 'user.name', 'TEST fixture')
        self.git('config', 'user.email', 'fixture@example.invalid')
        self.git('add', '.')
        self.git('commit', '-qm', 'TEST runtime boundary')

    def git(self, *args):
        return subprocess.check_output(['git', *args], cwd=self.project, env=self.env,
                                       stderr=subprocess.PIPE).decode().strip()

    def read(self):
        return json.loads(self.state.read_text())

    def change(self, change):
        state = self.read()
        change(state)
        temp = self.state.with_suffix('.tmp')
        temp.write_text(json.dumps(state))
        temp.replace(self.state)

    def calls(self):
        return [json.loads(line) for line in self.log.read_text().splitlines()] if self.log.exists() else []

    def mutations(self):
        return [call for call in self.calls()
                if call['argv'][0] != 'status' and call['argv'][:1] != ['capabilities']
                and not (call['argv'][0] == 'step' and call['argv'][3] == 'show')]

    def admissions(self):
        return [call for call in self.calls()
                if call['argv'][0] == 'step' and call['argv'][3] in ('launch', 'resume')]


class Terminal:
    def __init__(self, fixture, width, height, budget=True, mono=False,
                 operator='agent', authorization='fixture'):
        self.fixture = fixture
        self.master, self.slave = os.openpty()
        fcntl.ioctl(self.slave, termios.TIOCSWINSZ, struct.pack('HHHH', height, width, 0, 0))
        self.screen = Screen(width, height)
        self.output = bytearray()
        env = dict(fixture.env)
        if mono:
            env['NO_COLOR'] = '1'
        args = [str(REPO / 'target/debug/tinkery'), '--home',
                '--sdlc-root', str(fixture.project), '--seed-session-root', str(fixture.sessions),
                '--record-root', str(fixture.root / 'TEST-records'),
                '--link-root', str(fixture.root / 'TEST-links')]
        if operator is not None:
            args.extend(['--operator', operator])
        if authorization == 'fixture':
            authorization = fixture.root / 'TEST-relayed-authorization.json'
            authorization.write_text(json.dumps(dict(
                words='TEST operator authorizes this bounded worker start.',
                seed_sha256=hashlib.sha256((fixture.session / 'seed-contract.md').read_bytes()).hexdigest(),
                model=MODEL, responses=CAP)))
        if authorization is not None:
            args.extend(['--worker-authorization', str(authorization)])
        if budget:
            args.extend(['--worker-model', MODEL, '--worker-responses', str(CAP)])
        self.process = subprocess.Popen(args, cwd=fixture.project, env=env,
                                        stdin=self.slave, stdout=self.slave, stderr=self.slave)

    def flat(self):
        return ' '.join(self.screen.text().split())

    def wait(self, predicate, seconds=15):
        until = time.monotonic() + seconds
        def consume(chunk):
            self.output.extend(chunk)
            self.screen.feed(chunk)
        for returncode in wait_output(self.master, self.process, consume, until):
            if predicate() and not self.screen.synchronized:
                return
            if returncode is not None:
                break
        before = self.screen.text()
        os.write(self.master, END)
        for _ in wait_output(self.master, self.process, consume, time.monotonic() + 0.3):
            pass
        raise AssertionError(before + '\nEnd of screen:\n' + self.screen.text()
                             + '\nCalls:\n' + json.dumps(self.fixture.calls(), indent=2))

    def send(self, keys, text):
        if keys.startswith(END) and len(keys) > len(END):
            frames = self.output.count(b'\x1b[?2026l')
            os.write(self.master, END)
            self.wait(lambda: self.output.count(b'\x1b[?2026l') > frames)
            keys = keys[len(END):]
        os.write(self.master, keys)
        self.wait(lambda: text in self.flat())

    def copies(self):
        return [base64.b64decode(data).decode() for data in
                re.findall(rb'\x1b\]52;[^;]*;([^\x07\x1b]*)(?:\x07|\x1b\\)', self.output)]

    def wait_exit(self):
        def consume(chunk):
            self.output.extend(chunk)
            self.screen.feed(chunk)
        for returncode in wait_output(self.master, self.process, consume, time.monotonic() + 15):
            if returncode is not None:
                return returncode
        raise AssertionError('Worker exit did not finish:\n' + self.screen.text()
                             + '\nSource state:\n' + json.dumps(self.fixture.read(), indent=2))

    def close(self):
        # Release a deliberately held fake admission even if an assertion failed.
        self.fixture.release.touch()
        if self.process.poll() is None:
            self.process.kill()
        self.process.wait(timeout=5)
        os.close(self.master)
        os.close(self.slave)


def command_seed(session):
    """Use the real ledger renderer for a runnable TEST-only acceptance condition."""
    ledger.set_checks(session, [dict(statement='TEST candidate has the requested marker.',
        kind='cmd', cmd='python3 TEST-check.py', cwd='.', expect='TEST checked candidate',
        source='TEST fixture only')])
    body = ('status: draft\n# Seed: TEST worker\nrevision: r1\n'
            '## You are confirming\nTEST candidate marker.\n\n'
            '## Now  (the agent keeps these current)\nTEST draft.\n\n'
            '## Goal\nTEST candidate marker.\n\n'
            + ledger.decisions_block(ledger.load(session))
            + '\n## Constraints and exclusions\nTEST only; no real provider.\n\n'
            + ledger.checks_block(ledger.read_checks(session))
            + '## Open questions\nNone.\n\n## Knowledge map\nTEST only.\n\n'
            '## Evidence\nTEST fixture, not human acceptance.\n')
    (session / 'seed-contract.md').write_text(body)
    ledger.confirm_seed(session, 'r1', 'TEST fixture only: confirm r1')


def real_runtime_journey(fail_first=False):
    """Real intake, worktree, immutable step and verify; only the provider adapter is fake."""
    with tempfile.TemporaryDirectory(prefix='tinkery-TEST-worker-real-runtime-') as directory:
        fixture = Fixture(Path(directory).resolve())
        for folder in ('_system', '_shared', 'stages'):
            shutil.copytree(REPO / folder, fixture.project / folder, dirs_exist_ok=True,
                            ignore=shutil.ignore_patterns('__pycache__', '*.pyc'))
        adapter = fixture.project / '_system/scripts/pi-launcher'
        adapter.write_text(f'#!{sys.executable}\n' + ADAPTER)
        adapter.chmod(0o755)
        (fixture.project / '_system/verification.json').write_text(json.dumps(dict(
            require_tink=False, checks=[dict(argv=['python3', 'TEST-check.py'], timeout_seconds=10)])))
        (fixture.project / 'marker').write_text('TEST baseline\n')
        (fixture.project / 'TEST-check.py').write_text(
            "from pathlib import Path\nassert Path('marker').read_text() == 'TEST built candidate\\n'\n"
            "print('TEST checked candidate')\n")
        command_seed(fixture.session)
        fixture.git('add', '.')
        fixture.git('commit', '-qm', 'TEST real SDLC and fake adapter')
        adapter_log = fixture.root / 'adapter-calls.jsonl'
        fixture.env['TEST_WORKER_ADAPTER_CALLS'] = str(adapter_log)
        fixture.env['TEST_WORKER_FAIL_FIRST'] = '1' if fail_first else '0'
        terminal = Terminal(fixture, 100, 30, mono=True)
        try:
            open_review(terminal)
            assert not adapter_log.exists(), 'reading or copying contacted the adapter'
            terminal.send(F2, 'Worker: turn ended; completion still unrecorded')
            assert 'Verification: not-run' in terminal.flat(), terminal.screen.text()
            # Public status supplies the created checkout and canonical run identity.
            read = subprocess.run(['python3', '_system/scripts/sdlc.py', 'status', 'test-worker', '--json'],
                cwd=fixture.project, env=fixture.env, capture_output=True, text=True, check=True)
            source = json.loads(read.stdout)['run']
            build = Path(source['checkout'])
            assert build != fixture.project and build.is_relative_to(Path(fixture.env['HOME']))
            assert source['steps'][0]['state'] == 'active' and source['steps'][0]['turn'] == 'ended'
            assert source['seed_contract']['source'] == str(fixture.session / 'seed-contract.md')
            assert (fixture.project / 'marker').read_text() == 'TEST baseline\n'
            assert (build / 'marker').read_text() == ('TEST unfinished candidate\n' if fail_first else 'TEST built candidate\n')
            contract = json.loads((build / 'runs/test-worker/steps/build.json').read_text())
            assert contract['checkout'] == str(build) and contract['run_id'] == source['run_id']
            assert contract['model'] == MODEL and contract['max_responses'] == CAP
            receipt = json.loads((build / 'runs/test-worker/worker-authorization.jsonl').read_text().splitlines()[0])
            assert receipt['operator'] == 'agent' and receipt['authority'] == 'operator, relayed'
            assert 'cannot be verified' in receipt['provenance']
            assert receipt['run_id'] == contract['run_id'] and receipt['checkout'] == str(build)
            assert receipt['authorization'] == json.loads(
                (fixture.root / 'TEST-relayed-authorization.json').read_text())
            assert receipt['authorization']['model'] == contract['model']
            assert receipt['authorization']['responses'] == contract['max_responses']
            assert 'f finish' not in terminal.flat(), 'unverified work offered an irreversible step completion'
            terminal.send(b'f?', 'Worker actions')
            assert json.loads((build / 'runs/test-worker/steps/build.json').read_text())['state'] == 'active'
            terminal.send(b'\x1b', 'Worker: turn ended; completion still unrecorded')
            if fail_first:
                terminal.send(b'v?', 'Worker actions')
                terminal.wait(lambda: 'Request in progress' not in terminal.flat())
                terminal.send(b'\x1b', 'Current worker and verification state unknown.')
                assert 'Worker: turn ended; completion still unrecorded' in terminal.flat()
                terminal.send(b'r', 'Verification: failed')
                assert 'Worker: turn ended; completion still unrecorded' in terminal.flat()
                assert 'f finish' not in terminal.flat(), 'failed checks offered normal completion'
                terminal.send(b'f?', 'Worker actions')
                assert json.loads((build / 'runs/test-worker/steps/build.json').read_text())['state'] == 'active'
                terminal.send(b'\x1b', 'Verification: failed')
                terminal.send(END + b'u', 'Request in progress')
                terminal.wait(lambda: 'Worker: turn ended; completion still unrecorded' in terminal.flat()
                              and 'Request in progress' not in terminal.flat())
                resumed = json.loads((build / 'runs/test-worker/steps/build.json').read_text())
                assert resumed['session'] == contract['session']
                assert resumed['model'] == MODEL and resumed['max_responses'] == CAP
                assert (build / 'marker').read_text() == 'TEST built candidate\n'
            terminal.send(b'v', 'Verification: current')
            terminal.send(b'f', 'Worker: turn completed; verification is separate')
            read = subprocess.run(['python3', '_system/scripts/sdlc.py', 'status', 'test-worker', '--json'],
                cwd=build, env=fixture.env, capture_output=True, text=True, check=True)
            source = json.loads(read.stdout)['run']
            assert source['verification']['passed'] is True
            assert source['steps'][0]['state'] == 'completed'
            assert source['verification']['candidate']['head'] == subprocess.check_output(
                ['git', 'rev-parse', 'HEAD'], cwd=build, env=fixture.env).decode().strip()
            calls = [json.loads(line) for line in adapter_log.read_text().splitlines()]
            assert sum(call['operation'] == 'launch' for call in calls) == 1
            assert sum(call['operation'] == 'resume' for call in calls) == int(fail_first)
            assert not (fixture.root / 'unexpected-provider').exists()
            print(f'PASS worker real runtime: 100x30; fail_first={fail_first}, real intake/worktree/step/verify, '
                  f'one fake adapter launch, {int(fail_first)} same-session resumes, no providers')
        finally:
            terminal.close()


def check_cli():
    """Malformed worker allowances are refused without creating work or contacting tools."""
    with tempfile.TemporaryDirectory(prefix='tinkery-TEST-worker-cli-') as directory:
        fixture = Fixture(Path(directory).resolve())
        binary = str(REPO / 'target/debug/tinkery')
        for flags in (['--worker-responses', '7'], ['--worker-model', MODEL],
                      ['--worker-model', MODEL, '--worker-responses', '0'],
                      ['--worker-model', MODEL, '--worker-responses', '-1']):
            result = subprocess.run([binary, '--operator', 'agent', '--home', '--snapshot',
                                     '--sdlc-root', str(fixture.project), *flags],
                                    env=fixture.env, cwd=fixture.project, capture_output=True, text=True)
            assert result.returncode != 0, (flags, result.stdout, result.stderr)
        assert not fixture.mutations()
        assert not (fixture.root / 'unexpected-provider').exists()


def open_review(terminal):
    terminal.wait(lambda: '4  approve' in terminal.screen.text())
    terminal.send(b'4', 'Seed 1 of 1')
    assert not terminal.fixture.mutations(), 'reading Approve created work'
    terminal.send(b'c', 'Copied.')
    assert len(terminal.copies()) == 1
    assert not terminal.fixture.mutations(), 'copying intake executed it'
    terminal.send(b'b', 'Review worker start')
    terminal.send(END, 'esc back')


def cap_review(case):
    """Real review keys, Pi JSON-mode events and a separate source-reported build checkout."""
    with tempfile.TemporaryDirectory(prefix='tinkery-TEST-cap-review-') as directory:
        fixture = Fixture(Path(directory).resolve())
        slug = 'earlier-build'
        checkout = fixture.root / slug
        fixture.git('worktree', 'add', '-qb', slug, str(checkout))
        (fixture.project / 'runs' / slug).mkdir(parents=True)
        steps = checkout / 'runs' / slug / 'steps'
        steps.mkdir(parents=True)
        run_id = '6173fbd4-ff71-4831-9ce8-dc7881733032'
        row = dict(slug=slug, run_id=run_id, checkout=str(checkout),
                   seed_contract=None, verification_status='not-run', steps=[])

        def record(step, responses, edit=False, cap_stop=False, op='launch', match=True):
            session = f'tsdlc-TEST-{step}'
            capped = cap_stop or case == 'stopped-on-resume'
            fact = dict(name=step, state='stopped' if capped else 'completed')
            if capped:
                fact['stopped_by'] = 'cap'
            row['steps'].append(fact)
            contract = dict(schema=1, checkout=str(checkout), run_id=run_id,
                            session=session, max_responses=responses)
            (steps / f'{step}.json').write_text(json.dumps(contract))
            events = [dict(type='session', version=3, id=session if match else 'another-session',
                           cwd=str(checkout))]
            for n in range(1, responses + 1):
                message = dict(role='assistant', content=[], stopReason='toolUse')
                events.extend([dict(type='message_start', message=message), dict(type='message_end', message=message)])
                tool = 'edit' if n == 1 else ('write' if n == responses and edit else 'read')
                result = dict(role='toolResult', toolName=tool, toolCallId=f'call-{n}', isError=n == 1)
                events.extend([dict(type='message_end', message=result),
                               dict(type='turn_end', message=message, toolResults=[result])])
            content = ''.join(json.dumps(event) + '\n' for event in events)
            if cap_stop:
                content += f'pi-launcher: response cap {responses} reached; stopped process group 123.\n'
            path = steps / f'{step}.{op}-100.pi.log'
            path.write_text(content)
            return path

        if case == 'warn':
            record('build', CAP, edit=True)
            record('finish', 5, cap_stop=True)
        elif case == 'at-minimum':
            record('build', CAP - 1, edit=True)
        elif case == 'cap-only':
            record('finish', CAP, cap_stop=True)
        elif case == 'stopped-on-resume':
            record('finish', CAP, op='launch')
            record('finish', CAP, edit=True, op='resume')
        else:
            path = record('escaped', 14, edit=True)
            outside = fixture.root / 'other.log'
            path.rename(outside)
            path.symlink_to(outside)
            record('mismatch', 13, edit=True, match=False)
            record('resumed', 12, edit=True, op='resume')
        history = [row]
        if case == 'warn':
            # Creating intakes does not change which worker steps ran most recently.
            for index in range(3):
                pending = f'waiting-{index}'
                folder = fixture.project / 'runs' / pending
                folder.mkdir()
                later = (fixture.project / 'runs' / slug).stat().st_mtime + index + 1
                os.utime(folder, (later, later))
                history.append(dict(row, slug=pending, run_id=f'6173fbd4-ff71-4831-9ce8-{index:012d}',
                                    checkout=str(fixture.project), steps=[]))
        fixture.change(lambda state: state.update(history=history))
        terminal = Terminal(fixture, 100, 30)
        try:
            open_review(terminal)
            terminal.send(HOME, 'Review worker start')
            shown = terminal.flat()
            if case == 'warn':
                assert 'Suggested minimum: 8 responses' in shown and 'Based on 2 recent Pi launch logs' in shown, shown
                assert 'earlier-build/build: first edit/write at response 7' in shown, shown
                assert '! Your cap of 7 is below this minimum' in shown, shown
                assert [c['argv'] for c in fixture.calls() if len(c['argv']) == 3] == [
                    ['status', slug, '--json']], 'only the run with fresh launches needs a detail read'
                assert not fixture.mutations()
                terminal.send(END + F2, 'Read from tink-sdlc.')
                assert len(fixture.admissions()) == 1 and fixture.read()['step']['max_responses'] == CAP
            elif case == 'at-minimum':
                assert 'Suggested minimum: 7 responses' in shown and '! Your cap' not in shown, shown
            elif case == 'cap-only':
                assert 'Suggested minimum: 9 responses' in shown and 'no edit/write through response 7' in shown, shown
                assert '! Your cap of 7 is below this minimum' in shown, shown
            else:
                assert 'Suggested minimum unknown' in shown and '! Your cap' not in shown, shown
            assert not (fixture.root / 'unexpected-provider').exists()
            print(f'PASS worker cap review {case}: 100x30')
        finally:
            terminal.close()


def journey(width, height, case, mono=False):
    with tempfile.TemporaryDirectory(prefix='tinkery-TEST-worker-terminal-') as directory:
        fixture = Fixture(Path(directory).resolve(), scenario=case)
        if case in ('success', 'quit-busy'):
            fixture.change(lambda value: value.update(hold=True))
        terminal = Terminal(fixture, width, height, budget=case != 'no-budget', mono=mono)
        try:
            open_review(terminal)
            if case == 'no-budget':
                assert 'No worker allowance.' in terminal.flat(), terminal.screen.text()
                terminal.send(F2 + b'?', 'Worker actions')
                assert not fixture.mutations(), 'default allowance allowed a worker write'
            else:
                terminal.send(HOME, 'Review worker start')
                assert MODEL in terminal.flat() and f'Response cap: {CAP}' in terminal.flat(), terminal.screen.text()
                terminal.send(END + b'\x1b[1;1:2Q' * 3 + b'?', 'Worker actions')
                terminal.send(F2 + b'urvfsc', 'Worker actions')
                assert not fixture.mutations(), 'help or repeated F2 admitted work'
                terminal.send(b'\x1b', 'F2 start worker')
                terminal.send(END, 'F2 start worker')
                if case == 'changed-seed':
                    path = fixture.session / 'seed-contract.md'
                    path.write_text(path.read_text() + '\nTEST changed after review\n')
                    terminal.send(F2, 'Request refused or uncertain:')
                    assert not fixture.mutations(), 'changed reviewed seed created work'
                elif case == 'dirty-source':
                    context = fixture.project / 'context.md'
                    context.write_text('TEST uncommitted context must not be omitted.\n')
                    terminal.send(F2, 'Request refused or uncertain:')
                    terminal.send(END, 'Commit source changes first.')
                    assert not fixture.mutations(), 'uncommitted source created a run or worker'
                    assert not (fixture.project / 'runs').exists()
                    assert context.read_text() == 'TEST uncommitted context must not be omitted.\n'
                    # #738: nothing started, so fixing the cause and pressing F2 starts it in this session.
                    terminal.wait(lambda: 'F2 tries again' in terminal.flat() and 'F2 start worker' in terminal.flat())
                    fixture.git('add', 'context.md')
                    fixture.git('commit', '-qm', 'TEST commit context')
                    terminal.send(END + F2, 'Worker: turn ended; completion still unrecorded')
                    assert len(fixture.admissions()) == 1, fixture.calls()
                elif case == 'pending-gate':
                    terminal.send(F2, 'Worker result unavailable')
                    terminal.send(END, 'TEST definition approval is pending')
                    assert not fixture.admissions(), 'a pending definition gate admitted a worker'
                    assert not any(call['argv'][:1] == ['decide'] for call in fixture.calls())
                elif case == 'uncertain':
                    terminal.send(F2, 'Worker result unavailable')
                    terminal.send(END, 'TEST admission did not confirm')
                    assert len(fixture.admissions()) == 1
                    terminal.send(b'r', 'Worker: launch uncertain')
                    terminal.send(END + F2 + b'ulf?', 'Worker actions')
                    assert len(fixture.admissions()) == 1, 'uncertain admission was retried'
                    assert not any(call['argv'][:4] == ['step', 'test-worker', 'build', 'complete']
                                   for call in fixture.calls())
                elif case == 'launch-refused':
                    terminal.send(F2, 'Worker result unavailable')
                    terminal.send(END, 'TEST offline model unavailable')
                    assert len(fixture.admissions()) == 1
                    terminal.send(b'r', 'Worker: prepared, not launched')
                    assert len(fixture.admissions()) == 1, 'refresh launched a prepared step'
                    terminal.send(END + b'?', 'Worker actions')
                    terminal.send(b'l', 'Worker actions')
                    assert len(fixture.admissions()) == 1, 'help launched a prepared step'
                    terminal.send(b'\x1b' + END + b'l', 'Worker: turn ended; completion still unrecorded')
                    assert len(fixture.admissions()) == 2
                    assert all(call['argv'] == ['step', 'test-worker', 'build', 'launch']
                               for call in fixture.admissions())
                    assert sum(call['argv'][:4] == ['step', 'test-worker', 'build', 'prepare']
                               for call in fixture.calls()) == 1
                    assert fixture.read()['step']['model'] == MODEL
                    assert fixture.read()['step']['max_responses'] == CAP
                else:
                    terminal.send(F2, 'Request in progress')
                    if case in ('success', 'quit-busy'):
                        terminal.wait(lambda: len(fixture.admissions()) == 1)
                        if case == 'quit-busy':
                            terminal.send(b'\x03', 'Finishing the current request before exit')
                            assert terminal.process.poll() is None, 'quit abandoned in-flight admission'
                            fixture.release.touch()
                            assert terminal.wait_exit() == 0
                            assert fixture.read()['step']['state'] == 'active'
                            assert len(fixture.admissions()) == 1
                            assert not (fixture.root / 'unexpected-provider').exists()
                            print(f'PASS worker quit-busy: {width}x{height}; exit waited for admitted state')
                            return
                        terminal.send(F10, '1 think')
                        terminal.send(b'b', 'Request in progress')
                        terminal.send(F2 * 3 + b'?', 'Worker actions')
                        assert len(fixture.admissions()) == 1, 'reopening admission duplicated the worker'
                        terminal.send(b'\x1b', 'Request in progress')
                        fixture.release.touch()
                    terminal.wait(lambda: 'Worker: turn ended; completion still unrecorded' in terminal.flat())
                    assert 'Verification: not-run' in terminal.flat(), terminal.screen.text()
                    calls = fixture.calls()
                    prepare = next(c['argv'] for c in calls if c['argv'][:4] == ['step', 'test-worker', 'build', 'prepare'])
                    assert prepare[prepare.index('--model') + 1] == MODEL
                    assert prepare[prepare.index('--max-responses') + 1] == str(CAP)
                    assert len(fixture.admissions()) == 1
                    assert fixture.admissions()[0]['cwd'] == fixture.read()['run']['checkout']
                    if case == 'changed-pin':
                        fixture.change(lambda value: value['step'].update(model='test/different:off'))
                        terminal.send(END + b'u', 'Current worker and verification state unknown.')
                        terminal.send(END, 'Admission refused; inspect it again')
                        assert len(fixture.admissions()) == 1, 'changed displayed pin was resumed'
                    elif case == 'changed-run':
                        fixture.change(lambda value: value['run'].update(run_id='b6683af7-ab6a-4262-8509-9e404d0a7d0b'))
                        terminal.send(END + b'u', 'Current worker and verification state unknown.')
                        terminal.send(END, 'run or its build checkout changed')
                        assert len(fixture.admissions()) == 1, 'replaced run was resumed'
                    elif case == 'cap':
                        fixture.change(lambda value: value.update(turn='unknown'))
                        terminal.send(b'r', 'Worker: turn state unknown')
                        terminal.send(b'c', 'Worker: stopped by response cap')
                        assert 'Verification: not-run' in terminal.flat()
                        assert fixture.read()['step']['state'] == 'stopped'
                        assert fixture.read()['step']['stopped_by'] == 'cap'
                        assert not any(call['argv'] == ['step', 'test-worker', 'build', 'complete']
                                       for call in fixture.calls()), 'cap stop became normal completion'
                    else:
                        terminal.send(END + b'u', 'Worker: running')
                        assert len(fixture.admissions()) == 2
                        state = fixture.read()
                        assert state['step']['session'] == 'TEST-pinned-session'
                        assert state['step']['model'] == MODEL and state['step']['max_responses'] == CAP
                        assert fixture.admissions()[1]['argv'] == ['step', 'test-worker', 'build', 'resume']
                        fixture.change(lambda value: value.update(turn='ended'))
                        terminal.send(b'r', 'Worker: turn ended; completion still unrecorded')
                        assert 'f finish' not in terminal.flat(), 'unverified work offered completion'
                        terminal.send(b'f?', 'Worker actions')
                        assert fixture.read()['step']['state'] == 'active', 'early finish stranded the resumable worker'
                        terminal.send(b'\x1b', 'Worker: turn ended; completion still unrecorded')
                        terminal.send(b'v', 'Verification: current')
                        terminal.send(b'f', 'Worker: turn completed; verification is separate')
                        assert fixture.read()['run']['verification']['passed'] is True
                        assert [c['argv'] for c in fixture.calls() if c['argv'][0] == 'verify'] == [['verify', 'test-worker']]
                        terminal.send(F10, '1 think')
                        terminal.send(b'5', 'test-worker')
                        terminal.send(b'b', 'Worker: turn completed; verification is separate')
                        assert 'Verification: current' in terminal.flat(), 'Watch recovery lost source evidence'
                        assert len(fixture.admissions()) == 2
            assert not (fixture.root / 'unexpected-provider').exists()
            print(f'PASS worker {case}: {width}x{height}, mono={mono}, explicit fake-boundary actions only')
        finally:
            terminal.close()


if __name__ == '__main__':
    check_cli()
    for case in ('warn', 'at-minimum', 'cap-only', 'stopped-on-resume', 'unusable'):
        cap_review(case)
    journey(80, 24, 'success')
    journey(100, 30, 'success', mono=True)
    for scenario in ('no-budget', 'changed-seed', 'dirty-source', 'pending-gate', 'uncertain', 'changed-run',
                     'changed-pin', 'cap', 'quit-busy', 'launch-refused'):
        journey(80, 24, scenario, mono=True)
    real_runtime_journey()
    real_runtime_journey(fail_first=True)
