"""Shape answering journeys (#150, #159): a stub Pi, real Rust ledger sessions in TEST roots, no model calls.

The full journey confirms a goal in Think, opens Shape from tend, sees which folder the step reads and
picks another, looks into the first question, answers it, sees the second step build on the saved
finding, and confirms the drafted seed with the ledger's canonical sections. Short cases cover the zero budget, a stale version, a
model failure, stopping a step, a helper refusal, a reply that tries to confirm or to write the decisions, a
seed finding that looks like a secret, a practice session's confirmation, a citation that isn't in the folder,
and steps that found nothing."""
from terminal_process import wait_output
import fcntl, hashlib, importlib.util, json, os, select, signal, struct, tempfile, termios, time
import test_launch as subprocess
from pathlib import Path
from vt_screen import Screen

REPO = Path(__file__).resolve().parents[1]
BINARY = REPO / 'target/debug/tinkery'
import session_fixture as helper
F1, F2, ESC = b'\x1bOP', b'\x1bOQ', b'\x1b'

# Think's requests get a reading with one question; Shape's steps pop the next scripted reply.
HOST = r'''#!/usr/bin/env python3
import json, pathlib, sys
import os
r = json.load(sys.stdin)
home = pathlib.Path(os.environ['TINKERY_TEST_DIR'])
if 'ledger' in r or r.get('kind') == 'teach-back':
    with (home / 'shape-calls.jsonl').open('a') as f:
        guard = sys.argv[sys.argv.index('--extension') + 1] if '--extension' in sys.argv else None
        guard = guard and pathlib.Path(guard).read_text()
        f.write(json.dumps({'argv': sys.argv[1:], 'cwd': os.getcwd(), 'input': r, 'guard': guard}) + '\n')
    replies = home / 'shape-replies.json'
    queue = json.loads(replies.read_text())
    reply = queue.pop(0)
    replies.write_text(json.dumps(queue))
    if reply == 'FAIL':
        sys.exit(3)
    if reply == 'SLOW':
        import time
        time.sleep(60)
    print(reply if isinstance(reply, str) else json.dumps(reply))
    sys.exit(0)
with (home / 'think-calls.jsonl').open('a') as f:
    f.write(json.dumps(r) + '\n')
if r.get('kind') == 'decision-review':
    reply = [dict(part=d['part'], text=d['text'], answers=d['answers'], question=d['question'], covers=[i])
             for i, d in enumerate(r.get('decisions', []))]
    print(json.dumps({'decisions': reply}))
    sys.exit(0)
choices = []
if len(r['sources']) == 1:
    choices = [dict(part='must_not', text='Private text', answers=['omit', 'include'],
                    question='May refusals show private text?')]
print(json.dumps({'parts': {'who': 'I',
                            'outcome': 'Keep refusals calm' if os.environ.get('TINKERY_TEST_WHEN') else 'keep refusals calm',
                            'when': os.environ.get('TINKERY_TEST_WHEN'),
                            'why': os.environ.get('TINKERY_TEST_WHY', 'the work is stuck'),
                            'done_when': 'Refusals never show private text'},
                  'decisions': choices, 'questions': []}))
'''

QUESTION = {'next': 'question', 'id': 'Storage', 'label': 'Where records live',
            'question': 'Keep records local or synced?', 'gate': 'storage choice',
            'found': [{'id': 'read-local', 'claim': {'type': 'observation', 'scope': 'README.md:3'},
                       'checked': 'README.md:3', 'observed': 'says local only'}],
            'supported_by': ['read-local'],
            'suggestion': 'Local, matching the README.'}
def completion_review(checks):
    return {'contradictions': [], 'decided': ['TEST agreed scope, not completed work'], 'done': [],
            'failure_cases': ['TEST wrong output must fail' for _ in checks]}


def seed_fields(body):
    """Replay historical Markdown fixtures as prose fields; production never parses model headings."""
    fields = {}
    name, lines = 'You are confirming', []
    for line in body.splitlines():
        if line.startswith('## '):
            if lines:
                fields[name] = '\n'.join(lines).strip()
            name, lines = line[3:], []
        else:
            lines.append(line.replace('Probe:', 'Probe reading (guess):').replace('— answer:', '— evidence reading:'))
    if lines:
        fields[name] = '\n'.join(lines).strip()
    return fields


# The model also offers decisions of its own; Tinkery must ignore them and insert `seed decisions`.
SEED_BODY = '''Refusals stay calm and records stay local.
Build calm refusals.
Do not build sync.
Accepted unchanged: 0 defaults; chosen: 1 storage decision.
No blocking unknowns or reversed decisions.

## Problem statement
Refusals are noisy; the operator loses the thread.

## Proposed outcome
Refusals stay calm and records stay local.

## Affected users and systems
The operator and local refusal path.

## Constraints and boundaries
Keep the operator's words. Do not run agents without permission.

## Knowledge map
What we know, with proof: see recorded evidence below.
What we know we don't know: operator judgment of calmness.
What we have not read: paths not listed in the evidence.
What could surprise us: private text in a refusal; re-check when refusal code changes.

## Suggested first slice
Proposal only: inspect the local refusal path before changing it.
cmd: git status --short
cwd: .

## Risks and verification
Private text could leak; the proposed criterion is not completed verification.

## Open questions and deferrals
The operator judges calmness after implementation. No blocking decisions remain.

## Owners and next steps
The operator judges refusal output; implementation checks privacy.

## Downstream handoff
Carry seed-contract.md, ledger.json and seed-confirmation.json together; check session.py status for handoff.verified. This is not implementation approval.
'''
SEED = {'next': 'seed', 'confirming': seed_fields(SEED_BODY),
        'checks': [{'statement': 'Refusals never show private text.', 'kind': 'judge',
                    'source': 'TEST goal', 'how_to_judge': 'Inspect refusal output for private text.'}],
        'found': [{'id': 'read-refusal', 'claim': {'type': 'observation', 'scope': 'src/main.rs:12'},
                   'checked': 'src/main.rs:12', 'observed': 'the refusal path; the change touches it'}],
        'decisions': '- storage [user] MODEL-INVENTED synced (source: the model)'}
SEED['review'] = completion_review(SEED['checks'])
GOAL = ('I can see where my seed stands without switching tools.\nWhy: I lose the thread.\n'
        'Done when: Refusals never show private text.\nMust: Keep my words.\nMust not: Run agents.')


def node(identifier, **fields):
    return {'id': identifier, 'kind': 'decision', 'prerequisites': [], 'evidence': [],
            'owner': 'TEST operator', 'gate': 'TEST choice', **fields}


def make(root, question=True, operator='human', goal=GOAL):
    """A session with a confirmed goal and, optionally, one current question."""
    session = helper.create(root, operator)
    old = helper.load(session)
    state = helper.editable(old)
    authority = 'simulated' if operator == 'simulated' else 'user'
    nodes = [node('goal', status='settled', answer=goal, authority=authority,
                  authority_source='TEST fixture, not real approval')]
    if question:
        nodes.append(node('storage', status='unresolved', label='Where records live',
                          question='Keep records local or synced?'))
    # Most cases are about later steps, so the pre-mortem is already answered.
    nodes.append(node('probe-pre-mortem', status='settled', answer='TEST pre-mortem answer', authority=authority,
                      authority_source='TEST fixture, not real approval', label='Probe: pre-mortem',
                      question='Imagine this shipped and turned out wrong. What is the most likely reason?'))
    state.update(goal=goal, origin='goal', nodes=nodes, current_question='storage' if question else None)
    helper.publish(session, state, old['version'], 'TEST fixture')
    return session


def files(root):
    return {str(p): p.read_bytes() for p in root.rglob('*') if p.is_file() and p.name != '.writer.lock'}


class Run:
    def __init__(self, cwd, width, height, budget, replies=(), situation=None, agent=False, sdlc_root=None, why=None, shape_setup=True):
        self.cwd, self.width = cwd, width
        self.sessions = cwd / 'TEST-sessions'
        self.records = cwd / 'TEST-records'
        host = cwd / 'pi'
        host.write_text(HOST)
        host.chmod(0o755)
        (cwd / 'shape-replies.json').write_text(json.dumps(list(replies)))
        self.master, self.slave = os.openpty()
        fcntl.ioctl(self.slave, termios.TIOCSWINSZ, struct.pack('HHHH', height, width, 0, 0))
        self.screen = Screen(width, height)
        args = [str(BINARY), '--home', '--seed-session-root', str(self.sessions), '--record-root', str(self.records),
                '--link-root', str(cwd / 'TEST-links')]
        if shape_setup:
            args += ['--shape-pi', '--model', 'test/model', '--pi-command', str(host)]
        if sdlc_root is not None:
            args += ['--sdlc-root', str(sdlc_root)]
        if agent:
            args += ['--operator', 'agent']
        if budget is not None:
            args += ['--shape-budget', str(budget)]
        env = {**os.environ, 'TERM': 'xterm-256color', 'TINKERY_NODE': '/tinkery-TEST-no-node',
               'TINKERY_TEST_DIR': str(cwd)}
        if situation is not None:
            env['TINKERY_TEST_WHEN'] = situation
        if why is not None:
            env['TINKERY_TEST_WHY'] = why
        self.process = subprocess.Popen(args, stdin=self.slave, stdout=self.slave, stderr=self.slave,
                                        cwd=cwd, env=subprocess.environment(env))

    def flat(self):
        return ' '.join(self.screen.text().split())

    def wait(self, text, timeout=20, *, wrapped_path=False):
        deadline = time.monotonic() + timeout
        normalize = (lambda value: ''.join(value.split())) if wrapped_path else (lambda value: ' '.join(value.split()))
        want = normalize(text)
        for returncode in wait_output(self.master, self.process, self.screen.feed, deadline):
            if want in normalize(self.screen.text()) and not self.screen.synchronized:
                return
            if returncode is not None:
                break
        raise AssertionError(f'{text!r} not shown:\n{self.screen.text()}')

    def send(self, data, text):
        os.write(self.master, data)
        self.wait(text)

    def menu(self):
        """Return through completed screens; never answer a draft-discard prompt."""
        deadline = time.monotonic() + 12
        for _ in range(4):
            # A caller may have stopped reading in the middle of a synchronized frame.
            if self.screen.synchronized:
                for _ in wait_output(self.master, self.process, self.screen.feed, deadline):
                    if not self.screen.synchronized:
                        break
            before = self.screen.text()
            if self.screen.synchronized or 'lose this draft?' in self.flat():
                break
            if '4 approve' in self.flat():
                return
            os.write(self.master, ESC)
            transition_deadline = min(deadline, time.monotonic() + 3)
            for returncode in wait_output(self.master, self.process, self.screen.feed, transition_deadline):
                if not self.screen.synchronized and self.screen.text() != before:
                    if 'lose this draft?' in self.flat():
                        break
                    if '4 approve' in self.flat():
                        return
                    break
                if returncode is not None:
                    break
            else:
                break
        raise AssertionError(f'Esc never reached the menu:\n{self.screen.text()}')

    def choose(self, label, text):
        """Opens F1, picks the row naming `label` by its number, then waits for `text`."""
        self.send(F1, 'Right now')
        self.wait(label)
        self.send(self.screen.pick(label), text)

    def read_until(self, text, timeout=20, *, whole_line=False, wrapped_path=False):
        """Read each rendered page; reaching the bottom can hide an earlier wrapped warning."""
        deadline = time.monotonic() + timeout
        normalize = (lambda value: ''.join(value.split())) if wrapped_path else (lambda value: ' '.join(value.split()))
        want = normalize(text)
        def shown():
            if whole_line:
                return any(line.strip() == text for line in self.screen.text().splitlines())
            return want in normalize(self.screen.text())
        for _ in range(80):
            if shown() and not self.screen.synchronized:
                return
            before = self.screen.text()
            os.write(self.master, b'\x1b[6~')
            for returncode in wait_output(self.master, self.process, self.screen.feed, deadline):
                if not self.screen.synchronized:
                    if shown():
                        return
                    if self.screen.text() != before:
                        break
                if returncode is not None:
                    raise AssertionError(f'Exited while reading {text!r}:\n{self.screen.text()}')
            else:
                break
        raise AssertionError(f'{text!r} not found while reading:\n{self.screen.text()}')

    def type(self, words):
        os.write(self.master, words.encode())
        self.wait(words)

    def calls(self):
        path = self.cwd / 'shape-calls.jsonl'
        return [json.loads(l) for l in path.read_text().splitlines()] if path.exists() else []

    def open_shape(self):
        self.wait('3  shape')
        self.send(b'2', 'enter open')
        self.send(b'\r', 's shape')
        self.send(b's', 'esc back')

    def close(self):
        if self.process.poll() is None:
            self.process.kill()
            self.process.wait()
        os.close(self.master)
        os.close(self.slave)


