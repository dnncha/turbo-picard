Message examples
================

Use this page when you already have a narrow command-level result and want a
short message you can adapt quickly.

Internal chat
-------------

   We tested ``turbo-picard`` on one Picard step instead of treating it as a
   full-suite replacement. The first command was ``MarkDuplicates`` on a
   representative shard, and we compared BAM output plus metrics against
   upstream Picard before talking about anything broader.

Workflow or module PR
---------------------

   This change is scoped to a narrow ``turbo-picard`` evaluation path for one
   Picard-shaped step. The useful part is not a blanket switch. It is that the
   command shape stays familiar, the step can be tested on representative data,
   and the workflow can keep upstream Picard available while the result is
   reviewed.

GitHub discussion
-----------------

   If this pipeline step is already using Picard, ``turbo-picard`` may be worth
   a narrow trial on that one command. The sensible path is to test one
   expensive step on representative data, compare the exact downstream-consumed
   outputs against upstream Picard, and decide on that command before talking
   about a wider switch.

Email or direct maintainer note
-------------------------------

   I have been testing ``turbo-picard``, a faster Rust implementation of
   selected Picard commands. It keeps Picard-style command names and
   ``KEY=VALUE`` arguments, so the practical adoption path is to swap one
   command at a time instead of redesigning a workflow. If we want a low-risk
   trial, the best first move is to choose one expensive Picard step from our
   own workflow, compare it directly with upstream Picard on a representative
   shard, and decide from there.

Community post
--------------

   We have been evaluating ``turbo-picard`` as a command-by-command upgrade
   path for Picard-heavy workflows. The interesting part is not a claim to
   replace all of Picard at once. It is that you can pick one expensive command,
   run upstream Picard and ``turbo-picard`` on the same representative shard,
   compare the exact outputs the workflow consumes, and only widen the rollout
   if that command is boring on real data.

How to adapt these
------------------

Replace the generic parts with the result you actually have:

* the command that was tested;
* the workflow shape;
* the kind of input used;
* the exact next step.

Then point people to:

* :doc:`first-command`
* :doc:`evaluation-playbook`
* :doc:`faq`
* :doc:`share-results`
* :doc:`propose-it-in-a-workflow-repo`

If you need the repo-side templates too, see ``packaging/outreach/``.
