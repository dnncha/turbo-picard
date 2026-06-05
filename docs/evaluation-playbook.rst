Evaluation playbook
===================

This page is the shortest path through the repo if you are evaluating
``turbo-picard`` for a real workflow or trying to explain that evaluation to
other people.

1. Decide whether a trial is worth doing
----------------------------------------

Start with ``packaging/outreach/evaluation-checklist.md`` if you need a quick
internal go/no-go screen.

Good reasons to trial ``turbo-picard``:

* one Picard step keeps showing up in wall-time complaints;
* the workflow already shells out to Picard in a stable place;
* you can get one representative BAM or CRAM shard;
* the downstream-consumed outputs are easy to compare.

2. Pick the first command
-------------------------

If the right first substitution is unclear, use
``packaging/workflows/choose-your-first-command.md`` or :doc:`first-command`.

In practice, the best first trials are usually:

* ``MarkDuplicates`` for preprocessing-heavy pipelines;
* ``SortSam`` for repeated BAM or CRAM reshaping;
* ``SamToFastq`` for export-heavy realignment or remap paths;
* ``BuildBamIndex`` for a very small, low-risk first substitution.

3. Pick the workflow shape
--------------------------

Starter files live in ``packaging/workflows/``.

Use:

* ``markduplicates.wdl`` or ``sortsam.wdl`` for ``WDL`` / ``Cromwell``;
* ``markduplicates.nf``, ``sortsam.nf``, or ``samtofastq.nf`` for
  ``Nextflow`` / nf-core style trials;
* ``Snakefile`` for a small ``Snakemake``-style command swap.

Walkthroughs:

* ``packaging/workflows/wdl-cromwell.md``
* ``packaging/workflows/nextflow-nf-core.md``
* ``packaging/workflows/snakemake.md``

4. Run the smallest honest trial
--------------------------------

For the smallest reviewable evaluation flow, use:

* ``packaging/workflows/one-command-trial.md``
* ``packaging/workflows/trial.wdl``
* ``packaging/workflows/trial.nf``
* ``packaging/workflows/trial-config.yaml``

The minimum standard is simple:

* run upstream Picard and ``turbo-picard`` on the same representative shard;
* compare the exact files your downstream workflow consumes;
* keep the command lines, timings, metrics, and outputs together.

For a fuller comparison bundle, use ``tools/audit_real_data.py`` or
``tools/compare_real_data.py``.

5. Record the result
--------------------

Use ``packaging/outreach/team-review-template.md`` after a first trial when you
want a short written record other workflow owners can review quickly.

6. Share it
-----------

If the result is useful and you want to spread it:

* ``packaging/outreach/slack-message.md`` for internal chat;
* ``packaging/outreach/email-blurb.md`` for a workflow owner or platform team;
* ``packaging/outreach/module-pr-note.md`` for a workflow or module PR;
* ``packaging/outreach/community-post.md`` and
  ``packaging/outreach/github-discussion.md`` for broader visibility;
* ``packaging/outreach/channel-map.md`` if the main question is which of those
  channels fits the result you have;
* ``packaging/outreach/launch-plan.md`` for the order in which to do that.

If the conversation stalls on the usual concerns, use
``packaging/outreach/objections.md``.

7. Follow through
-----------------

If the first trial is good and you need the next steps spelled out, continue
with :doc:`after-evaluation`.

What this is not
----------------

This playbook is for getting to a responsible first trial quickly. It is not a
claim that every Picard workflow is ready to switch unchanged, and it is not a
reason to skip side-by-side checks on representative data.
