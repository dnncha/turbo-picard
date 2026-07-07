Choose your first command
=========================

This page is for the point where you already believe ``turbo-picard`` is worth
trying, but you have not decided which Picard command should go first.

The best first trial is usually:

* slow enough that a speedup matters;
* common enough that the workflow owner will care;
* narrow enough that output comparison stays boring.

Ask the installed tool for a concrete trial contract once you have a candidate:

.. code-block:: bash

   turbo-picard trial MarkDuplicates I=input.bam O=marked.bam M=metrics.txt

The report gives the matching upstream Picard command, the ``turbo-picard``
command, declared outputs, fallback state, and evidence to keep with the
workflow review.

Start with ``MarkDuplicates``
-----------------------------

Use this first when:

* duplicate marking is one of the longest steps in preprocessing;
* the input is already coordinate-sorted;
* you want an easy side-by-side check with a BAM or CRAM plus a metrics file.

This is often the highest-value first trial because the runtime is noticeable
and the output comparison is straightforward.

Start with ``SortSam``
----------------------

Use this first when:

* the workflow repeatedly changes BAM or CRAM order between stages;
* sorting time is noticeable across many shards or samples;
* you want a simple file-to-file comparison.

This is a good choice when the workflow spends real time on reshaping files and
you want a clean before-and-after test.

Start with ``SamToFastq``
-------------------------

Use this first when:

* Picard export still sits on a remap, realignment, or handoff path;
* FASTQ generation is a repeated source of waiting;
* you want to compare plain downstream FASTQ outputs directly, including
  per-read-group output when the workflow splits by ``PU`` or ``ID``.

This is a good choice when export is clearly in the critical path and the team
already knows how to check the resulting FASTQ files.

Start with ``BuildBamIndex``
----------------------------

Use this first when:

* you want the smallest possible first substitution;
* the workflow contains many light Picard glue steps;
* you want a low-risk test before moving to heavier commands.

This is the right first step when the team mainly needs confidence that the
package behaves sensibly in the workflow at all.

What to avoid as the first trial
--------------------------------

Do not start with:

* commands outside the documented native scope;
* workflows that depend on exact Picard-rendered chart PDFs;
* toy inputs that do not look like the data you actually process.

Next step
---------

After choosing the command:

1. go to :doc:`quickstart` if you still need the installation path;
2. go to :doc:`evaluation-playbook` if you want the full trial flow;
3. use ``packaging/workflows/`` if you want starter files for ``WDL``,
   ``Nextflow``, or ``Snakemake``.
