# Working on Turbo Picard

Turbo Picard implements selected Picard-style genomics commands in Rust.
Its value is native execution with a familiar workflow contract, not an
unqualified promise to replace the entire upstream suite.

## Find the right boundary

- `crates/turbo-picard-core`: argument parsing, I/O policy and external sorting.
- `crates/turbo-picard-markdup`: duplicate grouping, selection, optical handling and metrics.
- `crates/turbo-picard-cli`: commands, capability/trial JSON, collectors and dispatch.
- `tools/`: evaluation, regression tests and evidence verification.
- `docs/command-matrix.yml`: documented command coverage; update from its existing generator.
- `benchmarks/real-data/`: pinned inputs, scope and saved evidence, not universal validation.

Before changing a command, read its parser, implementation, integration tests,
comparison contract and documented unsupported options. Do not infer argument
roles from short option names: the same name can mean different things in
different Picard commands.

## Preserve scientific and execution semantics

Do not weaken parity checks to make optimisations pass. Keep duplicate flags,
representative selection, read groups, libraries, barcodes, optical decisions,
metrics and downstream-consumed sidecars inside the tested contract. A changed
comparison contract must be named, justified and independently tested.

Distinguish native execution from Java delegation. Evaluation runs must not
silently inherit an explicit fallback. Keep original inputs immutable and use
separate output paths. Preserve failed runs and never delete a user's existing
output directory to rerun a benchmark.

For agent consumers, discover capabilities before constructing a trial.
Inspection is not execution or input validation. Prefer argv arrays and
`subprocess.run(argv, shell=False)` over generated shell strings. Check the
installed version before using source-only discovery features.

## Complete the checks

Run `cargo fmt --all -- --check` and `cargo test --workspace --locked`.
Install the existing Python test dependency and run:

```sh
python3 -m pip install PyYAML==6.0.3
python3 -m unittest discover -s tools
python3 tools/verify_ci_coverage.py
python3 tools/verify_benchmark_log_evidence.py
python3 tools/verify_benchmark_suite_coverage.py
python3 tools/verify_readme_links.py
python3 tools/verify_site_links.py
```

The CI workflow discovers `tools/test_*.py` tests and compiles the entire tools
directory. New verifiers must still be explicitly executed in CI; discovery
does not replace scientific parity, release, or packaging gates.
Run the full CI workflow before merging. Native changes require Rust tests,
Picard parity, package/container checks and workflow-starter checks; a Python
benchmark is not a substitute. Build Sphinx documentation using the repository's
pinned docs requirements when documentation changes.

## Describe only what was measured

Record input and source hashes, options, resource limits, versions, repetitions,
absolute wall time and peak memory. Separate parsing microbenchmarks, validation
helper benchmarks, whole-command measurements and pipeline measurements.
Do not relabel synthetic fixtures as production or external adoption. Preserve
losses and limitations alongside wins. Never publish a version, claim a merge,
or describe a source-only feature as released without verifying the actual state.