def full_journey(width, height):
    with tempfile.TemporaryDirectory(prefix='tinkery-TEST-shape-answer-') as directory:
        cwd = Path(directory).resolve()
        run = Run(cwd, width, height, budget=6, replies=[QUESTION, SEED])
        try:
            run.wait('1  think')
            run.send(b'1', "What's on your mind?")
            run.type('Refusals feel noisy.')
            run.send(F2, 'May refusals show private text?')
            run.type('never')
            run.send(F2, "Ready for your review")
            run.choose('Review the goal', 'You are confirming')
            run.send(b'confirm\r', 'Saved.')
            record = json.loads(next(run.records.iterdir()).read_text())
            session = run.sessions / record['seed_me_session']
            goal = helper.load(session)['goal']
            assert goal == record['goal']
            run.send(ESC, '3  shape')
            before = files(run.sessions)
            target = cwd / 'TEST-target\nfolder'
            target.mkdir()
            # The question's citation is in the folder; the seed's src/main.rs:12 is not (#161).
            (target / 'README.md').write_text('# TEST\n\nLocal only.\n')
            run.open_shape()
            run.wait("Shape hasn't asked its next question yet.")
            run.wait('F2 look into the next question')
            # Before the first step, Shape says which folder it reads; F1 picks another.
            run.wait(f'The step reads files in {cwd.name}. F1 picks another folder.')
            # Help may run past a short screen; the Rust tests check it names the full path.
            run.send(F1, 'The next step reads files in:')
            run.send(b'\x1b', 'F2 look into the next question')
            run.choose('Choose the folder', 'Next: F2 uses this folder')
            # The editor may scroll off the path's start; exact destinations are checked in both calls below.
            assert cwd.name in ''.join(run.screen.text().split('Next:')[0].split()), run.screen.text()
            os.write(run.master, b'/TEST-targat')
            # Fix inside the folder, and prove Enter edits rather than applying it.
            run.send(b'\x1b[D' * 2 + b'\x1b[3~e\x1b[F\rfolder', 'enter newline')
            run.wait('TEST-target\nfolder', wrapped_path=True)
            assert files(run.sessions) == before and run.calls() == [], 'Folder editing wrote or called'
            run.send(F2, 'The step reads files in TEST-targetfolder.')
            # Opening, scrolling and help write nothing and call no model.
            run.send(b'\x1b[B', 'F2 look into the next question')
            run.send(F1, 'Drafting runs left this launch: 2 of 6.')
            run.send(b'\x1b', 'F2 look into the next question')
            assert files(run.sessions) == before and run.calls() == [], 'Shape wrote or called before F2'
            # Seed Me's pre-mortem comes first, in the operator's words, before any finding (#234).
            run.send(F2, 'Probe: pre-mortem Imagine this shipped and turned out wrong. What is the most likely reason?')
            assert run.calls() == [], 'The pre-mortem called a model'
            assert 'What I found' not in run.flat() and 'Nothing found' not in run.flat(), run.screen.text()
            assert 'or type skip and send to skip it' in run.flat(), run.screen.text()
            run.type('TEST It showed private text.')
            run.send(F2, 'Where records live Keep records local or synced?')
            run.wait('What I found: README.md:3: says local only')
            probe = next(n for n in helper.load(session)['nodes'] if n['id'] == 'probe-pre-mortem')
            assert probe['answer'] == 'TEST It showed private text.' and probe['authority'] == 'user', probe
            assert 'citation not found' not in run.flat(), run.screen.text()
            run.wait('F2 save + look')
            calls = run.calls()
            assert len(calls) == 1, calls
            argv = calls[0]['argv']
            assert argv[argv.index('--tools') + 1] == 'read,grep,find,ls' and '--no-tools' not in argv, argv
            assert calls[0]['guard'] == (REPO / 'pi/folder.mjs').read_text(), 'Pi must load the folder guard'
            assert calls[0]['input']['answered_in_think'] == [
                {'question': 'May refusals show private text?', 'answer': 'never'}], calls[0]['input']
            assert calls[0]['cwd'] == str(target), calls[0]['cwd']
            assert calls[0]['input']['found_so_far'] == [], calls[0]['input']
            # Typing writes nothing; capitals and ? are words, not commands.
            before = files(run.sessions)
            before_all = files(cwd)
            run.type('Local? Yes, Keep it remote.')
            run.send(b'\x1b[D\x17local', 'Local? Yes, Keep it local.')
            run.send(b'\x1b[F\rOnly on this machine.', 'Only on this machine.')
            run.send(b'\x1b[200~\rTEST CR\r\nTEST CRLF\x1b[201~', 'TEST CRLF')
            answer = 'Local? Yes, Keep it local.\nOnly on this machine.\rTEST CR\r\nTEST CRLF'
            # Kitty repeat events must not act, even with nonempty edited words.
            run.send(b'\x1b[1;1:2Q' * 3 + F1, 'Type your answer. Enter adds a line.')
            run.wait('Drafting runs left this launch: 1 of 6.')
            run.send(b'\x1b', 'TEST CRLF')
            assert files(run.sessions) == before and files(cwd) == before_all and len(run.calls()) == 1, 'Typing, paste, editing or held F2 wrote or called'
            if width == 100:
                print('TEST Shape answering 100x30 screen:\n' + run.screen.text())
            run.send(F2, 'Seed draft r1')
            run.wait('Confirming the seed is not approval to build.')
            # The displayed card comes from Seed Me, not model-written criteria.
            run.read_until('you judge: Inspect refusal output for private text.')
            # Inspect each page: the flag can be above the bottom viewport on either CI platform.
            run.read_until('citation not found: src/main.rs')
            run.wait('pgup/dn read to end')
            assert 'status: draft' not in run.screen.text(), run.screen.text()
            ledger = helper.load(session)
            answered = next(n for n in ledger['nodes'] if n['id'] == 'storage')
            assert answered['answer'] == answer and answered['authority'] == 'user', answered
            seed = (session / 'seed-contract.md').read_text()
            assert helper.read_checks(session) == SEED['checks']
            assert helper.checks_block(SEED['checks']) in seed
            assert helper.decisions_block(ledger) in seed, seed
            assert seed.index('## Decisions (from the ledger)') < seed.index('## Knowledge map')
            assert 'MODEL-INVENTED' not in seed, seed
            assert len(run.calls()) == 2
            # The second step built on the saved finding, in the folder the operator picked.
            second = run.calls()[1]
            assert second['cwd'] == str(target), second['cwd']
            recorded = second['input']['found_so_far']
            assert len(recorded) == 1 and recorded[0]['id'] == 'read-local', recorded
            assert recorded[0]['claim'] == QUESTION['found'][0]['claim'], recorded
            assert not any(n['kind'] == 'fact' for n in second['input']['ledger']['nodes'])
            # The seed's grounding is the saved findings, word for word, before the decisions.
            date = next(n for n in ledger['nodes'] if n['id'] == 'read-local')['evidence'][0]['at'][:10]
            today = ("## Evidence and uncertainty\n"
                     "Grounded in:\n- README.md\n\n"
                     "Provenance unavailable: reported reads do not verify filesystem origins or unrecorded reads. "
                     "Some recorded findings have no attributable read path.\n\n"
                     f"Checked by read-only file tools in {' '.join(str(target).split())}:\n"
                     f"- README.md:3: says local only (observation; scope: README.md:3) (observed {date})\n"
                     f"Checked by read-only file tools in {' '.join(str(target).split())}; "
                     "citation not found: src/main.rs isn't in the folder:\n"
                     f"- src/main.rs:12: the refusal path; the change touches it (observation; scope: src/main.rs:12) (observed {date})\n\n")
            assert today + '## Suggested first slice\n' in seed, seed
            # Both question and seed findings are facts; prose appears once in seed evidence.
            assert len([n for n in ledger['nodes'] if n['kind'] == 'fact']) == 2, ledger['nodes']
            assert answered['supported_by'] == ['read-local'] and answered['evidence'] == []
            for observed in ('says local only', 'the refusal path; the change touches it'):
                evidence = seed.split('## Evidence and uncertainty\n', 1)[1].split('## Suggested first slice', 1)[0]
                assert evidence.count(observed) == 1, seed
            # Returning rereads the same session, including the garden's stage, without work.
            before_return = files(cwd)
            run.send(b'\x1b', 'tinkery › tend')
            run.wait('Next: confirm the seed draft in Shape.')
            run.send(b'\x1b', 'seed drafted')
            run.send(b'\r', 'Next: confirm the seed draft in Shape.')
            assert files(cwd) == before_return and len(run.calls()) == 2, 'Returning from draft wrote or called'
            run.send(b's', 'Seed draft r1')
            run.wait('pgup/dn read to end')
            before = files(run.sessions)
            before_all = files(cwd)
            run.type('Yes, I reject r1.')
            run.send(b'\x1b[D' * 4 + b'\x17confirm', 'Yes, I confirm r1.')
            run.send(b'\x1b[H' + b'\x1b[1;2C' * 3 + b'Indeed', 'Indeed, I confirm r1.')
            run.send(b'\x1b[1;1:2Q' * 3 + F1, 'Read the seed, type confirm, then F2')
            run.send(b'\x1b', 'Indeed, I confirm r1.')
            assert files(run.sessions) == before and files(cwd) == before_all and len(run.calls()) == 2, 'Editing or held F2 wrote or called'
            # F2 waits until the whole draft has been on screen (#250), and says so.
            run.send(b'\x1b[H' + F2, "PgDn or Down to 'End of seed' first. Nothing was confirmed.")
            assert files(run.sessions) == before, 'F2 confirmed an unread draft'
            run.read_until('This is not implementation approval.')
            run.read_until('End of seed', whole_line=True)
            run.wait('Indeed, I confirm r1.')
            # Only the word confirm confirms; other words are kept and may be a correction (#614).
            run.send(F2, 'Type confirm, or F1 Revise the draft. Nothing was confirmed.')
            assert files(run.sessions) == before, 'F2 confirmed with words other than confirm'
            run.send(b'\x01\x0b' + b'confirm', 'your words confirm')
            run.send(F2, 'Seed ready: you confirmed r1, and it still matches.')
            status = helper.status(session)
            assert status['seed']['line_1'] == 'status: confirmed for intake', status
            assert status['handoff']['verified'] is True, status
            confirmation = json.loads((session / 'seed-confirmation.json').read_text())
            assert confirmation['source'] == 'confirm', confirmation
            assert helper.load(session)['goal'] == goal, 'The confirmed goal changed'
            assert len(run.calls()) == 2, 'Confirmation called a model'
            before_return = files(cwd)
            run.send(b'\x1b', 'tinkery › tend')
            run.wait('Next: check the seed in Shape.')
            run.send(b'\x1b', 'seed needs checking')
            run.send(b'\r', 'Next: check the seed in Shape.')
            assert ' '.join(goal.split()) in run.flat(), 'Return changed the displayed goal'
            assert files(cwd) == before_return and len(run.calls()) == 2, 'Returning from confirmation wrote or called'
            # Ending is an F1 row that asks once more; Esc between keeps the session open (#231).
            run.send(b's', 'Seed ready: you confirmed r1, and it still matches.')
            run.wait('F1 more')
            before = files(run.sessions)
            run.choose('End this session', 'End this session as completed?')
            run.wait('F2 end for good   esc keep it')
            run.send(b'\x1b', 'Nothing was ended.')
            assert files(run.sessions) == before, 'Asking or Esc ended the session'
            run.choose('End this session', 'Next: F2 ends it; Esc keeps it open.')
            run.send(F2, 'Session completed. It\'s read-only now.')
            run.wait('This session is completed, so nothing in it can change.')
            # Intake needs only the verified seed, so Approve still offers it.
            run.wait('F1, then 1: Open this seed in Approve.')
            run.send(F1, 'F1 close')
            assert 'End this session' not in run.flat(), run.screen.text()
            run.send(ESC, 'F1, then 1: Open this seed in Approve.')
            assert helper.load(session)['status'] == 'completed'
            assert helper.status(session)['handoff']['verified'] is True
            assert helper.load(session)['goal'] == goal, 'Ending changed the confirmed goal'
            assert len(run.calls()) == 2, 'Ending called a model'
            # Tend now counts the session ended, as its readiness needs.
            run.send(b'\x1b', 'tinkery › tend')
            run.wait('No active goals. 1 ended not shown.')
            print(f'Shape answering full journey passed at {width}x{height}: folder shown and picked, '
                  f'think answer and saved findings in the input, 2 stub calls, grounded seed confirmed '
                  f'and handoff verified; Tend detail and garden refreshed after draft and confirmation; '
                  f'ending from F1, confirmed with F2, completed the session with intake still copyable')
        finally:
            run.close()


