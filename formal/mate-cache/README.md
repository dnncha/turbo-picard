# Mate-cache pairing model

## What it verifies

Every completed pair is the canonical first-with-second, third-with-fourth pairing of that template
key's occurrences in input order, under every cache eviction and spill-filter false-positive
schedule. A different pairing marks the wrong read ends as duplicates and silently changes
MarkDuplicates output.

The model is `MateCache.tla`, checked by TLC. It mirrors `ExternalMateCache` in
`crates/turbo-picard-markdup/src/external_mates.rs` at commit `8b5dae0`.

## Model to code

| model action or branch | code |
| --- | --- |
| `Input`, filter branch | `push`, line 67, `was_spilled` branch |
| `Input`, cached occurrence | `push`, line 67, `pending.remove` then `push_pair` |
| `Input`, oversized record | `push`, line 67, `bytes > max_buffer_bytes` |
| `Input`, capacity flush | `push`, line 67, `flush` then recheck |
| `Input`, insert | `push`, line 67, final insert |
| `FalsePositive` | a bloom false positive in `was_spilled`, line 49 |
| `Finish`, flush | `flush`, line 104 |
| `Finish`, exact join | `finish_spilled`, line 155 |
| `push_pair` order guard | line 136, out of scope because the model assumes valid pair flags |

## Bounds

`MateCache.cfg` checks every input sequence of at most 5 records over 2 template keys, every record
size in `{1, 2, 4}` with a 3-byte cache budget and a 2-record cache, every flush point, and every
filter false-positive schedule. TLC explores 131,543 states in about one second.

## Assumptions

The filter may report a key as spilled without it being spilled. It may never report false for a
spilled key. `mark_spilled` sets the same three deterministic bits that `was_spilled` checks, and
the bit-array length never changes after the first insert, so the real filter has no false
negatives. The model therefore lets `may` over-approximate `spilled` freely.

## Out of scope

The first/second flag complementarity that `push_pair` rejects, the fragment stage and its taint
filter, the pair sort, and downstream duplicate decisions. A pass covers this model and these
bounds, not the whole duplicate-marking pipeline.

## Run

```bash
formal/mate-cache/run-tlc.sh
```

A pass ends with `Model checking completed. No error has been found.` and 131,543 generated states.
TLC writes no files into the repository.

## Read a counterexample

TLC prints one `State` per step. Each `Input` state shows the appended `input` record, and the last
state shows `completed` and `spillSeq`. Read the sequence in code terms, for example "cache k1, then
k2, flush on k3, re-cache k1, pair k1's second and third occurrences". The canonical pair is in
`CanonicalPairs` for that key. Use `/statechart` to render the failing path as a chart.

## Maintenance

When the covered code changes, update the model first and rerun. If the model describes the new
code and TLC still fails, that is a real finding. Report it with the trace. Do not weaken
`PairsCanonical` or `FinishedComplete` to make the check pass. After confirming the model still
describes the code, move the sync commit in
`.opencode/skills/verify-mate-cache/SKILL.md` forward.

The harness has a self-test that must keep failing on purpose:

```bash
formal/mate-cache/run-mutant.sh
```

`mutants/MateCacheBroken.tla` deletes the `was_spilled` guard. TLC must report
`Invariant PairsCanonical is violated` for the sequence k1, k2, k3, k1, k1, where the model pairs
occurrences 4 and 5 while the canonical pair is 1 and 4.

## Tool

TLA+ 1.7.4 with TLC 2.19, on any JDK 17 or newer. `run-tlc.sh` uses `TLA2TOOLS_JAR` when set, then
`~/.local/share/tlaplus/1.7.4/tla2tools.jar`, then `~/.cache/supertime-formal/tla2tools-1.7.4.jar`.
