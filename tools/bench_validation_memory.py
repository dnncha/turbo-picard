#!/usr/bin/env python3
"""Compare validation-helper memory, not native Picard command performance.

Use a pristine historical compare_real_data.py as --baseline-file. Each digest
runs in a fresh process; order alternates across repeats. The fixture is wholly
synthetic and no scientific or production-throughput claim follows from it.
"""
from __future__ import annotations
import argparse
import hashlib
import importlib.util
import json
import math
from pathlib import Path
import platform
import resource
import statistics
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
METHODS = ('digest_coordinate_sorted_sam_multiset', 'digest_sam_records_and_read_groups', 'digest_markduplicates_semantics')


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open('rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            digest.update(block)
    return digest.hexdigest()


def worker(module_path: Path, input_path: Path, method: str) -> dict:
    sys.path.insert(0, str(ROOT / 'tools'))
    spec = importlib.util.spec_from_file_location('validation_memory_subject', module_path)
    if spec is None or spec.loader is None:
        raise ValueError(f'cannot load comparison helper: {module_path}')
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    start = time.perf_counter()
    digest = getattr(module, method)(input_path)
    seconds = time.perf_counter() - start
    rss = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
    return {'digest': digest, 'seconds': seconds,
            'peak_rss_bytes': int(rss if sys.platform == 'darwin' else rss * 1024)}


def summarize(results: dict[str, list[dict]]) -> dict:
    if set(results) != {'baseline', 'candidate'}:
        raise ValueError('baseline and candidate measurements are required')
    all_rows = [row for group in results.values() for row in group]
    if not all_rows or any(not rows for rows in results.values()):
        raise ValueError('both implementations need completed measurements')
    if any(not math.isfinite(row['seconds']) or row['seconds'] <= 0 or not math.isfinite(row['peak_rss_bytes']) or row['peak_rss_bytes'] <= 0 for row in all_rows):
        raise ValueError('measurements must be positive and finite')
    if len({row['digest'] for row in all_rows}) != 1:
        raise ValueError('digest mismatch: refusing to report a performance comparison')
    summaries = {name: {'median_seconds': statistics.median(row['seconds'] for row in rows),
                        'median_peak_rss_bytes': statistics.median(row['peak_rss_bytes'] for row in rows)}
                 for name, rows in results.items()}
    return {'digest_parity': 'PASS', 'digest': all_rows[0]['digest'], 'measurements': results,
            'summary': summaries,
            'peak_rss_ratio_baseline_over_candidate': summaries['baseline']['median_peak_rss_bytes'] / summaries['candidate']['median_peak_rss_bytes'],
            'time_ratio_baseline_over_candidate': summaries['baseline']['median_seconds'] / summaries['candidate']['median_seconds']}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--baseline-file', type=Path)
    parser.add_argument('--candidate-file', type=Path, default=ROOT / 'tools/compare_real_data.py')
    parser.add_argument('--records', type=int, default=1_000_000)
    parser.add_argument('--repeats', type=int, default=3)
    parser.add_argument('--method', choices=METHODS, default=METHODS[0])
    parser.add_argument('--output', type=Path)
    parser.add_argument('--worker', type=Path, help=argparse.SUPPRESS)
    parser.add_argument('--input', type=Path, help=argparse.SUPPRESS)
    args = parser.parse_args()
    if args.worker is not None:
        if args.input is None:
            parser.error('--worker requires --input')
        print(json.dumps(worker(args.worker, args.input, args.method)))
        return 0
    if args.baseline_file is None or args.output is None:
        parser.error('--baseline-file and --output are required')
    if args.records < 1 or args.repeats < 1:
        parser.error('--records and --repeats must be positive')
    if args.output.exists() or args.output.is_symlink():
        parser.error('--output already exists; choose a new report path')
    results: dict[str, list[dict]] = {'baseline': [], 'candidate': []}
    sources = {'baseline': args.baseline_file.resolve(), 'candidate': args.candidate_file.resolve()}
    for path in sources.values():
        if not path.is_file():
            parser.error(f'missing implementation file: {path}')
    source_hashes = {name: sha256_file(path) for name, path in sources.items()}
    with tempfile.TemporaryDirectory(prefix='turbo-picard-memory-bench-') as directory:
        sam = Path(directory) / 'synthetic.sam'
        with sam.open('w') as stream:
            stream.write(f'@HD\tVN:1.6\tSO:coordinate\n@SQ\tSN:synthetic\tLN:{args.records+100}\n@RG\tID:rg1\tSM:synthetic\n')
            for i in range(args.records):
                qname = (i * 1664525 + 1013904223) % (2**32)
                stream.write(f'r{qname:010d}\t0\tsynthetic\t{i+1}\t60\t40M\t*\t0\t0\t{"A"*40}\t{"I"*40}\tRG:Z:rg1\tNM:i:0\n')
        input_digest, input_size = sha256_file(sam), sam.stat().st_size
        for repeat in range(args.repeats):
            order = ('baseline', 'candidate') if repeat % 2 == 0 else ('candidate', 'baseline')
            for name in order:
                completed = subprocess.run([sys.executable, str(Path(__file__).resolve()), '--worker', str(sources[name]),
                    '--input', str(sam), '--method', args.method], check=True, capture_output=True, text=True)
                row = json.loads(completed.stdout)
                row['repeat'] = repeat + 1
                results[name].append(row)
                print(f'{name} repeat {repeat+1}: {row["seconds"]:.3f}s, {row["peak_rss_bytes"] / 1024**2:.1f} MiB', file=sys.stderr)
    if source_hashes != {name: sha256_file(path) for name, path in sources.items()}:
        raise ValueError('source changed during benchmark; report discarded')
    report = {'schema_version': 1, 'scope': 'synthetic validation-helper benchmark; NOT native command or production-workflow performance',
        'method': args.method, 'records': args.records, 'input_bytes': input_size, 'input_sha256': input_digest,
        'python': sys.version, 'platform': platform.platform(), 'run_order': 'alternating; fresh processes; filesystem caches not cleared',
        'memory_measurement': 'peak process RSS from getrusage(RUSAGE_SELF); includes interpreter and imports',
        'source_sha256': source_hashes,
        'supporting_source_sha256': {name: sha256_file(ROOT / 'tools' / name) for name in ('disk_sort.py', 'bench_validation_memory.py')}, **summarize(results)}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open('x') as stream:
        json.dump(report, stream, indent=2, sort_keys=True)
        stream.write('\n')
    print(f'wrote {args.output}')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
