turbo-picard documentation
==========================

``turbo-picard`` is for teams that already use Picard and want selected
commands to run much faster and with less memory pressure, without retraining
everyone or rewriting the shape of a working pipeline.

It keeps the command shape people already know:

.. code-block:: bash

   picard MarkDuplicates I=input.bam O=marked.bam M=metrics.txt

Supported commands run natively in Rust. Commands that are not ready fail
clearly, or can run through upstream Picard when you configure a fallback. The
project is meant to be adopted one command at a time, with output comparisons,
benchmark logs, real-data checks, and citation guidance kept close to the claims
they support.

This is not a blanket claim that every Picard behavior has been rebuilt. Use the
native pieces where the documented scope and your own representative data agree,
and keep upstream Picard available for the rest.

The current saved benchmark suite reports ``32/32`` parity-checked commands,
an ``8.55x`` floor speedup, ``26.74x`` geometric mean speedup, and ``84.46x``
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
   faq
   adoption
   parity
   fallback
   commands
   benchmarks
   performance
   citation
   packaging
   troubleshooting

.. toctree::
   :maxdepth: 2
   :caption: Project

   development