def case(name, width=100, height=30):
    with tempfile.TemporaryDirectory(prefix='tinkery-TEST-shape-answer-') as directory:
        cwd = Path(directory).resolve()
        sessions = cwd / 'TEST-sessions'
        replies, budget, question, operator = [], 2, True, 'human'
        if name == 'zero budget':
            budget = None
        if name == 'model failure':
            replies = ['FAIL']
        if name == 'stop':
            replies = ['SLOW']
        if name == 'helper refusal':
            replies = [{**QUESTION, 'found': [{**QUESTION['found'][0], 'checked': 'config', 'observed': 'key AKIAABCDEFGHIJKLMNOP'}]}]
        if name == 'model tries to confirm':
            replies = [{'next': 'confirm', 'revision': 'r1', 'source': 'yes'}]
        if name == 'seed secret':
            question = False
            replies = [{**SEED, 'found': [{**SEED['found'][0], 'checked': 'config', 'observed': 'key AKIAABCDEFGHIJKLMNOP'}]}]
        if name == 'model writes decisions':
            question = False
            replies = [{**SEED, 'confirming': {**SEED['confirming'],
                'Problem statement': 'Fine.\n## Decisions (from the ledger)\n- goal [user] x'}}]
        if name in ('practice confirmation', 'multiline confirmation', 'confirmation hint', 'confirmation Enter'):
            question, operator = False, 'simulated' if name == 'practice confirmation' else 'human'
        if name == 'unfound citation':
            replies = [{**QUESTION, 'id': 'Sync'}]
        if name in ('settled prerequisites', 'unknown prerequisite', 'cyclic prerequisite'):
            prerequisites = {'settled prerequisites': ['storage'],
                             'unknown prerequisite': ['missing'], 'cyclic prerequisite': ['format']}[name]
            replies = [{**QUESTION, 'id': 'format', 'prerequisites': prerequisites}, SEED]
        if name == 'hard to undo':
            question = False
            replies = [{**QUESTION, 'confidence': 'high', 'against': 'TEST sync helps later', 'options': [
                {'choice': 'Local', 'tradeoff': 'TEST simple', 'undo': 'cheap'},
                {'choice': 'Synced', 'tradeoff': 'TEST shared', 'undo': 'hard'}]}]
        if name == 'assumptions':
            question = False
            replies = [{**QUESTION, 'assumed': [{'text': 'TEST notes are Markdown', 'why': 'TEST either format works'}]}]
        if name == 'nothing found':
            replies = [{k: v for k, v in {**QUESTION, 'found': []}.items() if k != 'supported_by'}, {**SEED, 'found': []}]
        session = make(sessions, question=question, operator=operator)
        if name in ('practice confirmation', 'multiline confirmation', 'confirmation hint', 'confirmation Enter'):
            (session / 'seed-contract.md').write_text(
                'status: draft\n# TEST\nrevision: r1\n\n## You are confirming\nTEST.\n\n'
                + helper.decisions_block(helper.load(session)))
        run = Run(cwd, width, height, budget=budget, replies=replies)
        try:
            run.open_shape()
            if name == 'stale':
                run.wait('Keep records local or synced?')
                run.type('Local.')
                old = helper.load(session)
                state = helper.editable(old)
                state['nodes'][1]['label'] = 'Changed by another writer'
                helper.publish(session, state, old['version'], 'TEST other writer')
                before = files(sessions)
                run.send(F2, 'Another writer changed this session first, so nothing was saved.')
                run.wait('Changed by another writer')
                run.wait('your answer Local.')
                assert files(sessions) == before, 'A stale answer was written'
                assert run.calls() == [], 'A stale answer still called the model'
            elif name == 'stop':
                run.wait('Keep records local or synced?')
                run.type('Local.')
                # While the step runs, the question and the saved answer stay in view.
                run.send(F2, f'Answer saved. Looking in {cwd.name} for the next question.')
                run.wait('Keep records local or synced? your answer Local.')
                run.wait('esc stop')
                deadline = time.monotonic() + 10
                while not run.calls() and time.monotonic() < deadline:
                    time.sleep(.05)
                run.send(b'\x1b', 'Your answer is saved. Stopped before the next step finished.')
                run.wait('Next: F2 looks into the next question.')
                run.wait("still counts against this launch's allowance.")
                assert helper.load(session)['nodes'][1]['answer'] == 'Local.'
                assert not (session / 'seed-contract.md').exists()
            elif name == 'end the interview':
                run.wait('Keep records local or synced?')
                run.type('Local.')
                before = files(sessions)
                run.choose('Stop this session', 'Stop this session?')
                run.wait('your answer Local.')
                # Asked, typing is not words and other keys do nothing; Esc keeps it open.
                run.send(b'x1', 'F2 stop for good   esc keep it')
                run.send(b'\x1b', 'Nothing was ended.')
                run.wait('your answer Local.')
                assert files(sessions) == before and run.calls() == [], 'Asking or Esc wrote or called'
                run.choose('Stop this session', 'Next: F2 stops it; Esc keeps it open.')
                run.send(F2, 'This session has ended (stopped).')
                ledger = helper.load(session)
                assert ledger['status'] == 'stopped', ledger['status']
                # The open question stays recorded, unanswered; the typed words were never saved.
                assert ledger['nodes'][1]['status'] == 'unresolved', ledger['nodes'][1]
                assert 'answer' not in ledger['nodes'][1], ledger['nodes'][1]
                assert run.calls() == [], 'Stopping called a model'
            elif name == 'hard to undo':
                run.wait("Shape hasn't asked its next question yet.")
                run.send(F2, "This is hard to undo, so what's your instinct?")
                for hidden in ('Option A', 'Suggestion:', 'Against my suggestion'):
                    assert hidden not in run.flat(), run.screen.text()
                node = next(n for n in helper.load(session)['nodes'] if n['id'] == 'storage')
                assert 'recommendation' not in node, node
                assert 'Option B: Synced; TEST shared. Undo cost: hard.' in node['question'], node
                run.choose('Show the options', 'Here are the options and my suggestion.')
                run.wait('Option B: Synced; TEST shared. Undo cost: hard.')
                run.wait('Suggestion: Local, matching the README.')
                assert 'Confidence:' not in run.screen.text(), run.screen.text()
                node = next(n for n in helper.load(session)['nodes'] if n['id'] == 'storage')
                assert node['recommendation'] == 'Local, matching the README.', node
                assert len(run.calls()) == 1, 'Showing the suggestion called a model'
            elif name == 'assumptions':
                run.wait("Shape hasn't asked its next question yet.")
                run.send(F2, "I'll assume these unless you object:")
                run.wait('1. TEST notes are Markdown (why: TEST either format works)')
                assert helper.load(session)['assumed'] == [{'text': 'TEST notes are Markdown', 'why': 'TEST either format works'}]
                assert len(run.calls()) == 1
                # Asked which, typing is not words; Esc keeps them, then Object and its number objects.
                before = files(sessions)
                run.choose('Object', 'Which assumption do you object to?')
                run.send(b'x', 'Which assumption do you object to?')
                run.send(b'\x1b', 'Nothing was changed.')
                assert files(sessions) == before, 'Asking which or Esc wrote'
                run.choose('Object', 'Which assumption do you object to?')
                run.send(b'1', 'Not assumed: TEST notes are Markdown.')
                ledger = helper.load(session)
                assert ledger['assumed'] == [], ledger['assumed']
                objection = next(n for n in ledger['nodes'] if n.get('label') == 'Instead of assuming')
                assert objection['status'] == 'unresolved', objection
                assert "I'll assume these" not in run.flat(), run.screen.text()
                # Answering the current question asks the objection next, with no model call.
                run.type('Local.')
                run.send(F2, 'You objected to assuming: TEST notes are Markdown. What should it be instead?')
                assert len(run.calls()) == 1, 'Asking the objection called a model'
                assert helper.load(session)['current_question'] == objection['id']
            elif name == 'unfound citation':
                run.wait('Keep records local or synced?')
                run.type('Local.')
                run.send(F2, "What I found: README.md:3: says local only (citation not found: README.md isn't in the folder)")
            elif name == 'settled prerequisites':
                run.wait('Keep records local or synced?')
                run.type('TEST local')
                run.send(F2, 'Suggestion: Local, matching the README.')
                saved = helper.load(session)
                assert saved['current_question'] == 'format'
                assert next(n for n in saved['nodes'] if n['id'] == 'format')['prerequisites'] == ['storage']
                assert len(run.calls()) == 1
            elif name in ('unknown prerequisite', 'cyclic prerequisite'):
                run.wait('Keep records local or synced?')
                run.type('TEST local')
                run.send(F2, "isn't recorded" if name == 'unknown prerequisite' else 'in a circle')
                run.wait('retry the step')
                saved = helper.load(session)
                assert saved['current_question'] is None
                assert next(n for n in saved['nodes'] if n['id'] == 'storage')['answer'] == 'TEST local'
                assert not any(n['id'] in ('format', 'read-local') for n in saved['nodes']), saved
                assert not saved['assumed'] and len(run.calls()) == 1
            elif name == 'nothing found':
                run.wait('Keep records local or synced?')
                run.type('Local.')
                run.send(F2, 'Nothing found: this question records nothing checked in the repository.')
                run.type('Yes.')
                run.send(F2, 'Nothing found: this seed records nothing checked in the repository.')
                ledger = helper.load(session)
                assert helper.decisions_block(ledger) in (session / 'seed-contract.md').read_text()
                assert not any(n['kind'] == 'fact' for n in ledger['nodes']), ledger['nodes']
            elif name in ('confirmation hint', 'confirmation Enter'):
                run.wait('Seed draft r1')
                # F2 needs the whole draft read first (#250); these cases are about the words.
                os.write(run.master, b'\x1b[6~' * 10)
                if name == 'confirmation hint':
                    run.wait('type confirm, then F2')
                run.wait('Confirming the seed is not approval to build.')
                before = files(cwd)
                run.send(b'\r', 'F2 confirms the seed; Enter does not add a line.')
                run.wait('type confirm, then F2')
                run.type('confirm')
                # The placeholder already contains "confirm"; wait for the edited field.
                run.wait('your words confirm')
                assert 'type confirm, then F2' not in run.flat(), run.screen.text()
                run.send(b'\r' + F1, 'Read the seed, type confirm, then F2')
                run.send(b'\x1b', 'your words confirm')
                # F2 reaches the ledger, not the word refusal: neither Enter inserted a break.
                run.send(F2, 'Tinkery refused:')
                assert 'Type confirm, or F1' not in run.flat(), run.screen.text()
                assert files(cwd) == before, 'Hint, Enter or refused F2 wrote a file'
                assert run.calls() == [], 'Confirmation guidance called a model'
            elif name == 'multiline confirmation':
                run.wait('Seed draft r1')
                os.write(run.master, b'\x1b[6~' * 10)
                before = files(cwd)
                # Pasted LF/CR/CRLF still refuse without normalizing the words.
                for ending in ('\n', '\r', '\r\n'):
                    run.send(b'\x1b[200~' + ('I confirm' + ending + 'r1.').encode() + b'\x1b[201~', 'r1.')
                    run.send(F2, 'Type confirm, or F1 Revise the draft. Nothing was confirmed.')
                    assert files(cwd) == before, 'Multiline F2 wrote a file'
                    assert run.calls() == [], 'Multiline F2 called a model'
                    run.wait('your words I confirm r1.')
                    # CRLF is one editor newline, like CR and LF.
                    # Changed words clear the refusal about the old ones (#249).
                    run.send(b'\x7f' * 13, 'type confirm, then F2')
                    assert 'Type confirm, or F1' not in run.flat(), run.screen.text()
                    run.send(F1, 'Read the seed, type confirm, then F2')
                    run.send(b'\x1b', 'your words')
                assert not (session / 'seed-confirmation.json').exists()
            elif name == 'practice confirmation':
                run.wait('Seed draft r1')
                run.type('confirm')
                run.read_until('End of seed')
                before = files(sessions)
                run.send(F2, 'Tinkery refused: a simulated session cannot be confirmed')
                assert files(sessions) == before, 'A practice seed was confirmed'
            else:
                if question:
                    run.wait('Keep records local or synced?')
                    run.type('Local.')
                else:
                    run.wait("Shape hasn't asked its next question yet.")
                before = files(sessions)
                expected = {
                    'zero budget': "No drafting runs are left: this launch's allowance is 0.",
                    'model failure': 'Your answer is saved. Looking into the next question failed: Pi exited',
                    'helper refusal': "Your answer is saved. The model's reply was refused, so nothing from it was saved: receipt appears to contain a secret",
                    'model tries to confirm': "Looking into the next question failed: Reply field next must be question or seed; a model reply cannot confirm a seed.",
                    'model writes decisions': "field confirming / Problem statement, line 2: ## Decisions (from the ledger)",
                    'seed secret': "The model's reply was refused, so nothing from it was saved: receipt appears to contain a secret",
                }[name]
                run.send(F2, expected)
                assert run.flat().count('Next:') == 1, run.screen.text()
                ledger = helper.load(session)
                if name == 'zero budget':
                    run.wait('Next: add runs from F1, Model and runs')
                    run.wait('your answer Local.')
                    assert files(sessions) == before and run.calls() == [], 'Zero budget wrote or called'
                elif name in ('model writes decisions', 'seed secret'):
                    assert not (session / 'seed-contract.md').exists(), 'A model-written decisions block or a secret was saved'
                else:
                    run.wait('Next: F2 looks into the next question.')
                    assert ledger['nodes'][1]['answer'] == 'Local.', ledger['nodes']
                    assert ledger['current_question'] is None
                    assert not (session / 'seed-contract.md').exists()
                    assert not (session / 'seed-confirmation.json').exists()
            assert helper.load(session)['goal'] == GOAL
            print(f'Shape answering {name} case passed at {width}x{height}')
        finally:
            run.close()


