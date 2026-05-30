# turbo-picard

`turbo-picard` is a Picard-shaped Rust toolkit focused on high-impact, drop-in
replacement workflows. The current native engines target `MarkDuplicates`,
`SortSam`, `CleanSam`, `MergeSamFiles`, `BuildBamIndex`, `SamToFastq`, `FastqToSam`,
`AddOrReplaceReadGroups`, `CollectAlignmentSummaryMetrics`,
`CollectQualityYieldMetrics`, `CreateSequenceDictionary`, `NormalizeFasta`, and
`BedToIntervalList`, plus partial native `ViewSam`, `ReplaceSamHeader`,
`QualityScoreDistribution`, `MeanQualityByCycle`,
`CollectBaseDistributionByCycle`, `CollectInsertSizeMetrics`,
`CollectMultipleMetrics`, `CollectWgsMetrics`, `FixMateInformation`,
`IntervalListTools`, `RevertSam`, `SetNmMdAndUqTags`, `ValidateSamFile`, `LiftoverVcf`,
`UpdateVcfSequenceDictionary`, `GatherVcfs`, `SortVcf`, and `MergeVcfs`.

## Why It Exists

Picard is embedded in bioinformatics pipelines everywhere, but a lot of routine
pipeline time is spent in command shapes that can be made dramatically faster
without asking workflow owners to rewrite their WDL, Nextflow, Snakemake, or
shell glue. `turbo-picard` keeps the Picard command model and accelerates the
hot native paths in Rust, while preserving a fallback route for surfaces that
still belong to upstream Picard.

Fresh local benchmark evidence from `python3 tools/bench_suite.py --repeats 1
--skip-build`:

- `28/28` benchmark parity checks passing.
- `104.68x` top measured speedup.
- `6.69x` current lowest suite speedup after the RevertSam fast path.
- `28.41x` geometric mean speedup across the 28-command suite.
- Native commands cover BAM/FASTQ transforms, common metrics collectors, VCF
  plumbing, interval/reference prep, and high-impact repair/tagging commands.

![turbo-picard benchmark speedups](docs/site/assets/benchmark-speedups.svg)

The static marketing page is available at
[`docs/site/index.html`](docs/site/index.html). It packages the current evidence
for pipeline owners evaluating a cautious Picard replacement path.

The main package installs one non-shadowing command-line entrypoint:

- `turbo-picard`

The optional `turbo-picard-picard-shim` package installs a `picard`
compatibility shim for workflow managers and scripts that invoke Picard by
command name. Use the shim deliberately because it shadows upstream Picard on
`PATH`.

## Install From Source

```bash
cargo install --locked --path crates/turbo-picard-cli --bin turbo-picard --bin picard
```

## Usage

```bash
turbo-picard MarkDuplicates \
  I=input.bam \
  O=marked.bam \
  M=metrics.txt \
  ASSUME_SORTED=true \
  VALIDATION_STRINGENCY=SILENT
```

The compatibility entrypoint accepts the same command shape:

```bash
picard MarkDuplicates I=input.bam O=marked.bam M=metrics.txt
```

Native `SortSam` supports coordinate and queryname sorting:

```bash
picard SortSam I=input.bam O=coordinate.bam SORT_ORDER=coordinate
picard SortSam I=input.bam O=queryname.bam SO=queryname
```

Native `CleanSam` streams SAM/BAM cleanup for common cases:

```bash
picard CleanSam I=input.bam O=cleaned.bam
```

Native `MergeSamFiles` merges repeated `INPUT` values and can emit coordinate,
queryname, or unsorted output:

```bash
picard MergeSamFiles I=lane1.bam I=lane2.bam O=merged.bam SORT_ORDER=coordinate
picard MergeSamFiles I=lane1.bam I=lane2.bam O=merged.sam SO=unsorted CO='merged lanes'
```

Native `BuildBamIndex` creates BAI sidecars for coordinate-sorted BAM files:

```bash
picard BuildBamIndex I=coordinate.bam
picard BuildBamIndex I=coordinate.bam O=coordinate.bai
```

Native `SamToFastq` streams SAM/BAM records to FASTQ:

```bash
picard SamToFastq I=input.bam FASTQ=reads.fastq
picard SamToFastq I=input.bam FASTQ=r1.fastq SECOND_END_FASTQ=r2.fastq
```

Native `FastqToSam` streams FASTQ records into unmapped SAM/BAM:

```bash
picard FastqToSam F1=r1.fastq F2=r2.fastq O=unmapped.bam SM=sample RG=rg1 QUALITY_FORMAT=Standard
```

Native `AddOrReplaceReadGroups` streams SAM/BAM records while replacing `@RG`
metadata and per-record `RG:Z` tags:

```bash
picard AddOrReplaceReadGroups I=input.bam O=rg.bam RGID=1 RGLB=lib RGPL=ILLUMINA RGPU=unit RGSM=sample
```

Native `CollectAlignmentSummaryMetrics` streams SAM/BAM records into Picard-style
alignment summary metrics:

```bash
picard CollectAlignmentSummaryMetrics I=input.bam O=alignment_metrics.txt
```

Native `CollectQualityYieldMetrics` streams SAM/BAM records into Picard-style
quality yield metrics:

```bash
picard CollectQualityYieldMetrics I=input.bam O=quality_yield_metrics.txt
```

Native `CollectWgsMetrics` streams coordinate-sorted SAM/BAM records into
Picard-style whole-genome coverage metrics for common no-interval runs:

```bash
picard CollectWgsMetrics I=input.bam O=wgs_metrics.txt R=reference.fa COUNT_UNPAIRED=true
```

Native quality histogram commands generate metrics and chart artifacts:

```bash
picard QualityScoreDistribution I=input.bam O=quality_distribution.txt CHART=quality_distribution.pdf
picard MeanQualityByCycle I=input.bam O=mean_quality_by_cycle.txt CHART=mean_quality_by_cycle.pdf
picard CollectBaseDistributionByCycle I=input.bam O=base_distribution.txt CHART=base_distribution.pdf
picard CollectGcBiasMetrics I=input.bam O=gc_bias.detail.txt S=gc_bias.summary.txt CHART=gc_bias.pdf R=reference.fa
picard CollectInsertSizeMetrics I=input.bam O=insert_size_metrics.txt H=insert_size_histogram.pdf
picard CollectMultipleMetrics I=input.bam O=multiple PROGRAM=null PROGRAM=CollectInsertSizeMetrics PROGRAM=CollectBaseDistributionByCycle PROGRAM=CollectGcBiasMetrics R=reference.fa EXTRA_ARGUMENT=CollectGcBiasMetrics::SCAN_WINDOW_SIZE=100
picard FixMateInformation I=queryname.bam O=fixed.bam ASSUME_SORTED=true SORT_ORDER=queryname
picard RevertSam I=aligned.bam O=unmapped.bam
picard SetNmMdAndUqTags I=coordinate.bam O=tagged.bam R=reference.fa
picard ValidateSamFile I=input.bam MODE=SUMMARY
picard IntervalListTools I=a.interval_list I=b.interval_list O=merged.interval_list ACTION=CONCAT SORT=true UNIQUE=true
```

Native `CreateSequenceDictionary` creates Picard-style `.dict` files from FASTA:

```bash
picard CreateSequenceDictionary R=reference.fa O=reference.dict
```

Native `NormalizeFasta` and `BedToIntervalList` cover common reference prep:

```bash
picard NormalizeFasta I=reference.fa O=normalized.fa LINE_LENGTH=100
picard BedToIntervalList I=targets.bed O=targets.interval_list SD=reference.dict
```

Native `ViewSam` and `ReplaceSamHeader` cover common SAM/BAM plumbing:

```bash
picard ViewSam I=input.bam > input.sam
picard ReplaceSamHeader I=input.bam O=reheadered.bam H=header.sam CREATE_MD5_FILE=true
```

Native `UpdateVcfSequenceDictionary` replaces VCF contig headers from a Picard
sequence dictionary:

```bash
picard UpdateVcfSequenceDictionary I=input.vcf O=updated.vcf SD=reference.dict
```

Native `GatherVcfs` and `SortVcf` cover common VCF shard handling:

```bash
picard GatherVcfs I=shard1.vcf I=shard2.vcf O=gathered.vcf
picard SortVcf I=unsorted.vcf O=sorted.vcf SD=reference.dict
picard MergeVcfs I=batch1.vcf I=batch2.vcf O=merged.vcf
picard LiftoverVcf I=input.vcf O=lifted.vcf CHAIN=build.chain REJECT=rejected.vcf R=target.fa
```

By default, unsupported Picard commands fail clearly. For drop-in deployments
that need the rest of Picard to keep working, set a fallback command:

```bash
export TURBO_PICARD_FALLBACK_COMMAND='mamba run -p /opt/conda/envs/picard picard'
picard SortSam I=input.bam O=queryname.bam SORT_ORDER=queryname
```

When configured, `turbo-picard` runs native accelerated commands first. It
delegates only unsupported commands or explicitly unsupported native surfaces to
the fallback command and returns the fallback exit code. Native I/O failures and
malformed native inputs are not delegated, so errors are not masked. The
fallback value is a shell command prefix, so `java -jar /path/to/picard.jar`
works. Prefer an absolute upstream Picard command or JAR path; a bare `picard`
fallback can resolve back to the shim if it shadows `PATH`.

## Adoption Playbook

The safest rollout is evidence-first and reversible:

1. **Shadow** representative pipeline shards with the non-shadowing
   `turbo-picard` binary. Keep upstream Picard as the production path while you
   compare command outputs, sidecars, metrics, and exit behavior.
2. **Prove** each native command with the parity scripts and benchmark suite:

   ```bash
   python3 tools/verify_command_matrix.py
   ./tools/verify_basic_revertsam_parity.sh
   ./tools/verify_basic_setnmmdanduqtags_parity.sh
   printf 'benchmark_date=%s source=python3 tools/bench_suite.py --repeats 1 --skip-build\n' "$(date +%F)" > docs/site/assets/bench-suite-output.txt
   python3 tools/bench_suite.py --repeats 1 --skip-build | tee -a docs/site/assets/bench-suite-output.txt
   python3 tools/render_benchmark_assets.py --suite-output docs/site/assets/bench-suite-output.txt
   python3 tools/verify_benchmark_log_evidence.py
   ```

3. **Switch** only the covered workflow surfaces to the `picard` shim. Set
   `TURBO_PICARD_FALLBACK_COMMAND` to an absolute upstream Picard command so
   unsupported surfaces continue to run without a workflow rewrite.
4. **Gate** upgrades in CI. Keep the command matrix, targeted parity scripts,
   and benchmark summary as release evidence. The rendered static site consumes
   `docs/site/assets/benchmark-data.json`, which records rank, parity status,
   command count, top speedup, floor speedup, median speedup, geometric mean,
   and the raw `docs/site/assets/bench-suite-output.txt` source artifact.

Recommended CI gates for Picard-heavy pipelines:

| Gate | Command | Purpose |
| --- | --- | --- |
| Command matrix | `python3 tools/verify_command_matrix.py` | Verifies supported, partial, and fallback command routing remains explicit. |
| Targeted parity | `./tools/verify_basic_<command>_parity.sh` | Compares stable output for the commands used by your workflow. |
| Full local suite | `printf 'benchmark_date=%s source=python3 tools/bench_suite.py --repeats 1 --skip-build\n' "$(date +%F)" > docs/site/assets/bench-suite-output.txt && python3 tools/bench_suite.py --repeats 1 --skip-build \| tee -a docs/site/assets/bench-suite-output.txt` | Regenerates parity-plus-speed evidence across the benchmarked native commands and preserves a dated raw log. |
| Website assets | `python3 tools/render_benchmark_assets.py --suite-output docs/site/assets/bench-suite-output.txt` | Rebuilds the graph and JSON behind the static adoption page from the saved suite output. |
| Evidence freshness | `python3 tools/verify_benchmark_log_evidence.py` | Fails if the rendered JSON drifts from the dated raw benchmark log. |

