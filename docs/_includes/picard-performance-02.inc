Compatibility is the hard part
------------------------------

A valid BAM is a low bar. The next stage in the pipeline may care about exactly which read carries the duplicate flag, which tags were written, how metrics were calculated, whether an index appeared and what happened on bad input.

For duplicate marking, I would check at least:

-  duplicate flags and the choice of representative read;
-  optical-duplicate and duplicate-set tags;
-  barcode or UMI behaviour when those options are in use;
-  secondary and supplementary alignments;
-  metrics, library-size estimates, sort order and index output;
-  exit codes and failure behaviour.

Two tools can report the same duplicate count and still disagree record by record. Sometimes that difference is harmless. Sometimes it changes downstream work. The pipeline owner has to decide which is true.

Turbo Picard documents compatibility by command and option. A path can be byte-compatible, semantically compatible, native only for a named option set, delegated to Picard, or unsupported. An option that changes output must be implemented, rejected or delegated; silently ignoring it is not acceptable. [9]_

The comparison itself also depends on the command. Exact BAM bytes can differ for harmless reasons such as header fields or tie ordering. ``BuildBamIndex`` can sensibly be checked byte for byte. ``SamToFastq`` should be judged by the FASTQ it writes. ``ValidateSamFile`` needs its issue histogram and exit code checked.

This is the part I would spend the most time on. Saving ten minutes is not worth discovering three stages later that the replacement changed a field nobody thought to compare.

What Turbo Picard does, and what it does not
--------------------------------------------

Turbo Picard keeps Picard command names and ``KEY=VALUE`` arguments where it has a native implementation. Before running a command, ``turbo-picard capabilities --json`` shows the available surface and ``turbo-picard explain --json …`` explains the exact invocation.

Unsupported work can fall back to an installed Picard. That matters because coverage is deliberately incomplete. Turbo Picard is not a reimplementation of every Picard command, and I would not treat it as a blind drop-in replacement.

The intended migration is boring on purpose: inspect one command, run it both ways, compare the outputs, then decide whether that one step is worth changing.

Evidence so far
---------------

For Turbo Picard 0.1.12, the saved fixture suite reports:

+----------------------------------+----------------------------------+
| Evidence                         | Saved result                     |
+==================================+==================================+
| Parity-checked commands          | 32 / 32 pass                     |
+----------------------------------+----------------------------------+
| Slowest saved speedup            | 22.88×                           |
+----------------------------------+----------------------------------+
| Geometric-mean saved speedup     | 84.52×                           |
+----------------------------------+----------------------------------+
| Fastest saved speedup            | 272.12×                          |
+----------------------------------+----------------------------------+
| ``MarkDuplicates`` saved speedup | 28.70×                           |
+----------------------------------+----------------------------------+
| Focused ``MarkDuplicates``       | ~1.2 GB Picard vs ~8.7 MB Turbo  |
| median RSS                       | Picard                           |
+----------------------------------+----------------------------------+

There is also an older public comparison using a pinned NA12878 mitochondrial BAM of roughly 2 MB. It checks 12 commands with command-specific digests. Speedups run from 3.09× for ``SortSam`` to 28.49× for ``CollectInsertSizeMetrics``; ``MarkDuplicates`` is 9.95×. [10]_

I keep that result because it is reproducible and uses public data. I do not present it as fresh 0.1.12 evidence. It came from an earlier release and the input is tiny.

At this point I think the evidence is good enough to justify trying the tool. I do not think it is good enough to make a blanket production claim. The project's production-readiness protocol sets the same bar: test the exact command and options on representative data, keep Picard available, and stop or fall back if the comparison fails. [11]_
