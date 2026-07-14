Algorithmic SOTA roadmap
========================

Turbo-Picard's target is not merely to outperform Picard on small fixtures.
It is to preserve the Picard interface and documented scientific semantics
while matching or beating the best specialised implementation for each
production workload.

North-star execution model
--------------------------

The long-term engine decodes each alignment once, derives shared biological
features once, and writes each requested final output once:

.. code-block:: text

   BAM/CRAM decode
          |
   packed record features
          |
   optional external sort/fixmate
          |
   duplicate decisions + requested metrics
          |
   ordered BAM/CRAM output + index + MD5

Standalone Picard-compatible commands remain supported. Cross-command fusion
is explicit and is enabled only when an intermediate file is not part of the
requested output contract.

Non-negotiable gates
--------------------

Every optimisation must pass all applicable gates before its result becomes a
release or marketing claim:

* deterministic output across thread counts, block sizes, and repeated runs;
* Picard-compatible flags, tags, metrics, sidecars, failures, and winner
  selection for the documented option scope;
* public, pinned BAM/CRAM datasets and exact command lines;
* median and p95 wall time, CPU time, peak RSS, temporary disk, and cost;
* normal and adversarial workloads, including clipping, distant mates, UMI,
  optical families, multiple libraries, supplementary records, and CRAM;
* comparison against the strongest relevant specialised tool, not Picard
  alone.

Thread A: bounded-memory duplicate marking
------------------------------------------

Goal
~~~~

Replace whole-file BAM retention with a bounded read-end/decision engine and
beat FastDup and samtools markdup without relaxing Picard winner identity.

Algorithm and milestones
~~~~~~~~~~~~~~~~~~~~~~~~

#. Extract fixed-width read-end metadata while decoding records.
#. Intern libraries and barcodes; collision-check compact name fingerprints.
#. Group integer duplicate signatures with radix partitions or cache-local maps.
#. Process coordinate blocks in parallel and reconcile clipped reads, distant
   mates, and cross-block families through a sparse dependency graph.
#. Persist decisions by record ordinal or virtual offset, then mutate records
   in ordered output.

Milestones are: A1 remove avoidable qname/barcode allocations and quadratic
optical scans; A2 compact read-end metadata with a bytes-per-record test; A3 an
exact bounded-memory two-pass BAM path; A4 block speculation and dependency
reconciliation; A5 marking during final external-sort merge with inline index.

Targets: below 4 GiB RSS on 30x WGS (stretch below 2 GiB), at least 20 percent
faster than the best of FastDup and samtools at equal cores, no quadratic
large-family behaviour, and 70 percent parallel efficiency through eight cores.

Thread B: shared metrics and rolling coverage
---------------------------------------------

Goal
~~~~

Beat Riker's multi-metric and WGS coverage performance while retaining
Picard-compatible metric definitions and output.

Decode structure-of-arrays feature batches containing flags, positions, CIGAR
spans, quality summaries, insert size, overlap observations, GC windows, and
reference mismatch events. Compute shared primitives once and reduce feature
columns with work stealing. WGS coverage moves from chromosome-sized counters
to coordinate-finalised tiles with adaptive-width counters and sparse overflow.

Milestones are: B1 measure repeated collector work; B2 shared feature batches;
B3 remove the slowest-worker batch barrier; B4 rolling WGS depth tiles; B5
native CollectHsMetrics on the shared interval engine.

Targets: 20 percent faster than Riker multi, 20 percent faster than the best
quality-aware WGS comparator, rolling WGS state below 128 MiB independent of
chromosome length, and no repeated full CIGAR traversal for shared features.

Thread C: fused preprocessing and benchmark science
---------------------------------------------------

Goal
~~~~

Win end-to-end workflow cost by removing intermediate BAM passes, and make
every SOTA claim independently reproducible.

Milestones are: C1 a production comparator runner for Picard, samtools,
FastDup, Riker, and Turbo-Picard; C2 pinned WGS, WES, RNA, UMI, CRAM,
multi-library, and optical-heavy manifests; C3 an explicit fused-plan contract
for sort, fixmate, duplicate marking, metrics, output, index, and MD5; C4 raw
BAM payload preservation through sort spills and marking during final merge;
C5 independent reproductions and cloud cost per sample.

Targets: 2x lower end-to-end preprocessing time than separate Turbo-Picard
commands on storage-bound WGS, at least 50 percent fewer alignment bytes
read/written where fusion applies, and complete version, host, thread, command,
parity, RSS, disk, and failure evidence for every published result.

Release scorecard
-----------------

The project may use "SOTA candidate" only after a production-scale profile
passes its specialised comparator. It may use a broad "SOTA" claim only after
all three threads pass on two machine profiles and an independent party
reproduces the evidence. Until then, every report names the exact command,
dataset, format, core count, and comparator.
