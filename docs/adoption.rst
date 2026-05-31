Adoption guide
==============

The safest way to adopt ``turbo-picard`` is to treat it as a command-by-command
replacement, not a whole-pipeline flag day.

Recommended rollout
-------------------

1. Shadow production-like data
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

Start with the explicit binary:

.. code-block:: bash

   turbo-picard MarkDuplicates I=input.bam O=turbo.bam M=turbo.metrics.txt

Keep upstream Picard as the production path while you compare:

* BAM, SAM, FASTQ, VCF, interval-list, and metrics outputs.
* Sidecar files such as indexes and md5 files.
* Exit codes and error messages for bad inputs.
* Runtime and memory behavior on realistic shards.

2. Prove the exact surfaces you need
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

Run the command matrix check and the relevant parity scripts:

.. code-block:: bash

   python3 tools/verify_command_matrix.py
   ./tools/verify_basic_picard_parity.sh
   ./tools/verify_basic_sortsam_parity.sh
   ./tools/verify_basic_samtofastq_parity.sh

The repository keeps one parity script per documented command surface. The goal
is not to prove all of Picard at once. The goal is to prove the Picard behavior
your workflows actually depend on.

3. Configure fallback for mixed coverage
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

Use fallback when a workflow calls both accelerated and unsupported Picard
surfaces:

.. code-block:: bash

   export TURBO_PICARD_FALLBACK_COMMAND='java -jar /opt/picard/picard.jar'

Native commands still run natively. Unsupported commands delegate to upstream
Picard. Details are in :doc:`fallback`.

4. Switch narrow workflow surfaces
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

Only after the relevant parity and benchmark evidence is acceptable, put the
``picard`` shim ahead of upstream Picard on ``PATH`` for that workflow or
environment.

5. Keep evidence in CI
~~~~~~~~~~~~~~~~~~~~~~

Run the command matrix check, targeted parity scripts, and benchmark suite for
the commands you depend on:

.. code-block:: bash

   python3 tools/bench_suite.py --repeats 1 --skip-build
   python3 tools/verify_benchmark_log_evidence.py

This keeps upgrades boring: coverage changes are explicit, and performance
claims stay tied to measured output.
