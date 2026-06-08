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

## Atlas / Tailscale run (riker smoke fixture)

For a Linux box on your tailnet (for example `atlas`), sync the repo and run the
riker-compatible **HG02675_4x** staging flow: stream the public 1000 Genomes CRAM,
subsample to ~4× coverage, then benchmark Picard vs turbo-picard vs riker.

```bash
./tools/run_atlas_riker_benchmark.sh
```

The remote runner lives at
`benchmarks/riker-comparison/atlas/setup_and_run.sh`. It:

1. installs `samtools`, `awscli`, `openjdk`, and a `picard=3.4.0` micromamba env;
2. builds `turbo-picard` and installs `riker-ngs`;
3. stages `GRCh38_full_analysis_set_plus_decoy_hla.fa` and a subsampled
   `HG02675_4x` BAM without keeping the full 30× CRAM on disk;
4. writes evidence to `benchmarks/riker-comparison/evidence/HG02675_4x-atlas/`.

Override the SSH target if needed:

```bash
export TURBO_PICARD_ATLAS_HOST=root@100.69.16.54
export TURBO_PICARD_ATLAS_IDENTITY=~/.ssh/tankful_codex
./tools/run_atlas_riker_benchmark.sh
```

Re-run benchmarks without re-staging:

```bash
ssh root@100.69.16.54 'TURBO_PICARD_BENCH_SKIP_STAGE=1 bash /root/turbo-picard/benchmarks/riker-comparison/atlas/setup_and_run.sh'
```

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

Hybcap (`CollectHsMetrics` vs `riker hybcap`) can run today through upstream
Picard delegation; a native `CollectHsMetrics` fast path is still planned.

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
