Trying turbo-picard in a pipeline
=================================

The safest way to use ``turbo-picard`` is to change one command at a time.
Run it beside Picard first, compare the outputs you care about, and keep
upstream Picard available for anything that has not been checked yet.

Practical path
--------------

1. Start beside Picard
~~~~~~~~~~~~~~~~~~~~~~

Start with the explicit binary:

.. code-block:: bash

   turbo-picard MarkDuplicates I=input.bam O=turbo.bam M=turbo.metrics.txt

Keep upstream Picard as the production path while you compare:

* BAM, SAM, FASTQ, VCF, interval-list, and metrics outputs.
* Sidecar files such as indexes and md5 files.
* Exit codes and error messages for bad inputs.
* Runtime and memory behavior on realistic shards.

2. Check the commands you need
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

Run the command matrix check and the relevant parity scripts:

.. code-block:: bash

   python3 tools/verify_command_matrix.py
   ./tools/verify_basic_picard_parity.sh
   ./tools/verify_basic_sortsam_parity.sh
   ./tools/verify_basic_samtofastq_parity.sh

The repository keeps one parity script per documented command. The goal
is not to prove all of Picard at once. The goal is to prove the Picard behavior
your workflows actually depend on. The comparison boundary is described in
:doc:`parity`.

3. Configure fallback for mixed coverage
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

Use fallback when a workflow calls both accelerated and unsupported Picard
commands:

.. code-block:: bash

   export TURBO_PICARD_FALLBACK_COMMAND='java -jar /opt/picard/picard.jar'

Native commands still run natively. Unsupported commands delegate to upstream
Picard. Details are in :doc:`fallback`.

4. Switch only the checked commands
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

Only after the relevant parity and benchmark evidence is acceptable, put the
``picard`` shim ahead of upstream Picard on ``PATH`` for that workflow or
environment.

5. Keep evidence in CI
~~~~~~~~~~~~~~~~~~~~~~

Run the command matrix check, targeted parity scripts, and benchmark suite for
the commands you depend on:

.. code-block:: bash

   python3 tools/bench_suite.py --repeats 1 --skip-build
   python3 tools/verify_benchmark_log_evidence.py

This keeps upgrades boring: coverage changes are visible, and performance
claims stay tied to measured output.

Representative-data validation protocol
---------------------------------------

Before replacing Picard in a research pipeline, run at least one representative
BAM through the real-data comparator. Use data that looks like the workflow you
plan to switch: same assay, aligner, read groups, duplicate-marking policy,
sort order, and common edge cases such as soft clips, orphaned mates, secondary
or supplementary records, UMIs, or mitochondrial reads when those matter.

For public data, cite an immutable source URL and commit or accession. For
GitHub fixtures, use a URL containing ``/blob/<commit>/`` and the full
40-character SHA. The full 40-character Git commit SHA must be visible in the
evidence, not a branch name or short hash. For accession-hosted data, put the
accession or release identifier in both the URL and ``--input-source-commit``.
For private production data, keep the evidence bundle with the private
dataset ID, input SHA-256, and the exact caveat describing what the shard
represents.

.. code-block:: bash

   cargo build --release -p turbo-picard-cli --bin picard

   python3 tools/compare_real_data.py \
     --input-bam /data/representative.bam \
     --input-source-url https://example.org/pinned/source-or-accession.bam \
     --input-source-commit example-release-or-commit \
     --output-dir benchmarks/real-data/my-workflow-representative/evidence \
     --dataset-id my-workflow-representative \
     --scope-caveat "representative shard for workflow X; not a full cohort" \
     --release-tier release_candidate \
     --commands AddOrReplaceReadGroups BuildBamIndex CleanSam CollectAlignmentSummaryMetrics CollectInsertSizeMetrics CollectQualityYieldMetrics MarkDuplicates RevertSam SamToFastq SortSam ValidateSamFile ViewSam \
     --picard-command "mamba run -p /opt/conda/envs/picard picard" \
     --turbo-picard-command ./target/release/picard \
     --skip-build

If all commands pass, review the generated Markdown and JSON reports, then add
the manifest entry:

.. code-block:: bash

   python3 tools/update_real_data_manifest.py \
     --entry benchmarks/real-data/my-workflow-representative/evidence/manifest-entry.json
   python3 tools/verify_real_data_evidence.py --release-ready

Treat a failure as useful information, not a paperwork problem. Keep upstream
Picard on that command until the mismatch is understood, fixed, and
covered by a regression test or a pinned real-data comparison. Do not make
claims about every lab dataset from the checked-in NA12878 mitochondrial test
BAM alone; it is packaging evidence and a real Picard edge case, not proof of
every workflow. The release-ready check must cover
AddOrReplaceReadGroups, BuildBamIndex, CleanSam,
CollectAlignmentSummaryMetrics, CollectInsertSizeMetrics,
CollectQualityYieldMetrics, MarkDuplicates, RevertSam, SamToFastq, SortSam,
ValidateSamFile, ViewSam somewhere in pinned release evidence. It also requires
enough input data that the check cannot pass on one tiny fixture alone.
