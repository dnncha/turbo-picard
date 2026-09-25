# Performance and scalability audit — 25 September 2026

Baseline: `6c06c6a722c39ff0602ffda135bba3ad86bae149`.
Implementation and review: [PR #33](https://github.com/dnncha/turbo-picard/pull/33).

## What this change establishes

The binary-key and BAM external sorters shared three avoidable costs: heap selection moved record wrappers, intermediate merge passes rewrote singleton runs, and empty run-sized vectors remained allocated during the merge. The binary-key sorter additionally paid for a stable sort even though its comparison already includes a unique insertion ordinal.

The candidate uses a shared tournament of run indices, carries singleton files forward under the existing ownership guard, releases exhausted vectors, and uses ordinal-preserving unstable sorting for binary-key runs. BAM within-run ordering, duplicate selection, optical handling, compression settings and command contracts are unchanged.

The tournament makes at most one record comparison per level after a head replacement. The generated exact-order test covers 12 seeds and 66 run counts, including empty streams, uneven lengths and equal keys. Other regressions cover cross-pass BAM ties, every byte boundary of a truncated binary run, error cleanup, and exact scratch-write accounting.

## Initial measured evidence

Run [36150586576](https://github.com/dnncha/turbo-picard/actions/runs/36150586576), candidate `e7292434bcd21fd1da30d3b70354f8e66ca0156b`, compared with the baseline above. The downloadable `external-sort-core-evidence` artifact contains raw observations, output hashes, source/binary hashes, host/toolchain details and test output. Its initial overall workflow status was failure because the new benchmark harness needed rustfmt changes; compilation, all 13 standard-library tests and all benchmark comparisons passed. Formatting has been corrected separately. Check the latest PR checks before merging.

This is a **synthetic external-sort-core experiment**, not a whole-MarkDuplicates, BAM, WGS or competitor benchmark. Each version received one warm-up and seven measured repetitions per case, alternating execution order. All 96 executions produced matching full-output SHA-256 values within their case. The machine was a four-vCPU GitHub-hosted AMD EPYC 7763 runner, using Rust 1.98.1. Timing includes deterministic record generation, sorting, spill I/O and buffered output; it does not include fsync or cold-cache storage behavior.

| Case | Baseline median seconds | Candidate median seconds | Baseline/candidate | Baseline median peak RSS bytes | Candidate median peak RSS bytes |
|---|---:|---:|---:|---:|---:|
| In-memory random | 0.146178208 | 0.126955411 | 1.151 | 54,136,832 | 46,092,288 |
| In-memory ordered | 0.053983023 | 0.053895204 | 1.002 | 46,043,136 | 46,055,424 |
| 32-run random merge | 0.170121658 | 0.151607807 | 1.122 | 3,969,024 | 3,489,792 |
| 32-run equal-key merge | 0.111873057 | 0.106724458 | 1.048 | 3,502,080 | 3,502,080 |
| 32-run reversed merge | 0.116610897 | 0.111857297 | 1.042 | 3,624,960 | 3,514,368 |
| Multi-pass, singleton tail | 0.157774641 | 0.143928980 | 1.096 | 3,981,312 | 3,481,600 |

Ordered input is effectively unchanged; do not promote noise into a win. The equal-key candidate had a 0.160019-second outlier, although its median improved. The raw bundle retains every repetition. The multi-pass case wrote 40,140,800 scratch bytes instead of 41,779,200, creating 22 runs instead of 24. This exact write reduction is independent of timer noise.

Most cases contain 262,144 records with 16-byte keys and 64-byte payloads. The multi-pass case contains 139,264 records. Spill cases use 8,192-record runs; the multi-pass fan-in is four and the other merge fan-in is 32. Full reproduction lives in `tools/bench_external_sort.rs` and `.github/workflows/external-sort-performance.yml`.

## Higher-priority remaining findings

### 1. The bounded MarkDuplicates path has an overly restrictive ordering gate

**Confirmed source behavior; end-to-end impact still needs a dedicated regression.** In `try_run_external_plan`, the monotonicity check compares `(tid, position, qname, cleared_flag)` and returns `None` on a decrease. It is applied to single inputs as well as multiple inputs. Two reads at the same coordinate, named `z` then `a`, therefore abandon this path. A terminal unplaced record with `tid = -1` also compares below an earlier mapped record.

`run_hts_container` then attempts a compact in-memory plan or the full-record fallback. Therefore the presence of external sorting does not establish bounded memory for ordinary coordinate-sorted files. The single-input fallback preserves input order; its contract must be distinguished from the multi-input path, which sorts the combined output.

Required repair: separate single-input replay order from the multi-input ordering contract, normalize unplaced coordinates wherever a coordinate comparator is required, and keep explicit validation rather than silently changing output order. Test beyond the 100,000-record compact threshold, with coordinate ties, unmapped tails, secondary/supplementary reads and multiple read groups. Verify both Picard output/metrics and which execution plan actually ran. Measure peak memory on increasing input sizes, not just a tiny functional fixture.

### 2. Equal QNAMEs across read groups require an adversarial parity test

**Correctness risk identified in source; not yet a demonstrated Picard mismatch.** The external pairing pass sorts by bare QNAME, then pairs adjacent records with the same name. Although read-group and library information survives in the payload, the shown pairing condition does not use it or require complementary mate flags.

Construct a large coordinate-sorted fixture in which the same QNAME appears in separate read groups/libraries and the first mates arrive before the second mates. Compare complete decisions, selected representatives, DS/DI, optical counts and library metrics against Picard. Do not change grouping rules based solely on this audit note: the exact upstream contract and the compact path must be tested together.

### 3. BAM run size is limited by record count, not allocated bytes

**Confirmed source behavior.** `BamExternalSortConfig` has `max_records_in_ram` but no byte budget; the default is 500,000 records. That is not a predictable memory bound for long reads or records with large auxiliary tags. The binary-key sorter already has a separate byte threshold.

Required repair: count allocated BAM payload capacity and record-wrapper storage, enforce a configurable byte threshold, define the isolated-oversized-record case, and include merge readers/compression queues in process-level memory accounting. Test long reads, large tags and alternating small/large records. Do not describe a record-count threshold as a hard RSS limit.

### 4. Thread settings are not a shared process budget

**Confirmed configuration mismatch.** `bgzf_threads_for` resolves the global override separately for each role, and `hts_io` calls `set_threads` separately on readers and writers. The competitor README calls the setting a global worker budget. It is currently a per-object setting, not a process-wide scheduler.

Required repair: use an explicit command-level budget or shared HTSlib pool, account for the application thread and concurrent readers/writers, and record observed thread peaks. Reconcile documentation with the implementation. Benchmark one-, two-, four- and eight-CPU limits, including many-input tasks. A faster wall time achieved by quietly allocating more workers is not a fair comparison.

### 5. Native SAM marking is a separate compatibility and memory boundary

**Confirmed source behavior; check the declared contract before classifying differences as bugs.** The text-SAM path reads the whole file and accumulates output in memory. Its visible duplicate key and first-seen selection differ structurally from the BAM read-end engine. The bounded BAM design must not be advertised as applying to every input format.

Required audit: trace CLI/parser guards and documented unsupported options; test format-equivalent SAM/BAM inputs for representative selection, library separation, CIGAR/unclipped position, optical behavior and removal/tagging options. Either consolidate the supported behavior or reject unsupported combinations with a clear message.

## Competition and release gate

Existing evidence is promising but not enough to claim that Turbo Picard beats every alternative. The repository's million-read synthetic MarkDuplicates guardrail records medians of 0.531262 seconds for Turbo Picard and 1.860894 seconds for Picard, but its input is not publicly retrievable. Its mitochondrial CRAM guardrail is explicitly not whole-genome evidence. The README correctly discloses that JVM startup contributes substantially to the small-fixture headline ratios.

Compare Picard, samtools, Sambamba and FastDup on the same suitable immutable coordinate-sorted input, with preparation cost either included for everyone or reported separately. Preserve exact winning-read differences rather than calling family-level agreement exact parity. SAMBLASTER belongs in an aligner-to-sorted-output pipeline comparison, not a coordinate-BAM comparison with hidden conversion costs.

The next release-level evidence must include retrievable 30x WGS, duplicate-heavy WES, UMI and optical-heavy inputs; five or more measured repetitions; exact versions, source/input hashes and options; CPU, memory, scratch and output size; full output/metric checks; and reproduction on a second machine. Test one-command replacement and the complete workflow separately. Keep slower cases visible.

## Merge status

This audit is not a merge or release authorization. The complete existing CI, Picard parity, package/container and workflow-starter checks remain required. The lightweight standard-library benchmark does not compile HTSlib or validate every native command. No scientific comparison contract was weakened by this change.
