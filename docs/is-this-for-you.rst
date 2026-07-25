Is this for you?
================

Use this page before you spend time evaluating ``turbo-picard``.

Good fit
--------

``turbo-picard`` is worth evaluating when most of the following are true:

* you already run Picard in a real pipeline;
* one or two Picard steps are a measurable wall-time or memory bottleneck;
* the workflow boundary is stable and you want to test the command inside it;
* you can compare outputs on representative BAM or CRAM inputs;
* you are willing to make a command-level decision rather than claim a complete
  replacement up front;
* keeping Picard command names and ``KEY=VALUE`` arguments is useful to the
  existing WDL, Nextflow, Snakemake, or shell task.

Typical good-fit teams:

* ``WDL`` and ``Cromwell`` teams with identified preprocessing tasks;
* ``Nextflow`` or nf-core maintainers evaluating a Picard-shaped process;
* ``Snakemake`` or shell pipeline owners who know where Picard sits in the run;
* platform teams that require a recorded command, input, output comparison, and
  fallback decision before changing a workflow.

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
* record the command lines, versions, timings, and outputs so another maintainer
  can review the result.

If that sounds reasonable, continue with :doc:`first-command`.

If that already sounds like too much process, ``turbo-picard`` is probably not
the right change to push right now.

If your goal is a full, all-at-once replacement with no fallback model, or you
need a different command surface designed from scratch, read
:doc:`picard-alternatives` and compare tools on the same input, workflow
contract, and output requirements.

Where to go next
----------------

* :doc:`quickstart` for installation
* :doc:`first-command` for choosing the best first trial
* :doc:`evaluation-playbook` for the full evaluation path
* :doc:`adoption` for workflow-level rollout guidance