def supported_question_options(width, height, kind):
    """#297: scripted questions prove prompt delivery and preservation, not model judgment."""
    with tempfile.TemporaryDirectory(prefix='tinkery-TEST-297-') as directory:
        cwd = Path(directory).resolve()
        session = make(cwd / 'TEST-sessions', question=False)
        question = {**QUESTION, 'id': 'loan-records', 'label': 'TEST Loan records',
                    'question': 'How should loans and returns be recorded?',
                    'found': [], 'suggestion': None, 'options': []}
        question.pop('supported_by')
        answer = ('A small single-purpose local tool with a simple local file for persistence. '
                  'I should use the tool to record loans and returns, without editing the data file by hand.')
        if kind == 'unknown undo':
            question.pop('options')
            question['question'] = 'What access do readers need? Undo costs are unknown.'
            answer = '  TEST readers use café notes.\r\nKeep the access choice open.  '
        elif kind == 'hard choice':
            question.update(question='Keep TEST notes private or publish them publicly?',
                            suggestion='Keep TEST notes private.', against='TEST public sharing helps readers.',
                            options=[
                                {'choice': 'Private', 'tradeoff': 'TEST readers need permission', 'undo': 'cheap'},
                                {'choice': 'Public', 'tradeoff': 'TEST others can keep copies', 'undo': 'hard'}])
            question['found'] = [{'id': 'public-copies',
                                 'claim': {'type': 'observation', 'scope': 'README.md:1'},
                                 'checked': 'README.md:1',
                                 'observed': 'TEST public copies cannot be recalled'}]
            question['supported_by'] = ['public-copies']
            (cwd / 'README.md').write_text('TEST public copies cannot be recalled.\n')
            answer = 'TEST keep notes private; do not publish them.'
        run = Run(cwd, width, height, budget=2, replies=[question, 'FAIL'])
        try:
            before = files(run.sessions)
            run.open_shape()
            run.wait('F2 look into the next question')
            assert files(run.sessions) == before and run.calls() == [], 'Opening wrote or called'
            run.send(F2, question['question'])
            argv = run.calls()[0]['argv']
            prompt = argv[argv.index('--system-prompt') + 1]
            for rule in [
                    'give exactly two options only for a genuine choice',
                    'evidence supporting each trade-off and undo cost',
                    'If properties can coexist or the trade-offs or undo costs are unknown, omit options',
                    'ask only the unresolved operator decision',
                    'never invent an alternative or undo cost']:
                assert rule in prompt, f'Missing #297 prompt rule: {rule}'
            assert 'With a question, give exactly two real options' not in prompt
            recorded = next(n for n in helper.load(session)['nodes'] if n['id'] == 'loan-records')
            if kind == 'hard choice':
                run.wait("This is hard to undo, so what's your instinct?")
                for hidden in ('Option A:', 'Suggestion:', 'Against my suggestion:'):
                    assert hidden not in run.flat(), run.screen.text()
                assert 'recommendation' not in recorded
                run.choose('Show the options', 'Here are the options and my suggestion.')
                run.read_until('Suggestion: Keep TEST notes private.')
                assert len(run.calls()) == 1, 'Revealing a suggestion called the model'
            else:
                assert recorded['question'] == question['question'], recorded
                assert 'hard to undo' not in run.flat() and 'Show the options' not in run.flat(), run.screen.text()
            before = files(run.sessions)
            run.send(b'\x1b[200~' + answer.encode() + b'\x1b[201~', 'your answer')
            assert files(run.sessions) == before and len(run.calls()) == 1, 'Typing wrote or called'
            run.send(F2, 'Your answer is saved. Looking into the next question failed: Pi exited')
            saved = helper.load(session)
            answered = next(n for n in saved['nodes'] if n['id'] == 'loan-records')
            assert answered['answer'] == answer and answered['authority'] == 'user', answered
            assert len(run.calls()) == 2
            sent = next(n for n in run.calls()[1]['input']['ledger']['nodes'] if n['id'] == 'loan-records')
            assert sent['answer'] == answer, sent
            assert saved['goal'] == GOAL
            assert not (session / 'seed-contract.md').exists()
            assert not (session / 'seed-confirmation.json').exists()
            run.send(F2, "No drafting runs are left: this launch's allowance is 2.")
            assert len(run.calls()) == 2, 'Exhausted budget called the model'
            print(f'Shape #297 {kind} passed at {width}x{height}: prompt sent, exact answer, 2 stub calls')
        finally:
            run.close()


