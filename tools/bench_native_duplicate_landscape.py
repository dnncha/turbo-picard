#!/usr/bin/env python3
"""Matched coordinate-BAM duplicate pipelines; scoped evidence, not WGS validation.

Picard, Turbo Picard and FastDup accept the original coordinate BAM directly.
samtools and dupblaster include their required preparation and final coordinate
sort in the timer. Full Picard output parity is distinct from the diagnostic
unordered mandatory-field/read-group comparison; the latter never qualifies a
tool for a full-contract speedup claim. Pipeline RSS is GNU time's maximum child
RSS, not simultaneous aggregate memory. Inputs and failed outputs are retained.
"""
from __future__ import annotations

import argparse
from collections import Counter
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess
import sys

from bench_bounded_markduplicates import digest_file, measure, read_histograms, summarize_bam
from compare_markduplicates import read_metrics


def pipeline_commands(tool, binary, samtools, source, output, metrics, scratch):
    """Best streaming preparation from the same coordinate input, one CPU."""
    collate = [samtools, 'collate', '--no-PG', '-@', '0', '-O', '-u', str(source), str(scratch / 'collate')]
    sort = [samtools, 'sort', '--no-PG', '-@', '0', '-m', '256M', '-T', str(scratch / 'sort')]
    if tool == 'samtools':
        return [collate, [samtools, 'fixmate', '--no-PG', '-@', '0', '-m', '-u', '-', '-'],
                [*sort, '-l', '0', '-'],
                [samtools, 'markdup', '--no-PG', '-@', '0', '-m', 's', '-c',
                 '--output-fmt-option', 'level=5', '-T', str(scratch / 'markdup'),
                 '-f', str(metrics), '-', str(output)]]
    if tool == 'dupblaster':
        return [collate, [binary, '-i', '-', '-o', '-', '-l', '0',
                         '--metrics-prefix', str(metrics), '--tmp-dir', str(scratch),
                         '--single-end-strategy', 'picard-exact', '--library-aware', 'on',
                         '--sequencing-duplicate-detection', 'off'],
                [*sort, '-l', '5', '-o', str(output), '-']]
    raise ValueError(f'unknown pipeline: {tool}')


def run_pipeline(commands):
    processes = []
    previous = None
    try:
        for index, command in enumerate(commands):
            process = subprocess.Popen(command, stdin=previous,
                                       stdout=subprocess.PIPE if index + 1 < len(commands) else None)
            if previous is not None:
                previous.close()
            processes.append(process)
            previous = process.stdout
        codes = [process.wait() for process in reversed(processes)]
        return next((code for code in codes if code), 0)
    finally:
        if previous is not None:
            previous.close()
        for process in processes:
            if process.poll() is None:
                process.kill()
        for process in processes:
            process.wait()


def mandatory_records(path, samtools):
    """Unordered first 11 SAM fields plus RG, strictly diagnostic only."""
    records = Counter()
    with subprocess.Popen([samtools, 'view', str(path)], stdout=subprocess.PIPE, text=True) as process:
        for line in process.stdout:
            fields = line.rstrip('\n').split('\t')
            if len(fields) < 11:
                raise ValueError('malformed SAM record')
            records[tuple(fields[:11] + sorted(tag for tag in fields[11:] if tag.startswith('RG:')))] += 1
    if process.returncode:
        raise RuntimeError('samtools view failed')
    return records


def full_summary(output, metrics, samtools, directory):
    summary = summarize_bam(output, samtools, directory)
    table = read_metrics(metrics) if metrics.exists() else []
    summary['duplication_metrics'] = [table[0], *sorted(table[1:])] if table else []
    try:
        summary['histograms'] = read_histograms(metrics) if metrics.exists() else []
    except ValueError as error:
        summary['histogram_error'] = str(error)
    return summary


