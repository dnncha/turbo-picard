Why Picard MarkDuplicates can be slow
-------------------------------------

There are two different performance problems, and it helps to separate them.

The first is the cost of the work itself. ``MarkDuplicates`` has to inspect relationships between reads, choose representatives, track duplicate families, write a new alignment file, generate metrics, and often create an index. Other Picard commands sort records, scan bases, read a reference, or compress and decompress BAM or CRAM data. On large files, those costs dominate.

The second is the cost of crossing the task boundary. Every Picard invocation brings up a JVM, loads classes and libraries, establishes a heap, and tears the process down again. That fixed cost matters little in a three-hour job. It matters a great deal in a 200-millisecond native job, a test suite, a highly scattered workflow, or a service that launches small commands repeatedly.

Picard's own FAQ gives a useful production-shaped reference point. Broad describes ``MarkDuplicates`` as memory-intensive, says it commonly ran the command with a 2 GB Java heap and a 10 GB hard memory limit, and gives historical examples of about one hour for an 8.6 GB file containing 63 million records and about 2.5 hours for a 20 GB file containing 133 million records. [1]_

Those figures are not a condemnation of Picard. They show what a workflow owner must plan for: wall time, heap, memory outside the heap, file handles, scratch space and I/O.

The operational pain appears in familiar searches:

-  “Picard MarkDuplicates slow”
-  “Picard MarkDuplicates OutOfMemoryError”
-  “Picard Java heap space”
-  “MAX_RECORDS_IN_RAM”
-  “Picard process killed by Slurm”
-  “samtools markdup vs Picard”
-  “Picard MarkDuplicates alternative”
-  “multithreaded Picard MarkDuplicates”

These are not abstract complaints. Public support threads include jobs that run for more than two hours before a Java heap failure, jobs killed after exceeding a cluster memory limit, and users trying to trade heap, temporary files and file handles against one another. [2]_ [3]_

The JVM tax is real—but it is not the whole bill
------------------------------------------------

“Remove the JVM” is a useful engineering direction. It is not a complete performance model.

For small and repeated jobs, startup and baseline memory can dominate. For large alignment files, the expensive part may instead be sorting, duplicate-family bookkeeping, reference access, temporary-file traffic, or BAM compression and decompression.

AMD published a useful example in May 2026. It kept Picard 3.4.0 and the workflow logic intact, but changed the compression path on an AMD EPYC system. On its 34.7-million-read paired-end dataset, elapsed time fell from 5.12 minutes to 2.85 minutes—a 44% reduction—because BAM I/O was a major part of the run. [4]_

That result matters because it prevents a lazy argument. Java is not “the problem” in every Picard job. Sometimes the codec is the problem. Sometimes the disk is the problem. Sometimes the algorithm is the problem. Sometimes the fixed process cost is the problem.

A trustworthy replacement should measure all of them:

-  wall-clock time;
-  CPU time;
-  peak resident memory, not only ``-Xmx``;
-  temporary disk and file-handle use;
-  compression settings;
-  input shape, coverage and duplicate-family structure;
-  output parity.

Speed without that context is marketing.

Fixed costs multiply across a pipeline
--------------------------------------

Turbo Picard's current saved benchmark suite contains 32 command-level checks against Picard 3.4.0. On those documented fixtures, all 32 parity checks pass. The saved speedups range from 22.88× to 272.12×, with an 84.52× geometric mean. The saved ``MarkDuplicates`` fixture records a 28.70× speedup. [5]_

Those are striking numbers. They are also small-fixture numbers.

The useful lesson is not that a whole genome will suddenly run 84 times faster. It is that Picard's fixed runtime boundary can be expensive relative to the useful work in short commands.

If we add the displayed median times for the 32 saved fixture runs, Picard uses 61.31 seconds and Turbo Picard uses 0.97 seconds. Repeat that same tiny suite 1,000 times and the totals become about 17.0 hours and 16.2 minutes. [6]_

That is a model of CI, sharding and repeated task launches—not a model of whole-genome throughput. It shows why small costs become material when a workflow engine multiplies them by commands, samples, shards, retries and test runs.

The same distinction applies to memory. A focused saved ``MarkDuplicates`` copy-path fixture reports median peak RSS of roughly 1.2 GB for Picard and 8.7 MB for Turbo Picard—about 140 times lower on that fixture. [7]_

Again, that is not a promise that every production run will use 8.7 MB. Large inputs and advanced options need more state. The point is that a native command can begin with a much smaller baseline, leaving more of the machine for the work itself.

That matters on a laptop. It matters more in a scheduler. Memory requests determine how many tasks fit on a node, how long they wait in a queue, and whether a burst of shards turns into a burst of evictions or out-of-memory kills.

“More heap” is not a complete answer
------------------------------------

When Picard runs out of memory, the first instinct is often to raise ``-Xmx``. Sometimes that is correct. Sometimes it makes another constraint worse.

Picard exposes controls such as ``MAX_RECORDS_IN_RAM``, ``SORTING_COLLECTION_SIZE_RATIO``, ``MAX_FILE_HANDLES_FOR_READ_ENDS_MAP`` and ``TMP_DIR``. Increasing in-memory records can reduce temporary files and file handles, but it consumes more RAM. Reducing them lowers memory pressure, but can increase disk traffic. The right setting depends on the command, input and storage system. [8]_

The heap is also not the process. Native libraries, buffers, memory-mapped files, compression and the JVM itself sit outside ``-Xmx``. A cluster can kill a process even when the configured heap appears to fit.

This is why the operational unit should be the measured process, not the heap flag. Use ``/usr/bin/time -v``, cgroup metrics or scheduler telemetry. Record peak RSS, scratch use and wall time together. A smaller ``-Xmx`` that triggers heavy spilling may save RAM and lose more in I/O. A larger heap may run faster and reduce node density.

There is no universal magic value.
