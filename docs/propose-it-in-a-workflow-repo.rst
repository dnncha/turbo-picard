Propose it in a workflow repo
=============================

This page is for the point where a command-level trial already looks good and
you want to bring ``turbo-picard`` into an existing workflow repository without
creating unnecessary noise.

Keep the proposal narrow
------------------------

Start from one command in one workflow shape.

Good proposal scope:

* one Picard step that already hurts in wall time;
* one workflow task, process, or rule;
* one reviewed comparison on representative data.

Bad proposal scope:

* "replace Picard everywhere";
* multiple commands before the first one is well-defended;
* a broad refactor mixed with the evaluation itself.

What to include in the PR or issue
----------------------------------

A useful proposal usually includes:

* the exact Picard command that was tested;
* why that command was chosen;
* the workflow context it fits;
* what outputs were compared against upstream Picard;
* whether upstream Picard remains available as fallback;
* the narrow next step you are proposing.

Example framing
---------------

   This proposal is limited to a narrow ``turbo-picard`` evaluation path for
   one Picard-shaped step. The goal is not to replace all of Picard. The goal
   is to test whether this one command can stay in the same workflow boundary,
   keep the output reviewable against upstream Picard, and reduce wall time
   enough to justify a small rollout.

What to avoid
-------------

Avoid:

* benchmark claims with no workflow context;
* proposals that skip representative input comparison;
* language that suggests full interchangeability before the evidence exists;
* hiding fallback or rollback options from reviewers.

What to link for reviewers
--------------------------

Useful links to include:

* :doc:`picard-vs-turbo-picard`
* :doc:`first-command`
* :doc:`evaluation-playbook`
* :doc:`faq`
* :doc:`share-results`
* :doc:`workflow-maintainer-checklist`

If you need a short blurb first, see :doc:`message-examples`.

When to post a discussion instead
---------------------------------

Use a GitHub discussion first when:

* the command is not yet under active PR review;
* maintainers need to agree on whether the trial is worth doing;
* the repo is sensitive to broad workflow changes and needs early alignment.

Use a pull request when:

* the command boundary is already clear;
* the comparison evidence exists;
* the proposed change is intentionally narrow.
