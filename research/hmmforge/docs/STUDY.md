# Phase two: compare with a direct model-major baseline

Version 0.1.0a3 adds a workload study, not a replacement scoring kernel. A result
against hmmscan alone cannot establish a competitive advantage: PyHMMER already
documents model-major searches and prefetching. The new baseline exercises that
approach, with independent result extraction and all proteins resident in memory.
It is authored in this project, not externally reviewed or advertised as an
independent expert endorsement. All three paths still use HMMER's kernels.

## Run the comparison

```sh
hmmforge-study run families.hmm proteins.faa --cpus 8 --repeats 6 \
  --dataset-kind biological --output-dir study
```

Add `--cutoffs gathering` for a catalogue with model-specific GA cutoffs. Each
child uses identical thresholds, seed and CPU allocation. The direct baseline
is allowed to hold the complete input in RAM rather than inheriting HMMForge's
batch limits. The distinction is explicit in every worker report. The existing
`hmmforge benchmark` command remains a two-engine comparison for compatibility;
use `hmmforge-study` for the stronger gate.

The study rotates run order in groups of three, starts a fresh process for each
run, and includes startup, hashing, preparation, input processing and output.
File caches are uncontrolled. Output files, worker logs, commands and input/source
hashes are retained. Compare every output to the first optimized scan, including
across repeats. A mismatch, failed worker or timeout suppresses all ratios.
Partial studies remain incomplete, even if one engine appears much faster.

Reported phases are input hashing, model loading/preparation, parsing, combined
search/digitization/extraction, serialization, output hashing and other work.
They are wall-time phases, not individually measured HMMER kernels. All worker
CPU consumption and process peak RSS are also recorded. Parent-observed elapsed
time includes process teardown and timeout-wait observation overhead; do not
interpret small differences in sub-second fixtures as robust performance wins.

Do not publish private inputs, identifiers, or their result files to public CI.
`--dataset-kind biological` is a label supplied by the operator, not proof that
the data are novel, representative, licensed for redistribution, or production
scale. Synthetic datasets remain engineering stress tests.

## Fixed-catalogue gate

```sh
python scripts/prepare_catalogue.py /data/pfam38 --release 38.0
# On subsequent acquisitions, pass --expected-sha256 using the first lock.
hmmforge-study run /data/pfam38/models.hmm /data/proteins.fa --cpus 8 \
  --repeats 6 --cutoffs gathering --dataset-kind biological --output-dir pfam-study
python scripts/native_check.py /data/pfam38/models.hmm /data/proteins.fa \
  --cpus 8 --cutoffs gathering > native-parity.json
```

Acquisition is an explicit network operation outside annotation. It never falls
back to a newer/older release and caps download/decompression sizes. The lock
records model count, compressed and uncompressed hashes, sizes and exact URL.
A first observed HTTPS hash is not a publisher-signed checksum. Verify it against
an independently trusted value when required. No model subset is substituted
when a full-catalogue download fails.

The initial hosted gate uses the full downloaded Pfam 38.0 catalogue against
512 hash-selected proteins from the existing small biological fixture. Selection
is independent of annotation results and preserves original record order.
This tests the full MODEL library, not the intended 100,000-protein metagenomic
corpus. Those are different milestones and must stay separately labelled.

## Native profiling

```sh
python scripts/native_profile.py native-profile -- \
  python -m hmmforge.study _worker families.hmm proteins.fa --cpus 8 \
  --engine model-major --output profile-output.jsonl
```

The script attempts user-space `perf` sampling and writes `status.json`, raw
sampling data and a symbol report when available. It does not install packages
or raise privileges. The disposable hosted CI runner explicitly configures its
own perf permissions in the workflow. Missing tools, denied permissions, empty
profiles and failed commands must not be described as a completed native profile.
Instrumented timings are never mixed into the uninstrumented speedup results.

## Acceptance criteria after this gate

Run a release-locked catalogue against at least 100,000 representative novel
proteins spanning input lengths, hit density, fragments and multidomain cases.
Include model-specific and E-value modes, near-threshold regression cases,
multiple thread budgets and batch sizes. Audit all discrepancies rather than
loosening tolerances. Compare native scan, optimized scan and direct model-major
implementations, with whole-job CPU, elapsed time and peak memory.

Do not expand the wrapper merely because scan is slower. Use the measured native
profile to choose the next kernel, data-layout or I/O change. A small advantage
against direct model-major code is not the several-fold cost reduction needed
for the proposed adoption strategy.

## References

- PyHMMER performance recipes: https://pyhmmer.readthedocs.io/en/stable/examples/performance_tips.html
- PyHMMER parallel strategy: https://pyhmmer.readthedocs.io/en/stable/guide/performance.html
- Pfam 38.0 release context: https://www.ebi.ac.uk/interpro/release_notes/107.0/
