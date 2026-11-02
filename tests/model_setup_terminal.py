"""The Model panel through real keys (#422, #449): saving, later launches, flags, top-up, refusals
and the Claude Code harness.

Fake Pi and Claude Code on PATH, temporary TEST homes and roots, no providers.
"""
import json
import os
from pathlib import Path
import tempfile

from think_history_terminal import Terminal

HOST = '''#!/usr/bin/env python3
import json, pathlib, sys
if sys.argv[1:] == ['--list-models', '--offline']:
    with pathlib.Path('TEST-lists.jsonl').open('a') as log:
        log.write('{}\\n')
    print('provider  model         context')
    print('test      listed-one    1M  ')
    print('test      listed-two    1M  ')
    sys.exit(0)
request = json.load(sys.stdin)
model = sys.argv[sys.argv.index('--model') + 1]
thinking = sys.argv[sys.argv.index('--thinking') + 1]
with pathlib.Path('TEST-calls.jsonl').open('a') as log:
    log.write(json.dumps(dict(model=model, thinking=thinking)) + '\\n')
if request.get('kind') == 'decision-review':
    print('{"decisions":[]}')
else:
    print(json.dumps(dict(parts=dict(who='I', outcome='keep TEST setup words',
        why='the setup matters', done_when='the saved model drafts'),
        open=['why'], questions=[dict(id='reason', target='why', text='Which TEST reason?')])))
'''
# Claude Code starts in a clean environment and an empty folder, so the log path is filled in.
CLAUDE = '''#!/usr/bin/env python3
import json, os, sys
request = json.load(sys.stdin)
argv = sys.argv[1:]
flag = lambda name: argv[argv.index(name) + 1]
with open(LOG, 'a') as log:
    log.write(json.dumps(dict(model=flag('--model'), thinking=flag('--effort'), tools=flag('--tools'),
        files=os.listdir('.'), env=sorted(os.environ))) + '\\n')
if request.get('kind') == 'decision-review':
    reply = dict(decisions=[])
else:
    reply = dict(parts=dict(who='I', outcome='keep TEST claude words', why='the harness matters',
        done_when='Claude Code drafts'), open=['why'],
        questions=[dict(id='reason', target='why', text='Which TEST claude reason?')])
print(json.dumps(dict(type='result', is_error=False, result=json.dumps(reply),
    modelUsage={flag('--model'): {}})))
'''
F1, F2, ESC, DOWN, UP = b'\x1bOP', b'\x1bOQ', b'\x1b', b'\x1b[B', b'\x1b[A'


class Launch:
    def __init__(self, root, home, args, size=(100, 30), operator='human'):
        self.root = root
        bin_dir = root / 'TEST-bin'
        self.terminal = Terminal(root, *size, ['--operator', operator, '--seed-session-root', root / 'TEST-sessions',
                                               '--record-root', root / 'TEST-records', *args],
                                 home=home, extra={'PATH': f"{bin_dir}{os.pathsep}{os.environ['PATH']}"})

    def __enter__(self):
        return self.terminal

    def __exit__(self, *_):
        self.terminal.close()


