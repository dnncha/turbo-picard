# turbo-picard

![turbo-picard branded hero](docs/site/assets/turbo-picard-branded-readme.png)

`turbo-picard` is a Picard-compatible command-line toolkit for bioinformatics
teams that want faster runs without rewriting established pipelines.

It keeps the command shape people already know:

```bash
picard MarkDuplicates I=input.bam O=marked.bam M=metrics.txt
```

Native Rust implementations handle covered commands first. Unsupported commands,
or supported commands used outside their current native scope, can fail clearly
or delegate to upstream Picard through a configured fallback. That makes
`turbo-picard` practical to evaluate command by command in WDL, Nextflow,
Snakemake, shell pipelines, and institutional workflow stacks.

## Documentation

The full user documentation is on Read the Docs:

**https://turbo-picard.readthedocs.io/en/latest/**

Start with:

- [Quickstart](https://turbo-picard.readthedocs.io/en/latest/quickstart.html)
  for installation, entrypoints, and first commands.
- [Command coverage](https://turbo-picard.readthedocs.io/en/latest/commands.html)
  for native, partial, and fallback-supported Picard surfaces.
- [Adoption guide](https://turbo-picard.readthedocs.io/en/latest/adoption.html)
  for safe pipeline rollout, parity checks, and CI gates.
- [Fallback behavior](https://turbo-picard.readthedocs.io/en/latest/fallback.html)
  for mixed deployments that still need upstream Picard.
- [Benchmarks](https://turbo-picard.readthedocs.io/en/latest/benchmarks.html)
  for reproducible performance checks tied to parity.
- [Packaging](https://turbo-picard.readthedocs.io/en/latest/packaging.html)
  for the main binary, the optional `picard` shim, and conda-style deployment.
- [Troubleshooting](https://turbo-picard.readthedocs.io/en/latest/troubleshooting.html)
  for common setup and output-comparison issues.

The generated docs source lives in [`docs/`](docs/) for contributors who prefer
to read or build it locally.

## Why Use It

Picard is deeply embedded in computational biology. That is a strength: labs and
platform teams have years of assumptions encoded in workflow definitions,
containers, and QC procedures. `turbo-picard` is designed to preserve that
contract while accelerating high-value commands where native implementation
already makes sense.

Use it when you want to:

- reduce wall-clock time for common Picard-heavy pipeline stages;
- keep familiar `KEY=VALUE` Picard command lines;
- evaluate replacement behavior one command at a time;
- keep unsupported or uncommon surfaces routed to upstream Picard;
- make coverage, parity, and benchmark evidence explicit.

This is not a claim that every Picard surface has been reimplemented. Treat
`turbo-picard` as a measured, reversible acceleration layer.

## Install From Source

From a repository checkout:

```bash
cargo install --locked --path crates/turbo-picard-cli --bin turbo-picard --bin picard
```

This installs:

- `turbo-picard`, the explicit non-shadowing entrypoint.
- `picard`, a compatibility shim for workflow managers and scripts that already
  call Picard by command name.

Use `turbo-picard` first when evaluating. Put the `picard` shim on `PATH` only
when you deliberately want it to shadow upstream Picard for a workflow or
environment.

## First Command

```bash
turbo-picard MarkDuplicates \
  I=input.bam \
  O=marked.bam \
  M=metrics.txt \
  ASSUME_SORTED=true \
  VALIDATION_STRINGENCY=SILENT
```

The shim accepts the same Picard-style syntax:

```bash
picard MarkDuplicates I=input.bam O=marked.bam M=metrics.txt
```

For command-specific behavior, use local help and the documentation:

```bash
turbo-picard --help
turbo-picard MarkDuplicates --help
turbo-picard SortSam --help
```

## Fallback To Upstream Picard

By default, unsupported commands fail clearly. For mixed deployments, configure
an upstream Picard command:

```bash
export TURBO_PICARD_FALLBACK_COMMAND='java -jar /opt/picard/picard.jar'
```

Native commands still run natively. Unsupported commands and explicitly
unsupported native surfaces delegate to upstream Picard. Prefer an absolute JAR
or command path so the fallback cannot resolve back to the `picard` shim.

See the [fallback documentation](https://turbo-picard.readthedocs.io/en/latest/fallback.html)
for the exact delegation rules.

## Adoption In Pipelines

For production genomics workflows, start narrow:

1. Run `turbo-picard` beside upstream Picard on representative BAM, FASTQ, VCF,
   interval-list, and metrics-producing steps.
2. Compare outputs, sidecars, exit codes, and runtime for the command surfaces
   your workflow actually uses.
3. Add the relevant parity scripts and benchmark checks to CI.
4. Switch only proven surfaces to the `picard` shim, with upstream Picard
   configured as fallback where needed.

The [adoption guide](https://turbo-picard.readthedocs.io/en/latest/adoption.html)
covers this process in more detail.

## Benchmarks

Benchmark claims are only useful when they stay tied to parity. The repository
includes a local suite for command-level evidence:

```bash
python3 tools/bench_suite.py --repeats 1 --skip-build
```

The current project site and benchmark assets live under [`docs/site/`](docs/site/).
For the reproducible workflow, see the
[benchmark documentation](https://turbo-picard.readthedocs.io/en/latest/benchmarks.html).

## Contributing

The most useful contributions are command-surface improvements backed by tests,
parity checks, and clear documentation updates. Before broadening a native
implementation, check the machine-readable command matrix in
[`docs/command-matrix.yml`](docs/command-matrix.yml) and the contributor notes in
the [development documentation](https://turbo-picard.readthedocs.io/en/latest/development.html).
