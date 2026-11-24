# nf-core Slack

Use this only in a relevant channel. Keep it short and make the ask concrete.

Message:

Hi all - I am the author of `turbo-picard` (0.1.21), native Rust
implementations of 35 selected Picard commands with Picard `KEY=VALUE` and
GATK4 `--kebab-case` flags:
https://github.com/dnncha/turbo-picard

On a 30x GRCh38 whole genome (1000 Genomes `HG02675`, 740M reads, 8 vCPU):
- `MarkDuplicates`: 59m 01s / 355 MB RSS vs Picard 3.4.0's 3h 30m 07s / 34.4 GB
  (3.6x faster, 97x less RAM, exact metrics & SAM columns 1-11 MD5; trade-off is
  99.2 GB vs 23.3 GB temp disk, tracked in #86).
- `CollectWgsMetrics`: 5m 08s / 352 MB RSS vs Picard's 2h 13m 55s / 4.0 GB
  (26x faster, 11x less RAM, exact field/histogram parity).

(The historical 14 August 2026 32-command small-fixture suite reports 84.52x
geometric mean and 272.12x top saved ratio where JVM startup dominates; treat
the 30x WGS numbers as the realistic large-file reference.)

There is a draft nf-core module and Nextflow trial starter in the repo:

- https://turbo-picard.readthedocs.io/en/latest/evaluation-playbook.html
- https://github.com/dnncha/turbo-picard/tree/main/packaging/nf-core
- https://github.com/dnncha/turbo-picard/tree/main/packaging/workflows

Install via pinned PyPI wheel (`pip install turbo-picard==0.1.21`) or container
(`ghcr.io/dnncha/turbo-picard:0.1.21`); verify current Bioconda channel status
before relying on Conda. Which nf-core module (`picard/markduplicates`,
`picard/collectwgsmetrics`, `picard/collectmultiplemetrics`) would be the most
useful first real-world side-by-side comparison?

Independent evaluation protocol:
https://turbo-picard.readthedocs.io/en/latest/independent-evaluation.html
