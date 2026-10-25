Independent workflow evaluations
================================

Help establish where a native Picard task belongs
-------------------------------------------------

Choose one existing Picard task that costs meaningful runtime, memory or
maintenance effort. Run Turbo Picard beside it on representative input, compare
the outputs your workflow needs, and report the result. A mismatch, an
unsupported option or a decision to retain another tool is useful evidence.

Start with the `local comparison planner
<https://turbo-picard.readthedocs.io/en/latest/evaluate/>`_ or the complete
:doc:`real-data-evaluation` guide. The planner uses fixed starter configurations;
it does not inspect files or copy every option from an existing pipeline.

The project is independent of the Broad Institute. An evaluation report is
neither an endorsement nor permission to use a team's name or logo.

Current evidence and open work
------------------------------

This guide distinguishes retained evidence from the evidence a new workflow
must supply. Downloads and example workflows are not independent adoption.

.. list-table::
   :header-rows: 1
   :widths: 25 35 40

   * - Evidence
     - Retained scope
     - What it establishes
   * - Historical 32-command suite
     - Small fixtures, saved on 14 August 2026 against Picard 3.4.0
     - Agreement under each named comparator and timings for those cases;
       startup can dominate short runs
   * - Pinned public inputs
     - Small mitochondrial BAM/CRAM and variant fixtures
     - Release checks on those inputs, not every assay or cohort
   * - v0.1.16 command evidence
     - Bounded synthetic MarkDuplicates cases and recorded paired spill runs
     - Scoped correctness and resource comparisons, including native
       competitor losses; see the retained release notes
   * - Representative full-scale comparisons
     - Open evaluation work
     - Require workload-specific raw results, complete resource measurements
       and the applicable scientific checks
   * - Independent reproduction and retained use
     - Report and review separately
     - A second-machine reproduction and an owner's retained task are
       distinct from an initial passing trial

See :doc:`benchmarks`, :doc:`parity` and the
`v0.1.16 release evidence
<https://github.com/dnncha/turbo-picard/blob/main/docs/releases/v0.1.16.md>`_
for the saved records. No new whole-genome performance ordering is established
by this guide.

Run one reviewable trial
------------------------

1. Record the installed release and inspect the exact command and options with
   ``capabilities`` and ``trial``. Keep upstream Picard explicit so its command
   cannot accidentally resolve to the compatibility shim.
2. Name the required comparison contract before running. Include relevant
   flags, representative selection, metrics, histograms, indexes and sidecars.
   State the required reference, read groups, libraries, barcode policy and
   optical-duplicate settings.
3. Keep the original input immutable. Use fresh output directories for each
   implementation and preserve failed runs. Require native execution on the
   Turbo Picard side when testing the native path.
4. Run the downstream consumer. A record match, equivalent duplicate families
   and unchanged scientific conclusions are different results; report the ones
   actually checked.
5. Record the complete task's resources. The starter evaluator times one
   invocation per side; it does not supply a statistically robust performance
   study or automatically measure peak RSS.
6. Decide whether to retain the substitution. Pin the executable and repeat
   the relevant comparison after an upgrade.

The :doc:`compatibility-contract` defines comparison levels and known
differences. Keep metrics-text checks separate from Picard-equivalent rendered
chart requirements. Advanced UMI normalization and consensus workflows require
their own appropriate tools and contracts.

Report a result
---------------

Use the `trial report form
<https://github.com/dnncha/turbo-picard/issues/new?template=trial-report.yml>`_.
If new-issue creation is unavailable, use the existing `public trial thread
<https://github.com/dnncha/turbo-picard/issues/4>`_. Include:

* Tool versions and install channel; the redacted command and native/fallback
  state.
* Assay and file shape, approximate size, ordering, read groups/libraries and
  relevant barcode or optical modes.
* The actual comparator, required outputs and downstream checks, including
  mismatches and failures.
* Host architecture, CPU allocation, storage, compression, thread settings,
  repeat counts, wall/CPU time, peak RSS and scratch when measured. Mark
  unavailable measurements as unmeasured.
* Whether this was a trial, an accepted task or a substitution retained after
  an upgrade; explain blockers or the reason to keep another tool.

The evaluator's optional shareable summary omits paths, hashes, raw arguments
and raw data. Review it before posting. Keep full private bundles in your own
infrastructure; a public report can describe input shape and outcomes. Public
raw evidence should use retrievable inputs and an immutable revision or
accession with SHA-256 hashes.

Identify maintainer-run and external-owner reports separately. A successful
trial is not automatically a reviewed independent reproduction or a retained
workflow. Review the evidence before presenting any result as a case study.

Reproduce a performance comparison
----------------------------------

For publishable workload-scale results, follow the existing
`production evidence protocol
<https://github.com/dnncha/turbo-picard/blob/main/benchmarks/production/README.md>`_
and the `MarkDuplicates competitor contract
<https://github.com/dnncha/turbo-picard/blob/main/benchmarks/markduplicates-competitors/README.md>`_.
Retain at least five measured repeats, raw logs, input/reference and executable
identities, host/storage details, resource measurements and comparison outcomes.
Report the distribution and limitations; a p95 from five observations has
limited precision.

The comparison portfolio calls for 30x WGS, duplicate-heavy WES, barcode/UMI
and optical-heavy cases before broad duplicate-marking claims. Reference-backed
CRAM, multiple libraries and cohort profiles answer additional workflow
questions. A result on one case supports that case only.

Include samtools preparation only when the workflow requires it. Compare
streaming tools from aligner input through final sorted output. For Riker,
match required collector bundles and audit metric definitions independently
of timing. For sorting, match order, compression, threads and scratch, and
consider samtools and mako alongside SortSam.

Second-machine reproduction must retain the same input, Turbo Picard commit
and argument-protocol hashes, plus an independent host profile and evidence
URL. The manifest validators check those fields; they do not replace scientific
review or establish a team's independence by themselves.

Keep slower runs, unsuccessful trials and semantic differences visible. The
useful outcome is a documented decision about a real task.
