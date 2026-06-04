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
date: 4 June 2026
bibliography: paper.bib
---

# Summary

`turbo-picard` is command-line software for speeding up selected steps in DNA
sequencing workflows. Many genomics pipelines use Picard-style tools to sort
alignment files, mark duplicate reads, convert between common file formats,
build indexes, and calculate quality-control metrics. `turbo-picard` reimplements
selected Picard commands in Rust while keeping familiar command names and
`KEY=VALUE` arguments. This lets a workflow try a faster implementation for a
known command without changing the surrounding pipeline.

The project is deliberately not presented as a complete replacement for Picard.
Each command has documented native scope, fallback scope, parity tests against
Picard 3.4.0, and saved benchmark evidence. Unsupported commands fail clearly by
default, or can be delegated to upstream Picard when the user configures a
fallback command. The current release provides native or partly native coverage
for 32 command surfaces across SAM/BAM transformations, FASTQ conversion,
metrics, FASTA dictionary generation, VCF utilities, interval-list processing,
and validation.

# Statement of need

Picard is a long-standing part of research and clinical sequencing pipelines
[@picard]. Its command-line conventions are embedded in workflow languages,
institutional standard operating procedures, and archived analysis methods.
Replacing those steps with a faster tool is therefore not only a performance
problem. It is also a compatibility and evidence problem: the new command must
accept the same style of invocation, produce equivalent outputs for the tested
use case, and make clear where its behavior has not been proven.

Large sequencing workflows can spend repeated wall-clock time in file-format
conversion, duplicate marking, coordinate sorting, metrics collection, and VCF
housekeeping. Faster implementations are useful when they reduce queue time,
iteration time, and compute cost without changing the scientific record.
`turbo-picard` addresses this by targeting Picard-compatible command surfaces
rather than introducing a new API. Its release evidence ties speed claims to
parity checks: the saved benchmark suite reports 32/32 benchmarked commands
passing parity, with a geometric mean speedup of 26.74x and a slowest saved
speedup of 8.55x in the 2026-06-04 benchmark log.

# State of the field

The relevant file formats and libraries are mature. SAM, BAM, and CRAM are
standard alignment formats maintained through the hts-specs project
[@sam-spec], and htslib provides widely used low-level implementations for
reading and writing those formats [@htslib]. Picard provides a broad Java-based
toolkit for manipulating high-throughput sequencing data [@picard]. Workflow
systems such as Snakemake and Nextflow make these command-line tools repeatable
across larger analyses [@snakemake; @nextflow].

`turbo-picard` fits beside these tools. It does not replace htslib, redefine the
file formats, or attempt to cover every Picard command. Instead, it uses Rust
[@rust] and Rust bindings to htslib where appropriate, and focuses on a growing
set of Picard-style commands where a native implementation can be tested and
maintained. This gives users a conservative migration path: use explicit
`turbo-picard` invocations for commands that have been checked, and keep
upstream Picard available for unsupported behavior.

# Software design

The repository is organized as a Rust workspace. The command-line crate handles
Picard-style argument parsing, command dispatch, output sidecars, fallback
behavior, and user-facing help. Shared record and file-format utilities live in
a core crate. Duplicate-marking logic is kept in a separate crate so that the
larger `MarkDuplicates` implementation can evolve without making the command
dispatcher harder to reason about.

Compatibility boundaries are tracked in `docs/command-matrix.yml`. Each entry
names the command status, the native scope, the fallback scope, and the parity
script used by the project. The documentation explains what parity means: a
specific command, option set, and input shape produced the same checked output
as upstream Picard under the comparison method named in the evidence. That is a
narrow claim, and the project treats it as such. For example, SAM/BAM
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
maintain sequencing pipelines and want to reduce runtime while preserving
traceable behavior. The most direct research application is side-by-side
evaluation of faster command implementations in existing workflows. A lab can
run a Picard-heavy step through `turbo-picard`, compare the relevant outputs,
and keep the command line, input digest, Picard version, `turbo-picard` version,
and evidence report with the analysis record.

The project is early, so this paper does not claim broad community adoption or
published downstream discoveries enabled by the software. Its immediate impact
is to make a tested, documented, citable implementation available for evaluation
and incremental adoption. The Zenodo archive for version 0.1.1 provides a
software DOI for the current release [@turbo-picard-zenodo].

# AI usage disclosure

Generative AI assistance was used during repository maintenance and preparation
of this paper draft. The author reviewed and edited the resulting text and code,
ran the repository checks described in the documentation, and kept generated
claims tied to checked benchmark, parity, citation, and release metadata.

# Acknowledgements

No external funding supported this work. The project depends on the public work
of the Picard, htslib, SAM/BAM/VCF specification, Rust, Bioconda, and workflow
tool communities.

# References
