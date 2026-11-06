"""Commandless checks cannot confirm; both operator repairs are zero-call PTY actions."""
import json
from pathlib import Path
import tempfile

from shape_answer_terminal import F2, Run, files, helper
from shape_terminal import make, seed


def journey(width, height, agent, repair):
    with tempfile.TemporaryDirectory(prefix='tinkery-TEST-commandless-') as directory:
        root = Path(directory).resolve()
        session = make(root / 'TEST-sessions', 'draft', goal='TEST exact goal', question=False)
        seed(session)
        old = helper.checks_block(helper.read_checks(session))
        record = dict(statement='standard library only', kind='provisional', source='TEST goal',
                      expect='No dependencies', cwd='.', missing='Dependency audit is absent')
        words = '  python3 -I -S -B tools/check_stdlib.py  ' if repair != 'judge' else '  I inspect imports myself.  '
        if repair == 'proposed':
            record['cmd'] = words
        helper.set_checks(session, [record])
        path = session / 'seed-contract.md'
        path.write_text(path.read_text().replace(old, helper.checks_block([record])))
        run = Run(root, width, height, budget=0, agent=agent)
        try:
            run.open_shape()
            run.wait('Seed draft r1')
            run.read_until('End of seed', whole_line=True)
            run.send(b'confirm', 'your words confirm')
            if repair != 'proposed':
                before = files(session)
                run.send(F2, 'Cannot confirm: "standard library only" has no command.')
                run.wait('Supply a check command, or Make it a judgment')
                assert files(session) == before, 'Refusal wrote a skip, receipt or seed'
                run.choose('Make it a judgment' if repair == 'judge' else 'Supply a check command',
                           'your how-to-judge words' if repair == 'judge' else 'your check command')
                run.send(b'\x1b[200~' + words.encode() + b'\x1b[201~', words.strip())
                run.send(F2, 'Check saved. Read the changed draft, then confirm. No model ran.')
                run.read_until('End of seed', whole_line=True)
            run.send(F2, 'Seed ready:')
            assert helper.status(session)['handoff']['verified']
            saved = json.loads((session / 'seed-checks.json').read_text())[0]
            assert saved['statement'] == record['statement']
            assert saved['kind'] == ('judge' if repair == 'judge' else 'provisional')
            assert saved['how_to_judge' if repair == 'judge' else 'cmd'] == words
            if repair == 'judge':
                assert 'you judge:' in path.read_text()
            assert run.calls() == [], 'Check repair or confirmation called a model'
            print(f'Shape commandless passed {width}x{height}: {"agent" if agent else "human"}, {repair}, zero calls')
        finally:
            run.close()


if __name__ == '__main__':
    for size in [(80, 24), (100, 30)]:
        for agent in [False, True]:
            for repair in ['command', 'judge', 'proposed']:
                journey(*size, agent, repair)
