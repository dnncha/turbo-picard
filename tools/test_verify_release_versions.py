#!/usr/bin/env python3

from __future__ import annotations

import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("verify_release_versions.py")
SPEC = importlib.util.spec_from_file_location("verify_release_versions", MODULE_PATH)
assert SPEC is not None
verify_release_versions = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules["verify_release_versions"] = verify_release_versions
SPEC.loader.exec_module(verify_release_versions)


def write(root: Path, path: str, text: str) -> None:
    target = root / path
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(text, encoding="utf-8")


class VerifyReleaseVersionsTest(unittest.TestCase):
    def make_tree(self) -> Path:
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        root = Path(temp.name)

        write(
            root,
            "Cargo.toml",
            """
[workspace.package]
version = "0.1.0"
""",
        )
        write(
            root,
            "Cargo.lock",
            """
version = 4

[[package]]
name = "turbo-picard-cli"
version = "0.1.0"

[[package]]
name = "turbo-picard-core"
version = "0.1.0"

[[package]]
name = "turbo-picard-markdup"
version = "0.1.0"
""",
        )
        write(
            root,
            "crates/turbo-picard-cli/Cargo.toml",
            """
[package]
name = "turbo-picard-cli"
version.workspace = true

[dependencies]
turbo-picard-core = { path = "../turbo-picard-core", version = "0.1.0" }
""",
        )
        write(
            root,
            "crates/turbo-picard-core/Cargo.toml",
            """
[package]
name = "turbo-picard-core"
version.workspace = true
""",
        )
        for recipe in verify_release_versions.RECIPE_PATHS:
            write(
                root,
                str(recipe),
                """
{% set name = "turbo-picard" %}
{% set version = "0.1.0" %}
""",
            )
        write(
            root,
            "CITATION.cff",
            """
cff-version: 1.2.0
message: "Cite the archived release and keep Picard parity evidence."
type: software
title: "turbo-picard"
version: "0.1.0"
abstract: "Picard-shaped tooling with parity evidence."
authors:
  - name: "turbo-picard contributors"
repository-code: "https://github.com/dnncha/turbo-picard"
url: "https://turbo-picard.readthedocs.io/"
license: "MIT"
keywords:
  - bioinformatics
  - genomics
  - Picard
  - SAM
  - BAM
  - VCF
  - Rust
""",
        )
        for doc in verify_release_versions.VERSIONED_DOC_PATHS:
            write(
                root,
                str(doc),
                """
https://github.com/dnncha/turbo-picard/archive/refs/tags/v0.1.0.tar.gz
<github-v0.1.0-source-archive-sha256>
python3 tools/prepare_bioconda_release.py --archive ~/Downloads/turbo-picard-0.1.0.tar.gz
Prefer --archive for release submission because it validates the downloaded GitHub source archive.
Use --sha256 only when the digest was computed from the downloaded GitHub source archive; that fallback skips archive filename and content validation.
filename turbo-picard-0.1.0.tar.gz v0.1.0.tar.gz
top-level turbo-picard-0.1.0/
Cargo.toml Cargo.lock CITATION.cff docs/command-matrix.yml docs/parity.rst benchmarks/real-data/manifest.json docs/site/assets/benchmark-data.json packaging/bioconda/turbo-picard/meta.yaml packaging/bioconda/turbo-picard-picard-shim/meta.yaml
rejects unsafe paths, duplicate entries, unsupported tar member types, and empty required source files
checks archive-internal metadata: workspace version, CITATION.cff archived-release fields, picard_reference for Picard 3.4.0, datasets, benchmarks, recipe version, source block
updates BIOCONDA_PR.md PR body
CITATION.cff software citation is separate from pinned input data with SHA-256.
CITATION.cff does not cite benchmark inputs.
Use the archived release with command-level parity evidence, exact command surfaces,
unsupported surfaces, and evidence reports.
Picard 3.4.0 full Git commit.
""",
            )
        return root

    def test_accepts_consistent_tree(self) -> None:
        root = self.make_tree()
        self.assertEqual([], verify_release_versions.collect_errors(root))

    def test_rejects_missing_citation_type(self) -> None:
        root = self.make_tree()
        write(
            root,
            "CITATION.cff",
            """
cff-version: 1.2.0
message: "Cite the archived release and keep Picard parity evidence."
title: "turbo-picard"
version: "0.1.0"
abstract: "Picard-shaped tooling with parity evidence."
authors:
  - name: "turbo-picard contributors"
repository-code: "https://github.com/dnncha/turbo-picard"
url: "https://turbo-picard.readthedocs.io/"
license: "MIT"
keywords:
  - bioinformatics
  - genomics
  - Picard
  - SAM
  - BAM
  - VCF
  - Rust
""",
        )

        self.assertIn(
            "CITATION.cff type must be software",
            verify_release_versions.collect_errors(root),
        )

    def test_rejects_stale_internal_dependency_version(self) -> None:
        root = self.make_tree()
        write(
            root,
            "crates/turbo-picard-cli/Cargo.toml",
            """
[package]
name = "turbo-picard-cli"
version.workspace = true

[dependencies]
turbo-picard-core = { path = "../turbo-picard-core", version = "0.2.0" }
""",
        )
        errors = verify_release_versions.collect_errors(root)
        self.assertIn(
            "crates/turbo-picard-cli/Cargo.toml dependency turbo-picard-core "
            "version 0.2.0 must match workspace 0.1.0",
            errors,
        )

    def test_rejects_missing_lockfile(self) -> None:
        root = self.make_tree()
        (root / "Cargo.lock").unlink()
        errors = verify_release_versions.collect_errors(root)
        self.assertIn(
            "Cargo.lock is required for reproducible release builds",
            errors,
        )

    def test_rejects_stale_lockfile_crate_version(self) -> None:
        root = self.make_tree()
        write(
            root,
            "Cargo.lock",
            """
version = 4

[[package]]
name = "turbo-picard-cli"
version = "0.1.0"

[[package]]
name = "turbo-picard-core"
version = "0.2.0"

[[package]]
name = "turbo-picard-markdup"
version = "0.1.0"
""",
        )
        errors = verify_release_versions.collect_errors(root)
        self.assertIn(
            "Cargo.lock turbo-picard-core version 0.2.0 must match workspace 0.1.0",
            errors,
        )

    def test_rejects_missing_lockfile_crate(self) -> None:
        root = self.make_tree()
        write(
            root,
            "Cargo.lock",
            """
version = 4

[[package]]
name = "turbo-picard-cli"
version = "0.1.0"

[[package]]
name = "turbo-picard-core"
version = "0.1.0"
""",
        )
        errors = verify_release_versions.collect_errors(root)
        self.assertIn(
            "Cargo.lock missing turbo-picard-markdup package",
            errors,
        )

    def test_rejects_crate_that_does_not_inherit_workspace_version(self) -> None:
        root = self.make_tree()
        write(
            root,
            "crates/turbo-picard-core/Cargo.toml",
            """
[package]
name = "turbo-picard-core"
version = "0.1.0"
""",
        )
        errors = verify_release_versions.collect_errors(root)
        self.assertIn(
            "crates/turbo-picard-core/Cargo.toml package version must inherit workspace version",
            errors,
        )

    def test_rejects_crate_missing_package_section(self) -> None:
        root = self.make_tree()
        write(
            root,
            "crates/turbo-picard-core/Cargo.toml",
            """
[dependencies]
""",
        )
        errors = verify_release_versions.collect_errors(root)
        self.assertIn(
            "crates/turbo-picard-core/Cargo.toml missing [package] section",
            errors,
        )

    def test_rejects_stale_recipe_version_and_docs(self) -> None:
        root = self.make_tree()
        write(
            root,
            str(verify_release_versions.RECIPE_PATHS[0]),
            """
{% set version = "0.2.0" %}
""",
        )
        write(
            root,
            str(verify_release_versions.VERSIONED_DOC_PATHS[0]),
            """
https://github.com/dnncha/turbo-picard/archive/refs/tags/v0.2.0.tar.gz
<github-v0.2.0-source-archive-sha256>
""",
        )
        errors = verify_release_versions.collect_errors(root)
        self.assertIn(
            "packaging/bioconda/turbo-picard/meta.yaml version 0.2.0 "
            "must match workspace 0.1.0",
            errors,
        )
        self.assertIn("README.md archive URL must use v0.1.0", errors)
        self.assertIn("README.md archive sha256 placeholder must use v0.1.0", errors)

    def test_rejects_missing_citation_file(self) -> None:
        root = self.make_tree()
        (root / "CITATION.cff").unlink()
        errors = verify_release_versions.collect_errors(root)
        self.assertIn(
            "CITATION.cff is required for release citation metadata",
            errors,
        )

    def test_rejects_stale_citation_version_and_repository(self) -> None:
        root = self.make_tree()
        write(
            root,
            "CITATION.cff",
            """
cff-version: 1.2.0
message: "Cite the archived release and keep Picard parity evidence."
title: "turbo-picard"
version: "0.2.0"
abstract: "Picard-shaped tooling with parity evidence."
authors:
  - name: "turbo-picard contributors"
repository-code: "https://example.org/wrong"
license: "Apache-2.0"
keywords:
  - bioinformatics
  - genomics
  - Picard
  - SAM
  - BAM
  - VCF
  - Rust
""",
        )
        errors = verify_release_versions.collect_errors(root)
        self.assertIn(
            "CITATION.cff version 0.2.0 must match workspace 0.1.0",
            errors,
        )
        self.assertIn(
            "CITATION.cff repository-code must be https://github.com/dnncha/turbo-picard",
            errors,
        )
        self.assertIn("CITATION.cff url must be https://turbo-picard.readthedocs.io/", errors)
        self.assertIn("CITATION.cff license must match workspace MIT", errors)

    def test_rejects_incomplete_citation_metadata(self) -> None:
        root = self.make_tree()
        write(
            root,
            "CITATION.cff",
            """
cff-version: 1.1.0
message: "Cite something."
title: "wrong-title"
version: "0.1.0"
abstract: "Picard-shaped tooling with parity evidence."
repository-code: "https://github.com/dnncha/turbo-picard"
url: "https://turbo-picard.readthedocs.io/"
license: "MIT"
keywords:
  - bioinformatics
  - genomics
  - Picard
  - SAM
  - BAM
  - VCF
  - Rust
""",
        )
        errors = verify_release_versions.collect_errors(root)
        self.assertIn("CITATION.cff cff-version must be 1.2.0", errors)
        self.assertIn("CITATION.cff title must be turbo-picard", errors)
        self.assertIn(
            "CITATION.cff must include a named creator",
            errors,
        )
        self.assertIn(
            "CITATION.cff message must ask users to cite the archived release",
            errors,
        )

    def test_rejects_unparseable_citation_yaml(self) -> None:
        root = self.make_tree()
        write(
            root,
            "CITATION.cff",
            """
cff-version: 1.2.0
message: "Cite the archived release and keep Picard parity evidence."
type: software
title: "turbo-picard"
version: "0.1.0"
authors: [
repository-code: "https://github.com/dnncha/turbo-picard"
url: "https://turbo-picard.readthedocs.io/"
license: "MIT"
""",
        )
        errors = verify_release_versions.collect_errors(root)
        self.assertTrue(
            any(error.startswith("CITATION.cff is not valid YAML:") for error in errors),
            errors,
        )
        self.assertIn("CITATION.cff must parse as a YAML mapping", errors)

    def test_rejects_citation_without_structured_author_or_keywords(self) -> None:
        root = self.make_tree()
        write(
            root,
            "CITATION.cff",
            """
cff-version: 1.2.0
message: "Cite the archived release and keep Picard parity evidence."
type: software
title: "turbo-picard"
version: "0.1.0"
abstract: "Picard-shaped tooling with parity evidence."
authors: "turbo-picard contributors"
repository-code: "https://github.com/dnncha/turbo-picard"
url: "https://turbo-picard.readthedocs.io/"
license: "MIT"
keywords:
  - genomics
""",
        )
        errors = verify_release_versions.collect_errors(root)
        self.assertIn("CITATION.cff authors must be a non-empty list", errors)
        self.assertIn(
            "CITATION.cff keywords must cover bioinformatics, Picard, SAM/BAM/VCF, and Rust",
            errors,
        )

    def test_rejects_input_data_details_in_citation_cff(self) -> None:
        root = self.make_tree()
        write(
            root,
            "CITATION.cff",
            """
cff-version: 1.2.0
message: "Cite the archived release and keep Picard parity evidence."
type: software
title: "turbo-picard"
version: "0.1.0"
abstract: "Picard-shaped tooling with parity evidence. Input source URL and SHA-256 are elsewhere."
authors:
  - name: "turbo-picard contributors"
repository-code: "https://github.com/dnncha/turbo-picard"
url: "https://turbo-picard.readthedocs.io/"
license: "MIT"
keywords:
  - bioinformatics
  - genomics
  - Picard
  - SAM
  - BAM
  - VCF
  - Rust
""",
        )
        errors = verify_release_versions.collect_errors(root)
        self.assertIn(
            "CITATION.cff must cite only the software release; "
            "move input-data citation details to evidence manifests/docs: "
            "source URL, SHA-256",
            errors,
        )

    def test_rejects_docs_without_citation_or_input_distinction(self) -> None:
        root = self.make_tree()
        write(
            root,
            "docs/citation.rst",
            """
Software citation only.
""",
        )
        errors = verify_release_versions.collect_errors(root)
        self.assertIn("docs/citation.rst must mention CITATION.cff", errors)
        self.assertIn(
            "docs/citation.rst must distinguish software citation from pinned input data",
            errors,
        )

    def test_rejects_citation_docs_without_methods_details(self) -> None:
        root = self.make_tree()
        write(
            root,
            "docs/citation.rst",
            """
CITATION.cff software citation is separate from pinned input data with SHA-256.
""",
        )
        errors = verify_release_versions.collect_errors(root)
        self.assertIn("docs/citation.rst must mention archived-release citation rule", errors)
        self.assertIn("docs/citation.rst must mention parity evidence rule", errors)
        self.assertIn("docs/citation.rst must mention Picard evidence version", errors)
        self.assertIn("docs/citation.rst must mention methods command replacement rule", errors)
        self.assertIn("docs/citation.rst must mention methods fallback disclosure rule", errors)
        self.assertIn("docs/citation.rst must mention methods evidence-report rule", errors)
        self.assertIn("docs/citation.rst must mention full Git commit citation rule", errors)
        self.assertIn("docs/citation.rst must mention CITATION.cff input-data boundary", errors)

    def test_rejects_stale_site_archive_command(self) -> None:
        root = self.make_tree()
        write(
            root,
            "docs/site/index.html",
            """
python3 tools/prepare_bioconda_release.py --archive ~/Downloads/turbo-picard-0.2.0.tar.gz
filename turbo-picard-0.1.0.tar.gz v0.1.0.tar.gz
""",
        )
        errors = verify_release_versions.collect_errors(root)
        self.assertIn("docs/site/index.html archive command must use 0.1.0", errors)

    def test_rejects_stale_release_helper_filename_note(self) -> None:
        root = self.make_tree()
        write(
            root,
            "docs/packaging.rst",
            """
python3 tools/prepare_bioconda_release.py --archive ~/Downloads/turbo-picard-0.1.0.tar.gz
filename turbo-picard-0.2.0.tar.gz v0.2.0.tar.gz
top-level turbo-picard-0.1.0/
Cargo.toml Cargo.lock CITATION.cff docs/command-matrix.yml docs/parity.rst benchmarks/real-data/manifest.json docs/site/assets/benchmark-data.json packaging/bioconda/turbo-picard/meta.yaml packaging/bioconda/turbo-picard-picard-shim/meta.yaml
rejects unsafe paths, duplicate entries, unsupported tar member types, and empty required source files
""",
        )
        errors = verify_release_versions.collect_errors(root)
        self.assertIn(
            "docs/packaging.rst archive command must use 0.1.0",
            errors,
        )

    def test_rejects_stale_release_helper_archive_layout_note(self) -> None:
        root = self.make_tree()
        write(
            root,
            "packaging/bioconda/BIOCONDA_PR.md",
            """
python3 tools/prepare_bioconda_release.py --archive ~/Downloads/turbo-picard-0.1.0.tar.gz
filename turbo-picard-0.1.0.tar.gz v0.1.0.tar.gz
top-level turbo-picard-0.2.0/
Cargo.toml Cargo.lock CITATION.cff docs/command-matrix.yml docs/parity.rst benchmarks/real-data/manifest.json docs/site/assets/benchmark-data.json packaging/bioconda/turbo-picard/meta.yaml packaging/bioconda/turbo-picard-picard-shim/meta.yaml
rejects unsafe paths, duplicate entries, unsupported tar member types, and empty required source files
""",
        )
        errors = verify_release_versions.collect_errors(root)
        self.assertIn(
            "packaging/bioconda/BIOCONDA_PR.md release-helper archive layout note must mention turbo-picard-0.1.0/",
            errors,
        )

    def test_rejects_release_helper_pr_body_note_without_pr_template_path(self) -> None:
        root = self.make_tree()
        write(
            root,
            "docs/site/index.html",
            """
python3 tools/prepare_bioconda_release.py --archive ~/Downloads/turbo-picard-0.1.0.tar.gz
filename turbo-picard-0.1.0.tar.gz v0.1.0.tar.gz
top-level turbo-picard-0.1.0/
Cargo.toml Cargo.lock CITATION.cff docs/command-matrix.yml docs/parity.rst benchmarks/real-data/manifest.json docs/site/assets/benchmark-data.json packaging/bioconda/turbo-picard/meta.yaml packaging/bioconda/turbo-picard-picard-shim/meta.yaml
rejects unsafe paths, duplicate entries, unsupported tar member types, and empty required source files
updates the PR body
""",
        )
        errors = verify_release_versions.collect_errors(root)
        self.assertIn(
            "docs/site/index.html release-helper PR body note must mention BIOCONDA_PR.md",
            errors,
        )

    def test_rejects_sha256_fallback_without_archive_preference(self) -> None:
        root = self.make_tree()
        write(
            root,
            "docs/packaging.rst",
            """
python3 tools/prepare_bioconda_release.py --archive ~/Downloads/turbo-picard-0.1.0.tar.gz
python3 tools/prepare_bioconda_release.py --sha256 abc
filename turbo-picard-0.1.0.tar.gz v0.1.0.tar.gz
top-level turbo-picard-0.1.0/
Cargo.toml Cargo.lock CITATION.cff docs/command-matrix.yml docs/parity.rst benchmarks/real-data/manifest.json docs/site/assets/benchmark-data.json packaging/bioconda/turbo-picard/meta.yaml packaging/bioconda/turbo-picard-picard-shim/meta.yaml
rejects unsafe paths, duplicate entries, unsupported tar member types, and empty required source files
checks archive-internal metadata: workspace version, CITATION.cff archived-release fields, picard_reference for Picard 3.4.0, datasets, benchmarks, recipe version, source block
updates BIOCONDA_PR.md PR body
""",
        )
        errors = verify_release_versions.collect_errors(root)
        self.assertIn(
            "docs/packaging.rst release-helper sha256 fallback must prefer --archive",
            errors,
        )
        self.assertIn(
            "docs/packaging.rst release-helper sha256 fallback must tie digest to "
            "the downloaded GitHub source archive",
            errors,
        )
        self.assertIn(
            "docs/packaging.rst release-helper sha256 fallback must disclose skipped "
            "archive validation",
            errors,
        )

    def test_accepts_sha256_fallback_wording_wrapped_across_lines(self) -> None:
        root = self.make_tree()
        write(
            root,
            "docs/packaging.rst",
            """
python3 tools/prepare_bioconda_release.py --archive ~/Downloads/turbo-picard-0.1.0.tar.gz
Prefer --archive for release submission because it validates the downloaded
GitHub source archive before writing the digest.
Use --sha256 only when the digest was computed from the downloaded GitHub
source archive; that fallback skips archive filename and content validation.
filename turbo-picard-0.1.0.tar.gz v0.1.0.tar.gz
top-level turbo-picard-0.1.0/
Cargo.toml Cargo.lock CITATION.cff docs/command-matrix.yml docs/parity.rst benchmarks/real-data/manifest.json docs/site/assets/benchmark-data.json packaging/bioconda/turbo-picard/meta.yaml packaging/bioconda/turbo-picard-picard-shim/meta.yaml
rejects unsafe paths, duplicate entries, unsupported tar member types, and empty required source files
checks archive-internal metadata: workspace version, CITATION.cff archived-release
fields, picard_reference for Picard 3.4.0, datasets, benchmarks,
recipe version, source block
updates BIOCONDA_PR.md PR body
""",
        )
        self.assertEqual([], verify_release_versions.collect_errors(root))

    def test_rejects_release_helper_archive_policy_note_missing_strict_checks(self) -> None:
        root = self.make_tree()
        write(
            root,
            "docs/packaging.rst",
            """
python3 tools/prepare_bioconda_release.py --archive ~/Downloads/turbo-picard-0.1.0.tar.gz
filename turbo-picard-0.1.0.tar.gz v0.1.0.tar.gz
top-level turbo-picard-0.1.0/
Cargo.toml Cargo.lock CITATION.cff docs/command-matrix.yml docs/parity.rst benchmarks/real-data/manifest.json docs/site/assets/benchmark-data.json packaging/bioconda/turbo-picard/meta.yaml packaging/bioconda/turbo-picard-picard-shim/meta.yaml
updates BIOCONDA_PR.md PR body
""",
        )
        errors = verify_release_versions.collect_errors(root)
        self.assertIn(
            "docs/packaging.rst release-helper archive policy note must mention "
            "unsafe paths, duplicate, unsupported tar member types, empty required source files",
            errors,
        )

    def test_rejects_release_helper_archive_metadata_note_missing_internal_checks(self) -> None:
        root = self.make_tree()
        write(
            root,
            "docs/packaging.rst",
            """
python3 tools/prepare_bioconda_release.py --archive ~/Downloads/turbo-picard-0.1.0.tar.gz
filename turbo-picard-0.1.0.tar.gz v0.1.0.tar.gz
top-level turbo-picard-0.1.0/
Cargo.toml Cargo.lock CITATION.cff docs/command-matrix.yml docs/parity.rst benchmarks/real-data/manifest.json docs/site/assets/benchmark-data.json packaging/bioconda/turbo-picard/meta.yaml packaging/bioconda/turbo-picard-picard-shim/meta.yaml
rejects unsafe paths, duplicate entries, unsupported tar member types, and empty required source files
updates BIOCONDA_PR.md PR body
""",
        )
        errors = verify_release_versions.collect_errors(root)
        self.assertIn(
            "docs/packaging.rst release-helper archive metadata note must mention "
            "workspace version, archived-release, picard_reference, Picard 3.4.0, "
            "datasets, recipe version, source block",
            errors,
        )

    def test_rejects_release_helper_archive_layout_note_missing_evidence_files(self) -> None:
        root = self.make_tree()
        write(
            root,
            "docs/packaging.rst",
            """
python3 tools/prepare_bioconda_release.py --archive ~/Downloads/turbo-picard-0.1.0.tar.gz
filename turbo-picard-0.1.0.tar.gz v0.1.0.tar.gz
top-level turbo-picard-0.1.0/
Cargo.toml Cargo.lock CITATION.cff packaging/bioconda/turbo-picard/meta.yaml
packaging/bioconda/turbo-picard-picard-shim/meta.yaml
rejects unsafe paths, duplicate entries, unsupported tar member types, and empty required source files
updates BIOCONDA_PR.md PR body
""",
        )
        errors = verify_release_versions.collect_errors(root)
        self.assertIn(
            "docs/packaging.rst release-helper archive layout note must mention "
            "docs/command-matrix.yml, docs/parity.rst, benchmarks/real-data/manifest.json, "
            "docs/site/assets/benchmark-data.json",
            errors,
        )


if __name__ == "__main__":
    unittest.main()
