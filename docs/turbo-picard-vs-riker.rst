turbo-picard vs riker
=====================

`riker <https://github.com/fulcrumgenomics/riker>`_ and ``turbo-picard`` overlap
on some sequencing QC work, but their command and output contracts differ. This
page is a workflow-fit guide, not a universal performance ranking.

Short answer
------------

Evaluate ``turbo-picard`` when an existing task needs Picard command names,
``KEY=VALUE`` arguments, and documented Picard-style output contracts. Its
native scope also includes preprocessing commands such as duplicate marking,
sorting, FASTQ conversion, and VCF utilities.

Evaluate riker when its QC-specific command and output model fits the workflow.
Compare both tools using the same input, required outputs, thread settings,
machine profile, and downstream checks. Do not infer a general ordering from a
single command or benchmark fixture.

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

Where turbo-picard differs today
--------------------------------

Command and output model
------------------------

Picard-shaped workflow tasks
   ``turbo-picard`` keeps Picard command names and ``KEY=VALUE`` arguments for
   its documented native scope. Existing ``WDL``, ``Nextflow``, ``Snakemake``,
   and shell tasks can use the explicit binary while outputs are evaluated.

Additional turbo-picard commands
   ``turbo-picard`` documents native or partial-native scope for
   ``MarkDuplicates``, ``SortSam``, ``SamToFastq``, ``FastqToSam``,
   ``FixMateInformation``, and selected VCF utilities. Check :doc:`commands`
   before treating any command or option as native.

Saved speedups against Picard
   The current saved suite reports ``CollectWgsMetrics`` at ``22.42x`` against
   Picard 3.4.0 on its documented fixture. It is not a comparison against riker.

Small direct comparison fixtures
   ``benchmarks/riker-comparison/`` includes small-input overlap profiles. They
   are smoke checks only; they do not settle a WGS-scale or laboratory-specific
   tool choice.

Output review
   ``turbo-picard`` documents Picard comparison targets for each native scope.
   Review the exact outputs required by the workflow rather than assuming that
   two tools with related metrics have interchangeable contracts.

Current turbo-picard limits
   ``CollectHsMetrics`` has a native core ALL_READS metrics, histogram,
   per-target, and per-base path, while unsupported advanced options delegate
   to upstream Picard. ``CollectSamErrorMetrics`` remains delegated.
   Metrics chart sidecars are lightweight PDFs rather than Picard-equivalent
   rendered charts. See :doc:`fallback` and :doc:`commands`.

How to benchmark them fairly
----------------------------

Use the repository helper:

.. code-block:: bash

   python3 tools/bench_qc_vs_riker.py --smoke --skip-build --allow-missing-riker

Smoke runs now default to ``5`` repeats and report the median so the tiny mito
fixture reflects steady-state overlap performance instead of one-shot startup
noise.

For larger inputs, follow ``benchmarks/riker-comparison/README.md`` and retain
the exact input, commands, versions, and hardware in the comparison record.

Fair comparison rules:

* use the same coordinate-sorted BAM for all three tools;
* compare bundle profiles against bundle profiles, not one riker ``multi`` call
  against a single Picard command;
* keep output parity checks separate from speed checks;
* publish wall time, peak RSS, and the exact tool versions together.

For capture or exome QC, use the native ``CollectHsMetrics`` path for the
documented ALL_READS metrics and sidecar scope and retain upstream Picard
fallback for the options listed in the command matrix.
