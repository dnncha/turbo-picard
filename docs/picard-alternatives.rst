Picard alternatives for bioinformatics workflows
================================================

.. note::

   Looking for a quick side-by-side summary? See `Choose a tool for your workflow <https://turbo-picard.readthedocs.io/en/latest/compare/>`_ and :doc:`turbo-picard-vs-riker`.

.. meta::
   :keywords: Picard alternatives, bioinformatics, duplicate marking, MarkDuplicates, samtools, Riker, FastDup, SAM, BAM, CRAM, sequencing QC

The right alternative to Broad Picard depends on what can change in your pipeline
and which exact outputs downstream tools expect.

``turbo-picard`` is built for existing Picard-shaped workflows: it keeps Picard
command names and ``KEY=VALUE`` options across **39 native commands**, matches
Picard outputs verified in CI against Picard 3.4.0 and 3.5.0, and transparently
delegates the remaining **88 commands** to upstream Picard when configured.

Measured head-to-head results
-----------------------------

* **Whole-genome WGS QC (``CollectWgsMetrics`` on 1000 Genomes ``HG02675``
  GRCh38, 6 threads):** On 4x WGS, ``turbo-picard`` finished in **21.8–22.6 s /
  125 MB RSS** (**~46x faster and 11.7x less RAM than Picard 3.5.0** at
  1,033.8 s / 1,468 MB, and **1.3x faster with 4.0x less RAM than ``riker wgs``
  0.5.0** at 29.7 s / 504 MB) while matching Picard on every metric field and
  histogram row. On 15x WGS, ``turbo-picard`` took **65.4–82.6 s / 274 MB**
  vs ``riker wgs`` **78.2–79.7 s / 518 MB**. Pre-PR #76 30x WGS ran in
  193.5 s / 412 MB vs Riker 147.4 s / 510 MB due to whole-contig capped
  fallback, now replaced by targeted cluster reprocessing in PR #76.
* **Matched 1-CPU ``MarkDuplicates`` (Linux x86_64, BAM compression level 5):**
  On ``paired-600k``, ``turbo-picard`` finished in **1.054 s / 94.1 MiB** with
  full Picard parity—**5.15x faster & 11.2x less RAM** than Picard 3.4.0
  (5.423 s / 1,052.6 MiB), faster and **4.1x less RAM** than FastDup
  (1.131 s / 383.8 MiB, differs from Picard), and **1.88x faster** than a
  ``samtools markdup`` pipeline (1.982 s / 235.7 MiB, differs from Picard). On
  ``gatk-na12878-mito``, ``turbo-picard`` ran in **0.141 s / 9.5 MiB** vs
  Picard 2.678 s / 721.3 MiB (**19.0x faster**) and FastDup 0.201 s.
* **Small-fixture 32-command suite:** Across 32 saved parity-checked fixtures
  against Picard 3.4.0, ``turbo-picard`` records **22.88x** to **272.12x**
  speedups (**84.52x** geometric mean).

Comparison by workflow
----------------------

.. list-table::
   :header-rows: 1
   :widths: 18 29 25 28

   * - Tool
     - Best-matched workflow
     - Interface and input contract
     - Important comparison boundary
   * - ``turbo-picard``
     - Existing Picard preprocessing, QC, sorting, FASTQ, and VCF tasks
     - Picard command names and ``KEY=VALUE`` arguments across BAM, CRAM, SAM,
       FASTQ, and VCF
     - Native coverage is command- and option-specific across 39 native and 88
       fallback commands; check :doc:`commands` and :doc:`parity`
   * - `samtools <https://www.htslib.org/>`_
     - Standard SAM/BAM/CRAM manipulation and HTSlib pipelines
     - ``samtools markdup`` expects a name-collated or name-sorted ``fixmate -m``
       stage followed by coordinate sorting
     - Duplicate tags and supplementary-read handling are not identical to
       every Picard mode
   * - `Sambamba <https://lomereiter.github.io/sambamba/docs/sambamba-markdup.html>`_
     - Parallel BAM processing and duplicate marking on coordinate-sorted BAM
     - Its own subcommands and options rather than Picard-compatible arguments
     - Validate duplicate flags, metrics, and downstream parser expectations
       before changing tools
   * - `SAMBLASTER <https://github.com/GregoryFaust/samblaster>`_
     - Streaming duplicate marking directly after an aligner
     - Read-id-grouped SAM stream, normally before conversion and coordinate
       sorting
     - Uses a different stage and input contract from a coordinate-sorted
       Picard ``MarkDuplicates`` task
   * - `dupblaster <https://github.com/fulcrumgenomics/dupblaster>`_
     - Streaming, library-aware duplicate marking straight after the aligner
     - Custom CLI; expects query-grouped (read-name-grouped) input
     - Needs a pipeline change when input is coordinate-sorted
   * - `FastDup <https://github.com/zzhofict/FastDup>`_
     - Multi-threaded duplicate marking on large coordinate-sorted BAM inputs
     - Dedicated FastDup CLI and narrower duplicate-marking scope
     - Higher memory footprint (~384 MiB on 600k pairs) and representative-read
       differences vs Picard
   * - `riker <https://github.com/fulcrumgenomics/riker>`_
     - New sequencing-QC workflows that prefer Riker's subcommands, TSV outputs,
       or RNA-seq metrics
     - QC-specific subcommands such as ``wgs``, ``alignment``, and ``multi``
     - No duplicate marking, sorting, or SAM/VCF utility commands; intentional
       metric differences from Picard and higher RSS on 4x–15x WGS

Specialized preprocessing alternatives
--------------------------------------

`fgumi <https://github.com/fulcrumgenomics/fgumi>`_ covers UMI extraction,
grouping, and consensus workflows; Turbo Picard's explicit barcode-tag grouping
is narrower than a full UMI pipeline. `mako <https://github.com/fg-labs/mako>`_
provides a standalone SAM/BAM sorter with coordinate, queryname, and
template-coordinate orders.

Which tool should replace Picard?
---------------------------------

* Choose ``turbo-picard`` when retaining Picard CLI flags, checked output
  formats, lower peak RAM, and one-step-at-a-time pipeline compatibility matters.
* Choose ``samtools`` or ``Sambamba`` when the workflow is already built around
  their commands and downstream tag conventions.
* Choose ``dupblaster`` or ``SAMBLASTER`` when duplicate marking belongs inside
  the aligner stream before coordinate sorting.
* Evaluate ``riker`` for greenfield QC-only pipelines that use RNA-seq metrics
  or prefer Riker's TSV schema.

See :doc:`benchmarks`, :doc:`performance`, :doc:`picard-vs-turbo-picard`, and
:doc:`turbo-picard-vs-riker` for reproduction commands and full logs.

Primary references
------------------

* `Broad Picard <https://github.com/broadinstitute/picard>`_
* `samtools duplicate-marking workflow <https://www.htslib.org/algorithms/duplicate.html>`_
* `Sambamba markdup documentation <https://lomereiter.github.io/sambamba/docs/sambamba-markdup.html>`_
* `SAMBLASTER paper <https://doi.org/10.1093/bioinformatics/btu314>`_
* `FastDup paper <https://doi.org/10.1093/bioinformatics/btaf633>`_
* `riker repository <https://github.com/fulcrumgenomics/riker>`_
