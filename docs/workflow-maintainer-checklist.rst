Workflow maintainer checklist
=============================

Use this page when you are the maintainer deciding whether a ``turbo-picard``
evaluation is ready for a discussion, a pull request, or a wider rollout.

Ready for a narrow trial if most of these are true
--------------------------------------------------

* one Picard step shows up repeatedly in wall-time complaints;
* the workflow already calls Picard in a stable place;
* you can get one representative BAM or CRAM shard without operational drama;
* the downstream files you care about are easy to compare;
* you are willing to decide one command at a time instead of arguing about a
  full replacement.

Best first commands
-------------------

Good first choices are usually:

* ``MarkDuplicates`` if duplicate marking is a visible cost;
* ``SortSam`` if reorder work keeps happening between stages;
* ``SamToFastq`` if FASTQ export is still on a realignment or remap path;
* ``BuildBamIndex`` if you want a very small low-risk first substitution.

Not a good first target
-----------------------

Avoid starting with:

* commands outside the documented native scope;
* workflows that depend on exact Picard chart PDF rendering;
* toy fixtures that do not resemble the real data shape.

Minimum evidence before a PR
----------------------------

Before proposing a workflow change, make sure you have:

1. run upstream Picard and ``turbo-picard`` on the same representative shard;
2. compared the exact files your downstream workflow consumes;
3. kept the command lines, timings, metrics, and output paths together;
4. decided what the narrow next step is for that one command.

Discussion first or PR first?
-----------------------------

Open a discussion first when:

* the command is not yet under active review;
* the team still needs to agree that the trial is worth doing;
* the workflow repository is sensitive to broad change and needs early alignment.

Open a PR first when:

* the command boundary is already clear;
* the comparison evidence exists;
* the proposed change is intentionally narrow.

Useful next pages
-----------------

* :doc:`first-command`
* :doc:`evaluation-playbook`
* :doc:`propose-it-in-a-workflow-repo`
* :doc:`faq`
* :doc:`share-results`
