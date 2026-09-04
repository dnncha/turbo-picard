The replacement trap is compatibility
-------------------------------------

It is easy to produce a valid BAM. It is harder to produce the BAM the next step expects.

Duplicate marking alone can involve:

-  which alignments participate in duplicate discovery;
-  how unclipped coordinates and orientation are calculated;
-  how the representative read is chosen;
-  optical-duplicate discovery;
-  barcode or UMI grouping;
-  secondary and supplementary alignments;
-  duplicate-set tags;
-  metrics and library-size estimates;
-  output ordering;
-  index creation;
-  exit status and failure behaviour.

Two tools can report the same number of duplicates and still disagree on individual reads. That may or may not matter downstream. The workflow owner has to decide—not the benchmark author.

Turbo Picard therefore uses an explicit compatibility contract. A command can be byte-compatible for a documented scope, semantically compatible, compatible only within a named option subset, delegated to upstream Picard, or unsupported. An output-changing option must be implemented, rejected clearly, or delegated. It must not be silently ignored. [9]_

The comparator also changes with the command. Exact BAM bytes are not always the right test because harmless header fields or equal-coordinate tie ordering can differ. For ``MarkDuplicates``, the comparison should cover duplicate flags, duplicate tags, duplicate-set metadata, optical counts and stable metrics. For ``BuildBamIndex``, exact BAI bytes make sense. For ``SamToFastq``, FASTQ bytes matter. For ``ValidateSamFile``, the issue histogram and exit code matter.

Compatibility is not a badge. It is a list of observable obligations.

What Turbo Picard actually is
-----------------------------

Turbo Picard is a native acceleration layer for existing Picard-shaped workflows.

It is designed around four ideas:

1. **Keep the interface familiar.** Existing Picard command names and ``KEY=VALUE`` arguments reduce migration work.
2. **Make native scope explicit.** ``turbo-picard capabilities --json`` and ``turbo-picard explain --json …`` let a person or workflow inspect what will run natively.
3. **Fall back instead of bluffing.** Unsupported commands or options can be delegated to an installed Picard command.
4. **Compare before switching.** The repository includes command-specific parity checks and reproducible benchmark artefacts.

It is not a complete reimplementation of every Picard command. It is not a blanket drop-in replacement. It is not a claim that a maintainer's fixture proves a clinical or cohort workflow safe.

That narrower description may sound less exciting. It is the reason the tool can be evaluated rationally.

What the current evidence says
------------------------------

As of Turbo Picard 0.1.12, the saved command suite reports:

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

A separate pinned public NA12878 mitochondrial BAM comparison, retained from an earlier Turbo Picard release, checks 12 commands using command-specific digests. On that roughly 2 MB input, the measured speedups range from 3.09× for ``SortSam`` to 28.49× for ``CollectInsertSizeMetrics``; ``MarkDuplicates`` measures 9.95×. The evidence file records the exact versions and input digest, so this is historical command-level evidence, not a fresh 0.1.12 benchmark. [10]_

This is enough evidence to justify a trial.

It is not enough evidence to justify an industry-wide performance promise.

The repository's own production-readiness protocol says that broad claims require production-scale evidence and independent reproduction. It asks workflow owners to test the exact command and option set on representative data, keep Picard available during migration, and stop or fall back on a mismatch. [11]_

That is the standard I want Turbo Picard to be judged by.
