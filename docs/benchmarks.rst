Benchmarks
==========

Benchmarks are useful only when they are reproducible and tied to parity. The
repository benchmark suite is designed to report speed while keeping command
coverage and Picard-compatible behavior visible.

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

The public benchmark suite currently reports 32 command speedups, so every
native or partly native matrix entry has a parity-checked public speedup claim.
Two scopes are deliberately narrow: ``CollectMultipleMetrics`` is benchmarked
with ``PROGRAM=CollectQualityYieldMetrics``, and chart-producing child programs
still use the chart-output disclosure below.

Real-data parity evidence
-------------------------

Synthetic benchmark speedups are not enough for switching decisions. Real-data
parity evidence is tracked in ``benchmarks/real-data/manifest.json`` and checked
with:

.. code-block:: bash

   python3 tools/verify_real_data_evidence.py
   python3 tools/verify_real_data_evidence.py --release-ready

The checked-in release evidence currently includes GATK's public NA12878
mitochondrial test BAM and Picard's public SNVQ metrics test BAM.
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
non-zero exit code on that input. The release check must cover this command set
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
