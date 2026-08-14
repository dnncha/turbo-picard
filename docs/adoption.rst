Trying turbo-picard in a pipeline
=================================

The safest way to use ``turbo-picard`` is to change one command at a time.
Pick a Picard step that is actually slowing you down, run it beside Picard
first, compare the outputs you care about, and keep upstream Picard available
for anything that has not been checked yet.

This pattern fits the way people already use Picard in WDL, Cromwell,
Nextflow, nf-core, Snakemake, and shell pipelines: keep the task or process
shape stable, prove one command, then widen the rollout only after the evidence
is good enough for that workflow.

Good first candidates are usually ``MarkDuplicates``, ``SortSam``,
``SamToFastq``, ``FastqToSam``, ``FixMateInformation``, ``BuildBamIndex``,
and the metrics commands that repeatedly slow down development or
preprocessing runs.

Workflow shapes
---------------

The easiest adoption pattern is to keep the workflow boundary stable and change
only the command inside it.

Barcode and UMI-tagged MarkDuplicates
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

If the workflow already stores molecular barcodes in SAM tags, the real-data
comparator can audit the exact bounded barcode-grouping mode rather than
silently comparing a default no-barcode run:

.. code-block:: bash

   python3 tools/compare_real_data.py \
     --input-bam /data/umi-panel.coordinate.bam \
     --output-dir benchmarks/real-data/umi-panel/evidence \
     --commands MarkDuplicates \
     --markduplicates-arg BARCODE_TAG=RX \
     --markduplicates-arg TAG_DUPLICATE_SET_MEMBERS=true \
     --picard-command "mamba run -p /opt/conda/envs/picard picard" \
     --turbo-picard-command ./target/release/picard \
     --skip-build

Use ``READ_ONE_BARCODE_TAG=BX`` and ``READ_TWO_BARCODE_TAG=BY`` for mate-specific
fields. The exact options are retained in the private JSON comparison evidence;
the optional shareable report omits command arguments and local data. Advanced
``UmiAwareMarkDuplicatesWithMateCigar`` normalization remains an upstream
fallback until a separate parity-tested native implementation exists.

WDL / Cromwell
~~~~~~~~~~~~~~

Keep the task inputs and outputs the same, and swap only the executable:

.. code-block:: text

   task MarkDuplicatesTurbo {
     input {
       File input_bam
       String sample_id
     }

     command <<<
       turbo-picard MarkDuplicates \
         I=~{input_bam} \
         O=~{sample_id}.marked.bam \
         M=~{sample_id}.metrics.txt \
         ASSUME_SORTED=true
     >>>
   }

From a repository checkout, install ``miniwdl`` and run the WDL starter smoke
to catch syntax and type drift. If ``TURBO_PICARD_WDL_IMAGE`` names a Docker
image containing ``turbo-picard``, the same command also executes the checked-in
``MarkDuplicates`` trial and validates its BAM and metrics outputs:

.. code-block:: bash

   python3 -m pip install "miniwdl==1.15.0"
   bash tools/verify_wdl_starters.sh

Nextflow / nf-core
~~~~~~~~~~~~~~~~~~

Pick the command at runtime so you can compare a module with and without
``turbo-picard``:

.. code-block:: text

   def picard = params.use_turbo_picard ? 'turbo-picard' : 'picard'
   """
   ${picard} MarkDuplicates \
       I=${bam} \
       O=${meta.id}.marked.bam \
       M=${meta.id}.metrics.txt \
       ASSUME_SORTED=true
   """

The repository also keeps a more detailed nf-core note in
``packaging/nf-core/README.md`` and starter workflow files in
``packaging/workflows/``.

Measure the adoption funnel
~~~~~~~~~~~~~~~~~~~~~~~~~~~

Maintainers can capture a repeatable, read-only public baseline after a
release:

.. code-block:: bash

   python3 tools/audit_public_adoption.py --pretty \
     --output turbo-picard-public-adoption.json

