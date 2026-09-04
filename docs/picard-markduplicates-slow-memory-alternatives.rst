.. meta::
   :description: Why Picard MarkDuplicates can run slowly or use large amounts of memory, where that cost comes from, which alternatives fit which workflows, and how to test Turbo Picard safely.
   :keywords: Picard MarkDuplicates slow, Picard MarkDuplicates memory, Picard MarkDuplicates alternative, samtools markdup vs Picard, Turbo Picard, BAM duplicate marking

Why Picard jobs get slow and memory-hungry, and how to test a replacement safely
================================================================================

*Picard has earned its place in genomics pipelines. That makes performance work harder, not easier: a faster command is useless if it quietly changes the scientific contract.*

.. rubric:: Engineering note

**Author:** Donncha O'Toole · **Published:** 4 September 2026 · **Rewritten:** 4 September 2026 · **Evidence baseline:** Turbo Picard 0.1.12 against Picard 3.4.0

I built `Turbo Picard <https://turbo-picard.readthedocs.io/>`__ after looking at a familiar kind of pipeline step: the command is trusted, the interface is everywhere, and replacing it sounds far riskier than putting up with the runtime.

The cheap pitch would be “Rust fast, Java slow.” I don't think that is a useful explanation.

Picard does real work. ``MarkDuplicates`` has to decide which reads belong to the same duplicate family, choose a representative, write a new alignment file, produce metrics and, often, build an index. On a large BAM or CRAM, that work can dwarf JVM startup. On a small command repeated across hundreds of shards, the fixed cost of starting Picard can be the thing you notice most.

So the question I care about is narrower than “Is Turbo Picard faster?” Can I replace one Picard step, on the data and options I actually use, and get the same result for less time or memory?

That is testable.

.. include:: _includes/picard-performance-01.inc

.. include:: _includes/picard-performance-02.inc

.. include:: _includes/picard-performance-03.inc

.. include:: _includes/picard-performance-04.inc

.. include:: _includes/picard-performance-references.inc
