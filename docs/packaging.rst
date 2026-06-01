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
     --archive ~/Downloads/turbo-picard-0.1.0.tar.gz

The preflight command summarizes the checks that are already green and calls out
the expected wait state while the recipes still use ``source.path``.

The helper computes the archive SHA-256 and writes it into both recipes and
``packaging/bioconda/BIOCONDA_PR.md``. Prefer ``--archive`` for release
submission because it validates the downloaded GitHub source archive before
writing the digest. If the digest was computed elsewhere, pass it explicitly
with ``--sha256`` only when it was computed from the downloaded GitHub source
archive; that fallback skips archive filename and content validation. When using
``--archive``, the filename must match the recipe version before it will be
hashed; for ``0.1.0``, use ``turbo-picard-0.1.0.tar.gz`` or GitHub's
``v0.1.0.tar.gz``. The helper also checks that the archive opens as a
GitHub-style source tarball with a ``turbo-picard-0.1.0/`` top-level directory,
workspace ``Cargo.toml``, ``Cargo.lock``, ``CITATION.cff``, ``docs/command-matrix.yml``,
``docs/parity.rst``,
``benchmarks/real-data/manifest.json``,
``docs/site/assets/benchmark-data.json``,
``packaging/bioconda/turbo-picard/meta.yaml``, and
``packaging/bioconda/turbo-picard-picard-shim/meta.yaml``. It rejects unsafe
paths, duplicate entries, unsupported tar member types, and empty required
source files before it writes recipe metadata. It also checks archive-internal
metadata: the workspace version, ``CITATION.cff`` archived-release citation
fields, the ``picard_reference`` entry for Picard 3.4.0, the ``datasets`` and
``benchmarks`` JSON evidence sections, and matching recipe version and source
block metadata.

Then run the release gates:

.. code-block:: bash

   python3 tools/update_real_data_manifest.py \
     --entry benchmarks/real-data/gatk-na12878-mito/evidence/manifest-entry.json \
     --entry benchmarks/real-data/picard-snvq/evidence/manifest-entry.json \
     --replace
   python3 tools/verify_benchmark_suite_coverage.py
   python3 tools/verify_benchmark_thresholds.py
   python3 tools/verify_ci_coverage.py
   python3 tools/verify_parity_docs.py
   python3 tools/verify_readme_links.py
   python3 tools/verify_site_links.py
   python3 tools/verify_real_data_evidence.py --release-ready
   python3 tools/verify_bioconda_recipes.py --release-ready

Use ``packaging/bioconda/BIOCONDA_PR.md`` as the starting Bioconda PR body. It
records the package split, the intentional shim conflict with upstream Picard,
the tagged source archive, and the pinned real-data parity evidence reviewers
need to audit the claim.

The release-candidate evidence used in that PR must cover this command
portfolio somewhere in pinned ``release_candidate`` data:
AddOrReplaceReadGroups, BuildBamIndex, CleanSam,
CollectAlignmentSummaryMetrics, CollectInsertSizeMetrics,
CollectQualityYieldMetrics, MarkDuplicates, RevertSam, SamToFastq, SortSam,
ValidateSamFile, ViewSam.

The benchmark threshold gate requires full saved benchmark parity, at least
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
