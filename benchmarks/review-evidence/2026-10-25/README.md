# Competitive performance evidence — October 25, 2026

This is a historical snapshot of the exact pre-v0.1.17 source patch identified below.
The shipping branch was subsequently rebased onto the v0.1.17 scientific-correctness
changes. These timings and comparator results have not been relabeled as measurements
of that rebased tree; its fresh CI benchmark and strict parity gates are linked from
[PR #46](https://github.com/dnncha/turbo-picard/pull/46).

Turbo Picard now stores a completed pair in one sort entry, reserves encoded key and payload sizes, and recovers paired fragment candidates only when mapped unpaired candidates exist. The shared HTSlib dependency enables libdeflate. Both original mate ordinals and all selection, barcode and optical metadata are retained. The FastDup pair representation was the initial public implementation lead; its pinned source and exact matched build are in `provenance.json`.

## Before and after: whole MarkDuplicates commands

Original checkout: `a4aeaa83659c581b7e2b7d587ad51faf4d48bc06`. Candidate: the native patch identified in `provenance.json`. One CPU, zero HTS worker threads, compression level 1, warm cache, one warm-up and three alternating measured repetitions. Wall time and peak RSS below are medians. Every candidate and reference repetition passed the bounded-v4 Picard contract: ordered mandatory fields, typed tags except PG, SQ/RG headers, normalized duplication metrics and numeric histograms.

| Synthetic case | Original seconds | Candidate seconds | Time reduction | Original peak RSS MiB | Candidate peak RSS MiB |
| --- | ---: | ---: | ---: | ---: | ---: |
| coordinate-ties-unplaced | 0.1580 | 0.1225 | 22.5% | 39.42 | 27.32 |
| same-name-same-library | 0.1395 | 0.1212 | 13.2% | 39.34 | 27.33 |
| same-name-separate-libraries | 0.1412 | 0.1112 | 21.3% | 39.39 | 27.33 |
| optical-and-barcodes | 0.1342 | 0.1224 | 8.8% | 39.36 | 27.33 |
| remove-optical | 0.1271 | 0.1121 | 11.8% | 39.31 | 27.27 |
| remove-all | 0.1549 | 0.1553 | -0.3% | 39.35 | 27.46 |
| large-duplicate-family | 0.2585 | 0.1987 | 23.1% | 67.21 | 48.02 |
| spill-600k | 1.0367 | 0.8424 | 18.7% | 161.42 | 107.71 |
| spill-1200k | 2.2059 | 1.7486 | 20.7% | 161.52 | 107.57 |
| paired-600k | 2.3832 | 1.0599 | 55.5% | 238.42 | 93.95 |
| paired-spill-1200k | 4.4588 | 2.1051 | 52.8% | 250.48 | 99.09 |

The removal case is effectively neutral (0.3% longer); it is retained rather than counted as a win. The paired cases contain 600,007 and 1,200,007 records. These are synthetic fixtures, not WGS/WES/cohort validation. Intermediate runs are retained as separate raw artifacts and are not measurements of the final build.

## Native competitors from the same coordinate input

One CPU, compression level 5, one warm-up and three alternating measured repetitions. FastDup uses the same statically linked HTSlib and libdeflate archives as the candidate; its own source is unmodified. samtools timers include collation, fixmate and final coordinate sorting. Picard is 3.4.0, FastDup reports 1.0.0 at the pinned source revision, and samtools is 1.21.

| Input | Tool | Median seconds | Median peak RSS MiB | Full Picard contract | Mandatory fields/RG multiset |
| --- | --- | ---: | ---: | --- | --- |
| paired-600k | Turbo Picard | 1.0541 | 94.07 | PASS | match |
| paired-600k | picard | 5.4230 | 1052.56 | PASS | match |
| paired-600k | fastdup | 1.1308 | 383.81 | FAIL | match |
| paired-600k | samtools | 1.9820 | 235.69 | FAIL | match |
| gatk-na12878-mito | Turbo Picard | 0.1408 | 9.53 | PASS | match |
| gatk-na12878-mito | picard | 2.6784 | 721.30 | PASS | match |
| gatk-na12878-mito | fastdup | 0.2014 | 25.71 | FAIL | match |
| gatk-na12878-mito | samtools | failed | — | FAIL | no match |

Turbo Picard had the shortest completed median in these two profiles. This does not establish a general lead. FastDup and samtools outputs differ from the full Picard contract; matching the diagnostic mandatory-field/RG multiset does not qualify a full-contract speedup claim. FastDup retained alignment/tag and metrics differences plus an invalid numeric histogram. samtools failed on the mitochondrial input because markdup required a missing `ms` mate-score tag after the included preparation; its error is retained. For pipelines, GNU time reports maximum child RSS, not simultaneous aggregate pipeline memory.

## Shared BAM I/O profiles

Original and candidate native commands, the same 600,007-record BAM, one CPU, zero HTS workers, compression level 5, one warm-up and three alternating measured repetitions. All outputs matched ordered decoded fields/tags except PG and SQ/RG headers. SortSam uses an already-coordinate-sorted input and measures rewriting rather than an external sort.

| Command | Original seconds | Candidate seconds | Time reduction | Original peak RSS MiB | Candidate peak RSS MiB |
| --- | ---: | ---: | ---: | ---: | ---: |
| CleanSam | 0.9199 | 0.5635 | 38.7% | 5.54 | 5.20 |
| SortSam | 0.8488 | 0.5335 | 37.1% | 5.67 | 5.57 |

Separate checks against actual Picard passed all twelve existing commands on the pinned NA12878 mitochondrial BAM and all six existing commands on its reference-backed CRAM. These small public inputs establish scoped regression coverage. Compressed BAM bytes can differ with the encoder; decoded equality is the tested contract.

## Reproduction and retained artifacts

`artifacts.json` identifies each losslessly gzipped raw JSON report, its original workspace path and both compressed and uncompressed hashes. The reports retain all repetitions, executable/input hashes, exact argv, versions, timings, resource values and comparison failures. Failed outputs and complete run logs remain in the recorded ignored `benchmarks/runs/` directories. `native.patch.gz` applies to the original source revision; `provenance.json` records its hash and all native source hashes. `run.sh` records the bounded, competitor and public-data invocations; `bam-io-reproducer.py` records the I/O experiment. Paths in these snapshots describe the measured workspace and must be adjusted for another host. Use fresh output directories.

Run the canonical bounded harness with the original/candidate binaries, actual Picard 3.4.0, samtools and three repeats. Run the native harness with `--tools fastdup samtools` and the same explicit binaries and inputs. Rebuild the pinned unmodified FastDup source with the recorded argv and matching libraries. This is single-host warm-cache evidence; production WGS/WES/UMI/optical/distant-mate workloads and independent reproduction remain open.

## Local validation

437 Rust tests passed, with one explicitly ignored release microbenchmark. Python discovery ran 516 tests with one optional skip. Formatting, required evidence/link/coverage verifiers, workflow-starter static validation, strict pinned Sphinx build, Linux package installation and runtime-container smoke checks passed. The container smoke used the built candidate in a pinned Debian runtime; the repository Dockerfile source build, macOS and workflow-engine execution were not rerun. Full CI remains required before merge.
