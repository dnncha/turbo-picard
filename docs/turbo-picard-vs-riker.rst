turbo-picard vs riker
=====================

`riker <https://github.com/fulcrumgenomics/riker>`_ and ``turbo-picard`` both
accelerate Picard-style sequencing QC in Rust, using single-pass multi-collector
execution. Where they differ is **compatibility, memory footprint, and command
breadth**:

* ``turbo-picard`` preserves Picard's command names, ``KEY=VALUE`` flags, and
  **exact Picard-checked metrics and histograms**, alongside native
  ``MarkDuplicates``, ``SortSam``, ``MergeSamFiles``, ``SamToFastq``,
  ``FastqToSam``, ``FixMateInformation``, and VCF utilities.
* ``riker`` uses its own subcommands (``wgs``, ``alignment``, ``isize``,
  ``gcbias``, ``basic``, ``multi``, ``hybcap``), simplified TSV outputs, plots,
  intentional metric definition changes (`ERRATA.md
  <https://github.com/fulcrumgenomics/riker/blob/main/ERRATA.md>`_), and RNA-seq
  metrics.

Measured whole-genome comparison (``CollectWgsMetrics``)
--------------------------------------------------------

On real 1000 Genomes ``HG02675`` GRCh38 alignments (Intel i7-8700, 6 threads,
16 GB RAM, cold page cache; see
``benchmarks/wgs-coverage-2026-11-13/README.md``), Turbo Picard matched **every
metric field and histogram row** of Picard 3.5.0:

.. list-table::
   :header-rows: 1
   :widths: 20 22 20 20 18

   * - Coverage depth
     - Turbo Picard (6 workers)
     - ``riker wgs`` 0.5.0 (6T)
     - Picard 3.5.0 (1T)
     - Key takeaway
   * - 4x GRCh38 WGS
     - **21.8–22.6 s / 125 MB**
     - 29.7 s / 504 MB
     - 1,033.8 s / 1,468 MB
     - **~46x faster & 11.7x less RAM** than Picard; **1.3x faster & 4.0x less RAM** than Riker
   * - 15x GRCh38 WGS
     - **65.4–82.6 s / 274 MB**
     - 78.2–79.7 s / 518 MB
     - —
     - Competitive wall time and **~1.9x less RAM** with exact Picard parity
   * - 30x GRCh38 WGS *(pre-PR #76)*
     - 193.5 s / 412 MB
     - **147.4 s** / 510 MB
     - —
     - 20% lower RSS; pre-PR #76 reprocessed an entire capped contig single-threaded (replaced with targeted cluster reprocessing in PR #76, pending rerun)

Small-input smoke comparison (``gatk-na12878-mito``)
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

On a 2,097,008-byte mitochondrial BAM
(``benchmarks/riker-comparison/evidence/bench_qc_vs_riker.md``):

* **wgs-only:** ``turbo-picard CollectWgsMetrics`` **0.012 s** vs ``riker wgs``
  **0.024 s** (**2.10x faster**) vs Picard **7.302 s** (**633.74x**).
* **wgs-bundle:** ``turbo-picard CollectMultipleMetrics`` **0.025 s** vs
  ``riker multi`` **0.053 s** (**2.14x faster**) vs Picard **11.801 s** across
  the separate Picard commands (**477.44x**).

Because this fixture is 2.1 MB and omits peak RSS, treat the whole-genome
``HG02675`` table above as the primary guide for large inputs.

What overlaps
-------------

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

When to choose each tool
------------------------

Choose ``turbo-picard`` when:

* You want faster QC **and** lower peak memory (125–274 MB on 4x–15x WGS)
  without rewriting existing Picard command lines, WDL/Nextflow modules, or
  downstream metric parsers.
* You need exact Picard-identical metric definitions, histograms, or
  preprocessing tools in the same binary (``MarkDuplicates``, ``SortSam``,
  ``MergeSamFiles``, ``SamToFastq``, ``FastqToSam``, ``FixMateInformation``,
  VCF tools).
* You want automatic fallback to upstream Picard for unsupported commands or
  edge flags.

Choose ``riker`` when:

* You are building a new QC-only pipeline that prefers Riker's subcommands,
  simplified TSV files, plots, or RNA-seq collector (``CollectRnaSeqMetrics``
  remains fallback-only in ``turbo-picard``).

Current turbo-picard QC limits
------------------------------

``CollectHsMetrics`` supports native ALL_READS metrics, histograms, per-target,
and per-base outputs (measured **12.8x faster** than Picard 3.4.0 on
capture-panel intervals in PR #76), while unsupported advanced options delegate
to upstream Picard. ``CollectSamErrorMetrics`` and ``CollectRnaSeqMetrics``
remain delegated. Metrics chart sidecars are lightweight PDFs rather than
Picard-equivalent R plots. See :doc:`fallback` and :doc:`commands`.

How to benchmark them fairly
----------------------------

Use the repository helper for smoke checks or full WGS comparisons:

.. code-block:: bash

   python3 tools/bench_qc_vs_riker.py --smoke --skip-build --allow-missing-riker

For larger inputs, follow ``benchmarks/riker-comparison/README.md`` and
``benchmarks/wgs-coverage-2026-11-13/README.md``. Fair comparison rules:

* use the same coordinate-sorted BAM/CRAM and reference FASTA for all tools;
* match worker/thread counts and compare bundle profiles against bundle profiles;
* keep output parity checks separate from speed checks;
* publish wall time, peak RSS, and exact tool versions together.
