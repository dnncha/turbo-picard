Is this for you?
================

Use this page before you spend time evaluating ``turbo-picard``.

Good fit
--------

``turbo-picard`` is a good fit when most of these are true:

* you already run Picard in a real pipeline;
* one or two Picard steps are clearly annoying in wall time;
* the workflow boundary is stable and you only want to swap the command inside it;
* you can compare outputs on representative BAM or CRAM inputs;
* you are willing to switch one command at a time instead of declaring a full replacement up front.

Typical good-fit users:

* ``WDL`` and ``Cromwell`` teams with heavy preprocessing tasks;
* ``Nextflow`` or ``nf-core`` maintainers who want a faster Picard-shaped step;
* ``Snakemake`` or shell pipeline owners who already know where Picard sits in the run;
* platform teams that need evidence before changing production behavior.

Probably not a fit yet
----------------------

``turbo-picard`` is probably not the right use of your time yet when any of
these is true:

* you need every Picard command immediately with no mixed-coverage period;
* the workflow depends on exact Picard-rendered chart PDFs rather than metrics text;
* you cannot run side-by-side checks on data that looks like your real workflow;
* the only acceptable rollout is a blind global switch with no command-level review;
* the workflow pain is elsewhere and Picard is not actually a bottleneck.

What a sensible first evaluation looks like
-------------------------------------------

The right first trial is small and boring:

* choose one command that is slow enough to matter;
* run Picard and ``turbo-picard`` on the same representative shard;
* compare the exact outputs your downstream workflow consumes;
* keep the timings and outputs together so another maintainer can review them.

If that sounds reasonable, continue with :doc:`first-command`.

If that already sounds like too much process, ``turbo-picard`` is probably not
the right change to push right now.

Where to go next
----------------

* :doc:`quickstart` for installation
* :doc:`first-command` for choosing the best first trial
* :doc:`evaluation-playbook` for the full evaluation path
* :doc:`adoption` for workflow-level rollout guidance
