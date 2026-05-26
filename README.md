# turbo-picard

`turbo-picard` is a Picard-shaped Rust toolkit focused on high-impact, drop-in
replacement workflows. The current native engine targets `MarkDuplicates` and
installs two command-line entrypoints:

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

Unsupported Picard commands fail clearly instead of silently delegating to a
different implementation.

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

## Runtime Knobs

- `TURBO_PICARD_THREADS`: worker threads for CPU-heavy MarkDuplicates phases.
- `COMPRESSION_LEVEL`: Picard-style output compression level, from `0` to `9`.

## Correctness Checks

```bash
cargo test --workspace
python3 -m unittest tools/test_compare_markduplicates.py
./tools/verify_basic_picard_parity.sh
```

`verify_basic_picard_parity.sh` compares `turbo-picard MarkDuplicates` against a
Picard installation from the local conda environment when available.

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

`turbo-picard` is not a full Picard suite yet. The shipped native command is
`MarkDuplicates`, and outputs are intended to be semantically compatible rather
than byte-for-byte identical to Picard. The current BAM engine still keeps the
record set in memory for large duplicate-marking runs; a streaming
coordinate-window engine is the next major scaling step.
