---
title: 'turbo-picard: Picard-compatible command-line tools for faster sequencing workflow steps'
tags:
  - bioinformatics
  - genomics
  - sequencing
  - SAM
  - BAM
  - VCF
  - Rust
authors:
  - name: Donncha O'Toole
    orcid: 0009-0003-5012-7229
    affiliation: 1
affiliations:
  - name: Independent researcher
    index: 1
date: 23 July 2026
bibliography: paper.bib
---

# Summary

`turbo-picard` is a command-line toolkit for speeding up selected Picard-style
steps in sequencing workflows. Picard commands are often wired into Snakemake,
Nextflow, WDL, and older shell pipelines for sorting alignments, marking
duplicates, creating indexes, converting FASTQ and SAM/BAM files, and collecting
quality-control metrics. `turbo-picard` reimplements selected commands in Rust
while keeping the familiar command names and `KEY=VALUE` arguments, so an
existing workflow can test a faster implementation one step at a time.

The project is deliberately not presented as a full Picard replacement. Each
command has documented native scope, fallback scope, parity tests against Picard
3.4.0, and saved benchmark evidence. Unsupported commands fail clearly by
default, or can be delegated to upstream Picard when the user configures a
fallback command. The current release provides native or partly native coverage
for 32 Picard-style commands across SAM/BAM transformations, FASTQ conversion,
metrics, FASTA dictionary generation, VCF utilities, interval-list processing,
and validation.

# Statement of need

Picard is a long-standing part of research and clinical sequencing pipelines
[@picard]. Its command-line conventions appear in workflow definitions,
standard operating procedures, and archived analysis methods. Replacing one of
those steps is therefore not just a performance question. It is also a
compatibility question: the faster command must accept the invocation a pipeline
already uses, produce equivalent outputs for the tested case, and make clear
where the evidence stops.

In practice, repeated wall-clock time can accumulate in format conversion,
duplicate marking, coordinate sorting, metrics collection, and VCF housekeeping.
Faster implementations are useful only if they reduce queue time and compute
cost without changing the analysis record. `turbo-picard` addresses this by
targeting Picard-compatible commands rather than requiring a new interface. Its
release evidence ties every public speed claim to parity checks:
the latest saved benchmark suite, run on 2026-08-14, reports 32/32 benchmarked
commands passing parity, with a geometric mean speedup of 87.47x and a slowest
saved speedup of 22.17x. These results are specific to the checked command set
and benchmark environment.

# State of the field

The relevant file formats and libraries are mature. SAM, BAM, and CRAM are
standard alignment formats maintained through the hts-specs project
[@sam-spec], and htslib provides widely used low-level implementations for
reading and writing those formats [@htslib]. Picard provides a broad Java-based
toolkit for manipulating high-throughput sequencing data [@picard]. Workflow
systems such as Snakemake and Nextflow make these command-line tools repeatable
across larger analyses [@snakemake; @nextflow].

`turbo-picard` fits beside these tools. It does not replace htslib, redefine the
file formats, or try to cover every Picard command at once. It uses Rust [@rust]
and Rust bindings to htslib where appropriate, and focuses on commands where a
native implementation can be tested and maintained. This preserves a
conservative migration path: call `turbo-picard` explicitly for commands that
have been checked, and keep upstream Picard available for unsupported behavior.

# Software design

The repository is organized as a Rust workspace. The command-line crate handles
Picard-style argument parsing, command dispatch, output sidecars, fallback
behavior, and user-facing help. Shared record and file-format utilities live in
a core crate. Duplicate-marking logic is kept in a separate crate so that the
larger `MarkDuplicates` implementation can evolve without making the command
dispatcher harder to reason about.

Compatibility boundaries are tracked in `docs/command-matrix.yml`. Each entry
names the command status, native scope, fallback scope, and parity script used
by the project. The documentation defines parity narrowly: a specific command,
option set, and input shape produced the same checked output as upstream Picard
under the comparison method named in the evidence. For example, SAM/BAM
transformations compare normalized record content, `BuildBamIndex` compares the
exact BAI digest, `SamToFastq` compares FASTQ outputs byte-for-byte, and metrics
commands compare stable metrics rows.

The benchmark and real-data evidence are checked into the repository. Synthetic
benchmarks are paired with parity checks, and public real-data fixtures are
pinned by source URL, full commit, and SHA-256 input hash. The release-ready
evidence currently includes a public GATK NA12878 mitochondrial BAM and a
Picard SNVQ metrics test BAM. These fixtures cover a representative release
command set, but the documentation is explicit that they are not proof for every
assay, aligner, reference build, UMI convention, or malformed input a laboratory
might encounter.

# Research impact statement

`turbo-picard` is intended for researchers and bioinformatics engineers who
maintain sequencing pipelines and need to reduce runtime without losing a clear
audit trail. The most direct use is side-by-side evaluation in an existing
workflow: run the Picard step and the corresponding `turbo-picard` command,
compare the outputs that matter for that command, and keep the command line,
input digest, Picard version, `turbo-picard` version, and evidence report with
the analysis record.

The project is early, so this paper does not claim broad community adoption or
published downstream discoveries enabled by the software. The current value is
more practical: a tested and documented implementation that laboratories can
software DOI for the current release [@turbo-picard-zenodo].
evaluate command by command. The Zenodo archive for version 0.1.10 provides a
software DOI for the current release [@turbo-picard-zenodo].
software DOI for the current release [@turbo-picard-zenodo].

# AI usage disclosure

Generative AI assistance, specifically OpenAI Codex, was used during repository
maintenance and preparation of this paper draft, including code editing, test
scaffolding, documentation editing, and copy-editing. The author made the
project design decisions, reviewed and edited AI-assisted changes, ran the
repository checks described in the documentation, and kept paper claims tied to
checked benchmark, parity, citation, and release metadata.

# Acknowledgements

No external funding supported this work. The project builds on the public work
of the Picard, htslib, SAM/BAM/VCF specification, Rust, Bioconda, and workflow
tool communities.

# References
