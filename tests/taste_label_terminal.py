"""Taste labels (#666, #672): "Is this good?" answered pass or fail in the operator's words reaches
tink-sdlc's own labels.jsonl verbatim, only on F2, signed with the name the form showed.

A real tink-sdlc checkout (this repo's bundled 1.52.0 scaffold) in a TEST root; no providers, no gh.
"""
import json
import os
from pathlib import Path
import shutil
import sys
import tempfile
import time
import test_launch as subprocess
from terminal_process import TerminalProcess, wait_output

REPO = Path(__file__).resolve().parents[1]
WORDS = 'Shape\'s check didn\'t run "as cloned" — 2 times? q Ω'
F2, F10, ESC = b'\x1bOQ', b'\x1b[21~', b'\x1b'


def checkout(base, env):
    root = base / 'TEST-checkout'
    root.mkdir()
    for part in ('_system', 'stages', '_shared'):
        shutil.copytree(REPO / part, root / part)
    git = lambda *args: subprocess.run(['git', *args], cwd=root, env=env, check=True, capture_output=True)
    git('init', '-q', '-b', 'main')
    git('-c', 'user.name=TEST', '-c', 'user.email=test@local', 'add', '-A')
    git('-c', 'user.name=TEST', '-c', 'user.email=test@local', 'commit', '-qm', 'TEST baseline')
    sdlc(root, env, 'new', 'test-label')
    return root, git


def sdlc(root, env, *args):
    return subprocess.run([sys.executable, '_system/scripts/sdlc.py', *args], cwd=root, env=env,
                          check=True, capture_output=True, text=True)


def labels(root):
    path = root / 'runs/test-label/labels.jsonl'
    return [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []


def journey(width, height, no_color, operator):
    with tempfile.TemporaryDirectory(prefix='tinkery-TEST-taste-label-') as directory:
        base = Path(directory).resolve()
        tools = base / 'bin'
        tools.mkdir()
        (tools / 'gh').write_text('#!/bin/sh\necho "TEST gh must not run" >&2\nexit 1\n')
        (tools / 'gh').chmod(0o755)
        env = {**os.environ, 'HOME': str(base / 'HOME'), 'PATH': f'{tools}:{os.environ["PATH"]}',
               'TERM': 'xterm-256color', 'TINKERY_NODE': '/tinkery-TEST-no-node',
               'GIT_CONFIG_NOSYSTEM': '1'}
        (base / 'HOME').mkdir()
        env.pop('NO_COLOR', None)
        if no_color:
            env['NO_COLOR'] = '1'
        env = subprocess.environment(env, home=env['HOME'])
        root, git = checkout(base, env)
        args = [str(REPO / 'target/debug/tinkery'), '--home', '--sdlc-root', str(root),
                '--seed-session-root', str(base / 'TEST-sessions')]
        if operator:
            args[1:1] = ['--operator', operator]
        terminal = TerminalProcess(args, cwd=base, env=env,
                                   width=width, height=height)
        master, screen = terminal.master, terminal.screen

        def wait(text, seconds=10):
            def feed(chunk):
                terminal.raw.extend(chunk)
                screen.feed(chunk)
            for returncode in wait_output(master, terminal.process, feed, time.monotonic() + seconds):
                if text in screen.text() and not screen.synchronized:
                    return
                assert returncode is None, screen.text()
            raise AssertionError(f'{text!r} not shown ({width}x{height}):\n{screen.text()}')

        def send(keys, text):
            os.write(master, keys)
            wait(text)

        try:
            wait('1  think')
            send(b'6', 'Press r to read results')
            send(b'r', 'No confirmed seed')
            assert 'waiting for your label' in screen.text(), screen.text()
            send(b'\r', 'Is this good?')
            assert 'w is this good?' in screen.text() and 'Run ID' not in screen.text(), screen.text()
            if operator == 'agent':
                send(b'w', "Labels are the operator's")
                assert 'Signed as' not in screen.text()
                assert labels(root) == []
                print(json.dumps(dict(journey='Taste label refused for agent', width=width, height=height,
                                      no_color=no_color, labels=0, provider_calls=0)))
                return
            # Esc discards: nothing reaches tink-sdlc.
            send(b'w', 'Your words')
            for line in ('1 pass', 'you\'d keep it', '2 fail', 'wrong or missing',
                         'What makes it good, or what is wrong with it?'):
                assert line in screen.text(), (line, screen.text())
            assert 'tink-sdlc' not in screen.text() and 'correction' not in screen.text(), screen.text()
            send(b'1', '› 1 pass')
            send('half'.encode(), 'half')
            send(ESC, 'Label discarded; nothing recorded.')
            assert labels(root) == []
            # No name to sign with: tink-sdlc is never asked, and the words stay in the form.
            send(b'w', 'No name to sign with')
            send(b'1', '› 1 pass')
            send(WORDS.encode(), 'Ω')
            send(F2, 'Checking your name again')
            wait('No name to sign with')
            assert labels(root) == [] and 'Your words' in screen.text()
            git('config', 'user.name', 'TEST operator')
            send(F2, 'Signed as TEST operator.')
            assert labels(root) == [], 'a name the form had not shown signed a label'
            send(F2, 'Saved your label.')
            [record] = labels(root)
            assert (record['kind'], record['words'], record['reviewer'], record['source']) == (
                'accepted', WORDS, 'TEST operator', 'taste'), record
            assert 'Your words' not in screen.text() and 'test-label' not in screen.text()
            # Words that look like an option still arrive as words; fail saves as rejected.
            send(b'w', 'Signed as TEST operator.')
            send(b'2', '› 2 fail')
            send(b'--no-verify', '--no-verify')
            send(F2, 'Saved your label.')
            assert [(r['kind'], r['words']) for r in labels(root)][1] == ('rejected', '--no-verify')
            # tink-sdlc's own refusal comes back word for word once the run is closed.
            sdlc(root, env, 'close', 'test-label', '--outcome', 'abandoned')
            send(b'w', 'Signed as TEST operator.')
            send(b'1', '› 1 pass')
            send(b'ok', 'ok')
            send(F2, 'This run is closed;')
            assert 'Not recorded. tink-sdlc said:' in screen.text() and len(labels(root)) == 2
            send(ESC, 'Label discarded')
            send(F10, '1  think')
            os.write(master, b'q')
            terminal.process.wait(timeout=5)
            assert terminal.process.returncode == 0
            print(json.dumps(dict(journey='Taste label', width=width, height=height, no_color=no_color,
                                  labels=2, verbatim=True, provider_calls=0, gh_calls=0)))
        finally:
            terminal.close()


if __name__ == '__main__':
    journey(100, 30, False, None)
    journey(80, 24, True, 'human')
    journey(80, 24, True, 'agent')
