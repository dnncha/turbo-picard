Picard alternatives for bioinformatics workflows
================================================

.. note::

   Looking for a practical starting point? See `Workflow-focused tool comparison <https://turbo-picard.readthedocs.io/en/latest/compare/>`_.



.. meta::
   :description: Compare turbo-picard, samtools, Sambamba, SAMBLASTER, FastDup and riker for Picard-compatible bioinformatics, duplicate marking and sequencing-QC workflows.
   :keywords: Picard alternatives, bioinformatics, duplicate marking, MarkDuplicates, SAM, BAM, CRAM, sequencing QC

The right alternative to Broad Picard depends on the command boundary. Duplicate
marking, BAM sorting, streaming aligner output and sequencing QC are different
workloads, and the tools below do not expose interchangeable inputs or outputs.

``turbo-picard`` is designed for existing Picard-shaped workflows. It retains
Picard command names and ``KEY=VALUE`` arguments, runs supported commands in
native Rust, and can delegate commands outside the native scope to upstream
Picard. This makes it possible to evaluate one existing task without first
rewriting the surrounding WDL, Nextflow, Snakemake or shell interface.

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
     - Existing Picard preprocessing, QC and file-manipulation tasks
     - Picard command names and ``KEY=VALUE`` arguments; BAM, CRAM, SAM,
       FASTQ, VCF and metrics outputs depend on the command
     - Native coverage is command- and option-specific; check the
       :doc:`command matrix <commands>` and :doc:`parity evidence <parity>`
   * - `samtools <https://www.htslib.org/>`_
     - Standard SAM/BAM/CRAM manipulation and pipelines already built around
       HTSlib tools
     - ``samtools markdup`` expects a name-collated or name-sorted ``fixmate -m``
       stage followed by coordinate sorting
     - Duplicate tags and supplementary-read handling are not identical to
       every Picard mode
   * - `Sambamba <https://lomereiter.github.io/sambamba/docs/sambamba-markdup.html>`_
     - Parallel BAM processing and duplicate marking on coordinate-sorted BAM
     - Its own subcommands and options rather than Picard-compatible arguments
     - Validate duplicate flags, metrics and downstream parser expectations
       before changing tools
   * - `SAMBLASTER <https://github.com/GregoryFaust/samblaster>`_
     - Streaming duplicate marking directly after an aligner, with optional
       discordant and split-read extraction
     - Read-id-grouped SAM stream, normally before conversion and coordinate
       sorting
     - It uses a different stage and input contract from a coordinate-sorted
       Picard ``MarkDuplicates`` task
   * - `FastDup <https://github.com/zzhofict/FastDup>`_
     - Multi-threaded duplicate marking on large coordinate-sorted BAM inputs
     - Dedicated FastDup command line and a narrower duplicate-marking scope
     - Its authors report Picard-compatible duplicate marking; reproduce that
       result on the exact library types and options used by the workflow
   * - `riker <https://github.com/fulcrumgenomics/riker>`_
     - New sequencing-QC workflows that prefer riker's command model and TSV
       outputs
     - QC-specific subcommands such as ``wgs``, ``alignment`` and ``multi``
     - It does not replace Picard duplicate marking, sorting or general
       SAM/VCF utility commands

Which tool should replace Picard?
---------------------------------

For an existing Picard task, start with the output contract:

* Use ``turbo-picard`` when retaining Picard command names, arguments and
  checked output formats is the primary constraint.
* Use ``samtools`` or ``Sambamba`` when the workflow already follows their BAM
  preparation and command contracts.
* Use ``SAMBLASTER`` when duplicate marking belongs in the streaming alignment
  pipeline rather than in a later coordinate-sorted BAM task.
* Evaluate ``FastDup`` when a dedicated, multi-threaded duplicate-marking stage
  matches the required library and output semantics.
* Evaluate ``riker`` for a new QC-only workflow where Picard-compatible task
  interfaces are not required.

No timing result transfers automatically between these shapes. A fair test uses
the same biological input, includes every required preparation stage, records
threads and tool versions, measures peak memory and temporary disk as well as
wall time, and compares the outputs consumed downstream.

Measured turbo-picard evidence
------------------------------

The saved public suite compares 32 native ``turbo-picard`` commands with Picard
3.4.0 and requires each documented parity check to pass before reporting time.
The current saved results range from ``22.88x`` to ``272.12x`` faster on those
fixtures, with an ``84.52x`` geometric mean. These are fixture-specific results,
not a claim that every dataset or option has the same speedup.

For direct QC overlap with riker, the repository contains two small-input smoke
profiles. They place ``turbo-picard`` ``2.10x`` and ``2.14x`` ahead on the saved
WGS-only and WGS-bundle runs. Use the WGS-scale protocol before drawing a
production conclusion.

For duplicate marking, ``tools/bench_markduplicates_competitors.py`` records
commands, versions, executable digests, wall and CPU time, peak RSS, temporary
disk and streaming record parity for ``turbo-picard``, Picard, samtools,
Sambamba and FastDup. SAMBLASTER requires a separate end-to-end streaming
pipeline comparison because its input contract is different.

See :doc:`benchmarks`, :doc:`performance`,
:doc:`picard-vs-turbo-picard` and :doc:`turbo-picard-vs-riker` for the saved
results, reproduction commands and limitations.

Primary references
------------------

* `Broad Picard <https://github.com/broadinstitute/picard>`_
* `samtools duplicate-marking workflow
  <https://www.htslib.org/algorithms/duplicate.html>`_
* `Sambamba markdup documentation
  <https://lomereiter.github.io/sambamba/docs/sambamba-markdup.html>`_
* `SAMBLASTER paper <https://doi.org/10.1093/bioinformatics/btu314>`_
* `FastDup paper <https://doi.org/10.1093/bioinformatics/btaf633>`_
* `riker repository <https://github.com/fulcrumgenomics/riker>`_
