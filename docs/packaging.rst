Packaging
=========

``turbo-picard`` is packaged with a conservative default: the main command does
not shadow upstream Picard.

Main package
------------

The main package installs:

``turbo-picard``
   Use this for evaluation, explicit workflow calls, and environments where
   upstream Picard must remain the default ``picard`` command.

Compatibility shim package
--------------------------

The optional shim package installs:

``picard``
   A compatibility entrypoint for workflow managers and scripts that already
   invoke Picard by command name.

Use the shim deliberately. It shadows upstream Picard wherever it appears first
on ``PATH``.

PyPI
----

The published PyPI package is available at
https://pypi.org/project/turbo-picard/.

It is built with ``maturin`` and packages the Rust command-line binary targets
as Python wheel scripts:

.. code-block:: bash

   python3 -m pip install turbo-picard

Release ``0.1.1`` publishes a macOS Apple Silicon wheel and a source
distribution. If ``pip`` builds from source, the machine needs a Rust toolchain
and native build dependencies. For Linux clusters and shared environments,
Bioconda is the cleaner target once the recipe is accepted.

The current wheel exposes both commands from the CLI crate:

``turbo-picard``
   The explicit command. Prefer this while testing or when upstream Picard must
   remain available as ``picard``.

``picard``
   The compatibility shim. It is useful in a dedicated virtual environment for
   workflow code that already invokes ``picard`` by name.

Use a dedicated virtual environment if you install from PyPI and also need
upstream Picard on ``PATH``. The wheel-level shim is convenient, but it can
shadow a different ``picard`` command in that environment.

To build and inspect the package locally:

.. code-block:: bash

   python3 -m pip install --upgrade "maturin>=1.8,<2" twine
   python3 -m maturin build --release --compatibility pypi --out dist
   python3 -m twine check dist/*

For publication, prefer PyPI Trusted Publishing from the GitHub release
workflow rather than storing a long-lived PyPI token in repository secrets.
Only publish after the wheel check, command smoke tests, parity checks, and
release metadata verifiers pass on the exact commit being released.

The publishing workflow is ``.github/workflows/publish-pypi.yml``. On PyPI,
configure a trusted publisher for project ``turbo-picard`` with owner
``dnncha``, repository ``turbo-picard``, workflow
``publish-pypi.yml``, and environment ``pypi``.

Container image
---------------

The repository root ``Dockerfile`` builds a minimal runtime image with both
``turbo-picard`` and the ``picard`` shim. Use it for side-by-side nf-core
profiles or cloud jobs where you want a pinned binary without conda solve time.

.. code-block:: bash

   docker build -t turbo-picard:local .
   docker run --rm turbo-picard:local MarkDuplicates -h

Conda-style deployment
----------------------

The repository includes Bioconda-oriented packaging files under
``packaging/bioconda``:

* ``turbo-picard`` for the explicit command;
* ``turbo-picard-picard-shim`` for the compatibility shim.

In shared environments, prefer installing the main package first, proving the
commands you need, and adding the shim only to pipeline-specific environments.

Bioconda release path
---------------------

The recipe files intentionally use the local checkout while release artifacts
are still being prepared:

.. code-block:: yaml

   source:
     path: ../../..

Do not open a Bioconda PR while the recipes still use ``source.path``. That
local source block is only for smoke testing this repository. The submission is
ready to copy into ``bioconda-recipes`` only after the tagged archive URL and
SHA-256 are written and the release-ready verifier passes.

Commit the intended release state before tagging. The preflight command reports
a dirty worktree as a release wait state so the source archive is not cut from
the wrong commit.

After cutting a GitHub release for the exact commit being packaged, download the
GitHub source archive and switch both recipes plus the draft Bioconda PR body to
the immutable tagged archive:

.. code-block:: bash

   python3 tools/bioconda_release_preflight.py
   python3 tools/prepare_bioconda_release.py \
     --archive ~/Downloads/turbo-picard-0.1.1.tar.gz

The preflight command summarizes the checks that are already green and calls out
the expected wait state while the recipes still use ``source.path``.

The helper computes the archive SHA-256 and writes it into both recipes and
``packaging/bioconda/BIOCONDA_PR.md``. Prefer ``--archive`` for release
submission because it validates the downloaded GitHub source archive before
writing the digest. The helper also checks that the archive matches the recipe
version, contains the expected release files, and carries the citation,
benchmark, and real-data metadata used by the PR body. If the digest was
computed elsewhere, pass it with ``--sha256`` only when it came from the
downloaded GitHub source archive. That fallback skips archive filename and
content validation. For ``0.1.1``, use ``turbo-picard-0.1.1.tar.gz`` or
GitHub's ``v0.1.1.tar.gz``.

Then run the release checks:

.. code-block:: bash

   python3 tools/update_real_data_manifest.py \
     --entry benchmarks/real-data/gatk-na12878-mito/evidence/manifest-entry.json \
     --entry benchmarks/real-data/picard-snvq/evidence/manifest-entry.json \
     --replace
   python3 tools/verify_benchmark_suite_coverage.py
   python3 tools/verify_benchmark_thresholds.py
   python3 tools/verify_ci_coverage.py
   python3 tools/verify_parity_docs.py
   python3 tools/verify_pypi_package.py
   python3 tools/verify_readme_links.py
   python3 tools/verify_site_links.py
   python3 tools/verify_workflow_starters.py
   python3 tools/verify_real_data_evidence.py --release-ready
   python3 tools/verify_bioconda_recipes.py --release-ready

Use ``packaging/bioconda/BIOCONDA_PR.md`` as the starting Bioconda PR body. It
records the package split, the intentional shim conflict with upstream Picard,
the tagged source archive, and the pinned real-data parity evidence reviewers
need to audit the claim.

The release evidence used in that PR must cover this command set somewhere in
pinned release data:
AddOrReplaceReadGroups, BuildBamIndex, CleanSam,
CollectAlignmentSummaryMetrics, CollectInsertSizeMetrics,
CollectQualityYieldMetrics, MarkDuplicates, RevertSam, SamToFastq, SortSam,
ValidateSamFile, ViewSam.

The benchmark threshold check requires full saved benchmark parity, at least
``5.00x`` floor speedup, at least ``20.00x`` geometric mean speedup, and at
least ``50.00x`` top speedup before benchmark numbers are used as release
evidence.

The PR body should keep citation responsibilities separate. ``CITATION.cff``
cites the archived turbo-picard release. Benchmark and validation inputs should
be cited separately with immutable source URLs, commits or accessions, and
SHA-256 hashes.

After copying both recipes into a ``bioconda-recipes`` checkout, run Bioconda's
lint step before the Docker/mulled builds:

.. code-block:: bash

   cp -R packaging/bioconda/turbo-picard recipes/turbo-picard
   cp -R packaging/bioconda/turbo-picard-picard-shim recipes/turbo-picard-picard-shim
   bioconda-utils lint recipes config.yml --packages turbo-picard turbo-picard-picard-shim
   bioconda-utils build --docker --mulled-test turbo-picard
   bioconda-utils build --docker --mulled-test turbo-picard-picard-shim
