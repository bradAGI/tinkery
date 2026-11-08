"""Quiet missing Who stays open in saved context; fake replay, zero real model calls."""
import json
import os
from pathlib import Path
import tempfile
from unittest.mock import patch

from shape_answer_terminal import Run, F2
from think_question_terminal import prepare

QUESTION = 'Whose access changes: staff or visitors?'
WORDS = 'TEST remove the ink blob. It distracts. Done when the ink blob is entirely removed.'


def journey(width, height, mono, case):
    with tempfile.TemporaryDirectory(prefix='tinkery-TEST-quiet-who-') as directory:
        root = Path(directory)
        with patch.dict(os.environ):
            os.environ.pop('NO_COLOR', None)
            if mono:
                os.environ['NO_COLOR'] = '1'
            run = Run(root, width, height, 2, agent=True)
        try:
            program, _ = prepare(root)
            (root / 'pi').write_text(program.read_text())
            parts = dict(outcome='The ink blob is removed', why='It distracts',
                         done_when='The ink blob is entirely removed')
            reply = dict(parts=parts)
            if case in ('question', 'skipped'):
                parts['outcome'] = 'Access is restricted'
                reply['questions'] = [dict(id='audience', target='who', text=QUESTION)]
            if case == 'other-missing':
                del parts['why']
            (root / 'reply.json').write_text(json.dumps(reply))
            run.send(b'1', "What's on your mind?")
            os.write(run.master, b'\x1b[200~' + WORDS.encode() + b'\x1b[201~')
            expected = ('Who: not named' if case == 'quiet' else
                        QUESTION if case in ('question', 'skipped') else 'Still missing: Who.')
            run.send(F2, expected)
            if case == 'skipped':
                run.choose('Skip this question', 'Still missing: Who.')
            if case == 'quiet':
                assert 'Still missing' not in run.flat() and 'Still open' not in run.flat()
                assert 'Ready for your review' not in run.flat()
            else:
                assert 'Who: not named' not in run.flat()
            run.choose('Review the goal', 'You are confirming')
            if case == 'quiet':
                run.wait('Who: not named')
                assert 'Still open' not in run.flat() and 'Still missing' not in run.flat()
            else:
                run.read_until('Still open')
                run.read_until(QUESTION if case in ('question', 'skipped') else 'Who — still open')
                assert 'Who: not named' not in run.flat()
            if case == 'other-missing':
                run.read_until('Why — still open')
            run.read_until('type confirm to save')
            run.send(b'confirm\r', 'Saved.')
            session, = run.sessions.iterdir()
            ledger = json.loads((session / 'ledger.json').read_text())
            origin = next(n for n in ledger['nodes'] if n['id'] == ledger['origin'])
            context = json.loads(origin['evidence'][-1]['observed'])
            assert context['goal_parts'].get('who') is None
            if case in ('question', 'skipped'):
                assert any(q['target'] == 'who' and q['text'] == QUESTION for q in context['unresolved'])
            else:
                assert 'Who — still open' in context['unresolved_notes']
            assert 'Who: not named' not in ledger['goal']
            assert any(e.get('observed') == WORDS for e in origin['evidence'])
            assert (root / 'calls').read_text() == 'shape\ndecision-review\n'
            record, = run.records.glob('*.json')
            assert json.loads(record.read_text())['goal'] == ledger['goal']
            assert not (session / 'seed-contract.md').exists()
            print(json.dumps(dict(width=width, height=height, no_color=mono, case=case,
                                  provider='fake replay; zero real calls',
                                  journey='Think/reading/review/confirm/saved open Who seed context')))
        finally:
            run.close()


if __name__ == '__main__':
    for size in [(80, 24), (100, 30)]:
        for mono in [False, True]:
            for case in ['quiet', 'question', 'skipped', 'other-missing']:
                journey(*size, mono, case)
