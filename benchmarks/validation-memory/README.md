# Validation helper memory: one million synthetic SAM records

This measures the repository's Python coordinate-sorted SAM comparator, **not**
Turbo Picard's native command runtime, WGS processing, or competitor performance.
It is a maintainer-run synthetic experiment, not an independent adoption result.

## Saved result

| Measurement | Historical helper | Candidate helper |
| --- | ---: | ---: |
| Median peak process RSS | 452.55 MiB | 99.93 MiB |
| Median digest wall time | 3.7041 s | 3.5181 s |
| Repetitions | 3 | 3 |

All six runs produced the same SHA-256 digest. Median peak RSS fell by
77.92% on this fixture. RSS includes Python and imported modules.
The input has 1,000,000 records / 141,888,973 bytes.

Raw measurements, input and source hashes, interpreter/platform details, and
measurement definitions are in `synthetic-1m-coordinate.json`. The benchmark
alternates implementation order in fresh processes. Filesystem caches are not
cleared. Three repetitions on one host do not establish universal throughput.
The key finding is memory scaling without dropping or sampling records.

## Reproduce

From a source checkout containing this candidate:

```sh
baseline="$(mktemp)"
git show caa4575178346c394815faff98d06618acebf688:tools/compare_real_data.py > "$baseline"
python3 tools/bench_validation_memory.py \
  --baseline-file "$baseline" \
  --records 1000000 --repeats 3 \
  --output /tmp/turbo-validation-memory-new.json
rm "$baseline"
```

The output path must not exist. The harness generates the synthetic SAM locally,
executes only the two helper implementations provided by the caller, and removes
its private scratch directory. It refuses to emit a performance comparison when
digests differ or measurements are invalid. Source hashes in the saved report
identify the measured implementation; a future edited helper must be remeasured.

Native external-sort changes are a separate, uncompiled candidate in this
implementation pass. No result here validates those changes or gives them credit.
