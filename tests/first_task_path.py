"""docs/first-task.md, run as written: its shell blocks, in order, in a TEST HOME.

Zero model calls: a stand-in `pi` answers on the pin and the check reads only local files.
Steps 4 and 5 are keys, not shell; their journeys live in the Think and Shape terminal tests.
Only the clone lines change: tink-sdlc comes from a pinned ref, and Tinkery is this checkout.
"""
import json
from pathlib import Path
import re
import shlex
import shutil
import sys
import tempfile
import time

import session_fixture as helper
import test_launch as subprocess

REPO = Path(__file__).resolve().parents[1]
GUIDE = (REPO / 'docs/first-task.md').read_text()
# tink-sdlc 1.48.3 (seed attestation mark hints, PR #140).
SDLC_REF = '9cc66663410183c5b17bae762592608431f04092'
PIN = 'test-provider/pinned-v1:low'
# A stand-in for Pi: writes a session the way Pi does and answers once on --model.
FAKE_PI = '''#!/usr/bin/env python3
import json, os, sys, uuid
from pathlib import Path
argv = sys.argv[1:]
flag = lambda name: argv[argv.index(name) + 1]
provider, model_id = flag('--model').split('/', 1)
def entry(kind, parent, **fields):
    return {'type': kind, 'id': uuid.uuid4().hex[:8], 'parentId': parent,
            'timestamp': '2026-10-31T00:00:00.000Z', **fields}
rows = [{'type': 'session', 'version': 3, 'id': flag('--session-id'), 'cwd': os.getcwd()}]
rows.append(entry('model_change', None, provider=provider, modelId=model_id))
rows.append(entry('message', rows[-1]['id'], message={'role': 'user', 'content': 'TEST'}))
rows.append(entry('message', rows[-1]['id'], message={'role': 'assistant', 'content': 'TEST',
                  'provider': provider, 'model': model_id, 'stopReason': 'stop'}))
path = Path(flag('--session-dir')) / ('2026-10-31T00-00-00-000Z_' + flag('--session-id') + '.jsonl')
path.write_text(''.join(json.dumps(r) + '\\n' for r in rows))
with open(os.environ['TEST_PI_CALLS'], 'a') as calls:
    calls.write(' '.join(argv[:4]) + '\\n')
'''


def blocks(heading):
    """The sh blocks under one `## ` heading of the guide, in order."""
    section = GUIDE.split(f'\n## {heading}\n', 1)[1].split('\n## ', 1)[0]
    return re.findall(r'```sh\n(.*?)```', section, re.S)


def shell(script, env, cwd, waiting=False):
    """Run `script`; `waiting` expects its last command, the readiness check, to say not yet (exit 1)."""
    result = subprocess.run(['bash', '-euo', 'pipefail', '-c', script], cwd=cwd, env=env,
                            capture_output=True, text=True, timeout=180)
    assert result.returncode == (1 if waiting else 0), (script, result.stdout, result.stderr)
    return result.stdout


def isolated_tools(directory, env):
    """Expose only guide dependencies; Pi comes from this journey's later stand-in install."""
    for name in ('bash', 'git', 'mkdir', 'chmod'):
        program = shutil.which(name, path=env['PATH'])
        if program is None:
            raise RuntimeError(f'First-task journey requires {name} on PATH.')
        (directory / name).symlink_to(Path(program).resolve())
    (directory / 'python3').symlink_to(sys.executable)
    (directory / 'tinkery').symlink_to(REPO / 'target/debug/tinkery')
    return {**env, 'PATH': str(directory), 'TINK_SDLC_PI': str(directory / 'pi')}


