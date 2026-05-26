# turbo-picard

`turbo-picard` is a Picard-shaped Rust toolkit focused on high-impact, drop-in
replacement workflows. The current native engines target `MarkDuplicates`,
`SortSam`, and `SamToFastq`, and install two command-line entrypoints:

- `turbo-picard`
- `picard`

The `picard` binary is a compatibility shim for workflow managers and scripts
that invoke Picard by command name.

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

Native `SamToFastq` streams SAM/BAM records to FASTQ:

```bash
picard SamToFastq I=input.bam FASTQ=reads.fastq
picard SamToFastq I=input.bam FASTQ=r1.fastq SECOND_END_FASTQ=r2.fastq
```

By default, unsupported Picard commands fail clearly. For drop-in deployments
that need the rest of Picard to keep working, set a fallback command:

```bash
export TURBO_PICARD_FALLBACK_COMMAND='mamba run -p /opt/conda/envs/picard picard'
picard SortSam I=input.bam O=queryname.bam SORT_ORDER=queryname
```

When configured, `turbo-picard` runs native accelerated commands first. If a
command is unsupported, or the requested `MarkDuplicates` surface is outside the
native implementation, it delegates the original Picard arguments to the
fallback command and returns the fallback exit code. The fallback value is a
shell command prefix, so `java -jar /path/to/picard.jar` works too.

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

## Supported SamToFastq Surface

Implemented input/output coverage:

- BAM input
- SAM text input
- single-end FASTQ output
- paired FASTQ output with `SECOND_END_FASTQ`
- unpaired read routing with `UNPAIRED_FASTQ`
- interleaved paired output with `INTERLEAVE=true`

Implemented options include:

- `FASTQ`
- `SECOND_END_FASTQ`
- `UNPAIRED_FASTQ`
- `INTERLEAVE`
- `RE_REVERSE`
- `VALIDATION_STRINGENCY`
- `QUIET`
- `COMPRESSION_LEVEL`

Accepted compatibility options that are validated or ignored when they do not
change the current native implementation:

- `VERBOSITY`

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
./tools/verify_basic_samtofastq_parity.sh
```

The parity scripts compare native `turbo-picard` output against a Picard
installation from the local conda environment when available.

## Packaging

Local package smoke test:

```bash
./tools/verify_package_install.sh
```

Bioconda-oriented assets live in `packaging/bioconda/turbo-picard/`. The recipe
currently uses the local checkout as its source so it can be tested before a
release tag exists. Before submitting to Bioconda, replace `source.path` with a
tagged release URL and `sha256`, and replace the maintainer placeholder.

## Current Limits

`turbo-picard` is not a full Picard suite yet. The shipped native commands are
`MarkDuplicates`, `SortSam`, and `SamToFastq`, and outputs are intended to be
semantically compatible rather than byte-for-byte identical to Picard. Set
`TURBO_PICARD_FALLBACK_COMMAND` for drop-in environments that need unsupported
Picard tools to continue working.
