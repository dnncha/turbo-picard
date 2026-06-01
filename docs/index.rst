turbo-picard documentation
==========================

``turbo-picard`` is for teams that already have Picard in real workflows and
want the slow parts to be less slow without rewriting every WDL, Nextflow
process, Snakemake rule, shell script, and validation note around it.

It keeps the command shape people already know:

.. code-block:: bash

   picard MarkDuplicates I=input.bam O=marked.bam M=metrics.txt

Covered command surfaces run natively in Rust. Surfaces that are not ready stay
visible: they fail clearly, or can delegate to upstream Picard when you
configure a fallback. The project is meant to be adopted command by command,
with parity evidence, benchmark logs, real-data comparisons, and citation
boundaries kept close to the claims they support.

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

      Install ``turbo-picard``, run the first command, and understand the two
      entrypoints.

   .. grid-item-card:: Pipeline owner
      :link: adoption
      :link-type: doc

      Evaluate safely with shadow runs, parity checks, real-data comparisons,
      benchmarks, and fallback behavior.

   .. grid-item-card:: Command lookup
      :link: commands
      :link-type: doc

      See which Picard commands are native, partially native, or fallback-only.

   .. grid-item-card:: Packaging
      :link: packaging
      :link-type: doc

      Understand the main package, the optional ``picard`` shim, citation
      boundaries, and the Bioconda release path.

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
