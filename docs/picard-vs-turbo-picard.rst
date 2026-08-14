Picard vs turbo-picard
======================

This page describes what changes when an existing Picard task is evaluated with
``turbo-picard``. It is not a recommendation to replace every Picard command.

Short version
-------------

Use ``turbo-picard`` for a command-level evaluation when keeping the Picard
command name and ``KEY=VALUE`` arguments matters, the candidate command is in
the documented native scope, and representative outputs can be compared. Use
upstream Picard directly when the required command or option is outside that
scope, exact Picard-rendered chart PDFs are required, or a mixed
native/delegated execution model is unacceptable.

What stays familiar
-------------------

``turbo-picard`` retains these parts of a Picard task:

* Picard command names;
* Picard-style ``KEY=VALUE`` arguments;
* workflow shapes that already call Picard inside ``WDL``, ``Nextflow``,
  ``Snakemake``, or shell steps.

The explicit ``turbo-picard`` command lets an evaluation distinguish its output
from upstream Picard. The optional ``picard`` shim should be added only after a
pipeline-specific review.

What changes
------------

The main differences are deliberate:

* selected commands and option scopes run natively in Rust instead of on the JVM;
* Picard 3.4.0 commands without a native implementation delegate only when
  upstream Picard is installed or configured as fallback;
* unsupported options on an accelerated command must delegate or fail clearly;
* the main package keeps ``turbo-picard`` explicit, with the ``picard`` shim
  left optional.

Read :doc:`commands` for the native, partial-native, and delegated split. A
command being accepted by the wrapper does not mean that it used a native path.

Saved comparison evidence
-------------------------

The current checked benchmark suite reports:

* ``32/32`` parity-checked commands;
* ``22.17x`` slowest saved speedup;
* ``87.47x`` geometric mean speedup;
* ``261.75x`` top saved speedup.

The repository also records a ``MarkDuplicates`` fixture with a median wall
time of ``2.595 s`` for Picard and ``0.127 s`` for ``turbo-picard``, and median
RSS of about ``1.2 GB`` and ``8.7 MB`` respectively. These are fixture-specific
measurements, not capacity or performance guarantees. See :doc:`benchmarks` for
the command lines, input scope, output comparators, and reproduction commands.

What you still need to do
-------------------------

Even with the saved evidence in the repository, a workflow evaluation should:

* choose one command to test first;
* run upstream Picard and ``turbo-picard`` on representative inputs;
* compare the exact outputs the downstream workflow consumes;
* keep upstream Picard available for anything not yet proven.

That is the difference between a benchmark claim and a workflow decision.

How to choose
-------------

Evaluate ``turbo-picard`` when:

* Picard is a real wall-time problem;
* the command boundary is stable;
* the hot step is now covered by the native surface, including common
  ``SamToFastq`` per-read-group export, ``FastqToSam``
  ``USE_SEQUENTIAL_FASTQS`` ingestion, or queryname-sorted
  ``FixMateInformation`` runs;
* the team can review one command-level change at a time.

Use upstream Picard when:

* the workflow depends on unsupported options or commands today;
* the team cannot run side-by-side checks;
* the bottleneck is somewhere other than Picard.

Where to go next
----------------

* :doc:`is-this-for-you` for a quick fit decision
* :doc:`first-command` for choosing a first trial
* :doc:`evaluation-playbook` for the full rollout path
* :doc:`parity` for the exact meaning of the comparison evidence
* :doc:`fallback` for mixed-coverage workflows
