Picard MarkDuplicates errors: heap space, GC overhead and disk space
====================================================================

Most failed ``MarkDuplicates`` runs end in one of five messages. Each one
points at a different limit, and a bigger ``-Xmx`` fixes only some of them.
Find your message below, apply the Picard fix first, and consider a native
path only when the step keeps failing or keeps costing too much.

.. contents:: On this page
   :local:
   :depth: 1

``java.lang.OutOfMemoryError: Java heap space``
-----------------------------------------------

**What it means.** The JVM heap filled up. ``MarkDuplicates`` keeps read-end
records and sorting buffers on the heap, and deep libraries or large
duplicate families make those structures grow.

**Fix it in Picard.**

* Raise the heap, for example ``java -Xmx16g -jar picard.jar MarkDuplicates ...``.
  Leave headroom below the job or container limit, because the JVM also uses
  memory outside the heap.
* Lower ``MAX_RECORDS_IN_RAM`` so sorting spills to disk sooner, and point
  ``TMP_DIR`` at fast local scratch with enough free space.
* Do not set ``READ_NAME_REGEX=null`` to save memory unless you mean to. It
  turns off optical-duplicate detection and changes the result.

``java.lang.OutOfMemoryError: GC overhead limit exceeded``
----------------------------------------------------------

**What it means.** The heap is almost full and the JVM is spending nearly all
its time on garbage collection. It is the same underlying problem as
``Java heap space``, caught earlier.

**Fix it in Picard.** Apply the same fixes as above. Adding
``-XX:-UseGCOverheadLimit`` only hides the warning. The job usually runs longer
and then fails with ``Java heap space``.

Killed, exit code 137, or a scheduler out-of-memory kill with no Java error
----------------------------------------------------------------------------

**What it means.** The whole process went over the job, cgroup or container
memory limit. ``-Xmx`` caps only the heap, so a heap set to the full
allocation leaves nothing for compression buffers, thread stacks and other
native memory.

**Fix it in Picard.** Set ``-Xmx`` to roughly 75–85% of the allocation, record
the peak resident memory (``/usr/bin/time -v`` on Linux, or your scheduler's
accounting), and size the next request from that figure rather than from the
heap setting.

``No space left on device`` or ``java.io.IOException`` while writing temporary files
-------------------------------------------------------------------------------------

**What it means.** The sorting spill files filled the temporary directory.
Picard uses ``java.io.tmpdir`` by default, which is often a small ``/tmp``
inside a container or on a compute node.

**Fix it in Picard.** Set ``TMP_DIR=/path/to/local/scratch`` explicitly and
check free space and inode quota before the run. In Nextflow, Snakemake or
WDL, make sure that directory is mounted into the task container.

``Too many open files``
-----------------------

**What it means.** ``MarkDuplicates`` opened more spill files than the process
file-handle limit allows.

**Fix it in Picard.** Raise the limit (``ulimit -n``) where you can, or lower
``MAX_FILE_HANDLES_FOR_READ_ENDS_MAP`` below it. A larger
``MAX_RECORDS_IN_RAM`` also produces fewer, larger spill files, at the cost of
more heap.

When a native path is worth testing
-----------------------------------

If the step still fails, or the fixes make it slow, ``turbo-picard
MarkDuplicates`` takes the same ``KEY=VALUE`` arguments and runs without a JVM.
For a single BAM, a single CRAM with an explicit reference, or inputs that are
already globally coordinate-ordered, it uses a disk-backed two-pass plan
rather than holding every alignment in memory. ``TMP_DIR`` and
``MAX_RECORDS_IN_RAM`` still apply and still need real scratch space. Neither
is a hard ceiling on total memory.

Run it beside Picard on the same input and compare duplicate flags, metrics
and optical decisions before you switch:

.. code-block:: bash

   turbo-picard trial MarkDuplicates I=sample.bam O=marked.bam M=metrics.txt

See :doc:`picard-markduplicates-slow-memory-alternatives` for where the time
and memory go, :doc:`picard-alternatives` for other duplicate-marking tools,
and :doc:`real-data-evaluation` for a full comparison protocol.
