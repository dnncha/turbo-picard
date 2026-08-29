Benchmarks
==========

Benchmarks are useful only when they are reproducible and tied to parity. The
repository benchmark suite is designed to report speed while keeping command
coverage and Picard-compatible behavior visible.

The current saved suite records ``22.88x`` as its slowest saved speedup,
``84.52x`` as its geometric mean speedup, and ``272.12x`` as its top speedup for
the documented command, option, input, and machine scopes. The companion
``MarkDuplicates`` fixture records median RSS of about ``1.2 GB`` for Picard
3.4.0 and ``8.7 MB`` for ``turbo-picard``. These are reproducible fixture
measurements, not performance guarantees for another workflow.

Small direct QC overlap fixtures with riker are kept separately in
``benchmarks/riker-comparison/``. They are useful smoke checks, but a tool
selection should use representative data and the same versions, machine, thread
settings, outputs, and downstream comparisons.

Run the suite
-------------

.. code-block:: bash

   python3 tools/bench_suite.py --repeats 1 --skip-build

To refresh the static benchmark assets used by the project site:

.. code-block:: bash

   printf 'benchmark_date=%s source=python3 tools/bench_suite.py --repeats 1 --skip-build\n' "$(date +%F)" > docs/site/assets/bench-suite-output.txt
   python3 tools/bench_suite.py --repeats 1 --skip-build | tee -a docs/site/assets/bench-suite-output.txt
   python3 tools/render_benchmark_assets.py --suite-output docs/site/assets/bench-suite-output.txt
   python3 tools/verify_benchmark_log_evidence.py
   python3 tools/verify_benchmark_suite_coverage.py
   python3 tools/verify_benchmark_thresholds.py

The threshold verifier is deliberately simple. Public release evidence must
keep every saved benchmark at parity, keep the slowest saved speedup at or
above ``5.00x``, keep the geometric mean at or above ``20.00x``, and keep the
top speedup at or above ``50.00x``. If one command drops below that floor, fix
the command, rerun the suite, or stop using the stale number as release
evidence.

Read benchmark claims carefully
-------------------------------

When you compare against upstream Picard, record:

* the exact command line;
* input size and sort order;
* Picard version;
* ``turbo-picard`` commit;
* CPU, memory, storage, and container or conda environment;
* parity result for the output you are measuring.

Do not generalize a benchmark from one command to another. ``MarkDuplicates``,
``SortSam``, FASTQ conversion, metrics collectors, and VCF utilities stress
different parts of the system.

The public benchmark suite currently reports 32 command speedups, covering the
native or partly native data-processing commands in the matrix.
``AccelerationStatus``, ``capabilities``, ``doctor``, ``explain``, and ``trial`` are exempt
because they are status/preflight commands with no Picard data-processing
runtime to benchmark.
Two benchmark scopes are deliberately narrow: ``CollectMultipleMetrics`` is benchmarked with
``PROGRAM=CollectQualityYieldMetrics``, and chart-producing child programs still
use the chart-output disclosure below.

Benchmark exemption: ``AccelerationStatus`` — status/preflight command with no Picard data-processing runtime to benchmark.
Benchmark exemption: ``capabilities`` — discovery/preflight command with no Picard data-processing runtime to benchmark.
Benchmark exemption: ``doctor`` — status/preflight command with no Picard data-processing runtime to benchmark.
Benchmark exemption: ``explain`` — status/preflight command with no Picard data-processing runtime to benchmark.
Benchmark exemption: ``trial`` — status/preflight command with no Picard data-processing runtime to benchmark.
Benchmark exemption: ``CollectHsMetrics`` — ALL_READS metrics and sidecar parity are guarded separately; public performance benchmark awaits representative capture data. The real-data comparator now supports pinned WES/capture interval-lists for command-level parity trials.

Genome-scale evidence
---------------------

Micro-benchmarks in ``tools/bench_suite.py`` are useful for regression tracking,
but a real switching decision should include at least one larger shard that
resembles the workflow:

* whole-genome or exome BAM size, not only 100k-read fixtures;
* the same duplicate-marking, sorting, and FASTQ conversion commands the workflow uses;
* wall time, peak RSS, and cloud cost estimates from the same machine profile.

Refresh public evidence with:

.. code-block:: bash

   python3 tools/audit_real_data.py \
     --input-bam /data/representative-wgs.bam \
     --input-source-url https://example.org/accession.bam \
     --input-source-commit example-accession \
     --output-dir benchmarks/real-data/wgs-representative/evidence \
     --dataset-id wgs-representative \
     --picard-command "mamba run -p /opt/conda/envs/picard picard" \
     --turbo-picard-command ./target/release/picard \
     --skip-build

Pair that bundle with ``python3 tools/bench_suite.py`` on the same commands so
performance claims stay tied to parity-checked outputs.

Large-input speed evidence
--------------------------

Use large-input speed evidence for direct comparisons with tools such as
``riker``. Keep it separate from the small release-candidate parity fixtures so
readers can tell which numbers are smoke checks and which numbers came from a
workflow-sized BAM.

For the QC overlap surface, stage the ``HG02675_4x`` BAM described in
``benchmarks/riker-comparison/README.md`` and run:

