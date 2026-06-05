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
* ``FixMateInformation`` compares stable SAM text with incidental ``@PG`` lines
  ignored.
* ``SetNmMdAndUqTags`` compares stable SAM text with incidental ``@PG`` lines
  ignored and optional tags sorted before comparison.

Those rules are encoded in ``tools/verify_real_data_evidence.py`` and the
command-specific parity scripts under ``tools/``. The saved evidence names the
comparison method for each command so reviewers can see the boundary.

CRAM preprocessing parity
-------------------------

``tools/verify_basic_cram_parity.sh`` compares Picard and ``turbo-picard`` on a
shared reference-backed CRAM shard for the hot preprocessing path:

* ``MarkDuplicates`` — semantic duplicate flags/tags and metrics (via
  ``tools/compare_markduplicates.py``)
* ``SortSam``, ``ViewSam``, ``RevertSam`` — record text via ``samtools view``
* ``MergeSamFiles`` — coordinate-sorted SAM record multiset digest (same rule
  as BAM real-data evidence)
* ``CleanSam`` — per-read MAPQ and CIGAR
* ``SamToFastq`` — first-end and second-end FASTQ
* ``ReplaceSamHeader`` — header lines and record order, ignoring incidental
  ``@PG`` command lines from the viewer
* ``AddOrReplaceReadGroups`` — stable SAM text
* ``ValidateSamFile`` — SUMMARY histogram rows
* ``FixMateInformation`` — stable SAM text
* ``SetNmMdAndUqTags`` — stable SAM text with optional tags sorted before
  comparison
* ``CollectQualityYieldMetrics``, ``CollectAlignmentSummaryMetrics``,
  ``CollectInsertSizeMetrics``, ``CollectBaseDistributionByCycle``,
  ``MeanQualityByCycle``, ``QualityScoreDistribution``, and
  ``CollectGcBiasMetrics`` (detail and summary) — stable metrics rows

``BuildBamIndex`` is checked on BAM, not CRAM. Picard 3.4.x rejects CRAM input
for that command, so the CRAM verifier does not claim CRAM index parity there.

``tools/verify_markdup_cram_parity.sh`` runs semantic MarkDuplicates parity on
every checked-in MarkDuplicates fixture through CRAM input and output.

``tools/verify_gatk_preprocessing_combo_parity.sh`` runs the GATK mitochondrial
BAM preprocessing chain: MarkDuplicates, SortSam on the raw shard, SortSam on
the markdup output, FixMateInformation after a queryname resort, and
SetNmMdAndUqTags with ``fixtures/reference/chrM.fa``.

``tools/verify_gatk_mito_bam_parity.sh`` exercises the same GATK mitochondrial
shard on native BAM I/O (ViewSam through chart metrics, plus BuildBamIndex,
RevertSam, SamToFastq, CollectMultipleMetrics, CollectWgsMetrics, and
FixMateInformation). ``tools/verify_gatk_mito_cram_parity.sh`` and
``tools/verify_gatk_preprocessing_combo_cram_parity.sh`` exercise the GATK
mitochondrial fixture through CRAM input and output. They prefer the checked-in
``benchmarks/real-data/gatk-na12878-mito-cram/input.cram`` shard (CRAM 3.0 for
Picard 3.4.x compatibility). The CRAM combo script runs MarkDuplicates, SortSam
on the raw shard, SortSam on the markdup output, and SetNmMdAndUqTags on the
sorted markdup output. It also checks ViewSam, CleanSam, AddOrReplaceReadGroups,
ValidateSamFile, and the same metrics commands covered by the checked-in CRAM
real-data bundle. Maintainer regeneration for CRAM real-data evidence is
``tools/bootstrap_gatk_mito_cram_evidence.sh``; checked-in bundles are
validated with ``tools/validate_gatk_mito_cram_evidence.sh`` (also run in CI).

Shared comparison helpers live in ``tools/parity_compare.py``. Real-data
audits accept CRAM inputs when ``--reference-fasta`` is supplied to
``tools/compare_real_data.py`` and ``tools/audit_real_data.py``.

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
