# MarkDuplicates competitor evidence

`tools/bench_markduplicates_competitors.py` creates a reproducible evidence
bundle for Turbo-Picard, Picard, samtools and FastDup. It is an evidence
generator, not a claim generator: unavailable programs, failed commands and
parity failures remain visible in `report.json` and `report.md`.

## Input contract

Use one immutable, coordinate-sorted BAM which is valid input for **every**
selected program. In particular, `samtools markdup` normally expects mate-score
and mate-CIGAR tags produced by `samtools fixmate -m`. The runner deliberately
does not repair or transform input, because timing different preparation paths
would make the comparison misleading.

Record the public accession or immutable URL alongside the bundle:

```bash
cargo build --release -p turbo-picard-cli --bin picard

python3 tools/bench_markduplicates_competitors.py \
  --input /data/HG002.fixmate.coordinate.bam \
  --output-dir benchmarks/runs/HG002-markduplicates-8t \
  --threads 8 \
  --warmups 1 \
  --repeats 5 \
  --require-tools turbo-picard,picard,samtools,fastdup \
  --source-url 'https://example.org/immutable/HG002.bam' \
  --source-revision 'accession-or-release'
```

Installed presets are attempted in this order:

- `turbo-picard` (or the local `target/release/picard`);
- `picard`;
- `samtools markdup`;
- `fastdup`.

Missing executables are reported as `unavailable`; they are never silently
removed from the result.

`--require-tools` makes the process return non-zero unless each named program
completes every measured repeat and has exact `PASS` (or reference) parity.
Omit it during environment discovery; use it for a release-candidate run so an
absent or incompatible competitor cannot accidentally yield a green job.
The output directory must be new or empty; the runner refuses to mix evidence
from different invocations.

The preset exports `TURBO_PICARD_THREADS={threads}` to constrain Turbo-Picard's
global HTS worker budget. FastDup and samtools receive their documented thread
arguments. Picard MarkDuplicates is principally single-threaded; the evidence
bundle preserves this distinction rather than implying that every program can
consume the requested thread budget identically.

A custom build or container launcher can be supplied without a shell:

```bash
python3 tools/bench_markduplicates_competitors.py \
  --input /data/input.bam \
  --output-dir benchmarks/runs/custom \
  --tools picard \
  --tool 'turbo-custom=/opt/turbo/picard MarkDuplicates I={input} O={output} M={metrics} TMP_DIR={tmp}' \
  --reference-tool picard
```

Supported placeholders are `{input}`, `{output}`, `{metrics}`, `{tmp}` and
`{threads}`. Templates are parsed into argument arrays and never evaluated by a
shell. Keep all tool-specific temporary output under `{tmp}` for comparable
temporary-disk measurements.

## Recorded evidence

Each measured and warm-up run retains:

- the exact argument vector and exit status;
- stdout and stderr;
- elapsed, user-CPU and system-CPU seconds;
- maximum resident set size;
- sampled peak bytes under the run's dedicated temporary directory;
- final output bytes;
- executable path, SHA-256 and version output;
- the input path, bytes, SHA-256 and optional immutable source citation;
- threads, repeats, warm-ups, host, architecture and CPU model;
- a streaming parity result against the selected reference.

GNU `time` is used where available. Minimal containers fall back to POSIX
`wait4`; the backend is stored on every run. For release evidence, install GNU
`time`, use a dedicated idle host, characterize the storage device/filesystem,
and retain the raw bundle.

The parity comparator checks records in order and includes read identity,
alignment location/CIGAR, duplicate flag, `DT`, `DS`, `DI`, and `RX`/`BX`/`BY`
tags. When both tools emit Picard `DuplicationMetrics`, it also compares the
normalized metrics table. A competitor with different winning-read stability
therefore fails exact parity even if it identifies the same duplicate families.
That distinction must remain visible in any publication.

## Claim gate

Do not promote a bundle into a public performance claim unless all of these are
true:

1. The dataset source, revision and SHA-256 are independently retrievable.
2. Every claimed tool completed all measured repeats.
3. Exact commands, versions, threads and resource backends are disclosed.
4. Parity is `PASS`, or every semantic difference is prominently scoped.
5. The benchmark covers at least 30x WGS plus duplicate-heavy WES, UMI and
   optical-heavy inputs; a single dataset is not a universal result.
6. CPU, memory, temporary disk and output validity are considered with wall
   time; wall time alone is insufficient.
7. A second machine independently reproduces the result.

The runner does not calculate or print “SOTA”, “industry standard”, or a
speedup headline. Those conclusions belong in reviewed evidence after the
claim gate passes.
