---
name: verify-mate-cache
description: Model-check the MarkDuplicates bounded mate cache and spill-filter pairing invariant with TLC. Use when a change touches external_mates.rs, mate eviction, the spill filter, pair encoding, or the bounded duplicate plan, and before merging such a change.
---

# Verify the mate-cache pairing invariant

Every completed pair must be the canonical first-with-second, third-with-fourth pairing of that
template key's occurrences in input order. A different pairing marks the wrong read ends as
duplicates. The model and its bounds are documented in
[`formal/mate-cache/README.md`](../../../formal/mate-cache/README.md).

Run it from the repository root:

```bash
formal/mate-cache/run-tlc.sh
```

A pass ends with `Model checking completed. No error has been found.` and about 131,543 generated
states. It takes about one second and writes nothing into the repository.

## If it fails

TLC prints the shortest sequence of `Input` steps that breaks the invariant, then the final
`completed` and `spillSeq`. Read the trace in code terms and use `/statechart` to draw it.

Decide which of these happened.

1. The code changed and the model did not. Update the model to match the code, rerun, and move the
   sync commit in this file.
2. The model matches the code. That is a real pairing bug. Report it with the trace and the
   canonical pair. Do not change the invariant or the model to make it pass.

## When this check applies

It covers `ExternalMateCache::push`, `flush`, the spill filter (`filter_bits`, `was_spilled`,
`mark_spilled`), `push_pair`, and `finish_spilled` in
`crates/turbo-picard-markdup/src/external_mates.rs`. Run it when any of those change, when eviction
or cache budgets change, when pair encoding changes, or before merging a MarkDuplicates planning
change.

It does not cover the fragment stage, the pair sort, flag complementarity, or downstream duplicate
decisions.

Check whether the covered file moved since the sync commit:

```bash
formal/mate-cache/check-sync.sh
```

It exits 0 when `external_mates.rs` is unchanged since the sync commit, and lists the commits that
touched it otherwise.

## Coverage record

- Verified files: `crates/turbo-picard-markdup/src/external_mates.rs`
- Verified functions: `ExternalMateCache::push`, `ExternalMateCache::flush`, `filter_bits`,
  `was_spilled`, `mark_spilled`, `push_pair`, `finish_spilled`
- Sync commit: `8b5dae0`
- Bounds: 2 template keys, at most 5 records, record sizes `{1, 2, 4}`, cache of 2 records and 3
  bytes. TLC checks every input sequence, eviction point, and filter false positive in that space.

Move the sync commit forward only after you confirm the model still describes the code at the new
commit. When the verification itself falls behind without being rechecked, use
`/maintain-formal-verification-skill`.

## Confirm the harness can fail

```bash
formal/mate-cache/run-mutant.sh
```

It must report `Invariant PairsCanonical is violated`. If it says the mutant passed, the check has
lost its teeth and needs fixing before you trust a pass.
