from terminal_process import wait_output
from pathlib import Path
import hashlib
import json
import os
import re
import test_launch as subprocess
import tempfile
import unittest

BINARY = Path(__file__).resolve().parents[1] / 'target/debug/tinkery'


class SessionRootTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='tinkery-TEST-roots-')
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.cwd = self.root / 'TEST-cwd'
        self.cwd.mkdir()
        self.env = {'PATH': os.environ['PATH'], 'TERM': 'xterm-256color',
                    'TINKERY_NODE': '/tinkery-TEST-no-node', 'PYTHONDONTWRITEBYTECODE': '1'}
        self.host = self.root / 'TEST-never-pi'
        self.called = self.root / 'TEST-provider-called'
        self.host.write_text('#!/bin/sh\nprintf unexpected > "' + str(self.called) + '"\nexit 99\n')
        self.host.chmod(0o700)

    def environment(self, home):
        env = dict(self.env)
        if home is not None:
            env['HOME'] = home
            if Path(home).is_absolute():
                return subprocess.environment(env, home=home)
        return env

    def run_cli(self, args, home):
        return subprocess.run([str(BINARY), *map(str, args)], cwd=self.cwd,
                              env=self.environment(home), capture_output=True, text=True, timeout=10)

    def ledger(self, root, name='TEST-session'):
        file = root / name / 'ledger.json'
        file.parent.mkdir(parents=True)
        file.write_text(json.dumps({'status': 'active', 'goal': 'TEST root sentinel'}))
        return file

    def has_count(self, output, count):
        return re.search(r'\btend\s+' + str(count) + r'\b', output) is not None

    def live_args(self):
        return ['--shape-pi', '--model', 'test/model', '--pi-command', self.host]

    def tree(self, root):
        return {str(p.relative_to(root)): None if p.is_dir() else p.read_bytes()
                for p in root.rglob('*')}

    def test_snapshots_never_migrate_or_create_session_data(self):
        for old_exists, new_exists in ((True, False), (False, True), (True, True), (False, False)):
            for menu in (False, True):
                with self.subTest(old=old_exists, new=new_exists, menu=menu):
                    home = self.root / f'TEST-home-{old_exists}-{new_exists}-{menu}'
                    home.mkdir()
                    if old_exists:
                        self.ledger(home / '.local/share/seed-me/sessions')
                    if new_exists:
                        self.ledger(home / '.local/share/tinkery/sessions')
                    before = self.tree(home)
                    result = self.run_cli(['--operator', 'agent', '--snapshot'] + (['--home'] if menu else []), str(home))
                    self.assertEqual(self.tree(home), before)
                    if old_exists:
                        self.assertNotEqual(result.returncode, 0)
                        self.assertEqual(result.stdout, '')
                        expected = 'Both old and new sessions roots exist' if new_exists else 'Snapshot mode does not move saved sessions'
                        self.assertIn(expected, result.stderr)
                        if not new_exists:
                            self.assertIn('Open Tinkery normally', result.stderr)
                            self.assertIn('retry --snapshot', result.stderr)
                    else:
                        self.assertEqual(result.returncode, 0, result.stderr)
                        self.assertTrue(result.stdout.strip())
                        self.assertNotIn('Moved saved sessions', result.stderr)
        self.assertFalse(self.called.exists())

    def test_explicit_snapshot_root_never_migrates_default_sessions(self):
        home = self.root / 'TEST-home'
        self.ledger(home / '.local/share/seed-me/sessions')
        self.ledger(home / '.local/share/tinkery/sessions')
        selected = self.root / 'TEST-selected'
        self.ledger(selected)
        before = self.tree(self.root)
        result = self.run_cli(['--operator', 'agent', '--home', '--snapshot', '--seed-session-root', selected], str(home))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(self.has_count(result.stdout, 1), result.stdout)
        self.assertEqual(self.tree(self.root), before)
        self.assertNotIn('Moved saved sessions', result.stderr)

    def test_one_time_move_preserves_session_bytes_and_reports_it(self):
        home = self.root / 'TEST-home'
        old = home / '.local/share/seed-me/sessions'
        new = home / '.local/share/tinkery/sessions'
        sentinel = self.ledger(old)
        before = sentinel.read_bytes()
        self.run_ordinary(home)
        self.assertFalse(old.exists())
        self.assertEqual((new / sentinel.relative_to(old)).read_bytes(), before)
        again = self.run_cli(['--operator', 'agent', '--home', '--snapshot'], str(home))
        self.assertEqual(again.returncode, 0, again.stderr)
        self.assertNotIn('Moved saved sessions', again.stderr)
        self.assertTrue(self.has_count(again.stdout, 1))

    def test_leftover_old_root_refuses_without_merging_or_reading_it(self):
        home = self.root / 'TEST-home'
        old = home / '.local/share/seed-me/sessions'
        new = home / '.local/share/tinkery/sessions'
        old_file = self.ledger(old)
        new_file = self.ledger(new)
        before = (old_file.read_bytes(), new_file.read_bytes())
        result = self.run_cli(['--operator', 'agent', '--home', '--snapshot'], str(home))
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('Both old and new sessions roots exist', result.stderr)
        self.assertEqual(before, (old_file.read_bytes(), new_file.read_bytes()))
        alias = self.root / 'TEST-old-alias'
        alias.symlink_to(old, target_is_directory=True)
        for args in (['seed', 'checks', alias / old_file.parent.name],
                     ['--home', '--snapshot', '--seed-session-root', old],
                     ['seed', 'status', '--json', old_file.parent],
                     ['seed', 'checks', old_file.parent]):
            result = self.run_cli(['--operator', 'agent', *args], str(home))
            self.assertNotEqual(result.returncode, 0, result.stdout)
            self.assertIn('old sessions root is retired', result.stderr)

    def test_symlinked_old_root_is_refused_without_following_it(self):
        home = self.root / 'TEST-home'
        old = home / '.local/share/seed-me/sessions'
        target = self.root / 'TEST-symlink-target'
        sentinel = self.ledger(target)
        old.parent.mkdir(parents=True)
        old.symlink_to(target, target_is_directory=True)
        before = sentinel.read_bytes()
        result = self.run_cli(['--home', '--snapshot'], str(home))
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('not an ordinary folder', result.stderr)
        self.assertTrue(old.is_symlink())
        self.assertEqual(sentinel.read_bytes(), before)

    def test_retired_root_aliases_and_their_parents_are_refused(self):
        home = self.root / 'TEST-home'
        old = home / '.local/share/seed-me/sessions'
        self.ledger(old, 'TEST-goal')
        (old / 'TEST-child').mkdir()
        (self.cwd / 'TEST-direct').symlink_to(old, target_is_directory=True)
        (self.cwd / 'TEST-alias').symlink_to(old / 'TEST-child', target_is_directory=True)
        before = self.tree(home)
        for selected in (Path('TEST-direct'), Path('TEST-alias/..'),
                         Path('TEST-missing/../TEST-alias/..')):
            commands = [
                ['seed', 'status', '--json', selected / 'TEST-goal'],
                ['seed', 'checks', selected / 'TEST-goal'],
                ['--home', '--snapshot', '--seed-session-root', selected],
                ['--home', '--snapshot', '--seed-session-root', selected / 'TEST-new/TEST-root'],
            ]
            for args in commands:
                with self.subTest(args=args):
                    result = self.run_cli(['--operator', 'agent', *args], str(home))
                    self.assertNotEqual(result.returncode, 0, result.stdout)
                    self.assertIn('old sessions root is retired', result.stderr)
                    self.assertEqual(self.tree(home), before)
        self.assertFalse(self.called.exists())

    def test_linked_retired_storage_refuses_other_aliases_to_its_target(self):
        for linked_path, suffix in (('.local/share', 'seed-me/sessions'),
                                    ('.local/share/seed-me/sessions', '.')):
            home = self.root / ('TEST-home-' + linked_path.replace('/', '-'))
            target = self.root / (home.name + '-target')
            old = target / suffix
            self.ledger(old)
            link = home / linked_path
            link.parent.mkdir(parents=True)
            link.symlink_to(target, target_is_directory=True)
            alias = self.cwd / (home.name + '-alias')
            alias.symlink_to(old, target_is_directory=True)
            before = self.tree(self.root)
            for args in (['seed', 'status', '--json', alias / 'TEST-session'],
                         ['--home', '--snapshot', '--seed-session-root', alias / 'TEST-new']):
                with self.subTest(linked_path=linked_path, args=args):
                    result = self.run_cli(['--operator', 'agent', *args], str(home))
                    self.assertNotEqual(result.returncode, 0, result.stdout)
                    self.assertIn('old sessions root is retired', result.stderr)
                    self.assertEqual(self.tree(self.root), before)
        self.assertFalse(self.called.exists())

    def test_ordinary_root_aliases_preserve_parent_and_missing_path_semantics(self):
        home = self.root / 'TEST-home'
        home.mkdir()
        selected = self.root / 'TEST-selected'
        self.ledger(selected)
        (selected / 'TEST-child').mkdir()
        (self.cwd / 'TEST-direct').symlink_to(selected, target_is_directory=True)
        (self.cwd / 'TEST-alias').symlink_to(selected / 'TEST-child', target_is_directory=True)
        before = self.tree(self.root)
        for alias in (Path('TEST-direct'), Path('TEST-alias/..')):
            for destination, count in ((alias, 1), (alias / 'TEST-new/TEST-root', 0)):
                with self.subTest(destination=destination):
                    result = self.run_cli(['--operator', 'agent', '--home', '--snapshot',
                                           '--seed-session-root', destination], str(home))
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertEqual(self.has_count(result.stdout, 1), count == 1, result.stdout)
                    self.assertEqual(self.tree(self.root), before)
        self.assertFalse(self.called.exists())

    def test_dangling_retired_root_does_not_block_a_safe_explicit_destination(self):
        home = self.root / 'TEST-home'
        old = home / '.local/share/seed-me/sessions'
        old.parent.mkdir(parents=True)
        old.symlink_to(self.root / 'TEST-absent', target_is_directory=True)
        selected = self.root / 'TEST-selected'
        self.ledger(selected)
        result = self.run_cli(['--operator', 'agent', '--home', '--snapshot',
                               '--seed-session-root', selected], str(home))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(self.has_count(result.stdout, 1), result.stdout)
        for destination in (old, self.root / 'TEST-absent'):
            result = self.run_cli(['--operator', 'agent', '--home', '--snapshot',
                                   '--seed-session-root', destination], str(home))
            self.assertNotEqual(result.returncode, 0)
            self.assertIn('old sessions root is retired', result.stderr)
        self.assertFalse((self.root / 'TEST-absent').exists())
        old.unlink()
        old.symlink_to(old, target_is_directory=True)
        result = self.run_cli(['--operator', 'agent', '--home', '--snapshot',
                               '--seed-session-root', selected], str(home))
        self.assertEqual(result.returncode, 0, result.stderr)
        result = self.run_cli(['--operator', 'agent', '--home', '--snapshot',
                               '--seed-session-root', old], str(home))
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('too many symbolic links', result.stderr)

    def test_case_alias_of_existing_retired_storage_is_refused(self):
        home = self.root / 'TEST-home'
        old = home / '.local/share/seed-me/sessions'
        self.ledger(old)
        alias = home / '.LOCAL/share/seed-me/sessions'
        if not alias.exists():
            self.skipTest('Filesystem distinguishes these letter cases')
        before = self.tree(home)
        result = self.run_cli(['--operator', 'agent', '--home', '--snapshot',
                               '--seed-session-root', alias], str(home))
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn('old sessions root is retired', result.stderr)
        self.assertEqual(self.tree(home), before)

    def test_sessions_move_never_reads_or_rewrites_think_records(self):
        home = self.root / 'TEST-home'
        old = home / '.local/share/seed-me/sessions'
        sentinel = self.ledger(old)
        records = home / '.local/share/tinkery/think'
        records.mkdir(parents=True)
        malformed = records / 'TEST-broken.json'
        malformed.write_text('TEST not JSON')
        before = sentinel.read_bytes()
        self.run_ordinary(home)
        self.assertFalse(old.exists())
        moved = home / '.local/share/tinkery/sessions' / sentinel.relative_to(old)
        self.assertEqual(moved.read_bytes(), before)
        self.assertEqual(malformed.read_text(), 'TEST not JSON')
        self.assertEqual(list(records.iterdir()), [malformed])

    def test_old_format_session_moves_with_every_folder_byte_untouched(self):
        home = self.root / 'TEST-home'
        old = home / '.local/share/seed-me/sessions'
        session = old / 'TEST-legacy-id'
        session.mkdir(parents=True)
        ledger = dict(schema_version=1, session_id=session.name, version=0, revision=0,
                      created_at='2026-10-19T00:00:00+00:00', updated_at='2026-10-19T00:00:00+00:00',
                      status='active', draft=dict(goal='', outcome='', options=[]), goal=None,
                      origin=None, current_question=None, frontier=[], nodes=[], assumed=[],
                      operator='human', legacy_viewer='TEST older schema field')
        (session / 'ledger.json').write_text(json.dumps(ledger))
        (session / 'ledger-view.html').write_bytes(b'TEST old view\\r\\n')
        (session / 'TEST-originals').mkdir()
        (session / 'TEST-originals/bytes').write_bytes(b'\\x00TEST original\\xff')
        before = {p.relative_to(session): p.read_bytes() for p in session.rglob('*') if p.is_file()}
        self.run_ordinary(home)
        self.assertFalse(old.exists())
        moved = home / '.local/share/tinkery/sessions' / session.name
        self.assertEqual({p.relative_to(moved): p.read_bytes() for p in moved.rglob('*') if p.is_file()}, before)
        refused = self.run_cli(['seed', 'status', '--json', moved], str(home))
        self.assertNotEqual(refused.returncode, 0)
        self.assertIn('invalid stored ledger fields', refused.stderr)
        self.assertEqual({p.relative_to(moved): p.read_bytes() for p in moved.rglob('*') if p.is_file()}, before)

    def run_ordinary(self, home):
        import fcntl
        import select
        import struct
        import termios
        import time
        from vt_screen import Screen
        master, slave = os.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', 24, 80, 0, 0))
        screen = Screen(80, 24)
        process = subprocess.Popen([str(BINARY), '--operator', 'agent', '--home'],
                                   cwd=self.cwd, env=self.environment(str(home)),
                                   stdin=slave, stdout=slave, stderr=slave)
        try:
            deadline = time.monotonic() + 5
            for _ in wait_output(master, process, screen.feed, deadline):
                if 'Press any key to continue.' in screen.text() and not screen.synchronized:
                    break
            self.assertIn('Moved saved sessions', screen.text())
            self.assertIn('Press any key to continue.', screen.text())
            os.write(master, b'q')
            deadline = time.monotonic() + 5
            for _ in wait_output(master, process, screen.feed, deadline):
                if '1  think' in screen.text() and not screen.synchronized:
                    break
            self.assertIsNone(process.poll(), 'acknowledging the notice also navigated')
            self.assertIn('1  think', screen.text())
            os.write(master, b'q')
            process.wait(timeout=5)
            self.assertEqual(process.returncode, 0)
            self.assertFalse(self.called.exists())
        finally:
            if process.poll() is None:
                process.kill()
                process.wait()
            os.close(master)
            os.close(slave)

    def test_move_notice_is_visible_in_the_terminal_before_navigation(self):
        home = self.root / 'TEST-home'
        self.ledger(home / '.local/share/seed-me/sessions')
        self.run_ordinary(home)

    def test_removed_skill_flag_is_not_an_alias_or_requirement(self):
        home = str(self.root / 'TEST-home')
        help_text = self.run_cli(['--help'], home)
        self.assertNotIn('--seed-me', help_text.stdout)
        removed = self.run_cli(['--seed-me', '/TEST/unused'], home)
        self.assertNotEqual(removed.returncode, 0)
        self.assertIn('unknown argument: --seed-me', removed.stderr)

    def test_invalid_home_is_refused_before_discovery_or_directory_creation(self):
        for home in (None, '', 'TEST-relative-home'):
            with self.subTest(home=home), self.assertRaises(subprocess.UnsafeTestHome):
                self.run_cli(['--home', '--snapshot'], home)
        self.assertEqual([], list(self.cwd.iterdir()))
        self.assertFalse(self.called.exists())

    def test_explicit_roots_work_in_practice_without_persistence(self):
        destination = self.root / 'TEST-selected'
        sentinel = self.ledger(destination)
        before = sentinel.read_bytes()
        for home in (str(self.root / 'TEST-home'),):
            for selected in (destination, Path('../TEST-selected')):
                with self.subTest(home=home, selected=selected):
                    result = self.run_cli(['--home', '--snapshot', '--seed-session-root', selected], home)
                    self.assertEqual(0, result.returncode, result.stderr)
                    self.assertTrue(self.has_count(result.stdout, 1), result.stdout)
                    self.assertEqual(before, sentinel.read_bytes())
        self.assertFalse(self.called.exists())

    def test_empty_explicit_root_is_refused_instead_of_using_the_current_folder(self):
        records = self.root / 'TEST-records'
        for args in (['--home', '--snapshot'], self.live_args() + ['--record-root', records]):
            with self.subTest(args=args):
                result = self.run_cli(args + ['--seed-session-root', ''], str(self.root / 'TEST-home'))
                self.assertNotEqual(0, result.returncode)
                self.assertIn('--seed-session-root needs a location', result.stderr)
                self.assertEqual([], list(self.cwd.iterdir()))
                self.assertFalse(records.exists())
                self.assertFalse(self.called.exists())

    # Unsafe HOME integration cases now exercise the shared refusal before any executable;
    # application-level absent/relative HOME rules remain covered by pure Rust tests.
    def test_live_invalid_home_refuses_before_host_or_binary_access(self):
        for home in (None, '', 'TEST-relative-home'):
            for entry in ([], ['--home']):
                records = self.root / 'TEST-records'
                with self.subTest(home=home, entry=entry), self.assertRaises(subprocess.UnsafeTestHome):
                    self.run_cli(entry + self.live_args() + ['--record-root', records], home)
                self.assertFalse(records.exists())
                self.assertFalse(self.called.exists())

    def test_explicit_session_root_cannot_bypass_the_test_home_guard(self):
        for home in (None, '', 'TEST-relative-home'):
            with self.subTest(home=home), self.assertRaises(subprocess.UnsafeTestHome):
                self.run_cli(self.live_args() + ['--seed-session-root', self.root / 'TEST-sessions'], home)
            self.assertFalse(self.called.exists())

    def test_live_snapshot_cannot_bypass_the_test_home_guard(self):
        for home in (None, '', 'TEST-relative-home'):
            with self.subTest(home=home), self.assertRaises(subprocess.UnsafeTestHome):
                self.run_cli(['--home', '--snapshot'] + self.live_args() +
                             ['--record-root', self.root / 'TEST-records'], home)
            self.assertFalse(self.called.exists())
            self.assertFalse((self.root / 'TEST-records').exists())

    def test_absolute_home_default_and_explicit_override_are_distinct(self):
        home = self.root / 'TEST-home'
        default = home / '.local/share/tinkery/sessions'
        self.ledger(default)
        selected = self.root / 'TEST-selected'
        self.ledger(selected, 'one')
        self.ledger(selected, 'two')
        result = self.run_cli(['--home', '--snapshot'], str(home))
        self.assertEqual(0, result.returncode, result.stderr)
        self.assertTrue(self.has_count(result.stdout, 1), result.stdout)
        result = self.run_cli(['--home', '--snapshot', '--seed-session-root', selected], str(home))
        self.assertEqual(0, result.returncode, result.stderr)
        self.assertTrue(self.has_count(result.stdout, 2), result.stdout)


if __name__ == '__main__':
    unittest.main()
