Which alternative fits which job?
-------------------------------

The useful comparison is not “which Picard replacement is fastest?” The tools below solve different versions of the problem.

samtools ``markdup``
~~~~~~~~~~~~~~~~~~~~

If I were happy to change the duplicate-marking workflow as well as the executable, samtools would be an obvious candidate. Its documented flow groups reads, runs ``fixmate``, coordinate-sorts and then runs ``markdup``. Those stages can be piped so you do not have to write every intermediate compressed file. [12]_

That is a mature, widely used route. It is also a different migration from swapping one Picard-shaped command.

Sambamba
~~~~~~~~

Sambamba has a multithreaded ``markdup`` and documents duplicate criteria intended to match Picard. Its documentation also says external sort is not implemented and estimates roughly 2 GB of memory for each 100 million reads. [13]_

If its command line and memory behaviour suit the pipeline, it deserves a benchmark on the real input.

FastDup
~~~~~~~

FastDup is much more focused: duplicate marking on coordinate-sorted SAM/BAM. Its authors report about 20× the speed of Picard and 8× MarkDuplicatesSpark at the same thread count. [14]_

One detail is especially worth noticing. FastDup says duplicate sets are identical while the particular read chosen as the duplicate can differ because of sorting stability. Whether that matters depends on what comes next in your pipeline.

SAMBLASTER
~~~~~~~~~~

SAMBLASTER is interesting when you can change the pipeline earlier. It can mark duplicates in a read-name-grouped SAM stream straight after alignment, avoiding an intermediate write and reread. [15]_

If I were designing a new streaming alignment path, I would look at this before assuming the right answer was a faster stand-alone ``MarkDuplicates``.

riker
~~~~~

Riker tackles sequencing QC rather than trying to impersonate Picard. Its authors report 12–38× speedups across WGS, exome and RNA-seq metrics, and its ``multi`` command calculates several metrics in one pass. [16]_

It also says plainly that it is not a drop-in replacement: the CLI and output formats differ, and some behaviour is intentionally changed. That is attractive when you are willing to modernise the QC layer. It is more work when existing downstream code expects Picard's shape.

MarkDuplicatesSpark
~~~~~~~~~~~~~~~~~~~

GATK's MarkDuplicatesSpark combines duplicate marking and coordinate sorting and can use multiple cores. GATK says it is typically about 15% faster than ``MarkDuplicates`` plus ``SortSam`` at two cores and can scale toward 16 cores. The documentation also calls out behavioural differences and recommends at least 16 GB for a typical 30× WGS BAM. [17]_

For a team already deep in GATK, that may be a smaller operational change than introducing another tool.

Sentieon and NVIDIA Parabricks
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

Commercial CPU and GPU stacks move the decision to a different level. Sentieon advertises Picard/GATK-concordant CPU pipelines and claims a 30× WGS run in under 30 minutes for under $2 of compute. NVIDIA says Parabricks ``fq2bam`` can process a 30× genome in as little as six minutes on a DGX system. [18]_ [19]_

Those are vendor claims tied to their own software and hardware assumptions. They are relevant if you are choosing a broader pipeline platform, less so if you want to replace one command without changing the rest of the system.

Where I think Turbo Picard fits
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

I built Turbo Picard for the awkward middle case: the pipeline already speaks Picard, the step is expensive enough to bother you, and you do not want a performance project to become a workflow rewrite.

The useful features are mundane: familiar arguments, an explicit native-support check, command-specific parity tests, fallback to Picard and a small native process. None of those proves it is the fastest choice for your data.

They make it cheap to find out.
