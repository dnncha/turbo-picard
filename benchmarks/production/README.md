# Production-scale benchmark evidence

This directory is for workflow-sized evidence. It is separate from the fast
synthetic benchmark suite.

A release-evidence submission needs:

- manifest-entry.json following manifest-entry.example.json;
- raw command logs;
- Picard and Turbo-Picard versions and commits;
- immutable input source and local SHA-256;
- host, CPU, RAM, storage, container/conda image and thread settings;
- at least five timed repeats with median and p95;
- peak RSS and temporary-disk measurements;
- parity result and comparator method;
- a scope caveat describing what the dataset does and does not represent.

Recommended profiles:

- wgs_30x: coordinate-sorted 30x human WGS;
- wes_capture: bait/target metrics and duplicate marking;
- rna_seq: spliced alignments and alignment metrics;
- umi_panel: barcode/UMI duplicate-marking behaviour;
- cram_reference: reference-backed CRAM;
- multi_library: multiple lanes, read groups and libraries;
- cohort_batch: at least 25 samples.

Pass the profile name to the competitor runner so the evidence bundle records
which workflow shape was measured. The profile is descriptive unless it has a
specific input contract: `umi_panel` requires a barcode tag and
`cram_reference` requires a CRAM input plus an explicit reference.
For `production_scale` and `independent_reproduction` manifests, the profile
is mandatory and the measured input must have positive byte and read counts;
the validators reject empty or unscoped evidence before it can be promoted.

Run quick regression evidence with:

    python3 tools/bench_suite.py --repeats 5 --skip-build \
      --profile-output benchmarks/production/synthetic-profile.json

For a production audit, use tools/audit_real_data.py, retain the generated
bundle beside the manifest, then run:

    python3 tools/verify_real_data_evidence.py --release-ready
    python3 tools/validate_production_manifest.py manifest-entry.json

The repository also has a manual GitHub Actions runner for a hash-pinned BAM.
Open **Actions -> Production evidence validators -> Run workflow**, provide the
immutable HTTPS URL, its SHA-256, the source revision, and the exact tool set.
The run builds the checked-out commit, measures at least five repeats, records
versions/resource backends/host memory, creates `manifest-entry.json`, and
uploads the raw bundle. A `production_scale` tier is still evidence only until
the same protocol is independently reproduced and reviewed; the workflow does
not mark that gate as passed automatically.

The dispatch form also exposes the runner's evidence controls: choose a
workflow profile, set `READ_NAME_REGEX` for no-optical or optical-heavy runs,
request paired `DS`/`DI` tags, and provide primary or mate-specific barcode tags
for UMI-panel evidence. The selected profile and options are retained in the
raw report and manifest; `umi_panel` and `cram_reference` fail closed when their
required inputs are absent.

The dispatch form runs the same local contract as
`tools/validate_production_dispatch.py` before downloading the input. It
requires both `turbo-picard` and Picard in the selected and required tool sets,
rejects mismatched CRAM/reference or UMI/barcode settings, and requires at
least five measured repeats. Its focused tests run in the workflow validation
job, so a future dispatch-input change is reviewed before a large evidence run.
The same validation job also runs
`tools/verify_production_evidence_workflow.py`, which checks that the shared
validator remains before input download/build and that manual measurement still
depends on the validation job.

Independent reproduction is a separate evidence contract, not a reviewer-name
checkbox. A manifest may use `status=pass` only when it retains an evidence URL,
reviewer, independent host profile, and matching SHA-256 values for the
Turbo-Picard commit, input, and command protocol. Build those fields explicitly
when a second-machine review is complete:

    python3 tools/build_production_manifest.py \
      --report report.json \
      --output manifest-entry.json \
      --dataset-id HG002-markduplicates \
      --scope-caveat 'Pinned coordinate-sorted input; MarkDuplicates only.' \
      --turbo-picard-commit "$(git rev-parse HEAD)" \
      --read-count 123456789 \
      --tier production_scale \
      --independent-status pass \
      --reviewer 'Independent reviewer or team' \
      --independent-host-profile 'second-linux-x86_64-machine' \
      --independent-turbo-picard-commit "$(git rev-parse HEAD)" \
      --independent-input-sha256 '<same-64-character-input-sha256>' \
      --independent-arguments-sha256 '<same-64-character-protocol-sha256>' \
      --evidence-url 'https://example.org/retained-independent-bundle'

`tools/validate_production_manifest.py --release-ready` rejects missing,
mismatched, or non-retained independent evidence. The placeholders above are
instructions, not evidence; replace them only with values from the exact raw
bundle.

For local or self-hosted runs, the equivalent adapter is:

    python3 tools/bench_markduplicates_competitors.py \
      --input /data/coordinate-sorted.bam \
      --output-dir benchmarks/production/HG002-markduplicates \
      --tools turbo-picard,picard \
      --require-tools turbo-picard,picard \
      --repeats 5 \
      --profile wgs_30x \
      --source-url 'https://example.org/immutable/HG002.bam' \
      --source-revision 'accession-or-release'

The runner defaults to `READ_NAME_REGEX=null` for the bounded no-optical plan on
single BAMs and explicit-reference CRAMs. For optical-heavy evidence, add
`--read-name-regex default` to use each tool's default optical-family behavior,
or provide the exact pinned Picard regex. The chosen setting is retained in the
report protocol. Add `--tag-duplicate-set-members` when the workflow requires
paired duplicate-set `DS`/`DI` tags; the flag is passed to the Picard-compatible
presets and retained in the protocol.

For a barcode/UMI panel, pass the exact tag fields used by the workflow. The
runner forwards them to the Picard-compatible presets and retains them in the
protocol; it does not normalize or invent barcode values:

    python3 tools/bench_markduplicates_competitors.py \
      --input /data/umi-panel.coordinate.bam \
      --output-dir benchmarks/production/umi-panel-markduplicates \
      --tools turbo-picard,picard \
      --require-tools turbo-picard,picard \
      --profile umi_panel \
      --barcode-tag RX \
      --repeats 5

For CRAM, pass `--reference-fasta /refs/GRCh38.fa`; the runner records the
reference SHA-256 and the production manifest requires it. The manual workflow
also accepts `input_format=CRAM`, `reference_url`, and `reference_sha256`.

The smaller real-data trial wrapper accepts the same bounded barcode options
with repeated `--markduplicates-arg KEY=VALUE` flags. This is useful when a
workflow owner wants semantic parity evidence before running the five-repeat
competitor benchmark. Advanced UMI normalization remains outside this mode.

    python3 tools/build_production_manifest.py \
      --report benchmarks/production/HG002-markduplicates/report.json \
      --output benchmarks/production/HG002-markduplicates/manifest-entry.json \
      --dataset-id HG002-markduplicates \
      --scope-caveat 'Coordinate-sorted HG002 input; MarkDuplicates only.' \
      --turbo-picard-commit "$(git rev-parse HEAD)" \
      --read-count 123456789 \
      --tier production_scale

Replace the example read count with the value measured from the exact input;
do not hand-edit timings, hashes, parity, or version fields.

If Turbo-Picard loses a profile, publish the loss and the next bottleneck.
Negative results are useful compatibility and product evidence.

For production-scale MarkDuplicates comparisons against Picard, samtools and
FastDup, use the auditable runner documented in
`benchmarks/markduplicates-competitors/README.md`. It preserves failed and
unavailable competitors, raw resource logs, executable hashes and streaming
parity evidence instead of emitting an unsupported headline speedup.
