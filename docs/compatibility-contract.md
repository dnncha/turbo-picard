# Compatibility contract

The compatibility contract makes Turbo-Picard safe to evaluate without turning
a passing fixture into an unsupported claim of universal equivalence.

## Compatibility levels

- A — byte-compatible: output bytes and required sidecars match Picard for the
  documented input and option scope.
- B — semantic-compatible: records, flags, tags, indexes, metrics rows and exit
  behaviour match the documented comparator; incidental headers or tie ordering
  may differ.
- C — scoped-compatible: a named subset of Picard options is covered. Options
  outside the subset must fail clearly or delegate.
- D — delegated: upstream Picard is the execution authority.
- X — unsupported: the command or option is unavailable in this release.

The level applies to a command, option scope, input format and output contract.
It is never a blanket statement about all Picard behaviour.

## Required command-matrix fields

Each command entry must identify:

- upstream Picard reference version;
- compatibility level;
- native scope and fallback scope;
- input formats and reference requirements;
- supported options and intentional differences;
- output and sidecar contracts;
- parity comparator;
- pinned real-data evidence identifiers;
- known failure modes;
- last independently reviewed release.

## Unsupported options

A native command must not ignore an option that changes scientific output. It
must implement it, reject it with a clear non-zero diagnostic, or delegate the
complete command to upstream Picard when fallback is configured.

The diagnostic must identify the command, option, compatibility level and
fallback action. This is especially important for duplicate marking, UMI, CRAM
reference, metrics, interval and VCF options.

## Comparison rules

Use the narrowest comparator that protects downstream science:

- duplicate flags, duplicate tags, optical counts and stable metrics for
  MarkDuplicates;
- record multisets for sorting commands where tie ordering is incidental;
- exact BAI bytes for BuildBamIndex;
- FASTQ bytes for SamToFastq and FastqToSam;
- stable metric rows with generated comments removed for metrics commands;
- summary histogram and exit status for ValidateSamFile;
- headers, records and sidecars where those are consumed downstream.

Every comparator must record its method in generated JSON evidence.

## Workflow preflight

The machine-readable explain and trial contracts are the supported integration
surface for WDL, Nextflow, Snakemake, CWL and CI wrappers. A wrapper must inspect:

- command name and schema version;
- compatibility level;
- native or delegated execution path;
- fallback command;
- declared inputs and outputs;
- reference requirement;
- known caveats;
- evidence target.

If the schema is unavailable or unknown, retain the existing Picard path.

## Versioning and known differences

Compatibility changes are release changes. If a previously native option becomes
delegated, unsupported or semantically different, record it in the changelog,
add a regression fixture and update the compatibility report.

Known differences must be public before execution. Current examples include
lightweight PDF chart artifacts rather than Picard-equivalent rendered plots for
some chart-producing metrics commands, and partial-native scopes whose advanced
options remain delegated.

