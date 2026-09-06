# Continue HMMForge without redoing discovery

This directory is the complete independent Python package. Current code is
0.1.0a2, backed by pinned PyHMMER 0.12.3. Read README and ENGINEERING, then run:

```sh
python3 -m venv .venv
. .venv/bin/activate
python -m pip install -e '.[test]'
python -m pytest -q
hmmforge capabilities
```

The connected-session limitation was creation of a NEW GitHub repository, not
write access. The source is parked at `dnncha/turbo-picard`, branch
`research/hmmforge-prototype`, directory `research/hmmforge`. Do not merge it into
Turbo Picard main. The downloadable source archive is already standalone.

With authenticated local GitHub CLI, initialise a separate checkout and create
`dnncha/hmmforge` as PRIVATE for now. Verify the name is available; never overwrite
an existing repo. Commit the standalone directory, including its standalone CI
workflow, then use `gh repo create dnncha/hmmforge --private --source=. --push`.
Do not publish a PyPI package or announce a production accelerator yet.

Important evidence distinction: the first committed prototype passed 40 tests
and the synthetic benchmark in GitHub Actions run 33999639022. That initial run
was ~1.65x faster in elapsed time than the scan path, but only ~6% lower CPU time.
Version 0.1.0a2 adds prepared-profile reuse and compact extraction. The latest
completed GitHub run 34000192881 passed all 42 tests and independent native HMMER
3.4 comparisons on both fixtures. Same-run median wall speedups were 1.50x
synthetic and 1.47x small-biological. CPU-time reductions were 13.5% and 36.6%.
Read RESULTS.md; never present initial timings as latest-version timings.

Create small datasets without a network fetch:

```sh
python scripts/make_fixture.py /tmp/hmmforge-synthetic
python scripts/backend_fixture.py /tmp/hmmforge-biological
hmmforge benchmark /tmp/hmmforge-biological/models.hmm \
  /tmp/hmmforge-biological/proteins.fa --cpus 2 --repeats 5 \
  --dataset-kind biological > biological-benchmark.json
```

The biological fixture script documents removal of terminal translation-stop
markers before either engine sees the input. The CLI itself rejects stops; do
not silently change that contract. These are 14 profiles and 2,100 proteins, not
full Pfam and not a metagenomic production corpus.

Install a native HMMER executable separately and run:

```sh
python scripts/native_check.py /tmp/hmmforge-biological/models.hmm \
  /tmp/hmmforge-biological/proteins.fa > native-parity.json
```

Next implementation work: add full-catalogue regression inputs under appropriate
licenses, benchmark an expert model-major baseline, and profile native kernels.
Then implement the dominant measured bottleneck. Preserve per-protein domZ,
upstream duplicate suppression, deterministic seeds, exact input identities and
failure-on-mismatch. Do not add a GPU stub or claim a rewrite is complete.

Keep the response to the project owner concrete: commit, commands, tests run,
raw benchmark evidence, remaining gate. No repeated market essay is needed.
