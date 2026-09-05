# One-command trial

Use this when you want the fastest honest answer about whether `turbo-picard`
belongs in your workflow.

Best first commands:

- `MarkDuplicates` for broad preprocessing pipelines
- `SortSam` for repeated BAM or CRAM reshaping
- `SamToFastq` for export-heavy alignment or remap paths, including per-read-group FASTQ output
- `FastqToSam` for lane-sharded FASTQ ingestion before alignment or archival handoff
- `FixMateInformation` for mate repair paths that already provide queryname-sorted input
- `BuildBamIndex` for a very small, low-risk first substitution

Recommended flow:

1. Pick one representative shard, not your smallest toy file.
2. Run the upstream Picard step and the `turbo-picard` step on that same shard.
3. Compare the exact files the downstream workflow consumes.
4. Keep the outputs, metrics, timings, and command lines together.
5. Only widen the rollout after that command is stable on your own data.

PyPI trial:

```bash
python3 -m venv .venv-turbo-picard-trial
. .venv-turbo-picard-trial/bin/activate
python3 -m pip install --upgrade pip
python3 -m pip install turbo-picard
turbo-picard --version
turbo-picard doctor
```

Then run the same Picard-shaped command twice on one representative shard. For a
`MarkDuplicates` trial:

```bash
export INPUT_BAM=/path/to/representative.bam
export PICARD_JAR=/path/to/picard.jar
mkdir -p turbo-picard-trial/picard turbo-picard-trial/turbo

/usr/bin/time -p java -jar "$PICARD_JAR" MarkDuplicates \
  I="$INPUT_BAM" \
  O=turbo-picard-trial/picard/marked.bam \
  M=turbo-picard-trial/picard/metrics.txt

# Legacy isolation also works with 0.1.12; REQUIRE_NATIVE adds an explicit
# fail-closed policy in the next-release source. Preserve upstream separately.
env -u TURBO_PICARD_FALLBACK_COMMAND \
  TURBO_PICARD_DISABLE_AUTO_FALLBACK=1 TURBO_PICARD_REQUIRE_NATIVE=1 \
  /usr/bin/time -p turbo-picard MarkDuplicates \
  I="$INPUT_BAM" \
  O=turbo-picard-trial/turbo/marked.bam \
  M=turbo-picard-trial/turbo/metrics.txt
```

Minimum checks before sharing the result:

```bash
samtools quickcheck -v \
  turbo-picard-trial/picard/marked.bam \
  turbo-picard-trial/turbo/marked.bam
diff -u turbo-picard-trial/picard/metrics.txt turbo-picard-trial/turbo/metrics.txt
```

If you are in a `turbo-picard` checkout and want a reviewable bundle, use the
repo comparison helper instead of hand-collecting outputs:

```bash
python3 tools/compare_real_data.py \
  --input-bam "$INPUT_BAM" \
  --output-dir turbo-picard-trial/evidence \
  --commands MarkDuplicates \
  --picard-command "java -jar $PICARD_JAR" \
  --turbo-picard-command turbo-picard \
  --shareable-report turbo-picard-trial/evidence/shareable-trial-report.md \
  --skip-build
```

The comparison helper uses the configured Picard-compatible `ViewSam`
entrypoints to inspect BAM/CRAM outputs, so it does not require a separate
`samtools` executable on `PATH`. `samtools quickcheck` remains a useful
optional manual integrity check for the generated files.

The optional `--shareable-report` output is a deliberately lossy summary for
the public trial-report issue form. It omits local paths, input hashes,
command arguments, generated artifact names, and raw data. Review it before
posting, and do not attach the full `work/` directory or the raw comparison
JSON when the input is private. Add `--include-public-source` only when the
source URL and revision are genuinely public.

If you add `--dataset-id` to create a repository-ready `manifest-entry.json`,
also pass `--input-source-url` and `--input-source-commit`, and put the output
under `benchmarks/real-data/<dataset-id>/evidence/`. The comparator checks this
layout and citation before starting the comparison, so a malformed release
candidate request fails immediately instead of consuming a full real-data run.

Use the generated `shareable-trial-report.md` as the starting point for the
[trial report issue](https://github.com/dnncha/turbo-picard/issues/new?template=trial-report.yml).
If GitHub does not offer new-issue creation, paste the reviewed report as a
comment on the [public trial report thread](https://github.com/dnncha/turbo-picard/issues/4).
It is still a report to review, not an automatic publication step.

Barcode/UMI panel trial:

If the workflow already stores molecular barcodes in SAM tags, pass the exact
fields to both sides of the comparison. The comparator records these options
in the private JSON evidence while the shareable report remains redacted:

```bash
python3 tools/compare_real_data.py \
  --input-bam "$INPUT_BAM" \
  --output-dir turbo-picard-trial/evidence \
  --commands MarkDuplicates \
  --markduplicates-arg BARCODE_TAG=RX \
  --markduplicates-arg TAG_DUPLICATE_SET_MEMBERS=true \
  --picard-command "java -jar $PICARD_JAR" \
  --turbo-picard-command turbo-picard \
  --shareable-report turbo-picard-trial/evidence/shareable-trial-report.md \
  --skip-build
```

Use `READ_ONE_BARCODE_TAG=BX` and `READ_TWO_BARCODE_TAG=BY` instead when the
workflow keeps mate-specific barcode fields. `UmiAwareMarkDuplicatesWithMateCigar`
and other advanced UMI normalization modes remain upstream-fallback commands;
do not replace them with the standard `MarkDuplicates` trial unless the
workflow's exact semantics are barcode grouping only.

Files in this directory:

- `trial.wdl`: tiny `WDL` workflow for a single `MarkDuplicates` trial
- `trial.nf`: tiny `Nextflow` workflow for the same trial shape
- `trial-samtofastq.nf`: tiny `Nextflow` workflow for a `SamToFastq` trial, including optional per-read-group output
- `trial-samtofastq.wdl`: tiny `WDL` workflow for a `SamToFastq` trial, including optional per-read-group output
- `trial-fastqtosam.nf`: tiny `Nextflow` workflow for a paired `FastqToSam` trial
- `trial-fastqtosam.wdl`: tiny `WDL` workflow for a paired `FastqToSam` trial
- `trial-fixmateinformation.wdl`: tiny `WDL` workflow for a `FixMateInformation` trial
- `trial-fixmateinformation.nf`: tiny `Nextflow` workflow for a `FixMateInformation` trial
- `trial-config.yaml`: small config stub showing the expected inputs

Useful toggles in `trial-config.yaml`:

- `output_per_rg` and `rg_tag` for per-read-group `SamToFastq` trials
- `use_sequential_fastqs` for lane-sharded `FastqToSam` trials

If you need a reviewable evidence bundle instead of a quick local trial, use
`tools/audit_real_data.py` or `tools/compare_real_data.py`.
