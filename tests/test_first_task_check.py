"""The first-task check claims only what it read, and leaves the checked project untouched."""
import json
import os
from pathlib import Path
import shutil
import sys
import tempfile
import unittest

import test_launch as subprocess

REPO = Path(__file__).resolve().parents[1]
CHECK = REPO / 'scripts/first-task-check.py'


class FirstTaskCheckTests(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory(prefix='tinkery-TEST-first-task-check-')
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name).resolve()
        self.home = self.root / 'HOME'
        self.home.mkdir()
        tools = self.root / 'TEST-bin'
        tools.mkdir()
        self.pi_calls = self.root / 'TEST-pi-calls'
        for name, body in (('pi', f'printf launched >> {self.pi_calls}\nexit 97'), ('tinkery', 'exit 0')):
            (tools / name).write_text(f'#!/bin/sh\n{body}\n')
            (tools / name).chmod(0o755)
        self.env = subprocess.environment(home=self.home)
        # CI's verification sets this, which hid the bytecode the check used to write.
        self.env.pop('PYTHONDONTWRITEBYTECODE', None)
        self.env.update(PATH=f"{tools}:{self.env['PATH']}", TINK_SDLC_PI=str(tools / 'pi'),
                        GIT_AUTHOR_NAME='TEST', GIT_AUTHOR_EMAIL='test@example.invalid',
                        GIT_COMMITTER_NAME='TEST', GIT_COMMITTER_EMAIL='test@example.invalid')
        self.tinkery = tools / 'tinkery'

    def project(self, history):
        """A checkout with the workflow installed but not committed."""
        project = self.root / f'TEST-project-{history}'
        project.mkdir()
        self.git(project, 'init', '-q', '-b', 'main')
        if history == 'existing':
            self.git(project, 'commit', '-q', '--allow-empty', '-m', 'TEST base')
        shutil.copytree(REPO / '_system', project / '_system',
                        ignore=shutil.ignore_patterns('__pycache__'))
        return project

    def git(self, project, *args):
        subprocess.run(['git', '-C', str(project), *args], env=self.env, check=True, capture_output=True)

    def check(self, project, *extra):
        result = subprocess.run([sys.executable, str(CHECK), '--tinkery', str(self.tinkery),
                                 '--sdlc-root', str(project), *extra],
                                env=self.env, capture_output=True, text=True, timeout=60)
        return result.returncode, result.stdout

    def test_staged_install_is_not_connected_until_committed(self):
        for history in ('unborn', 'existing'):
            with self.subTest(history=history):
                project = self.project(history)
                for stage in ('untracked', 'staged'):
                    if stage == 'staged':
                        self.git(project, 'add', '_system')
                    code, out = self.check(project)
                    self.assertEqual(code, 1, out)
                    self.assertIn('not yet: The workflow is connected', out)
                    self.assertTrue(out.splitlines()[-1].startswith('Next: Commit the tink-sdlc install'), out)
                self.git(project, 'commit', '-q', '-m', 'TEST install')
                code, out = self.check(project)
                self.assertIn('ready: The workflow is connected', out)

    def test_no_login_claim_before_credentials_are_read(self):
        project = self.project('existing')
        code, out = self.check(project, '--operator', 'agent')
        self.assertEqual(code, 1, out)
        self.assertNotIn('Pi has a login', out)
        self.assertIn("not yet: Drafting is set up; the first real send proves it", out)
        self.assertTrue(out.splitlines()[-1].startswith('Next: Commit the tink-sdlc install'), out)
        # Connected but with no credential store: the login is asked for, not assumed.
        self.git(project, 'add', '_system')
        self.git(project, 'commit', '-q', '-m', 'TEST install')
        code, out = self.check(project, '--operator', 'agent')
        self.assertNotIn('Pi has a login', out)
        self.assertTrue(out.splitlines()[-1].startswith('Next: Log in to Pi'), out)
        self.assertFalse(self.pi_calls.exists(), 'the check launched Pi')

    def test_checking_leaves_the_project_clean(self):
        project = self.project('existing')
        self.git(project, 'add', '_system')
        self.git(project, 'commit', '-q', '-m', 'TEST install')
        agent = self.home / '.pi/agent'
        agent.mkdir(parents=True)
        (agent / 'auth.json').write_text(json.dumps({'test-provider': {'type': 'oauth', 'refresh': 'TEST'}}))
        code, out = self.check(project, '--operator', 'agent', '--worker-model', 'test-provider/pinned-v1:low')
        self.assertIn('ready: The workflow is connected', out)
        status = subprocess.run(['git', '-C', str(project), 'status', '--porcelain', '--untracked-files=all'],
                                env=self.env, capture_output=True, text=True, check=True)
        self.assertEqual(status.stdout, '', out)
        self.assertFalse(self.pi_calls.exists(), 'the check launched Pi')


if __name__ == '__main__':
    os.environ.setdefault('TINKERY_NODE', '/tinkery-TEST-no-node')
    unittest.main()
