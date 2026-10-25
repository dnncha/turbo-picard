# Competitive process coverage

Treat a faster rival as an engineering lead: reproduce the loss, inspect the
public implementation, adapt the useful mechanism, and retain before/after
measurements with the original scientific output contract. A result applies to
its input, options and resource budget. Keep failed comparisons and slower cases.

## Current work

The October 25, 2026 investigation starts with the duplicate-marking loss
recorded in the 0.1.16 release notes. FastDup's
[pinned read-end generator](https://github.com/zzhofict/FastDup/blob/7b3f62587283a257fd38c5c20ceeda0364c285ef/src/markdup/md_pipeline.cpp)
stores a completed template as one sortable pair. Turbo Picard now does the
same while retaining both input ordinals and all metadata used by Picard's
selection and tagging rules. It also reserves encoded buffer sizes and uses
the retained pair payloads to avoid constructing unused fragment entries.
Mixed inputs recover paired ends and unmatched mates for fragment dominance.
The shared HTSlib dependency also enables its optional libdeflate backend,
following samtools' use of that codec for BAM I/O. Benchmark provenance records
the codec and linked libraries as well as each executable.

[Retained measurements and reproduction details](review-evidence/2026-10-25/README.md)
include all eleven synthetic parity cases, the two native competitor profiles,
the shared BAM I/O profiles, neutral results and competitor output differences.
They identify the measured pre-v0.1.17 source. The shipping branch preserves the
subsequent correctness release and is checked by fresh benchmark and strict parity
jobs in [PR #46](https://github.com/dnncha/turbo-picard/pull/46).

The bounded harness measures the original checkout, the candidate and actual
Picard on eleven adversarial whole-command cases, with one warm-up and three
alternating measured repetitions. The native landscape harness measures full
coordinate-input pipelines against selected rivals; samtools' required
collation, mate tagging and final coordinate sorting stay inside its timer.
The full Picard contract and the diagnostic unordered mandatory-field/RG
comparison remain separate. See [the runner documentation](../docs/performance.rst).

## Coverage still needed

The existing Picard suite covers 32 commands on small fixtures. It does not
establish a lead over other native tools or on production data. This table
records comparison boundaries for expanding that coverage.

| Process | Rivals to compare | Existing evidence and next measurement |
| --- | --- | --- |
| Coordinate-input duplicate marking | FastDup, samtools, Sambamba, dupblaster | Current work above; add duplicate-heavy WES, WGS, UMI, optical, multiple libraries and distant mates. |
| Duplicate marking after alignment | dupblaster, SAMBLASTER | Compare the complete pipeline from the same starting input, including grouping and final sorting. |
| WGS coverage and bundled QC | riker | [Small mitochondrial smoke evidence](riker-comparison/README.md) exists; run the staged WGS profiles before generalizing. |
| Hybrid-capture QC | riker hybcap, Picard | Native ALL_READS parity and interval lookup improvements exist; representative capture-data competitor timings remain pending. |
| Alignment sorting, merging and indexing | samtools, Sambamba | Match sorting stability, input order, output compression, index format and thread budget; retain external-spill and already-sorted cases. |
| BAM/CRAM/FASTQ conversion and mate repair | samtools, Picard | Match clipping, orphan handling, read groups, tags, references and sidecars before timing complete transformations. |
| VCF gathering, sorting, merging and reference updates | bcftools, Picard | Match headers, sample order, record order, normalization and indexing semantics; subcommand names alone do not establish equivalence. |

For every new comparison, record source and executable hashes, input provenance,
exact argv, versions, repetitions, wall and CPU time, peak memory and temporary
disk. Improve the bottleneck identified by that comparison, then rerun parity
and the same resource-controlled profile. A missing comparison is an open gap.
