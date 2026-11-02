#!/usr/bin/env python3
"""Release-binary menu latency: launch -> complete synchronized menu frame -> processed input.

POSIX PTY, temporary HOME, synthetic fixture sessions, no providers, no microphone.
Not part of the zero-call suite (it needs a release build and takes minutes):

    cargo build --release --locked
    python3 scripts/bench-menu.py --binary target/release/tinkery --samples 30 --sizes 0,5000

The monotonic clock starts immediately before the process is spawned. "First frame" ends when
the PTY output has delivered a finished synchronized update that shows all seven places.
"Input" is the time from writing one Down arrow, right after that frame, until a finished frame
shows the selection moved (tend's description appears). Page cache is warm: fixtures are written
just before sampling and every sample reuses them; nothing here drops caches or claims a cold disk.
"""
import argparse, fcntl, json, math, os, platform, select, shutil, statistics, struct, subprocess, sys
import tempfile, termios, time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'tests'))
from vt_screen import Screen  # noqa: E402

PLACES = ['think', 'tend', 'shape', 'approve', 'watch', 'taste', 'measure']
TEND_LINE = 'Open saved goals'
WIDTH, HEIGHT = 100, 30


def make_fixture(root, count):
    root.mkdir(parents=True)
    total = 0
    for i in range(count):
        d = root / f'session-{i:06d}'
        d.mkdir()
        status = 'completed' if i % 5 == 0 else 'active'
        ledger = json.dumps(dict(status=status, goal=f'Synthetic goal {i}\nWhy: fixture only',
                                 updated_at=f'2026-10-{1 + i // 1440 % 28:02d}T{i // 60 % 24:02d}:{i % 60:02d}:00Z'))
        (d / 'ledger.json').write_text(ledger)
        total += len(ledger)
        if i % 3 == 0:
            seed = 'status: draft\n# synthetic\n'
            (d / 'seed-contract.md').write_text(seed)
            total += len(seed)
    return total


def sample(binary, home):
    master, slave = os.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', HEIGHT, WIDTH, 0, 0))
    screen = Screen(WIDTH, HEIGHT)
    env = {'HOME': str(home), 'TERM': 'xterm-256color', 'PATH': os.environ.get('PATH', ''),
           'TINKERY_NODE': '/tinkery-TEST-no-node'}
    t0 = time.monotonic_ns()
    process = subprocess.Popen([str(binary), '--home'], stdin=slave, stdout=slave, stderr=slave, cwd=home, env=env)

    def until(done, timeout=10):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if select.select([master], [], [], 0.0005)[0]:
                data = os.read(master, 65536)
                at = time.monotonic_ns()
                screen.feed(data)
                if done() and not screen.synchronized:
                    return at
            if process.poll() is not None:
                break
        raise RuntimeError('timed out:\n' + screen.text())

    try:
        first = until(lambda: all(name in screen.text() for name in PLACES))
        sent = time.monotonic_ns()
        os.write(master, b'\x1b[B')
        done = until(lambda: TEND_LINE in screen.text())
        os.write(master, b'q')
        process.wait(timeout=5)
        return (first - t0) / 1e6, (done - sent) / 1e6, (done - t0) / 1e6
    finally:
        if process.poll() is None:
            process.kill(); process.wait()
        os.close(master); os.close(slave)


def stats(values):
    ordered = sorted(values)
    p95 = ordered[max(0, math.ceil(0.95 * len(ordered)) - 1)]
    return dict(n=len(ordered), median=round(statistics.median(ordered), 2), p95=round(p95, 2), max=round(ordered[-1], 2))


def command(*args):
    try:
        return subprocess.run(args, capture_output=True, text=True, timeout=10).stdout.strip()
    except Exception:
        return 'unknown'


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--binary', required=True)
    ap.add_argument('--samples', type=int, default=30)
    ap.add_argument('--sizes', default='0,5000', help='comma-separated session counts')
    ap.add_argument('--label', default='')
    ap.add_argument('--revision', default='', help='revision the binary was built from (default: git HEAD here)')
    args = ap.parse_args()
    if args.samples < 20:
        sys.exit('at least 20 fresh-process samples are required')
    binary = Path(args.binary).resolve()
    results = dict(label=args.label, binary=str(binary), samples=args.samples, terminal=f'{WIDTH}x{HEIGHT}',
                   os=platform.platform(), cpu=command('sh', '-c', "lscpu | grep -m1 'Model name' | cut -d: -f2").strip(),
                   cores=os.cpu_count(), python=platform.python_version(),
                   revision=args.revision or command('git', 'rev-parse', 'HEAD'), rustc=command('rustc', '--version'),
                   cache='warm page cache; fixtures written just before sampling; no cache dropping', fixtures=[])
    for size in [int(x) for x in args.sizes.split(',')]:
        with tempfile.TemporaryDirectory(prefix='tinkery-bench-') as tmp:
            home = Path(tmp)
            root = home / '.local/share/tinkery/sessions'
            nbytes = make_fixture(root, size) if size else 0
            if not size:
                root.mkdir(parents=True)
            fs = command('stat', '-f', '-c', '%T', str(home))
            rows = [sample(binary, home) for _ in range(args.samples)]
            results['fixtures'].append(dict(sessions=size, ledger_bytes=nbytes, filesystem=fs,
                first_frame_ms=stats([r[0] for r in rows]), input_ms=stats([r[1] for r in rows]),
                launch_to_processed_input_ms=stats([r[2] for r in rows])))
    print(json.dumps(results, indent=2))


main()