## Supported MarkDuplicates Surface

Implemented input/output coverage:

- BAM input and output
- SAM text input and output
- repeated `INPUT` / `I` for multi-BAM workflows
- Picard-style `KEY=VALUE` arguments and short aliases such as `I`, `O`, and `M`

Implemented options include:

- `REMOVE_DUPLICATES`
- `REMOVE_SEQUENCING_DUPLICATES`
- `ASSUME_SORTED`
- `ASSUME_SORT_ORDER=coordinate`
- `VALIDATION_STRINGENCY`
- `QUIET`
- `CREATE_INDEX`
- `CREATE_MD5_FILE`
- `DUPLICATE_SCORING_STRATEGY=SUM_OF_BASE_QUALITIES`
- `READ_NAME_REGEX=null`
- `TAGGING_POLICY=All|OpticalOnly|DontTag`
- `TAG_DUPLICATE_SET_MEMBERS`
- `BARCODE_TAG`
- `READ_ONE_BARCODE_TAG`
- `READ_TWO_BARCODE_TAG`
- `CLEAR_DT`
- `OPTICAL_DUPLICATE_PIXEL_DISTANCE`
- `COMPRESSION_LEVEL`

Accepted compatibility options that are validated or ignored when they do not
change the current native implementation:

- `MAX_RECORDS_IN_RAM`
- `MAX_FILE_HANDLES_FOR_READ_ENDS_MAP`
- `MAX_SEQUENCES_FOR_DISK_READ_ENDS_MAP`
- `SORTING_COLLECTION_SIZE_RATIO`
- `TMP_DIR`
- `VERBOSITY`
- `ADD_PG_TAG_TO_READS`
- `USE_JDK_INFLATER`
- `USE_JDK_DEFLATER`
- `PROGRAM_RECORD_ID`
- `PROGRAM_GROUP_NAME`
- `PROGRAM_GROUP_VERSION`
- `PROGRAM_GROUP_COMMAND_LINE`
- `REFERENCE_SEQUENCE`
- `COMMENT`

## Supported SortSam Surface

Implemented input/output coverage:

- BAM input and output
- SAM text input and output
- Picard-style `KEY=VALUE` arguments and short aliases such as `I`, `O`, and
  `SO`

Implemented options include:

- `SORT_ORDER=coordinate|queryname`
- `VALIDATION_STRINGENCY`
- `QUIET`
- `TMP_DIR`
- `MAX_RECORDS_IN_RAM`
- `COMPRESSION_LEVEL`
- `CREATE_INDEX`
- `CREATE_MD5_FILE`

Accepted compatibility options that are validated or ignored when they do not
change the current native implementation:

- `VERBOSITY`

Performance behavior:

- Already-sorted input is streamed with a rewritten header instead of being
  materialized and sorted again.

## Supported MergeSamFiles Surface

Implemented input/output coverage:

- repeated `INPUT` / `I` values
- BAM input and output
- SAM text input and output
- identical sequence dictionaries across inputs
- read-group collision rewrite for conflicting `@RG ID` values and per-record
  `RG:Z` tags
- Picard-style `KEY=VALUE` arguments and short aliases such as `I`, `O`, `SO`,
  and `CO`

Implemented options include:

- `SORT_ORDER=coordinate|queryname|unsorted`; default is `coordinate`
- `ASSUME_SORTED=true|false`; skips native sortedness validation when callers
  already know each input is sorted by the requested order
- `COMMENT`
- `VALIDATION_STRINGENCY`
- `QUIET`
- `TMP_DIR`
- `MAX_RECORDS_IN_RAM`
- `COMPRESSION_LEVEL`
- `CREATE_INDEX`
- `CREATE_MD5_FILE`
- `MERGE_SEQUENCE_DICTIONARIES=false`

Accepted compatibility options that are validated or ignored when they do not
change the current native implementation:

- `VERBOSITY`

Unsupported merge surfaces, including CRAM, `.list` input expansion, interval
filtering, and dictionary merging, should be run through
`TURBO_PICARD_FALLBACK_COMMAND`.

Performance behavior:

- Coordinate/queryname merges use an exact k-way heap merge when every input is
  already sorted by the requested order. `ASSUME_SORTED=true` avoids the
  preflight sortedness scan for trusted pipeline inputs.
- If any input is not sorted by the requested order, the command falls back to
  the full in-memory sort path.

## Supported SamToFastq Surface

Implemented input/output coverage:

- BAM input
- SAM text input
- single-end FASTQ output
- paired FASTQ output with `SECOND_END_FASTQ`
- unpaired read routing with `UNPAIRED_FASTQ`
- interleaved paired output with `INTERLEAVE=true`
- Picard-compatible default filtering of non-PF and non-primary records
- optional inclusion with `INCLUDE_NON_PF_READS=true` and
  `INCLUDE_NON_PRIMARY_ALIGNMENTS=true`

Implemented options include:

- `FASTQ`
- `SECOND_END_FASTQ`
- `UNPAIRED_FASTQ`
- `INTERLEAVE`
- `RE_REVERSE`
- `INCLUDE_NON_PF_READS`
- `INCLUDE_NON_PRIMARY_ALIGNMENTS`
- `VALIDATION_STRINGENCY`
- `QUIET`
- `COMPRESSION_LEVEL`
- `CREATE_MD5_FILE`

Accepted compatibility options that are validated or ignored when they do not
change the current native implementation:

