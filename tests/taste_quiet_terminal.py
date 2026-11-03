"""Quiet Taste and unchanged detail/copy transport, using synthetic API 1 and gh only."""
from terminal_process import wait_output
import base64
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import select
import struct
import sys
import tempfile
import termios
import time
import test_launch as subprocess
from taste_terminal import FAKE, GOAL, HEAD, REPO, RUN_ID, URL, confirmed_seed, fixture, tree
from vt_screen import Screen

GOLDEN = REPO / 'tests/fixtures/taste-handoff-355.json'
GAP = 'TEST source gap: /TEST/receipt; command TEST-gap-command'
LONG_GOAL = ('I compare clinic staffing allocations across a fixed weekly budget, keeping the assumptions inspectable '
             'and making trade-offs understandable before any real operational decision.\n'
             'Why: Explore trade-offs without claiming clinical validation.\n'
             'Done when: I can compare two allocations and inspect each assumption.\n'
             'Must not: Claim clinical validation or change the approved budget.')
LONG_CHECKS = [
    'The allocation comparison keeps the approved weekly budget unchanged while showing how staffing choices affect the synthetic planning assumptions.',
    'The assumptions panel preserves each operator-entered value exactly and identifies which synthetic estimate contributed to the displayed comparison.',
    'Changing one staffing choice updates both allocation previews without silently changing another choice or replacing the operator\'s original assumptions.',
    'The comparison explains an infeasible allocation in plain words and retains the previously entered budget and staffing values so the operator can correct it.',
    'A keyboard-only operator can revisit both allocations, inspect their assumptions and return to the comparison without losing unsent planning changes.',
    'Confirm that the result makes no clinical-validation claim and that the synthetic staffing assumptions remain distinguishable from observed real-world outcomes.',
]


