"""Partial worker creation stays truthful through the real PTY and fake SDLC."""
import tempfile
from pathlib import Path

from worker_terminal import END, F2, F10, FAKE, Fixture, Terminal, open_review, journey


def partial_start(width, height, scenario, mono=False):
    with tempfile.TemporaryDirectory(prefix='tinkery-TEST-partial-start-') as directory:
        fixture = Fixture(Path(directory).resolve(), scenario=scenario)
        script = FAKE.replace(
            "if args[0] == 'status':\n",
            "if args[0] == 'status':\n"
            "    if len(args) == 3 and state.get('run') and not state.get('failed'):\n"
            "        state['failed'] = True\n"
            "        save()\n"
            "        if state['scenario'] == 'identity-read':\n"
            "            refuse('TEST injected identity read failure after new persisted')\n"
            "        if state['scenario'] == 'identity-invalid':\n"
            "            print(json.dumps(envelope(run=dict(run(), run_id='invalid'))))\n"
            "            sys.exit(0)\n")
        script = script.replace(
            "    print('TEST created run')",
            "    if state['scenario'] == 'new-error':\n"
            "        refuse('TEST new persisted then failed')\n"
            "    print('TEST created run')")
        (fixture.project / '_system/scripts/sdlc.py').write_text(script)
        fixture.git('add', '_system/scripts/sdlc.py')
        fixture.git('commit', '-qm', 'TEST partial start injection')
        terminal = Terminal(fixture, width, height, mono=mono)
        try:
            open_review(terminal)
            terminal.send(END + F2, 'Request refused or uncertain:')
            terminal.send(END, 'Request refused or uncertain:')
            shown = terminal.flat()
            assert fixture.read()['run']['slug'] == 'test-worker'
            assert (fixture.project / 'runs/test-worker/run.json').is_file()
            assert fixture.read()['run']['seed_contract'] is None
            assert not fixture.read().get('step') and not fixture.admissions()
            assert 'Nothing was started' not in shown, shown
            assert 'F2 start worker' not in shown and 'F2 tries again' not in shown, shown
            assert 'test-worker' in shown and 'Watch' in shown, shown
            before = fixture.mutations()
            terminal.send(F2 * 3 + b'?', 'Worker actions')
            terminal.send(b'\x1b', 'Review worker start')
            terminal.send(END, 'Watch')
            terminal.send(F10, '1 think')
            terminal.send(b'5', 'test-worker')
            assert fixture.mutations() == before, fixture.calls()
            assert not (fixture.root / 'unexpected-provider').exists()
            print(f'PASS partial start {scenario}: {width}x{height}, mono={mono}; Watch reads saved run, no retry')
        finally:
            terminal.close()


if __name__ == '__main__':
    for case in ('identity-read', 'identity-invalid', 'new-error'):
        partial_start(80, 24, case)
        partial_start(100, 30, case, mono=True)
    journey(80, 24, 'dirty-source', mono=True)
