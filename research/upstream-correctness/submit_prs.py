#!/usr/bin/env python3
"""Submit the three independently tested patches using the user's local gh login.

Defaults to a read-only preview. --submit creates/reuses forks, branches and PRs.
No API token is accepted, printed, or stored by this script.
"""
from __future__ import annotations
import argparse
import base64
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import time
import urllib.parse

EVIDENCE_REPO = 'dnncha/turbo-picard'
DEFAULT_RUN = '34027560803'
ACCOUNT = 'dnncha'
CASES = {
    'bedtools-union': ('arq5x/bedtools2', 'fix/split-union-start',
        'intersect: retain the start of a merged overlap under -split',
        ['src/utils/FileRecordTools/Records/BlockMgr.cpp', 'test/intersect/test-intersect.sh', 'test/intersect/test-split-union.sh']),
    'htsjdk-cigar': ('samtools/htsjdk', 'fix/overlap-extended-cigar',
        'SAMUtils: clip partial = and X elements like M',
        ['src/main/java/htsjdk/samtools/SAMUtils.java', 'src/test/java/htsjdk/samtools/SAMUtilsExtendedCigarOverlapTest.java']),
    'htsjdk-reference': ('samtools/htsjdk', 'fix/overlap-cross-reference',
        'SAMUtils: do not overlap-clip mates on different references',
        ['src/main/java/htsjdk/samtools/SAMUtils.java', 'src/test/java/htsjdk/samtools/SAMUtilsReferenceOverlapTest.java']),
}


