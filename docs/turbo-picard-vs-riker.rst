turbo-picard vs riker
=====================

`riker <https://github.com/fulcrumgenomics/riker>`_ is a fast Rust QC toolkit from
Fulcrum Genomics. It overlaps with part of what ``turbo-picard`` does, but the two
projects solve different adoption problems.

Short answer
------------

Choose ``turbo-picard`` when you already run Picard-shaped pipeline steps and want
to keep the same command names, ``KEY=VALUE`` arguments, and output contracts while
running much faster on the commands already accelerated.

Choose riker when you are designing a new QC-only workflow from scratch, want
riker's simplified TSV outputs, and are willing to rewrite task interfaces around
``riker <subcommand>`` instead of ``picard <Command>``.

What overlaps
-------------

Both projects accelerate Picard-style sequencing QC metrics. The direct overlap
today is roughly:

.. list-table::
   :header-rows: 1

   * - Picard command
     - riker command
   * - ``CollectWgsMetrics``
     - ``riker wgs``
   * - ``CollectAlignmentSummaryMetrics``
     - ``riker alignment``
   * - ``CollectInsertSizeMetrics``
     - ``riker isize``
   * - ``CollectGcBiasMetrics``
     - ``riker gcbias``
   * - ``CollectBaseDistributionByCycle``, ``MeanQualityByCycle``,
       ``QualityScoreDistribution``
     - part of ``riker basic``
   * - ``CollectMultipleMetrics``
     - ``riker multi``
   * - ``CollectHsMetrics``
     - ``riker hybcap``

Where turbo-picard is ahead today
---------------------------------

Drop-in pipeline compatibility
   ``turbo-picard`` keeps Picard command names and ``KEY=VALUE`` arguments. Existing
   ``WDL``, ``Nextflow``, ``Snakemake``, and shell steps can swap the executable
   without redesigning task inputs or output parsers.

Broader command coverage
   ``turbo-picard`` accelerates preprocessing and utility commands riker does not
   attempt: ``MarkDuplicates``, ``SortSam``, ``SamToFastq``, ``FastqToSam``,
   ``FixMateInformation``, VCF utilities, and more. Riker explicitly stays QC-only
   and points users elsewhere for dedup/sort work.

Saved speedups on overlapping metrics
   The current saved benchmark suite reports much higher speedups than riker's
   published Picard comparisons on the overlapping metrics surface. For example,
   ``CollectWgsMetrics`` is currently saved at ``22.42x`` versus Picard 3.4.0,
   while riker's published WGS numbers are roughly ``10-13x`` on 1000 Genomes
   30x BAMs.

Parity-checked outputs
   ``turbo-picard`` is built to match Picard outputs on the documented native
   scope. Riker intentionally changes output shape and some metric semantics to
   produce cleaner TSVs.

Memory on preprocessing hot paths
   The checked ``MarkDuplicates`` run in this repository drops median RSS from about
   ``1.2 GB`` in Picard to about ``8.7 MB``. That matters for high-fanout
   workflows. Riker does not compete on duplicate marking.

Where riker is ahead today
--------------------------

Bioconda availability
   riker is already packaged on Bioconda. ``turbo-picard`` has submitted recipes
   but is not accepted yet.

Single-pass QC bundles
   ``riker multi`` is a strong story: one BAM pass, many collectors, one command
   line. ``turbo-picard`` has ``CollectMultipleMetrics``, but riker's benchmark
   narrative around bundle QC is more mature.

Hybrid-capture and error metrics
   riker ships ``hybcap`` and ``error`` today. ``turbo-picard`` has a
   ``CollectHsMetrics`` scaffold, but native bait/target accumulation is not
   complete yet; ``CollectSamErrorMetrics`` and ``CollectHsMetrics`` currently
   delegate to upstream Picard.

WGS-scale public benchmark narrative
   riker publishes reproducible 1000 Genomes 30x WGS numbers. ``turbo-picard``
   keeps stronger synthetic and smaller real-data evidence today. Use
   ``tools/bench_qc_vs_riker.py`` to generate three-way evidence on the same BAM.

How to benchmark them fairly
----------------------------

Use the repository helper:

.. code-block:: bash

   python3 tools/bench_qc_vs_riker.py --smoke --skip-build --allow-missing-riker

Smoke runs now default to ``5`` repeats and report the median so the tiny mito
fixture reflects steady-state overlap performance instead of one-shot startup
noise.

For WGS-scale runs, stage the same BAMs riker uses and follow
``benchmarks/riker-comparison/README.md``.

Fair comparison rules:

* use the same coordinate-sorted BAM for all three tools;
* compare bundle profiles against bundle profiles, not one riker ``multi`` call
  against a single Picard command;
* keep output parity checks separate from speed checks;
* publish wall time, peak RSS, and the exact tool versions together.

Practical rollout guidance
--------------------------

If your workflow already calls Picard by name, start with ``turbo-picard``. The
migration cost is one executable swap plus a representative output check.

If you are greenfielding QC analytics and want lowercase TSV columns with no
Picard comment headers, riker may be simpler to adopt even though it is still
labeled alpha software.

For capture/exome QC specifically, wait for native ``CollectHsMetrics`` in
``turbo-picard`` or use upstream Picard fallback until that command ships.
