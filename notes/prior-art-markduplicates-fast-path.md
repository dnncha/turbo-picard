# Prior-art notes: MarkDuplicates fast path

Date: 2026-06-06
Status: private working note

This is an engineering prior-art screen, not legal advice.

## Bottom line

There is already substantial prior art around making duplicate marking faster
and smaller-memory than Picard.

That means the likely protectable part of `turbo-picard` is not:

- "duplicate marking, but faster";
- "single-pass duplicate marking";
- "smaller-memory duplicate marking";
- "duplicate marking without intermediate files."

Those themes are already heavily occupied.

If there is a viable invention here, it is narrower:

- ordered uniqueness proof for the no-duplicate case on coordinate-sorted BAM;
- bounded pair-resolution state for proving uniqueness;
- self-invalidating fast path that falls back to the general algorithm when the
  proof fails;
- direct BAM copy or minimal-output bypass after uniqueness is proven.

## Sources reviewed

### Picard MarkDuplicates documentation

Source:
[GATK / Picard MarkDuplicates docs](https://gatk.broadinstitute.org/hc/en-us/articles/360051306171-MarkDuplicates-Picard)

Relevant points:

- Picard compares 5' positions of reads and pairs.
- Duplicate groups are then resolved using summed base qualities.
- It supports barcode-assisted duplicate marking and duplicate-type tagging.

Takeaway:

- Picard itself is the compatibility baseline.
- It does not, from the user-facing docs, advertise a no-duplicate bypass proof
  path like the current `turbo-picard` fast path.

### samtools markdup algorithm documentation

Source:
[samtools duplicate marking algorithm](https://www.htslib.org/algorithms/duplicate.html)

Relevant points:

- samtools performs duplicate marking in a single pass through position-sorted
  data;
- it uses a moving memory window rather than loading the whole file;
- it treats duplicate detection as local in the sorted stream;
- it documents duplicate-chain handling and optional second-pass behavior.

Risk to our position:

- any broad claim around "single-pass duplicate marking on sorted BAM" is likely
  dead on arrival;
- any broad claim around "reduced memory through local windowing" is also weak.

What still looks potentially distinct:

- proving there are no duplicates and using that proof to avoid general
  duplicate processing entirely;
- direct-copy success behavior after uniqueness is established.

### biobambam

Source:
[biobambam paper abstract](https://arxiv.org/abs/1306.0836)

Relevant points:

- explicitly targets efficient BAM reading and read-pair collation without full
  resorting;
- reports more efficient runtime and memory than prior Picard-style approaches;
- includes duplicate marking among its target tasks.

Risk to our position:

- any claim around efficient pair collation or reduced-memory duplicate marking
  has to be differentiated from this family of work.

### SAMBLASTER

Source:
[SAMBLASTER paper abstract](https://arxiv.org/abs/1403.7486)

Relevant points:

- duplicate marking as a piped post-pass on read-sorted SAM;
- dramatically reduced runtime and complexity versus Picard and Sambamba;
- explicitly positioned as faster and lower-memory than Picard.

Risk to our position:

- broad "we are the fast low-memory alternative to Picard" has long been in the
  literature;
- pipeline bypass and low-overhead duplicate handling are not new at the
  product-category level.

### FastDup

Source:
[FastDup abstract](https://arxiv.org/abs/2505.06127)

Relevant points:

- claims up to 20x throughput speedup;
- explicitly uses a "speculation-and-test mechanism";
- claims identical output to Picard.

Risk to our position:

- this is the most important recent overlap because the name of the mechanism
  suggests conditional optimization plus validation/fallback rather than simple
  low-level speedups;
- we need to inspect the full paper or code before claiming novelty for any
  speculative or proof-based fast path.

### Recent U.S. patent: merged duplicate marking during alignment

Source:
[U.S. Patent 12,620,455](https://patents.justia.com/patent/12620455)

Relevant points:

- performs duplicate marking concurrently with alignment and sorting;
- uses linked-list metadata structures keyed by alignment position;
- groups entries by mate-distance value and compares average quality score;
- generates BAM without intermediate SAM or unmarked BAM;
- explicitly claims technical savings in memory and pipeline operations.

Risk to our position:

- broad claims around "avoid intermediate SAM/BAM while duplicate marking" are
  likely dangerous;
- broad claims around local metadata structures keyed by genomic position are
  likely dangerous;
- we should assume a patent examiner will find this reference quickly.

What still appears different from the `turbo-picard` fast path:

- the reviewed patent is about integrated duplicate marking during alignment and
  position-linked metadata grouping;
- the `turbo-picard` candidate is about proving the no-duplicate case on an
  already coordinate-sorted BAM and then bypassing general duplicate marking.

That difference may matter, but it is not yet enough by itself.

## Current novelty assessment

### Weak or crowded themes

These look heavily occupied by prior art:

- duplicate marking on sorted BAM;
- single-pass duplicate marking;
- moving-window or local-memory duplicate marking;
- more efficient pair collation;
- lower-memory alternative to Picard;
- avoiding intermediate files during duplicate marking;
- throughput/scalability claims alone.

### More promising themes

These still look potentially differentiable:

- a proof-oriented no-duplicate detection method rather than a general
  duplicate-marking stream processor;
- monotonic ordered duplicate-key progression as evidence that duplicate
  candidates are globally unique;
- bounded deferred pair resolution plus monotonic key validation;
- automatic downgrade from proof path to full duplicate-grouping path when a
  repeated or out-of-order key appears;
- direct BAM copy after proof success when no output mutation is required.

## What needs deeper review

Before concluding anything, we need:

1. the full FastDup paper and, ideally, source code review;
2. a closer read of samtools `markdup` internals, not just docs;
3. review of Sambamba `markdup` internals;
4. a patent search specifically around:
   "duplicate marking", "BAM", "sorted stream", "proof", "bypass", "no-op",
   "speculation", "fallback", and "genomics pipeline";
5. review of claims and cited art around U.S. Patent 12,620,455.

## Practical consequence

The `turbo-picard` fast path might still be claimable, but only if we narrow it
to the exact mechanism that current prior art does not appear to teach:

- prove uniqueness in the no-duplicate case;
- use bounded state to do it;
- invalidate the proof immediately on repeated or out-of-order keys;
- skip the heavy duplicate-marking machinery and optionally copy the BAM
  directly when the proof succeeds.

That is much narrower than "faster Picard replacement," but it is the only
direction that currently looks worth pressing.
