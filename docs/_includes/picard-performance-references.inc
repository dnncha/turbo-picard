References
----------

.. [1]
   Broad Institute, `Picard frequently asked questions <https://broadinstitute.github.io/picard/faq.html>`__, especially the ``MarkDuplicates`` runtime, heap, hard-memory and GC notes.

.. [2]
   GATK community, `Picard MarkDuplicates failing with Java heap space after 124.67 minutes <https://gatk.broadinstitute.org/hc/en-us/community/posts/31390716098203-Picard-MarkDuplicates-Failing>`__, November 2024. This is an anecdote, not a representative benchmark.

.. [3]
   Picard issue tracker, `Memory footprint on MarkDuplicates? <https://github.com/broadinstitute/picard/issues/1773>`__, January 2022.

.. [4]
   AMD, `Picard BAM Processing: AOCL as a Practical Option for Accelerating Picard Workflows <https://www.amd.com/en/developer/resources/technical-articles/2026/aocl-for-picard-workflows.html>`__, May 2026.

.. [5]
   Turbo Picard, :doc:`benchmark documentation <benchmarks>` and `saved benchmark data <https://github.com/dnncha/turbo-picard/blob/main/docs/site/assets/benchmark-data.json>`__, verified 14 August 2026.

.. [6]
   Turbo Picard, `saved raw benchmark output <https://github.com/dnncha/turbo-picard/blob/main/docs/site/assets/bench-suite-output.txt>`__. The 61.31-second and 0.97-second totals are sums of the displayed command medians; the 1,000-run scenario is a transparent extrapolation of those same fixtures.

.. [7]
   Turbo Picard, :doc:`Picard versus Turbo Picard <picard-vs-turbo-picard>`. The RSS result is fixture-specific.

.. [8]
   GATK, `MarkDuplicates (Picard) <https://gatk.broadinstitute.org/hc/en-us/articles/360037052812-MarkDuplicates-Picard>`__.

.. [9]
   Turbo Picard, :doc:`compatibility contract <compatibility-contract>`.

.. [10]
   Turbo Picard, `pinned NA12878 mitochondrial BAM comparison <https://github.com/dnncha/turbo-picard/blob/main/benchmarks/real-data/gatk-na12878-mito/evidence/real-data-comparison.md>`__.

.. [11]
   Turbo Picard, :doc:`workflow validation protocol <production-readiness>`.

.. [12]
   HTSlib, `Samtools duplicate-marking algorithm and workflow <https://www.htslib.org/algorithms/duplicate.html>`__.

.. [13]
   Sambamba, `markdup documentation <https://lomereiter.github.io/sambamba/docs/sambamba-markdup.html>`__.

.. [14]
   FastDup, `project documentation <https://github.com/zzhofict/FastDup>`__ and Zhang et al., *Bioinformatics* 41(12), 2025.

.. [15]
   SAMBLASTER, `project documentation <https://github.com/GregoryFaust/samblaster>`__.

.. [16]
   Riker, `project documentation and reproducible performance notes <https://github.com/fulcrumgenomics/riker>`__, July 2026.

.. [17]
   GATK, `MarkDuplicatesSpark <https://gatk.broadinstitute.org/hc/en-us/articles/54698851176603-MarkDuplicatesSpark>`__, updated August 2026.

.. [18]
   Sentieon, `products and performance claims <https://www.sentieon.com/products/>`__. These figures are vendor claims and should be evaluated on the target workflow.

.. [19]
   NVIDIA, `Parabricks fq2bam documentation <https://docs.nvidia.com/clara/latest/Documentation/ToolDocs/man_fq2bam.html>`__. This is a vendor claim tied to specified GPU hardware.
