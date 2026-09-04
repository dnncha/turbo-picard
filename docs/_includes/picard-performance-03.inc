What the alternatives are good at
---------------------------------

There is no honest “best Picard alternative” without a workload.

samtools ``markdup``
~~~~~~~~~~~~~~~~~~~~

Use samtools when you want a mature, widely deployed toolchain and are willing to adopt its workflow and semantics. Its documented duplicate-marking flow commonly groups reads, runs ``fixmate``, coordinate-sorts, and then runs ``markdup``; the stages can be piped to avoid intermediate compressed files. [12]_

That can be an excellent pipeline. It is not the same migration as replacing one Picard command with a familiar command-line surface.

Sambamba
~~~~~~~~

Sambamba offers its own multithreaded ``markdup`` command and says it uses the same duplicate criteria as Picard. Its documentation also warns that external sort is not implemented and that memory grows by about 2 GB for each 100 million reads. [13]_

It is a strong fit when its CLI, option set and memory profile match the workflow.

FastDup
~~~~~~~

FastDup focuses on duplicate marking. Its authors report roughly 20× speed over Picard and 8× over MarkDuplicatesSpark with the same thread count. It is available through Bioconda and targets coordinate-sorted SAM/BAM input. [14]_

Its own limitations are worth reading: although duplicate sets are described as identical, the particular read selected as the duplicate may differ because of sorting stability. That may be harmless in one workflow and disqualifying in another.

SAMBLASTER
~~~~~~~~~~

SAMBLASTER is most attractive immediately after an aligner, where it can mark duplicates in a read-name-grouped SAM stream and avoid writing and rereading an intermediate alignment file. It solves the problem at a different point in the pipeline. [15]_

riker
~~~~~

Riker is a serious Rust rethinking of sequencing QC. Its authors report 12–38× speedups over Picard across WGS, exome and RNA-seq metrics, and its ``multi`` command computes several metrics in one pass. [16]_

Riker is explicit that it is not a drop-in Picard replacement. It uses a different CLI, simplifies output formats, changes column names and intentionally fixes behaviours that can produce slightly different results. That is a feature when you are modernising QC. It is a migration when you are preserving an established contract.

MarkDuplicatesSpark
~~~~~~~~~~~~~~~~~~~

GATK's MarkDuplicatesSpark can combine duplicate marking and coordinate sorting and use multiple cores. GATK says it is typically about 15% faster than ``MarkDuplicates`` plus ``SortSam`` at two cores and can scale toward 16 cores. It also documents behavioural differences, extra work for coordinate-sorted input, optional metrics and a recommendation of at least 16 GB for a typical 30× WGS BAM. [17]_

Sentieon and NVIDIA Parabricks
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

For organisations prepared to adopt a commercial suite or GPU workflow, the performance ceiling is much higher. Sentieon markets CPU-based, Picard/GATK-concordant pipelines and claims 30× WGS in under 30 minutes for under $2 of compute. NVIDIA says Parabricks ``fq2bam`` can process a 30× genome in as little as six minutes on a DGX system. [18]_ [19]_

Those are broader product and infrastructure decisions, not single-command substitutions.

Where Turbo Picard fits
~~~~~~~~~~~~~~~~~~~~~~~

Turbo Picard is most useful when the current workflow is already written in Picard's language and the team wants to change one boundary at a time.

Its advantage is not that every native command must beat every specialist. Its advantage is the combination of:

-  familiar Picard-shaped invocations;
-  explicit command-and-option scope;
-  command-specific parity evidence;
-  a machine-readable preflight;
-  upstream fallback;
-  a native binary with a low baseline cost.

The positioning is simple: **accelerate one Picard step without betting the pipeline.**
