"""Confirmation assertions wait for the input echo, not an already-visible placeholder."""
import os
import signal
import unittest
from unittest import mock

import shape_answer_terminal as journey


class ConfirmationWaitTests(unittest.TestCase):
    def test_confirmation_hint_waits_for_typed_words_before_asserting(self):
        original_type = journey.Run.type
        original_wait = journey.Run.wait
        original_close = journey.Run.close
        paused = set()
        resumed = []

        def type_while_stopped(run, words):
            if words == 'confirm':
                os.kill(run.process.pid, signal.SIGSTOP)
                _, status = os.waitpid(run.process.pid, os.WUNTRACED)
                self.assertTrue(os.WIFSTOPPED(status))
                paused.add(run.process.pid)
            return original_type(run, words)

        def wait_for_echo(run, text, *args, **kwargs):
            if text == 'your words confirm' and run.process.pid in paused:
                self.assertIn('type confirm, then F2', run.flat())
                self.assertNotIn('your words confirm', run.flat())
                os.kill(run.process.pid, signal.SIGCONT)
                paused.remove(run.process.pid)
                resumed.append(run.process.pid)
            return original_wait(run, text, *args, **kwargs)

        def close(run):
            if run.process.pid in paused:
                os.kill(run.process.pid, signal.SIGCONT)
                paused.remove(run.process.pid)
            return original_close(run)

        with mock.patch.object(journey.Run, 'type', type_while_stopped), \
             mock.patch.object(journey.Run, 'wait', wait_for_echo), \
             mock.patch.object(journey.Run, 'close', close), \
             mock.patch.dict(os.environ, NO_COLOR='1'):
            journey.case('confirmation hint', 80, 24)
        self.assertEqual(len(resumed), 1)
        self.assertFalse(paused)


if __name__ == '__main__':
    unittest.main()
