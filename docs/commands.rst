Command coverage
================

``turbo-picard`` exposes the full Picard 3.4.0 command surface. Accelerated
commands run natively in Rust when possible; every other Picard 3.4.0 command is
delegated transparently to upstream Picard when it is installed or
auto-discovered.

List every upstream command with:

.. code-block:: bash

   turbo-picard --list-commands

Common command examples
-----------------------

These examples cover the accelerated preprocessing and QC path. Check the
machine-readable matrix below for the exact accelerated versus delegated split.

.. code-block:: bash

   picard MarkDuplicates I=input.bam O=marked.bam M=metrics.txt
   picard SortSam I=input.bam O=coordinate.bam SORT_ORDER=coordinate
   picard CleanSam I=input.bam O=cleaned.bam
   picard MergeSamFiles I=lane1.bam I=lane2.bam O=merged.bam SORT_ORDER=coordinate
   picard BuildBamIndex I=coordinate.bam O=coordinate.bai
   picard SamToFastq I=input.bam FASTQ=r1.fastq SECOND_END_FASTQ=r2.fastq
   picard SamToFastq I=input.bam OUTPUT_PER_RG=true OUTPUT_DIR=fastq-by-rg
   picard FastqToSam F1=r1.fastq F2=r2.fastq O=unmapped.bam SM=sample RG=rg1
   picard FastqToSam F1=reads_R1_001.fastq F2=reads_R2_001.fastq O=unmapped.bam SM=sample RG=rg1 USE_SEQUENTIAL_FASTQS=true
   picard ViewSam I=input.bam > view.sam
   picard ReplaceSamHeader I=input.bam O=reheadered.bam H=replacement-header.sam
   picard AddOrReplaceReadGroups I=input.bam O=rg.bam RGID=1 RGLB=lib RGPL=ILLUMINA RGPU=unit RGSM=sample
   picard CollectAlignmentSummaryMetrics I=input.bam O=alignment_metrics.txt
   picard CollectQualityYieldMetrics I=input.bam O=quality_yield_metrics.txt
   picard CreateSequenceDictionary R=reference.fa O=reference.dict
   picard NormalizeFasta I=reference.fa O=normalized.fa LINE_LENGTH=100
   picard BedToIntervalList I=targets.bed O=targets.interval_list SD=reference.dict
   turbo-picard AccelerationStatus
   turbo-picard capabilities --json
   turbo-picard doctor
   turbo-picard explain MarkDuplicates I=input.bam O=marked.bam M=metrics.txt
   turbo-picard explain --json MarkDuplicates I=input.bam O=marked.bam M=metrics.txt
   turbo-picard explain --format json MarkDuplicates I=input.bam O=marked.bam M=metrics.txt
   turbo-picard trial MarkDuplicates I=input.bam O=marked.bam M=metrics.txt
   turbo-picard trial --json SortSam I=input.bam O=coordinate.bam SORT_ORDER=coordinate

Use the text ``explain`` output for interactive checks. Use ``--json`` when a
workflow wrapper, CI check, or platform module needs to read ``schema_version``,
command status, execution path, fallback command, and declared output arguments
without parsing human text.

Use ``trial`` when checking whether an installed workflow is a good first
candidate for side-by-side evaluation. Text output gives copyable Picard and
``turbo-picard`` commands plus comparison and evidence targets. JSON output
keeps the same contract for workflow-manager or CI checks.

Metrics and repair examples
---------------------------

.. code-block:: bash

   picard QualityScoreDistribution I=input.bam O=quality_distribution.txt CHART=quality_distribution.pdf
   picard MeanQualityByCycle I=input.bam O=mean_quality_by_cycle.txt CHART=mean_quality_by_cycle.pdf
   picard CollectBaseDistributionByCycle I=input.bam O=base_distribution.txt CHART=base_distribution.pdf
   picard CollectInsertSizeMetrics I=input.bam O=insert_size_metrics.txt H=insert_size_histogram.pdf
   picard CollectGcBiasMetrics I=input.bam O=gc_bias_detail.txt S=gc_bias_summary.txt CHART=gc_bias.pdf R=reference.fa
   picard CollectHsMetrics I=input.bam O=hs_metrics.txt BAIT=baits.interval_list TARGET=targets.interval_list R=reference.fa
   picard CollectMultipleMetrics I=input.bam O=multiple_metrics PROGRAM=CollectInsertSizeMetrics
   picard CollectWgsMetrics I=input.bam O=wgs_metrics.txt R=reference.fa COUNT_UNPAIRED=true
   picard CollectWgsMetrics I=input.bam O=wgs_metrics.fast.txt R=reference.fa COUNT_UNPAIRED=true USE_FAST_ALGORITHM=true
   picard FixMateInformation I=queryname.bam O=fixed.bam ASSUME_SORTED=true SORT_ORDER=queryname
   picard RevertSam I=aligned.bam O=unmapped.bam
   picard SetNmMdAndUqTags I=coordinate.bam O=tagged.bam R=reference.fa
   picard ValidateSamFile I=input.bam MODE=SUMMARY

