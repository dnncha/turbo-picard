Where the time and memory go
----------------------------

Picard's own FAQ is a good place to start because it describes the sort of run people actually have to provision. Broad calls ``MarkDuplicates`` memory-intensive. Its historical examples include roughly one hour for an 8.6 GB, 63-million-record file and about 2.5 hours for a 20 GB, 133-million-record file. The same FAQ discusses a 2 GB Java heap alongside a 10 GB hard memory limit. [1]_

Those numbers are old, hardware-dependent examples, not a benchmark for today's machines. They still make the resource problem concrete. A scheduler has to care about more than ``-Xmx``: the process also needs native memory, buffers, file handles, scratch space and enough I/O to keep the command moving.

The failure modes are familiar. A public GATK thread shows ``MarkDuplicates`` running for more than two hours before dying with Java heap space. A Picard issue discusses jobs killed after crossing a cluster memory limit. [2]_ [3]_ These are anecdotes, but useful ones. They show why “give it more heap” is often the start of the investigation rather than the end.

The JVM is only one line on the bill
------------------------------------

For a short command, JVM startup and baseline memory are easy to see. For a large alignment file, they may barely matter. Sorting, duplicate bookkeeping, reference access, temporary files and BAM compression can cost much more.

AMD published a neat example in May 2026. It kept Picard 3.4.0 and the workflow logic, but changed the compression path on an AMD EPYC system. On a 34.7-million-read paired-end dataset, elapsed time fell from 5.12 minutes to 2.85 minutes. [4]_ The codec mattered enough to cut the run by 44%.

That is why I would not diagnose a slow Picard job from the language it is written in. Measure the process. I want wall time, CPU time, peak RSS, temporary disk use and the compression settings next to the input description. Then I want to know whether the output still matches.

Anything less makes a speedup hard to interpret.

Why small jobs make the overhead obvious
----------------------------------------

The saved Turbo Picard suite currently covers 32 command-level fixtures against Picard 3.4.0. All 32 parity checks pass. On those fixtures, the speedups range from 22.88× to 272.12×, with an 84.52× geometric mean. ``MarkDuplicates`` is 28.70× faster on its saved fixture. [5]_

Those numbers need the next sentence: these are small fixtures.

They do not tell us that an 84× whole-genome speedup is waiting in a production pipeline. They show something more modest and, for scattered workflows, still useful. The fixed cost of a Picard process is large compared with the useful work in a tiny command.

Add the displayed medians for the 32 fixture runs and Picard takes 61.31 seconds; Turbo Picard takes 0.97 seconds. Run that same tiny suite 1,000 times and the arithmetic becomes about 17 hours versus 16.2 minutes. [6]_ I would read that as a CI and task-launch example, not as a WGS forecast.

Memory shows the same effect. One focused ``MarkDuplicates`` copy-path fixture records median peak RSS of about 1.2 GB for Picard and 8.7 MB for Turbo Picard. [7]_ That roughly 140× gap is a property of that fixture. Larger inputs and more demanding options need more state.

The reason I care about the baseline is scheduling. A task that asks for less memory is easier to pack onto a node. Multiply the request by hundreds of samples or shards and the difference can show up as queue time, node count or OOM kills.

Why more heap can make something else worse
-------------------------------------------

Picard gives you several knobs: ``MAX_RECORDS_IN_RAM``, ``SORTING_COLLECTION_SIZE_RATIO``, ``MAX_FILE_HANDLES_FOR_READ_ENDS_MAP`` and ``TMP_DIR`` among them. [8]_ They trade memory against temporary files, file handles and I/O.

Raise the number of records kept in memory and you may spill less to disk, but RAM use rises. Push the other way and the process may fit into a smaller memory request while spending more time on storage. Neither setting is “the fast one” in isolation.

I prefer to measure the whole process with ``/usr/bin/time -v``, cgroup metrics or scheduler telemetry. Peak RSS, scratch use and wall time belong in the same result. ``-Xmx`` alone does not tell you what the job cost.
