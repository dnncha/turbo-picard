Quickstart
==========

Install from source
-------------------

From a checkout of the repository:

.. code-block:: bash

   cargo install --locked --path crates/turbo-picard-cli --bin turbo-picard --bin picard

This installs two binaries:

``turbo-picard``
   The explicit, non-shadowing entrypoint. Use this first when evaluating the
   tool.

``picard``
   A compatibility shim with the same command shape as Picard. Use this only
   when you deliberately want workflow code that calls ``picard`` to resolve to
   ``turbo-picard``.

Run a familiar command
----------------------

.. code-block:: bash

   turbo-picard MarkDuplicates \
     I=input.bam \
     O=marked.bam \
     M=metrics.txt \
     ASSUME_SORTED=true \
     VALIDATION_STRINGENCY=SILENT

The shim accepts the same Picard-style syntax:

.. code-block:: bash

   picard MarkDuplicates I=input.bam O=marked.bam M=metrics.txt

Use the command-specific help while evaluating:

.. code-block:: bash

   turbo-picard --help
   turbo-picard MarkDuplicates --help
   turbo-picard SortSam --help

Good first commands
-------------------

These commands are useful first tests because they are common in production
pipelines and easy to compare against upstream Picard output:

.. code-block:: bash

   picard SortSam I=input.bam O=coordinate.bam SORT_ORDER=coordinate
   picard CleanSam I=input.bam O=cleaned.bam
   picard BuildBamIndex I=coordinate.bam O=coordinate.bai
   picard SamToFastq I=input.bam FASTQ=reads.fastq
   picard CollectQualityYieldMetrics I=input.bam O=quality_yield_metrics.txt

For broader coverage, see :doc:`commands`.