def prerequisite_questions(width, height):
    """Prepared A/B ledger: recorded ready questions still precede any model step (#286).

    Adjusted from #286 (Claude's call, keeping the documented order): a stub cannot propose B
    while ready A is unanswered, because that existing scheduling rule correctly asks A before calling Pi.
    """
    with tempfile.TemporaryDirectory(prefix='tinkery-TEST-prerequisites-') as directory:
        cwd = Path(directory).resolve()
        session = make(cwd / 'TEST-sessions')
        old = helper.load(session)
        state = helper.editable(old)
        state['current_question'] = None
        state['nodes'].append(node('b', status='unresolved', prerequisites=['storage'],
            label='TEST second choice', question='TEST which format after storage?'))
        helper.publish(session, state, old['version'], 'TEST prepared prerequisite questions')
        run = Run(cwd, width, height, budget=1, replies=[SEED])
        try:
            run.open_shape()
            run.wait('F2 look into the next question')
            before = files(run.sessions)
            run.choose('So far', 'Open questions')
            run.read_until('TEST second choice — waits for: Where records live')
            assert files(run.sessions) == before and run.calls() == []
            run.send(ESC, 'F2 look into the next question')
            run.send(F2, 'Keep records local or synced?')
            assert helper.load(session)['current_question'] == 'storage'
            assert run.calls() == [], 'Promoting A called a model'
            run.type('TEST A stays local')
            run.send(F2, 'TEST which format after storage?')
            saved = helper.load(session)
            assert saved['current_question'] == 'b'
            assert next(n for n in saved['nodes'] if n['id'] == 'storage')['answer'] == 'TEST A stays local'
            assert next(n for n in saved['nodes'] if n['id'] == 'b')['prerequisites'] == ['storage']
            assert run.calls() == [], 'Promoting B called a model'
            run.choose('So far', 'Open questions')
            assert 'waits for:' not in run.flat(), run.screen.text()
            run.send(ESC, 'TEST which format after storage?')
            run.type('TEST B is Markdown')
            run.send(F2, 'Seed draft r1')
            assert len(run.calls()) == 1 and helper.load(session)['goal'] == GOAL
            print(f'Shape prerequisites passed at {width}x{height}: prepared TEST A/B ledger, '
                  'So far names the held prerequisite, A then B without model calls, 1 final stub reply')
        finally:
            run.close()


def typed_findings_review(width, height):
    """Two stub investigations, then an operator-only review before confirmation (#282)."""
    with tempfile.TemporaryDirectory(prefix='tinkery-TEST-typed-findings-') as directory:
        cwd = Path(directory).resolve()
        (cwd / 'README.md').write_text('# TEST\n\nLocal only.\nA later observation conflicts.\n')
        inferred = {'id': 'infer-local', 'claim': {'type': 'inference', 'scope': 'TEST storage path',
                    'limits': 'TEST not runtime verification'}, 'checked': 'README.md:3',
                    'observed': 'TEST the documented path is local', 'supported_by': ['read-local']}
        question = {**QUESTION, 'found': [*QUESTION['found'], inferred],
                    'supported_by': ['read-local', 'infer-local']}
        conflict = {'id': 'conflict', 'claim': {'type': 'observation', 'scope': 'TEST later lines'},
                    'checked': 'README.md:4', 'observed': 'TEST later evidence conflicts with storage',
                    'contradicts': 'storage'}
        seed = {**SEED, 'found': [conflict]}
        session = make(cwd / 'TEST-sessions', question=False)
        run = Run(cwd, width, height, budget=2, replies=[question, seed])
        try:
            run.open_shape()
            run.wait('F2 look into the next question')
            run.send(F2, 'Keep records local or synced?')
            ledger = helper.load(session)
            current = next(n for n in ledger['nodes'] if n['id'] == 'storage')
            assert current['supported_by'] == ['read-local', 'infer-local'], current
            assert current['status'] == 'unresolved' and 'answer' not in current
            facts = [n for n in ledger['nodes'] if n['kind'] == 'fact']
            assert [n['claim']['type'] for n in facts] == ['observation', 'inference'], facts
            assert facts[1]['supported_by'] == ['read-local']
            assert all(n['authority'] == 'evidence' and n['evidence'] for n in facts)
            run.choose('So far', 'TEST the documented path is local')
            run.send(ESC, 'your answer')
            run.type('TEST keep local')
            run.send(F2, 'Current question: review flagged answer')
            run.wait('Keep Where records live?')
            run.wait('Settled answer: TEST keep local')
            run.wait('F2 keep with reason')
            ledger = helper.load(session)
            assert helper.review_flags(ledger['nodes']), ledger
            seed_text = (session / 'seed-contract.md').read_text()
            for text in ['observation; scope:', 'inference; scope: TEST storage path',
                         'limits: TEST not runtime verification', 'supported by: read-local']:
                assert text in seed_text, seed_text
            evidence = seed_text.split('## Evidence and uncertainty\n', 1)[1].split('## Suggested first slice', 1)[0]
            assert evidence.count('TEST the documented path is local') == 1, seed_text
            before = files(run.sessions)
            run.send(F2, "Type why you're keeping this answer first; nothing was saved.")
            assert files(run.sessions) == before and len(run.calls()) == 2
            run.type('TEST the scope still fits')
            # A held keep key is not an operator review; typing and help also save nothing.
            run.send(b'\x1b[1;1:2Q' + F1, 'F2 keeps it unchanged with your reason; no model runs.')
            run.send(b'\x1b', 'your reason TEST the scope still fits')
            assert files(run.sessions) == before and len(run.calls()) == 2
            run.send(F2, 'Seed draft r1')
            run.wait('Answer kept with your reason. No model ran.')
            ledger = helper.load(session)
            kept = next(n for n in ledger['nodes'] if n['id'] == 'storage')
            assert kept['answer'] == 'TEST keep local' and kept['authority'] == 'user', kept
            assert kept['history'][-1]['reason'] == 'TEST the scope still fits', kept
            assert not helper.review_flags(ledger['nodes']), ledger
            run.type('confirm')
            run.read_until('This is not implementation approval.')
            # The handoff paragraph is not the end: confirmation rows follow it.
            run.read_until('End of seed', whole_line=True)
            run.send(F2, 'Seed ready: you confirmed r1, and it still matches.')
            assert helper.status(session)['handoff']['verified'] is True
            assert len(run.calls()) == 2 and helper.load(session)['goal'] == GOAL
            print(f'Shape typed findings and contradiction review passed at {width}x{height}: '
                  '2 stub calls, F2 keep-with-reason, unchanged answer, verified seed')
        finally:
            run.close()


def answer_actions(width, height):
    """Explicit deferral, grounded delegation and cascading revision, with stub replies only."""
    for action in ['defer', 'delegate', 'no facts', 'no suggestion', 'revise', 'revise draft', 'revise zero budget']:
        with tempfile.TemporaryDirectory(prefix='tinkery-TEST-answer-actions-') as directory:
            cwd = Path(directory).resolve()
            (cwd / 'README.md').write_text('# TEST\n\nLocal only.\n')
            session = make(cwd / 'TEST-sessions')
            old = helper.load(session)
            state = helper.editable(old)
            current = state['nodes'][1]
            if action in ['delegate', 'no facts', 'no suggestion']:
                if action != 'no suggestion':
                    current['recommendation'] = 'TEST choose local from the evidence.'
                if action != 'no facts':
                    state['nodes'].append({'id': 'test-grounding', 'kind': 'fact', 'status': 'settled',
                        'prerequisites': [], 'claim': {'type': 'observation', 'scope': 'TEST README'},
                        'answer': 'TEST local only', 'authority': 'evidence',
                        'authority_source': 'TEST fixture', 'evidence': ['TEST README says local only']})
                    current['supported_by'] = ['test-grounding']
            if action.startswith('revise'):
                current.update(status='settled', answer='TEST old local', authority='user',
                               authority_source='TEST fixture, not operator approval')
                state['current_question'] = None
                dependents = [('child', 'storage'), ('grandchild', 'child')] if action == 'revise' else []
                for identifier, parent in dependents:
                    state['nodes'].append(node(identifier, prerequisites=[parent], status='settled',
                        question=f'TEST answer {identifier} again?', answer=f'TEST old {identifier}',
                        authority='user', authority_source='TEST fixture, not operator approval'))
                state['nodes'].append({'id': 'test-conflict', 'kind': 'fact', 'status': 'settled',
                    'prerequisites': [], 'claim': {'type': 'observation', 'scope': 'TEST conflict'},
                    'answer': 'TEST conflicts with old storage', 'authority': 'evidence',
                    'authority_source': 'TEST fixture', 'evidence': ['TEST conflicting observation'],
                    'contradicts': 'storage'})
            helper.publish(session, state, old['version'], 'TEST action fixture')
            if action == 'revise draft':
                (session / 'seed-contract.md').write_text('status: draft\n# TEST old draft\nrevision: r1\n\n'
                    + helper.decisions_block(helper.load(session)))
            replies = [SEED] if action.startswith('revise') else [QUESTION, SEED]
            run = Run(cwd, width, height, budget=0 if action == 'revise zero budget' else 2, replies=replies)
            try:
                run.open_shape()
                run.wait('F1 more')
                before = files(run.sessions)
                if action == 'defer':
                    run.type('TEST untouched answer')
                    run.choose('Defer', 'reason to defer')
                    run.wait('Gate: TEST choice')
                    run.type('TEST wait for scope')
                    run.send(F2, 'revisit condition')
                    run.send(F2, 'deferral requires reason and revisit condition')
                    assert files(run.sessions) == before and run.calls() == []
                    run.send(b'\t', 'reason to defer TEST wait for scope')
                    run.send(b'\t', 'revisit condition')
                    run.type('TEST before implementation')
                    run.send(F2, 'Suggestion: Local, matching the README.')
                    saved = helper.load(session)
                    deferred = next(n for n in saved['nodes'] if n['id'] == 'storage')
                    assert deferred['status'] == 'deferred', deferred
                    assert deferred['defer_reason'] == 'TEST wait for scope'
                    assert deferred['revisit_condition'] == 'TEST before implementation'
                    assert deferred.get('answer') is None and not (session / 'seed-confirmation.json').exists()
                    assert len(run.calls()) == 1
                elif action == 'delegate':
                    run.choose('Delegate', 'Suggestion: Local, matching the README.')
                    delegated = next(n for n in helper.load(session)['nodes'] if n['id'] == 'storage')
                    assert delegated['authority'] == 'delegated' and delegated['answer'] == 'TEST choose local from the evidence.', delegated
                    assert delegated['recommendation'] == current['recommendation'], delegated
                    assert delegated['evidence'] == ['TEST README says local only'], delegated
                    assert "'Delegate to the suggestion'" in delegated['authority_source'], delegated
                    assert len(run.calls()) == 1
                elif action in ['no facts', 'no suggestion']:
                    run.type('TEST retain my answer')
                    run.choose('Delegate', 'no supporting facts' if action == 'no facts' else 'no suggestion')
                    run.wait('your answer TEST retain my answer')
                    assert files(run.sessions) == before and run.calls() == []
                elif action in ['revise draft', 'revise zero budget']:
                    run.choose('Revise', 'F2 save revision and look')
                    run.type('TEST revised synced only with permission')
                    if action == 'revise zero budget':
                        run.send(F2, 'No drafting runs are left')
                        run.wait('TEST revised synced only with permission')
                        assert files(run.sessions) == before and run.calls() == []
                    else:
                        run.send(F2, 'Seed draft r2')
                        saved = helper.load(session)
                        assert saved['goal'] == GOAL and saved['current_question'] is None
                        seed = (session / 'seed-contract.md').read_text()
                        assert helper.decisions_block(saved) in seed and 'TEST revised synced only with permission' in seed
                        assert len(run.calls()) == 1, 'Revision without dependents did not take the normal next step'
                else:
                    run.choose('Revise', 'F2 save revision and look')
                    run.type('TEST revised synced only with permission')
                    run.send(F2, 'TEST answer child again?')
                    saved = helper.load(session)
                    assert saved['goal'] == GOAL and saved['current_question'] == 'child'
                    for identifier in ['child', 'grandchild']:
                        reopened = next(n for n in saved['nodes'] if n['id'] == identifier)
                        assert reopened['status'] == 'unresolved' and reopened.get('answer') is None, reopened
                        assert 'storage' in reopened['reopen_reason'], reopened
                    assert run.calls() == [], 'Revision called a model'
                    run.type('TEST new child')
                    run.send(F2, 'TEST answer grandchild again?')
                    assert run.calls() == [], 'Asking a reopened dependent called a model'
                    run.type('TEST new grandchild')
                    run.send(F2, 'Seed draft r1')
                    assert len(run.calls()) == 1
                if action in ['defer', 'delegate']:
                    run.type('TEST answer the next question')
                    run.send(F2, 'Seed draft r1')
                    seed = (session / 'seed-contract.md').read_text()
                    decisions = helper.decisions_block(helper.load(session))
                    assert decisions in seed, seed
                    assert ('storage deferred:' if action == 'defer' else '[delegated]') in decisions, decisions
                    assert len(run.calls()) == 2
                print(f'Shape {action} passed at {width}x{height}: TEST roots, {len(run.calls())} stub calls')
            finally:
                run.close()