def run(base, width, height, mono, binary, legacy=False, long_checks=False):
    checkout = base / 'TEST-checkout'
    scripts = checkout / '_system/scripts'
    scripts.mkdir(parents=True, exist_ok=True)
    (scripts / 'sdlc.py').write_text(FAKE)
    goal = LONG_GOAL if long_checks else GOAL
    seed = confirmed_seed(base, goal)
    record = {'source': str(seed), 'path': 'runs/TEST-result/seed-contract.md',
              'sha256': hashlib.sha256(seed.read_bytes()).hexdigest()}
    state = fixture(record)
    state['detail']['run']['errors'] = [GAP]
    state['detail']['run']['checklist'][0]['check']['cwd'] = './'
    if long_checks:
        automatic, human = state['detail']['run']['checklist']
        state['detail']['run']['checklist'] = [
            {**automatic, 'id': f'TEST-check-{i}', 'verify': label,
             'status': 'passed' if i < 4 else 'pending'}
            for i, label in enumerate(LONG_CHECKS[:-1])
        ] + [{**human, 'verify': LONG_CHECKS[-1]}]
        state['pr']['statusCheckRollup'] = []
    statefile, log, pid = base / 'state.json', base / 'calls', base / 'pid'
    statefile.write_text(json.dumps(state))
    log.write_text('')
    helpers = base / 'bin'
    helpers.mkdir(exist_ok=True)
    if not (helpers / 'python3').exists():
        (helpers / 'python3').symlink_to(sys.executable)
    (helpers / 'gh').write_text(f'#!{sys.executable}\n' + FAKE)
    (helpers / 'gh').chmod(0o755)
    original = tree(checkout), tree(seed.parent)
    master, slave = os.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', height, width, 0, 0))
    screen, raw = Screen(width, height), bytearray()
    env = {**os.environ, 'HOME': str(base / 'HOME'), 'PATH': f'{helpers}:/usr/bin:/bin',
           'TERM': 'xterm-256color', 'TINKERY_NODE': '/tinkery-TEST-no-node',
           'TEST_TASTE_STATE': str(statefile), 'TEST_TASTE_CALLS': str(log), 'TEST_TASTE_PID': str(pid)}
    env.pop('NO_COLOR', None)
    if mono:
        env['NO_COLOR'] = '1'
    process = subprocess.Popen(
        [str(binary), '--operator', 'agent', '--home', '--sdlc-root', str(checkout),
         '--seed-session-root', str(base / 'TEST-sessions')],
        cwd=base, env=subprocess.environment(env, home=env['HOME']), stdin=slave, stdout=slave, stderr=slave)

    def wait(predicate, seconds=8):
        deadline = time.monotonic() + seconds
        def consume_wait_output(chunk):
            raw.extend(chunk)
            screen.feed(chunk)
        for returncode in wait_output(master, process, consume_wait_output, deadline):
            if predicate() and not screen.synchronized:
                return
            assert returncode is None, screen.text()
        raise AssertionError((width, height, mono, screen.text()))

    def send(key, expected):
        os.write(master, key)
        wait(lambda: expected in screen.text())

    def copied():
        return [base64.b64decode(p) for p in re.findall(
            rb'\x1b\]52;[^;]*;([A-Za-z0-9+/=]+)(?:\x07|\x1b\\)', raw)]

    def list_only():
        text = screen.text()
        for hidden in [URL, HEAD, RUN_ID, 'TEST-result', '/TEST/receipt', 'TEST-gap-command',
                       'TEST-allocation-check.py', 'argv:', 'cwd:', 'Why: Explore trade-offs.',
                       'Guess · Next:', 'Allocation works', 'confirm no clinical claims',
                       'Independent review', 're-approve the plan']:
            assert hidden not in text, (hidden, text)
        for label in LONG_CHECKS if long_checks else []:
            assert label[:45] not in text, text
        assert 'You owe 2' in text and 'passed' in text and 'waiting' in text, text
        if long_checks:
            assert '… · open' in text and '? 1 unknown' in text, text
        else:
            assert 'TEST café' in text and '✓ 2 passed' in text, text

    def transport():
        values = {}
        for action in ['a', 'c']:
            before = len(copied())
            os.write(master, action.encode())
            wait(lambda: len(copied()) > before and 'Handoff copied' in screen.text())
            values[action] = copied()[-1]
        return values

    def frames_until(last):
        frames = [screen.text()]
        for _ in range(64):
            if last in screen.text():
                break
            previous = screen.text()
            os.write(master, b'\x1b[6~')
            wait(lambda: screen.text() != previous)
            frames.append(screen.text())
        assert last in screen.text(), frames
        return frames

    def document(frames, fixed_header=False):
        rows = []
        for frame in frames:
            body = [line[2:width-2].rstrip() for line in frame.split('\n')[7 if fixed_header else 3:height-4]]
            overlap = 0
            for count in range(1, min(len(rows), len(body)) + 1):
                if rows[-count:] == body[:count]:
                    overlap = count
            rows.extend(body[overlap:])
        return '\n'.join(rows)

    try:
        wait(lambda: '1  think' in screen.text())
        send(b'6', 'Press r to read results')
        assert not log.read_text(), 'opening performed a read'
        send(b'r', 'Results waiting on you' if legacy else ('✓ 4 passed' if long_checks else '✓ 2 passed'))
        listing = screen.text()
        if not legacy:
            list_only()
        copies = transport()
        send(b'\r', 'Goal (unchanged)' if legacy else 'Checks')
        send(b'l', 'Guess · Next:')
        main = screen.text()
        assert main.count('Guess · Next:') == 1, main
        assert transport() == copies, 'list/detail copy bytes differ'
        if legacy:
            assert URL in screen.text() and HEAD in screen.text()
            detail = frames_until('run API')
            first = '\n'.join(screen.text().split('\n')[:7]) + document(detail, True)
            evidence_frames = []
        else:
            detail = frames_until('Read 0 min ago')
            first = document(detail)
            for heading in ['You owe', 'Checks', 'Goal', 'Details']:
                assert heading in first, (heading, detail)
            assert first.index('You owe') < first.index('Checks') < first.index('Goal') < first.index('Details')
            assert 'matching evidence missing' not in first and '(automated)' not in first, first
            for hidden in ['argv:', 'cwd:', '/TEST/receipt', 'TEST-gap-command',
                           state['detail']['run']['verification']['log']]:
                assert hidden not in first, (hidden, detail)
            assert first.count(URL) == 1 and first.count('Commit:') == 1, first
            checks = first[first.index('Checks'):first.index('Goal')]
            human = state['detail']['run']['checklist'][-1]['verify']
            assert re.sub(r'\s+', '', human) not in re.sub(r'\s+', '', checks), checks
            if long_checks:
                assert '? unknown: no CI reported' in first and 'waiting: no record found' in first, first
            send(b'e', 'Evidence')
            evidence_frames = frames_until('TEST-gap-command')
        joined = re.sub(r'\s+', '', first + document(evidence_frames))
        for value in [goal, URL, HEAD, RUN_ID, 'TEST-result', GAP,
                      str(checkout / 'runs/TEST-result/04-test/output/test-log.md'),
                      'argv: ["python3","TEST-allocation-check.py"]', 'cwd: "./"',
                      state['detail']['run']['verification']['log'],
                      *[item['verify'] for item in state['detail']['run']['checklist']]]:
            assert re.sub(r'\s+', '', value) in joined, (value, detail, evidence_frames)
        assert transport() == copies, 'evidence view copy bytes differ'
        if not legacy:
            send(b'\x1b', 'e evidence')
        send(b'\x1b', 'Results waiting on you' if legacy else 'enter/v open result')
        if not legacy:
            list_only()
        send(b'\x1b', '1  think')
        os.write(master, b'q')
        process.wait(timeout=5)
        assert process.returncode == 0
        assert (tree(checkout), tree(seed.parent)) == original
        assert len(log.read_text().splitlines()) == 3, 'navigation/copying performed another read or browser action'
        assert not (base / 'TEST-sessions').exists()
        return {'copies': copies, 'main': main, 'detail': detail, 'list': listing,
                'seed_digest': record['sha256']}
    finally:
        if process.poll() is None:
            process.kill()
            process.wait()
        os.close(master)
        os.close(slave)


