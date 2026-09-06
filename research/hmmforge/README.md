# HMMForge

**Protein-domain annotation, with the execution plan under test.**

HMMForge is an experimental engine for searching batches of proteins against an
HMM database. It retains HMMER's numerical kernels through PyHMMER and tests a
model-major execution plan against an optimised, in-memory `hmmscan` reference.
It is not a new scoring algorithm, a full InterProScan replacement, or a claim
of a production speedup. The first milestone is measurable savings **with the
same relevant statistical and biological outputs**.

## Why this experiment

Repeated model configuration can be expensive. PyHMMER already documents the
benefit of reversing some scan workloads into model-major searches. That idea
belongs to upstream; HMMForge does not claim to have invented it.

The part that must not be hand-waved is statistics. Full-sequence and independent
domain E-values depend on the size of the model database. Conditional domain
E-values depend on the number of reported models **for each protein**. A naive
transposition can preserve scores while changing which domains are reported.
This implementation restores that per-protein search space, preserves upstream
duplicate-alignment suppression, and exposes a differential verification command.

## Install from this source checkout

Python 3.11 or later on Linux/macOS:

```sh
python3 -m venv .venv
. .venv/bin/activate
python -m pip install -e '.[test]'
python -m pytest -q
hmmforge capabilities
```

The package is **not published on PyPI**. Do not use an unqualified `pip install
hmmforge` assuming it refers to this project. PyHMMER is pinned to 0.12.3 because
its interface and scientific behavior are part of the validation contract.

## Verify before substituting

Use the exact HMM database, proteins, thresholds and thread count of interest:

```sh
hmmforge verify families.hmm proteins.faa --cpus 8 > parity.json
hmmforge annotate families.hmm proteins.faa --cpus 8 \
  --output annotations.jsonl > run.json
```

Exit codes are 0 for success/parity, 2 for invalid input/execution failure, and 3
for a parity mismatch. Verification compares reported hits, reported domains,
scores, bias, coordinates, E-values and inclusion flags. This is floating-point
tolerance parity, not a promise of bit-for-bit identical output or complete
HMMER CLI compatibility. `verify` uses the same upstream backend on both paths;
`scripts/native_check.py` adds a comparison against an independently installed
`hmmscan`, at native table precision. Representative large-catalogue validation
remains necessary before release.

For model-specific thresholds, add `--cutoffs gathering`, `trusted`, or `noise`.
Every model must contain the selected cutoff. E-value options apply when no
model-specific cutoff is selected. Inclusion thresholds must not be looser than
reporting thresholds. Seed 0 is deliberately rejected.

Output contains one JSON object per protein, including proteins with no hits.
Repeated FASTA names remain distinct through stable input ordinals. Coordinates
are 1-based, inclusive. Names and descriptions are retained; sequences are not
copied into the output. All processing stays local, with no telemetry or network
requests. Inputs may still be sensitive: do not upload private datasets or
reports to public issue trackers.

An existing output is never overwritten. Results are published atomically only
after a successful complete search. Publication requires a local filesystem
supporting hard links. Input mutation detected during execution aborts publication.

## Measure a workload honestly

```sh
hmmforge benchmark families.hmm proteins.faa --cpus 8 \
  --repeats 5 --dataset-kind biological > benchmark.json
```

Each engine runs in a fresh process with the same input and CPU allocation.
The measured interval includes startup, hashing, model preparation, parsing,
searching and output. Run order alternates. The report retains all measurements,
CPU time, process peak RSS, input hashes, options, versions and parity results.
A failed parity check suppresses the speedup value and exits 3.

These are process-cold measurements; OS file caches are not flushed. Both engines
use bounded sequence batches; the reference preloads optimised profiles. This
is not a comparison against every possible HMMER deployment or a claim to beat
an already-optimised model-major PyHMMER program. No results are cached or reused.

Synthetic engineering exercise:

```sh
python scripts/make_fixture.py /tmp/hmmforge-fixture
hmmforge benchmark /tmp/hmmforge-fixture/models.hmm \
  /tmp/hmmforge-fixture/proteins.fa --cpus 2 --repeats 3 \
  --dataset-kind synthetic > synthetic-benchmark.json
```

Synthetic evidence is not biological sensitivity validation.

## Scope and memory

The current API supports amino-acid HMMER3 models and protein FASTA/FASTA.gz;
JSONL is the only annotation format. Stops, gaps, digits, empty records and
unsupported symbols are rejected rather than silently repaired. Lowercase is
normalised. Duplicate model names are rejected.

`--batch-residues` (default one million) and `--batch-count` (4096) bound the
sequence batch; an oversized individual protein occupies its own batch.
`--max-length` (100,000) caps each sequence. **These are not hard RSS limits.**
All HMMs remain resident; DP workspaces and compact candidate domain summaries also consume
memory. High-hit-density or long-protein workloads can still require substantial
RAM. Both paths prepare their profiles once and reuse them across batches. The
model-major path extracts compact scalar summaries and releases native hit-list
alignment buffers after each model instead of retaining them for the whole batch.

Not implemented: GPU kernels, `domtblout`, full `hmmscan` CLI compatibility,
InterPro member-database orchestration, Pfam clan-overlap resolution, distributed
execution, persistent result caching or clinical validation.

## Development

Version 0.1.0a2 passes 42 tests, plus independent HMMER 3.4 table comparisons
on both a synthetic workload and a small biological fixture. On the tested
GitHub runner, median elapsed-time improvements were 1.50x and 1.47x,
respectively. These are small-fixture results, not production savings.
See [measured results and limitations](docs/RESULTS.md) and the raw reports in
`evidence/`. Results from different machines or versions are recorded separately.

See [the engineering plan](docs/ENGINEERING.md) and [handoff](docs/HANDOFF.md).
Do not claim a speedup that has not survived a representative-data parity gate.
Do not loosen filters or thresholds to improve the benchmark.

HMMForge source is MIT-licensed. PyHMMER, HMMER and Easel retain their own
licenses and should be cited in work using their algorithms. No third-party
protein database is bundled.

## Primary references

- [PyHMMER execution/performance recipe](https://pyhmmer.readthedocs.io/en/stable/examples/performance_tips.html)
- [HMMER pipeline/search-space definitions](https://pyhmmer.readthedocs.io/en/stable/api/plan7/pli.html)
- [PyHMMER result semantics](https://pyhmmer.readthedocs.io/en/stable/api/plan7/results.html)
- [Upstream thresholding and duplicate-alignment suppression](https://github.com/EddyRivasLab/hmmer/blob/master/src/p7_tophits.c)
