"""A hidden F2 cannot send typed or dictated words; PTYs, TEST roots and fake hosts."""
import ast
import json
import tempfile
from pathlib import Path

from hidden_review_terminal import pump, resize_with_input
from think_budget_terminal import HOST
from think_history_terminal import Terminal

NODE = next(ast.literal_eval(node.value)
            for node in ast.parse(Path(__file__).with_name('voice_terminal.py').read_text()).body
            if isinstance(node, ast.Assign) and any(isinstance(name, ast.Name) and name.id == 'NODE'
                                                   for name in node.targets))

F2 = b'\x1bOQ'
DRAFT = 'TEST café draft\nKeep these exact words. '


def journey(width, height, voice):
    with tempfile.TemporaryDirectory(prefix='tinkery-TEST-hidden-send-') as directory:
        root = Path(directory)
        host = root / 'TEST-pi'
        host.write_text(HOST)
        host.chmod(0o700)
        node = root / 'TEST-node'
        node.write_text(NODE)
        node.chmod(0o700)
        terminal = Terminal(root, 100, 30, [
            '--operator', 'agent', '--shape-pi', '--shape-budget', '2',
            '--model', 'test/budget', '--pi-command', host,
            '--record-root', root / 'TEST-records', '--seed-session-root', root / 'TEST-sessions',
        ], extra={'TINKERY_NODE': str(node)} if voice else None)

        def calls(name):
            path = root / name
            return [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []

        try:
            terminal.wait(lambda screen: "What's on your mind?" in screen)
            terminal.send(b'\x1b[200~' + DRAFT.encode() + b'\x1b[201~', 'Keep these exact words.')
            if voice:
                terminal.wait(lambda screen: (root / 'node-ready.txt').exists())
                pump(terminal)  # The UI polls the helper's ready event before the speak key.
                terminal.send(b'\x1b[17~', 'ghost words')
            # The resize and F2 are both pending before the app can process either.
            resize_with_input(terminal, width, height, F2)
            terminal.wait(lambda screen: 'Need 80×24.' in screen)
            assert calls('TEST-calls.jsonl') == [], 'hidden F2 called the model host'
            if voice:
                assert [call['command'] for call in calls('voice-calls.jsonl')] == ['start']
            resize_with_input(terminal, 100, 30)
            terminal.wait(lambda screen: 'Keep these exact words.' in screen)
            assert calls('TEST-calls.jsonl') == [], 'restoring the screen sent without a new key'
            terminal.send(F2, 'Which TEST reason?')
            requests = calls('TEST-calls.jsonl')
            assert len(requests) == 2, requests  # One send runs shaping and its decision review.
            assert requests[0]['sources'][0]['text'] == DRAFT + ('spoken words' if voice else '')
            assert requests[1]['kind'] == 'decision-review'
            assert not (root / 'TEST-records').exists()
            print(f'hidden send {width}×{height}, voice={voice}: no hidden send; '
                  'fresh visible F2 sends exact words once')
        finally:
            terminal.close()


if __name__ == '__main__':
    for size in [(79, 24), (100, 23)]:
        for voice in [False, True]:
            journey(*size, voice)