The report combines the live PyPI version and README freshness, PyPIStats
downloads without mirrors, GitHub repository interest signals, open issue
counts, published GitHub releases, public GHCR image tags, and Bioconda package
and pull-request state. It also records the source version, current commit,
worktree cleanliness, and whether the matching local and ``origin`` release tag
points at that commit. This makes a post-release source/package mismatch and a
channel that is still serving an older version visible without turning download
counts into user telemetry. It records the source URLs and explicitly marks
sustained external usage, customer demand, and production readiness as
unverified. Download, repository, release, container, and package counters are
distribution and interest signals, not user telemetry or a substitute for
reviewed workflow-owner trials.
The community section also records the public trial-report thread's comment
count and the observed author provenance for open issues and trial comments.
Maintainer, external, and unknown authors are counted separately without
retaining public usernames. It deliberately does not count comments as
workflow-owner trial reports; ``workflow_owner_trial_reports_verified`` stays
false until a human reviews the reports and confirms their workflow context.
An empty external-author count is only an observation about the sampled public
API response, not evidence that no private or unreported trials exist.
The distribution section is also read-only: a missing workspace-version tag,
release, or Bioconda package is recorded as a channel-state gap, not silently
treated as a publication failure or repaired by the audit.
The repository also has a quiet weekly and manual
``.github/workflows/public-adoption-audit.yml`` job that stores the JSON report
as a short-lived GitHub Actions artifact; it has read-only repository
permissions and performs no publication or outreach.

Snakemake
~~~~~~~~~

For Snakemake, the usual pattern is a straight command swap inside an existing
rule:

.. code-block:: python

   rule build_bam_index:
       input:
           bam="results/{sample}.bam"
       output:
           bai="results/{sample}.bam.bai"
       shell:
           "turbo-picard BuildBamIndex I={input.bam} O={output.bai}"

If you want concrete files to start from, see ``packaging/workflows/`` for a
minimal ``WDL`` tasks, small ``Nextflow`` processes, and a starter
``Snakemake`` rule set covering ``BuildBamIndex``, ``SortSam``,
``MarkDuplicates``, ``SamToFastq``, ``FastqToSam``, and
``FixMateInformation``.
That directory also includes short walkthroughs for ``WDL / Cromwell``,
``Nextflow / nf-core``, and ``Snakemake`` so a workflow owner can choose a
starter path quickly.
If the right first substitution is still unclear, start with
``choose-your-first-command.md`` in that same directory.
If a Python 3.9 environment resolves Snakemake 7.32.4, pin ``PuLP==2.7.0``
for the trial environment. That Snakemake release calls the legacy
``pulp.list_solvers`` API, which is not available in PuLP 3.x. Use the newest
compatible pair available to the workflow environment when possible; this
pin keeps the documented Snakemake 7.32.4 starter reproducible.

.. code-block:: bash

   python3 -m venv .venv-snakemake-trial
   . .venv-snakemake-trial/bin/activate
   python3 -m pip install "snakemake==7.32.4" "PuLP==2.7.0"

From a repository checkout, run the disposable starter smoke against the
checked-in BAM fixture:

.. code-block:: bash

   TURBO_PICARD_BIN="$(command -v turbo-picard)" \
     bash tools/verify_snakemake_starter.sh

For the smallest reviewable trial shape, it also includes
``one-command-trial.md`` plus tiny ``trial.wdl`` and ``trial.nf`` workflows
that show a single-command evaluation flow.
Practical path
--------------

1. Start beside Picard
~~~~~~~~~~~~~~~~~~~~~~

Start with the explicit binary:

.. code-block:: bash

   turbo-picard MarkDuplicates I=input.bam O=turbo.bam M=turbo.metrics.txt

Generate the side-by-side trial contract before wiring it into a workflow:

.. code-block:: bash

   turbo-picard trial MarkDuplicates I=input.bam O=turbo.bam M=turbo.metrics.txt

Keep upstream Picard as the production path while you compare:

* BAM, SAM, FASTQ, VCF, interval-list, and metrics outputs.
* Sidecar files such as indexes and md5 files.
* Exit codes and error messages for bad inputs.
* Runtime and memory behavior on realistic shards.

2. Check the commands you need
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

Check the command list and run the parity scripts for the steps you plan to
change:

.. code-block:: bash

   python3 tools/verify_command_matrix.py
   ./tools/verify_basic_picard_parity.sh
   ./tools/verify_basic_sortsam_parity.sh
   ./tools/verify_basic_samtofastq_parity.sh

The repository keeps one parity script per documented command. You are not
trying to prove all of Picard in one afternoon; you are checking the behavior
your pipeline actually depends on. The comparison boundary is described in
:doc:`parity`.

3. Configure fallback for mixed coverage
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

Use fallback when a pipeline calls both accelerated and unsupported Picard
commands:

.. code-block:: bash

   export TURBO_PICARD_FALLBACK_COMMAND='java -jar /opt/picard/picard.jar'

Native commands still run natively. Unsupported commands delegate to upstream
Picard. Details are in :doc:`fallback`.

