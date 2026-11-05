"""Checked local Taste results: actual Git and PTY, fake API/opener/server, zero providers."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import socket
import sys
import tempfile
import time
import test_launch as subprocess
from taste_preview_terminal import SERVER, alive, process_state
from taste_terminal import FAKE, REPO, confirmed_seed, fixture, tree
from terminal_process import TerminalProcess, wait_output


def journey(width=100, height=30, mono=False, forwarded=True, case='ready'):
    with tempfile.TemporaryDirectory(prefix='tinkery-TEST-local-preview-') as directory:
        base = Path(directory).resolve()
        repo = base / 'TEST-source'
        repo.mkdir()
        checkout = base / 'TEST-intake' if forwarded else repo
        scripts = checkout / '_system/scripts'
        scripts.mkdir(parents=True)
        (scripts / 'sdlc.py').write_text(FAKE)
        helpers = base / 'bin'
        helpers.mkdir()
        (helpers / 'python3').symlink_to(sys.executable)
        gh_called = base / 'gh-called'
        (helpers / 'gh').write_text(f'#!{sys.executable}\nfrom pathlib import Path\n'
                                   f'Path({str(gh_called)!r}).touch()\nraise SystemExit(1)\n')
        (helpers / 'gh').chmod(0o755)
        opened, started = base / 'opened', base / 'started'
        for name in ['open', 'xdg-open']:
            path = helpers / name
            path.write_text(f'#!{sys.executable}\nfrom pathlib import Path\nimport sys\n'
                            f'Path({str(opened)!r}).write_text(sys.argv[1])\n')
            path.chmod(0o755)
        env = subprocess.environment({**os.environ, 'HOME': str(base / 'HOME'), 'TERM': 'xterm-256color',
                                      'PATH': f'{helpers}:{os.environ["PATH"]}',
                                      'TINKERY_NODE': '/tinkery-TEST-no-node'}, home=base / 'HOME')
        env.pop('NO_COLOR', None)
        if mono:
            env['NO_COLOR'] = '1'
        env.update(TEST_TASTE_STATE=str(base / 'state.json'), TEST_TASTE_CALLS=str(base / 'calls'),
                   TEST_TASTE_PID=str(base / 'pid'), TEST_PREVIEW_STARTED=str(started),
                   TEST_PREVIEW_EVENTS=str(base / 'http-events'), TEST_PREVIEW_RESPONSE_DELAY='0')

        def git(*args):
            return subprocess.check_output([shutil.which('git'), *args], cwd=repo, env=env).decode().strip()

        git('init', '-q')
        git('config', 'user.name', 'TEST')
        git('config', 'user.email', 'TEST@example.invalid')
        with socket.socket() as sock:
            sock.bind(('127.0.0.1', 0))
            port = sock.getsockname()[1]
        spec = {'command': f'exec python3 preview.py {port}', 'url': f'http://127.0.0.1:{port}/'}
        (repo / 'tinkery.preview').write_text(json.dumps(spec) + '\n')
        (repo / 'preview.py').write_text(SERVER)
        (repo / 'marker').write_text('exact checked result')
        (repo / 'AGENTS.md').write_text('TEST authored instructions\n')
        git('add', '.')
        git('commit', '-qm', 'TEST checked source')
        head = git('rev-parse', 'HEAD')
        # Later evidence is not source. The receipt still names the checked commit.
        evidence = repo / 'runs/TEST-result/extra.log'
        evidence.parent.mkdir(parents=True)
        evidence.write_text('TEST later run evidence')
        git('add', 'runs')
        git('commit', '-qm', 'TEST evidence-only commit')
        evidence.write_text('TEST normal uncommitted run evidence')
        seed = confirmed_seed(base)
        state = fixture({'source': str(seed), 'path': 'runs/TEST-result/seed-contract.md',
                         'sha256': hashlib.sha256(seed.read_bytes()).hexdigest()})
        run = state['detail']['run']
        run.update(pr=None, closure=None, gates=[{'stage': 3, 'status': 'approved'}])
        run['verification']['candidate']['head'] = head
        if forwarded:
            run['checkout'] = str(repo)
        if case in ('missing', 'failed', 'stale'):
            run['verification_status'] = 'not-run' if case == 'missing' else case
            if case == 'missing':
                run['verification'] = None
        elif case == 'dirty':
            (repo / 'marker').write_text('TEST checked but not committed')
        elif case in ('generated-rules', 'agents-edit', 'agents-edit-with-rules'):
            # The worker launcher writes tink-sdlc's rules block; tink-sdlc excludes it from the candidate.
            authored = 'TEST edited instructions\n' if case != 'generated-rules' else 'TEST authored instructions\n'
            rules = '' if case == 'agents-edit' else ('\n<!-- tink:rules begin skillset=TEST-skillset -->\n'
                                                      'TEST generated rules\n<!-- tink:rules end -->\n')
            (repo / 'AGENTS.md').write_text(authored + rules)
        elif case == 'untracked':
            (repo / 'new-source.js').write_text('TEST uncommitted source')
        elif case == 'malformed-pr':
            run['pr'] = {'url': 'https://invalid.example/pull/1'}
        elif case == 'missing-head':
            run['verification']['candidate']['head'] = 'not-a-commit'
        statefile = base / 'state.json'
        statefile.write_text(json.dumps(state))
        original = tree(repo)
        terminal = TerminalProcess([str(REPO / 'target/debug/tinkery'), '--operator', 'agent', '--home',
                                    '--sdlc-root', str(checkout), '--seed-session-root', str(base / 'TEST-sessions')],
                                   cwd=base, env=env, width=width, height=height)
        screen, process = terminal.screen, terminal.process

        def wait(predicate, seconds=12):
            for code in wait_output(terminal.master, process, screen.feed, time.monotonic() + seconds):
                if predicate() and not screen.synchronized:
                    return
                assert code is None, screen.text()
            raise AssertionError(screen.text())

        def send(key, expected):
            os.write(terminal.master, key)
            wait(lambda: expected in screen.text())

        def stopped(record):
            wait(lambda: process_state(record['pid'], env) is None and not alive(record['child'], env))
            assert not Path(record['cwd']).exists(), 'temporary checkout was not removed'

        try:
            wait(lambda: '1  think' in screen.text())
            send(b'6', 'Press r to read results')
            assert not started.exists() and not opened.exists()
            send(b'r', 'TEST café')
            send(b'\r', 'e details')
            refusals = {
                'missing': 'current passed verification', 'failed': 'current passed verification',
                'stale': 'current passed verification', 'dirty': 'source differs',
                'agents-edit': 'source differs', 'agents-edit-with-rules': 'source differs',
                'untracked': 'source has uncommitted files', 'malformed-pr': 'invalid PR URL',
                'missing-head': 'commit is unknown',
            }
            if case in refusals:
                assert refusals[case] in screen.text(), screen.text()
                send(b'v', refusals[case])
                assert not started.exists() and not opened.exists()
                return
            assert 'Local checked result' in screen.text() and 'v runs' in screen.text(), screen.text()
            assert 'o open PR' not in screen.text()
            send(b'o', 'This local result has no PR')
            send(b'a', 'This local result has no PR')
            send(b'?', "v runs the project's preview")
            assert 'o opens the PR' not in screen.text() and 'a/c copy' not in screen.text()
            os.write(terminal.master, b'v')
            time.sleep(.08)
            assert not started.exists(), 'help executed the preview'
            send(b'\x1b', 'Local checked result')
            assert not started.exists() and not opened.exists(), 'navigation or copy executed the preview'
            if case in ('changed-run', 'changed-candidate', 'changed-declaration', 'changed-source', 'stale-before-v'):
                if case == 'changed-run':
                    run['run_id'] = '00000000-0000-4000-8000-000000000999'
                elif case == 'stale-before-v':
                    run['verification_status'] = 'stale'
                elif case == 'changed-source':
                    (repo / 'marker').write_text('TEST changed after reading')
                else:
                    (repo / ('tinkery.preview' if case == 'changed-declaration' else 'marker')).write_text(
                        json.dumps({**spec, 'command': 'echo TEST changed command'}) if case == 'changed-declaration'
                        else 'TEST another checked candidate')
                    git('add', '.')
                    git('commit', '-qm', 'TEST changed candidate after reading')
                    run['verification']['candidate']['head'] = git('rev-parse', 'HEAD')
                statefile.write_text(json.dumps(state))
                os.write(terminal.master, b'v')
                wait(lambda: 'Preview stopped:' in screen.text() or started.exists())
                assert not started.exists() and not opened.exists(), 'stale reading executed a command'
                assert any(s in screen.text() for s in ['changed since reading', 'source differs', 'current passed verification']), screen.text()
                return
            listener = None
            if case == 'occupied-port':
                listener = socket.socket()
                listener.bind(('127.0.0.1', port))
                listener.listen(1)
            try:
                os.write(terminal.master, b'v')
                if listener:
                    wait(lambda: 'port is already in use' in screen.text())
                    assert not started.exists() and not opened.exists()
                    return
                wait(started.exists)
                record = json.loads(started.read_text())
                assert record['marker'] == 'exact checked result', record
                assert record['cwd'] not in [str(repo), str(checkout)]
                assert not opened.exists(), 'browser opened before HTTP readiness'
                wait(lambda: opened.exists() and 'sent http' in screen.text(), seconds=35)
                assert opened.read_text() == spec['url']
                send(b'\x1b', 'Preview stopped.')
                stopped(record)
                started.unlink()
                opened.unlink()
                os.write(terminal.master, b'v')
                wait(started.exists)
                record = json.loads(started.read_text())
                send(b'\x1b[21~', '1  think')
                stopped(record)
                assert tree(repo) == original, 'preview changed its source checkout'
            finally:
                if listener:
                    listener.close()
        finally:
            # An orderly leave exercises product ownership even after an assertion fails.
            if process.poll() is None:
                os.write(terminal.master, b'\x1b[21~')
                for _ in wait_output(terminal.master, process, screen.feed, time.monotonic() + .4):
                    pass
            terminal.close()
            assert not gh_called.exists(), 'local result or malformed PR called GitHub'
        print(json.dumps({'journey': 'Taste local verified preview', 'size': [width, height],
                          'mono': mono, 'forwarded': forwarded, 'case': case, 'provider_calls': 0}))


if __name__ == '__main__':
    for width, height in [(100, 30), (80, 24)]:
        for mono in [False, True]:
            journey(width, height, mono, forwarded=mono)
    for case in ['missing', 'failed', 'stale', 'dirty', 'untracked', 'malformed-pr', 'missing-head',
                 'generated-rules', 'agents-edit', 'agents-edit-with-rules',
                 'changed-run', 'changed-candidate', 'changed-declaration', 'changed-source',
                 'stale-before-v', 'occupied-port']:
        journey(case=case)
    print('PASS 20 local-preview journeys; no provider or GitHub calls')
