"""#400: explicit retries carry one current host diagnostic, never accepted facts or consent."""
from terminal_process import wait_output
import copy
import json
import os
from pathlib import Path
import select
import tempfile
import time

from shape_answer_terminal import Run, make, helper, files, SEED, GOAL, F1, F2

CRITERION = ('Enter three fictional places and compare known monthly costs; '
             'Keep unknown parking visible and restore the same saved hunt; '
             'The operator still judges usefulness and polish.')


def replies(criterion=CRITERION):
    valid = copy.deepcopy(SEED)
    valid['found'] = []
    valid['checks'][0]['statement'] = criterion
    bad = copy.deepcopy(valid)
    bad['checks'] = [{**valid['checks'][0], 'statement': part.strip()}
                     for part in criterion.split(';')]
    bad['review']['failure_cases'] = ['TEST missing behavior must fail'] * len(bad['checks'])
    return bad, valid


def wait_calls(run, count):
    deadline = time.monotonic() + 15
    for _ in wait_output(run.master, run.process, run.screen.feed, deadline):
        if len(run.calls()) >= count:
            return
    raise AssertionError(f'Expected {count} stub calls; got {run.calls()}')


def wait_either(run, *texts):
    deadline = time.monotonic() + 20
    for _ in wait_output(run.master, run.process, run.screen.feed, deadline):
        if not run.screen.synchronized:
            for text in texts:
                if text in run.flat():
                    return text
    raise AssertionError(f'None of {texts!r} shown:\n{run.screen.text()}')


def reject(run, session, bounded=False):
    run.open_shape()
    run.wait('Keep records local or synced?')
    words = 'TEST keep this exact answer café\r\n  '
    os.write(run.master, b'\x1b[200~' + words.encode() + b'\x1b[201~')
    run.wait('café')
    run.send(F2, 'Next: F2 looks into the next question.')
    if not bounded:
        run.wait('keep this complete Done when line unchanged in one check statement:')
    state = helper.load(session)
    assert next(n for n in state['nodes'] if n['id'] == 'storage')['answer'] == words
    assert len(run.calls()) == 1, 'Rejection automatically retried'
    assert 'previous_refusal' not in run.calls()[0]['input']
    assert not (session / 'seed-contract.md').exists()
    assert not (session / 'seed-checks.json').exists()
    return files(session)


