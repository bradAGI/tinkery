"""Public failed-Verify journeys and exact, restored source reversion proofs."""
import argparse
import os
from pathlib import Path
import tempfile

import test_launch as subprocess
from worker_terminal import CAP, END, F2, HOME, MODEL, REPO, FAKE, Fixture, Terminal, open_review

ENDED = 'Worker: turn ended; completion still unrecorded'
UNKNOWN = 'Current worker and verification state unknown.'
BLOCKER = ('Checklist incomplete: seed-1, seed-4 (mark them with: '
           'sdlc.py mark test-worker <id> passed --evidence "..." '
           "--reviewer 'REVIEWER' --source 'SOURCE')")


def fixture_at(root):
    fixture = Fixture(root)
    script = FAKE.replace(
        "if args[0] == 'status':\n",
        "if args[0] == 'status':\n"
        "    if state.pop('fail_read', False):\n"
        "        save()\n"
        "        refuse('TEST status reread failed')\n")
    old = "    assert args == ['verify', state['run']['slug']]\n"
    new = (old + "    state['run']['verification_status'] = 'failed'\n"
           "    save()\n" + f"    refuse({BLOCKER!r})\n")
    assert script.count(old) == 1
    (fixture.project / '_system/scripts/sdlc.py').write_text(script.replace(old, new))
    fixture.git('add', '.')
    fixture.git('commit', '-qm', 'TEST failed verification and one failed reread')
    return fixture


def settle(terminal, keys):
    terminal.send(keys + b'?', 'Worker actions')
    terminal.wait(lambda: 'Request in progress' not in terminal.flat())
    terminal.send(b'\x1b', 'esc back')


def failed_verify(terminal):
    settle(terminal, b'v')
    terminal.send(END, '--source')
    assert BLOCKER in terminal.flat(), terminal.screen.text()


def failed_read(terminal):
    terminal.fixture.change(lambda state: state.update(fail_read=True))
    settle(terminal, b'r')
    terminal.send(END, 'TEST status reread failed')


def retention(terminal):
    failed_verify(terminal)
    terminal.send(HOME, 'esc back')
    shown = terminal.flat()
    assert ENDED in shown, 'ended-state retention: ' + shown
    assert MODEL in shown and f'Response cap: {CAP}' in shown, shown
    assert 'turn completed' not in shown and 'f finish' not in shown, shown
    terminal.send(END, '--source')
    assert 'Checklist incomplete: seed-1, seed-4' in terminal.flat(), terminal.screen.text()
    assert 'sdlc.py mark test-worker <id> passed --evidence "..."' in terminal.flat()
    assert "--reviewer 'REVIEWER' --source 'SOURCE'" in terminal.flat()
    assert terminal.fixture.read()['step']['state'] == 'active'


def freshness(terminal):
    fixture = terminal.fixture
    for state, verification in (('active', 'current'), ('prepared', 'not-run')):
        fixture.change(lambda value: (value['step'].update(state=state),
                                     value['run'].update(verification_status=verification)))
        settle(terminal, b'r')
        failed_verify(terminal)
        for phase in ('verify', 'reread'):
            if phase == 'reread':
                failed_read(terminal)
            terminal.send(HOME, 'esc back')
            assert UNKNOWN in terminal.flat(), 'failed reread must be explicitly unknown: ' + terminal.flat()
            before = fixture.calls()
            for key in (b'v', b'f', b's', b'c', b'l', F2):
                settle(terminal, END + key)
                assert fixture.calls() == before, 'stale-action no-mutation: ' + repr(key)
    fixture.change(lambda value: value['step'].update(state='active'))
    settle(terminal, b'r')
    terminal.send(END, 'Read from tink-sdlc.')
    settle(terminal, b's')
    assert fixture.read()['step']['state'] == 'stopped', 'successful reread did not restore deliberate actions'


def no_launch(terminal):
    fixture = terminal.fixture
    before = fixture.admissions()
    assert len(before) == 1
    for phase in ('verify', 'reread'):
        if phase == 'verify':
            failed_verify(terminal)
        else:
            failed_read(terminal)
        settle(terminal, END + b'u')
        assert fixture.admissions() == before, 'no-extra-launch call-count: stale resume reached SDLC'
        assert not (fixture.root / 'unexpected-provider').exists(), 'unexpected provider call'
    settle(terminal, b'r')
    terminal.send(END, 'Read from tink-sdlc.')
    settle(terminal, END + b'u')
    assert len(fixture.admissions()) == 2, 'successful reread did not restore deliberate resume'
    assert not (fixture.root / 'unexpected-provider').exists()


CASES = {'retention': retention, 'freshness': freshness, 'no-launch': no_launch}


def journey(case, width, height):
    with tempfile.TemporaryDirectory(prefix='tinkery-TEST-verify-retention-') as directory:
        fixture = fixture_at(Path(directory).resolve())
        terminal = Terminal(fixture, width, height, mono=True)
        try:
            open_review(terminal)
            terminal.send(F2, ENDED)
            assert ENDED in terminal.flat(), terminal.screen.text()
            CASES[case](terminal)
            print(f'PASS {case}: {width}x{height}, NO_COLOR, TEST storage, zero providers', flush=True)
        finally:
            terminal.close()


def reversion():
    path = REPO / 'src/menu/worker.rs'
    original = path.read_text()
    guard = ('                    if !self.fresh && !matches!(action, Action::Refresh) {\n'
             '                        return None;\n'
             '                    }\n')
    patches = (
        ('retention', '            Err(failure) => {\n',
         '            Err(failure) => {\n                self.snapshot = None;\n',
         'ended-state retention:'),
        ('freshness', guard, '', 'stale-action no-mutation:'),
        ('no-launch', guard, '', 'no-extra-launch call-count:'),
    )
    env = subprocess.environment()
    build = ['cargo', '+stable', 'build', '--locked', '--quiet', '--bin', 'tinkery']
    try:
        for case, old, new, assertion in patches:
            assert original.count(old) == 1, f'reversion patch no longer unique: {case}'
            path.write_text(original.replace(old, new))
            subprocess.run(build, cwd=REPO, env=env, check=True)
            red = subprocess.run([os.sys.executable, __file__, '--case', case], cwd=REPO,
                                 env=env, capture_output=True, text=True)
            output = red.stdout + red.stderr
            assert red.returncode != 0 and ('AssertionError: ' + assertion) in output, output
            print(f'RED {case}: {next(line for line in output.splitlines() if line.startswith("AssertionError:"))}',
                  flush=True)
            path.write_text(original)
            subprocess.run(build, cwd=REPO, env=env, check=True)
            subprocess.run([os.sys.executable, __file__, '--case', case], cwd=REPO, env=env, check=True)
            print(f'RESTORED GREEN {case}', flush=True)
    finally:
        path.write_text(original)
        subprocess.run(build, cwd=REPO, env=env, check=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--case', choices=CASES)
    parser.add_argument('--revert', action='store_true')
    args = parser.parse_args()
    if args.revert:
        reversion()
    else:
        for case in ([args.case] if args.case else CASES):
            for width, height in ((100, 30), (80, 24)):
                journey(case, width, height)