- `VERBOSITY`

## Supported FastqToSam Surface

Implemented input/output coverage:

- single-end FASTQ input with `FASTQ` / `F1`
- paired FASTQ input with `FASTQ2` / `F2`
- plain or gzip-compressed FASTQ input
- SAM/BAM unmapped output with queryname sort-order header
- Picard-style read-group header and per-record `RG:Z` tag
- `QUALITY_FORMAT=Standard` and `QUALITY_FORMAT=Illumina`

Implemented options include `OUTPUT` / `O`, `SAMPLE_NAME` / `SM`,
`READ_GROUP_NAME` / `RG`, `LIBRARY_NAME` / `LB`, `PLATFORM` / `PL`,
`PLATFORM_UNIT` / `PU`, `SEQUENCING_CENTER` / `CN`, `DESCRIPTION` / `DS`,
`RUN_DATE` / `DT`, `PREDICTED_INSERT_SIZE` / `PI`, `PROGRAM_GROUP`,
`PLATFORM_MODEL`, `SORT_ORDER=queryname`, `SORT_ORDER=coordinate`,
`SORT_ORDER=unsorted`, `COMMENT`, `VALIDATION_STRINGENCY`, `QUIET`,
`COMPRESSION_LEVEL`, and `CREATE_MD5_FILE`.

Unsupported input name sorting, custom non-comment header injection, and
advanced quality detection surfaces should be run through
`TURBO_PICARD_FALLBACK_COMMAND`.

## Supported AddOrReplaceReadGroups Surface

Implemented input/output coverage:

- BAM input and output
- SAM text input and output
- one replacement read group
- per-record `RG:Z` rewrite

Implemented options include:

- `RGID`
- `RGLB`
- `RGPL`
- `RGPU`
- `RGSM`
- `RGCN`
- `RGDS`
- `RGDT`
- `RGPI`
- `RGPG`
- `RGPM`
- `VALIDATION_STRINGENCY`
- `QUIET`
- `COMPRESSION_LEVEL`

Accepted compatibility options that are validated or ignored when they do not
change the current native implementation:

- `VERBOSITY`

## Supported CollectAlignmentSummaryMetrics Surface

Implemented input/output coverage:

- BAM input
- SAM text input
- no-reference `ALL_READS`, `SAMPLE`, `LIBRARY`, and `READ_GROUP` alignment
  summary metrics
- Picard-style `AlignmentSummaryMetrics` output and read-length histogram

Implemented options include:

- `INPUT` / `I`
- `OUTPUT` / `O`
- `VALIDATION_STRINGENCY`
- `QUIET`
- `ASSUME_SORTED`
- `COLLECT_ALIGNMENT_INFORMATION=true`
- `STOP_AFTER`
- `COMPRESSION_LEVEL`
- `METRIC_ACCUMULATION_LEVEL=ALL_READS|SAMPLE|LIBRARY|READ_GROUP` /
  `LEVEL=ALL_READS|SAMPLE|LIBRARY|READ_GROUP`

Accepted compatibility options that are validated or ignored when they do not
change the current native implementation:

- `VERBOSITY`
- `TMP_DIR`
- `MAX_RECORDS_IN_RAM`

Unsupported metrics surfaces, including reference-dependent mismatch/error
metrics, should be run through `TURBO_PICARD_FALLBACK_COMMAND`.

## Supported CreateSequenceDictionary Surface

Implemented input/output coverage:

- plain or gzip-compressed FASTA input
- Picard-style SAM dictionary output
- MD5 sequence digests
- `UR:file://...` output
- derived `.dict` output path when `OUTPUT` is omitted

Implemented options include:

- `REFERENCE` / `REFERENCE_SEQUENCE` / `R`
- `OUTPUT` / `O`
- `TRUNCATE_NAMES_AT_WHITESPACE`
- `URI`
- `GENOME_ASSEMBLY`
- `SPECIES`
- `NUM_SEQUENCES`
- `VALIDATION_STRINGENCY`
- `QUIET`

Accepted compatibility options that are validated or ignored when they do not
change the current native implementation:

- `VERBOSITY`

## Supported BuildBamIndex Surface

Implemented input/output coverage:

- coordinate-sorted BAM input
- Picard-style default `.bai` output path
- explicit `OUTPUT` / `O`

Unsupported SAM, CRAM, CSI, and non-coordinate inputs should be run through
`TURBO_PICARD_FALLBACK_COMMAND`.

## Supported CollectQualityYieldMetrics Surface

Implemented input/output coverage:

- BAM input
- SAM text input
- Picard-style `QualityYieldMetrics` output
- primary alignments only by default
- optional secondary and supplemental alignment inclusion
- original `OQ:Z` qualities by default, or current `QUAL` values with
  `USE_ORIGINAL_QUALITIES=false`

Implemented options include:

- `INPUT` / `I`
- `OUTPUT` / `O`
- `USE_ORIGINAL_QUALITIES`
- `INCLUDE_SECONDARY_ALIGNMENTS`
- `INCLUDE_SUPPLEMENTAL_ALIGNMENTS`
- `STOP_AFTER`
- `VALIDATION_STRINGENCY`
- `QUIET`

## Supported CollectBaseDistributionByCycle Surface

Implemented input/output coverage:

- BAM input
- SAM text input
- Picard-style `BaseDistributionByCycleMetrics` output
- `CHART_OUTPUT` / `CHART` chart artifact
- primary alignments only, with Picard-compatible reverse-strand cycle ordering

Implemented options include `INPUT` / `I`, `OUTPUT` / `O`,
`CHART_OUTPUT` / `CHART`, `ALIGNED_READS_ONLY`, `PF_READS_ONLY`,
`ASSUME_SORTED`, `STOP_AFTER`, `VALIDATION_STRINGENCY`, `QUIET`, `TMP_DIR`,
and `MAX_RECORDS_IN_RAM`.

