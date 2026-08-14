Performance Notes
=================

``turbo-picard`` gets most of its speed from avoiding JVM startup, running common
Picard operations natively, and keeping BAM/CRAM I/O on mature HTSlib code.
Those choices matter more for current Picard-style preprocessing than sending
work to a GPU just because one is present.

They also explain the scalability story. Faster wall time matters, but so does
the ability to fan out many Picard-shaped tasks without paying Picard-scale JVM
startup and memory costs on every shard. The saved benchmark suite currently
shows a ``6.86x`` floor speedup, ``24.94x`` geometric mean speedup, and
``94.36x`` top speedup against Picard 3.4.0. Those are saved-fixture results,
not whole-genome guarantees.

For a single BAM, an explicit-reference CRAM, or multiple alignment inputs that
are already globally coordinate-ordered, the native path builds a disk-backed
two-pass duplicate plan and replays the original alignment files for final
output. The bounded path includes ``BARCODE_TAG`` and
``READ_ONE_BARCODE_TAG``/``READ_TWO_BARCODE_TAG`` grouping, Picard-compatible
optical-family discovery with the default or a validated three-capture-group
``READ_NAME_REGEX``, and ``REMOVE_SEQUENCING_DUPLICATES``. Explicit
``READ_NAME_REGEX=null`` disables optical discovery and retains Picard's
no-optical metrics behavior. Duplicate-set tagging (``DS``/``DI``) is carried
through bounded replay for paired duplicate families. Multiple streams that are
not already globally coordinate-ordered also fall back to the compact path so
the existing output-order contract is preserved. A local adversarial
300,000-read run with duplicate families of 4,096 records passed the synthetic
Picard parity comparator and measured ``75,038,720`` bytes of peak RSS for the
external plan versus ``110,592,000`` bytes for the compact path on the same
machine, with ``TMP_DIR`` pointed at dedicated scratch space. The external plan
took ``1.09`` seconds for the Turbo-Picard command in that direct resource run;
the suite profile remains the comparable timing source for Picard speedups.
Reproduce the timing profile with:

.. code-block:: bash

   python3 tools/bench_suite.py --only markduplicates --repeats 3 --skip-build \
     --markduplicates-reads 300000 --markduplicates-family-size 4096 \
     --profile-output /tmp/turbo-picard-markduplicates-profile.json

   # The suite-level RSS includes comparator child processes. For a
   # tool-specific reading, wrap target/release/picard MarkDuplicates with
   # ``/usr/bin/time -l`` on the generated BAM.

This is bounded external planning for one documented option scope, not a claim
that every MarkDuplicates mode is constant-memory or production-approved.
Production-scale WGS, WES, UMI, optical-heavy, CRAM, and multi-library evidence
still belongs in the pinned evidence workflow described in
:doc:`production-readiness`.

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

For larger runs, the I/O thread policy can now be tuned by role:

.. code-block:: bash

   TURBO_PICARD_READER_THREADS=4 \
   TURBO_PICARD_WRITER_THREADS=8 \
   TURBO_PICARD_INDEX_THREADS=8 \
   turbo-picard MarkDuplicates I=input.bam O=marked.bam M=metrics.txt

``TURBO_PICARD_THREADS`` remains the broad override. The role-specific variables
win when set. ``TURBO_PICARD_MAX_THREADS`` caps the automatic defaults, and
``TURBO_PICARD_THREADS=auto`` returns to the built-in policy. Commands that use
a dedicated application reader thread, such as large WGS/QC paths, use a smaller
``htslib_pipeline_reader_threads`` value so BGZF workers do not fight the
pipeline thread for the same CPU budget. Set
``TURBO_PICARD_PIPELINE_READER_THREADS`` only when profiling shows that this
specialized path needs a different value from ``TURBO_PICARD_READER_THREADS``.

This helps most when the command is spending real time in BAM or CRAM
compression, decompression, reference-backed CRAM work, or BAI generation after
MarkDuplicates or other indexed BAM outputs. It will not make a tiny test file
much faster, and it will not fix slow storage.