def main():
    with tempfile.TemporaryDirectory(prefix='tinkery-TEST-first-task-') as directory:
        root = Path(directory).resolve()
        home = root / 'HOME'
        (home / 'bin').mkdir(parents=True)
        env = subprocess.environment(home=home)
        host = root / 'TEST-host-bin'
        host.mkdir()
        host_calls = root / 'TEST-host-calls'
        for name in ('pi', 'tink'):
            program = host / name
            program.write_text(f'#!/bin/sh\nprintf {name} >> {shlex.quote(str(host_calls))}\nexit 97\n')
            program.chmod(0o755)
        host_originals = {p.name: p.read_bytes() for p in host.iterdir()}
        env['PATH'] = f"{host}:{env['PATH']}"
        env['TINK_SDLC_PI'] = str(host / 'pi')
        env['TEST_PI_CALLS'] = str(root / 'pi-calls')
        env.update(GIT_AUTHOR_NAME='TEST', GIT_AUTHOR_EMAIL='test@example.invalid',
                   GIT_COMMITTER_NAME='TEST', GIT_COMMITTER_EMAIL='test@example.invalid')
        # The guide's own variable names, with this checkout and a TEST pin as values.
        names = re.search(r'```sh\n(tinkery_src=.*?)```', GUIDE, re.S).group(1)
        assert [line.split('=')[0] for line in names.splitlines()] == [
            'tinkery_src', 'project', 'notes'], names
        assert '`worker_model=provider/model-id:low`' in GUIDE  # a placeholder, not a real model id
        prelude = f"tinkery_src={REPO}\nproject=~/dev/my-project\nnotes=~/dev/run-notes\nworker_model={PIN}\n"
        subprocess.run(['cargo', '+stable', 'build', '--locked'], cwd=REPO, env=env, check=True,
                       stdout=subprocess.DEVNULL)
        env = isolated_tools(home / 'bin', env)
        score = []

        # 1. The readiness check, not the contributor suite.
        install, check = blocks('1. Install Tinkery')
        assert 'cargo +stable build --locked --release' in install
        # A fresh user with no Git identity learns it here, not from step 2's failing commit.
        anonymous = {k: v for k, v in env.items() if not k.startswith(('GIT_AUTHOR', 'GIT_COMMITTER'))}
        for host_config in ('XDG_CONFIG_HOME', 'GIT_CONFIG_GLOBAL'):
            anonymous.pop(host_config, None)
        anonymous.update(GIT_CONFIG_NOSYSTEM='1', GIT_CONFIG_COUNT='1',
                         GIT_CONFIG_KEY_0='user.useConfigOnly', GIT_CONFIG_VALUE_0='true')
        out = shell(prelude + check, anonymous, root, waiting=True)
        assert out.splitlines()[-1].startswith('Next: Tell Git who you are'), out
        out = shell(prelude + check, env, root, waiting=True)
        assert 'ready: Tinkery runs' in out and 'not yet: The workflow is connected' in out, out
        assert out.splitlines()[-1].startswith('Next: Name your project with --sdlc-root'), out
        score.append('1 install')

        # 2. tink-sdlc into a fresh project; the clone is pinned, every other line verbatim.
        [connect] = blocks('2. Connect your project to tink-sdlc')
        clone = 'git clone https://github.com/jon-devlapaz/tink-sdlc ~/dev/tink-sdlc\n'
        assert clone in connect, connect
        shell(f'mkdir -p ~/dev/my-project && cd ~/dev/my-project && git init -q -b main'
              f' && printf "#!/bin/sh\\necho TEST output\\n" > TEST-check && chmod +x TEST-check'
              f' && git add -A && git commit -qm base\n'
              f'git init -q ~/dev/tink-sdlc && cd ~/dev/tink-sdlc'
              f' && git fetch -q --depth 1 https://github.com/jon-devlapaz/tink-sdlc.git {SDLC_REF}'
              f' && git checkout -q --detach FETCH_HEAD', env, root)
        out = shell(prelude + connect.replace(clone, '', 1), env, root, waiting=True)
        assert 'ready: The workflow is connected' in out, out
        assert out.splitlines()[-1].startswith('Next: Install Pi and log in'), out
        project = home / 'dev/my-project'
        # An install not yet committed is not connected (the fresh-Pi run read it as ready).
        out = shell('mkdir ~/dev/uncommitted && git -C ~/dev/uncommitted init -q'
                    ' && python3 ~/dev/tink-sdlc/scripts/init.py ~/dev/uncommitted >/dev/null\n'
                    f'python3 {REPO}/scripts/first-task-check.py --sdlc-root ~/dev/uncommitted',
                    env, root, waiting=True)
        assert out.splitlines()[-1].startswith('Next: Commit the tink-sdlc install'), out
        # The guide's next instruction: choose real checks in _system/verification.json.
        (project / '_system/verification.json').write_text(json.dumps({'require_tink': False, 'checks': [
            {'argv': ['./TEST-check'], 'timeout_seconds': 10}]}))
        shell('git add -A && git commit -qm "Choose TEST checks"', env, project)
        score.append('2 connect')

        # 3. What `s` in the Model panel saves, and a Pi with that model and a login on disk.
        [launch] = blocks('3. Choose the drafting model')
        assert launch.strip() == 'tinkery --home --sdlc-root "$project"'
        readiness = prelude + 'python3 "$tinkery_src/scripts/first-task-check.py" --sdlc-root "$project"'
        pi = home / 'bin/pi'
        pi.write_text(FAKE_PI)
        pi.chmod(0o755)
        agent = home / '.pi/agent'
        agent.mkdir(parents=True)
        (agent / 'models-store.json').write_text(json.dumps({'test-provider': {'models': [
            {'id': 'pinned-v1', 'provider': 'test-provider', 'reasoning': True}]}}))
        # Pi installed but not logged in: log in before the model menu.
        out = shell(readiness, env, root, waiting=True)
        assert out.splitlines()[-1].startswith('Next: Log in to Pi'), out
        (agent / 'auth.json').write_text(json.dumps({'test-provider': {'type': 'oauth', 'refresh': 'TEST'}}))
        out = shell(readiness, env, root, waiting=True)
        assert out.splitlines()[-1].startswith('Next: Press m on the menu'), out
        assert '--shape-budget CALLS' in out.splitlines()[-1], out
        (home / '.config/tinkery').mkdir(parents=True)
        (home / '.config/tinkery/model.json').write_text(json.dumps(
            {'harness': 'pi', 'model': PIN.rsplit(':', 1)[0], 'thinking': 'low', 'calls': 4}))
        out = shell(readiness, env, root, waiting=True)
        assert 'ready: Drafting is set up' in out and 'not yet: A worker can launch' in out, out
        snapshot = shell(prelude + launch.strip() + ' --snapshot --no-color', env, root)
        assert '1  think' in snapshot, snapshot
        score.append('3 drafting')

        # 6. A confirmed seed (Shape's journeys prove F2 confirms one), then Approve's command shape.
        session = helper.create(root / 'TEST-sessions')
        old = helper.load(session)
        state = helper.editable(old)
        state.update(goal='TEST first task', origin='goal', current_question=None, nodes=[{
            'id': 'goal', 'kind': 'decision', 'prerequisites': [], 'evidence': [], 'owner': 'TEST fixture',
            'gate': 'TEST only', 'status': 'settled', 'answer': 'TEST first task',
            'authority': 'user', 'authority_source': 'TEST fixture only'}])
        helper.publish(session, state, old['version'], 'TEST fixture')
        helper.set_checks(session, [{'statement': 'TEST output is shown.', 'kind': 'cmd', 'cmd': './TEST-check',
                                     'expect': 'TEST output', 'cwd': '.', 'source': 'TEST goal'}])
        body = (REPO / 'tests/fixtures/shape-seed-reply.txt').read_text().replace(
            '## Affected users and systems',
            helper.checks_block(helper.read_checks(session)) + '## Affected users and systems', 1)
        body = body.replace('## Knowledge map', helper.decisions_block(helper.load(session)) + '\n## Knowledge map', 1)
        body = body.replace('## Suggested first slice',
                            '## Evidence and uncertainty\nTEST fixture.\n\n## Suggested first slice', 1)
        (session / 'seed-contract.md').write_text(
            'status: draft\n# TEST first task\nrevision: r1\n\n## You are confirming\n' + body)
        helper.confirm_seed(session, 'r1', 'confirm')
        [intake] = blocks('6. Approve: create the run')
        run_vars = f"{prelude}seed={session / 'seed-contract.md'}\nrun=my-run\n"
        shell(run_vars + intake, env, root)
        score.append('6 approve')

        # 7. The unchanged seed opens the build stage with no further question.
        [stage] = blocks('7. Open the build stage')
        out = shell(run_vars + stage, env, root)
        assert 'Stage 3: approved' in out and 'matches confirmed seed' in out, out
        score.append('7 stage')

        # 8. The pin checks out offline, then one pinned step launches the stand-in Pi.
        [start] = blocks('8. Start the worker')
        out = shell(run_vars + start, env, root)
        assert 'ready: A worker can launch' in out and 'Next: start your first task' in out, out
        assert '"state": "active"' in out, out
        score.append('8 worker')

        # 9. The ended turn is observed; the worker's report is recorded; verify runs the checks.
        show, finish, verify = blocks('9. Watch it, then finish')
        for _ in range(50):
            out = shell(run_vars + show, env, root)
            if 'ended: read the report' in out:
                break
            time.sleep(0.1)
        assert '"observed_turn": "ended: read the report, then complete the step"' in out, out
        (home / 'dev/run-notes/my-run-build.md').write_text('TEST report written by the stand-in worker.\n')
        out = shell(run_vars + finish, env, root)
        assert '"state": "completed"' in out, out
        assert 'Stage 3: approved' in out, out
        shell(run_vars + verify, env, root)
        status = json.loads(shell('python3 _system/scripts/sdlc.py status my-run --json', env, project))
        assert status['run']['verification']['passed'] is True, status['run']['verification']
        assert (root / 'pi-calls').read_text().count('--mode json') == 1
        assert not host_calls.exists(), 'the journey invoked a host Pi or tink'
        assert {p.name: p.read_bytes() for p in host.iterdir()} == host_originals
        score.append('9 finish')
        print(f'First-task guide ran as written: {len(score)} shell steps ({", ".join(score)}), '
              f'1 stand-in Pi launch, 0 model calls')


if __name__ == '__main__':
    main()
