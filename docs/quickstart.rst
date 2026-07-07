Quickstart
==========

Install with pip
----------------

For the quickest first try, install from PyPI in a fresh environment:

.. code-block:: bash

   python3 -m venv .venv
   source .venv/bin/activate
   python3 -m pip install --upgrade pip
   python3 -m pip install turbo-picard
   turbo-picard --version

The live PyPI release publishes Linux x86_64 and macOS Apple Silicon wheels
plus a source distribution. For Linux clusters and shared environments,
Bioconda is the better fit once the recipe is accepted; until then, use the
source install below if the PyPI package is not a good fit for the target
system.

The PyPI package installs two commands:

``turbo-picard``
   The explicit command. Start with ``turbo-picard`` while you are testing the
   tool.

``picard``
   A compatibility shim with the same command shape as Picard. Use this only
   when you deliberately want workflow code that calls ``picard`` to resolve to
   ``turbo-picard``.

Use a dedicated virtual environment if you also need upstream Picard on the
same machine. Inside that environment, the ``picard`` command from this package
can shadow another Picard installation.

Install from source
-------------------

From a checkout of the repository:

.. code-block:: bash

   cargo install --locked --path crates/turbo-picard-cli --bin turbo-picard --bin picard

This installs the same two commands:

``turbo-picard``
   The explicit command. Use this first while you are testing the tool.

``picard``
   A compatibility shim with the same command shape as Picard. Use this only
   when you deliberately want workflow code that calls ``picard`` to resolve to
   ``turbo-picard``.

Run a familiar command
----------------------

Start with ``turbo-picard`` rather than the ``picard`` shim. That keeps the
test separate from any existing Picard installation and makes it clear which
binary produced each output.

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

Use the shim only after you have checked the commands your pipeline needs. For
a first pass, pick one representative input and run ``turbo-picard`` beside
upstream Picard. Compare the primary output, sidecars, metrics, exit code, and
runtime before changing anything permanent.

Use the command-specific help while evaluating:

.. code-block:: bash

   turbo-picard --help
   turbo-picard MarkDuplicates --help
   turbo-picard SortSam --help

Check the runtime
-----------------

For normal use, ``turbo-picard`` runs on the CPU and uses HTSlib worker threads
for BAM/CRAM I/O. You can check what the installed build will do on the current
machine:

.. code-block:: bash

   turbo-picard AccelerationStatus

If a workflow must fail unless a production GPU backend is available, make that
explicit in the preflight:

.. code-block:: bash

   TURBO_PICARD_ACCELERATOR=gpu-required turbo-picard AccelerationStatus

Current release builds report ``gpu_acceleration=not-enabled``. That is
intentional: a production pipeline should know whether GPU acceleration is
really available instead of silently running on the threaded CPU path.

Workflow preflight
------------------

Use ``explain`` before switching a task. The text form is useful while
debugging; the JSON form is intended for workflow wrappers and CI checks:

.. code-block:: bash

   turbo-picard explain MarkDuplicates I=input.bam O=marked.bam M=metrics.txt
   turbo-picard explain --json MarkDuplicates I=input.bam O=marked.bam M=metrics.txt
   turbo-picard explain --format json MarkDuplicates I=input.bam O=marked.bam M=metrics.txt

The JSON report includes ``schema_version``, command status, execution path,
resolved fallback command, and the output arguments declared on the command line.

Use ``trial`` when you want a copyable side-by-side command pair plus the
comparison evidence to keep with a workflow review:

.. code-block:: bash

   turbo-picard trial MarkDuplicates I=input.bam O=marked.bam M=metrics.txt
   turbo-picard trial --json MarkDuplicates I=input.bam O=marked.bam M=metrics.txt

The report names the upstream Picard command, the ``turbo-picard`` command,
declared outputs, fallback state, and the comparison notes to preserve with the
representative input.

Good first commands
-------------------

These commands are useful first tests because they are common in real pipelines
and easy to compare against upstream Picard output:

.. code-block:: bash

   turbo-picard SortSam I=input.bam O=coordinate.bam SORT_ORDER=coordinate
   turbo-picard CleanSam I=input.bam O=cleaned.bam
   turbo-picard BuildBamIndex I=coordinate.bam O=coordinate.bai
   turbo-picard SamToFastq I=input.bam FASTQ=reads.fastq
   turbo-picard SamToFastq I=input.bam OUTPUT_PER_RG=true OUTPUT_DIR=fastq-by-rg
   turbo-picard CollectQualityYieldMetrics I=input.bam O=quality_yield_metrics.txt

For broader coverage, see :doc:`commands`.
If the right first test is still unclear, go to :doc:`first-command`.

CRAM inputs and outputs
-----------------------

Hot-path commands accept ``.cram`` when a reference FASTA is available. Pass
Picard-style ``REFERENCE_SEQUENCE`` or export ``TURBO_PICARD_REFERENCE``:

.. code-block:: bash

   export TURBO_PICARD_REFERENCE=/path/to/reference.fa
   turbo-picard SortSam I=reads.cram O=sorted.cram SORT_ORDER=coordinate
   turbo-picard MarkDuplicates I=reads.cram O=marked.cram M=metrics.txt

Before changing a workflow
--------------------------

Before putting the ``picard`` shim on ``PATH`` for a pipeline:

* check the command in :doc:`commands`;
* read :doc:`parity` so you know what has actually been compared;
* keep upstream Picard configured as fallback for unsupported commands;
* keep the evidence for the exact command, input, Picard version, and
  ``turbo-picard`` version you tested.

The pipeline guide goes deeper: :doc:`adoption`.
