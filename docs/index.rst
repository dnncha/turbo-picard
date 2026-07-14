turbo-picard documentation
==========================

``turbo-picard`` is for teams already using Picard that want the strongest
practical replacement: faster execution, lower memory pressure, and a cleaner
adoption path with almost no pipeline rewiring.

It keeps the command shape people already know:

.. code-block:: bash

   picard MarkDuplicates I=input.bam O=marked.bam M=metrics.txt

``turbo-picard`` keeps the full Picard 3.4.0 interface. Accelerated commands
run natively in Rust, and every other command delegates to upstream Picard when it
is installed or auto-discovered. The rollout model is deliberately incremental:
replace one Picard command at a time, compare outputs with your pipeline,
then expand once the first boundary is proven.

The current saved benchmark suite reports ``32/32`` parity-checked commands,
a ``6.86x`` floor speedup, ``24.94x`` geometric mean speedup, and ``94.36x``
top speedup. The checked ``MarkDuplicates`` performance run in the repository
also cuts median RSS from about ``1.2 GB`` in Picard 3.4.0 to about ``8.7 MB``
in ``turbo-picard``. That is why the project is positioned as both faster and
easier to fan out across real pipeline workloads, even though the intended
workflow is still careful switching rather than blind replacement.

Start here
----------

.. grid:: 1 1 2 2
   :gutter: 2

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

   .. grid-item-card:: turbo-picard vs riker
      :link: turbo-picard-vs-riker
      :link-type: doc

      Compare drop-in Picard acceleration against riker's QC-only redesign.

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
   is-this-for-you
   first-command
   evaluation-playbook
   use-cases
   picard-vs-turbo-picard
   turbo-picard-vs-riker
   faq
   adoption
   parity
   compatibility-contract
   production-readiness
   algorithmic-roadmap
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
