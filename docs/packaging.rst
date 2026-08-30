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

The latest provider-verified PyPI release is ``0.1.12``. It publishes Linux
x86_64 and ARM64 wheels, macOS Intel and Apple Silicon wheels, and a source
distribution. The matching GitHub release, PyPI package, and GHCR image have
passed their provider checks. For Linux clusters and shared environments,
Bioconda is the cleaner target once the recipe is accepted. The release scope
and evidence boundaries are summarized in ``CHANGELOG.md``.

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
Only publish after the wheel check, command smoke tests, parity checks, release
metadata verifiers, and the built-artifact check pass on the exact commit being
released. The artifact check compares every wheel and source distribution with
the checked-out version and README, including the container version shown in
the public install path, and checks that each wheel's embedded executable
matches its platform tag.

The publishing workflow is ``.github/workflows/publish-pypi.yml``. On PyPI,
configure a trusted publisher for project ``turbo-picard`` with owner
``dnncha``, repository ``turbo-picard``, workflow
``publish-pypi.yml``, and environment ``pypi``.

After validating the distributions, create the privacy-conscious release
handoff manifest. It records artifact filenames, byte sizes, SHA-256 digests,
source/tag state, and optional local benchmark summaries without recording
local filesystem paths:

.. code-block:: bash

   python3 tools/build_release_manifest.py \
     --dist dist \
     --benchmark-profile /path/to/bench-profile.json \
     --output turbo-picard-release-manifest.json

The publish workflow runs
``python3 tools/verify_release_artifacts.py --dist dist`` after downloading all
architecture artifacts and before the PyPI upload. This keeps package metadata
and install instructions aligned when a release tag is cut from a different
commit than the latest documentation update.
It also installs the Linux x86_64 wheel in a clean virtual environment and runs
``--version``, ``doctor``, the read-only ``trial`` contract, and the ``picard``
compatibility shim before publishing. The final step runs the self-contained
``tools/verify_install_smoke.sh`` fixture, which marks a duplicate SAM record
and checks the output and metrics rather than stopping at ``--help``.
It also runs ``tools/verify_mate_barcode_install_smoke.sh`` against the checked
in mate-specific barcode fixture and fails if the Picard-compatible size-2
histogram drifts.
The macOS arm64 and Intel wheel build jobs run the same checks natively through
``tools/verify_pypi_wheel_install_smoke.sh`` before uploading their artifacts.
The Linux arm64 job remains cross-build and artifact-validated rather than
pretending that an x86 runner executed an arm64 binary.
The publication jobs also run ``tools/verify_markduplicates_guardrails.py`` so
checked-in MarkDuplicates benchmark provenance, parity status, resource ratios,
and README disclosures cannot drift independently.
After upload, the workflow retries a live PyPI JSON check and compares the public
long description with the checked-out ``README.md``. This catches a release that
uploaded valid wheels but left installation guidance or the container tag stale
on the package index.

Container image
---------------

The published ``0.1.12`` image is available at
``ghcr.io/dnncha/turbo-picard:0.1.12``. It resolves to
``sha256:0edcfd38a2ada2b3f83279ff27c0f3e3214312669f16bf305de87de89c681baf``.
It contains both ``turbo-picard`` and the ``picard`` shim, with
``turbo-picard`` as the container entrypoint. Use the release tag or digest for
side-by-side nf-core profiles or cloud jobs where you want a pinned binary
without a Conda solve.

.. code-block:: bash

   docker run --rm ghcr.io/dnncha/turbo-picard:0.1.12 --version

The container publication workflow supports manual dispatch for operational
convenience, but it fails before GHCR login unless the selected ref is the
exact ``v<workspace-version>`` tag. Branch dispatch cannot publish an image.
After pushing, it pulls the exact version tag and runs ``--version``, ``doctor``,
the read-only ``trial`` contract, and a real MarkDuplicates fixture against the
published image. The local guard is checked by
``python3 tools/verify_publish_workflows.py``.

The repository root ``Dockerfile`` builds the same runtime shape locally:

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

Before release tagging, the recipe files may use the local checkout while
artifacts are still being prepared:

.. code-block:: yaml

   source:
     path: ../../..

Do not open a Bioconda PR while the recipes still use ``source.path`` or a
source archive SHA-256 placeholder. Those states are only for smoke testing and
release preparation in this repository. The submission is ready to copy into
``bioconda-recipes`` only after the tagged archive URL and SHA-256 are written
and the release-ready verifier passes.

Commit the intended release state before tagging. The preflight command reports
a dirty worktree as a release wait state so the source archive is not cut from
the wrong commit. It also compares the current ``HEAD`` with the local and
remote release-tag commit, and waits if the workspace still carries the old
version after that tag has moved on.

After cutting a GitHub release for the exact commit being packaged, download the
GitHub source archive and switch both recipes plus the draft Bioconda PR body to
the immutable tagged archive:

.. code-block:: bash

   python3 tools/bioconda_release_preflight.py
   python3 tools/prepare_bioconda_release.py \
     --archive ~/Downloads/turbo-picard-0.1.12.tar.gz

The preflight command summarizes the checks that are already green and calls out
the expected wait state while the recipes still use ``source.path`` or a source
archive SHA-256 placeholder.

The helper computes the archive SHA-256 and writes it into both recipes and
``packaging/bioconda/BIOCONDA_PR.md``. Prefer ``--archive`` for release
submission because it validates the downloaded GitHub source archive before
writing the digest. The helper also checks that the archive matches the recipe
version, contains the expected release files, and carries the citation,
benchmark, and real-data metadata used by the PR body. If the digest was
computed elsewhere, pass it with ``--sha256`` only when it came from the
downloaded GitHub source archive. That fallback skips archive filename and
content validation. For ``0.1.12``, use ``turbo-picard-0.1.12.tar.gz`` or
GitHub's ``v0.1.12.tar.gz``.

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
