# Rust community forum

Category:

Announcements

Title:

`turbo-picard` 0.1.21: native Rust implementations of 35 Broad Picard genomics commands

Post:

I have released `turbo-picard` 0.1.21:

https://github.com/dnncha/turbo-picard

It implements 35 Broad Picard genomics commands in Rust while keeping Picard's
`KEY=VALUE` and GATK4 `--kebab-case` CLI arguments, with transparent fallback to
upstream Picard for the remaining 86 commands.

```bash
turbo-picard MarkDuplicates I=input.bam O=marked.bam M=metrics.txt
```

Recent 30x GRCh38 whole-genome measurements (1000 Genomes `HG02675`, 740M
records, 8 vCPU / 50 GB RAM, single run, 23 November 2026):

- `MarkDuplicates`: on Host A, pre-#87 build `9a15db96` ran in 59m 01s at
  355 MB RSS vs Picard 3.4.0's 3h 30m 07s at 34.4 GB RSS (3.6x faster, 97x less
  memory; 99.2 GB vs 23.3 GB peak temp disk tracked in #86; optical count
  differed by 0.18% pre-#87). On Host B, the #87 build `93216217` (43m 07s /
  354 MB RSS, temp disk not sampled) matched Picard metrics, duplicate count
  (79,289,617), and SAM columns 1-11 MD5 exactly; #88 matched `PG` tags on
  chr21 (11.4M records, whole genome not rerun).
- `CollectWgsMetrics`: 5m 08s at 352 MB RSS vs Picard's 2h 13m 55s at 4.0 GB
  RSS (26x faster, 11x less memory), matching every metric field and histogram
  row.

The historical 14 August 2026 32-command small-fixture suite reports 84.52x
geometric mean and 272.12x maximum saved ratio against Picard 3.4.0 (JVM
startup dominates tiny inputs, so those ratios are not whole-genome
predictions).

Architecture notes:
- Workspace split into `turbo-picard-core` (argument parsing, bounded external
  sorting, HTS I/O), `turbo-picard-markdup` (bounded two-pass duplicate engine
  and binary spill wire codec), and `turbo-picard-cli` (declarative command
  catalog, parallel contig/region collectors, and a 6-tool stdio MCP server
  including isolated native-vs-Picard `verify`).
- `#![forbid(unsafe_code)]`, `clippy::pedantic = "deny"`, `unwrap_used = "deny"`,
  and HTS file I/O through `rust-htslib` with `libdeflate`.

Install:

```bash
python3 -m pip install turbo-picard==0.1.21
```

or from source:

```bash
cargo install --locked --path crates/turbo-picard-cli --bin turbo-picard --bin picard
```

Docs:

- https://turbo-picard.readthedocs.io/en/latest/
- https://turbo-picard.readthedocs.io/en/latest/evidence/
- https://turbo-picard.readthedocs.io/en/latest/parity.html

Feedback on the `rust-htslib` boundary, bounded external-sort spill compression
(#86), and Rust/Python wheel packaging is very welcome.
