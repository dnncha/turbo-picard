# Upstream correctness contributions — 6 September 2026

## Status

Three independent fixes have been built and tested against pinned upstream code. **No upstream pull requests have been opened yet.** The connected GitHub interface can create branches and PRs, but cannot create forks; the required `dnncha/bedtools2` and `dnncha/htsjdk` forks were not available.

This directory is staging/evidence on the isolated `research/upstream-correctness-20260906` branch. It is not a Turbo Picard implementation change, and must not be merged into main as an application feature.

## Actual upstream execution

[Completed native validation run 34027560803](https://github.com/dnncha/turbo-picard/actions/runs/34027560803), harness commit `fb539c88748d0218023637ce8058f1905c62d978`.

| Proposed change | Unmodified production code with new tests | After fix | Additional validation |
| --- | --- | --- | --- |
| BEDTools: retain active union start under `-split` | 18 failures / 39 checks | 39 / 39 passing | Full BEDTools `make test` passes, including its intentional negative-control convention |
| HTSJDK: handle partial `=` and `X` like `M` | 21 failures / 32 tests | 63 / 63 focused SAMUtils tests passing | `spotlessCheck` passes |
| HTSJDK: reject cross-reference mate overlap | 5 failures / 13 tests | 44 / 44 focused SAMUtils tests passing | `spotlessCheck` passes |

The HTSJDK after-fix totals include 31 existing tests. Those 31 are shared between the two separate runs; the totals must not be added as if all tests were unique. The entire HTSJDK repository suite was not run. There were no skipped tests in the recorded focused results.

The original CIGAR defect was observed through the public clipping API as well as its count helper: `150=` became `150S` rather than the expected `50=100S` for the 1001/1051 example.

Pinned source commits:

- BEDTools: `614e9a5c5935ab86e873dab9072fbbaf003c1b7e`.
- HTSJDK: `78296e9adab558f053241720426102d64ee603ff`.

The three workflow artifacts contain the exact formatted patches, before/after logs, evidence JSON, and Java test XML where applicable. GitHub artifact retention is 30 days. A separate downloadable contribution bundle preserves copies of these artifacts.

## Submit with the local authenticated GitHub CLI

Requirements: Python 3.10+, `git`, and `gh` authenticated to github.com as `dnncha`. No local C++/Java compilation or AI agent is required for submission.

From this directory, preview without remote writes:

```bash
python3 submit_prs.py
```

Create the two forks, three independent branches, and three upstream pull requests:

```bash
python3 submit_prs.py --submit
```

From the downloaded contribution bundle, use its preserved evidence instead of downloading the artifacts again:

```bash
python3 submit_prs.py --evidence-dir evidence --submit
```

The script validates the successful workflow, artifact provenance, permitted paths and patch application before its first remote write. It verifies fork ancestry, refuses to overwrite different work, reuses existing matching PRs, and records actual PR URLs in `submission-results.json`. It does not merge or release anything. No tokens are accepted, printed, or stored by this script; authentication is delegated to `gh`.

The submission script passed nine offline safety/transport tests with a mocked GitHub API. These are not a claim that live fork creation or PR submission has been executed.

## Scope and attribution

The BEDTools union fix is independent of [existing PR #1144](https://github.com/arq5x/bedtools2/pull/1144), which addresses reciprocal/per-record filtering. That PR is not duplicated here. The cross-reference fix is related to [Picard issue #2039](https://github.com/broadinstitute/picard/issues/2039); released Picard builds will not acquire the library fix automatically.

Do not convert the prior audit's already-fixed BCFtools bug, documented Scanpy effect-size estimand, known pseudoreplication concern, or minimap2 multipart-index limitation into duplicate or misleading bug-fix PRs.

Implementation and tests were prepared with AI assistance. Native execution, pinned sources and narrow diffs are the evidence; no claim of novelty or prevalence in real datasets is made.
