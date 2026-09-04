turbo-picard documentation
==========================

``turbo-picard`` is for bioinformatics teams that already use Picard and want
to evaluate a known preprocessing or QC bottleneck without redesigning the
surrounding task interface.

It keeps the command shape people already know:

.. code-block:: bash

   picard MarkDuplicates I=input.bam O=marked.bam M=metrics.txt

Native and partial-native commands run in Rust. Other Picard 3.4.0 commands can
delegate to upstream Picard only when fallback is available; delegation is not
native support. The command matrix records the exact scope, known caveats, and
fallback behaviour. The intended evaluation is incremental: choose one command,
compare the outputs your workflow consumes on representative data, then decide
whether to use that command.

The saved benchmark suite records ``32/32`` parity-checked command runs, a
``22.88x`` floor speedup, ``84.52x`` geometric mean speedup, and ``272.12x`` top
speedup on its documented fixtures. Those results describe the saved commands,
options, inputs, and machine profile; they are not a prediction for another
workflow. See :doc:`benchmarks` and :doc:`parity` before using them in an
evaluation.

Start here
----------

.. grid:: 1 1 2 2
   :gutter: 2

   .. grid-item-card:: Why Picard jobs get slow
      :link: picard-markduplicates-slow-memory-alternatives
      :link-type: doc

      Understand JVM costs, memory pressure, I/O, competing tools, and the
      safest way to test one replacement command.

   .. grid-item-card:: New user
      :link: quickstart
      :link-type: doc

      Install from PyPI, check the two entrypoints, and run a first
      Picard-style command.

   .. grid-item-card:: Is this for you?
      :link: is-this-for-you
      :link-type: doc

      Decide quickly whether this is worth evaluating in your workflow at all.

   .. grid-item-card:: Pipeline owner
      :link: evaluation-playbook
      :link-type: doc

      Follow the shortest path from first interest to trial, review, and team
      rollout.

   .. grid-item-card:: Coding agent
      :link: agentic-coders
      :link-type: doc

      Make a machine-readable tool decision, preserve the workflow boundary,
      and emit a reviewable trial contract.

   .. grid-item-card:: Use cases
      :link: use-cases
      :link-type: doc

      See the workflow situations where this package is most likely to help.

   .. grid-item-card:: Command lookup
      :link: commands
      :link-type: doc

      See which Picard commands are native, partly native, or fallback-only.

   .. grid-item-card:: Picard vs turbo-picard
      :link: picard-vs-turbo-picard
      :link-type: doc

      See what stays the same, what changes, and when to stay with Picard.

   .. grid-item-card:: Picard alternatives
      :link: picard-alternatives
      :link-type: doc

      Compare turbo-picard, samtools, Sambamba, SAMBLASTER, FastDup and riker
      by workflow and input contract.

   .. grid-item-card:: turbo-picard vs riker
      :link: turbo-picard-vs-riker
      :link-type: doc

      Compare workflow contracts and evaluation boundaries without assuming a
      universal performance ordering.

   .. grid-item-card:: FAQ
      :link: faq
      :link-type: doc

      Get direct answers to the common evaluation and rollout questions.

   .. grid-item-card:: First command
      :link: first-command
      :link-type: doc

      Pick the best first Picard step to trial instead of guessing.

   .. grid-item-card:: Packaging
      :link: packaging
      :link-type: doc

      Understand PyPI, the optional ``picard`` shim, citation boundaries, and
      the Bioconda release path.

.. toctree::
   :maxdepth: 2
   :caption: User Guide

   quickstart
   agentic-coders
   is-this-for-you
   first-command
   evaluation-playbook
   picard-markduplicates-slow-memory-alternatives
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