4. Switch only the checked commands
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

Only after the relevant parity and benchmark evidence is acceptable, put the
``picard`` shim ahead of upstream Picard on ``PATH`` for that pipeline or
environment.

5. Keep evidence in CI
~~~~~~~~~~~~~~~~~~~~~~

Keep the targeted parity scripts and benchmark suite close to the pipeline code
for the commands you depend on:

.. code-block:: bash

   python3 tools/bench_suite.py --repeats 1 --skip-build
   python3 tools/verify_benchmark_log_evidence.py

This keeps upgrades boring: coverage changes are visible, and performance
claims stay tied to measured output.

For workflow-manager checks, use the JSON trial report and store it beside the
targeted comparison output:

.. code-block:: bash

   turbo-picard trial --json MarkDuplicates I=input.bam O=turbo.bam M=turbo.metrics.txt

Share a trial result
--------------------

If you run a one-command trial, share the result through the `trial report issue
form <https://github.com/dnncha/turbo-picard/issues/new?template=trial-report.yml>`_.
Include the tool and Picard versions, install method, redacted command, input
shape, comparison outcome, and any timing or workflow friction. Successful
matches and negative results are both useful: they show which workflow shapes
are checked and which still need work.

If GitHub does not offer new-issue creation, add the same redacted report as a
comment on the `public trial report thread
<https://github.com/dnncha/turbo-picard/issues/4>`_. Do not include private
clinical, human-subject, or controlled-access data.

If you are working from a repository checkout, the real-data comparator can
also produce a public-report starting point alongside the full private audit
bundle:

.. code-block:: bash

   python3 tools/compare_real_data.py \
     --input-bam "$INPUT_BAM" \
     --output-dir turbo-picard-trial/evidence \
     --commands MarkDuplicates \
     --picard-command "java -jar $PICARD_JAR" \
     --turbo-picard-command turbo-picard \
     --shareable-report turbo-picard-trial/evidence/shareable-trial-report.md \
     --skip-build

The shareable report deliberately leaves out local paths, input hashes,
command arguments, generated artifact names, and raw data. Review it before
posting. Add ``--include-public-source`` only when the supplied source URL and
revision are genuinely public; do not attach the full ``work/`` directory or
the raw comparison JSON for a private input.
The comparator materializes output SAM through the configured
Picard-compatible ``ViewSam`` entrypoints, so this reviewable path does not
require a separate ``samtools`` executable on ``PATH``. ``samtools quickcheck``
remains an optional manual integrity check.

Do not upload private clinical, human-subject, or controlled-access data. A
summarized file shape, redacted command, and description of the outputs that
matched or differed are enough.

Check a representative file
---------------------------

In plain terms, choose data that looks like the run you want to switch.

Before replacing Picard in a research pipeline, run at least one representative
BAM or CRAM through the real-data comparator. Pick data that looks like the
workflow you plan to switch: same assay, aligner, read groups,
duplicate-marking policy, sort order, and common edge cases such as soft clips,
orphaned mates, secondary or supplementary records, UMIs, or mitochondrial reads
when those matter.

For public data, cite an immutable source URL and commit or accession. For
GitHub fixtures, use a URL containing ``/blob/<commit>/`` and the full
40-character SHA. The full 40-character Git commit SHA must be visible in the
evidence, not a branch name or short hash. For accession-hosted data, put the
accession or release identifier in both the URL and ``--input-source-commit``.
For private production data, keep the comparison bundle with the private
dataset ID, input SHA-256, and a plain sentence describing what the shard
represents.

.. code-block:: bash

   cargo build --release -p turbo-picard-cli --bin picard

   python3 tools/audit_real_data.py \
     --input-bam /data/representative.bam \
     --input-source-url https://example.org/pinned/source-or-accession.bam \
     --input-source-commit example-release-or-commit \
     --output-dir benchmarks/real-data/my-workflow-representative/evidence \
     --dataset-id my-workflow-representative \
     --scope-caveat "representative shard for this workflow; not a full cohort" \
     --picard-command "mamba run -p /opt/conda/envs/picard picard" \
     --turbo-picard-command ./target/release/picard \
     --skip-build

The audit wrapper runs the same comparisons as ``compare_real_data.py`` and can
update the manifest when the run succeeds. For CRAM shards, pass
``--reference-fasta``:

.. code-block:: bash

   python3 tools/audit_real_data.py \
     --input-bam /data/representative.cram \
     --reference-fasta /refs/hg38.fa \
     --input-source-url https://example.org/pinned/source-or-accession.cram \
     --input-source-commit example-release-or-commit \
     --output-dir benchmarks/real-data/my-workflow-cram/evidence \
     --dataset-id my-workflow-cram \
     --scope-caveat "representative CRAM shard for this workflow; not a full cohort" \
     --picard-command "mamba run -p /opt/conda/envs/picard picard" \
     --turbo-picard-command ./target/release/picard \
     --skip-build

You can also pass ``REFERENCE_SEQUENCE`` or ``TURBO_PICARD_REFERENCE`` on
individual commands when you run them by hand.

If you need to control the command list directly, use ``compare_real_data.py``:

.. code-block:: bash

   python3 tools/compare_real_data.py \
     --input-bam /data/representative.bam \
     --input-source-url https://example.org/pinned/source-or-accession.bam \
     --input-source-commit example-release-or-commit \
     --output-dir benchmarks/real-data/my-workflow-representative/evidence \
     --dataset-id my-workflow-representative \
     --scope-caveat "representative shard for this workflow; not a full cohort" \
     --release-tier release_candidate \
     --commands AddOrReplaceReadGroups BuildBamIndex CleanSam CollectAlignmentSummaryMetrics CollectInsertSizeMetrics CollectQualityYieldMetrics FixMateInformation MarkDuplicates MergeSamFiles ReplaceSamHeader RevertSam SamToFastq SetNmMdAndUqTags SortSam ValidateSamFile ViewSam \
     --picard-command "mamba run -p /opt/conda/envs/picard picard" \
     --turbo-picard-command ./target/release/picard \
     --skip-build

For CRAM, add ``--reference-fasta`` and include the same command list; the
comparator records the reference SHA-256 beside the alignment input.

.. code-block:: bash

   python3 tools/compare_real_data.py \
     --input-bam /data/representative.cram \
     --reference-fasta /refs/hg38.fa \
     --input-source-url https://example.org/pinned/source-or-accession.cram \
     --input-source-commit example-release-or-commit \
     --output-dir benchmarks/real-data/my-workflow-cram/evidence \
     --dataset-id my-workflow-cram \
     --scope-caveat "representative CRAM shard for this workflow; not a full cohort" \
     --release-tier release_candidate \
     --commands AddOrReplaceReadGroups BuildBamIndex CleanSam CollectAlignmentSummaryMetrics CollectInsertSizeMetrics CollectQualityYieldMetrics FixMateInformation MarkDuplicates MergeSamFiles ReplaceSamHeader RevertSam SamToFastq SetNmMdAndUqTags SortSam ValidateSamFile ViewSam \
     --picard-command "mamba run -p /opt/conda/envs/picard picard" \
     --turbo-picard-command ./target/release/picard \
     --skip-build

If all commands pass, review the generated Markdown and JSON reports, then add
the manifest entry:

.. code-block:: bash

   python3 tools/update_real_data_manifest.py \
     --entry benchmarks/real-data/my-workflow-representative/evidence/manifest-entry.json
   python3 tools/verify_real_data_evidence.py --release-ready

Treat a failure as useful information and pause that command. Keep upstream
Picard on that step until the mismatch is understood, fixed, and covered by a
regression test or a pinned real-data comparison. Do not make claims about every
lab dataset from the checked-in NA12878 mitochondrial test BAM alone; it is
useful package evidence and a real Picard edge case, not proof of every
workflow. The release-ready check must cover
AddOrReplaceReadGroups, BuildBamIndex, CleanSam,
CollectAlignmentSummaryMetrics, CollectInsertSizeMetrics,
CollectQualityYieldMetrics, MarkDuplicates, RevertSam, SamToFastq, SortSam,
ValidateSamFile, ViewSam somewhere in pinned release evidence. It also requires
enough input data that the check cannot pass on one tiny fixture alone.

Run it beside Picard first. For each workflow, compare turbo-picard and Picard
outputs on representative inputs before changing production defaults.

GATK-style preprocessing
------------------------

Typical GATK best-practice slices map cleanly to side-by-side checks:

* ``MarkDuplicates`` on coordinate-sorted BAM or CRAM;
* ``SortSam`` before recalibration when order changes;
* ``SamToFastq`` before re-alignment;
* ``ValidateSamFile`` SUMMARY mode before downstream VCF work.

Run those commands through ``tools/audit_real_data.py`` on the same NA12878 or
production shard you plan to switch, keep Picard as fallback for commands outside
the matrix, and only move the ``picard`` shim ahead on ``PATH`` after the audit
bundle passes.