def closing_summary(width, height):
    """Ledger risks appear above confirmation, and F2 records an exact skip without a call."""
    with tempfile.TemporaryDirectory(prefix='tinkery-TEST-closing-summary-') as directory:
        cwd = Path(directory).resolve()
        session = make(cwd / 'TEST-sessions', question=False)
        old = helper.load(session)
        state = helper.editable(old)
        for identifier, answer, recommendation, authority in [
            ('changed', 'TEST local', None, 'user'),
            ('delegated', 'TEST suggested', 'TEST suggested', 'delegated'),
            ('accepted', 'TEST yes', 'TEST yes', 'user'),
            ('chosen', 'TEST yes ', 'TEST yes', 'user')]:
            choice = node(identifier, status='settled', answer=answer, authority=authority,
                          authority_source='TEST fixture only', label=identifier,
                          evidence=['TEST grounding'] if authority == 'delegated' else [])
            if recommendation:
                choice['recommendation'] = recommendation
            state['nodes'].append(choice)
        state['assumed'] = [{'text': 'TEST Markdown', 'why': 'TEST either format works'}]
        helper.publish(session, state, old['version'], 'TEST choices')
        old = helper.load(session)
        state = helper.editable(old)
        next(n for n in state['nodes'] if n['id'] == 'changed')['answer'] = 'TEST remote'
        helper.publish(session, state, old['version'], 'TEST reversed choice')
        run = Run(cwd, width, height, budget=1, replies=[{**SEED, 'found': []}])
        try:
            run.open_shape()
            run.wait('F2 look into the next question')
            run.send(F1, 'F1 close')
            assert 'Teach back' not in run.flat(), run.screen.text()
            run.send(ESC, 'F2 look into the next question')
            run.send(F2, 'Seed draft r1')
            for line in ['Before you confirm', 'Accepted as suggested: 2; chosen otherwise: 3.',
                         'changed: TEST remote — reverses an earlier answer',
                         'delegated: TEST suggested — delegated; accepted exactly as suggested',
                         'Assumed: TEST Markdown — TEST either format works',
                         'accepted: TEST yes — accepted exactly as suggested']:
                run.read_until(line)
            run.wait('F1 more')
            assert len(run.calls()) == 1
            argv = run.calls()[0]['argv']
            prompt = argv[argv.index('--system-prompt') + 1]
            for rule in ['Before drafting in this same step', 'Separate decided', 'one case that must fail']:
                assert rule in prompt, rule
            run.type('confirm')
            run.read_until('End of seed', whole_line=True)
            run.send(F2, 'Seed ready: you confirmed r1, and it still matches.')
            saved = helper.load(session)
            origin = next(n for n in saved['nodes'] if n['id'] == saved['origin'])
            assert origin['evidence'][-1]['observed'] == 'Skipped teach-back.'
            assert origin['evidence'][-1]['artifact'] == 'tinkery:teach-back/skipped'
            assert saved['goal'] == GOAL and len(run.calls()) == 1
            assert helper.status(session)['handoff']['verified'] is True
            print(f'Shape confirmation summary + skip passed at {width}x{height}: 1 drafting stub call, no confirmation call')
        finally:
            run.close()


def completion_review_display(width, height, done=False, replay_299=False):
    """Drafting review survives saving and appears as guesses above F2 confirming, without extra calls."""
    with tempfile.TemporaryDirectory(prefix='tinkery-TEST-review-display-') as directory:
        cwd = Path(directory).resolve()
        session = make(cwd / 'TEST-sessions', question=False)
        checks = SEED['checks']
        cases = [('TEST private text appears', checks[0]['statement'])]
        if replay_299:
            cases = [
                ('A saved loan must not disappear after a real close and reopen.',
                 'The list survives closing and reopening.'),
                ('Attempting to save a loan with one required field missing must fail or be rejected.',
                 'The tool records a title, borrower, and date.'),
                # Retain this replay harness's confirmed criterion alongside the historical cases.
                ('TEST private text appears', SEED['checks'][0]['statement']),
            ]
            checks = [dict(statement=statement, kind='judge', source='TEST issue #299 replay',
                           how_to_judge='Inspect the fictional loan tool.') for _, statement in cases]
        review = completion_review(checks)
        review['decided'] = ['TEST café scope.\r\nKeep words.  ']
        review['done'] = ['TEST README observed.\r\nNo checks ran.  '] if done else []
        review['failure_cases'] = [case for case, _ in cases]
        reply = {**SEED, 'found': [], 'review': review, 'checks': checks}
        run = Run(cwd, width, height, budget=1, replies=[reply])
        try:
            run.open_shape()
            run.wait('F2 look into the next question')
            run.send(F2, 'Seed draft r1')
            seed = (session / 'seed-contract.md').read_bytes()
            saved = helper.load(session)
            origin = next(n for n in saved['nodes'] if n['id'] == saved['origin'])
            receipt = next(r for r in origin['evidence'] if r.get('artifact') == 'tinkery:review/completed')
            expected_review = {**review, 'failure_cases': [dict(case=case, statement=statement) for case, statement in cases]}
            assert json.loads(receipt['observed']) == expected_review, receipt
            assert receipt['checked'] == 'Review r1 · ' + hashlib.sha256(seed).hexdigest(), receipt
            run.read_until('This is not implementation approval.')
            for line in [
                'Drafting step guess — decided: TEST café scope. Keep words.',
                ('Drafting step guess — observed done: TEST README observed. No checks ran.' if done
                 else 'Drafting step guess: Nothing observed done.'),
            ] + [f'Must fail: {case} Check: {statement}' for case, statement in cases]:
                run.read_until(line)
                flat = run.flat()
                assert flat.index(line) < flat.index('esc back'), run.screen.text()
                for forbidden in ['Drafting step guess: If ', '., ', '. must fail',
                                  'Must fail: The list survives closing and reopening.']:
                    assert forbidden not in flat, run.screen.text()
                if line.startswith('Must fail:'):
                    rows = run.screen.text().splitlines()
                    case_row = next(i for i, row in enumerate(rows) if row.lstrip().startswith('Must fail:'))
                    check_row = next(i for i, row in enumerate(rows) if row.lstrip().startswith('Check:'))
                    assert case_row < check_row, run.screen.text()
                    assert rows[check_row].index('Check:') > rows[case_row].index('Must fail:'), run.screen.text()
            assert len(run.calls()) == 1 and saved['goal'] == GOAL
            run.type('confirm')
            run.read_until('End of seed', whole_line=True)
            run.send(F2, 'Seed ready: you confirmed r1, and it still matches.')
            assert helper.status(session)['handoff']['verified'] is True
            print(f'Shape completion review display {"#299 exact replay" if replay_299 else "done" if done else "empty done"} passed at {width}x{height}: 1 stub call')
        finally:
            run.close()


def completion_reviews(width, height):
    """Missing or incomplete review refuses before any seed write; a blocker is a question."""
    for kind in ['missing', 'untyped', 'missing case', 'blocker', 'question']:
        with tempfile.TemporaryDirectory(prefix='tinkery-TEST-completion-review-') as directory:
            cwd = Path(directory).resolve()
            session = make(cwd / 'TEST-sessions', question=False)
            reply = {**SEED, 'found': [], 'review': completion_review(SEED['checks'])}
            if kind == 'missing':
                del reply['review']
            elif kind == 'untyped':
                reply['review'] = 'TEST reviewed it'
            elif kind == 'missing case':
                reply['review']['failure_cases'] = []
            elif kind == 'blocker':
                reply['review']['contradictions'] = ['TEST choices conflict']
            else:
                reply = {**QUESTION, 'question': 'TEST choices conflict; local or synced?'}
            run = Run(cwd, width, height, budget=2, replies=[reply, {**SEED, 'found': []}])
            try:
                run.open_shape()
                run.wait('F2 look into the next question')
                before = files(run.sessions)
                expected = {'missing': 'needs a typed completion review', 'untyped': 'needs a typed completion review',
                            'missing case': 'exactly one failure case', 'blocker': 'Return it as a question',
                            'question': 'TEST choices conflict; local or synced?'}[kind]
                run.send(F2, expected)
                assert len(run.calls()) == 1 and not (session / 'seed-contract.md').exists()
                if kind == 'question':
                    run.type('TEST local')
                    run.send(F2, 'Seed draft r1')
                    assert len(run.calls()) == 2, 'Completion review added a call per draft'
                else:
                    assert files(run.sessions) == before, 'Refused review saved part of a seed'
                print(f'Shape completion review {kind} passed at {width}x{height}: stub replies only')
            finally:
                run.close()


