Command coverage
================

``turbo-picard`` focuses on common high-value Picard surfaces. Some commands are
native for the documented scope, and many are partially native with fallback for
advanced or uncommon surfaces.

Common native command examples
------------------------------

.. code-block:: bash

   picard MarkDuplicates I=input.bam O=marked.bam M=metrics.txt
   picard SortSam I=input.bam O=coordinate.bam SORT_ORDER=coordinate
   picard CleanSam I=input.bam O=cleaned.bam
   picard MergeSamFiles I=lane1.bam I=lane2.bam O=merged.bam SORT_ORDER=coordinate
   picard BuildBamIndex I=coordinate.bam O=coordinate.bai
   picard SamToFastq I=input.bam FASTQ=r1.fastq SECOND_END_FASTQ=r2.fastq
   picard FastqToSam F1=r1.fastq F2=r2.fastq O=unmapped.bam SM=sample RG=rg1
   picard AddOrReplaceReadGroups I=input.bam O=rg.bam RGID=1 RGLB=lib RGPL=ILLUMINA RGPU=unit RGSM=sample
   picard CollectAlignmentSummaryMetrics I=input.bam O=alignment_metrics.txt
   picard CollectQualityYieldMetrics I=input.bam O=quality_yield_metrics.txt
   picard CreateSequenceDictionary R=reference.fa O=reference.dict
   picard NormalizeFasta I=reference.fa O=normalized.fa LINE_LENGTH=100
   picard BedToIntervalList I=targets.bed O=targets.interval_list SD=reference.dict

Metrics and repair examples
---------------------------

.. code-block:: bash

   picard QualityScoreDistribution I=input.bam O=quality_distribution.txt CHART=quality_distribution.pdf
   picard MeanQualityByCycle I=input.bam O=mean_quality_by_cycle.txt CHART=mean_quality_by_cycle.pdf
   picard CollectInsertSizeMetrics I=input.bam O=insert_size_metrics.txt H=insert_size_histogram.pdf
   picard CollectWgsMetrics I=input.bam O=wgs_metrics.txt R=reference.fa COUNT_UNPAIRED=true
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

Machine-readable coverage
-------------------------

The canonical command matrix lives in ``docs/command-matrix.yml``. It records
the current status, parity script, native scope, and fallback scope for each
documented command.

.. literalinclude:: command-matrix.yml
   :language: yaml
