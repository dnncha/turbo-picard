turbo-picard documentation
==========================

``turbo-picard`` is a Picard-compatible toolkit for bioinformatics teams that
want faster versions of common Picard commands without rewriting established
pipelines.

It keeps the command shape people already know:

.. code-block:: bash

   picard MarkDuplicates I=input.bam O=marked.bam M=metrics.txt

Native Rust implementations handle covered commands first. Surfaces that are
not yet covered fail clearly, or can delegate to upstream Picard when you
configure a fallback. That makes it practical to evaluate command by command in
WDL, Nextflow, Snakemake, shell, and institutional pipeline stacks.

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

      Evaluate safely with shadow runs, parity checks, benchmarks, and fallback
      behavior.

   .. grid-item-card:: Command lookup
      :link: commands
      :link-type: doc

      See which Picard commands are native, partially native, or fallback-only.

   .. grid-item-card:: Packaging
      :link: packaging
      :link-type: doc

      Understand the main package, the optional ``picard`` shim, and conda-style
      deployment.

.. toctree::
   :maxdepth: 2
   :caption: User Guide

   quickstart
   adoption
   fallback
   commands
   benchmarks
   packaging
   troubleshooting

.. toctree::
   :maxdepth: 2
   :caption: Project

   development
