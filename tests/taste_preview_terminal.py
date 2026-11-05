"""Exact-commit Taste previews: local Git fixture, fake server, stub opener, no providers."""
from terminal_process import wait_output
import fcntl
import hashlib
import json
import os
from pathlib import Path
import select
import shutil
import socket
import struct
import sys
import tempfile
import termios
import time
import test_launch as subprocess
from taste_terminal import FAKE, REPO, RUN_ID, URL, confirmed_seed, fixture, tree
from vt_screen import Screen

SERVER = '''import http.server, json, os, socket, socketserver, subprocess, sys, time
from pathlib import Path
child = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(120)'])
record = Path(os.environ['TEST_PREVIEW_STARTED'])
staged = record.with_suffix('.tmp')
staged.write_text(json.dumps(dict(pid=os.getpid(), child=child.pid, cwd=os.getcwd(), marker=Path('marker').read_text())))
staged.replace(record)
def event(kind):
    with open(os.environ['TEST_PREVIEW_EVENTS'], 'a') as log:
        log.write(json.dumps(dict(event=kind, at=time.monotonic())) + '\\n')

def refuse_fqdn(*args, **kwargs):
    event('unexpected fqdn lookup')
    raise AssertionError('numeric loopback fixture must not perform reverse DNS')

socket.getfqdn = refuse_fqdn

class LoopbackHTTPServer(http.server.HTTPServer):
    def server_bind(self):
        socketserver.TCPServer.server_bind(self)
        self.server_name, self.server_port = '127.0.0.1', self.server_address[1]

class Handler(http.server.SimpleHTTPRequestHandler):
    def do_GET(self):
        event('request')
        time.sleep(float(os.environ['TEST_PREVIEW_RESPONSE_DELAY']))
        try:
            super().do_GET()
            event('response')
        except (BrokenPipeError, ConnectionResetError):
            event('abandoned')

event('startup')
time.sleep(.6)
event('constructing server')
server = LoopbackHTTPServer(('127.0.0.1', int(sys.argv[1])), Handler)
event('listening')
server.serve_forever()
'''


def make_stubs_executable(paths):
    for path in paths:
        if not path.is_symlink():
            path.chmod(0o755)


def process_state(pid, env):
    result = subprocess.run(['/bin/ps', '-o', 'stat=', '-p', str(pid)],
                            env=env, capture_output=True, text=True)
    if result.returncode == 1 and not result.stdout.strip() and not result.stderr.strip():
        return None
    assert result.returncode == 0 and result.stdout.strip(), (pid, result)
    return result.stdout.strip()[0]


def alive(pid, env):
    return process_state(pid, env) not in (None, 'Z')


def verify_portable_helpers():
    with tempfile.TemporaryDirectory(prefix='tinkery-TEST-preview-portability-') as directory:
        base = Path(directory)
        env = subprocess.environment(home=base / 'HOME')
        target = base / 'TEST-interpreter'
        target.write_text('TEST symlink target must retain its permissions')
        target.chmod(0o640)
        link = base / 'python3'
        link.symlink_to(target)
        script = base / 'stub'
        script.write_text('#!/bin/sh\nexit 0\n')
        make_stubs_executable([script, link])
        assert target.stat().st_mode & 0o777 == 0o640, 'chmod followed the interpreter symlink'
        assert script.stat().st_mode & 0o777 == 0o755

        read_fd, write_fd = os.pipe()
        pid = os.fork()
        if pid == 0:
            os.close(write_fd)
            os.read(read_fd, 1)
            os._exit(0)
        os.close(read_fd)
        reaped = False
        try:
            assert alive(pid, env), 'live child was not detected'
            os.close(write_fd)
            write_fd = None
            until = time.monotonic() + 5
            while process_state(pid, env) != 'Z':
                assert time.monotonic() < until, 'exited child was not detected as a zombie'
                time.sleep(.01)
            assert not alive(pid, env), 'zombie was counted as running'
            waited, status = os.waitpid(pid, 0)
            reaped = True
            assert waited == pid and os.WIFEXITED(status) and os.WEXITSTATUS(status) == 0
            assert process_state(pid, env) is None, 'waitpid did not remove the child'
        finally:
            if write_fd is not None:
                os.close(write_fd)
            if not reaped:
                try:
                    os.kill(pid, 9)
                except ProcessLookupError:
                    pass
                os.waitpid(pid, 0)
    print('PASS preview harness: symlink permissions unchanged; ps detects live/zombie/reaped child')