def calls(root):
    path = root / 'TEST-calls.jsonl'
    return [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []


def header(terminal):
    return terminal.screen.text().splitlines()[1]


def pick(terminal, label):
    """Open F1 and press the digit of the row naming `label`."""
    terminal.send(F1, label)
    terminal.send(terminal.screen.pick(label), 'Model ')


def recipient(terminal, harness, model):
    """Read the current destination beside the separately labeled later settings."""
    pick(terminal, 'Model and runs')
    shown = terminal.screen.text()
    for text in (f'F2 sends to {harness}: {model}.', 'For later launches', 's save', 'esc close'):
        assert text in shown, (text, shown)
    terminal.send(ESC, 'F2 send')


def zero_override(root, home, colour, started):
    """An explicit zero without --shape-pi keeps submitted words in practice."""
    saved = home / '.config/tinkery/model.json'
    before, starts = saved.read_bytes(), started()
    with Launch(root, home, ['--shape-budget', '0', *colour]) as terminal:
        terminal.wait(lambda screen: "What's on your mind?" in screen)
        assert 'practice' in header(terminal), header(terminal)
        terminal.send(b'TEST zero-budget words', 'TEST zero-budget words')
        terminal.send(F2, 'Simulated sensemaking')
        assert 'TEST zero-budget words' in terminal.screen.text()
    assert started() == starts, 'explicit zero started a provider'
    assert saved.read_bytes() == before, 'explicit zero changed the saved setup'


def saved_errors(no_color=False):
    """Saved-file errors describe the file; resolved launch state names the recipient (#583)."""
    with tempfile.TemporaryDirectory(prefix='tinkery-TEST-model-error-', dir='/tmp') as directory:
        root = Path(directory)
        home = root / 'HOME'
        saved = home / '.config/tinkery/model.json'
        saved.parent.mkdir(parents=True)
        (root / 'TEST-bin').mkdir()
        host = root / 'TEST-bin/pi'
        host.write_text(HOST)
        host.chmod(0o700)
        colour = ['--no-color'] if no_color else []
        for error in ['invalid', 'unreadable']:
            if error == 'invalid':
                saved.write_text('not json')
            else:
                saved.unlink()
                saved.mkdir()  # A directory refuses reading even when tests run as root.
            for size in [(80, 24), (100, 30)]:
                for budget in [None, 0, 1]:
                    flags = (['--shape-budget', '0'] if budget is None else
                             ['--shape-pi', '--model', 'test/active', '--shape-budget', str(budget)])
                    with Launch(root, home, ['--home', *flags, *colour], size, 'agent') as terminal:
                        terminal.wait(lambda screen: '1  think' in screen)
                        terminal.send(b'm', 'For later launches')
                        shown = terminal.screen.text()
                        words = ' '.join(shown.replace('│', ' ').split())
                        for text in ['s save', 'esc close']:
                            assert text in shown, (text, shown)
                        expected_error = 'is not a model setup' if error == 'invalid' else 'Cannot read'
                        assert expected_error in words, shown
                        assert 'model.json' in words, shown
                        if budget is None:
                            assert 'This launch: practice; no model requests.' in words, shown
                            assert 'practice' in header(terminal)
                            assert 'F2 sends' not in shown
                        else:
                            assert 'F2 sends to Pi: test/active.' in words, shown
                            assert 'practice' not in words, shown
                            assert f'{budget} run' in header(terminal), header(terminal)
                        assert calls(root) == [] and not (root / 'TEST-lists.jsonl').exists()
                        terminal.send(ESC, '1  think')
                    if error == 'invalid':
                        assert saved.read_text() == 'not json'
                    else:
                        assert saved.is_dir()
        print(f'Saved-model errors no_color={no_color}: 12 PTY cases; zero host starts')


def journey(no_color):
    with tempfile.TemporaryDirectory(prefix='tinkery-TEST-model-') as directory:
        root = Path(directory)
        home = root / 'TEST-home'
        home.mkdir()
        (root / 'TEST-bin').mkdir()
        host = root / 'TEST-bin' / 'pi'
        host.write_text(HOST)
        host.chmod(0o700)
        saved = home / '.config/tinkery/model.json'
        colour = ['--no-color'] if no_color else []

        # No file: practice, unchanged. m opens the panel; esc writes nothing.
        with Launch(root, home, ['--home', *colour]) as terminal:
            terminal.wait(lambda screen: '1  think' in screen)
            assert 'practice' in header(terminal), header(terminal)
            terminal.send(b'm', 'Harness   Pi')
            # Enter lists Pi's models once and then picks through them; t types any model.
            terminal.send(DOWN + b'\r', 'Pi lists 2 models')
            terminal.send(b'\r', 'Model     test/listed-one')
            terminal.send(b'\r', 'Model     test/listed-two')
            terminal.send(b't', 'enter done')
            terminal.send(b'\x7f' * len('test/listed-two') + b'test/saved', 'test/saved')
            terminal.send(b'\r', '↑↓ choose')
            terminal.send(DOWN + DOWN + b'\r', '5 each launch')
            terminal.send(ESC, '1  think')
            assert not saved.exists(), 'esc wrote the setup'
            terminal.send(b'm', 'Model     none')
            terminal.send(DOWN + b'\r', 'Model     test/listed-one')
            terminal.send(b't' + b'\x7f' * len('test/listed-one') + b'test/saved' + b'\r', '↑↓ choose')
            terminal.send(DOWN + b'\r', 'medium')
            terminal.send(DOWN + b'\r', '5 each launch')
            terminal.send(b's', 'drafts with it.')
            assert json.loads(saved.read_text()) == dict(harness='pi', model='test/saved', thinking='medium', calls=5)
            assert 'practice' in header(terminal), 'saving changed this launch'
            terminal.send(ESC, '1  think')
            terminal.quit()
        assert calls(root) == []
        assert len((root / 'TEST-lists.jsonl').read_text().splitlines()) == 1, 'Pi listed more than once'

        zero_override(root, home, colour, lambda: calls(root))

        # A later plain launch drafts with the saved setup; launching starts nothing, F2 sends.
        with Launch(root, home, colour) as terminal:
            terminal.wait(lambda screen: "What's on your mind?" in screen)
            assert '5 runs left' in header(terminal), header(terminal)
            assert calls(root) == []
            terminal.send(b'TEST saved words', 'TEST saved words')
            terminal.send(F2, 'Which TEST reason?')
            assert calls(root) and all(c == dict(model='test/saved', thinking='medium') for c in calls(root)), calls(root)
            terminal.wait(lambda _: f'{5 - len(calls(root))} runs left' in header(terminal))

        # Flags win for one launch and write nothing.
        before = saved.read_text()
        for size in [(80, 24), (100, 30)]:
            for operator in ['human', 'agent']:
                start = len(calls(root))
                with Launch(root, home, ['--shape-pi', '--model', 'test/flag', '--shape-budget', '1', *colour], size, operator) as terminal:
                    terminal.wait(lambda _: '1 run left' in header(terminal))
                    recipient(terminal, 'Pi', 'test/flag')
                    assert len(calls(root)) == start, 'opening the panel started a request'
                    terminal.send(b'TEST flag words', 'TEST flag words')
                    terminal.send(F2, 'Which TEST reason?')
                    assert [c['model'] for c in calls(root)[start:]] == ['test/flag'], calls(root)
                    # The thinking flag was not given, so the saved level applies.
                    assert calls(root)[-1]['thinking'] == 'medium'
        assert saved.read_text() == before

        # Out of calls: words stay, nothing starts; F1's row adds calls for this launch only.
        start = len(calls(root))
        with Launch(root, home, ['--shape-pi', '--model', 'test/flag', '--shape-budget', '0', *colour]) as terminal:
            terminal.wait(lambda _: '0 runs left' in header(terminal))
            terminal.send(b'TEST topped up words', 'TEST topped up words')
            os.write(terminal.master, F2)
            terminal.wait(lambda screen: 'Add runs to keep going.' in ' '.join(screen.split()))
            assert len(calls(root)) == start
            assert 'TEST topped up words' in terminal.screen.text()
            pick(terminal, 'Model and runs')
            terminal.send(DOWN * 4 + b'\r', '5 left · enter adds 5')
            terminal.send(ESC, 'TEST topped up words')
            assert '5 runs left' in header(terminal), header(terminal)
            terminal.send(F2, 'Which TEST reason?')
            assert len(calls(root)) > start
        assert saved.read_text() == before, 'adding calls saved them'

        # An agent never inherits the saved calls, and can't save a setup or add runs.
        with Launch(root, home, colour, operator='agent') as terminal:
            terminal.wait(lambda screen: "What's on your mind?" in screen)
            assert 'practice' in header(terminal), header(terminal)
        with Launch(root, home, ['--shape-pi', '--shape-budget', '0', *colour], operator='agent') as terminal:
            terminal.wait(lambda _: '0 runs left' in header(terminal))
            terminal.send(b'TEST agent words', 'TEST agent words')
            pick(terminal, 'Model and runs')
            terminal.wait(lambda screen: '--shape-budget CALLS' in ' '.join(screen.replace('│', ' ').split()))
            terminal.send(b's', "can't save a setup or add runs")
            terminal.send(DOWN * 4 + b'\r', '0 left · enter adds 5')
            assert '0 runs left' in header(terminal)
            terminal.send(ESC, 'TEST agent words')
        assert saved.read_text() == before, 'an agent changed the setup'

        # An invalid file is refused with its path, and the launch stays practice.
        saved.write_text('not json')
        with Launch(root, home, ['--home', *colour]) as terminal:
            terminal.wait(lambda screen: '1  think' in screen)
            assert 'practice' in header(terminal)
            shown = terminal.send(b'm', 'is not a model setup')
            assert 'This launch: practice; no model requests.' in ' '.join(shown.split()), shown
            terminal.send(ESC, '1  think')
            terminal.quit()
        print(f'Model panel no_color={no_color}: {len(calls(root))} fake Pi starts')
        claude_code(root, colour)


def claude_code(root, colour):
    """Claude Code is picked, saved and drafts with no tools in an empty folder (#449)."""
    home = root / 'TEST-claude-home'
    home.mkdir()
    saved = home / '.config/tinkery/model.json'
    log = root / 'TEST-claude-calls.jsonl'
    host = root / 'TEST-bin' / 'claude'
    host.write_text(CLAUDE.replace('LOG', repr(str(log))))
    host.chmod(0o700)
    started = lambda: [json.loads(line) for line in log.read_text().splitlines()] if log.exists() else []
    pi_calls = len(calls(root))

    # The model is picked from a list, never typed, and thinking keeps to Claude Code's levels.
    with Launch(root, home, ['--home', *colour]) as terminal:
        terminal.wait(lambda screen: '1  think' in screen)
        terminal.send(b'm', 'Harness   Pi')
        terminal.send(b'\r', 'claude-sonnet-5-5')
        terminal.send(DOWN + b'\r', 'claude-opus-5-5')
        terminal.send(DOWN + b'\r', 'Thinking  medium')
        terminal.send(DOWN + b'\r', '5 each launch')
        terminal.send(b's', 'drafts with it.')
        assert json.loads(saved.read_text()) == dict(harness='claude-code', model='claude-opus-5-5',
                                                     thinking='medium', calls=5)
        terminal.send(ESC, '1  think')
        terminal.quit()
    assert started() == []

    zero_override(root, home, colour, started)

    # A later launch drafts through Claude Code: no tools, an empty folder, a clean environment.
    with Launch(root, home, colour) as terminal:
        terminal.wait(lambda _: '5 runs left' in header(terminal))
        terminal.send(b'TEST claude words', 'TEST claude words')
        terminal.send(F2, 'Which TEST claude reason?')
    assert started(), 'Claude Code never started'
    for call in started():
        assert (call['model'], call['thinking'], call['tools'], call['files']) == ('claude-opus-5-5', 'medium', '', []), call
        assert not any(name.startswith('TINKERY') for name in call['env']), call['env']
    assert len(calls(root)) == pi_calls, 'Pi started for a Claude Code setup'

    # Flags win for one launch; Pi's own levels and --pi-command are refused for Claude Code.
    before = saved.read_text()
    for size in [(80, 24), (100, 30)]:
        for operator in ['human', 'agent']:
            start = len(started())
            with Launch(root, home, ['--shape-pi', '--model', 'claude-haiku-5-5', '--thinking', 'max', '--shape-budget', '1', *colour], size, operator) as terminal:
                terminal.wait(lambda _: '1 run left' in header(terminal))
                recipient(terminal, 'Claude Code', 'claude-haiku-5-5')
                assert len(started()) == start, 'opening the panel started a request'
                terminal.send(b'TEST flag words', 'TEST flag words')
                terminal.send(F2, 'Which TEST claude reason?')
            assert [(c['model'], c['thinking']) for c in started()[start:]] == [('claude-haiku-5-5', 'max')], started()
    assert saved.read_text() == before
    for args, refusal in [(['--thinking', 'minimal'], 'with Claude Code'),
                          (['--pi-command', 'pi'], '--pi-command applies to Pi'),
                          (['--harness', 'claude'], 'Use harness pi|claude-code')]:
        with Launch(root, home, ['--shape-pi', '--harness', 'claude-code', '--model', 'claude-haiku-5-5', *args]) as terminal:
            terminal.wait(lambda screen: refusal in ' '.join(screen.split()))
    print(f'Claude Code no_color={bool(colour)}: {len(started())} fake Claude Code starts')


if __name__ == '__main__':
    for no_color in [False, True]:
        saved_errors(no_color)
        journey(no_color)
