# Duel: turbo-picard against every competitor

The duel races turbo-picard, Picard 3.5.0, riker, dupblaster, FastDup and
samtools on the same Colab VM, on 1000 Genomes HG02675 downsampled to 4x and
15x and the full genome. Every run appends to `ledger.jsonl`, and
`tools/duel.py report` rewrites `SCOREBOARD.md` and `losses.json`.

`losses.json` is the work queue. Each entry is a case where turbo-picard is
not the fastest, not the leanest, not Picard-exact, or refused the native
path. Speed losses carry a diagnosis (share of cores busy, hottest threads)
and the hottest functions from a `perf` profile of the turbo-picard run.

## Tasks

| Task | turbo-picard | Rivals |
| --- | --- | --- |
| `wgs` | `CollectWgsMetrics`, default and all-cores | `riker wgs --threads N` |
| `multi` | `CollectMultipleMetrics` with alignment, insert size, base distribution, quality, GC bias and WGS | `riker multi --tools wgs alignment basic isize gcbias` |
| `markdup-coord` | `MarkDuplicates` on coordinate-sorted input | FastDup; samtools collate, fixmate, sort, markdup; samtools collate, dupblaster, sort |
| `markdup-qgroup` | `MarkDuplicates` on queryname input, levels 5 and 0 | dupblaster on the same input, levels 5 and 0 |

Pipelines that need a different input order include the steps that produce
it, so every row ends with the same kind of output. samtools markdup keeps
duplicate flags already in its input, so its pipeline clears flag 1024
first; the others reset it themselves. Picard runs once per case as the
accuracy reference. Accuracy for duplicate marking is the symmetric
difference of duplicate-flagged `(read name, read flags)` keys against
Picard; for `wgs` it is the exact comparator for turbo-picard and a
field-by-field deviation for riker.

## Run it

```sh
export COLAB_STATE=$TMPDIR/turbo-picard-colab-state.json
tools/colab_duel.sh launch turbo-picard-duel-cpu REF=<branch> DATASETS=4x,15x,30x PICARD_DATASETS=4x
tools/colab_duel.sh sync turbo-picard-duel-cpu
tools/colab_duel.sh report turbo-picard-duel-cpu
tools/colab_duel.sh stop turbo-picard-duel-cpu
```

`COLAB_GPU=G4` gives a 48-core AMD EPYC host. After a fix, rerun only
turbo-picard with `ONLY_TOOLS=turbo-picard`; the report carries rival rows
over from the latest earlier run on the same host. Data and tools stay
cached on the VM between runs, so a rerun only rebuilds the branch.

Timings are medians of measured repeats with a warm page cache (the Colab
container cannot drop caches). The 30x rows run two repeats and no warmup.
Treat cross-host comparisons with care: the scoreboard groups rows by host.