def captured_reply_recovery(width, height):
    """#296: raw historical refusals and an isolated missing source retain answers; F2 recovers."""
    fixtures = REPO / 'tests/fixtures/shape-296'
    cases = [
        ('missing-source.json', 'failure_cases must be a list of strings in check order'),
        ('call-6.json', 'failure_cases must be a list of strings in check order'),
        ('call-7.json', 'failure_cases must be a list of strings in check order'),
        ('call-8.json', 'failure_cases must be a list of strings in check order'),
        ('probe-only', 'probe answers belong to the ledger'),
        ('source-only', 'Source must name the confirmed goal'),
    ]
    for name, reason in cases:
        raw = json.loads((fixtures / ('missing-source.json' if name in ('source-only', 'probe-only') else name)).read_text())
        if name in ('source-only', 'probe-only'):
            raw['review']['failure_cases'] = [case['case'] for case in raw['review']['failure_cases']]
        if name == 'source-only':
            raw['confirming'] = SEED['confirming']
        elif name == 'probe-only':
            # Isolate the captured prose now that pure check validation runs first.
            raw['checks'], raw['review'] = SEED['checks'], SEED['review']
        with tempfile.TemporaryDirectory(prefix='tinkery-TEST-296-recovery-') as directory:
            cwd = Path(directory).resolve()
            session = make(cwd / 'TEST-sessions')
            run = Run(cwd, width, height, budget=2, replies=[raw, {**SEED, 'found': []}])
            try:
                run.open_shape()
                run.wait('Keep records local or synced?')
                words = 'TEST #296 preserve this answer exactly.'
                run.type(words)
                run.send(F2, reason)
                saved = helper.load(session)
                answer = next(n for n in saved['nodes'] if n['id'] == 'storage')
                assert answer['answer'] == words
                assert saved['current_question'] is None
                assert len(run.calls()) == 1, 'Refusal retried automatically'
                assert not (session / 'seed-contract.md').exists()
                assert not (session / 'seed-checks.json').exists()
                run.send(F2, 'Seed draft r1')
                current = helper.load(session)
                assert next(n for n in current['nodes'] if n['id'] == 'storage') == answer
                assert current['goal'] == GOAL and len(run.calls()) == 2
                assert not (session / 'seed-confirmation.json').exists()
                print(f'Shape #296 {name}: usable refusal, saved answer and F2 recovery passed at {width}x{height}; 2 stub calls')
            finally:
                run.close()


def read_end_revision(width, height):
    """Reading belongs to draft bytes, not editor size or a same-draft teach-back reread."""
    with tempfile.TemporaryDirectory(prefix='tinkery-TEST-read-end-') as directory:
        cwd = Path(directory).resolve()
        session = make(cwd / 'TEST-sessions', question=False)
        reply = json.loads(json.dumps(SEED))
        reply['found'] = []
        reply['confirming']['Downstream handoff'] += '\n' + ''.join(f'TEST draft row {i}.\n' for i in range(60)) + 'TEST-SEED-END'
        run = Run(cwd, width, height, budget=2,
                  replies=[reply, {'mismatches': []}])
        try:
            run.open_shape()
            run.wait('F2 look into the next question')
            run.send(F2, 'Seed draft r1')
            run.wait('pgup/dn read to end')
            assert 'F2 confirm' not in run.screen.text(), run.screen.text()
            seed_before = (session / 'seed-contract.md').read_bytes()
            before = files(session)
            run.send(F2, "PgDn or Down to 'End of seed' first. Nothing was confirmed.")
            assert files(session) == before and len(run.calls()) == 1
            assert b'End of seed' not in seed_before
            assert 'Next: type your own words' not in run.screen.text()
            run.read_until('TEST-SEED-END')
            run.read_until('End of seed', whole_line=True)
            assert 'F2 confirm seed r1' in run.screen.text(), run.screen.text()
            assert 'read to end' not in run.screen.text(), run.screen.text()
            assert sum(line.strip() == 'End of seed' for line in run.screen.text().splitlines()) == 1
            assert (session / 'seed-contract.md').read_bytes() == seed_before
            # The marker follows the risk summary too; grow the editor without another scroll.
            words = 'TEST I confirm r1. ' + 'TEST read already. ' * 30 + 'TEST exact end.'
            os.write(run.master, b'\x1b[200~' + words.encode() + b'\x1b[201~')
            run.wait('TEST read already.')
            # Read satisfaction and typed words survive resizing, even with a taller editor.
            resized_width, resized_height = (100, 30) if width == 80 else (80, 24)
            run.width = resized_width
            run.screen = Screen(resized_width, resized_height)
            fcntl.ioctl(run.slave, termios.TIOCSWINSZ, struct.pack('HHHH', resized_height, resized_width, 0, 0))
            run.wait('F2 confirm seed r1')
            assert files(session) == before and len(run.calls()) == 1
            run.choose('Teach back', 'your teach-back')
            run.send(F2, 'Teach-back compared. Mismatches never block confirming.')
            run.wait('F2 confirm seed r1')
            assert (session / 'seed-contract.md').read_bytes() == seed_before
            # No page key after typing or the fresh read of the same revision.
            run.send(F2, 'Type confirm, or F1 Revise the draft. Nothing was confirmed.')
            run.send(b'\x01\x1b[1;2F' + b'confirm', 'your words confirm')
            run.send(F2, 'Seed ready: you confirmed r1, and it still matches.')
            confirmation = json.loads((session / 'seed-confirmation.json').read_text())
            assert confirmation['source'] == 'confirm'
            assert helper.load(session)['goal'] == GOAL
            assert helper.status(session)['handoff']['verified'] is True
            assert len(run.calls()) == 2
            print(f'Shape reading hint and read-to-end retained through typing, resize and same-r1 reread at {width}x{height}, NO_COLOR={"NO_COLOR" in os.environ}; 2 stub calls')
        finally:
            run.close()


def sdlc_root_read_folder(width, height):
    """Without a Think origin, Shape reads the named SDLC checkout's repository, not the launch folder (#467)."""
    with tempfile.TemporaryDirectory(prefix='tinkery-TEST-sdlc-root-folder-') as directory:
        cwd = Path(directory).resolve()
        make(cwd / 'TEST-sessions')
        checkout = cwd / 'TEST-checkout'
        (checkout / '.git').mkdir(parents=True)
        (checkout / 'TEST-sub').mkdir()
        before = files(cwd / 'TEST-sessions')
        run = Run(cwd, width, height, budget=1, sdlc_root=checkout / 'TEST-sub')
        try:
            run.open_shape()
            run.wait('The step reads files in TEST-checkout. F1 picks another folder.')
            assert files(cwd / 'TEST-sessions') == before and run.calls() == [], 'Shape wrote or called before F2'
            print(f'Shape reads the --sdlc-root repository at {width}x{height}; 0 stub calls')
        finally:
            run.close()


def teach_back(width, height, exhausted=False):
    """Capture is explicit and budget-gated; even reported mismatches leave confirming available."""
    with tempfile.TemporaryDirectory(prefix='tinkery-TEST-teach-back-') as directory:
        cwd = Path(directory).resolve()
        session = make(cwd / 'TEST-sessions', question=False)
        target = cwd / 'TEST-repository'
        target.mkdir()
        mismatch = 'TEST sync is outside the draft.\r\nKeep café mismatch words.  '
        run = Run(cwd, width, height, budget=1 if exhausted else 2,
                  replies=[{**SEED, 'found': []}, {'mismatches': [mismatch]}])
        try:
            run.open_shape()
            run.wait('F2 look into the next question')
            run.choose('Choose the folder', 'Next: F2 uses this folder')
            run.send(b'/TEST-repository' + F2, 'The step reads files in TEST-repository.')
            run.send(F2, 'Seed draft r1')
            run.wait('F1 more')
            run.type('confirm')
            before = files(run.sessions)
            run.choose('Teach back', 'your teach-back')
            run.wait('your teach-back confirm')
            run.send(b'\x01cancel me\x1b', 'Teach-back cancelled. Nothing was saved.')
            run.wait('your words confirm')
            assert files(run.sessions) == before and len(run.calls()) == 1
            run.choose('Teach back', 'your teach-back')
            run.wait('your teach-back confirm')
            words = 'TEST build sync.\r\nNo deletes.  '
            # Select the prefill and replace it; the separate confirmation editor stays intact.
            run.send(b'\x01\x1b[1;2F\x1b[200~' + words.encode() + b'\x1b[201~', 'No deletes.')
            run.send(F1, 'F2 saves and compares (one drafting run)')
            run.send(b'\x1b', 'your teach-back')
            assert files(run.sessions) == before and len(run.calls()) == 1
            if exhausted:
                run.send(F2, 'No drafting runs are left')
                assert files(run.sessions) == before and len(run.calls()) == 1
                run.send(b'\x1b', 'Teach-back cancelled.')
            else:
                run.send(b'\x1b[1;1:2Q' + F2, 'Teach-back compared. Mismatches never block confirming.')
                run.wait('pgup/dn read to end')
                run.wait('your words confirm')
                run.read_until('Teach-back mismatch (guess): TEST sync is outside the draft.')
                saved = helper.load(session)
                origin = next(n for n in saved['nodes'] if n['id'] == saved['origin'])
                assert [r['observed'] for r in origin['evidence'] if r.get('artifact', '').startswith('tinkery:teach-back/')] == [words, mismatch, 'Teach-back compared.']
                assert len(run.calls()) == 2
                assert '--no-tools' in run.calls()[1]['argv'] and '--tools' not in run.calls()[1]['argv']
                assert run.calls()[1]['guard'] is None, run.calls()[1]
                assert run.calls()[0]['cwd'] == str(target), run.calls()
                assert run.calls()[1]['cwd'] == str(cwd), run.calls()
                assert run.calls()[1]['input'] == {'kind': 'teach-back',
                    'draft': (session / 'seed-contract.md').read_text(), 'operator_words': words}
            run.read_until('End of seed', whole_line=True)
            run.send(F2, 'Seed ready: you confirmed r1, and it still matches.')
            saved = helper.load(session)
            origin = next(n for n in saved['nodes'] if n['id'] == saved['origin'])
            if exhausted:
                assert origin['evidence'][-1]['observed'] == 'Skipped teach-back.'
            else:
                assert [r['observed'] for r in origin['evidence'] if r.get('artifact', '').startswith('tinkery:teach-back/')] == [words, mismatch, 'Teach-back compared.']
            assert saved['goal'] == GOAL and helper.status(session)['handoff']['verified'] is True
            assert len(run.calls()) == (1 if exhausted else 2)
            run.send(F1, 'F1 close')
            assert 'Teach back' not in run.flat(), run.screen.text()
            print(f'Shape teach-back {"zero budget" if exhausted else "mismatch + F2"} passed at {width}x{height}')
        finally:
            run.close()


def revise_the_draft(width, height):
    """Words typed where confirm goes never confirm; F1 Revise the draft saves them and redrafts (#614)."""
    with tempfile.TemporaryDirectory(prefix='tinkery-TEST-revise-draft-') as directory:
        cwd = Path(directory).resolve()
        session = make(cwd / 'TEST-sessions', question=False)
        run = Run(cwd, width, height, budget=2, replies=[{**SEED, 'found': []}, {**SEED, 'found': []}])
        try:
            run.open_shape()
            run.wait('F2 look into the next question')
            run.send(F2, 'Seed draft r1')
            run.read_until('End of seed', whole_line=True)
            change = 'TEST keep the cache local, not shared'
            run.type(change)
            before = files(run.sessions)
            run.send(F2, 'Type confirm, or F1 Revise the draft. Nothing was confirmed.')
            assert files(run.sessions) == before and len(run.calls()) == 1, 'A correction confirmed or called'
            assert not (session / 'seed-confirmation.json').exists()
            run.choose('Revise the draft', 'your change ' + change)
            run.wait('F2 save + redraft')
            run.send(F2, 'Seed draft r2')
            ledger = helper.load(session)
            node = next(n for n in ledger['nodes'] if n['id'] == 'change-to-r1')
            assert (node['status'], node['answer'], node['authority']) == ('settled', change, 'user'), node
            assert ledger['goal'] == GOAL and len(run.calls()) == 2
            assert change in json.dumps(run.calls()[1]['input']), 'The redraft step never saw the change'
            assert f'- change-to-r1 [user] {change}' in (session / 'seed-contract.md').read_text()
            assert not (session / 'seed-confirmation.json').exists(), 'Revising confirmed the seed'
            print(f'Shape revise the draft passed at {width}x{height}: refused correction, 2 stub calls, r2 records the change')
        finally:
            run.close()


