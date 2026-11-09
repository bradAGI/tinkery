"""Pending judgment is inspectable, never accepted. Public PTYs and restored guard probes."""
import argparse
import json
import os
from pathlib import Path
import tempfile

import test_launch as subprocess
from taste_local_preview_terminal import journey as preview
from worker_terminal import END, F2, HOME, REPO, Fixture, Terminal, open_review
from worker_verify_retention_terminal import ENDED, settle

IDS = ['seed-2', 'seed-4']


def worker(case, width=100, height=30):
    with tempfile.TemporaryDirectory(prefix='tinkery-TEST-awaiting-') as directory:
        fixture = Fixture(Path(directory).resolve())
        terminal = Terminal(fixture, width, height, mono=True)
        try:
            open_review(terminal)
            terminal.send(F2, ENDED)
            head = fixture.git('rev-parse', 'HEAD')

            def change(state, status='awaiting-attestation'):
                state['run'].update(verification_status=status, next_action='TEST inspect the result',
                    verification={'passed': status == 'current',
                                  'result': 'passed' if status == 'current' else status,
                                  'candidate': {'head': head, 'tree': 'a' * 64},
                                  'pending_attestations': IDS},
                    checklist=[{'id': item, 'verify': 'TEST human observation ' + item,
                                'status': 'pending', 'proof': 'attested', 'automatic': False,
                                'operator_attestation': True, 'needs_recheck': False, 'mark': None}
                               for item in IDS])
            fixture.change(change)
            settle(terminal, b'r')
            if case == 'worker':
                terminal.send(HOME, 'esc back')
                shown = terminal.flat()
                assert 'waiting on your judgment: seed-2, seed-4' in shown, 'worker-judgment: ' + shown
                assert 'verification failed' not in shown.lower(), shown
                assert 'f finish' not in shown, 'finish-offer: ' + shown
                before = fixture.mutations()
                settle(terminal, END + b'f')
                assert fixture.mutations() == before, 'finish-key: pending judgment completed worker'
                assert fixture.read()['step']['state'] == 'active'
            elif case == 'finish-reread':
                fixture.change(lambda state: change(state, 'current'))
                settle(terminal, b'r')
                terminal.send(END, 'f finish')
                fixture.change(change)
                settle(terminal, b'f')
                assert fixture.read()['step']['state'] == 'active', 'finish-reread: pending judgment completed worker'
                terminal.send(END, 'Finish needs current passing checks')
            elif case == 'watch':
                terminal.send(b'w', 'Needs you')
                shown = terminal.flat()
                assert 'waiting on your judgment' in shown, 'watch-judgment: ' + shown
                assert 'seed-2, seed-4' in shown, 'watch-ids: ' + shown
                assert 'verification failed' not in shown.lower(), 'watch-failed: ' + shown
                before = fixture.mutations()
                terminal.send(b'\r', 'Checklist')
                assert 'waiting on your judgment' in terminal.flat(), terminal.screen.text()
                assert fixture.mutations() == before, 'Watch mutated the run'
            elif case == 'worker-refusals':
                for status, person in [('failed', False), ('failed', True), ('stale', False)]:
                    def negative(state):
                        change(state, status)
                        if person:
                            state['run']['checklist'][0].update(status='failed', mark={'evidence': 'TEST rejected'})
                    fixture.change(negative)
                    settle(terminal, b'r')
                    terminal.send(HOME, 'esc back')
                    shown = terminal.flat()
                    assert 'Verification: ' + status in shown, 'worker-refusal-state: ' + shown
                    assert 'waiting on your judgment' not in shown, shown
                    assert 'f finish' not in shown, 'worker-refusal-finish: ' + shown
                    before = fixture.mutations()
                    settle(terminal, END + b'f')
                    assert fixture.mutations() == before, 'worker-refusal-mutation'
            elif case == 'watch-refusals':
                fixture.change(lambda state: change(state, 'failed'))
                terminal.send(b'w', 'verification failed')
                for status, expected in [('failed', 'verification failed'), ('stale', 'verification out of date')]:
                    fixture.change(lambda state: change(state, status))
                    terminal.send(b'r', expected)
                    assert 'waiting on your judgment' not in terminal.flat(), terminal.flat()
                fixture.change(lambda state: change(state, 'TEST-unknown'))
                terminal.send(b'r', "Watch doesn't know")
                assert 'waiting on your judgment' not in terminal.flat(), terminal.flat()
            elif case == 'unknown':
                fixture.change(lambda state: change(state, 'TEST-unknown'))
                settle(terminal, b'r')
                assert 'Verification state is unknown or missing' in terminal.flat(), 'unknown-worker: ' + terminal.flat()
                before = fixture.mutations()
                settle(terminal, b'f')
                assert fixture.mutations() == before, 'unknown worker completed'
            else:
                raise AssertionError(case)
            assert not (fixture.root / 'unexpected-provider').exists()
            assert not any(call['argv'][0] in ('mark', 'decide') for call in fixture.calls())
        finally:
            terminal.close()
    print(f'PASS {case}: {width}x{height}, NO_COLOR, zero providers', flush=True)