Unsupported accumulation levels and hidden chart customizations should be run
through `TURBO_PICARD_FALLBACK_COMMAND`.

## Supported QualityScoreDistribution and MeanQualityByCycle Surface

Implemented input/output coverage:

- BAM input
- Picard-style histogram outputs
- `CHART_OUTPUT` / `CHART` chart artifacts
- primary alignments only, with Picard-compatible reverse-strand cycle ordering
- `OQ` original quality histograms when present

Implemented options include `INPUT` / `I`, `OUTPUT` / `O`,
`CHART_OUTPUT` / `CHART`, `ALIGNED_READS_ONLY`, `PF_READS_ONLY`,
`ASSUME_SORTED`, `STOP_AFTER`, `VALIDATION_STRINGENCY`, `QUIET`, `TMP_DIR`,
and `MAX_RECORDS_IN_RAM`.

Unsupported accumulation levels and hidden chart customizations should be run
through `TURBO_PICARD_FALLBACK_COMMAND`.

## Supported CollectWgsMetrics Surface

Implemented input/output coverage:

- BAM input
- SAM text input
- FASTA reference input
- optional Picard `.interval_list` territory restriction
- Picard-style `WgsMetrics` output and high-quality coverage histogram
- optional base-quality histogram column with `INCLUDE_BQ_HISTOGRAM=true`
- default high-quality depth thresholds
- duplicate, mapping-quality, unpaired-read, base-quality, and capped-base exclusions

Implemented options include `INPUT` / `I`, `OUTPUT` / `O`,
`REFERENCE_SEQUENCE` / `R`, `MINIMUM_MAPPING_QUALITY` / `MQ`,
`MINIMUM_BASE_QUALITY` / `Q`, `COVERAGE_CAP` / `CAP`,
`LOCUS_ACCUMULATION_CAP`,
`COUNT_UNPAIRED`, `INCLUDE_BQ_HISTOGRAM`, `INTERVALS`, `STOP_AFTER`,
`SAMPLE_SIZE=0|1`, `VALIDATION_STRINGENCY`, `QUIET`, `TMP_DIR`, and
`MAX_RECORDS_IN_RAM`.

Unsupported fast algorithm mode, full theoretical sensitivity sampling, overlap
clipping, and non-default accumulation surfaces should be run through
`TURBO_PICARD_FALLBACK_COMMAND`.

## Supported CollectGcBiasMetrics Surface

Implemented input/output coverage:

- SAM/BAM input
- FASTA reference input
- Picard-style detail and summary metric files
- `CHART_OUTPUT` / `CHART` placeholder chart artifact
- all-reads accumulation
- optional duplicate-filtered `READS_USED=UNIQUE` rows with
  `ALSO_IGNORE_DUPLICATES=true`
- primary mapped reads, with secondary and supplementary records ignored

Implemented options include `INPUT` / `I`, `OUTPUT` / `O`,
`SUMMARY_OUTPUT` / `S`, `CHART_OUTPUT` / `CHART`,
`REFERENCE_SEQUENCE` / `R`, `SCAN_WINDOW_SIZE`, `MINIMUM_GENOME_FRACTION`,
`ALSO_IGNORE_DUPLICATES`, `ASSUME_SORTED`, `STOP_AFTER`,
`VALIDATION_STRINGENCY`, and `QUIET`.

Unsupported bisulfite mode, non-`ALL_READS` accumulation levels, and exact R
chart rendering should be run through
`TURBO_PICARD_FALLBACK_COMMAND`. Explicit
`CollectMultipleMetrics PROGRAM=CollectGcBiasMetrics` is supported when
`REFERENCE_SEQUENCE` / `R` is supplied.

## Supported CollectInsertSizeMetrics Surface

Implemented input/output coverage:

- BAM input
- SAM text input
- Picard-style `InsertSizeMetrics` output
- `HISTOGRAM_FILE` / `H` chart artifact
- `METRIC_ACCUMULATION_LEVEL=ALL_READS`
- duplicate records skipped by default, or included with `INCLUDE_DUPLICATES=true`
- secondary, supplementary, unmapped, and mate-unmapped records skipped
- sample, library, and read-group accumulation with
  `METRIC_ACCUMULATION_LEVEL=SAMPLE|LIBRARY|READ_GROUP` when `@RG` sample,
  library, platform-unit metadata and record `RG` tags are present

Implemented options include `INPUT` / `I`, `OUTPUT` / `O`,
`HISTOGRAM_FILE` / `H`, `ASSUME_SORTED`, `DEVIATIONS`, `MINIMUM_PCT` / `M`,
`METRIC_ACCUMULATION_LEVEL=ALL_READS|SAMPLE|LIBRARY|READ_GROUP` /
`LEVEL=ALL_READS|SAMPLE|LIBRARY|READ_GROUP`,
`INCLUDE_DUPLICATES`, `STOP_AFTER`, `VALIDATION_STRINGENCY`, `QUIET`,
`TMP_DIR`, and `MAX_RECORDS_IN_RAM`.

Unsupported accumulation levels should be run through
`TURBO_PICARD_FALLBACK_COMMAND`.

## Supported CollectMultipleMetrics Surface

Native `CollectMultipleMetrics` is an orchestrator for already-native collectors.
When `PROGRAM` is omitted, it runs Picard's default native set:
`CollectAlignmentSummaryMetrics`, `CollectBaseDistributionByCycle`,
`CollectInsertSizeMetrics`, `MeanQualityByCycle`, and
`QualityScoreDistribution`.

