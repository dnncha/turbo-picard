# HMMForge

**Protein-domain annotation with a verifiable execution plan.**

HMMForge searches protein sequences against profile-HMM databases using the
unchanged HMMER kernels exposed by PyHMMER. Its model-major executor reuses
prepared profiles, processes bounded sequence batches and reconstructs
per-protein scan statistics. It includes a verifier and a reproducible study
against both optimized scan and direct model-major execution.

This is **research software, version 0.1.0a3**. It is not a new scoring algorithm,
a GPU implementation, a complete InterProScan replacement or a demonstrated
production cost breakthrough. The [phase-two results](docs/PHASE2_RESULTS.md)
include the stronger baseline: the current executor is essentially tied with
direct model-major PyHMMER on the small fixtures. PyHMMER already documents this
execution strategy; HMMForge does not claim to have invented it.

## Install from source

Python 3.11 or later on Linux/macOS:

```sh
python3 -m venv .venv
. .venv/bin/activate
python -m pip install -e '.[test]'
python -m pytest -q
hmmforge capabilities
```

The package is **not published on PyPI**. Do not assume an unqualified
`pip install hmmforge` refers to this project. PyHMMER is pinned to 0.12.3 as
part of the validation contract. CI builds an installable wheel and verifies it.

## Verify the exact workload

```sh
hmmforge verify families.hmm proteins.faa --cpus 8 > parity.json
hmmforge annotate families.hmm proteins.faa --cpus 8 \
  --output annotations.jsonl > run.json
```

Add `--cutoffs gathering`, `trusted` or `noise` when using model-specific
thresholds. Every model must contain the chosen cutoff. Otherwise the configured
reporting and inclusion E-value thresholds apply. Seed 0 is unsupported.

Verification compares reported matches, domains, coordinates, scores, bias,
E-values and inclusion flags. Score/bias tolerances are 1e-6 relative and 1e-5
absolute; P/E-values have no absolute tolerance floor. This is not a guarantee
of bit-identical output across every input or platform. Exit codes are 0 for
success/parity, 2 for input/execution failure and 3 for a parity mismatch.

An independent native check is also available, with `hmmscan` and `hmmpress`
installed separately:

```sh
python scripts/native_check.py families.hmm proteins.faa --cpus 8
```

That script checks reported target/domain tables at native printed precision.
It does not check alignment strings, inclusion flags or the full HMMER CLI.
It is intended for bounded verification fixtures, not enormous input files.

## Measure against both baselines

```sh
hmmforge-study run families.hmm proteins.faa --cpus 8 --repeats 6 \
  --dataset-kind biological --output-dir study
```

The three engines are optimized in-memory `hmmscan`, direct fully resident
model-major PyHMMER with separate extraction, and HMMForge's bounded-batch
executor. The direct baseline was written in this project; it is not an external
expert endorsement. All engines retain the same HMMER numerical kernels.

The study retains all worker outputs, errors, input/source hashes, options,
CPU time, elapsed time, peak memory and phase timings. Each run starts a fresh
process; order is balanced in groups of three. A mismatch, failed worker or
incomplete study suppresses speedup ratios. File caches are uncontrolled and
very short timings are sensitive to process-observation overhead. Phase timers
are not native kernel profiles. Wall-time ratios are not measured cloud savings.

See [the study protocol](docs/STUDY.md) for version-locked catalogue acquisition,
independent checks and native sampling. Normal annotation never downloads data.

## Input and output contract

Amino-acid HMMER3 models and protein FASTA/FASTA.gz are supported. The CLI rejects
stops, gaps, digits, empty records, unsupported symbols and duplicate model names;
it does not silently repair them. Lowercase amino-acid letters are normalized.

JSONL contains one row per input protein, including no-hit proteins. Stable
ordinals keep repeated FASTA names distinct. Names and descriptions are retained;
sequences are not copied into output. Coordinates are 1-based and inclusive.
An existing output is never overwritten; complete results are published atomically
on a local filesystem supporting hard links. Detected input mutation aborts output.
All processing stays local, without telemetry. Do not upload private inputs or
annotation reports to public CI or issue trackers.

`--batch-residues` (one million) and `--batch-count` (4096) bound the sequence batch,
not process RSS. One oversized protein occupies its own batch. `--max-length`
defaults to 100,000 residues and cannot exceed that limit. Models remain resident;
workspaces and candidate summaries also consume memory. The direct study baseline
holds every protein in RAM. High-hit-density workloads can use substantial memory.

Not implemented: GPU kernels, `domtblout`, full `hmmscan` CLI compatibility,
InterPro orchestration, Pfam clan-overlap resolution, distributed execution,
persistent result caching or clinical validation.

## Evidence and development

The a3 package CI passed 65 tests, two three-engine studies, independent native
HMMER 3.4 table comparisons and wheel installation/verification. Read the
[phase-two results](docs/PHASE2_RESULTS.md) for measured scope and remaining gates.
Historical a2 measurements remain in [RESULTS.md](docs/RESULTS.md).

The next target is a version-locked full catalogue against representative novel
proteins, with native profiling and a stronger practical competitor. Do not
loosen thresholds, hide slower runs or advertise synthetic results as production
savings. See [ENGINEERING.md](docs/ENGINEERING.md) and [HANDOFF.md](docs/HANDOFF.md).

HMMForge temporarily lives under `research/hmmforge` on Turbo Picard's isolated
`research/hmmforge-prototype` branch. **Do not merge that branch into main.**
A separate private repository can be created from an authenticated local GitHub
CLI using `bash scripts/publish_standalone.sh ../hmmforge-standalone`. The script
refuses existing repositories/destinations and extracts only this package.

## License and upstream credit

HMMForge source is MIT-licensed. PyHMMER, HMMER and Easel retain their own licenses
and should be cited for their algorithms. No third-party protein database is
bundled with the source package.

- [PyHMMER performance recipes](https://pyhmmer.readthedocs.io/en/stable/examples/performance_tips.html)
- [HMMER pipeline and search spaces](https://pyhmmer.readthedocs.io/en/stable/api/plan7/pli.html)
- [PyHMMER result semantics](https://pyhmmer.readthedocs.io/en/stable/api/plan7/results.html)
- [Upstream thresholding and duplicate-alignment suppression](https://github.com/EddyRivasLab/hmmer/blob/master/src/p7_tophits.c)
