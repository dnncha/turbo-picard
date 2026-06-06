Picard vs turbo-picard
======================

This page is for the practical question an evaluator usually asks first:
what changes if we use ``turbo-picard`` instead of upstream Picard?

Short version
-------------

Use ``turbo-picard`` when you want:

* the same Picard-style command shape;
* much faster execution on the commands already accelerated;
* lower per-task memory pressure when you fan those commands out across many
  samples or shards;
* a command-by-command rollout instead of a full tool rewrite;
* fallback to upstream Picard for unsupported surfaces.

Stay with upstream Picard when you need:

* commands or options outside the documented native scope;
* exact Picard-rendered chart PDFs rather than metrics text;
* one immediate full-suite replacement with no mixed-coverage period.

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

* supported commands run natively in Rust instead of on the JVM;
* unsupported commands fail clearly by default;
* fallback can delegate unsupported surfaces to upstream Picard;
* the main package keeps ``turbo-picard`` explicit, with the ``picard`` shim
  left optional.

That is a more conservative packaging and rollout model than pretending the
whole suite is already interchangeable.

What you get in return
----------------------

The current checked benchmark suite reports:

* ``32/32`` parity-checked commands;
* ``8.55x`` slowest saved speedup;
* ``26.74x`` geometric mean speedup;
* ``84.46x`` top saved speedup.

The saved ``MarkDuplicates`` performance run in the repository also shows why
the project is more scalable in practice, not just faster in a micro-benchmark:
median wall time dropped from ``2.595 s`` to ``0.127 s`` while median RSS
dropped from about ``1.2 GB`` to about ``8.7 MB`` on the checked fixture.

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
