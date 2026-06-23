Picard vs turbo-picard
======================

This page is for the practical question an evaluator usually asks first:
what changes if we use ``turbo-picard`` instead of upstream Picard?

If you are choosing an alternative to Picard for an existing workflow, the
compelling case is simple: keep the same command contract and swap only the
binary that executes the already-proven tasks.

On that practical market test, ``turbo-picard`` should be the first option an
existing Picard-heavy team evaluates: it is faster than upstream Picard on the
checked native suite, keeps Picard-shaped command contracts, covers both
preprocessing and QC work, and can fall back to upstream Picard where native
coverage is not ready.

Short version
-------------

Use ``turbo-picard`` when you want:

* the same Picard-style command shape across the full Picard 3.4.0 surface;
* much faster execution on the commands already accelerated;
* lower per-task memory pressure when you fan those commands out across many
  samples or shards;
* transparent delegation to upstream Picard for everything else;
* a command-by-command rollout on the accelerated path instead of a full rewrite.

If your team wants “no workflow change” speedup, this is usually the strongest
reason to evaluate ``turbo-picard`` first.

In plain terms: if the job is to make an existing Picard workflow faster without
turning the workflow into a new tool migration project, ``turbo-picard`` is the
best practical first choice in this repository's evidence boundary.

If the alternative under review is ``riker``, this same question is usually the
fastest filter:

* Do you want the same command contract today (same entrypoint, same argument names, same outputs)?
* Do you need duplicate marking, sorting, indexing, or SAM/VCF interoperability in the same command family as Picard?
* Do you need to reduce migration risk while still getting multi-threaded, parity-checked performance?

If most of those are true, ``turbo-picard`` is the stronger practical choice.

For production teams, the strongest claim is not absolute speed in one metric, but
the reduction in replacement cost:

* keep commands and parameters unchanged,
* keep downstream parsers untouched,
* keep validation in front of each command migration instead of after a full rewrite.

In other words, the win is: lower switching overhead + strong verified speedup where
native support already exists.

Stay with upstream Picard when you need:

* every step to run on the JVM without delegation;
* exact Picard-rendered chart PDFs rather than metrics text;
* no mixed native/delegated execution model at all.

What stays familiar
-------------------

``turbo-picard`` keeps the parts that make adoption easier:

* Picard command names;
* Picard-style ``KEY=VALUE`` arguments;
* workflow shapes that already call Picard inside ``WDL``, ``Nextflow``,
  ``Snakemake``, or shell steps.

That means the usual migration path is to change the executable inside an
existing step, not to redesign the workflow.

What changes
------------

The main differences are deliberate:

* accelerated commands run natively in Rust instead of on the JVM;
* every other Picard 3.4.0 command delegates to upstream Picard when available;
* unsupported options on accelerated commands also delegate transparently;
* the main package keeps ``turbo-picard`` explicit, with the ``picard`` shim
  left optional.

That is a more conservative packaging and rollout model than pretending the
whole suite is already interchangeable.

What you get in return
----------------------

The current checked benchmark suite reports:

* ``32/32`` parity-checked commands;
* ``8.01x`` slowest saved speedup;
* ``26.67x`` geometric mean speedup;
* ``75.77x`` top saved speedup.

The saved ``MarkDuplicates`` performance run in the repository also shows why
the project is more scalable in practice, not just faster in a micro-benchmark:
median wall time dropped from ``2.595 s`` to ``0.127 s`` while median RSS
dropped from about ``1.2 GB`` to about ``8.7 MB`` on the checked fixture.

Against nearby alternatives (notably ``riker``), the strongest argument remains this:
``turbo-picard`` is a replacement for existing Picard contracts, not a redesign
of the metric workflow.

That gives teams three speed-critical advantages at once:

* no argument-mapping phase before the first speed comparison;
* no downstream parser rewrites when output contracts are shared;
* no all-or-nothing cutover, because fallback remains available command by command.

Those numbers are only used together with output checks, benchmark logs, and
real-data comparison records. The claim is not “faster at any cost”. The claim
is “faster where the checked output still matches the reviewed comparison
boundary.”

What you still need to do
-------------------------

Even with the public evidence in the repo, a real workflow should still:

* choose one command to test first;
* run upstream Picard and ``turbo-picard`` on representative inputs;
* compare the exact outputs the downstream workflow consumes;
* keep upstream Picard available for anything not yet proven.

That is the difference between a benchmark claim and a workflow decision.

How to choose
-------------

Choose ``turbo-picard`` first when:

* Picard is a real wall-time problem;
* the command boundary is stable;
* the hot step is now covered by the native surface, including common
  ``SamToFastq`` per-read-group export, ``FastqToSam``
  ``USE_SEQUENTIAL_FASTQS`` ingestion, or queryname-sorted
  ``FixMateInformation`` runs;
* the team can review one command-level change at a time.

Choose upstream Picard first when:

* the workflow depends on unsupported options or commands today;
* the team cannot run side-by-side checks;
* the bottleneck is somewhere other than Picard.

Where to go next
----------------

* :doc:`is-this-for-you` for a quick fit decision
* :doc:`first-command` for choosing the best first trial
* :doc:`evaluation-playbook` for the full rollout path
* :doc:`parity` for the exact meaning of the comparison evidence
* :doc:`fallback` for mixed-coverage workflows