def retry_journey(width, height, case):
    with tempfile.TemporaryDirectory(prefix='tinkery-TEST-400-') as directory:
        root = Path(directory).resolve()
        criterion = CRITERION if case != 'bounded' else 'TEST ' + 'café ' * 600 + '; The operator judges.'
        goal = GOAL.replace('Refusals never show private text.', criterion)
        bad, valid = replies(criterion)
        question = {'next': 'question', 'id': 'TEST-next', 'question': 'TEST what remains?', 'found': []}
        queue = [bad, question, valid] if case == 'success' else [bad, valid]
        if case == 'storage failure':
            queue = [bad, valid, valid]
        if case in ('provider failure', 'cancel'):
            queue = [bad, 'FAIL' if case == 'provider failure' else 'SLOW', valid]
        budget = 1 if case == 'budget' else len(queue)
        session = make(root / 'TEST-sessions', operator='agent', goal=goal)
        run = Run(root, width, height, budget=budget, replies=queue, agent=True)
        try:
            saved = reject(run, session, bounded=case == 'bounded')
            if case != 'bounded':
                # #435: the refusal says what a retry has left, without opening help.
                run.wait(f'Drafting runs left this launch: {budget - 1} of {budget}.')
            # Looking at recovery information, then closing it, neither writes nor calls.
            run.send(F1, 'Drafting runs left this launch:')
            run.send(F1, 'Next: F2 looks into the next question.')
            assert len(run.calls()) == 1 and files(session) == saved

            if case == 'budget':
                run.send(F2, 'No drafting runs are left:')
                assert len(run.calls()) == 1 and files(session) == saved
                print(json.dumps(dict(size=[width, height], case=case, stub_calls=1, live_calls=0)))
                return
            if case == 'folder':
                other = root / 'TEST-other-folder'
                other.mkdir()
                run.choose('Choose the folder', 'Next: F2 uses this folder;')
                run.type('/TEST-other-folder')
                run.send(F2, 'The step reads files in TEST-other-folder.')
                assert len(run.calls()) == 1 and files(session) == saved
            drafted = False
            if case == 'stale':
                state = helper.load(session)
                changed = helper.editable(state)
                changed['draft']['goal'] = 'TEST another writer changed the draft context'
                helper.publish(session, changed, state['version'], 'TEST other writer')
                assert helper.load(session)['version'] != state['version']
                os.write(run.master, F2)
                outcome = wait_either(run, 'Another writer changed this session first, so nothing was saved.',
                                      'Seed draft r1')
                drafted = outcome == 'Seed draft r1'
                if not drafted:
                    run.wait('Next: F2 looks into the next question.')
                    assert len(run.calls()) == 1
            if case in ('session', 'visit'):
                run.menu()
                if case == 'session':
                    goal = 'TEST another session.\n' + goal.split('\n', 1)[1]
                    session = make(root / 'TEST-sessions', question=False, operator='agent', goal=goal)
                    run.send(b'3', 'enter open')
                    run.wait('TEST another session.')
                    if '› TEST another session.' not in run.flat():
                        run.send(b'\x1b[B', '› TEST another session.')
                    run.send(b'\r', 'esc back')
                else:
                    run.open_shape()
                run.wait('F2 look into the next question')
                assert len(run.calls()) == 1
            if case in ('provider failure', 'cancel'):
                os.write(run.master, F2)
                wait_calls(run, 2)
                if case == 'cancel':
                    # The stub records its call before consuming the reply. Let it enter
                    # SLOW before stopping, so cancellation cannot corrupt its queue file.
                    deadline = time.monotonic() + 10
                    while time.monotonic() < deadline:
                        try:
                            if len(json.loads((root / 'shape-replies.json').read_text())) == 1:
                                break
                        except json.JSONDecodeError:
                            pass
                        time.sleep(.01)
                    else:
                        raise AssertionError('The slow stub did not consume its reply')
                    run.send(b'\x1b', 'Stopped before the next step finished.')
                else:
                    run.wait('Looking into the next question failed: Pi exited')
                run.wait('Next: F2 looks into the next question.')
                assert 'previous_refusal' in run.calls()[1]['input']
                assert files(session) == saved
            if case == 'storage failure':
                (session / 'seed-checks.json').mkdir()
                run.send(F2, 'Tinkery refused: could not save check records:')
                run.wait('Next: F2 looks into the next question.')
                assert 'previous_refusal' in run.calls()[1]['input']
                (session / 'seed-checks.json').rmdir()
                assert files(session) == saved
            if case == 'success':
                run.send(F2, 'TEST what remains?')
                assert 'previous_refusal' in run.calls()[1]['input']
                run.type('TEST nothing else')
            if not drafted:
                run.send(F2, 'Seed draft r1')
            calls = run.calls()
            assert len(calls) == budget
            last = calls[-1]['input']
            if case in ('retry', 'bounded'):
                feedback = last['previous_refusal']
                assert feedback['reason'].startswith('Tinkery refused: No seed was drafted:')
                assert 'keep this complete Done when line unchanged in one check statement:' in feedback['reason']
                assert len(feedback['reason'].encode()) <= 2048
                assert feedback['truncated'] == (case == 'bounded')
                if case == 'retry':
                    assert criterion in feedback['reason']
                # The diagnostic is a separate field; it changes no established context.
                assert {key: value for key, value in last.items() if key != 'previous_refusal'} == calls[0]['input']
            else:
                assert 'previous_refusal' not in last, f'Unrelated feedback reached {case}: {last}'
            assert last['goal'] == goal
            assert helper.load(session)['goal'] == goal
            assert helper.read_checks(session)[0]['statement'] == criterion
            assert not (session / 'seed-confirmation.json').exists()
            assert 'previous_refusal' not in (session / 'ledger.json').read_text()
            assert 'previous_refusal' not in (session / 'seed-contract.md').read_text()
            run.send(F1, 'Drafting runs left this launch: 0 of')
            assert len(run.calls()) == budget, 'Recovery display called a model'
            print(json.dumps(dict(size=[width, height], case=case, stub_calls=len(calls),
                                  live_calls=0, confirmed=False)))
        finally:
            run.close()


def inference_journey(width, height):
    """#649: the screen names the broken rule plainly; only the retry hears the node id."""
    with tempfile.TemporaryDirectory(prefix='tinkery-TEST-649-') as directory:
        root = Path(directory).resolve()
        bad = copy.deepcopy(SEED)
        bad['found'] = SEED['found'] + [{
            'id': 'inference-counter-1', 'claim': {'type': 'inference', 'scope': 'the refusal path'},
            'checked': 'the cited investigation', 'observed': 'refusals pass through main',
            'supported_by': [SEED['found'][0]['id']]}]
        session = make(root / 'TEST-sessions', operator='agent')
        run = Run(root, width, height, budget=2, replies=[bad, SEED], agent=True)
        try:
            run.open_shape()
            run.wait('Keep records local or synced?')
            os.write(run.master, b'\x1b[200~TEST local only\x1b[201~')
            run.wait('TEST local only')
            run.send(F2, 'Next: F2 looks into the next question.')
            run.wait("The model's reply was refused, so nothing from it was saved: an inference must "
                     'state its limits: what it does not establish.')
            assert 'inference-counter-1' not in run.flat(), run.screen.text()
            assert next(n for n in helper.load(session)['nodes'] if n['id'] == 'storage')['answer'] == 'TEST local only'
            assert len(run.calls()) == 1, 'Rejection automatically retried'
            run.send(F2, 'Seed draft r1')
            reason = run.calls()[1]['input']['previous_refusal']['reason']
            assert reason.startswith('Tinkery refused: node inference-counter-1: an inference must state its limits'), reason
            print(json.dumps(dict(size=[width, height], case='inference', stub_calls=2, live_calls=0)))
        finally:
            run.close()


if __name__ == '__main__':
    for size in [(100, 30), (160, 40)]:
        inference_journey(*size)
    for size in [(100, 30), (160, 40)]:
        for case in ['retry', 'success', 'stale', 'session', 'visit', 'folder', 'provider failure', 'storage failure', 'cancel', 'budget', 'bounded']:
            retry_journey(*size, case)