Supported programs are `CollectAlignmentSummaryMetrics`,
`CollectBaseDistributionByCycle`, `CollectGcBiasMetrics`,
`CollectInsertSizeMetrics`,
`QualityScoreDistribution`, `MeanQualityByCycle`, `CollectWgsMetrics`, and
`CollectQualityYieldMetrics`. Non-`ALL_READS` accumulation levels are supported
for explicit `PROGRAM=null PROGRAM=CollectAlignmentSummaryMetrics` and
`PROGRAM=null PROGRAM=CollectInsertSizeMetrics` runs and should be run through
`TURBO_PICARD_FALLBACK_COMMAND` for other program selections.
`STOP_AFTER` is
passed through to supported native child collectors. `FILE_EXTENSION` / `EXT`
is appended to metric text outputs using Picard's filename convention, while
chart PDFs keep their standard names. Picard-style
`EXTRA_ARGUMENT=CollectGcBiasMetrics::SCAN_WINDOW_SIZE=...` and
`EXTRA_ARGUMENT=CollectGcBiasMetrics::MINIMUM_GENOME_FRACTION=...` are passed
through for explicit `CollectGcBiasMetrics` runs, as is
`EXTRA_ARGUMENT=CollectGcBiasMetrics::ALSO_IGNORE_DUPLICATES=true`.
`EXTRA_ARGUMENT` also passes `INCLUDE_DUPLICATES`, `DEVIATIONS`, and
`MINIMUM_PCT` to
`CollectInsertSizeMetrics`,
`ALIGNED_READS_ONLY` and `PF_READS_ONLY` to `QualityScoreDistribution` and
`MeanQualityByCycle`, plus `INCLUDE_NO_CALLS` to `QualityScoreDistribution`,
and secondary/supplemental inclusion flags to `CollectQualityYieldMetrics`.

## Supported FixMateInformation Surface

Implemented input/output coverage:

- one SAM/BAM input
- explicit SAM/BAM output
- queryname-sorted input, or `ASSUME_SORTED=true`
- adjacent primary paired records with the same read name
- supplementary records in a primary-pair read group corrected to the primary mate
- optional coordinate-sorted output and BAI creation for BAM output
- mate reference, position, insert size, `MC`, and `MQ` repair
- missing singleton mates passed through with default `IGNORE_MISSING_MATES=true`
- Picard-compatible missing paired mate failure with `IGNORE_MISSING_MATES=false`

Implemented options include `INPUT` / `I`, `OUTPUT` / `O`,
`ADD_MATE_CIGAR` / `MC`, `ASSUME_SORTED`, `SORT_ORDER=queryname|coordinate|unsorted`,
`IGNORE_MISSING_MATES`, `VALIDATION_STRINGENCY`, `QUIET`, `TMP_DIR`,
`MAX_RECORDS_IN_RAM`, `CREATE_MD5_FILE`, and `CREATE_INDEX`.

Unsupported multi-input merge and in-place overwrite should be run through
`TURBO_PICARD_FALLBACK_COMMAND`.

## Supported RevertSam Surface

Implemented input/output coverage:

- one SAM/BAM input
- explicit SAM/BAM output
- default `REMOVE_ALIGNMENT_INFORMATION=true`
- default `REMOVE_DUPLICATE_INFORMATION=true`
- default `RESTORE_ORIGINAL_QUALITIES=true`
- retained alignment information with `REMOVE_ALIGNMENT_INFORMATION=false`
  and `RESTORE_HARDCLIPS=false`
- queryname-sorted, coordinate-header, or unsorted reverted output
- secondary and supplementary input records filtered from reverted output
- negative-strand reads restored to original orientation, including
  `ATTRIBUTE_TO_REVERSE` and `ATTRIBUTE_TO_REVERSE_COMPLEMENT`
- hard-clipped bases and qualities restored from `XB`/`XQ` tags when
  `RESTORE_HARDCLIPS=true`
- Picard-style `.md5` sidecar with `CREATE_MD5_FILE=true`
- Picard-compatible `CREATE_INDEX=true` acceptance without BAI creation for
  reverted queryname output
- Picard-compatible `CREATE_INDEX=true` BAI creation for coordinate BAM output
- clearing default alignment tags `NM`, `UQ`, `PG`, `MD`, `MQ`, `SA`, `MC`, and `AS`
- repeated `ATTRIBUTE_TO_CLEAR` for additional two-character auxiliary tags

Implemented options include `INPUT` / `I`, `OUTPUT` / `O`,
`REMOVE_ALIGNMENT_INFORMATION`, `REMOVE_DUPLICATE_INFORMATION`,
`RESTORE_ORIGINAL_QUALITIES`, `RESTORE_HARDCLIPS`,
`SORT_ORDER=queryname|coordinate|unsorted`,
`ATTRIBUTE_TO_CLEAR`, `ATTRIBUTE_TO_REVERSE`,
`ATTRIBUTE_TO_REVERSE_COMPLEMENT`, `VALIDATION_STRINGENCY`, `QUIET`,
`COMPRESSION_LEVEL`, `CREATE_MD5_FILE`, `CREATE_INDEX`, `TMP_DIR`, and
`MAX_RECORDS_IN_RAM`.

Unsupported read-group split output, sanitize mode, and keep-alignment
hard-clip restoration should be run through
`TURBO_PICARD_FALLBACK_COMMAND`.

## Supported SetNmMdAndUqTags Surface

Implemented input/output coverage:

- coordinate-sorted SAM/BAM input
- SAM/BAM output
- FASTA reference input
- `M`, `=`, `X`, `I`, `D`, `S`, `H`, and `P` CIGAR operations
- Picard-compatible `NM`, `MD`, and `UQ` tags for ordinary DNA alignments

Implemented options include `INPUT` / `I`, `OUTPUT` / `O`,
`REFERENCE_SEQUENCE` / `R`, `IS_BISULFITE_SEQUENCE=false`,
`SET_ONLY_UQ`, `VALIDATION_STRINGENCY`, and `QUIET`.

Unsupported bisulfite mode, reference skips, CRAM-specific behavior, and
non-coordinate inputs should be run through `TURBO_PICARD_FALLBACK_COMMAND`.

## Supported ValidateSamFile Surface