.. code-block:: bash

   python3 tools/bench_qc_vs_riker.py \
     --sample-id HG02675_4x \
     --input-bam /mnt/scratch/HG02675_4x/input.bam \
     --reference-fasta /mnt/scratch/refs/hg38.fa \
     --output-dir benchmarks/riker-comparison/evidence/HG02675_4x \
     --repeats 3 \
     --measure-rss \
     --skip-build

The report includes per-profile Picard, ``turbo-picard``, and ``riker`` wall
time, optional RSS, total profile speedups, and an explicit overlap leader/gap
summary for ``wgs-only`` and ``wgs-bundle``. If ``riker`` wins a profile, record
that gap and the next bottleneck instead of using the smoke fixture as a
substitute for WGS-scale evidence.

Real-data parity evidence
-------------------------

Synthetic benchmark speedups are not enough for switching decisions. Real-data
parity evidence is tracked in ``benchmarks/real-data/manifest.json`` and checked
with:

.. code-block:: bash

   python3 tools/verify_real_data_evidence.py
   python3 tools/verify_real_data_evidence.py --release-ready

The checked-in release evidence currently includes GATK's public NA12878
mitochondrial test BAM, the same shard converted to CRAM with a pinned
reference, and Picard's public SNVQ metrics test BAM.
For GitHub-hosted real-data inputs, the evidence must cite a
``/blob/<commit>/`` URL and the full 40-character Git commit SHA, not a branch
name or short hash.

GATK NA12878 mitochondrial evidence:

* source:
  ``https://github.com/broadinstitute/gatk/blob/e8c49f600b06c658e0fa9bf67256340ebb46bc48/src/test/resources/org/broadinstitute/hellbender/tools/mutect/mito/NA12878.bam``
* commit: ``e8c49f600b06c658e0fa9bf67256340ebb46bc48``
* local SHA-256:
  ``70ea2e429805a75ce6007a32ba176ea7c697a398e0c39a9d58aaaa30e1ed86c3``
* evidence report:
  ``benchmarks/real-data/gatk-na12878-mito/evidence/real-data-comparison.md``
* scope caveat: ``GATK public NA12878 mitochondrial test BAM.``
* minimum input threshold: ``1000000`` bytes

GATK NA12878 mitochondrial CRAM evidence:

* source:
  ``https://github.com/broadinstitute/gatk/blob/e8c49f600b06c658e0fa9bf67256340ebb46bc48/src/test/resources/org/broadinstitute/hellbender/tools/mutect/mito/NA12878.bam``
* commit: ``e8c49f600b06c658e0fa9bf67256340ebb46bc48``
* local SHA-256:
  ``68931e7cea6e9a35029cfed3638d0d8ea2c4bb662b4d83232968da247b68f7bc``
* evidence report:
  ``benchmarks/real-data/gatk-na12878-mito-cram/evidence/real-data-comparison.md``
* scope caveat:
  ``GATK public NA12878 mitochondrial test BAM converted to CRAM with assembly38 mt-only reference.``
* minimum input threshold: ``910668`` bytes

Picard SNVQ metrics evidence:

* source:
  ``https://github.com/broadinstitute/picard/blob/fc0b08410d38a10afd08e467dab74bf5e2e71310/testdata/picard/sam/snvq_metrics_test.bam``
* commit: ``fc0b08410d38a10afd08e467dab74bf5e2e71310``
* local SHA-256:
  ``be0daa7cb8e9ce11f2f68ac3db8c229d530736aaf7b80df3669fdb00779c06b3``
* evidence report:
  ``benchmarks/real-data/picard-snvq/evidence/real-data-comparison.md``
* scope caveat: ``Picard public SNVQ metrics test BAM.``
* minimum input threshold: ``1000000`` bytes

Both saved runs pass Picard 3.4.0 on ``ViewSam``, ``CleanSam``,
``CollectQualityYieldMetrics``, ``CollectAlignmentSummaryMetrics``, and
``MarkDuplicates``. The GATK NA12878 mitochondrial bundle also passes
``AddOrReplaceReadGroups`` with a SAM record and read-group header digest,
``BuildBamIndex`` with an exact BAI binary digest, ``RevertSam`` with a
reverted SAM record digest, ``SortSam`` with a coordinate-sorted record multiset
digest, ``SamToFastq`` with first-end, second-end, and unpaired FASTQ outputs
matched byte-for-byte, and ``CollectInsertSizeMetrics`` with the stable metrics
table and insert-size histogram digest matched against Picard. It also passes
``ValidateSamFile`` by matching the summary validation histogram and Picard's
non-zero exit code on that input. The CRAM bundle passes ``CleanSam``,
``CollectQualityYieldMetrics``, ``CollectInsertSizeMetrics``, ``MarkDuplicates``,
``SortSam``, and ``AddOrReplaceReadGroups`` on the same public mitochondrial
shard with native CRAM I/O. The release check must cover this command set
somewhere in the pinned evidence:
AddOrReplaceReadGroups, BuildBamIndex, CleanSam,
CollectAlignmentSummaryMetrics, CollectInsertSizeMetrics,
CollectQualityYieldMetrics, MarkDuplicates, RevertSam, SamToFastq, SortSam,
ValidateSamFile, ViewSam. These fixtures are useful release evidence, but they
are still small. They are not proof for every dataset a lab might process. Add
larger public shards or representative private data before claiming the tool is
proven for a whole production dataset. The release-ready check also requires
enough pinned input data that one tiny fixture cannot carry the release by
itself.
