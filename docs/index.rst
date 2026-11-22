Turbo Picard
============

Native Rust implementations of **39 Picard commands** with familiar CLI names,
``KEY=VALUE`` options, and Picard-checked outputs—plus transparent upstream
Picard fallback for the remaining 88 commands.

.. code-block:: bash

   turbo-picard MarkDuplicates I=input.bam O=marked.bam M=metrics.txt

Measured comparisons show **~46x faster and 11.7x less RAM than Picard 3.5.0**
(and **1.3x faster with 4.0x less RAM than Riker 0.5.0**) on 4x GRCh38 whole-genome
``CollectWgsMetrics``, and **5.15x–19.0x faster with 11x–76x less RAM than
Picard 3.4.0** on matched 1-CPU ``MarkDuplicates`` runs (faster and ~4x less RAM
than FastDup, and 1.88x faster than a ``samtools markdup`` pipeline). Start with
one slow command, verify outputs on a representative input, and switch only when
required outputs agree.

The saved small-fixture benchmark suite also records ``32/32`` parity-checked
command runs with a ``22.88x`` floor speedup, ``84.52x`` geometric mean
speedup, and ``272.12x`` top speedup on its documented fixtures. Because JVM
startup contributes heavily on small inputs, consult :doc:`benchmarks`,
:doc:`turbo-picard-vs-riker`, and :doc:`parity` alongside whole-genome
measurements before sizing production pipelines.

Research and evidence
---------------------

`Cheerful Duck Research <https://cheerfulduck.com/research>`_ publishes our
bioinformatics software investigations, reproduction material, and corrections.
Begin with :doc:`compatibility-contract` to define the outputs that matter to
your workflow, then use :doc:`real-data-evaluation` to compare them.

Start here
----------

.. grid:: 1 1 2 2
   :gutter: 2

   .. grid-item-card:: New user
      :link: quickstart
      :link-type: doc

      Install, check the executable, and run your first familiar command.

   .. grid-item-card:: Pipeline owner
      :link: real-data-evaluation
      :link-type: doc

      Compare an actual task with isolated outputs and inspectable evidence.

   .. grid-item-card:: Command coverage
      :link: commands
      :link-type: doc

      Check native support, partial support, and the boundary with upstream Picard.

   .. grid-item-card:: Compare alternatives
      :link: turbo-picard-vs-riker
      :link-type: doc

      Review real-genome timings and contracts against Riker, FastDup, and samtools.

   .. grid-item-card:: Coding agent
      :link: agentic-coders
      :link-type: doc

      Discover command scope, use structured arguments, and avoid silent delegation.

Packaging, detailed benchmarks and migration references are in the guide below.

.. toctree::
   :maxdepth: 2
   :caption: User Guide

   quickstart
   agentic-coders
   is-this-for-you
   first-command
   evaluation-playbook
   real-data-evaluation
   independent-evaluation
   picard-markduplicates-slow-memory-alternatives
   picard-markduplicates-errors
   use-cases
   picard-alternatives
   picard-vs-turbo-picard
   turbo-picard-vs-riker
   faq
   adoption
   parity
   compatibility-contract
   production-readiness
   fallback
   commands
   benchmarks
   performance
   citation
   joss-submission
   packaging
   troubleshooting

.. toctree::
   :maxdepth: 2
   :caption: Project

   development
