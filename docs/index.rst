Turbo Picard
============

**Picard workflows. Native speed.**

Run selected Picard tools in Rust without redesigning the surrounding pipeline.
Keep familiar command names, ``KEY=VALUE`` arguments, and the metrics and output
contracts covered by the documented native scope.

.. code-block:: bash

   turbo-picard MarkDuplicates I=input.bam O=marked.bam M=metrics.txt

Start with the slow step you already have. Compare it on your data, retain the
results, and switch only when the required outputs agree. Unsupported commands
or options can use upstream Picard when fallback is configured; delegation is
not native acceleration.

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
