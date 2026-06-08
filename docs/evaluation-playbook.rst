Evaluation playbook
===================

This page is the shortest path through the repo if you are evaluating
``turbo-picard`` for a real workflow or trying to explain that evaluation to
other people.

1. Decide whether a trial is worth doing
----------------------------------------

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
* ``SamToFastq`` for export-heavy realignment or remap paths, including
  per-read-group FASTQ output;
* ``FastqToSam`` for lane-sharded FASTQ ingestion before alignment or archival
  handoff;
* ``FixMateInformation`` when mate repair is still in the workflow and the
  queryname-sorted boundary is already stable;
* ``BuildBamIndex`` for a very small, low-risk first substitution.

3. Pick the workflow shape
--------------------------

Starter files live in ``packaging/workflows/``.

Use:

* ``markduplicates.wdl``, ``sortsam.wdl``, ``samtofastq.wdl``,
  ``fastqtosam.wdl``, or ``fixmateinformation.wdl`` for ``WDL`` /
  ``Cromwell``;
* ``markduplicates.nf``, ``sortsam.nf``, ``samtofastq.nf``,
  ``fastqtosam.nf``, or ``fixmateinformation.nf`` for ``Nextflow`` / nf-core
  style trials;
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
* ``packaging/workflows/trial-samtofastq.nf``
* ``packaging/workflows/trial-samtofastq.wdl``
* ``packaging/workflows/trial-fastqtosam.nf``
* ``packaging/workflows/trial-fastqtosam.wdl``
* ``packaging/workflows/trial-fixmateinformation.wdl``
* ``packaging/workflows/trial-fixmateinformation.nf``
* ``packaging/workflows/trial-config.yaml``

The shared ``trial-config.yaml`` now carries the main knobs those command-level
trials usually need: ``output_per_rg`` and ``rg_tag`` for ``SamToFastq``, plus
``use_sequential_fastqs`` for ``FastqToSam``.

The minimum standard is simple:

* run upstream Picard and ``turbo-picard`` on the same representative shard;
* compare the exact files your downstream workflow consumes;
* keep the command lines, timings, metrics, and outputs together.

For a fuller comparison bundle, use ``tools/audit_real_data.py`` or
``tools/compare_real_data.py``.

What this is not
----------------

This playbook is for getting to a responsible first trial quickly. It is not a
claim that every Picard workflow is ready to switch unchanged, and it is not a
reason to skip side-by-side checks on representative data.
