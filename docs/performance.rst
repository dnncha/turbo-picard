Performance Notes
=================

``turbo-picard`` gets most of its speed from avoiding JVM startup, running common
Picard operations natively, and keeping BAM/CRAM I/O on mature HTSlib code.
Those choices matter more for current Picard-style preprocessing than sending
work to a GPU just because one is present.

Threading
---------

By default, ``turbo-picard`` lets HTSlib use a small number of worker threads
for BAM and CRAM reading and writing. You can set the count explicitly:

.. code-block:: bash

   TURBO_PICARD_THREADS=8 turbo-picard SortSam \
     I=reads.cram \
     O=sorted.cram \
     SORT_ORDER=coordinate \
     R=reference.fa

This helps most when the command is spending real time in BAM or CRAM
compression, decompression, reference-backed CRAM work, or BAI generation after
MarkDuplicates or other indexed BAM outputs. It will not make a tiny test file
much faster, and it will not fix slow storage.

Without ``TURBO_PICARD_THREADS``, readers and writers still pick a small default
thread count (up to four workers on multi-core hosts). Index creation uses the
same worker count instead of a single thread.

``SortSam`` streams BAM/CRAM inputs without loading them into memory when the
``@HD`` sort order already matches the requested ``SORT_ORDER``. Inputs with
``SO:unsorted`` or a mismatched header still get verified (or sorted) the same
way as before.

``MergeSamFiles`` uses the same header fast path when deciding whether every
input shard is already sorted for k-way streaming merge.

Metrics accumulation
--------------------

Cycle- and quality-oriented metrics commands resize their per-cycle buffers once
per read (or once per SAM line) instead of on every base or cycle index. Quality
score histograms use fixed ``[u64; 256]`` arrays. SAM-text alignment summaries
scan optional tags in one pass instead of allocating a per-line tag vector. These
choices keep parity with Picard output while avoiding repeated vector growth on
long reads.

GPU acceleration
----------------

The current native commands are mostly streaming, parsing, grouping, sorting,
small histogram updates, and BAM/CRAM codec work. That is usually a poor fit
for a GPU because records have variable length, the transfer cost is high, and
the code must run predictably on laptops, clusters, and Bioconda builders that
may not have CUDA, ROCm, or Metal.

There is a production-facing accelerator preflight:

.. code-block:: bash

   turbo-picard AccelerationStatus

It reports the active policy, HTSlib worker-thread count, and whether a CUDA,
ROCm, or Metal runtime appears to be present. Current release builds still use
the CPU backend for Picard-compatible work:

.. code-block:: text

   backend=cpu
   policy=auto
   htslib_worker_threads=4
   gpu_runtime=metal
   gpu_acceleration=not-enabled

If a workflow requires GPU acceleration, make that requirement explicit:

.. code-block:: bash

   TURBO_PICARD_ACCELERATOR=gpu-required turbo-picard AccelerationStatus

That command fails unless the installed build contains a production GPU backend.
This is deliberate. It gives workflow authors a clean guardrail without letting
a run silently fall back to CPU after someone asked for GPU-only execution.

The realistic GPU candidates are narrow:

* very large, independent per-base scans where the input is already in memory;
* compression/decompression through a stable GPU codec with CPU fallback;
* future metrics commands that can batch millions of bases without changing
  Picard-compatible output.

Those are worth benchmarking, but they should not be shipped until they beat
the threaded CPU path on representative BAM/CRAM inputs and keep the same
parity checks. Until that evidence exists, the production option is the
accelerator policy check above plus the threaded CPU/HTSlib path.

Where a GPU might actually help
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

The useful question is not "can this run on a GPU?" It is "does this command
have enough independent work per byte to pay for moving data to the device?"
For the current Picard-shaped workload, the answer is mixed:

.. list-table::
   :header-rows: 1

   * - Area
     - Fit
     - Why
   * - ``CollectWgsMetrics``
     - plausible
     - Coverage accumulation over large coordinate-sorted inputs has a lot of
       independent per-base work. The hard part is keeping Picard's exact
       filtering, interval, and histogram behavior while batching enough bases
       to make the device transfer worthwhile.
   * - ``SetNmMdAndUqTags``
     - plausible for long reads or large batches
     - NM, MD, and UQ are reference-backed per-alignment calculations. A GPU
       kernel could compare read bases with reference slices in bulk, but CIGAR
       handling and tag rendering still need careful CPU-side control.
   * - ``CollectGcBiasMetrics``
     - plausible for the reference pre-scan
     - Sliding-window GC counting over a large reference is regular work. Read
       placement and Picard-compatible summary formatting are less likely to
       benefit.
   * - duplicate optical-distance checks
     - plausible for very large duplicate sets
     - Distance checks inside big duplicate groups are independent. Most
       duplicate groups are not large enough to justify a GPU trip, so this
       would need a size threshold and a CPU path for normal cases.
   * - BAM/CRAM compression
     - possible only through a mature codec
     - Compression can benefit from accelerators, but ``turbo-picard`` should
       not replace HTSlib with a custom codec unless the output stays standard,
       tested, and faster on real pipeline files.
   * - ``SortSam`` and ``MergeSamFiles``
     - poor first target
     - These are dominated by file I/O, ordering, headers, and variable-length
       records. The current CPU/HTSlib path is the right place to optimize
       first.
   * - FASTQ conversion and simple SAM viewing
     - poor first target
     - These are mostly parsing, formatting, and streaming. GPU transfer would
       usually cost more than it saves.

The first serious GPU experiment should be ``CollectWgsMetrics`` or
``SetNmMdAndUqTags`` on large BAM/CRAM inputs, with a build-time feature flag
and a runtime threshold that leaves small files on CPU. The acceptance bar is
simple: same Picard-compatible output, same fallback behavior when no device is
available, and faster wall time than ``TURBO_PICARD_THREADS`` on representative
workflow data.

What to benchmark
-----------------

If you are deciding whether ``turbo-picard`` helps a workflow, benchmark the
actual command and file type you plan to switch:

* BAM input to BAM output;
* CRAM input to CRAM output with the same reference FASTA;
* the same storage and thread count used by the workflow runner;
* output comparisons from :doc:`parity`, not runtime alone.

The saved benchmark suite and real-data evidence are described in
:doc:`benchmarks`.
