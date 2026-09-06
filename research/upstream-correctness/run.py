#!/usr/bin/env python3
"""Build pinned upstream code, prove red/green regressions, export narrow patches."""
from __future__ import annotations
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import xml.etree.ElementTree as ET

HERE = Path(__file__).resolve().parent


def run(args: list[str], cwd: Path, log: Path) -> int:
    print('+', ' '.join(args), flush=True)
    with log.open('w') as out:
        result = subprocess.run(args, cwd=cwd, stdout=out, stderr=subprocess.STDOUT)
    print(f'  exit={result.returncode} log={log}', flush=True)
    if result.returncode:
        print('\n'.join(log.read_text(errors='replace').splitlines()[-55:]), flush=True)
    return result.returncode


def replace_once(path: Path, old: str, new: str) -> None:
    text = path.read_text()
    if text.count(old) != 1:
        raise RuntimeError(f'{path}: expected one exact source anchor, got {text.count(old)}')
    path.write_text(text.replace(old, new, 1))


def xml_summary(repo: Path, destination: Path) -> dict:
    files = sorted((repo / 'build/test-results/test').glob('TEST-*.xml'))
    if not files:
        raise RuntimeError('No TestNG/JUnit XML produced; a build failure is not a regression reproduction')
    destination.mkdir(parents=True, exist_ok=True)
    totals = dict(tests=0, failures=0, errors=0, skipped=0)
    failures = []
    for file in files:
        shutil.copy2(file, destination / file.name)
        root = ET.parse(file).getroot()
        for key in totals:
            totals[key] += int(root.attrib.get(key, 0))
        for case in root.findall('testcase'):
            for failure in case.findall('failure') + case.findall('error'):
                failures.append({'test': case.attrib.get('name'), 'message': failure.attrib.get('message')})
    return {**totals, 'failed_cases': failures}


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument('case', choices=['bedtools-union', 'htsjdk-cigar', 'htsjdk-reference'])
    parser.add_argument('--work', type=Path, default=Path('upstream-work'))
    parser.add_argument('--output', type=Path, default=Path('upstream-evidence'))
    args = parser.parse_args()
    config = json.loads((HERE / 'changes.json').read_text())[args.case]
    work, output = args.work.resolve(), args.output.resolve()
    work.mkdir(parents=True, exist_ok=True)
    output.mkdir(parents=True, exist_ok=True)
    repo = work / args.case
    if repo.exists():
        raise RuntimeError(f'Refusing to overwrite an existing checkout: {repo}')
    evidence = {'case': args.case, 'upstream': config['repo'], 'upstream_sha': config['sha'],
                'title': config['title'], 'verified': False,
                'workflow_run': os.environ.get('GITHUB_RUN_ID'),
                'workflow_sha': os.environ.get('GITHUB_SHA')}
    allowed = [config['source'], config['test_file']]
    try:
        # Full history supplies the release tag required by HTSJDK's version plugin.
        if run(['git', 'clone', 'https://github.com/' + config['repo'] + '.git', str(repo)], work, output / 'clone.log'):
            raise RuntimeError('Upstream clone failed')
        subprocess.run(['git', 'checkout', '--detach', config['sha']], cwd=repo, check=True)
        test = repo / config['test_file']
        if test.exists():
            raise RuntimeError('Regression test path already exists upstream')
        test.parent.mkdir(parents=True, exist_ok=True)
        test.write_text((HERE / config['fixture']).read_text())
        if args.case == 'bedtools-union':
            integration = 'test/intersect/test-intersect.sh'
            replace_once(repo / integration,
                         '    new_test-intersect.sh \\\n',
                         '    new_test-intersect.sh \\\n    test-split-union.sh \\\n')
            allowed.append(integration)
            if run(['make', '-j2'], repo, output / 'build-before.log'):
                raise RuntimeError('Unmodified production code did not build')
            code = run(['bash', 'test-split-union.sh'], repo / 'test/intersect', output / 'regression-before.log')
            log = (output / 'regression-before.log').read_text()
            import re
            match = re.search(r'split-union: (\d+) checks, (\d+) failures', log)
            if code != 1 or 'FAIL: nested (normal)' not in log or not match:
                raise RuntimeError('Original union defect not reproduced as expected')
            evidence['before'] = {'tests': int(match[1]), 'failures': int(match[2])}
            replace_once(repo / config['source'], config['old'], config['new'])
            if run(['make', '-j2'], repo, output / 'build-after.log'):
                raise RuntimeError('Patched production code did not build')
            code = run(['bash', 'test-split-union.sh'], repo / 'test/intersect', output / 'regression-after.log')
            if code:
                raise RuntimeError('Patched union regression failed')
            evidence['after'] = {'tests': int(match[1]), 'failures': 0}
            code = run(['make', 'test'], repo, output / 'suite-after.log')
            evidence['suite_exit'] = code
            if code:
                raise RuntimeError('Full BEDTools suite failed; inspect suite-after.log')
        else:
            test_class = Path(config['test_file']).stem
            command = ['./gradlew', '--no-daemon', 'test', '--tests', 'htsjdk.samtools.' + test_class]
            code = run(command, repo, output / 'regression-before.log')
            before = xml_summary(repo, output / 'xml-before')
            evidence['before'] = before
            expected_tests = 32 if args.case == 'htsjdk-cigar' else 13
            if code == 0 or before['tests'] != expected_tests or before['failures'] == 0 or before['errors'] or before['skipped']:
                raise RuntimeError('Expected original-code assertion failures were not reproduced')
            replace_once(repo / config['source'], config['old'], config['new'])
            if args.case == 'htsjdk-reference':
                replace_once(repo / config['source'],
                    "     * or the given record's start position is greater than its mate's start position, zero is automatically returned.",
                    "     * the mates map to different reference sequences, or the given record's start position is greater than its mate's\n     * start position, zero is automatically returned.")
            shutil.rmtree(repo / 'build/test-results/test')
            code = run(['./gradlew', '--no-daemon', 'test', '--tests', 'htsjdk.samtools.SAMUtils*Test'], repo, output / 'suite-after.log')
            evidence['after'] = xml_summary(repo, output / 'xml-after')
            evidence['suite_exit'] = code
            if code or evidence['after']['failures'] or evidence['after']['errors'] or evidence['after']['skipped']:
                raise RuntimeError('Patched SAMUtils tests failed or skipped')
            if run(['./gradlew', '--no-daemon', 'spotlessCheck'], repo, output / 'format.log'):
                raise RuntimeError('Upstream formatting check failed')
        subprocess.run(['git', 'add', '--', *allowed], cwd=repo, check=True)
        subprocess.run(['git', 'diff', '--cached', '--check'], cwd=repo, check=True)
        unexpected = subprocess.check_output(['git', 'diff', '--name-only'], cwd=repo, text=True).strip()
        if unexpected:
            raise RuntimeError('Build modified additional tracked files: ' + unexpected)
        evidence['verified'] = True
    finally:
        if (repo / '.git').exists():
            subprocess.run(['git', 'add', '--', *[p for p in allowed if (repo / p).exists()]], cwd=repo, check=False)
            patch = subprocess.check_output(['git', 'diff', '--cached', '--binary'], cwd=repo)
            (output / (args.case + '.patch')).write_bytes(patch)
        (output / 'evidence.json').write_text(json.dumps(evidence, indent=2) + '\n')
        print(json.dumps(evidence, indent=2), flush=True)


if __name__ == '__main__':
    main()
