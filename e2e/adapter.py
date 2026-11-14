#!/usr/bin/env python3
"""Read-only discovery/history; run records real Rune renderer evidence."""
import argparse
import datetime as dt
import json
from pathlib import Path
import shutil
import subprocess
import sys
import uuid

ROOT = Path(__file__).resolve().parents[1]
RUNS = ROOT / 'out/e2e-runs'
CASE = 'island-renderer'

def emit(value):
    print(json.dumps(value, indent=2))

def now():
    return dt.datetime.now(dt.timezone.utc).isoformat()

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('command', choices=['list', 'history', 'run'])
    parser.add_argument('case', nargs='?', default=CASE)
    args = parser.parse_args()
    if args.command == 'list':
        emit({'version': 1, 'cases': [{'id': CASE,
            'title': 'Rune island expand, select, reverse and collapse',
            'map': 'e2e/maps/island.toml',
            'files': ['scripts/test-e2e.sh', 'scripts/island-qa.json', 'src/docking.rs']}]})
        return 0
    if args.command == 'history':
        emit({'version': 1, 'runs': [json.loads(p.read_text())
             for p in sorted(RUNS.glob('*/result.json'))]})
        return 0
    if args.case != CASE:
        parser.error('unknown case: ' + args.case)
    run_id = dt.datetime.now(dt.timezone.utc).strftime('%Y%m%dT%H%M%SZ-') + uuid.uuid4().hex[:8]
    dest = RUNS / run_id
    dest.mkdir(parents=True)
    result = {'version': 1, 'caseId': CASE, 'runId': run_id,
              'startedAt': now(), 'env': 'macos-rune-headless', 'checks': [], 'artifacts': []}
    try:
        with (dest / 'log.txt').open('w') as log:
            completed = subprocess.run(['sh', 'scripts/test-e2e.sh'], cwd=ROOT,
                                       stdout=log, stderr=subprocess.STDOUT, timeout=600)
        result['verdict'] = 'PASS' if completed.returncode == 0 else 'FAIL'
        result['checks'].append({'label': 'Renderer interaction flow and docking regressions',
            'status': 'pass' if completed.returncode == 0 else 'fail', 'at': 'suite',
            'detail': 'cargo tests, release build, Rune scripted frames and layout assertions',
            'evidence': {'artifact': 'log.txt'}})
        # Copy only this successful invocation's frames; never reuse stale evidence on failure.
        if completed.returncode == 0:
            for name in ('expanded.png', 'expanded.json', 'compact.png', 'compact.json', 'left.png', 'left.json', 'right.png', 'right.json', 'right-expanded.png', 'right-expanded.json'):
                shutil.copy2(ROOT / 'out/e2e' / name, dest / name)
    except Exception as exc:
        result['verdict'] = 'ERROR'
        result['checks'].append({'label': 'Test harness', 'status': 'fail', 'at': 'harness', 'detail': str(exc)})
    result['checks'].append({'label': 'Native window coverage', 'status': 'info', 'at': 'native',
        'detail': 'Headless tests do not prove physical edge dragging, multiple monitors or cross-app image drops.'})
    result['finishedAt'] = now()
    result['artifacts'] = [{'label': p.name, 'path': p.relative_to(ROOT).as_posix()}
                           for p in sorted(dest.iterdir()) if p.is_file()]
    (dest / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
    emit(result)
    return 0 if result['verdict'] == 'PASS' else 1

if __name__ == '__main__':
    sys.exit(main())
