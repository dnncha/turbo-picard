# riker comparison benchmarks

This directory holds three-way QC benchmark evidence for **Picard**,
**turbo-picard**, and **riker** on the same BAM inputs.

## Quick smoke run (repo fixture)

Uses the pinned GATK NA12878 mitochondrial BAM and `fixtures/reference/chrM.fa`.
No large downloads required.

```bash
python3 tools/bench_qc_vs_riker.py --smoke --skip-build --allow-missing-riker
```

In smoke mode the helper now defaults to `5` repeats and records the median,
because one-shot timings on the mitochondrial fixture are too noisy to trust.

Outputs land in `benchmarks/riker-comparison/evidence/`:

- `bench_qc_vs_riker.tsv`
- `bench_qc_vs_riker.md`
- `bench_qc_vs_riker.json`

Install riker to generate canonical three-way evidence:

```bash
cargo install riker-ngs
# or: conda install -c bioconda riker
```

Without ``--allow-missing-riker``, the helper now fails if ``riker`` is not
installed so the checked-in evidence cannot silently drop the third tool.

## WGS-scale staging notes

For large riker-compatible evidence, run on a controlled Linux host with local
NVMe and keep host-specific SSH wrappers outside this public repository. Stage
the public 1000 Genomes CRAM and matching reference with your local
infrastructure, then run the benchmark helper against the staged BAM.

## WGS-scale run (riker-compatible fixtures)

For apples-to-apples comparisons against
[riker's benchmark pipeline](https://github.com/fulcrumgenomics/riker/tree/main/benchmark-pipeline),
stage one of the public 1000 Genomes CRAMs that riker locks in
`config/samples.wgs.tsv`, transcode to BAM, and point the helper at the staged
file plus the matching reference FASTA.

Example after staging `HG02675_4x` (~7 GB BAM):

```bash
python3 tools/bench_qc_vs_riker.py \
  --sample-id HG02675_4x \
  --input-bam /mnt/scratch/HG02675_4x/input.bam \
  --reference-fasta /mnt/scratch/refs/hg38.fa \
  --output-dir benchmarks/riker-comparison/evidence/HG02675_4x \
  --repeats 3 \
  --skip-build
```

Profiles benchmarked:

| Profile | turbo-picard / Picard | riker |
| --- | --- | --- |
| `wgs-only` | `CollectWgsMetrics` | `riker wgs` |
| `wgs-bundle` | `CollectMultipleMetrics` + `CollectGcBiasMetrics` + `CollectWgsMetrics` | `riker multi --tools wgs alignment basic isize gcbias` |

Hybcap (`CollectHsMetrics` vs `riker hybcap`) can run through the native
core-metrics and sidecar path for the documented ALL_READS scope, with
upstream Picard fallback retained for unsupported advanced options. No
WES-scale performance claim is implied by this smoke surface.

## Environment overrides

```bash
export TURBO_PICARD_BENCH_PICARD_COMMAND='mamba run -p /opt/conda/envs/picard picard'
export TURBO_PICARD_BENCH_TURBO_COMMAND='./target/release/picard'
export TURBO_PICARD_BENCH_RIKER_COMMAND='riker'
```

## Measurement notes

- Run on local NVMe when benchmarking large BAMs. EBS variance contaminates wall
  times.
- Keep one timed job at a time on the host.
- The smoke fixture validates the harness, not WGS throughput claims. Publish
  WGS numbers only from staged 1000 Genomes or workflow-representative shards.