Implemented input/output coverage:

- SAM/BAM input
- Picard-style `MODE=SUMMARY` output to stdout or `OUTPUT` / `O`
- Picard-style `MODE=VERBOSE` detail output for the native validation issue
  types
- Picard-compatible `MAX_OUTPUT` truncation for verbose detail output
- common read-group, sequence-dictionary, and missing-`NM` summary counts
- `IGNORE` filtering for the error types emitted by the native path
- unpaired records, paired records with `SKIP_MATE_VALIDATION=true`, and
  valid adjacent paired records with reciprocal mate coordinates

Implemented options include `INPUT` / `I`, `OUTPUT` / `O`, `MODE` / `M`,
`MAX_OUTPUT` / `MO`, `IGNORE`, `SKIP_MATE_VALIDATION` / `SMV`,
`VALIDATION_STRINGENCY`, and `QUIET`.

Unsupported reference-backed validation and advanced paired mate validation
should be run through `TURBO_PICARD_FALLBACK_COMMAND`.

## Supported LiftoverVcf Surface

Implemented input/output coverage:

- VCF input/output and reject VCF output
- target FASTA with adjacent `.dict`
- positive-strand single-block UCSC chain mappings
- target reference allele validation
- Picard-style `MismatchedRefAllele` and `NoTarget` reject records
- sorted lifted output by target sequence dictionary order

Implemented options include `INPUT` / `I`, `OUTPUT` / `O`, `CHAIN` / `C`,
`REJECT`, `REFERENCE_SEQUENCE` / `R`, `WARN_ON_MISSING_CONTIG` / `WMC`,
`VALIDATION_STRINGENCY`, and `QUIET`.

Unsupported reverse-strand chains, gapped or multi-block chains, swapped-allele
recovery, genotype rewrites, symbolic alleles, and complex annotation
rewriting should be run through `TURBO_PICARD_FALLBACK_COMMAND`.

## Supported IntervalListTools Surface

Implemented input/output coverage:

- one or more `.interval_list` inputs
- `.interval_list` output
- `ACTION=CONCAT`
- dictionary-order sorting with `SORT=true`
- overlapping and abutting interval merging with `UNIQUE=true`
- overlap-only merging with `DONT_MERGE_ABUTTING=true`

Implemented options include `INPUT` / `I`, `OUTPUT` / `O`, `ACTION=CONCAT`,
`SORT`, `UNIQUE`, `PADDING=0`, `DONT_MERGE_ABUTTING=false`,
`VALIDATION_STRINGENCY`, and `QUIET`.

Unsupported VCF inputs, `SECOND_INPUT`, subtract/intersect/symdiff/overlap
actions, inversion, padding, scatter output, count output, and non-abutting
merge actions should be run through `TURBO_PICARD_FALLBACK_COMMAND`.

## Supported NormalizeFasta Surface

Implemented options include `INPUT` / `I`, `OUTPUT` / `O`, `LINE_LENGTH`, and
`TRUNCATE_SEQUENCE_NAMES_AT_WHITESPACE`.

## Supported BedToIntervalList Surface

Implemented input/output coverage:

- BED3, BED4, and BED6 local inputs
- Picard interval_list output
- dictionary-order sorting with `SEQUENCE_DICTIONARY` / `SD`

Implemented options include `SORT`, `UNIQUE`, `VALIDATION_STRINGENCY`, and
`QUIET`.

## Runtime Knobs

- `TURBO_PICARD_THREADS`: worker threads for CPU-heavy MarkDuplicates phases.
- `TURBO_PICARD_FALLBACK_COMMAND`: Picard command prefix used for unsupported
  commands or unsupported native `MarkDuplicates` surfaces.
- `COMPRESSION_LEVEL`: Picard-style output compression level, from `0` to `9`.

## Correctness Checks

```bash
cargo test --workspace
python3 -m unittest tools/test_compare_markduplicates.py
./tools/verify_basic_picard_parity.sh
./tools/verify_basic_sortsam_parity.sh
./tools/verify_basic_cleansam_parity.sh
./tools/verify_basic_mergesamfiles_parity.sh
./tools/verify_basic_buildbamindex_parity.sh
./tools/verify_basic_samtofastq_parity.sh
./tools/verify_basic_fastqtosam_parity.sh
./tools/verify_basic_addorreplacereadgroups_parity.sh
./tools/verify_basic_alignmentmetrics_parity.sh
./tools/verify_basic_collectbasedistributionbycycle_parity.sh
./tools/verify_basic_collectgcbiasmetrics_parity.sh
./tools/verify_basic_qualityyield_parity.sh
./tools/verify_basic_collectwgsmetrics_parity.sh
./tools/verify_basic_createdict_parity.sh
./tools/verify_basic_viewsam_parity.sh
./tools/verify_basic_replacesamheader_parity.sh
./tools/verify_basic_updatevcfsequencedictionary_parity.sh
./tools/verify_basic_gathervcfs_parity.sh
./tools/verify_basic_sortvcf_parity.sh
./tools/verify_basic_mergevcfs_parity.sh
./tools/verify_basic_qualityscoredistribution_parity.sh
./tools/verify_basic_meanqualitybycycle_parity.sh
./tools/verify_basic_collectinsertsizemetrics_parity.sh
./tools/verify_basic_collectmultiplemetrics_parity.sh
./tools/verify_basic_fixmateinformation_parity.sh
./tools/verify_basic_intervallisttools_parity.sh
./tools/verify_basic_revertsam_parity.sh
./tools/verify_basic_setnmmdanduqtags_parity.sh
./tools/verify_basic_validatesamfile_parity.sh
./tools/verify_basic_liftovervcf_parity.sh
```

The parity scripts compare native `turbo-picard` output against a Picard
installation from the local conda environment when available.

Performance evidence scripts include parity checks in their output:

```bash
./tools/bench_addorreplacereadgroups.py --reads 100000
./tools/bench_alignmentmetrics.py --reads 100000
./tools/bench_bedtointervallist.py --reads 100000
./tools/bench_buildbamindex.py --reads 50000
./tools/bench_cleansam.py --reads 50000
./tools/bench_collectbasedistributionbycycle.py --reads 100000
./tools/bench_collectwgsmetrics.py --reads 100000
./tools/bench_createdict.py --reads 10000
./tools/bench_fastqtosam.py --reads 100000
./tools/bench_fixmateinformation.py --reads 100000
./tools/bench_gathervcfs.py --reads 100000
./tools/bench_insertsize.py --reads 500000
./tools/bench_markduplicates_synthetic.py --reads 50000
./tools/bench_meanqualitybycycle.py --reads 100000
./tools/bench_mergevcfs.py --reads 100000
./tools/bench_mergesamfiles.py --reads 50000 --shards 4
./tools/bench_normalizefasta.py --reads 10000
./tools/bench_qualityscoredistribution.py --reads 100000
./tools/bench_qualityyield.py --reads 100000
./tools/bench_replacesamheader.py --reads 50000
./tools/bench_revertsam.py --reads 100000
./tools/bench_samtofastq.py --reads 50000
./tools/bench_setnmmdanduqtags.py --reads 100000
./tools/bench_sortvcf.py --reads 100000
./tools/bench_sortsam.py --reads 100000
./tools/bench_updatevcfsequencedictionary.py --reads 100000
./tools/bench_validatesamfile.py --reads 100000
./tools/bench_viewsam.py --reads 50000
./tools/bench_suite.py --repeats 5
```

Latest local suite snapshot, used by the README graph and marketing page:

- `28/28` benchmarked commands passed parity checks.
- `104.68x` top speedup: `UpdateVcfSequenceDictionary`.
- `6.69x` floor speedup: `RevertSam`.
- `26.24x` median speedup.
- `28.41x` geometric mean speedup.

| Command | Speedup vs Picard | Parity |
| --- | ---: | --- |
| UpdateVcfSequenceDictionary | 104.68x | PASS |
| CollectInsertSizeMetrics | 84.96x | PASS |
| NormalizeFasta | 83.49x | PASS |
| BuildBamIndex | 60.15x | PASS |
| GatherVcfs | 57.84x | PASS |
| CreateSequenceDictionary | 53.50x | PASS |
| MergeVcfs | 49.67x | PASS |
| SamToFastq | 48.34x | PASS |
| MeanQualityByCycle | 38.72x | PASS |
| CollectAlignmentSummaryMetrics | 38.41x | PASS |
| AddOrReplaceReadGroups | 30.68x | PASS |
| CleanSam | 30.15x | PASS |
| FastqToSam | 26.51x | PASS |
| SortVcf | 26.24x | PASS |
| CollectBaseDistributionByCycle | 23.85x | PASS |
| ViewSam | 22.40x | PASS |
| MergeSamFiles | 22.27x | PASS |
| SortSam | 21.60x | PASS |
| ValidateSamFile | 20.39x | PASS |
| CollectQualityYieldMetrics | 20.17x | PASS |
| BedToIntervalList | 19.79x | PASS |
| CollectWgsMetrics | 19.73x | PASS |
| MarkDuplicates | 19.30x | PASS |
| ReplaceSamHeader | 18.71x | PASS |
| QualityScoreDistribution | 17.62x | PASS |
| FixMateInformation | 10.03x | PASS |
| SetNmMdAndUqTags | 8.88x | PASS |
| RevertSam | 6.69x | PASS |

Regenerate the graph and website data after a fresh suite run with:

```bash
printf 'benchmark_date=%s source=python3 tools/bench_suite.py --repeats 1 --skip-build\n' "$(date +%F)" > docs/site/assets/bench-suite-output.txt
python3 tools/bench_suite.py --repeats 1 --skip-build | tee -a docs/site/assets/bench-suite-output.txt
python3 tools/render_benchmark_assets.py --suite-output docs/site/assets/bench-suite-output.txt
```

## Packaging

Local package smoke test:

```bash
./tools/verify_package_install.sh
```

Bioconda-oriented assets live in `packaging/bioconda/turbo-picard/` and
`packaging/bioconda/turbo-picard-picard-shim/`. The main package is
non-shadowing; the shim package owns the optional `picard` command. The recipes
currently use the local checkout as their source so they can be tested before a
release tag exists. Before submitting to Bioconda, replace `source.path` with a
tagged release URL and `sha256`, and replace the maintainer placeholder.

## Current Limits

`turbo-picard` is not a full Picard suite yet. The shipped native commands are
`MarkDuplicates`, `SortSam`, `CleanSam`, `MergeSamFiles`, `BuildBamIndex`, `SamToFastq`, `FastqToSam`,
`AddOrReplaceReadGroups`, `CollectAlignmentSummaryMetrics`,
`CollectQualityYieldMetrics`, `CreateSequenceDictionary`, `NormalizeFasta`, and
`BedToIntervalList`, with partial native `ViewSam`, `ReplaceSamHeader`, and
`QualityScoreDistribution`, `MeanQualityByCycle`,
`CollectBaseDistributionByCycle`, `CollectGcBiasMetrics`,
`CollectInsertSizeMetrics`, `CollectMultipleMetrics`, `CollectWgsMetrics`, `FixMateInformation`,
`UpdateVcfSequenceDictionary`, `IntervalListTools`, `RevertSam`,
`SetNmMdAndUqTags`, `ValidateSamFile`, `LiftoverVcf`, `GatherVcfs`, `SortVcf`,
and `MergeVcfs`.
Outputs are intended to be semantically compatible rather than byte-for-byte
identical to Picard. Set
`TURBO_PICARD_FALLBACK_COMMAND` for drop-in environments that need unsupported
Picard tools to continue working.
