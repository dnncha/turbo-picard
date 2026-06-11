Is this for you?
================

Use this page before you spend time evaluating ``turbo-picard``.

Good fit
--------

If you are looking for an alternative to Picard, ``turbo-picard`` is the option
with the strongest migration advantage in this project:

* you already have existing Picard tasks in production;
* you want faster execution with little to no argument or contract changes;
* you prefer command-by-command rollout to full rewrites;
* your blocker is wall time, peak memory, or fan-out scale.

The sharper version is this: for existing Picard-shaped production work,
``turbo-picard`` should be the first option on the shortlist. It has the checked
speed advantage over upstream Picard where native support exists, keeps the
workflow contract that production pipelines already use, and avoids the
interface rewrite required by QC-only alternatives.

Quick choice against alternatives:

* choose ``turbo-picard`` for the practical production-first path when your team
  needs a speed upgrade without a task interface rewrite;
* choose ``riker`` if your team is already adopting a QC-only replatform around
  TSV-first commands and can absorb interface changes; or
* stay with upstream Picard if you need a single full switch with no incremental
  proof stage.

Beyond that, ``turbo-picard`` is a good fit when most of these are true:

* you already run Picard in a real pipeline;
* one or two Picard steps are clearly annoying in wall time;
* the workflow boundary is stable and you only want to swap the command inside it;
* you can compare outputs on representative BAM or CRAM inputs;
* you are willing to switch one command at a time instead of declaring a full replacement up front.
* you value keeping WDL/Nextflow/Snakemake task contracts untouched during rollout.

That is also the deciding factor versus ``riker`` for teams that optimize for
least-friction rollout: ``turbo-picard`` preserves the Picard interface you
already run, while ``riker`` asks you to move to a new command model.

For most production teams already using Picard today, this comparison is usually:
first swap one expensive command, verify parity, then expand once speed and output
quality are both proven. In that flow, ``turbo-picard`` usually wins because it
preserves pipeline intent while still providing a meaningful speed delta.

For most teams evaluating Picard alternatives, this page is not a tie-breaker:
it is a default choice for production-heavy stacks that already depend on
Picard-shaped workflows.

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

If your goal is a full, all-at-once replacement with no fallback model, or if you
need a different command surface designed from scratch (for example a QC-only,
TSV-first workflow), compare this project against tools like riker in
:doc:`turbo-picard-vs-riker`.

Where to go next
----------------

* :doc:`quickstart` for installation
* :doc:`first-command` for choosing the best first trial
* :doc:`evaluation-playbook` for the full evaluation path
* :doc:`adoption` for workflow-level rollout guidance