Without explicit thread variables, readers, writers, index construction, and
pipelined readers each pick a bounded automatic default from available CPU
parallelism. Reader defaults still cap at eight workers, writer and index
defaults cap at twelve workers, and pipelined readers cap lower because the
command is already overlapping I/O with application-level processing.

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

``CollectWgsMetrics`` keeps only the live coordinate window that future
coordinate-sorted records can still overlap. Finalized loci are folded into the
coverage histogram and discarded, so depth state is independent of chromosome
length. The collector loads reference lengths from ``.fai`` when present (no
full-genome FASTA load), applies Picard-style mate overlap exclusion with
``FxHashMap`` mate pairing and packed overlap bitmaps, and overlaps BGZF decode
with pileup on BAM/CRAM inputs via a dedicated reader thread. These paths retain
Picard-identical summary and histogram output for the validated option scope.

``CollectMultipleMetrics`` on BAM/CRAM inputs runs all selected collectors in
one HTSlib pass (the same idea as riker's ``multi`` command) instead of
re-opening and re-scanning the alignment file once per ``PROGRAM=``. When two
or more collectors are active, a dedicated reader thread fills recycled
128-record batches while persistent collector workers process in-flight batches
asynchronously. The default now scales up to six collector workers when CPUs and
active collectors are available. Override with ``TURBO_PICARD_CMM_THREADS=N`` or
set ``TURBO_PICARD_CMM_THREADS=auto`` to force the built-in policy. SAM inputs
still use per-program passes so the existing SAM-text fast paths stay available.

Profiling benchmark runs
------------------------

Use the suite profiler when working on speed claims:

.. code-block:: bash

   python3 tools/bench_suite.py --repeats 3 --skip-build \
     --profile-output benchmarks/runs/bench-suite-profile.json

The JSON artifact records per-command wall time, wrapper CPU time, observed RSS,
thread-related environment variables, read count, parity result, and per-repeat
details. Keep these generated artifacts under ``benchmarks/runs/`` unless a
specific release evidence bundle is being promoted.

For focused regression work, run only the command family under investigation:

.. code-block:: bash

   python3 tools/bench_suite.py --repeats 5 --skip-build \
     --only revertsam,setnmmdanduqtags,qualityscoredistribution \
     --profile-output benchmarks/runs/floor-command-profile.json

``--only`` accepts benchmark labels from ``tools/bench_suite.py`` and can be
repeated or comma-separated. This is the preferred loop when auditing the saved
suite floor, because it preserves Picard parity checks while avoiding unrelated
benchmark setup and runtime.

The MarkDuplicates synthetic benchmark now defaults to BAM so it exercises the
native production path. Stress large duplicate families and optical-name
tracking explicitly with:

.. code-block:: bash

   python3 tools/bench_suite.py --repeats 5 --skip-build \
     --only markduplicates --markduplicates-reads 1000000 \
     --markduplicates-family-size 4096 \
     --markduplicates-read-name-regex null \
     --profile-output benchmarks/runs/markduplicates-highdup-profile.json

The explicit ``READ_NAME_REGEX=null`` setting selects the documented bounded
no-optical variant. Omit the option, or pass a validated three-capture-group
Picard regex, to exercise bounded optical-family discovery. Record both the
normal family-size profile and this adversarial profile; the large-family result
is a scalability guardrail, not a replacement for WGS/WES evidence.

``CollectGcBiasMetrics`` loads one reference contig at a time via ``.fai`` seek
for read-time GC windows and precomputes genome-window counts without keeping
the full reference in memory.

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

It reports the active policy, HTSlib worker-thread counts, and whether a CUDA,
ROCm, or Metal runtime appears to be present. Current release builds still use
the CPU backend for Picard-compatible work:

.. code-block:: text

   backend=cpu
   policy=auto
   htslib_worker_threads=4
   htslib_reader_threads=4
   htslib_writer_threads=4
   htslib_pipeline_reader_threads=2
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
