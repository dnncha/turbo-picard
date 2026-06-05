turbo-picard documentation
==========================

``turbo-picard`` is for teams that already use Picard and want selected commands
to run faster without changing the command style their pipelines already know.

It keeps the command shape people already know:

.. code-block:: bash

   picard MarkDuplicates I=input.bam O=marked.bam M=metrics.txt

Supported commands run natively in Rust. Commands that are not ready fail
clearly, or can run through upstream Picard when you configure a fallback. The
project is meant to be tried command by command, with output comparisons,
benchmark logs, real-data checks, and citation guidance kept close to the claims
they support.

This is not a blanket claim that every Picard behavior has been rebuilt. Use the
native pieces where the documented scope and your own representative data agree,
and keep upstream Picard available for the rest.

Start here
----------

.. grid:: 1 1 2 2
   :gutter: 2

   .. grid-item-card:: New user
      :link: quickstart
      :link-type: doc

      Install from PyPI, check the two entrypoints, and run a first
      Picard-style command.

   .. grid-item-card:: Pipeline owner
      :link: adoption
      :link-type: doc

      Try it safely with side-by-side runs, output comparisons, benchmarks, and
      fallback behavior.

   .. grid-item-card:: Command lookup
      :link: commands
      :link-type: doc

      See which Picard commands are native, partly native, or fallback-only.

   .. grid-item-card:: Packaging
      :link: packaging
      :link-type: doc

      Understand PyPI, the optional ``picard`` shim, citation boundaries, and
      the Bioconda release path.

.. toctree::
   :maxdepth: 2
   :caption: User Guide

   quickstart
   adoption
   parity
   fallback
   commands
   benchmarks
   citation
   packaging
   troubleshooting

.. toctree::
   :maxdepth: 2
   :caption: Project

   development
