"""Closing a terminal exits promptly, even when its child inherits ignored SIGHUP."""
import fcntl
import os
from pathlib import Path
import resource
import signal
import struct
import tempfile
import termios
import time
import unittest

import test_launch as subprocess
from terminal_process import wait_output

BINARY = Path(__file__).resolve().parents[1] / 'target/debug/tinkery'


def child_cpu():
    usage = resource.getrusage(resource.RUSAGE_CHILDREN)
    return usage.ru_utime + usage.ru_stime


def hangup(ignore_hup, width, height):
    with tempfile.TemporaryDirectory(prefix='tinkery-TEST-hangup-') as directory:
        root = Path(directory)
        env = subprocess.environment()
        env.update(TERM='xterm-256color', TINKERY_NODE='/tinkery-TEST-no-node')
        master, slave = os.openpty()
        process = None
        cpu_before = child_cpu()

        def setup():
            os.setsid()
            fcntl.ioctl(0, termios.TIOCSCTTY, 0)
            signal.signal(signal.SIGHUP, signal.SIG_IGN if ignore_hup else signal.SIG_DFL)

        try:
            fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', height, width, 0, 0))
            process = subprocess.Popen(
                [str(BINARY), '--operator', 'agent', '--home', '--no-color',
                 '--seed-session-root', str(root / 'sessions'),
                 '--record-root', str(root / 'think'), '--link-root', str(root / 'links')],
                env=env, stdin=slave, stdout=slave, stderr=slave, preexec_fn=setup, cwd=root)
            os.close(slave)
            slave = None
            output = bytearray()
            deadline = time.monotonic() + 5
            for returncode in wait_output(master, process, output.extend, deadline):
                if b'1  think' in output or returncode is not None:
                    break
            assert b'1  think' in output, f'Menu did not draw: {output!r}'
            os.close(master)
            master = None
            started = time.monotonic()
            timed_out = False
            try:
                process.wait(timeout=2)
            except subprocess.TimeoutExpired:
                timed_out = True
                process.kill()
                process.wait()
            elapsed = time.monotonic() - started
            cpu = child_cpu() - cpu_before
            print(f'hangup {width}x{height}, ignored SIGHUP={ignore_hup}: '
                  f'exit={process.returncode}, timed_out={timed_out}, '
                  f'elapsed={elapsed:.3f}s, child CPU={cpu:.3f}s', flush=True)
            assert not timed_out, 'Terminal hangup left Tinkery running for more than 2 seconds'
            assert cpu < 1, f'Terminal hangup consumed sustained CPU: {cpu:.3f}s'
        finally:
            if process is not None:
                if process.poll() is None:
                    process.kill()
                process.wait()
            for fd in (master, slave):
                if fd is not None:
                    os.close(fd)


class Hangup(unittest.TestCase):
    def test_terminal_close_exits_with_default_and_ignored_sighup(self):
        for width, height in ((100, 30), (80, 24)):
            for ignore_hup in (False, True):
                with self.subTest(width=width, height=height, ignore_hup=ignore_hup):
                    hangup(ignore_hup, width, height)


if __name__ == '__main__':
    unittest.main()