def teach_back_retry(width, height):
    """A malformed comparison spends one call, keeps words, and retries only on another key."""
    with tempfile.TemporaryDirectory(prefix='tinkery-TEST-teach-back-retry-') as directory:
        cwd = Path(directory).resolve()
        session = make(cwd / 'TEST-sessions', question=False)
        run = Run(cwd, width, height, budget=3,
                  replies=[{**SEED, 'found': []}, {'mismatches': [42]}, {'mismatches': []}])
        try:
            run.open_shape()
            run.wait('F2 look into the next question')
            run.send(F2, 'Seed draft r1')
            run.type('confirm')
            run.choose('Teach back', 'your teach-back')
            run.wait('your teach-back confirm')
            run.send(b'\x01\x1b[1;2F', 'your teach-back')
            run.type('TEST first teach-back')
            run.send(F2, 'The teach-back reply needs a mismatches list.')
            saved = helper.load(session)
            origin = next(n for n in saved['nodes'] if n['id'] == saved['origin'])
            assert [r['observed'] for r in origin['evidence'] if r.get('artifact', '').startswith('tinkery:teach-back/')] == ['TEST first teach-back']
            assert len(run.calls()) == 2
            for parser_text in ['invalid type', 'integer', 'line 1 column']:
                assert parser_text not in run.flat(), run.screen.text()
            run.send(F1, 'Drafting runs left this launch: 1 of 3.')
            run.send(b'\x1b', 'F1 more')
            assert len(run.calls()) == 2, 'A failed comparison retried on its own'
            run.choose('Teach back', 'your teach-back')
            run.wait('your teach-back confirm')
            run.send(b'\x01\x1b[1;2F', 'your teach-back')
            run.type('TEST corrected teach-back')
            run.send(F2, 'Teach-back compared. Mismatches never block confirming.')
            run.read_until('Teach-back guess: no mismatches reported.')
            run.read_until('End of seed', whole_line=True)
            run.send(F2, 'Seed ready: you confirmed r1, and it still matches.')
            saved = helper.load(session)
            origin = next(n for n in saved['nodes'] if n['id'] == saved['origin'])
            assert [r['observed'] for r in origin['evidence'] if r.get('artifact', '').startswith('tinkery:teach-back/')] == [
                'TEST first teach-back', 'TEST corrected teach-back', 'Teach-back compared.']
            assert len(run.calls()) == 3 and saved['goal'] == GOAL
            print(f'Shape teach-back failure + explicit retry passed at {width}x{height}: exactly 3 stub calls')
        finally:
            run.close()


def exit_hint(width, height, stage):
    """An old goal receipt follows current files, not its save-time stage; all calls are stubs."""
    with tempfile.TemporaryDirectory(prefix='tinkery-TEST-exit-hint-') as directory:
        cwd = Path(directory).resolve()
        situation = 'For the first slice' if stage == 'goal' else 'for the first slice'
        headline = ('Keep refusals calm, for me' + ('. ' if stage == 'goal' else ', ')
                    + situation + '.')
        run = Run(cwd, width, height, budget=5, replies=[{**SEED, 'found': []}], situation=situation)
        try:
            run.wait('1  think')
            run.send(b'1', "What's on your mind?")
            run.type('TEST calm refusals, first slice.')
            run.send(F2, 'May refusals show private text?')
            run.type('never')
            run.send(F2, "Ready for your review")
            run.wait(headline)
            run.choose('Review the goal', 'You are confirming')
            run.send(b'confirm\r', 'Saved.')
            session = next(run.sessions.iterdir())
            goal = helper.load(session)['goal']
            assert goal.splitlines()[0] == headline, goal
            run.send(ESC, '3  shape')
            if stage != 'goal':
                run.open_shape()
                run.wait('F2 look into the next question')
                run.send(F2, 'Probe: pre-mortem')
                # The skip the screen names is a settled answer, not a deferral (#625).
                run.type('skip')
                run.send(F2, 'Seed draft r1')
                probe = next(n for n in helper.load(session)['nodes'] if n['id'] == 'probe-pre-mortem')
                assert (probe['status'], probe['answer']) == ('settled', 'skip'), probe
                if stage in ('ready', 'stale'):
                    helper.confirm_seed(session, 'r1', 'TEST fixture only: confirm r1')
                if stage == 'stale':
                    seed = session / 'seed-contract.md'
                    seed.write_bytes(seed.read_bytes() + b'\nTEST changed after confirmation\n')
                if stage == 'unreadable':
                    (session / 'ledger.json').write_text('TEST invalid JSON')
                run.menu()
            before = files(run.sessions)
            os.write(run.master, b'q')
            output = bytearray()
            deadline = time.monotonic() + 10
            def consume_wait_output(chunk):
                output.extend(chunk)
            for returncode in wait_output(run.master, run.process, consume_wait_output, deadline):
                if returncode is not None:
                    break
            assert run.process.poll() == 0, output
            text = output.decode().replace('\r\n', '\n')
            expected = {'goal': 'Next: shape it into a seed in Shape.',
                        'draft': 'Next: confirm the seed draft in Shape.',
                        'ready': 'Next: take the confirmed seed to Approve.',
                        'stale': 'Next: check this session in Shape.',
                        'unreadable': 'Next: check this session in Shape.'}[stage]
            assert expected in text, text
            assert text.count('Next:') == 1 and goal in text, text
            assert all(word not in text for word in ('Invalid JSON', 'line 1 column', 'Error(')), text
            assert files(run.sessions) == before, 'Exit changed session files'
            assert len(run.calls()) == (0 if stage == 'goal' else 1)
            print(f'Exit hint {stage} + situation spelling passed at {width}x{height}: stub replies only')
        finally:
            run.close()


def complete_scenario_recovery(width, height):
    """#298: weakened scenario refuses without writes; an explicit retry retains the whole scenario."""
    scenario = 'I can lend two books, return one, restart, and see only the other still out.'
    goal = f'I track loans.\nWhy: I need to know what is out.\nDone when: {scenario}'
    def check(statement):
        return dict(statement=statement, kind='provisional', source='TEST confirmed goal',
                    expect='The complete scenario passes.', cwd='.', missing='No scenario test exists yet.')
    def reply(checks):
        return {**SEED, 'found': [], 'checks': checks, 'review': completion_review(checks)}
    with tempfile.TemporaryDirectory(prefix='tinkery-TEST-298-') as directory:
        cwd = Path(directory).resolve()
        session = make(cwd / 'TEST-sessions', question=False, goal=goal)
        split = [check('The list survives closing and reopening.'),
                 check('Only outstanding books are shown after one return.')]
        complete = [check(scenario)]
        run = Run(cwd, width, height, budget=2, replies=[reply(split), reply(complete)])
        try:
            run.open_shape()
            run.wait('F2 look into the next question')
            before = files(run.sessions)
            run.send(F2, 'No seed was drafted')
            assert files(run.sessions) == before
            assert len(run.calls()) == 1, 'Refusal retried automatically'
            run.send(F2, 'Seed draft r1')
            assert helper.read_checks(session) == complete
            assert helper.load(session)['goal'] == goal
            assert scenario in (session / 'seed-contract.md').read_text()
            assert len(run.calls()) == 2
            assert not (session / 'seed-confirmation.json').exists()
            print(f'Shape #298 complete scenario: refusal without writes and explicit recovery at {width}x{height}; 2 stub calls')
        finally:
            run.close()


def missing_success_before_calls(width, height, no_color):
    """#314: the captured saved goal names its missing success criterion before any provider call."""
    previous = os.environ.get('NO_COLOR')
    if no_color:
        os.environ['NO_COLOR'] = '1'
    else:
        os.environ.pop('NO_COLOR', None)
    try:
        with tempfile.TemporaryDirectory(prefix='tinkery-TEST-314-') as directory:
            cwd = Path(directory).resolve()
            session = cwd / 'TEST-sessions/TEST-captured'
            session.mkdir(parents=True)
            original = (REPO / 'tests/fixtures/shape-296/ledger.json').read_bytes()
            (session / 'ledger.json').write_bytes(original)
            run = Run(cwd, width, height, budget=1, replies=[])
            try:
                before = files(run.sessions)
                run.open_shape()
                run.wait('The saved goal leaves Done when open.')
                run.wait('F2 look into the next question')
                run.menu()
                assert run.calls() == [], 'Opening or leaving Shape called a provider'
                assert files(run.sessions) == before, 'Shape changed the saved goal or ledger'
                print(f'Shape #314 missing Done when named at {width}x{height} NO_COLOR={no_color}; zero calls/writes')
            finally:
                run.close()
    finally:
        if previous is None:
            os.environ.pop('NO_COLOR', None)
        else:
            os.environ['NO_COLOR'] = previous


if __name__ == '__main__':
    # Most journeys run once at 80x24, the smallest supported size; the core flows run at both sizes.
    small = [(80, 24)]
    both = [(100, 30), (80, 24)]
    for size in small:
        for kind in ['compatible properties', 'unknown undo', 'hard choice']:
            supported_question_options(*size, kind)
    for no_color in [False, True]:
        missing_success_before_calls(80, 24, no_color)
    for size in small:
        complete_scenario_recovery(*size)
    previous_color = os.environ.get('NO_COLOR')
    try:
        for no_color in [False, True]:
            if no_color:
                os.environ['NO_COLOR'] = '1'
            else:
                os.environ.pop('NO_COLOR', None)
            read_end_revision(80, 24)
    finally:
        if previous_color is None:
            os.environ.pop('NO_COLOR', None)
        else:
            os.environ['NO_COLOR'] = previous_color
    for size in small:
        for stage in ('goal', 'draft', 'ready', 'stale', 'unreadable'):
            exit_hint(*size, stage)
        completion_review_display(*size)
        completion_review_display(*size, done=True)
        completion_review_display(*size, replay_299=True)
        teach_back_retry(*size)
        revise_the_draft(*size)
        closing_summary(*size)
        completion_reviews(*size)
        captured_reply_recovery(*size)
        teach_back(*size)
        teach_back(*size, exhausted=True)
        prerequisite_questions(*size)
    for name in ['settled prerequisites', 'unknown prerequisite', 'cyclic prerequisite']:
        case(name)
    for size in both:
        sdlc_root_read_folder(*size)
        answer_actions(*size)
    for size in small:
        typed_findings_review(*size)
    for size in both:
        full_journey(*size)
    for name in ['zero budget', 'stale', 'model failure', 'stop', 'helper refusal', 'model tries to confirm',
                 'model writes decisions', 'seed secret', 'practice confirmation', 'unfound citation', 'nothing found',
                 'end the interview', 'assumptions', 'hard to undo']:
        case(name)
    for size in small:
        case('multiline confirmation', *size)
        os.environ['NO_COLOR'] = '1'
        case('confirmation hint', *size)
        case('confirmation Enter', *size)