def main():
    if len(sys.argv) > 1 and sys.argv[1] == '_pipeline':
        tool, binary, samtools, source, output, metrics, scratch = sys.argv[2:]
        return run_pipeline(pipeline_commands(tool, binary, samtools, Path(source),
                                             Path(output), Path(metrics), Path(scratch)))
    parser = argparse.ArgumentParser(description=__doc__)
    for tool in ('candidate', 'picard', 'samtools', 'fastdup', 'dupblaster'):
        parser.add_argument('--' + tool, type=Path, required=True)
    parser.add_argument('--input', action='append', required=True, help='NAME=coordinate-input.bam')
    parser.add_argument('--output-dir', type=Path, required=True)
    parser.add_argument('--repeats', type=int, default=3)
    parser.add_argument('--candidate-source', required=True)
    args = parser.parse_args()
    if args.repeats < 1:
        parser.error('repeats must be positive')
    inputs = [item.split('=', 1) for item in args.input]
    if any(len(item) != 2 or not item[0] or item[0] in ('.', '..') or Path(item[0]).name != item[0] for item in inputs):
        parser.error('inputs require a simple NAME=PATH')
    if len({item[0] for item in inputs}) != len(inputs):
        parser.error('input names must be unique')
    root = args.output_dir.resolve()
    root.mkdir(parents=True, exist_ok=False)
    samtools = str(args.samtools.resolve())
    binaries = {name: str(getattr(args, name).resolve()) for name in
                ('picard', 'candidate', 'fastdup', 'samtools', 'dupblaster')}
    env = {key: value for key, value in os.environ.items() if not key.startswith('TURBO_PICARD_')}
    env.update(TURBO_PICARD_REQUIRE_NATIVE='1', TURBO_PICARD_THREADS='0',
               JAVA_TOOL_OPTIONS='-Xmx2g -XX:ActiveProcessorCount=1')
    affinity = ['taskset', '-c', str(min(os.sched_getaffinity(0)))]
    report = {'scope': __doc__, 'contract': 'bounded-v4 full output (exact RG date instants); mandatory/RG multiset is diagnostic only',
              'candidate_source': args.candidate_source, 'host': platform.platform(),
              'affinity': affinity, 'compression_level': 5, 'samtools_sort_memory': '256M',
              'repeats': args.repeats, 'warmups': 1, 'cache': 'warm cache; no fsync or cache dropping',
              'harness_sha256': digest_file(Path(__file__)), 'executables': {},
              'inputs': [], 'runs': [], 'summaries': [], 'candidate_pass': True}
    for name, binary in binaries.items():
        version = subprocess.run([binary, '--version'], capture_output=True, text=True, timeout=60)
        report['executables'][name] = {'path': binary, 'sha256': digest_file(Path(binary)),
                                     'version': version.stdout + version.stderr, 'version_exit': version.returncode}
    def save():
        (root / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    for name, input_path in inputs:
        source = Path(input_path).resolve()
        source_hash = digest_file(source)
        report['inputs'].append({'name': name, 'path': str(source), 'sha256': source_hash, 'bytes': source.stat().st_size})
        case_root = root / name
        case_root.mkdir()
        reference = reference_records = None
        for repeat in range(args.repeats + 1):
            order = list(binaries) if repeat % 2 == 0 else list(reversed(binaries))
            # Establish the real Picard oracle first, never a competing output.
            if reference is None:
                order.remove('picard')
                order.insert(0, 'picard')
            for tool in order:
                destination = case_root / f'{tool}-{repeat}'
                output, metrics = destination / 'output.bam', destination / 'metrics.txt'
                scratch = destination / 'scratch'
                if tool in ('candidate', 'picard'):
                    argv = [binaries[tool], 'MarkDuplicates', f'I={source}', f'O={output}',
                            f'M={metrics}', f'TMP_DIR={scratch}', 'READ_NAME_REGEX=null',
                            'ASSUME_SORTED=true', 'VALIDATION_STRINGENCY=SILENT', 'QUIET=true',
                            'ADD_PG_TAG_TO_READS=false', 'COMPRESSION_LEVEL=5', 'CLEAR_DT=true',
                            'TAG_DUPLICATE_SET_MEMBERS=false', 'TAGGING_POLICY=DontTag']
                elif tool == 'fastdup':
                    argv = [binaries[tool], '--input', str(source), '--output', str(output),
                            '--metrics', str(metrics), '--num-threads', '1', '--read-name-regex', 'null',
                            '--tagging-policy', 'DontTag']
                else:
                    argv = [sys.executable, str(Path(__file__).resolve()), '_pipeline', tool,
                            binaries[tool], samtools, str(source), str(output), str(metrics), str(scratch)]
                run_env = {**env, 'TMPDIR': str(scratch)}
                row = measure([*affinity, *argv], destination, run_env)
                row.update(case=name, tool=tool, repeat=repeat, warmup=repeat == 0,
                           full_parity=False, mandatory_rg_multiset_match=False)
                if tool in ('samtools', 'dupblaster'):
                    row['pipeline_argv'] = pipeline_commands(tool, binaries[tool], samtools, source, output, metrics, scratch)
                    row['rss_scope'] = 'maximum child RSS; simultaneous pipeline aggregate not measured'
                if row['exit_code'] == 0:
                    summary = full_summary(output, metrics, samtools, destination)
                    records = mandatory_records(output, samtools)
                    if reference is None:
                        reference, reference_records = summary, records
                        if not summary['duplication_metrics'] or 'histogram_error' in summary:
                            raise RuntimeError('invalid Picard oracle metrics')
                    row.update(full_parity=summary == reference,
                               mandatory_rg_multiset_match=records == reference_records,
                               differences=sorted(key for key in set(summary) | set(reference) if summary.get(key) != reference.get(key)),
                               output_summary=summary, output_bytes=output.stat().st_size)
                    if row['full_parity']:
                        output.unlink()
                if tool in ('candidate', 'picard') and not row['full_parity']:
                    report['candidate_pass'] = False
                report['runs'].append(row)
                save()
        if digest_file(source) != source_hash:
            raise RuntimeError('input changed during evaluation')
        summary = {'case': name}
        for tool in binaries:
            rows = [row for row in report['runs'] if row['case'] == name and row['tool'] == tool and not row['warmup']]
            successful = [row for row in rows if row['exit_code'] == 0]
            summary[tool] = {'successful_runs': len(successful), 'full_parity': all(row['full_parity'] for row in rows),
                             'mandatory_rg_multiset_match': all(row['mandatory_rg_multiset_match'] for row in rows)}
            if successful:
                summary[tool].update(median_wall_seconds=statistics.median(row['wall_seconds'] for row in successful),
                                     median_peak_rss_bytes=statistics.median(row['peak_rss_bytes'] for row in successful))
        report['summaries'].append(summary)
        save()
        print(json.dumps(summary), flush=True)
    return 0 if report['candidate_pass'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