VCF and interval examples
-------------------------

.. code-block:: bash

   picard UpdateVcfSequenceDictionary I=input.vcf O=updated.vcf SD=reference.dict CREATE_INDEX=true
   picard GatherVcfs I=shard1.vcf I=shard2.vcf O=gathered.vcf CREATE_INDEX=true
   picard SortVcf I=unsorted.vcf O=sorted.vcf SD=reference.dict CREATE_INDEX=true
   picard MergeVcfs I=batch1.vcf I=batch2.vcf O=merged.vcf CREATE_INDEX=true
   picard LiftoverVcf I=input.vcf O=lifted.vcf CHAIN=build.chain REJECT=rejected.vcf R=target.fa
   picard IntervalListTools I=a.interval_list I=b.interval_list O=merged.interval_list ACTION=CONCAT SORT=true UNIQUE=true

For large VCF inputs, ``GatherVcfs`` and
``UpdateVcfSequenceDictionary`` write records as they are read. ``SortVcf``
and ``MergeVcfs`` use bounded temporary runs; set ``TMP_DIR`` to local scratch
space and ``MAX_RECORDS_IN_RAM`` to the desired run size. For coordinate-sorted
BAM or CRAM input, set either of those options on ``SamToFastq`` to stage a
bounded queryname sort before pairing mates.

Machine-readable coverage
-------------------------

The canonical command matrix lives in ``docs/command-matrix.yml``. It records
the current status, parity script, native scope, and fallback scope for each
Picard 3.4.0 command plus turbo-only utilities such as
``AccelerationStatus``, ``doctor``, ``explain``, and ``trial``.
The ``capabilities`` utility combines that matrix with the checked-in benchmark
evidence in one schema-versioned response for coding agents and CI policies.

Current matrix status summary:

* ``38 accelerated`` commands with native or partial-native Rust implementations
* ``88 delegated`` Picard 3.4.0 commands forwarded to upstream Picard

Accelerated command status:

* ``AddOrReplaceReadGroups``: ``native``
* ``AccelerationStatus``: ``native``
* ``BedToIntervalList``: ``native``
* ``BuildBamIndex``: ``native``
* ``capabilities``: ``native``
* ``CleanSam``: ``partial-native``
* ``CollectAlignmentSummaryMetrics``: ``partial-native``
* ``CollectBaseDistributionByCycle``: ``partial-native``
* ``CollectGcBiasMetrics``: ``partial-native``
* ``CollectHsMetrics``: ``partial-native`` for the core ALL_READS
  hybrid-capture metrics, histogram, per-target coverage, and per-base
  coverage; unsupported advanced options remain delegated.
* ``CollectQualityYieldMetrics``: ``native``
* ``CollectWgsMetrics``: ``partial-native``. ``INCLUDE_BQ_HISTOGRAM`` defaults
  to ``false`` to match Picard 3.4.0 histogram output. ``USE_FAST_ALGORITHM=true``
  stays native and defaults ``SAMPLE_SIZE`` to ``0`` unless it is set explicitly.
  ``TURBO_PICARD_WGS_FAST_DEFAULT=true`` applies that sample-size default when
  the command line does not set ``USE_FAST_ALGORITHM``.
* ``doctor``: ``native``
* ``explain``: ``native``
* ``trial``: ``native``
* ``CreateSequenceDictionary``: ``native``
* ``FastqToSam``: ``partial-native``
* ``GatherVcfs``: ``partial-native``
* ``MarkDuplicates``: ``partial-native``
* ``MergeSamFiles``: ``partial-native``
* ``MergeVcfs``: ``partial-native``
* ``MeanQualityByCycle``: ``partial-native``
* ``NormalizeFasta``: ``native``
* ``QualityScoreDistribution``: ``partial-native``
* ``ReplaceSamHeader``: ``partial-native``
* ``SamToFastq``: ``partial-native``
* ``SortSam``: ``partial-native``
* ``SortVcf``: ``partial-native``
* ``UpdateVcfSequenceDictionary``: ``partial-native``
* ``ViewSam``: ``partial-native``
* ``CollectInsertSizeMetrics``: ``partial-native``
* ``CollectMultipleMetrics``: ``partial-native``
* ``FixMateInformation``: ``partial-native``
* ``IntervalListTools``: ``partial-native``
* ``LiftoverVcf``: ``partial-native``
* ``RevertSam``: ``partial-native``
* ``SetNmMdAndUqTags``: ``partial-native``
* ``ValidateSamFile``: ``partial-native``

.. literalinclude:: command-matrix.yml
   :language: yaml
