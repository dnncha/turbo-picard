# Continue HMMForge

Current implementation: **0.1.0a3**, backed by PyHMMER 0.12.3. Code commit:
`825607c3976035cf1ef41fd1dae66b1d0586c998`. The underlying annotation kernels
and engine are unchanged from a2; this phase improves the evidence and baselines.

Source is at `dnncha/turbo-picard`, branch `research/hmmforge-prototype`, directory
`research/hmmforge`. **Do not merge this branch into Turbo Picard main.**
The standalone repository target remains `dnncha/hmmforge`, private until release
gates pass. The connected environment cannot create new repositories or run an
authenticated GitHub CLI; it can and did commit the code to the existing branch.

## One-time standalone extraction

From the independent package directory, on a machine with authenticated `gh`:

```sh
bash scripts/publish_standalone.sh ../hmmforge-standalone
```

The script obtains the current research source directly from GitHub, extracts
only HMMForge, initializes a standalone history, and creates/pushes a private
`dnncha/hmmforge`. It refuses an existing repository or destination. It does not
publish to PyPI, touch Turbo Picard main, or announce production readiness.

## Work already completed

GitHub run `34026780616`, package job `101469049073`, passed 65 tests, synthetic
and small-biological three-engine studies, native-HMMER checks, and a built-wheel
installation/verification check. The package/evidence artifact is `9987312491`.
The full-catalogue job in that run is a separate gate; check its actual completion
and artifact before calling it successful. Read the latest evidence report.

The direct, fully resident model-major baseline is implemented in
`src/hmmforge/baseline.py`. It does not reuse HMMForge's extraction functions or
batch executor, but shares input validation, prepared models and HMMER kernels.
It is authored here, not independently reviewed by an external expert.

The larger local synthetic study used 256 models and 10,000 proteins. All nine
outputs were byte-identical. HMMForge was 2.06x faster than scan, but only 1.04x
faster than the direct baseline. This does not justify a breakthrough claim.

## Resume without rediscovery

```sh
python3 -m venv .venv
. .venv/bin/activate
python -m pip install -e '.[test]'
python -m pytest -q
hmmforge-study run models.hmm proteins.fa --cpus 8 --repeats 6 \
  --dataset-kind biological --output-dir study
```

Read `docs/STUDY.md` for full-catalogue acquisition, native profiling and the
remaining gate: a version-locked full model catalogue against at least 100,000
representative novel proteins, multiple CPU/batch budgets, and independent
biological/correctness review. Preserve all losses and mismatches. Never relax
filters, use a permissive absolute E-value tolerance, or pretend phase timers
identify native kernel costs.

When native profiles identify a dominant bottleneck, implement that change and
rerun both the direct baseline and scan. Do not expand or market a wrapper as a
new scoring kernel. A GPU branch should be justified by measured kernel cost and
include transfer/residency costs; no GPU stubs or speculative speedup numbers.
