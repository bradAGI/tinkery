#!/usr/bin/env python3
"""Audit #608's shared waits and the deliberately local PTY pumps.

Run with --check to reject an unclassified local read loop. This inventory covers
conditional and exit-only loops as well as functions named wait. It never imports
or launches journey modules. Feature predicates remain in their journey files.
"""
import argparse
import ast
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

# These loops have no success predicate. Their duration/transport semantics differ
# from wait_output's stop-on-exit semantics. Keys bind both owner and condition so
# another loop added to the same function cannot silently inherit the exception.
LOCAL_PUMPS = {
    ('hidden_review_terminal.py', 'pump',
     'time.monotonic() < end or (run.screen.synchronized and time.monotonic() < end + 3)'):
        'Pump for a fixed duration, extending at most three seconds to finish synchronization.',
    ('reading_scroll_terminal.py', 'pump', 'time.monotonic() < deadline'):
        'Fixed-duration input settling, not a success-predicate wait.',
    ('selection_terminal.py', 'pump', 'time.monotonic() < deadline'):
        'Fixed-duration settling preserves raw clipboard transport evidence.',
    ('selection_terminal.py', 'paste', 'frame'):
        'Nonblocking input transport retries partial writes and pumps output to avoid backpressure.',
    ('shape_terminal.py', 'journey', 'select.select([master], [], [], 0.02)[0]'):
        'Drain currently available output before sampling each of 80 fixed viewport steps; no success predicate.',
    ('shape_incomplete_read_terminal.py', 'journey', 'time.monotonic() < until'):
        'Fixed 80ms viewport sampling interval, not a success-predicate wait.',
    ('theme_terminal.py', 'settle', 'time.monotonic() < deadline'):
        'Fixed-duration theme paint sampling must not stop on an early matching frame.',
    ('think_escape_terminal.py', 'pump', 'time.monotonic() < deadline'):
        'Fixed-duration settling between input events.',
    ('think_question_terminal.py', 'journey', 'time.monotonic() < until'):
        'Fixed 40ms viewport sampling interval, not a success-predicate wait.',
    ('voice_terminal.py', 'journey', 'time.monotonic() < deadline'):
        'Fixed 100ms negative-observation interval before asserting no helper requests.',
}


def inventory(root=ROOT):
    """Return shared call sites, justified local loops, and unclassified loops."""
    shared, local, unknown = [], [], []
    for path in sorted((root / 'tests').glob('*.py')):
        if path.name == 'terminal_process.py':
            continue
        tree = ast.parse(path.read_text())
        parents = {child: node for node in ast.walk(tree) for child in ast.iter_child_nodes(node)}

        def owner(node):
            parent = parents.get(node)
            while parent is not None and not isinstance(parent, ast.FunctionDef):
                parent = parents.get(parent)
            return parent.name if parent else '<module>'

        for node in ast.walk(tree):
            if isinstance(node, ast.Call) and isinstance(node.func, ast.Name) and node.func.id == 'wait_output':
                shared.append((str(path.relative_to(root)), node.lineno, owner(node)))
            if not isinstance(node, ast.While):
                continue
            calls = [child for child in ast.walk(node) if isinstance(child, ast.Call)]
            if not any(ast.unparse(call.func) in ('select.select', 'pump', 'os.read') for call in calls):
                continue
            condition = ast.unparse(node.test)
            key = (path.name, owner(node), condition)
            record = (str(path.relative_to(root)), node.lineno, owner(node), condition)
            if key in LOCAL_PUMPS:
                local.append((*record, LOCAL_PUMPS[key]))
            else:
                unknown.append(record)
    return shared, local, unknown


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    shared, local, unknown = inventory()
    for path, line, owner in shared:
        print(f'SHARED {path}:{line} {owner}')
    for path, line, owner, condition, reason in local:
        print(f'LOCAL {path}:{line} {owner}: {condition}: {reason}')
    for path, line, owner, condition in unknown:
        print(f'UNCLASSIFIED {path}:{line} {owner}: {condition}')
    print(f'{len(shared)} shared waits in {len(set(row[0] for row in shared))} files; '
          f'{len(local)} justified local loops; {len(unknown)} unclassified loops')
    if args.check and unknown:
        raise SystemExit(1)


if __name__ == '__main__':
    main()
