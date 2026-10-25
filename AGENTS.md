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
python3 tools/render_competitor_card.py --check
```

The CI workflow discovers `tools/test_*.py` tests and compiles the entire tools
directory. New verifiers must still be explicitly executed in CI; discovery
does not replace scientific parity, release, or packaging gates.
Run the full CI workflow before merging. Native changes require Rust tests,
Picard parity, package/container checks and workflow-starter checks; a Python
benchmark is not a substitute. Build Sphinx documentation using the repository's
pinned docs requirements when documentation changes.

## Keep the competitor card current

`docs/site/assets/competitor-card{,-mobile}.{svg,png}` (landscape and portrait
phone layouts) are the first graphic visitors see
(README, PyPI, site home, link previews). They are rendered only from evidence:
the newest `benchmarks/markduplicates-competitors/landscape-YYYY-MM-DD.json`,
`docs/site/assets/benchmark-data.json` and `benchmarks/progress.json`. Never
edit the SVG or PNG by hand, and never type a number into the renderer.

- When an accepted hill-climb change lands on main, record its before/after
  medians from the same run with `python3 tools/render_competitor_card.py
  record --series <id> --version <release, #PR or short sha> --before <s>
  --after <s> --evidence <immutable URL or repo path>`. This re-renders the
  card. Steps chain as ratios, so runs on different hosts never share an
  absolute axis. Add a new series with `--label`, `--protocol` and
  `--baseline`.
- After a new competitor run, copy its report unchanged to a new dated
  `landscape-YYYY-MM-DD.json` (or `.json.gz`) and re-render. The headline
  changes itself: "fastest we've measured" only when Turbo Picard beats every
  tool in that run, "fastest Picard-identical" when it beats only the tools
  that match Picard's output, and no speed claim otherwise.
- Commit all four regenerated SVG and PNG files with the evidence that changed
  them.

## Describe only what was measured

Record input and source hashes, options, resource limits, versions, repetitions,
absolute wall time and peak memory. Separate parsing microbenchmarks, validation
helper benchmarks, whole-command measurements and pipeline measurements.
Do not relabel synthetic fixtures as production or external adoption. Preserve
losses and limitations alongside wins. Never publish a version, claim a merge,
or describe a source-only feature as released without verifying the actual state.