def journey(width, height, mono, wrong_head=False, response_delay=0):
    with tempfile.TemporaryDirectory(prefix='tinkery-TEST-preview-journey-') as directory:
        base = Path(directory).resolve()
        env = {**os.environ, 'HOME': str(base / 'HOME'), 'TERM': 'xterm-256color',
               'TINKERY_NODE': '/tinkery-TEST-no-node'}
        env.pop('NO_COLOR', None)
        if mono:
            env['NO_COLOR'] = '1'
        env = subprocess.environment(env, home=env['HOME'])
        repo = base / 'TEST-result-repo'
        repo.mkdir()
        git = shutil.which('git')

        def run_git(*args):
            return subprocess.check_output([git, *args], cwd=repo, env=env).decode().strip()

        run_git('init', '-q')
        run_git('config', 'user.name', 'TEST')
        run_git('config', 'user.email', 'TEST@example.invalid')
        with socket.socket() as sock:
            sock.bind(('127.0.0.1', 0))
            port = sock.getsockname()[1]
        spec = {'command': f'exec python3 preview.py {port}', 'url': f'http://127.0.0.1:{port}/'}
        (repo / 'tinkery.preview').write_text(json.dumps(spec) + '\n')
        (repo / 'preview.py').write_text(SERVER)
        (repo / 'marker').write_text('exact result')
        run_git('add', '.')
        run_git('commit', '-qm', 'TEST exact result')
        head = run_git('rev-parse', 'HEAD')
        (repo / 'marker').write_text('wrong result')
        run_git('commit', '-qam', 'TEST newer result not selected')
        other = run_git('rev-parse', 'HEAD')
        checkout = base / 'TEST-status'
        scripts = checkout / '_system/scripts'
        scripts.mkdir(parents=True)
        (scripts / 'sdlc.py').write_text(FAKE)
        seed = confirmed_seed(base)
        state = fixture({'source': str(seed), 'path': 'runs/TEST-result/seed-contract.md',
                         'sha256': hashlib.sha256(seed.read_bytes()).hexdigest()})
        criterion = 'Can I compare two allocations without changing the assumptions?'
        # A long criterion stays scrollable; it cannot push the command offscreen before v runs it.
        if wrong_head:
            criterion += ' Check the previously entered staffing and budget values.' * 32
        state['detail']['run']['checklist'][1]['verify'] = criterion
        state['pr']['headRefOid'] = head
        statefile = base / 'state.json'
        statefile.write_text(json.dumps(state))
        helpers = base / 'bin'
        helpers.mkdir()
        (helpers / 'python3').symlink_to(sys.executable)
        (helpers / 'gh').write_text(f'#!{sys.executable}\n' + f'''import json, os, sys
from pathlib import Path
if sys.argv[1] == 'api':
    assert sys.argv[-1].endswith('tinkery.preview?ref={head}')
    error = json.loads(Path(os.environ['TEST_TASTE_STATE']).read_text()).get('preview_error')
    if error == 'malformed':
        print('not JSON')
        sys.exit(0)
    if error:
        print(error, file=sys.stderr)
        sys.exit(1)
    print({json.dumps(spec)!r})
    sys.exit(0)
''' + FAKE)
        (helpers / 'git').write_text(f'#!{sys.executable}\n' + f'''import os, sys
args = sys.argv[1:]
if 'fetch' in args:
    assert 'credential.helper=!gh auth git-credential' in args
    assert args[-2] == 'https://github.com/TEST/clinic-tycoon.git'
    assert args[-1] == {head!r}
    args[-2:] = [{str(repo)!r}, {other if wrong_head else head!r}]
os.execv({git!r}, [{git!r}, *args])
''')
        opened = base / 'opened'
        opener = f'#!{sys.executable}\nfrom pathlib import Path\nimport sys\nPath({str(opened)!r}).write_text(sys.argv[1])\n'
        for name in ['open', 'xdg-open']:
            (helpers / name).write_text(opener)
        make_stubs_executable(helpers / name for name in ['gh', 'git', 'open', 'xdg-open'])
        started = base / 'started'
        env.update(PATH=f'{helpers}:/usr/bin:/bin', TEST_TASTE_STATE=str(statefile),
                   TEST_TASTE_CALLS=str(base / 'calls'), TEST_TASTE_PID=str(base / 'pid'),
                   TEST_PREVIEW_STARTED=str(started), TEST_PREVIEW_EVENTS=str(base / 'http-events'),
                   TEST_PREVIEW_RESPONSE_DELAY=str(response_delay))
        master, slave = os.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', height, width, 0, 0))
        screen = Screen(width, height)
        process = subprocess.Popen([str(REPO / 'target/debug/tinkery'), '--operator', 'agent', '--home',
                                    '--sdlc-root', str(checkout), '--seed-session-root', str(base / 'TEST-sessions')],
                                   cwd=base, env=env,
                                   stdin=slave, stdout=slave, stderr=slave)
        original = tree(checkout)
        repo_original = tree(repo)

        def wait(predicate, seconds=10):
            until = time.monotonic() + seconds
            for returncode in wait_output(master, process, screen.feed, until):
                if predicate() and not screen.synchronized:
                    return
                assert returncode is None, screen.text()
            events = base / 'http-events'
            raise AssertionError(screen.text() + '\nHTTP fixture events:\n' +
                                 (events.read_text() if events.exists() else 'not started'))

        def send(key, expected):
            os.write(master, key)
            wait(lambda: expected in screen.text())

        def stopped(record):
            wait(lambda: process_state(record['pid'], env) is None and not alive(record['child'], env))
            assert not Path(record['cwd']).exists(), 'temporary checkout survived cancellation'

        try:
            wait(lambda: '1  think' in screen.text())
            send(b'6', 'Press r to read results')
            assert not started.exists() and not opened.exists()
            send(b'r', '› TEST clinic allocation comparison')
            assert 'Enter open result' in screen.text(), screen.text()
            send(b'?', 'Enter opens a result')
            send(b'\x1b', 'Enter open result')
            send(b'\r' if wrong_head else b'v', 'e details')
            review = screen.text()
            assert criterion.split('?')[0] in review and 'What to judge' in review, review
            assert 'v runs' in review and 'Human verdict only' in review, review
            text = screen.text().replace('\n', '').replace(' ', '')
            assert f'vruns"{spec["command"]}",thenopens{spec["url"]}'.replace(' ', '') in text, screen.text()
            assert head not in text, 'the commit is checked at launch and shown in details'
            send(b'e', 'You owe')
            send(b'\x1b[F', head)
            send(b'e', 'What to judge')
            assert not started.exists() and not opened.exists(), 'read/navigation executed command'
            send(b'?', 'What Taste reads')
            assert 'Enter opens a result' not in screen.text(), screen.text()
            assert "v runs the project's preview" in screen.text(), screen.text()
            os.write(master, b'v')
            time.sleep(.1)
            assert not started.exists(), 'help leaked preview execution'
            os.write(master, b'\x1b')
            wait(lambda: 'What Taste reads' not in screen.text() and 'v runs "' in screen.text())
            os.write(master, b'v')
            if wrong_head:
                wait(lambda: 'Checkout commit disagrees' in screen.text())
                assert not started.exists() and not opened.exists(), 'wrong commit executed'
                send(b'\x1b', 'Preview stopped.')
            else:
                wait(started.exists)
                record = json.loads(started.read_text())
                assert alive(record['pid'], env) and alive(record['child'], env), 'preview group was not running'
                assert record['marker'] == 'exact result'
                assert record['cwd'] != str(repo) and record['cwd'] != str(checkout)
                assert not opened.exists(), 'opened browser before URL answered'
                wait(lambda: opened.exists() and 'sent http' in screen.text(), seconds=35)  # 30 s readiness + opener/paint slack
                assert opened.read_text() == spec['url']
                assert 'Esc stops and returns' in screen.text() and 'reap' not in screen.text(), screen.text()
                send(b'\x1b', 'Preview stopped.')
                stopped(record)
                started.unlink()
                opened.unlink()
                os.write(master, b'v')
                wait(started.exists)
                record = json.loads(started.read_text())
                assert alive(record['pid'], env) and alive(record['child'], env), 'preview group was not running'
                send(b'\x1b[21~', '1  think')  # F10 leaves and stops, including startup.
                stopped(record)
                send(b'6', 'Press r to read results')
                state['preview_error'] = 'HTTP 404'
                statefile.write_text(json.dumps(state))
                send(b'r', '› TEST clinic allocation comparison')
                send(b'\r', 'No preview for this project.')
                send(b'v', '? says how to add one.')
                send(b'?', 'commit tinkery.preview at the project root')
                assert '"command"' in screen.text(), screen.text()
                # The suggested declaration binds the probed address, not localhost (often ::1 only).
                assert '--host 127.0.0.1' in ' '.join(screen.text().split()), screen.text()
                send(b'\x1b', 'What to judge')
                for error, reason in [('malformed', 'Invalid tinkery.preview'),
                                      ('HTTP 403', "Couldn't read tinkery.preview")]:
                    state['preview_error'] = error
                    statefile.write_text(json.dumps(state))
                    send(b'r', reason)
                    os.write(master, b'v')
                    wait(lambda: reason in screen.text().splitlines()[-4])
                    assert 'how to add one' not in screen.text() and 'No preview for this project.' not in screen.text(), screen.text()
                    send(b'?', 'commit tinkery.preview at the project root')
                    assert 'v runs the project' not in screen.text(), screen.text()
                    send(b'\x1b', 'What to judge')
                assert not opened.exists(), 'unavailable preview opened a browser'
            assert tree(checkout) == original and tree(repo) == repo_original
            events = base / 'http-events'
            http_events = [json.loads(line) for line in events.read_text().splitlines()] if events.exists() else []
            print(json.dumps(dict(journey='Taste exact-head preview' if not wrong_head else 'Taste wrong-head refusal',
                                  width=width, height=height, no_color=mono, provider_calls=0,
                                  opener='stub', response_delay=response_delay,
                                  http_requests=sum(e['event'] == 'request' for e in http_events),
                                  esc_reaped=not wrong_head, leave_reaped=not wrong_head)))
        finally:
            if process.poll() is None:
                os.write(master, b'\x1b[21~')
                wait(lambda: '1  think' in screen.text())
                os.write(master, b'q')
                until = time.monotonic() + 5
                for returncode in wait_output(master, process, lambda chunk: None, until):
                    if returncode is not None:
                        break
                process.wait(timeout=1)
            os.close(master)
            os.close(slave)


if __name__ == '__main__':
    verify_portable_helpers()
    for width, height in [(100, 30), (80, 24)]:
        for mono in [False, True]:
            journey(width, height, mono)
            journey(width, height, mono, wrong_head=True)
    journey(100, 30, False, response_delay=.25)
