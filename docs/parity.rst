What Parity Means
=================

``turbo-picard`` uses the word parity narrowly. It means a specific command,
with a specific input shape and option set, produced the same checked output as
upstream Picard under the comparison method named in the evidence.
It does not mean every Picard behavior has been reimplemented, and it does not
mean one small fixture proves safety for every cohort or assay.

This distinction matters. Picard is often part of a larger scientific claim,
not just a command line. If a faster replacement changes duplicate flags,
metrics tables, FASTQ pairing, validation status, or sidecar files in a way the
workflow depends on, the speedup is not useful.

What Gets Compared
------------------

The comparison target depends on the command:

* BAM or SAM transformations compare normalized record content, with headers
  ignored only when the command semantics do not require exact header identity.
* ``SortSam`` compares a coordinate-sorted record multiset so harmless tie
  ordering differences do not hide real record changes.
* ``MarkDuplicates`` compares duplicate flags and duplicate-related tags as
  semantic record data, and also compares the stable metrics table.
* Metrics commands compare stable metrics rows after removing generated comment
  headers.
* ``BuildBamIndex`` compares the exact BAI binary digest.
* ``SamToFastq`` compares first-end, second-end, and unpaired FASTQ outputs.
* ``ValidateSamFile`` compares the summary validation histogram and exit code.

Those rules are encoded in ``tools/verify_real_data_evidence.py`` and the
command-specific parity scripts under ``tools/``. The saved evidence names the
comparison method for each command so reviewers can see the boundary.

What Parity Does Not Prove
--------------------------

Passing parity evidence does not prove broad switching safety by itself. It
does not cover:

* untested Picard options;
* commands still marked as fallback-only or unsupported;
* every assay, aligner, read-group layout, UMI convention, or reference build;
* all malformed-input behavior;
* Picard chart rendering for metrics commands that currently emit lightweight
  PDF sidecars.

Treat the checked-in public data as a starting point. Before a large research
workflow switches, run side-by-side comparisons on representative inputs from
that workflow and keep the JSON, Markdown, input SHA-256, source citation,
command line, Picard version, and turbo-picard version with the analysis record.

How To Use The Evidence
-----------------------

For a production-like change:

1. Start with ``turbo-picard`` beside upstream Picard, not the ``picard`` shim.
2. Compare the exact commands your workflow uses.
3. Add representative real-data evidence with ``tools/compare_real_data.py``.
4. Run ``python3 tools/verify_real_data_evidence.py --release-ready`` before a
   scientific release or Bioconda submission.
5. Switch only the proven commands, and keep upstream Picard available as
   fallback for everything else.

The pipeline guide gives a fuller sequence in :doc:`adoption`; the
current checked-in benchmark and real-data evidence is described in
:doc:`benchmarks`.
