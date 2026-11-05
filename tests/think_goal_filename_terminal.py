"""Think must show the filename it confirms and saves, including a bidi control."""
import json
from pathlib import Path
import tempfile

from think_history_terminal import DETAIL, GARDEN, MENU, Terminal

STUB = '''#!/usr/bin/env python3
import json, sys
request = json.load(sys.stdin)
if request.get('kind') == 'decision-review':
    reply = {'decisions': []}
else:
    reply = {'parts': {'who': 'I', 'outcome': request['sources'][0]['text'],
             'why': 'the handoff must keep filenames', 'done_when': 'the exact name is listed'},
             'questions': []}
print(json.dumps(reply))
'''


def goal_below(screen, heading):
    """Read the first nonblank row under the heading, in the same pane."""
    lines = screen.splitlines()
    idx = next(i for i, line in enumerate(lines) if heading in line)
    column = lines[idx].index(heading)
    return next(line[column:].strip() for line in lines[idx + 1:] if line[column:].strip())


def journey(marker, displayed_marker):
    original = f'archive report{marker}cod.exe'
    expected_row = f'I archive report{displayed_marker}cod.exe.'
    with tempfile.TemporaryDirectory(prefix='tinkery-TEST-goal-filename-') as directory:
        root = Path(directory).resolve()
        records, sessions = root / 'TEST-records', root / 'TEST-sessions'
        host = root / 'stub-pi'
        host.write_text(STUB)
        host.chmod(0o700)
        terminal = Terminal(root, 120, 36, ['--operator', 'agent', '--home', '--shape-pi',
            '--shape-budget', '2', '--model', 'test/model', '--pi-command', host,
            '--record-root', records, '--seed-session-root', sessions], home=root / 'TEST-home')
        try:
            terminal.wait(lambda frame: MENU in frame)
            terminal.send(b'1', "What's on your mind?")
            terminal.send(b'\x1b[200~' + original.encode() + b'\x1b[201~', 'archive report')
            terminal.send(b'\x1bOQ', 'Ready for your review')
            terminal.send(b'\x1bOP', 'Review the goal')
            review = terminal.send(terminal.screen.pick('Review the goal'), 'You are confirming')
            receipt = terminal.send(b'confirm\r', 'Saved.')
            record_path, = records.glob('*.json')
            record = json.loads(record_path.read_bytes())
            ledger = json.loads((sessions / record['seed_me_session'] / 'ledger.json').read_bytes())
            assert record['turns'][0]['text'] == original
            assert record['goal'] == ledger['goal'] == (
                f'I {original}.\nWhy: the handoff must keep filenames.\nDone when: the exact name is listed.')
            assert record['goal_confirmation']['declaration'] == 'declared: agent'
            assert record['goal_confirmation']['action'] == 'typed confirm at this terminal'
            assert ledger['operator'] == 'agent'
            origin = next(node for node in ledger['nodes'] if node['id'] == ledger['origin'])
            assert origin['authority'] == 'delegated' and origin['delegation'] == {'agent': 'agent'}
            assert any(entry['observed'] == original for entry in origin['evidence'])

            terminal.send(b'\x1b', MENU)
            terminal.send(b'2', GARDEN)
            detail = terminal.send(b'\r', DETAIL)
            terminal.send(b'r', 'Think history')
            history = terminal.wait(lambda frame: 'Confirmed goal' in frame)
            rows = {'review': goal_below(review, 'You are confirming'),
                    'receipt': goal_below(receipt, 'Saved.'),
                    'Tend detail': next(line.strip() for line in detail.splitlines()
                                        if line.strip().startswith('I archive')),
                    'history': goal_below(history, 'Confirmed goal')}
            assert all(row == expected_row for row in rows.values()), (expected_row, rows)
        finally:
            terminal.close()


if __name__ == '__main__':
    journey('', '')
    journey('\u202e', r'\u{202e}')
