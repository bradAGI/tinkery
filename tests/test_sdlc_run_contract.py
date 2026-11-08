"""One API-word boundary, fixture parity, and field-specific refusal probes.

Run with --revert to remove each named recognition guard temporarily, run its
public-behavior assertion red, and restore the checkout bytes in a finally block.
No runtime, provider, installed package, or operator data is changed.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import test_launch as subprocess
from support.rust_contract_source import test_functions

ROOT = Path(__file__).resolve().parents[1]
READERS = [ROOT / 'src/watch.rs', ROOT / 'src/worker.rs',
           *sorted((ROOT / 'src/watch').glob('*.rs')), *sorted((ROOT / 'src/worker').glob('*.rs')),
           *sorted((ROOT / 'src/taste').glob('*.rs'))]
READERS = [path for path in READERS if not path.name.endswith('tests.rs')]
WORDS = ('blocked', 'running', 'not-run', 'current', 'stale', 'failed', 'interrupted',
         'invalid', 'approved', 'pending', 'changes-requested', 'passed', 'unknown',
         'automated', 'attested', 'prepared', 'active', 'completed', 'stopped',
         'unreadable', 'uncertain', 'ended', 'not-ended', 'cap', 'operator')
FIELDS = {
    'verification_status': ('Verification', 'contract_refuses_verification'),
    'gates[].status': ('GateStatus', 'contract_refuses_gate'),
    'checklist[].status': ('CheckStatus', 'contract_refuses_check_status'),
    'checklist[].proof': ('Proof', 'contract_refuses_proof'),
    'steps[].state': ('StepState', 'contract_refuses_step_state'),
    'steps[].turn': ('TurnStatus', 'contract_refuses_step_turn'),
    'steps[].stopped_by': ('StopReason', 'contract_refuses_step_stop'),
    'worker contract.state': ('StepState', 'contract_refuses_worker_state'),
    'worker contract.stopped_by': ('StopReason', 'contract_refuses_worker_stop'),
    'worker active turn': ('TurnStatus', 'contract_refuses_worker_turn'),
    'preview verification.result': ('CheckStatus', 'contract_refuses_receipt_result'),
}
ENUM_WORDS = {
    'Verification': {'blocked', 'running', 'not-run', 'current', 'stale', 'failed', 'interrupted', 'invalid'},
    'GateStatus': {'approved', 'pending', 'stale', 'changes-requested'},
    'CheckStatus': {'passed', 'pending', 'failed', 'unknown'},
    'Proof': {'automated', 'attested'},
    'StepState': {'prepared', 'active', 'uncertain', 'completed', 'stopped', 'unreadable'},
    'TurnStatus': {'ended', 'not-ended', 'unknown'},
    'StopReason': {'cap', 'operator'},
}
COMPATIBILITY = {
    'summary': 'Watch requires verification, slug, next/error and counts; Taste skips invalid IDs/states and does not require detail.',
    'verification_status': 'Watch accepts invalid; Taste detail refuses it; worker display accepts any string but refuses missing/null/non-string.',
    'verification.result': 'Unknown/missing/null is not a refusal; only passed supports a local preview or old-pass history.',
    'gates[].status': 'Missing/null/non-string is refused; pending is not a decision receipt.',
    'checklist[].status': 'Watch uses its own refusal; Taste/shared ownership reject before Taste result interpretation.',
    'checklist[].proof': 'Must agree with automatic, including missing/null/non-string and unknown words.',
    'steps': 'Omission is supported by Watch; null/malformed is refused. Worker requires a list.',
    'steps[].state': 'Watch accepts unreadable; worker contract refuses it. Worker uses its contract state, not the summary state.',
    'steps[].turn': 'Watch requires an active turn and refuses any present turn on non-active steps, including null; worker ignores non-active turns.',
    'steps[].stopped_by': 'Required for stopped steps/contracts, ignored otherwise.',
    'steps[].owed': 'Missing defaults true only for uncertain Watch steps; null/non-bool is refused.',
    'checklist[].operator_attestation': 'Missing defaults true; null/non-bool is refused, even on automatic items.',
    'checklist[].pending': 'Missing defaults false; null/non-bool is refused.',
    'checklist[].automatic,needs_recheck': 'Required booleans; no coercion.',
    'checklist[].mark': 'Null or object required. Watch requires textual evidence on an object; Taste retains missing evidence as unknown.',
    'verification': 'Taste requires field presence; null is an absent receipt, not a malformed receipt. Worker finish requires passed=true.',
    'decisions': 'Taste accepts omission as empty; present null/non-array is refused.',
    'identity': 'Run slug/ID/checkout must still match each reader’s existing identity requirements.',
    'metadata': 'Stage/counts remain unsigned integers; id/verify/next/errors remain text. Source evidence argv/cwd/log/digest checks remain unchanged.',
}
NON_REFUSED = {'verification.result': 'Run summaries/results retain unfamiliar receipt results without a parse refusal; preview separately requires passed.',
               'worker verification_status': 'Any string is displayed; missing/non-string is refused.'}


def production(path):
    source = path.read_text()
    return source.split('#[cfg(test)]\nmod tests {')[0]


def source_boundary(revision=None):
    words = '|'.join(map(re.escape, WORDS))
    literal = rf'"(?:{words})"'
    recognition = re.compile(rf'{literal}[^\n]{{0,120}}=>|(?:==|!=)\s*(?:Some\()?{literal}')
    display_only = {'src/taste/view.rs': {'let receipt = if item.kind == "automated" {'}}
    violations = []
    for path in READERS:
        if revision is None:
            source = production(path)
        else:
            relative = str(path.relative_to(ROOT))
            files = subprocess.check_output(['git', 'ls-tree', '--name-only', revision, '--', relative],
                                             cwd=ROOT, env=subprocess.environment(), text=True)
            if not files.strip():
                continue
            source = subprocess.check_output(['git', 'show', f'{revision}:{relative}'],
                                              cwd=ROOT, env=subprocess.environment(), text=True)
            source = source.split('#[cfg(test)]\nmod tests {')[0]
        for line, text in enumerate(source.splitlines(), 1):
            display = text.strip() in display_only.get(str(path.relative_to(ROOT)), set())
            if recognition.search(text) and not display and not text.lstrip().startswith(('//', '///')):
                violations.append(f'{path.relative_to(ROOT)}:{line}: {text.strip()}')
        for match in re.finditer(rf'matches!\([\s\S]{{0,160}}?,\s*(?:Some\()?{literal}', source):
            if path.relative_to(ROOT).as_posix() == 'src/taste/next.rs' and re.fullmatch(r'matches!\(\s*i\.kind\.as_str\(\),\s*"automated"', match[0]):
                continue
            line = source[:match.start()].count('\n') + 1
            violations.append(f'{path.relative_to(ROOT)}:{line}: {match[0]}')
    assert not violations, 'Reader-owned API-word recognition:\n' + '\n'.join(violations)


def cargo(test):
    result = subprocess.run(['cargo', '+stable', 'test', '--locked', '--lib', test, '--', '--nocapture'],
                            cwd=ROOT, env=subprocess.environment(), stdout=subprocess.PIPE,
                            stderr=subprocess.STDOUT, text=True, timeout=120)
    assert 'running 0 tests' not in result.stdout, result.stdout
    return result


def fixture_parity():
    baseline = json.loads((ROOT / 'tests/fixtures/sdlc-run-contract-parity.json').read_text())
    count = 0
    for path, functions in baseline.items():
        current = test_functions((ROOT / path).read_text())
        for name, digest in functions.items():
            assert name in current, f'Existing fixture test removed: {path}:{name}'
            assert hashlib.sha256(current[name].encode()).hexdigest() == digest, f'Existing fixture/output assertions changed: {path}:{name}'
            count += 1
    print(f'Fixture parity: {count} original fixture/output tests retained verbatim and rerun.', flush=True)


def check():
    source_boundary()
    source = (ROOT / 'src/sdlc.rs').read_text()
    for kind, words in ENUM_WORDS.items():
        declaration = re.search(rf'word_enum!\({kind}, None,\s*\{{(.*?)\}}\);', source, re.S)
        assert declaration and set(re.findall(r'=> "([^"]+)"', declaration[1])) == words, kind
    assert {kind for kind, _ in FIELDS.values()} == set(ENUM_WORDS)
    fixture_parity()
    for field, (_, test) in FIELDS.items():
        result = cargo(test)
        assert result.returncode == 0, f'{field}: {result.stdout}'
    for reader in ['sdlc::tests::', 'watch::tests::', 'taste::read::tests::', 'taste::preview::tests::', 'taste::evidence::tests::', 'worker::tests::']:
        result = cargo(reader)
        assert result.returncode == 0, result.stdout
    print(f'Run contract: {len(FIELDS)} refused word contexts; {len(NON_REFUSED)} compatibility exceptions; fixture assertions unchanged.')


def revert():
    path = ROOT / 'src/sdlc.rs'
    original = path.read_text()
    try:
        for field, (kind, test) in FIELDS.items():
            fallback = {'Verification': 'Current', 'GateStatus': 'Approved', 'CheckStatus': 'Passed',
                        'Proof': 'Automated', 'StepState': 'Prepared', 'TurnStatus': 'Running',
                        'StopReason': 'Operator'}[kind]
            marker = f'word_enum!({kind}, None,'
            assert original.count(marker) == 1, marker
            mutated = original.replace(marker, f'word_enum!({kind}, Some(Self::{fallback}),', 1)
            path.write_text(mutated)
            result = cargo(test)
            running = re.search(r'running (\d+) tests?', result.stdout)
            failed = re.search(r'test result: FAILED\. \d+ passed; (\d+) failed;', result.stdout)
            assert result.returncode != 0 and running and failed and int(failed[1]) == int(running[1]), result.stdout
            print(f'RED {field}: {test}, {failed[1]}/{running[1]} corresponding assertions failed', flush=True)
            path.write_text(original)
    finally:
        path.write_text(original)
    result = cargo('contract_refuses_')
    assert result.returncode == 0, result.stdout
    print('Restored all refusal guards; refusal assertions pass.')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument('--revert', action='store_true')
    mode.add_argument('--source-at', metavar='REV', help='Check source ownership at a Git revision, without changing files.')
    args = parser.parse_args()
    if args.source_at:
        source_boundary(args.source_at)
    elif args.revert:
        revert()
    else:
        check()
