.. meta::
   :description: Why Picard MarkDuplicates can become slow or memory-heavy, how samtools, FastDup, riker and Turbo Picard differ, and how to test a compatible replacement safely.
   :keywords: Picard MarkDuplicates slow, Picard MarkDuplicates memory, Picard MarkDuplicates alternative, samtools markdup vs Picard, Turbo Picard, BAM duplicate marking

Why Picard jobs get slow and memory-hungry—and how to test a replacement safely
===============================================================================

*The JVM is only part of the bill. The rest is process startup, heap headroom, temporary I/O, compression, and the risk of changing a workflow that already works.*

.. rubric:: Engineering note

**Published:** 4 September 2026 · **Last verified:** 4 September 2026 · **Evidence baseline:** Turbo Picard 0.1.12 against Picard 3.4.0

.. important::

   **Accelerate one Picard step without betting the pipeline.** Compatibility is the product. Speed is the evidence. Fallback is the risk control.

I did not build `Turbo Picard <https://turbo-picard.readthedocs.io/>`__ because Java is bad.

Picard is trusted for good reasons. It has spent years turning awkward details of SAM, BAM, CRAM and VCF processing into commands that research and production pipelines can depend on. Its behaviour—not merely its command names—has become part of the contract of many genomics workflows.

That is precisely why replacing it is hard.

A faster program is not useful if it marks a different read as the duplicate, changes a tag, emits a subtly different metric, forgets an index, or succeeds where the old command would have failed. In genomics, “the file looks fine” is not a compatibility test.

But the cost is also real. A short Picard task starts a JVM. A large task needs heap, native memory and temporary storage. A scattered cohort may start hundreds or thousands of separate Picard processes. The pipeline pays that bill before it gets any scientific value from the command.

Turbo Picard is an attempt to remove that cost without asking a team to redesign the pipeline first. It keeps familiar Picard command names and ``KEY=VALUE`` arguments, implements a documented subset natively in Rust, and can send unsupported work back to upstream Picard.

The right question is not: **“Is Turbo Picard faster?”**

It is: **“Can this exact Picard step run faster and leaner on my data while preserving every output my workflow depends on?”**

That is a question we can test.

.. include:: _includes/picard-performance-01.inc

.. include:: _includes/picard-performance-02.inc

.. include:: _includes/picard-performance-03.inc

.. include:: _includes/picard-performance-04.inc

.. include:: _includes/picard-performance-references.inc