def pin():
    value = json.loads((REPO / '_system/scaffold.json').read_text())
    assert value['version'] == '1.57.0', 'scaffold-pin: ' + value['version']


def run(case, width=100, height=30):
    if case == 'pin':
        pin()
    elif case in ('worker', 'watch', 'finish-reread', 'unknown', 'worker-refusals', 'watch-refusals'):
        worker(case, width, height)
    else:
        preview(width, height, mono=True, case=case)


CASES = ['awaiting', 'worker', 'watch', 'finish-reread', 'worker-refusals', 'watch-refusals',
         'failed', 'person-failed', 'stale', 'dirty',
         'unknown', 'awaiting-invalid-ids', 'awaiting-stale-before-v', 'pin']


def reversion():
    patches = [
        ('src/taste/preview.rs', '        if !passed && !awaiting {', '        if !passed {',
         'awaiting', 'candidate-label:'),
        ('src/taste/view.rs', '"Unaccepted candidate"', '"Local checked result"',
         'awaiting', 'candidate-label:'),
        ('src/taste/read.rs', 'row.pending_attestations = parsed.pending_attestations.clone()?;',
         'row.pending_attestations = vec!["TEST-changed".into()];', 'awaiting', 'pending-ids:'),
        ('src/taste/view.rs', 'Only a person can record them in tink-sdlc.', 'TEST reminder removed.',
         'awaiting', 'human-only:'),
        ('src/menu/worker.rs', 'waiting on your judgment: {}', 'TEST pending: {}',
         'worker', 'worker-judgment:'),
        ('src/watch/state.rs', 'Self::AwaitingAttestation => "waiting on your judgment"',
         'Self::AwaitingAttestation => "verification failed"', 'watch', 'watch-judgment:'),
        ('src/menu/worker.rs', '.is_some_and(|s| s.verification.is(crate::sdlc::Verification::Current))',
         '.is_some_and(|s| s.verification.is(crate::sdlc::Verification::AwaitingAttestation))',
         'worker', 'finish-offer:'),
        ('src/worker.rs', '!value.verification.is(sdlc::Verification::Current)\n                || value.data["verification"]["passed"].as_bool() != Some(true)',
         'false', 'finish-reread', 'finish-reread: pending judgment completed worker'),
        ('src/taste/preview.rs', '        if !passed && !awaiting {', '        if false {',
         'failed', 'refusal-failed:'),
        ('src/taste/preview.rs', '        if !passed && !awaiting {', '        if false {',
         'person-failed', 'refusal-person-failed:'),
        ('src/taste/preview.rs', '        if !passed && !awaiting {', '        if false {',
         'stale', 'refusal-stale:'),
        ('src/sdlc.rs', 'Invalid => "invalid",', 'Invalid => "TEST-unknown",',
         'unknown', 'unknown-worker:'),
        ('src/sdlc/source.rs', '    compare(&[&paths[..], &[":(exclude)AGENTS.md"]].concat()).map_err(|_| differs)?;',
         '    let _ = &paths;', 'dirty', 'refusal-dirty:'),
        ('src/taste/preview.rs', '        local.recheck(stop)?;', '        let _ = local;',
         'awaiting-stale-before-v', 'stale reading executed a command'),
        ('_system/scaffold.json', '"version": "1.57.0"', '"version": "1.55.0"',
         'pin', 'scaffold-pin:'),
    ]
    env = subprocess.environment()
    build = ['cargo', '+stable', 'build', '--locked', '--quiet', '--bin', 'tinkery']
    try:
        for name, old, new, case, assertion in patches:
            path = REPO / name
            original = path.read_text()
            count = original.count(old)
            assert count == (2 if case == 'awaiting-stale-before-v' else 1), (name, old, count)
            try:
                path.write_text(original.replace(old, new))
                if name.endswith('.rs'):
                    subprocess.run(build, cwd=REPO, env=env, check=True)
                red = subprocess.run([os.sys.executable, __file__, '--case', case], cwd=REPO,
                                     env=env, capture_output=True, text=True)
                output = red.stdout + red.stderr
                assert red.returncode != 0 and assertion in output, output
                print(f'RED {case}: {name}: {assertion}', flush=True)
            finally:
                path.write_text(original)
                if name.endswith('.rs'):
                    subprocess.run(build, cwd=REPO, env=env, check=True)
            subprocess.run([os.sys.executable, __file__, '--case', case], cwd=REPO, env=env, check=True)
            print(f'RESTORED GREEN {case}', flush=True)
    finally:
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
            run(case)
        if not args.case:
            for width, height in [(80, 24)]:
                for case in ['awaiting', 'worker', 'watch']:
                    run(case, width, height)
            preview(100, 30, mono=False, case='awaiting')
            preview(case='unknown')
        print('PASS awaiting-attestation public behavior and 1.57.0 pin', flush=True)
