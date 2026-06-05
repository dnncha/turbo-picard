Channel-specific examples
=========================

Use this page when you want a short example that already sounds appropriate for
the kind of place you are posting in.

nf-core Slack
-------------

   We have been evaluating ``turbo-picard`` on a single Picard-shaped step
   instead of treating it as a full replacement story. The interesting part so
   far is that a command such as ``MarkDuplicates`` or ``SortSam`` can be tested
   on representative data, compared directly against upstream Picard, and kept
   behind a narrow workflow boundary before anyone talks about a wider switch.

Seqera Community / Nextflow forum
---------------------------------

   We have been testing ``turbo-picard`` as a command-by-command upgrade path
   for Picard-heavy ``Nextflow`` workflows. The useful adoption path is small:
   choose one expensive Picard step, run upstream Picard and ``turbo-picard`` on
   the same representative shard, compare the exact downstream-consumed outputs,
   and only widen the rollout if that command is boring on real data.

Workflow repo discussion
------------------------

   This may be worth a narrow trial on the Picard step already used in this
   workflow. The goal is not to replace all of Picard. The goal is to test
   whether one command can stay in the same workflow boundary, keep the output
   reviewable against upstream Picard, and reduce wall time enough to justify a
   small change.

Workflow repo PR
----------------

   This proposal is intentionally limited to one Picard-shaped step. The useful
   part is that the command shape stays familiar, the comparison against
   upstream Picard is reviewable, and the workflow can keep upstream Picard
   available while the result is checked.

Internal platform or lab chat
-----------------------------

   We tried ``turbo-picard`` on one expensive Picard command rather than
   treating it as a broad migration. The result looks worth a narrow follow-up
   because the command can be compared directly against upstream Picard on our
   own representative shard before any wider change.

How to adapt them
-----------------

Swap in:

* the exact command tested;
* the workflow shape;
* the kind of representative input;
* the exact next step you want from the audience.

Then point readers to:

* :doc:`first-command`
* :doc:`evaluation-playbook`
* :doc:`community-targets`
* :doc:`propose-it-in-a-workflow-repo`

If you want ready-to-post repo files, see:

* ``packaging/outreach/nf-core-slack.md``
* ``packaging/outreach/seqera-community.md``
* ``packaging/outreach/workflow-repo-discussion.md``