def gh(*args: str, input_data: dict | None = None, allow_404: bool = False):
    command = ['gh', *args]
    if input_data is not None:
        command += ['--input', '-']
    result = subprocess.run(command, input=json.dumps(input_data) if input_data is not None else None,
                            text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if result.returncode:
        if allow_404 and '(HTTP 404)' in result.stderr:
            return None
        raise RuntimeError(f"{' '.join(command)} failed:\n{result.stderr.strip()}")
    return json.loads(result.stdout) if result.stdout.strip() else None


def api(endpoint: str, payload: dict | None = None, allow_404: bool = False):
    args = ['api', '--hostname', 'github.com', endpoint]
    if payload is not None:
        args += ['--method', 'POST']
    return gh(*args, input_data=payload, allow_404=allow_404)


def pr_body(case: str, evidence: dict) -> str:
    summary = {
        'bedtools-union': '''`getNonRedundantOverlap()` moves the active merged interval's start forward when another overlap joins it. This loses already-covered bases. For example, [0,100) plus [40,60) is calculated as 60 bases rather than 100, so adding a contained database interval can remove an otherwise qualifying `intersect -split -f 0.9` result.

Keep the active interval's original start while advancing its end. This deliberately preserves the existing sorted-endpoint algorithm and cumulative `-f` semantics. It does not change reciprocal/per-record filtering and is independent of #1144.

The regression script is included in the existing intersect test runner. It checks containment, partial overlap, overlap chains, duplicates, bookended/disjoint intervals, a real BED12 split query, intron-only hits, coordinate offsets, reversed database order, and the sorted sweep path.''',
        'htsjdk-cigar': '''`getNumOverlappingAlignedBasesToClip()` correctly handles a mate start inside an `M` element, but handles `=` and `X` through the branch that counts the entire element. With a 150-base alignment starting at 1001 and mate starting at 1051, `150M` returns 100 but `150=` and `150X` return 150.

Route all three aligned operators through the existing partial-element calculation. Leave insertion, deletion, skip, clipping and padding handling unchanged.

Tests cover boundary/interior mate starts, mixed `=`/`X` CIGARs, insertions, deletions, skips and clipping. They also assert the resulting CIGAR from the public clipping API, read-length preservation, and non-mutation when `noSideEffects=true`.''',
        'htsjdk-reference': '''The overlap helper compares numeric alignment positions without first checking that the mates are on the same reference sequence. A read on chr1 at 1001 with `150M` and a mate on chr2 at 1051 therefore returns 100 bases to clip instead of zero.

Return zero for different reference names before comparing coordinates. Comparing names also supports headerless records without introducing reference-index resolution or boxed-integer identity assumptions. The public convenience clipping method then leaves cross-reference records unchanged.

Tests include swapped references, equal positions, the first/second-of-pair tie rule, non-overlapping coordinates, reference indices above the Integer cache range, and headerless records. Same-reference clipping remains covered.

Related downstream report: broadinstitute/picard#2039. This is a library fix, not a claim that already-released Picard versions pick it up automatically.''',
    }[case]
    before, after = evidence['before'], evidence['after']
    suite = 'the full BEDTools `make test` suite' if case == 'bedtools-union' else 'the focused upstream `SAMUtils*Test` suite and `spotlessCheck`'
    scope_note = '' if case == 'bedtools-union' else ' HTSJDK validation is limited to the focused SAMUtils tests, not its entire repository test suite.'
    return f'''## Problem and change

{summary}

## Validation

Pinned upstream commit: `{evidence['upstream_sha']}`.

The added tests were first run against unmodified production code: **{before['tests']} tests/checks, {before['failures']} failures**. After the source fix, **{after['tests']} tests/checks passed with no failures**, and {suite} passed.{scope_note}

[Native build logs and test-report artifacts](https://github.com/{EVIDENCE_REPO}/actions/runs/{evidence['workflow_run']}). These are executions of the actual upstream code, not an independent algorithm model.

Implementation and regression tests were prepared with AI assistance.
'''


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--submit', action='store_true', help='Create forks, branches and upstream pull requests')
    parser.add_argument('--run', default=DEFAULT_RUN, help='Evidence workflow run ID')
    parser.add_argument('--evidence-dir', type=Path, help='Existing directory containing downloaded workflow artifacts')
    parser.add_argument('--results', type=Path, default=Path('submission-results.json'))
    args = parser.parse_args()
    if not shutil.which('gh') or not shutil.which('git'):
        raise SystemExit('Install GitHub CLI (gh) and git, then authenticate gh to github.com. No local build tools are needed.')
    user = api('user')
    if user['login'] != ACCOUNT:
        raise SystemExit(f"Expected GitHub account {ACCOUNT}, found {user['login']}; no writes performed.")
    run = api(f'repos/{EVIDENCE_REPO}/actions/runs/{args.run}')
    if run['status'] != 'completed' or run['conclusion'] != 'success':
        raise SystemExit(f"Evidence run is {run['status']}/{run['conclusion']}; refusing to submit unverified patches.")
    results = {}
    with tempfile.TemporaryDirectory(prefix='upstream-prs-') as temporary:
        root = Path(temporary)
        evidence_dir = args.evidence_dir.resolve() if args.evidence_dir else root / 'artifacts'
        if not args.evidence_dir:
            subprocess.run(['gh', 'run', 'download', args.run, '--repo', EVIDENCE_REPO,
                            '--dir', str(evidence_dir)], check=True)
        prepared = []
        # Validate all three artifacts and patch applications before any remote write.
        for case, (upstream, branch, title, paths) in CASES.items():
            directory = evidence_dir / ('upstream-' + case)
            evidence = json.loads((directory / 'evidence.json').read_text())
            if not evidence.get('verified') or evidence['case'] != case or evidence['upstream'] != upstream:
                raise RuntimeError(f'{case}: missing or unverified evidence')
            if str(evidence['workflow_run']) != str(args.run) or evidence['workflow_sha'] != run['head_sha']:
                raise RuntimeError(f'{case}: artifact provenance does not match workflow run')
            patch = (directory / (case + '.patch')).resolve()
            changed = [line[6:] for line in patch.read_text().splitlines() if line.startswith('+++ b/')]
            if sorted(changed) != sorted(paths):
                raise RuntimeError(f'{case}: unexpected changed paths: {changed}')
            checkout = root / case
            checkout.mkdir()
            subprocess.run(['git', 'init', '-q', str(checkout)], check=True)
            for path in paths:
                response = api(f'repos/{upstream}/contents/{path}?ref={evidence["upstream_sha"]}', allow_404=True)
                if response is not None:
                    target = checkout / path
                    target.parent.mkdir(parents=True, exist_ok=True)
                    target.write_bytes(base64.b64decode(response['content']))
            subprocess.run(['git', 'apply', '--check', str(patch)], cwd=checkout, check=True)
            subprocess.run(['git', 'apply', str(patch)], cwd=checkout, check=True)
            tree = [{'path': path, 'mode': '100644', 'type': 'blob', 'content': (checkout / path).read_text()} for path in paths]
            body = pr_body(case, evidence)
            (root / (case + '-pr.md')).write_text(body)
            prepared.append((case, upstream, branch, title, evidence, tree, body))
            print(f'Validated {case}: {len(paths)} changed files; {evidence["after"]["tests"]} passing checks')
            print(body)
        if not args.submit:
            print('Read-only preview complete. Run with --submit to create the three upstream PRs.')
            return
        args.results.parent.mkdir(parents=True, exist_ok=True)
        for case, upstream, branch, title, evidence, entries, body in prepared:
            fork = ACCOUNT + '/' + upstream.split('/')[1]
            info = api('repos/' + fork, allow_404=True)
            if info is None:
                api(f'repos/{upstream}/forks', {'default_branch_only': True})
                for _ in range(30):
                    info = api('repos/' + fork, allow_404=True)
                    if info is not None:
                        break
                    time.sleep(2)
            if info is None or not info.get('fork') or info.get('source', {}).get('full_name') != upstream:
                raise RuntimeError(f'{fork} is not the expected fork; refusing to write')
            base = api(f'repos/{upstream}/git/commits/{evidence["upstream_sha"]}')
            tree = api(f'repos/{fork}/git/trees', {'base_tree': base['tree']['sha'], 'tree': entries})
            ref = api(f'repos/{fork}/git/ref/heads/{branch}', allow_404=True)
            if ref is None:
                commit = api(f'repos/{fork}/git/commits', {'message': title, 'tree': tree['sha'], 'parents': [evidence['upstream_sha']]})
                api(f'repos/{fork}/git/refs', {'ref': 'refs/heads/' + branch, 'sha': commit['sha']})
            else:
                commit = api(f'repos/{fork}/git/commits/{ref["object"]["sha"]}')
                if commit['tree']['sha'] != tree['sha']:
                    raise RuntimeError(f'{fork}:{branch} already contains different work; not overwriting it')
            query = urllib.parse.urlencode({'state': 'all', 'head': ACCOUNT + ':' + branch})
            existing = api(f'repos/{upstream}/pulls?{query}')
            if existing:
                pr = existing[0]
            else:
                pr = api(f'repos/{upstream}/pulls', {'title': title, 'body': body, 'head': ACCOUNT + ':' + branch,
                        'base': 'master', 'draft': False, 'maintainer_can_modify': True})
            results[case] = {'url': pr['html_url'], 'state': pr['state'], 'commit': commit['sha']}
            args.results.write_text(json.dumps(results, indent=2) + '\n')
            print(pr['html_url'], flush=True)


if __name__ == '__main__':
    main()
