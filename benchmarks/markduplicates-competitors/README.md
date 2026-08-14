# MarkDuplicates competitor evidence

`tools/bench_markduplicates_competitors.py` creates a reproducible evidence
bundle for Turbo-Picard, Picard, samtools, Sambamba and FastDup. It is an evidence
generator, not a claim generator: unavailable programs, failed commands and
parity failures remain visible in `report.json` and `report.md`.

## Input contract

Use one immutable, coordinate-sorted BAM or reference-backed CRAM which is
valid input for **every** selected program. For CRAM, pass the exact reference
FASTA with `--reference-fasta`; its path and SHA-256 are retained in the report.
In particular, `samtools markdup` normally expects mate-score
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
  --require-tools turbo-picard,picard,samtools,sambamba,fastdup \
  --source-url 'https://example.org/immutable/HG002.bam' \
  --source-revision 'accession-or-release'
```

The default runner setting is `READ_NAME_REGEX=null`, which selects the
bounded no-optical Turbo-Picard plan for a single BAM, reference-backed CRAM
when the reference is explicit, or already globally coordinate-ordered multiple
alignment inputs. The bounded plan also carries Picard's `BARCODE_TAG` and
`READ_ONE_BARCODE_TAG`/`READ_TWO_BARCODE_TAG` grouping fields through its
external sort key, paired duplicate-set `DS`/`DI` tags, plus optical-family
decisions and sequencing-duplicate removal when the default or an exact
three-capture-group Picard regex is used.
For an optical-family comparison, pass the exact Picard regex, or pass
`--read-name-regex default` to omit the option and use each tool's default:

```bash
python3 tools/bench_markduplicates_competitors.py \
  --input /data/optical-heavy.bam \
  --output-dir benchmarks/runs/optical-heavy-markduplicates \
  --read-name-regex default \
  --tools turbo-picard,picard \
  --require-tools turbo-picard,picard \
  --repeats 5
```

The selected value is recorded in the report protocol. Regex quantifiers such
as `\d{4}` are passed as literal command arguments; quote the value in the
shell when it contains characters with shell meaning.

For a paired duplicate-set-tag workflow, add
`--tag-duplicate-set-members`. This passes `TAG_DUPLICATE_SET_MEMBERS=true` to
the Picard-compatible presets and records the choice in the evidence protocol.

For a barcode/UMI panel, label the evidence and pass the exact SAM tags used by
the workflow. The bounded plan supports the primary barcode tag and the two
mate-specific fields:

```bash
python3 tools/bench_markduplicates_competitors.py \
  --input /data/umi-panel.coordinate.bam \
  --output-dir benchmarks/runs/umi-panel-markduplicates \
  --tools turbo-picard,picard \
  --require-tools turbo-picard,picard \
  --profile umi_panel \
  --barcode-tag RX \
  --repeats 5
```

The runner records the profile and tag arguments in `report.json`. It does not
claim support for advanced UMI normalization modes that are outside these
explicit Picard barcode fields.

In a shell argument, use the regex spelling required by the tool itself (for
example, a single backslash before `d` for a digit class).

Installed presets are attempted in this order:

- `turbo-picard` (or the local `target/release/picard`);
- `picard`;
- `samtools markdup`;
- `sambamba markdup`;
- `fastdup`.

Missing executables are reported as `unavailable`; they are never silently
removed from the result.

`--require-tools` makes the process return non-zero unless each named program
completes every measured repeat and has exact `PASS` (or reference) parity.
Omit it during environment discovery; use it for a release-candidate run so an
absent or incompatible competitor cannot accidentally yield a green job.
The output directory must be new or empty; the runner refuses to mix evidence
from different invocations.

For a CRAM run, the reference-aware shape is:

```bash
python3 tools/bench_markduplicates_competitors.py \
  --input /data/HG002.fixmate.coordinate.cram \
  --reference-fasta /refs/GRCh38.fa \
  --output-dir benchmarks/runs/HG002-cram-markduplicates \
  --tools turbo-picard,picard \
  --require-tools turbo-picard,picard \
  --repeats 5
```

The preset exports `TURBO_PICARD_THREADS={threads}` to constrain Turbo-Picard's
global HTS worker budget. FastDup, samtools and Sambamba receive their documented
thread arguments. Picard MarkDuplicates is principally single-threaded; the evidence
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

SAMBLASTER is not a preset in this coordinate-sorted alignment comparison. Its
intended contract is a read-id-grouped SAM stream immediately after alignment,
so a fair test must measure the complete aligner-to-sorted-BAM pipeline rather
than insert an unreported input conversion into this runner.

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
tags. For BAM output it uses `pysam` when available and otherwise streams
`samtools view -h`; if neither reader is available, required parity fails rather
than silently dropping the comparison. When both tools emit Picard
`DuplicationMetrics`, it also compares the normalized metrics table. A
competitor with different winning-read stability therefore fails exact parity
even if it identifies the same duplicate families. That distinction must remain
visible in any publication.

## Claim gate

Do not promote a bundle into a public performance claim unless all of these are
true:

1. The dataset source, revision and SHA-256 are independently retrievable.
2. Every claimed tool completed all measured repeats.
3. Exact commands, versions, threads and resource backends are disclosed.
4. Parity is `PASS`, or every semantic difference is prominently scoped.
5. The benchmark covers at least 30x WGS plus duplicate-heavy WES, UMI and
   optical-heavy inputs, using explicit `READ_NAME_REGEX` settings where
   relevant; a single dataset is not a universal result.
6. CPU, memory, temporary disk and output validity are considered with wall
   time; wall time alone is insufficient.
7. A second machine independently reproduces the result.

The runner does not calculate or print broad performance superlatives or a
speedup headline. Conclusions belong in reviewed evidence after the claim gate
passes.

The checked-in guardrails are validated by:

    python3 tools/verify_markduplicates_guardrails.py

## Checked-in synthetic guardrail

[`synthetic-1m-external-guardrail.json`](synthetic-1m-external-guardrail.json)
records one local five-repeat run of the bounded external-plan scope on a
1,000,000-read, 4,096-record-family synthetic BAM. Exact parity passed. The
bounded external sort window was 500,000 records or 256 MiB per sorter. Turbo
Turbo Picard used 237,420,544 bytes of peak RSS, compared with 1,160,527,872
bytes for Picard; median wall time was 0.531262 seconds versus 1.860894 seconds.
The input is not publicly retrievable and the result is evidence only: it is a
memory/scalability guardrail, not a WGS or production-readiness claim.

## Checked-in reference-backed CRAM guardrail

[`real-na12878-cram-external-guardrail.json`](real-na12878-cram-external-guardrail.json)
records a three-repeat comparison on the pinned GATK-derived mitochondrial CRAM
with `READ_NAME_REGEX=null` and the explicit `fixtures/reference/chrM.fa`
reference. The required tool gate passed and exact parity passed. Turbo Picard's
median wall time was 0.232844 seconds versus 0.826270 seconds for Picard, with peak
RSS of 39,452,672 versus 859,127,808 bytes. This is evidence for the guarded
CRAM path only; it is not a 30x WGS or independent-reproduction claim.
