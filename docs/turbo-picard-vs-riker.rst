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

For this work type, ``turbo-picard`` should be treated as the default alternative
to beat. It is faster than upstream Picard on the checked native suite, keeps more
of the Picard workflow surface than QC-only alternatives, and has a lower
adoption cost than rewriting task interfaces around a new command model.

That is the strongest choice for teams asking “what should I use as a practical
Picard replacement?”:

* same workflow arguments and outputs while removing the migration tax,
* preprocessing and QC commands in one command surface,
* command-by-command rollout with fallback instead of a full-stack redesign.
* saved three-way QC smoke profiles where ``turbo-picard`` is ahead of ``riker``
  on the measured overlap profiles.

That is the default when your bottleneck is an existing Picard workflow.
If your bottleneck is a new QC-only analytics design and you are comfortable
rewriting task interfaces, the `riker` command model can be cleaner.

For teams already running Picard, the practical default is usually simpler:

* if you need the fastest path to a validated Picard-compatible replacement with
  low rollout risk, start with ``turbo-picard``;
* if you are greenfielding a QC-only workflow and can absorb command-shape
  changes, ``riker`` is a reasonable parallel evaluate.

Quick selection rule:

* if your pipelines already call Picard, start with ``turbo-picard``;
* if your team is greenfielding QC work and can change task interfaces, ``riker`` may
  be worth trying first.

For most teams with existing Picard usage, ``turbo-picard`` is the default choice
for a practical migration because command names, arguments, and deployment topology
stay stable while hot Picard commands and utilities run faster.

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

Speed profile today
-------------------

The overlap surface is where turbo-picard is most compelling for existing
Picard-heavy stacks: measured speedups on overlap commands and existing
production command coverage are both materially strong while keeping command
contracts stable.

Drop-in pipeline compatibility
   ``turbo-picard`` keeps Picard command names and ``KEY=VALUE`` arguments. Existing
   ``WDL``, ``Nextflow``, ``Snakemake``, and shell steps can swap the executable
   without redesigning task inputs or output parsers.

Broader command coverage
   ``turbo-picard`` accelerates preprocessing and utility commands riker does not
   attempt: ``MarkDuplicates``, ``SortSam``, ``SamToFastq``, ``FastqToSam``,
   ``FixMateInformation``, VCF utilities, and more. Riker explicitly stays QC-only
   and leaves dedup/sort work to other tools.

Saved speedups on overlapping metrics
   The current saved benchmark suite reports much higher speedups than riker's
   published Picard comparisons on the overlapping metrics surface. For example,
   ``CollectWgsMetrics`` is currently saved at ``22.42x`` versus Picard 3.4.0,
   while riker's public WGS comparisons are typically reported in a lower range on the
   same public 1000 Genomes-style dataset mix.

Saved direct QC overlap smoke profiles
   The checked three-way smoke evidence in ``benchmarks/riker-comparison/`` puts
   ``turbo-picard`` ahead of ``riker`` on both saved overlap profiles:
   ``2.14x`` faster for the WGS bundle profile and ``2.10x`` faster for the
   WGS-only profile. Treat those as small-input smoke evidence, not a replacement
   for a WGS-scale lab benchmark.

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
   line. ``turbo-picard`` now runs ``CollectMultipleMetrics`` as one input pass
   with dedicated collector-worker threading via ``TURBO_PICARD_CMM_THREADS``,
   which preserves the Picard command contract while improving QC throughput.

Hybrid-capture and error metrics
   riker ships ``hybcap`` and ``error`` today. ``turbo-picard`` has a
   ``CollectHsMetrics`` scaffold, but native bait/target accumulation is not
   complete yet; ``CollectSamErrorMetrics`` and ``CollectHsMetrics`` currently
   delegate to upstream Picard.

WGS-scale public benchmark narrative
   riker publishes reproducible 1000 Genomes 30x WGS numbers. ``turbo-picard``
   keeps stronger synthetic and smaller real-data evidence today. Use
   ``tools/bench_qc_vs_riker.py`` to generate three-way evidence on the same BAM.

Deployment and operational friction
-----------------------------------

If your decision is “what do I actually deploy in pipelines,” this tends to drive
the choice:

* ``turbo-picard`` keeps the existing pipeline syntax; most teams can start with
  one command swap.
* fallback remains available for commands and options not yet native.
* the ``picard`` shim supports mixed legacy/new execution during rollout.

``riker`` is often an easier fit for new QC-first tooling, but it has a stronger
interface migration cost because it is not a Picard command-level replacement.

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
