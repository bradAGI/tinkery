"""Noninteractive CLI checks; every actual launch has a temporary guarded TEST HOME."""
from pathlib import Path
import os
import tempfile

import test_launch as subprocess

BINARY = Path(__file__).resolve().parents[1] / 'target/debug/tinkery'
ENV = subprocess.environment()

for arguments in (['--help'], ['--snapshot'], ['--no-color', '--snapshot']):
    result = subprocess.run([str(BINARY), *arguments], env=ENV, capture_output=True, check=True)
    assert b'\x1b' not in result.stdout, 'Noninteractive mode emitted terminal escapes'

result = subprocess.run([str(BINARY), '--help'], env=ENV, capture_output=True, check=True)
assert b'prototype' not in result.stdout
assert b'--workbench' not in result.stdout
assert b'--no-color   Turn off colors' in result.stdout

result = subprocess.run([str(BINARY)], env=ENV, capture_output=True)
assert result.returncode != 0 and b'needs a terminal' in result.stderr
result = subprocess.run([str(BINARY), '--unknown'], env=ENV, capture_output=True)
assert result.returncode != 0 and b'unknown argument' in result.stderr
for removed in ('--scratchpad', '--workbench', '--seed-me', '--taste-result'):
    result = subprocess.run([str(BINARY), removed], env=ENV, capture_output=True)
    assert result.returncode != 0 and f'unknown argument: {removed}'.encode() in result.stderr
for arguments, error in [
    (['--shape-pi'], b'requires an explicit --model'),
    (['--model', 'test/model'], b'require --shape-pi'),
    (['--thinking', 'low'], b'require --shape-pi'),
    (['--record-root', ''], b'--record-root needs a folder'),
    (['--shape-pi', '--model', 'bare'], b'explicit model: provider/model-id'),
    # Think alone has no Watch, Approve or Taste, so their input is refused, not ignored.
    (['--snapshot', '--sdlc-root', 'TEST-sdlc'], b'requires --home'),
]:
    result = subprocess.run([str(BINARY), *arguments], env=ENV, capture_output=True)
    assert result.returncode != 0 and error in result.stderr, (arguments, result.stderr)
    assert b'\x1b' not in result.stdout

assert b'--taste-result' not in subprocess.check_output([str(BINARY), '--help'], env=ENV)

binding = '00000000-0000-4000-8000-000000000729:00000000-0000-7000-8000-000000000729:TEST report:one.json'
for arguments, error in [
    (['--codex-report'], b'needs RUN_UUID:CHAT_ID:PATH'),
    (['--codex-report', ''], b'needs RUN_UUID:CHAT_ID:PATH'),
    (['--codex-report', binding.replace('4000', '7000', 1)], b'canonical UUIDs'),
    (['--codex-report', 'bad:' + binding.split(':', 1)[1]], b'canonical UUIDs'),
    (['--codex-report', binding.split(':', 1)[0] + '::TEST-file'], b'canonical UUIDs'),
    (['--codex-report', binding.rsplit(':', 2)[0] + ':'], b'canonical UUIDs'),
    (['--codex-report', binding, '--codex-report', binding], b'only once'),
    (['--codex-report', binding], b'requires --home and --sdlc-root'),
    (['--home', '--codex-report', binding], b'requires --home and --sdlc-root'),
]:
    result = subprocess.run([str(BINARY), *arguments], env=ENV, capture_output=True)
    assert result.returncode != 0 and error in result.stderr, (arguments, result.stderr)
assert b'--codex-report RUN_UUID:CHAT_ID:PATH' in subprocess.check_output(
    [str(BINARY), '--help'], env=ENV)

# Invalid-HOME scenarios now stop at the test boundary, never by executing Tinkery.
# Rust's pure root tests retain the application-level invalid-HOME contracts.
no_home = {key: value for key, value in ENV.items() if key != 'HOME'}
for env in (no_home, {**no_home, 'HOME': ''}, {**no_home, 'HOME': 'TEST-relative'}):
    for arguments in (['--snapshot'], ['--home', '--snapshot'],
                      ['--shape-pi', '--model', 'test/model']):
        try:
            subprocess.run([str(BINARY), *arguments], env=env, capture_output=True)
        except subprocess.UnsafeTestHome:
            pass
        else:
            raise AssertionError('Unsafe HOME bypassed the shared test launch guard')

with tempfile.TemporaryDirectory(prefix='tinkery-TEST-link-cli-') as directory:
    root = Path(directory)
    for entry in ([], ['--home']):
        result = subprocess.run([str(BINARY), *entry, '--snapshot',
                                 '--seed-session-root', str(root / 'TEST-sessions'),
                                 '--link-root', str(root / 'TEST-links')],
                                env=ENV, capture_output=True)
        assert result.returncode == 0, result.stderr
    for arguments in (['--link-root'], ['--link-root', '']):
        result = subprocess.run([str(BINARY), *arguments], env=ENV, capture_output=True)
        assert result.returncode != 0 and b'--link-root needs a folder' in result.stderr
    assert not (root / 'TEST-links').exists()
assert b'--link-root PATH' in subprocess.check_output([str(BINARY), '--help'], env=ENV)
print('PASS: guarded TEST-HOME CLI modes, removed flags, explicit errors and practice link roots')