if __name__ == '__main__':
    if '--record-structure-baseline' in sys.argv:
        with tempfile.TemporaryDirectory(prefix='tinkery-TEST-structure-355-') as directory:
            base = Path(directory).resolve()
            result = run(base, 100, 30, False,
                         REPO / 'target/TEST-taste-structure-before-355', legacy=True, long_checks=True)
            (REPO / 'tests/fixtures/taste-structure-handoff-355.json').write_text(json.dumps(
                {key: value.decode().replace(str(base), '{ROOT}').replace(result['seed_digest'], '{SEED_DIGEST}')
                 for key, value in result['copies'].items()},
                ensure_ascii=False, indent=2) + '\n')
            (REPO / 'target/TEST-taste-structure-before-list-355.txt').write_text(result['list'])
            (REPO / 'target/TEST-taste-structure-before-review-355.txt').write_text(result['main'])
    elif '--record-baseline' in sys.argv:
        with tempfile.TemporaryDirectory(prefix='tinkery-TEST-quiet-355-') as directory:
            base = Path(directory).resolve()
            result = run(base, 100, 30, False, REPO / 'target/TEST-taste-before-355', legacy=True)
            GOLDEN.write_text(json.dumps({k: v.decode().replace(str(base), '{ROOT}').replace(result['seed_digest'], '{SEED_DIGEST}')
                                          for k, v in result['copies'].items()}, ensure_ascii=False, indent=2) + '\n')
            (REPO / 'target/TEST-taste-before-355.txt').write_text(result['main'])
    else:
        for width, height in [(100, 30), (80, 24)]:
            for mono in [False, True]:
                with tempfile.TemporaryDirectory(prefix='tinkery-TEST-quiet-355-') as directory:
                    base = Path(directory).resolve()
                    baseline = os.environ.get('TINKERY_TASTE_BASELINE')
                    before = run(base, width, height, mono, Path(baseline), legacy=True) if baseline else None
                    result = run(base, width, height, mono, REPO / 'target/debug/tinkery')
                    for action, expected in json.loads(GOLDEN.read_text()).items():
                        assert result['copies'][action] == expected.replace('{ROOT}', str(base)).replace(
                            '{SEED_DIGEST}', result['seed_digest']).encode(), action
                    if before:
                        assert result['copies'] == before['copies'], 'parent/new payload bytes changed'
                        # Layout changes deliberately; source wording and actual copy bytes do not.
                    if (width, height, mono) == (100, 30, False):
                        (REPO / 'target/TEST-taste-after-355.txt').write_text(result['main'])
                    print(json.dumps(dict(journey='quiet Taste/main/detail/identical copies', width=width,
                                          height=height, no_color=mono, parent_comparison=bool(before),
                                          provider_calls=0, source_reads=3, preview_reads=1)))
