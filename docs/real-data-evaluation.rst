Prove the switch on your data
=============================

An evaluation should answer one question: does this version of Turbo Picard
preserve the outputs your task needs, with useful runtime or resource savings?
A fast fixture alone does not answer that question.

This page describes the **next-release repository evaluator**. The evaluator
is a Python script in the checkout, not a new command shipped in PyPI 0.1.12.
It can evaluate an explicitly selected installed binary. The strict native-only
environment policy also disables legacy fallback paths for older candidates.

Run a local comparison
----------------------

Use a representative input and an explicit upstream executable. In particular,
use the Java JAR path below rather than an ambiguous ``picard`` executable that
might resolve to this package's compatibility shim. Replace the two input paths
with your actual BAM and upstream JAR locations.

From the repository root, with Python 3.11 or newer:

.. code-block:: bash

   trial="$(mktemp -d)"
   python3 tools/compare_real_data.py \
     --skip-build \
     --input-bam /data/sample.bam \
     --commands MarkDuplicates \
     --turbo-picard-command "$(command -v turbo-picard)" \
     --picard-command 'java -jar /opt/picard/picard.jar' \
     --output-dir "$trial" \
     --shareable-report "$trial/shareable.md"

For CRAM, also pass ``--reference-fasta /data/reference.fa``. Keep that exact
reference with the evaluation record. Additional duplicate-marking options can
be passed with repeatable ``--markduplicates-arg KEY=VALUE`` arguments; inspect
``--help`` for reserved paths and supported options.

The evaluator runs locally; it does not upload inputs or reports. An executable
prefix is split into arguments, not executed by a shell. Supply only executable
prefixes you intend to run.

What you receive
----------------

``real-data-comparison.json`` and ``real-data-comparison.md`` record the input
identity, executable versions, per-command timings, parity status, compared
artifacts, and digests. The ``work/`` directory keeps separate Turbo Picard and
upstream outputs. A shareable report deliberately omits local paths, input
hashes, command arguments and raw data; review it before posting it anywhere.

A passing comparison applies to the implemented comparison contract, the exact
options, input and versions. It is not full cohort validation, a guarantee about
all optional tags, a chart-rendering equivalence claim, or downstream variant
calling validation. Read :doc:`parity` and :doc:`compatibility-contract` alongside
the report. Command timings are single measurements, not repeated statistical
benchmarks; rerun into fresh directories to assess variability.

No clobbering, including failed runs
------------------------------------

An existing ``work/`` directory or fixed evidence report is an error, not an
invitation to delete the previous run. Choose a new ``--output-dir`` to retry.
The shared report cannot replace an existing file, a command artifact or a
reserved evidence manifest. Failed comparisons retain their intermediates.
Evidence reports are created exclusively, so a file created after the initial
checks is also preserved rather than overwritten.
``--discard-work`` removes this run's command outputs only after parity passes
and reports have been written. It does not remove failed-run evidence.

Regenerating checked-in evidence is a deliberate operation. Existing bootstrap
scripts that target a populated evidence directory now stop rather than replace
it. Preserve the old bundle and evaluate into a fresh directory; review the new
report before replacing checked-in evidence and updating its manifest.

Comparison memory and temporary disk
------------------------------------

The coordinate and duplicate-semantics comparators sort records in bounded
chunks and merge on disk. They preserve duplicate multiplicity and exact digest
ordering rather than sampling. Coordinate order is checked while streaming.
Read-group comparison streams alignment records instead of collecting all of
them in memory.

The default chunk budget is 8 MiB of estimated byte-record storage, with at most
50,000 records in a chunk and 32 input runs per merge. These are **not** total
process RSS limits: interpreter state, sort keys, file buffers, read-group
headers and a single oversized record use additional memory. Set ``TMPDIR`` to
suitable local scratch storage and allow space for uncompressed comparison
records plus an intermediate merge. Scratch files are private to each call and
are cleaned on errors and early consumer exits.

.. code-block:: bash

   TMPDIR=/scratch python3 tools/compare_real_data.py --help

The repository's ``tools/bench_validation_memory.py`` measures this helper in
fresh Python processes against a historical helper file. It alternates order,
records peak RSS and all individual timings, and refuses to promote a digest
mismatch. Its synthetic results are not native Turbo Picard or WGS benchmarks.

For a production decision
-------------------------

Check the flags, metrics, ordering, sidecars and failure behaviour consumed by
your task. Use matching options and resource budgets. Retain upstream Picard
until the comparison and the relevant downstream checks pass. A mismatch is a
useful, actionable report: preserve the evidence rather than changing a
comparison rule just to turn the result green.
