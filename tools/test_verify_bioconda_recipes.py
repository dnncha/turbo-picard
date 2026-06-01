#!/usr/bin/env python3
"""Tests for Bioconda recipe readiness checks."""

from __future__ import annotations

import importlib.util
import io
import sys
import tempfile
import unittest
from contextlib import redirect_stderr
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("verify_bioconda_recipes.py")
SPEC = importlib.util.spec_from_file_location("verify_bioconda_recipes", MODULE_PATH)
assert SPEC is not None
verify_bioconda_recipes = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules["verify_bioconda_recipes"] = verify_bioconda_recipes
SPEC.loader.exec_module(verify_bioconda_recipes)


def recipe_meta(*, source: str = "", shim: bool = False) -> str:
    package_name = "turbo-picard-picard-shim" if shim else "turbo-picard"
    run_dependency = (
        "    - turbo-picard =={{ version }}\n" if shim else ""
    )
    run_constrained = "  run_constrained:\n    - picard ==0\n" if shim else ""
    summary = (
        "Optional picard command shim for turbo-picard."
        if shim
        else "Picard-compatible Rust toolkit with selected native command surfaces."
    )
    description = (
        "Installs a picard compatibility entrypoint backed by turbo-picard. "
        "This package intentionally conflicts with upstream picard because it "
        "shadows the same command name."
        if shim
        else "Installs the non-shadowing turbo-picard command entrypoint only."
    )
    meta = f"""
{{% set version = "0.1.0" %}}
package:
  name: {package_name}
  version: {{{{ version }}}}
{source}
build:
  number: 0
  skip: true  # [win]
requirements:
  build:
    - {{{{ compiler('c') }}}}
    - {{{{ compiler('rust') }}}}
    - cargo-bundle-licenses
  run:
{run_dependency}    - libzlib
{run_constrained}about:
  home: https://github.com/dnncha/turbo-picard
  doc_url: https://turbo-picard.readthedocs.io/
  dev_url: https://github.com/dnncha/turbo-picard
  license: MIT
  license_file:
    - LICENSE
    - THIRDPARTY.yml
  summary: {summary}
  description: |
    {description}
    See CITATION.cff for software citation and separate real-data evidence citations.
extra:
  recipe-maintainers:
    - dnncha
"""
    test_commands = (
        "test:\n"
        "  commands:\n"
        "    - picard --version\n"
        "    - picard MarkDuplicates --help\n"
        if shim
        else "test:\n"
        "  commands:\n"
        "    - turbo-picard --version\n"
    )
    return meta.replace("about:\n", f"{test_commands}\nabout:\n", 1)


class BiocondaRecipeTests(unittest.TestCase):
    def test_replacement_overclaim_scanner_allows_cautionary_language(self) -> None:
        self.assertEqual(
            verify_bioconda_recipes.replacement_overclaims(
                "Do not describe them as complete cohort-scale validation."
            ),
            [],
        )

    def test_replacement_overclaim_scanner_rejects_positive_claims(self) -> None:
        self.assertEqual(
            verify_bioconda_recipes.replacement_overclaims(
                "This is a drop-in replacement for production genomics workflows."
            ),
            ["drop-in replacement", "production genomics workflows"],
        )

    def test_main_recipe_accepts_required_rust_packaging_bits(self) -> None:
        build_sh = """
export OPENSSL_NO_VENDOR=1
export CARGO_NET_GIT_FETCH_WITH_CLI=true
cargo-bundle-licenses --format yaml --output THIRDPARTY.yml
cargo install --locked --no-track --root "${PREFIX}" --path crates/turbo-picard-cli --bin turbo-picard
"""
        run_test_sh = """
turbo-picard --version
turbo-picard MarkDuplicates --help
turbo-picard SortSam --help
turbo-picard CleanSam --help
turbo-picard ViewSam --help
"""

        errors = verify_bioconda_recipes.validate_recipe(
            name="turbo-picard",
            meta_yaml=recipe_meta(),
            build_sh=build_sh,
            run_test_sh=run_test_sh,
            expected_bin="turbo-picard",
            is_shim=False,
        )

        self.assertEqual(errors, [])

    def test_recipe_package_identity_and_summary_match_expected_package(self) -> None:
        main_build_sh = (
            "export OPENSSL_NO_VENDOR=1\nexport CARGO_NET_GIT_FETCH_WITH_CLI=true\ncargo-bundle-licenses --format yaml --output THIRDPARTY.yml\n"
            'cargo install --locked --no-track --root "${PREFIX}" '
            "--path crates/turbo-picard-cli --bin turbo-picard"
        )
        main_run_test_sh = (
            "turbo-picard --version\n"
            "turbo-picard MarkDuplicates --help\n"
            "turbo-picard SortSam --help\n"
            "turbo-picard CleanSam --help\n"
            "turbo-picard ViewSam --help\n"
        )
        shim_build_sh = (
            "export OPENSSL_NO_VENDOR=1\nexport CARGO_NET_GIT_FETCH_WITH_CLI=true\ncargo-bundle-licenses --format yaml --output THIRDPARTY.yml\n"
            'cargo install --locked --no-track --root "${PREFIX}" '
            "--path crates/turbo-picard-cli --bin picard"
        )

        errors = verify_bioconda_recipes.validate_recipe(
            name="turbo-picard-picard-shim",
            meta_yaml=recipe_meta(shim=True).replace(
                "name: turbo-picard-picard-shim",
                "name: turbo-picard",
            ),
            build_sh=shim_build_sh,
            run_test_sh="picard --version\npicard MarkDuplicates --help",
            expected_bin="picard",
            is_shim=True,
        )

        self.assertIn(
            "turbo-picard-picard-shim meta.yaml package.name is turbo-picard, expected turbo-picard-picard-shim",
            errors,
        )

        main_errors = verify_bioconda_recipes.validate_recipe(
            name="turbo-picard",
            meta_yaml=recipe_meta().replace(
                "summary: Picard-compatible Rust toolkit with selected native command surfaces.",
                "summary: Fast toolkit.",
            ),
            build_sh=main_build_sh,
            run_test_sh=main_run_test_sh,
            expected_bin="turbo-picard",
            is_shim=False,
        )

        self.assertIn(
            "turbo-picard meta.yaml summary must describe the Picard-compatible toolkit",
            main_errors,
        )

        shim_errors = verify_bioconda_recipes.validate_recipe(
            name="turbo-picard-picard-shim",
            meta_yaml=recipe_meta(shim=True).replace(
                "summary: Optional picard command shim for turbo-picard.",
                "summary: Fast toolkit.",
            ),
            build_sh=shim_build_sh,
            run_test_sh="picard --version\npicard MarkDuplicates --help",
            expected_bin="picard",
            is_shim=True,
        )

        self.assertIn(
            "turbo-picard-picard-shim meta.yaml summary must describe the opt-in picard shim",
            shim_errors,
        )

    def test_recipe_metadata_rejects_replacement_overclaims(self) -> None:
        errors = verify_bioconda_recipes.validate_recipe(
            name="turbo-picard",
            meta_yaml=recipe_meta()
            + "This package is a drop-in replacement for production genomics workflows.\n",
            build_sh=(
                "export OPENSSL_NO_VENDOR=1\n"
                "export CARGO_NET_GIT_FETCH_WITH_CLI=true\n"
                "cargo-bundle-licenses --format yaml --output THIRDPARTY.yml\n"
            ),
            run_test_sh="turbo-picard --version\n",
            expected_bin="turbo-picard",
            is_shim=False,
        )

        self.assertIn(
            "turbo-picard meta.yaml contains unsupported replacement overclaim: drop-in replacement",
            errors,
        )
        self.assertIn(
            "turbo-picard meta.yaml contains unsupported replacement overclaim: production genomics workflows",
            errors,
        )

    def test_missing_rust_packaging_bits_are_reported(self) -> None:
        errors = verify_bioconda_recipes.validate_recipe(
            name="turbo-picard",
            meta_yaml="""
requirements:
  build:
    - {{ compiler('c') }}
about:
  license_file:
    - LICENSE
extra:
  recipe-maintainers:
    - replace-with-bioconda-maintainer
""",
            build_sh="cargo install --path crates/turbo-picard-cli --bin turbo-picard",
            run_test_sh="",
            expected_bin="turbo-picard",
            is_shim=False,
        )

        self.assertIn("turbo-picard meta.yaml missing {{ compiler('rust') }}", errors)
        self.assertIn("turbo-picard meta.yaml missing cargo-bundle-licenses", errors)
        self.assertIn("turbo-picard meta.yaml missing THIRDPARTY.yml license_file", errors)
        self.assertIn(
            "turbo-picard build.sh must write bundled Rust dependency licenses to THIRDPARTY.yml",
            errors,
        )
        self.assertIn("turbo-picard meta.yaml missing MIT license metadata", errors)
        self.assertIn("turbo-picard meta.yaml missing documentation URL", errors)
        self.assertIn("turbo-picard meta.yaml missing source development URL", errors)
        self.assertIn("turbo-picard meta.yaml description missing CITATION.cff", errors)
        self.assertIn(
            "turbo-picard meta.yaml description missing evidence citation boundary",
            errors,
        )
        self.assertIn("turbo-picard meta.yaml contains maintainer placeholder", errors)
        self.assertIn("turbo-picard build.sh missing cargo-bundle-licenses invocation", errors)
        self.assertIn("turbo-picard build.sh missing OPENSSL_NO_VENDOR=1", errors)
        self.assertIn(
            "turbo-picard build.sh missing CARGO_NET_GIT_FETCH_WITH_CLI=true",
            errors,
        )
        self.assertIn("turbo-picard build.sh missing cargo install --locked", errors)
        self.assertIn("turbo-picard build.sh missing cargo install --no-track", errors)
        self.assertIn("turbo-picard build.sh missing cargo install --root ${PREFIX}", errors)
        self.assertIn("turbo-picard run_test.sh missing turbo-picard smoke test", errors)

    def test_recipe_rejects_noarch_for_compiled_rust_binary(self) -> None:
        errors = verify_bioconda_recipes.validate_recipe(
            name="turbo-picard",
            meta_yaml=recipe_meta().replace(
                "skip: true  # [win]",
                "skip: true  # [win]\n  noarch: generic",
            ),
            build_sh=(
                "export OPENSSL_NO_VENDOR=1\n"
                "export CARGO_NET_GIT_FETCH_WITH_CLI=true\n"
                "cargo-bundle-licenses --format yaml --output THIRDPARTY.yml\n"
                'cargo install --locked --no-track --root "${PREFIX}" '
                "--path crates/turbo-picard-cli --bin turbo-picard"
            ),
            run_test_sh=(
                "turbo-picard --version\n"
                "turbo-picard MarkDuplicates --help\n"
                "turbo-picard SortSam --help\n"
                "turbo-picard CleanSam --help\n"
                "turbo-picard ViewSam --help\n"
            ),
            expected_bin="turbo-picard",
            is_shim=False,
        )

        self.assertIn(
            "turbo-picard meta.yaml must not use noarch for compiled Rust binaries",
            errors,
        )

    def test_shim_recipe_requires_main_package_pin_and_picard_conflict(self) -> None:
        errors = verify_bioconda_recipes.validate_recipe(
            name="turbo-picard-picard-shim",
            meta_yaml=recipe_meta(shim=True),
            build_sh="export OPENSSL_NO_VENDOR=1\nexport CARGO_NET_GIT_FETCH_WITH_CLI=true\ncargo-bundle-licenses --format yaml --output THIRDPARTY.yml\ncargo install --locked --no-track --root \"${PREFIX}\" --path crates/turbo-picard-cli --bin picard",
            run_test_sh="picard --version\npicard MarkDuplicates --help",
            expected_bin="picard",
            is_shim=True,
        )

        self.assertEqual(errors, [])

    def test_shim_recipe_reports_missing_pin_and_conflict(self) -> None:
        errors = verify_bioconda_recipes.validate_recipe(
            name="turbo-picard-picard-shim",
            meta_yaml="""
requirements:
  build:
    - {{ compiler('rust') }}
    - cargo-bundle-licenses
about:
  license_file:
    - LICENSE
    - THIRDPARTY.yml
extra:
  recipe-maintainers:
    - dnncha
""",
            build_sh="export OPENSSL_NO_VENDOR=1\nexport CARGO_NET_GIT_FETCH_WITH_CLI=true\ncargo-bundle-licenses --format yaml --output THIRDPARTY.yml\ncargo install --locked --no-track --root \"${PREFIX}\" --path crates/turbo-picard-cli --bin picard",
            run_test_sh="picard --version",
            expected_bin="picard",
            is_shim=True,
        )

        self.assertIn(
            "turbo-picard-picard-shim meta.yaml missing exact turbo-picard version run dependency",
            errors,
        )
        self.assertIn(
            "turbo-picard-picard-shim meta.yaml missing picard ==0 run_constrained conflict",
            errors,
        )
        self.assertIn(
            "turbo-picard-picard-shim meta.yaml test commands missing picard --version",
            errors,
        )
        self.assertIn(
            "turbo-picard-picard-shim meta.yaml test commands missing picard MarkDuplicates --help",
            errors,
        )
        self.assertIn(
            "turbo-picard-picard-shim meta.yaml description must disclose upstream picard conflict",
            errors,
        )
        self.assertIn(
            "turbo-picard-picard-shim meta.yaml description must disclose picard command shadowing",
            errors,
        )

    def test_main_recipe_rejects_shim_only_metadata(self) -> None:
        meta_yaml = (
            recipe_meta()
            + "\n    - turbo-picard =={{ version }}\n"
            + "run_constrained:\n"
            + "  - picard ==0\n"
        )

        errors = verify_bioconda_recipes.validate_recipe(
            name="turbo-picard",
            meta_yaml=meta_yaml.replace("non-shadowing turbo-picard", "shadowing picard"),
            build_sh=(
                "export OPENSSL_NO_VENDOR=1\nexport CARGO_NET_GIT_FETCH_WITH_CLI=true\ncargo-bundle-licenses --format yaml --output THIRDPARTY.yml\n"
                'cargo install --locked --no-track --root "${PREFIX}" '
                "--path crates/turbo-picard-cli --bin turbo-picard"
            ),
            run_test_sh="turbo-picard --version\nturbo-picard MarkDuplicates --help\nturbo-picard SortSam --help\nturbo-picard CleanSam --help\nturbo-picard ViewSam --help",
            expected_bin="turbo-picard",
            is_shim=False,
        )

        self.assertIn(
            "turbo-picard meta.yaml must not depend on itself as a shim dependency",
            errors,
        )
        self.assertIn(
            "turbo-picard meta.yaml must not declare shim-only picard conflict",
            errors,
        )
        self.assertIn(
            "turbo-picard meta.yaml description must disclose non-shadowing entrypoint",
            errors,
        )

    def test_recipe_meta_tests_must_match_package_entrypoint(self) -> None:
        errors = verify_bioconda_recipes.validate_recipe(
            name="turbo-picard",
            meta_yaml=recipe_meta().replace("turbo-picard --version", "picard --version"),
            build_sh=(
                "export OPENSSL_NO_VENDOR=1\nexport CARGO_NET_GIT_FETCH_WITH_CLI=true\ncargo-bundle-licenses --format yaml --output THIRDPARTY.yml\n"
                'cargo install --locked --no-track --root "${PREFIX}" '
                "--path crates/turbo-picard-cli --bin turbo-picard"
            ),
            run_test_sh="turbo-picard --version\nturbo-picard MarkDuplicates --help\nturbo-picard SortSam --help\nturbo-picard CleanSam --help\nturbo-picard ViewSam --help",
            expected_bin="turbo-picard",
            is_shim=False,
        )

        self.assertIn(
            "turbo-picard meta.yaml test commands missing turbo-picard --version",
            errors,
        )
        self.assertIn(
            "turbo-picard meta.yaml main package tests must not use picard shim",
            errors,
        )

        shim_errors = verify_bioconda_recipes.validate_recipe(
            name="turbo-picard-picard-shim",
            meta_yaml=recipe_meta(shim=True).replace("picard --version", "turbo-picard --version"),
            build_sh=(
                "export OPENSSL_NO_VENDOR=1\nexport CARGO_NET_GIT_FETCH_WITH_CLI=true\ncargo-bundle-licenses --format yaml --output THIRDPARTY.yml\n"
                'cargo install --locked --no-track --root "${PREFIX}" '
                "--path crates/turbo-picard-cli --bin picard"
            ),
            run_test_sh="picard --version\npicard MarkDuplicates --help",
            expected_bin="picard",
            is_shim=True,
        )

        self.assertIn(
            "turbo-picard-picard-shim meta.yaml test commands missing picard --version",
            shim_errors,
        )
        self.assertIn(
            "turbo-picard-picard-shim meta.yaml shim tests must not use turbo-picard",
            shim_errors,
        )

    def test_main_run_test_command_surface_tracks_command_matrix(self) -> None:
        matrix_text = """
commands:
  - name: MarkDuplicates
    status: partial-native
  - name: SortSam
    status: partial-native
  - name: UnsupportedFutureCommand
    status: fallback-only
"""

        self.assertEqual(
            verify_bioconda_recipes.validate_main_run_test_command_surface(
                run_test_sh="turbo-picard MarkDuplicates --help\nturbo-picard SortSam --help\n",
                matrix_text=matrix_text,
            ),
            [],
        )
        self.assertEqual(
            verify_bioconda_recipes.validate_main_run_test_command_surface(
                run_test_sh="turbo-picard MarkDuplicates --help\n",
                matrix_text=matrix_text,
            ),
            ["turbo-picard run_test.sh missing command smoke: SortSam"],
        )

    def test_main_meta_test_command_surface_tracks_command_matrix(self) -> None:
        matrix_text = """
commands:
  - name: MarkDuplicates
    status: partial-native
  - name: SortSam
    status: partial-native
  - name: UnsupportedFutureCommand
    status: fallback-only
"""
        meta_yaml = """
test:
  commands:
    - turbo-picard MarkDuplicates --help
    - turbo-picard SortSam --help
"""

        self.assertEqual(
            verify_bioconda_recipes.validate_main_meta_test_command_surface(
                meta_yaml=meta_yaml,
                matrix_text=matrix_text,
            ),
            [],
        )
        self.assertEqual(
            verify_bioconda_recipes.validate_main_meta_test_command_surface(
                meta_yaml=meta_yaml.replace("    - turbo-picard SortSam --help\n", ""),
                matrix_text=matrix_text,
            ),
            ["turbo-picard meta.yaml missing command smoke: SortSam"],
        )

    def test_release_ready_mode_rejects_local_source_path(self) -> None:
        recipe = recipe_meta(source="""
source:
  path: ../../..
""")
        errors = verify_bioconda_recipes.validate_recipe(
            name="turbo-picard",
            meta_yaml=recipe,
            build_sh="export OPENSSL_NO_VENDOR=1\nexport CARGO_NET_GIT_FETCH_WITH_CLI=true\ncargo-bundle-licenses --format yaml --output THIRDPARTY.yml\ncargo install --locked --no-track --root \"${PREFIX}\" --path crates/turbo-picard-cli --bin turbo-picard",
            run_test_sh="turbo-picard --version",
            expected_bin="turbo-picard",
            is_shim=False,
            release_ready=True,
        )

        self.assertIn("turbo-picard meta.yaml still uses local source.path", errors)
        self.assertIn("turbo-picard meta.yaml missing release source url", errors)
        self.assertIn("turbo-picard meta.yaml missing release source sha256", errors)

    def test_release_ready_mode_accepts_url_and_sha256(self) -> None:
        recipe = recipe_meta(source="""
source:
  url: https://github.com/dnncha/turbo-picard/archive/refs/tags/v0.1.0.tar.gz
  sha256: 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef
""")
        errors = verify_bioconda_recipes.validate_recipe(
            name="turbo-picard",
            meta_yaml=recipe,
            build_sh="export OPENSSL_NO_VENDOR=1\nexport CARGO_NET_GIT_FETCH_WITH_CLI=true\ncargo-bundle-licenses --format yaml --output THIRDPARTY.yml\ncargo install --locked --no-track --root \"${PREFIX}\" --path crates/turbo-picard-cli --bin turbo-picard",
            run_test_sh="turbo-picard --version\nturbo-picard MarkDuplicates --help\nturbo-picard SortSam --help\nturbo-picard CleanSam --help\nturbo-picard ViewSam --help",
            expected_bin="turbo-picard",
            is_shim=False,
            release_ready=True,
        )

        self.assertEqual(errors, [])

    def test_release_ready_mode_rejects_url_not_matching_recipe_version(self) -> None:
        recipe = recipe_meta(source="""
source:
  url: https://github.com/dnncha/turbo-picard/archive/refs/tags/v0.2.0.tar.gz
  sha256: 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef
""")
        errors = verify_bioconda_recipes.validate_recipe(
            name="turbo-picard",
            meta_yaml=recipe,
            build_sh="export OPENSSL_NO_VENDOR=1\nexport CARGO_NET_GIT_FETCH_WITH_CLI=true\ncargo-bundle-licenses --format yaml --output THIRDPARTY.yml\ncargo install --locked --no-track --root \"${PREFIX}\" --path crates/turbo-picard-cli --bin turbo-picard",
            run_test_sh="turbo-picard --version\nturbo-picard MarkDuplicates --help\nturbo-picard SortSam --help\nturbo-picard CleanSam --help\nturbo-picard ViewSam --help",
            expected_bin="turbo-picard",
            is_shim=False,
            release_ready=True,
        )

        self.assertIn(
            "turbo-picard meta.yaml release source url must be https://github.com/dnncha/turbo-picard/archive/refs/tags/v0.1.0.tar.gz",
            errors,
        )

    def test_recipe_set_versions_must_match(self) -> None:
        self.assertEqual(
            verify_bioconda_recipes.validate_recipe_set_consistency(
                [("turbo-picard", "0.1.0"), ("turbo-picard-picard-shim", "0.1.0")]
            ),
            [],
        )

        self.assertEqual(
            verify_bioconda_recipes.validate_recipe_set_consistency(
                [("turbo-picard", "0.1.0"), ("turbo-picard-picard-shim", "0.2.0")]
            ),
            [
                "Bioconda recipe versions differ: turbo-picard=0.1.0, turbo-picard-picard-shim=0.2.0"
            ],
        )

        self.assertEqual(
            verify_bioconda_recipes.validate_recipe_set_consistency(
                [("turbo-picard", None), ("turbo-picard-picard-shim", "0.1.0")]
            ),
            ["turbo-picard meta.yaml missing version"],
        )

    def test_release_ready_mode_rejects_source_url_from_wrong_repository(self) -> None:
        recipe = recipe_meta(source="""
source:
  url: https://github.com/example/turbo-picard/archive/refs/tags/v0.1.0.tar.gz
  sha256: 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef
""")
        errors = verify_bioconda_recipes.validate_recipe(
            name="turbo-picard",
            meta_yaml=recipe,
            build_sh="export OPENSSL_NO_VENDOR=1\nexport CARGO_NET_GIT_FETCH_WITH_CLI=true\ncargo-bundle-licenses --format yaml --output THIRDPARTY.yml\ncargo install --locked --no-track --root \"${PREFIX}\" --path crates/turbo-picard-cli --bin turbo-picard",
            run_test_sh="turbo-picard --version\nturbo-picard MarkDuplicates --help\nturbo-picard SortSam --help\nturbo-picard CleanSam --help\nturbo-picard ViewSam --help",
            expected_bin="turbo-picard",
            is_shim=False,
            release_ready=True,
        )

        self.assertIn(
            "turbo-picard meta.yaml release source url must be https://github.com/dnncha/turbo-picard/archive/refs/tags/v0.1.0.tar.gz",
            errors,
        )

    def test_source_extractors_read_recipe_and_pr_fields(self) -> None:
        meta_yaml = """
source:
  url: https://github.com/dnncha/turbo-picard/archive/refs/tags/v0.1.0.tar.gz
  sha256: 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef
"""
        pr_text = """
- Tagged archive URL:
  `https://github.com/dnncha/turbo-picard/archive/refs/tags/v0.1.0.tar.gz`
- Archive SHA-256:
  `0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef`
"""

        self.assertEqual(
            verify_bioconda_recipes.recipe_source_url(meta_yaml),
            "https://github.com/dnncha/turbo-picard/archive/refs/tags/v0.1.0.tar.gz",
        )
        self.assertEqual(
            verify_bioconda_recipes.recipe_source_sha256(meta_yaml),
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        )
        self.assertEqual(
            verify_bioconda_recipes.pr_source_url(pr_text),
            "https://github.com/dnncha/turbo-picard/archive/refs/tags/v0.1.0.tar.gz",
        )
        self.assertEqual(
            verify_bioconda_recipes.pr_source_sha256(pr_text),
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        )

    def test_release_evidence_requires_manifest_verifier_and_docs(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "benchmarks" / "real-data").mkdir(parents=True)
            (root / "tools").mkdir()
            (root / "docs" / "site").mkdir(parents=True)
            (root / "docs" / "site" / "assets").mkdir(parents=True)
            (root / "packaging" / "bioconda").mkdir(parents=True)
            (root / "packaging" / "bioconda" / "turbo-picard").mkdir(parents=True)
            (root / "packaging" / "bioconda" / "turbo-picard-picard-shim").mkdir(
                parents=True
            )
            bioconda_submission_commands = (
                "cp -R packaging/bioconda/turbo-picard recipes/turbo-picard\n"
                "cp -R packaging/bioconda/turbo-picard-picard-shim recipes/turbo-picard-picard-shim\n"
                "bioconda-utils lint recipes config.yml --packages turbo-picard turbo-picard-picard-shim\n"
                "bioconda-utils build --docker --mulled-test turbo-picard\n"
                "bioconda-utils build --docker --mulled-test turbo-picard-picard-shim\n"
                "CITATION.cff cites the archived turbo-picard release; benchmark input citations stay separate with SHA-256 hashes.\n"
                "python3 tools/verify_benchmark_thresholds.py\npython3 tools/verify_ci_coverage.py\npython3 tools/verify_parity_docs.py\npython3 tools/verify_readme_links.py\npython3 tools/verify_site_links.py\n"
                "benchmark threshold release gate requires 5.00x 20.00x 50.00x\n"
                "Do not open a Bioconda PR while the recipes still use source.path; wait for the release-ready verifier.\n"
                "Recipe notes\n"
                "not `noarch`\n"
                "skip: true  # [win]\n"
                "cargo-bundle-licenses --format yaml --output THIRDPARTY.yml\n"
                "license_file\n"
                "THIRDPARTY.yml\n"
                "turbo-picard =={{ version }}\n"
                "picard ==0\n"
                "run_constrained\n"
                f"{verify_bioconda_recipes.RELEASE_CANDIDATE_PORTFOLIO_COMMAND_TEXT}\n"
            )
            for recipe_dir in (
                root / "packaging" / "bioconda" / "turbo-picard",
                root / "packaging" / "bioconda" / "turbo-picard-picard-shim",
            ):
                (recipe_dir / "meta.yaml").write_text(
                    """
source:
  url: https://github.com/dnncha/turbo-picard/archive/refs/tags/v0.1.0.tar.gz
  sha256: 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef
""",
                    encoding="utf-8",
                )
            (root / "benchmarks" / "real-data" / "manifest.json").write_text(
                '{"datasets": [{"release_tier": "release_candidate"}]}',
                encoding="utf-8",
            )
            (root / "tools" / "verify_real_data_evidence.py").write_text(
                "# verifier\n", encoding="utf-8"
            )
            for path in (
                root / "README.md",
                root / "docs" / "site" / "index.html",
                root / "docs" / "packaging.rst",
                root / "packaging" / "bioconda" / "turbo-picard" / "README.md",
            ):
                path.write_text(
                    "python3 tools/verify_real_data_evidence.py\n"
                    "python3 tools/verify_real_data_evidence.py --release-ready\n"
                    "python3 tools/update_real_data_manifest.py\n",
                    encoding="utf-8",
                )
            for path in (
                root / "docs" / "packaging.rst",
                root / "packaging" / "bioconda" / "turbo-picard" / "README.md",
                root
                / "packaging"
                / "bioconda"
                / "turbo-picard-picard-shim"
                / "README.md",
            ):
                with path.open("a", encoding="utf-8") as handle:
                    handle.write(bioconda_submission_commands)
            manifest_text = """
{
  "datasets": [
    {
      "id": "gatk-na12878-mito",
      "release_tier": "release_candidate",
      "evidence_markdown": "benchmarks/real-data/gatk-na12878-mito/evidence/real-data-comparison.md",
      "evidence_json": "benchmarks/real-data/gatk-na12878-mito/evidence/real-data-comparison.json",
      "source_url": "https://github.com/broadinstitute/gatk/blob/e8c49f600b06c658e0fa9bf67256340ebb46bc48/src/test/resources/org/broadinstitute/hellbender/tools/mutect/mito/NA12878.bam",
      "source_commit": "e8c49f600b06c658e0fa9bf67256340ebb46bc48",
      "sha256": "70ea2e429805a75ce6007a32ba176ea7c697a398e0c39a9d58aaaa30e1ed86c3",
      "scope_caveat": "GATK public NA12878 mitochondrial test BAM.",
      "minimum_input_bytes": 1000000,
      "expected_commands": {
        "ViewSam": "SAM record digest",
        "CleanSam": "post-command SAM record digest",
        "CollectQualityYieldMetrics": "stable metrics digest",
        "CollectAlignmentSummaryMetrics": "stable metrics digest",
        "MarkDuplicates": "duplicate-marking semantic digest plus stable metrics digest",
        "CollectInsertSizeMetrics": "stable metrics digest with insert-size histogram",
        "ValidateSamFile": "summary validation histogram plus exit code"
      }
    },
    {
      "id": "picard-snvq",
      "release_tier": "release_candidate",
      "evidence_markdown": "benchmarks/real-data/picard-snvq/evidence/real-data-comparison.md",
      "evidence_json": "benchmarks/real-data/picard-snvq/evidence/real-data-comparison.json",
      "source_url": "https://github.com/broadinstitute/picard/blob/fc0b08410d38a10afd08e467dab74bf5e2e71310/testdata/picard/sam/snvq_metrics_test.bam",
      "source_commit": "fc0b08410d38a10afd08e467dab74bf5e2e71310",
      "sha256": "be0daa7cb8e9ce11f2f68ac3db8c229d530736aaf7b80df3669fdb00779c06b3",
      "scope_caveat": "Picard public SNVQ metrics test BAM.",
      "minimum_input_bytes": 1000000,
      "expected_commands": {
        "ViewSam": "SAM record digest",
        "CleanSam": "post-command SAM record digest",
        "CollectQualityYieldMetrics": "stable metrics digest",
        "CollectAlignmentSummaryMetrics": "stable metrics digest",
        "MarkDuplicates": "duplicate-marking semantic digest plus stable metrics digest"
      }
    }
  ]
}
"""
            (root / "benchmarks" / "real-data" / "manifest.json").write_text(
                manifest_text,
                encoding="utf-8",
            )
            benchmark_text = (
                "Benchmark evidence\n"
                "CITATION.cff cites the archived turbo-picard release; benchmark input citations stay separate with SHA-256 hashes.\n"
                "python3 tools/bench_suite.py --repeats 1 --skip-build\n"
                "docs/site/assets/bench-suite-output.txt\n"
                "docs/site/assets/benchmark-data.json\n"
                "2026-05-31\n"
                "32/32 PASS\n"
                "112.07x\n"
                "UpdateVcfSequenceDictionary\n"
                "7.40x\n"
                "RevertSam\n"
                "26.24x\n"
                "27.31x\n"
                "IntervalListTools\n"
                "LiftoverVcf\n"
                "CollectMultipleMetrics\n"
                "CollectGcBiasMetrics\n"
                "5.00x\n"
                "20.00x\n"
                "50.00x\n"
            )
            (root / "docs" / "site" / "assets" / "benchmark-data.json").write_text(
                """
{
  "source": "python3 tools/bench_suite.py --repeats 1 --skip-build",
  "date": "2026-05-31",
  "parity": "32/32 PASS",
  "summary": {
    "top_speedup": 112.07,
    "top_command": "UpdateVcfSequenceDictionary",
    "floor_speedup": 7.4,
    "floor_command": "RevertSam",
    "median_speedup": 26.24,
    "geometric_mean_speedup": 27.31
  },
  "benchmarks": [
    {"command": "IntervalListTools", "speedup": 30.62, "parity": "PASS"},
    {"command": "LiftoverVcf", "speedup": 17.11, "parity": "PASS"},
    {"command": "CollectMultipleMetrics", "speedup": 23.53, "parity": "PASS"},
    {"command": "CollectGcBiasMetrics", "speedup": 39.92, "parity": "PASS"}
  ],
  "source_artifact": "docs/site/assets/bench-suite-output.txt"
}
""",
                encoding="utf-8",
            )
            (root / "packaging" / "bioconda" / "BIOCONDA_PR.md").write_text(
                "python3 tools/verify_real_data_evidence.py\n"
                "python3 tools/verify_real_data_evidence.py --release-ready\n"
                "python3 tools/update_real_data_manifest.py\n"
                "https://github.com/dnncha/turbo-picard/archive/refs/tags/v0.1.0.tar.gz\n"
                "Archive SHA-256:\n"
                "`0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef`\n"
                "python3 tools/bioconda_release_preflight.py\n"
                "python3 -m unittest discover tools\n"
                "python3 tools/prepare_bioconda_release.py\n"
                "--archive ~/Downloads/turbo-picard-0.1.0.tar.gz\n"
                "Prefer `--archive` for release submission\n"
                "python3 tools/verify_bioconda_recipes.py --release-ready\n"
                "python3 tools/verify_release_versions.py\n"
                "python3 tools/verify_benchmark_suite_coverage.py\n"
                "python3 tools/verify_benchmark_thresholds.py\npython3 tools/verify_ci_coverage.py\npython3 tools/verify_parity_docs.py\npython3 tools/verify_readme_links.py\npython3 tools/verify_site_links.py\n"
                "./tools/verify_package_install.sh\n"
                "cargo test --workspace\n"
                "cp -R packaging/bioconda/turbo-picard recipes/turbo-picard\n"
                "cp -R packaging/bioconda/turbo-picard-picard-shim recipes/turbo-picard-picard-shim\n"
                "bioconda-utils lint recipes config.yml --packages turbo-picard turbo-picard-picard-shim\n"
                "bioconda-utils build --docker --mulled-test turbo-picard\n"
                "bioconda-utils build --docker --mulled-test turbo-picard-picard-shim\n"
                "Recipe notes\n"
                "not `noarch`\n"
                "skip: true  # [win]\n"
                "cargo-bundle-licenses --format yaml --output THIRDPARTY.yml\n"
                "license_file\n"
                "THIRDPARTY.yml\n"
                "turbo-picard =={{ version }}\n"
                "picard ==0\n"
                "run_constrained\n"
                f"{verify_bioconda_recipes.RELEASE_CANDIDATE_PORTFOLIO_COMMAND_TEXT}\n"
                f"{benchmark_text}"
                "benchmarks/real-data/gatk-na12878-mito/evidence/real-data-comparison.md\n"
                "benchmarks/real-data/gatk-na12878-mito/evidence/real-data-comparison.json\n"
                "https://github.com/broadinstitute/gatk/blob/e8c49f600b06c658e0fa9bf67256340ebb46bc48/src/test/resources/org/broadinstitute/hellbender/tools/mutect/mito/NA12878.bam\n"
                "e8c49f600b06c658e0fa9bf67256340ebb46bc48\n"
                "70ea2e429805a75ce6007a32ba176ea7c697a398e0c39a9d58aaaa30e1ed86c3\n"
                "GATK public NA12878 mitochondrial test BAM.\n"
                "1000000\n"
                "ViewSam\n"
                "SAM record digest\n"
                "CleanSam\n"
                "post-command SAM record digest\n"
                "CollectQualityYieldMetrics\n"
                "stable metrics digest\n"
                "CollectAlignmentSummaryMetrics\n"
                "MarkDuplicates\n"
                "duplicate-marking semantic digest plus stable metrics digest\n"
                "CollectInsertSizeMetrics\n"
                "stable metrics digest with insert-size histogram\n"
                "ValidateSamFile\n"
                "summary validation histogram plus exit code\n"
                "benchmarks/real-data/picard-snvq/evidence/real-data-comparison.md\n"
                "benchmarks/real-data/picard-snvq/evidence/real-data-comparison.json\n"
                "https://github.com/broadinstitute/picard/blob/fc0b08410d38a10afd08e467dab74bf5e2e71310/testdata/picard/sam/snvq_metrics_test.bam\n"
                "fc0b08410d38a10afd08e467dab74bf5e2e71310\n"
                "be0daa7cb8e9ce11f2f68ac3db8c229d530736aaf7b80df3669fdb00779c06b3\n"
                "Picard public SNVQ metrics test BAM.\n",
                encoding="utf-8",
            )

            self.assertEqual(verify_bioconda_recipes.validate_release_evidence(root), [])

            (root / "packaging" / "bioconda" / "BIOCONDA_PR.md").write_text(
                "python3 tools/verify_real_data_evidence.py\n"
                "python3 tools/verify_real_data_evidence.py --release-ready\n"
                "python3 tools/update_real_data_manifest.py\n"
                "python3 tools/bioconda_release_preflight.py\n"
                "python3 -m unittest discover tools\n"
                "python3 tools/prepare_bioconda_release.py\n"
                "python3 tools/verify_bioconda_recipes.py --release-ready\n"
                "python3 tools/verify_benchmark_suite_coverage.py\n"
                "./tools/verify_package_install.sh\n"
                "cargo test --workspace\n"
                "cp -R packaging/bioconda/turbo-picard recipes/turbo-picard\n"
                "cp -R packaging/bioconda/turbo-picard-picard-shim recipes/turbo-picard-picard-shim\n"
                "bioconda-utils lint recipes config.yml --packages turbo-picard turbo-picard-picard-shim\n"
                "bioconda-utils build --docker --mulled-test turbo-picard\n"
                "bioconda-utils build --docker --mulled-test turbo-picard-picard-shim\n"
                "benchmarks/real-data/gatk-na12878-mito/evidence/real-data-comparison.md\n"
                "benchmarks/real-data/gatk-na12878-mito/evidence/real-data-comparison.json\n"
                "https://github.com/broadinstitute/gatk/blob/e8c49f600b06c658e0fa9bf67256340ebb46bc48/src/test/resources/org/broadinstitute/hellbender/tools/mutect/mito/NA12878.bam\n"
                "e8c49f600b06c658e0fa9bf67256340ebb46bc48\n"
                "70ea2e429805a75ce6007a32ba176ea7c697a398e0c39a9d58aaaa30e1ed86c3\n"
                "GATK public NA12878 mitochondrial test BAM.\n"
                "1000000\n"
                "benchmarks/real-data/picard-snvq/evidence/real-data-comparison.md\n"
                "benchmarks/real-data/picard-snvq/evidence/real-data-comparison.json\n"
                "https://github.com/broadinstitute/picard/blob/fc0b08410d38a10afd08e467dab74bf5e2e71310/testdata/picard/sam/snvq_metrics_test.bam\n"
                "fc0b08410d38a10afd08e467dab74bf5e2e71310\n"
                "be0daa7cb8e9ce11f2f68ac3db8c229d530736aaf7b80df3669fdb00779c06b3\n"
                "Picard public SNVQ metrics test BAM.\n",
                encoding="utf-8",
            )
            self.assertIn(
                "packaging/bioconda/BIOCONDA_PR.md missing Bioconda PR evidence text: python3 tools/verify_release_versions.py",
                verify_bioconda_recipes.validate_release_evidence(root),
            )
            self.assertIn(
                "packaging/bioconda/BIOCONDA_PR.md missing Bioconda PR evidence text: python3 tools/verify_benchmark_thresholds.py",
                verify_bioconda_recipes.validate_release_evidence(root),
            )

            (root / "packaging" / "bioconda" / "BIOCONDA_PR.md").write_text(
                "python3 tools/verify_real_data_evidence.py\n"
                "python3 tools/verify_real_data_evidence.py --release-ready\n"
                "python3 tools/update_real_data_manifest.py\n"
                "python3 tools/bioconda_release_preflight.py\n"
                "python3 -m unittest discover tools\n"
                "python3 tools/prepare_bioconda_release.py\n"
                "python3 tools/verify_bioconda_recipes.py --release-ready\n"
                "python3 tools/verify_release_versions.py\n"
                "python3 tools/verify_benchmark_suite_coverage.py\n"
                "./tools/verify_package_install.sh\n"
                "cargo test --workspace\n"
                "cp -R packaging/bioconda/turbo-picard recipes/turbo-picard\n"
                "cp -R packaging/bioconda/turbo-picard-picard-shim recipes/turbo-picard-picard-shim\n"
                "bioconda-utils lint recipes config.yml --packages turbo-picard turbo-picard-picard-shim\n"
                "bioconda-utils build --docker --mulled-test turbo-picard\n"
                "bioconda-utils build --docker --mulled-test turbo-picard-picard-shim\n"
                "benchmarks/real-data/gatk-na12878-mito/evidence/real-data-comparison.md\n"
                "benchmarks/real-data/gatk-na12878-mito/evidence/real-data-comparison.json\n"
                "https://github.com/broadinstitute/gatk/blob/e8c49f600b06c658e0fa9bf67256340ebb46bc48/src/test/resources/org/broadinstitute/hellbender/tools/mutect/mito/NA12878.bam\n"
                "e8c49f600b06c658e0fa9bf67256340ebb46bc48\n"
                "70ea2e429805a75ce6007a32ba176ea7c697a398e0c39a9d58aaaa30e1ed86c3\n"
                "GATK public NA12878 mitochondrial test BAM.\n"
                "1000000\n"
                "benchmarks/real-data/picard-snvq/evidence/real-data-comparison.md\n"
                "benchmarks/real-data/picard-snvq/evidence/real-data-comparison.json\n"
                "https://github.com/broadinstitute/picard/blob/fc0b08410d38a10afd08e467dab74bf5e2e71310/testdata/picard/sam/snvq_metrics_test.bam\n"
                "fc0b08410d38a10afd08e467dab74bf5e2e71310\n"
                "be0daa7cb8e9ce11f2f68ac3db8c229d530736aaf7b80df3669fdb00779c06b3\n"
                "Picard public SNVQ metrics test BAM.\n",
                encoding="utf-8",
            )
            errors = verify_bioconda_recipes.validate_release_evidence(root)
            self.assertIn(
                "packaging/bioconda/BIOCONDA_PR.md missing tagged source archive URL",
                errors,
            )
            self.assertIn(
                "packaging/bioconda/BIOCONDA_PR.md missing concrete source archive SHA-256",
                errors,
            )

            (root / "packaging" / "bioconda" / "BIOCONDA_PR.md").write_text(
                "python3 tools/verify_real_data_evidence.py\n"
                "python3 tools/verify_real_data_evidence.py --release-ready\n"
                "python3 tools/update_real_data_manifest.py\n"
                "https://github.com/dnncha/turbo-picard/archive/refs/tags/v0.1.0.tar.gz\n"
                "Archive SHA-256:\n"
                "`ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff`\n"
                "python3 tools/bioconda_release_preflight.py\n"
                "python3 -m unittest discover tools\n"
                "python3 tools/prepare_bioconda_release.py\n"
                "python3 tools/verify_bioconda_recipes.py --release-ready\n"
                "python3 tools/verify_release_versions.py\n"
                "python3 tools/verify_benchmark_suite_coverage.py\n"
                "./tools/verify_package_install.sh\n"
                "cargo test --workspace\n"
                "cp -R packaging/bioconda/turbo-picard recipes/turbo-picard\n"
                "cp -R packaging/bioconda/turbo-picard-picard-shim recipes/turbo-picard-picard-shim\n"
                "bioconda-utils lint recipes config.yml --packages turbo-picard turbo-picard-picard-shim\n"
                "bioconda-utils build --docker --mulled-test turbo-picard\n"
                "bioconda-utils build --docker --mulled-test turbo-picard-picard-shim\n"
                "benchmarks/real-data/gatk-na12878-mito/evidence/real-data-comparison.md\n"
                "benchmarks/real-data/gatk-na12878-mito/evidence/real-data-comparison.json\n"
                "https://github.com/broadinstitute/gatk/blob/e8c49f600b06c658e0fa9bf67256340ebb46bc48/src/test/resources/org/broadinstitute/hellbender/tools/mutect/mito/NA12878.bam\n"
                "e8c49f600b06c658e0fa9bf67256340ebb46bc48\n"
                "70ea2e429805a75ce6007a32ba176ea7c697a398e0c39a9d58aaaa30e1ed86c3\n"
                "GATK public NA12878 mitochondrial test BAM.\n"
                "1000000\n"
                "benchmarks/real-data/picard-snvq/evidence/real-data-comparison.md\n"
                "benchmarks/real-data/picard-snvq/evidence/real-data-comparison.json\n"
                "https://github.com/broadinstitute/picard/blob/fc0b08410d38a10afd08e467dab74bf5e2e71310/testdata/picard/sam/snvq_metrics_test.bam\n"
                "fc0b08410d38a10afd08e467dab74bf5e2e71310\n"
                "be0daa7cb8e9ce11f2f68ac3db8c229d530736aaf7b80df3669fdb00779c06b3\n"
                "Picard public SNVQ metrics test BAM.\n",
                encoding="utf-8",
            )
            self.assertIn(
                "packaging/bioconda/BIOCONDA_PR.md source SHA-256 does not match turbo-picard recipe",
                verify_bioconda_recipes.validate_release_evidence(root),
            )

            (root / "packaging" / "bioconda" / "turbo-picard-picard-shim" / "meta.yaml").write_text(
                """
source:
  url: https://github.com/dnncha/turbo-picard/archive/refs/tags/v0.2.0.tar.gz
  sha256: 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef
""",
                encoding="utf-8",
            )
            (root / "packaging" / "bioconda" / "BIOCONDA_PR.md").write_text(
                "python3 tools/verify_real_data_evidence.py\n"
                "python3 tools/verify_real_data_evidence.py --release-ready\n"
                "python3 tools/update_real_data_manifest.py\n"
                "https://github.com/dnncha/turbo-picard/archive/refs/tags/v0.1.0.tar.gz\n"
                "Archive SHA-256:\n"
                "`0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef`\n"
                "python3 tools/bioconda_release_preflight.py\n"
                "python3 -m unittest discover tools\n"
                "python3 tools/prepare_bioconda_release.py\n"
                "python3 tools/verify_bioconda_recipes.py --release-ready\n"
                "python3 tools/verify_release_versions.py\n"
                "python3 tools/verify_benchmark_suite_coverage.py\n"
                "./tools/verify_package_install.sh\n"
                "cargo test --workspace\n"
                "cp -R packaging/bioconda/turbo-picard recipes/turbo-picard\n"
                "cp -R packaging/bioconda/turbo-picard-picard-shim recipes/turbo-picard-picard-shim\n"
                "bioconda-utils lint recipes config.yml --packages turbo-picard turbo-picard-picard-shim\n"
                "bioconda-utils build --docker --mulled-test turbo-picard\n"
                "bioconda-utils build --docker --mulled-test turbo-picard-picard-shim\n"
                "benchmarks/real-data/gatk-na12878-mito/evidence/real-data-comparison.md\n"
                "benchmarks/real-data/gatk-na12878-mito/evidence/real-data-comparison.json\n"
                "https://github.com/broadinstitute/gatk/blob/e8c49f600b06c658e0fa9bf67256340ebb46bc48/src/test/resources/org/broadinstitute/hellbender/tools/mutect/mito/NA12878.bam\n"
                "e8c49f600b06c658e0fa9bf67256340ebb46bc48\n"
                "70ea2e429805a75ce6007a32ba176ea7c697a398e0c39a9d58aaaa30e1ed86c3\n"
                "GATK public NA12878 mitochondrial test BAM.\n"
                "1000000\n"
                "benchmarks/real-data/picard-snvq/evidence/real-data-comparison.md\n"
                "benchmarks/real-data/picard-snvq/evidence/real-data-comparison.json\n"
                "https://github.com/broadinstitute/picard/blob/fc0b08410d38a10afd08e467dab74bf5e2e71310/testdata/picard/sam/snvq_metrics_test.bam\n"
                "fc0b08410d38a10afd08e467dab74bf5e2e71310\n"
                "be0daa7cb8e9ce11f2f68ac3db8c229d530736aaf7b80df3669fdb00779c06b3\n"
                "Picard public SNVQ metrics test BAM.\n",
                encoding="utf-8",
            )
            self.assertIn(
                "packaging/bioconda/BIOCONDA_PR.md source URL does not match turbo-picard-picard-shim recipe",
                verify_bioconda_recipes.validate_release_evidence(root),
            )

            (root / "packaging" / "bioconda" / "turbo-picard-picard-shim" / "meta.yaml").write_text(
                """
source:
  url: https://github.com/dnncha/turbo-picard/archive/refs/tags/v0.1.0.tar.gz
  sha256: 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef
""",
                encoding="utf-8",
            )

            (root / "packaging" / "bioconda" / "BIOCONDA_PR.md").write_text(
                "python3 tools/verify_real_data_evidence.py\n"
                "python3 tools/verify_real_data_evidence.py --release-ready\n"
                "python3 tools/update_real_data_manifest.py\n"
                "python3 tools/bioconda_release_preflight.py\n"
                "python3 -m unittest discover tools\n"
                "python3 tools/prepare_bioconda_release.py\n"
                "python3 tools/verify_bioconda_recipes.py --release-ready\n"
                "python3 tools/verify_release_versions.py\n"
                "python3 tools/verify_benchmark_suite_coverage.py\n"
                "./tools/verify_package_install.sh\n"
                "cargo test --workspace\n"
                "bioconda-utils build --docker --mulled-test turbo-picard\n"
                "bioconda-utils build --docker --mulled-test turbo-picard-picard-shim\n"
                "<github-v0.1.0-source-archive-sha256>\n"
                "benchmarks/real-data/gatk-na12878-mito/evidence/real-data-comparison.md\n"
                "benchmarks/real-data/gatk-na12878-mito/evidence/real-data-comparison.json\n"
                "https://github.com/broadinstitute/gatk/blob/e8c49f600b06c658e0fa9bf67256340ebb46bc48/src/test/resources/org/broadinstitute/hellbender/tools/mutect/mito/NA12878.bam\n"
                "e8c49f600b06c658e0fa9bf67256340ebb46bc48\n"
                "70ea2e429805a75ce6007a32ba176ea7c697a398e0c39a9d58aaaa30e1ed86c3\n"
                "GATK public NA12878 mitochondrial test BAM.\n"
                "1000000\n"
                "benchmarks/real-data/picard-snvq/evidence/real-data-comparison.md\n"
                "benchmarks/real-data/picard-snvq/evidence/real-data-comparison.json\n"
                "https://github.com/broadinstitute/picard/blob/fc0b08410d38a10afd08e467dab74bf5e2e71310/testdata/picard/sam/snvq_metrics_test.bam\n"
                "fc0b08410d38a10afd08e467dab74bf5e2e71310\n"
                "be0daa7cb8e9ce11f2f68ac3db8c229d530736aaf7b80df3669fdb00779c06b3\n"
                "Picard public SNVQ metrics test BAM.\n",
                encoding="utf-8",
            )
            self.assertIn(
                "packaging/bioconda/BIOCONDA_PR.md still contains source archive SHA placeholder",
                verify_bioconda_recipes.validate_release_evidence(root),
            )

            (root / "packaging" / "bioconda" / "BIOCONDA_PR.md").write_text(
                "python3 tools/verify_real_data_evidence.py\n"
                "python3 tools/verify_real_data_evidence.py --release-ready\n"
                "python3 tools/update_real_data_manifest.py\n"
                "python3 tools/bioconda_release_preflight.py\n"
                "python3 -m unittest discover tools\n"
                "python3 tools/prepare_bioconda_release.py\n"
                "python3 tools/verify_bioconda_recipes.py --release-ready\n"
                "./tools/verify_package_install.sh\n"
                "cargo test --workspace\n"
                "bioconda-utils build --docker --mulled-test turbo-picard\n"
                "bioconda-utils build --docker --mulled-test turbo-picard-picard-shim\n"
                "benchmarks/real-data/gatk-na12878-mito/evidence/real-data-comparison.md\n"
                "benchmarks/real-data/gatk-na12878-mito/evidence/real-data-comparison.json\n"
                "https://github.com/broadinstitute/gatk/blob/e8c49f600b06c658e0fa9bf67256340ebb46bc48/src/test/resources/org/broadinstitute/hellbender/tools/mutect/mito/NA12878.bam\n"
                "e8c49f600b06c658e0fa9bf67256340ebb46bc48\n"
                "70ea2e429805a75ce6007a697a398e0c39a9d58aaaa30e1ed86c3\n"
                "GATK public NA12878 mitochondrial test BAM.\n",
                encoding="utf-8",
            )
            self.assertIn(
                "packaging/bioconda/BIOCONDA_PR.md missing Bioconda PR evidence text: benchmarks/real-data/picard-snvq/evidence/real-data-comparison.md",
                verify_bioconda_recipes.validate_release_evidence(root),
            )

            (root / "packaging" / "bioconda" / "BIOCONDA_PR.md").write_text(
                "python3 tools/verify_real_data_evidence.py\n"
                "python3 tools/verify_real_data_evidence.py --release-ready\n"
                "python3 tools/update_real_data_manifest.py\n"
                "python3 tools/bioconda_release_preflight.py\n"
                "python3 -m unittest discover tools\n"
                "python3 tools/prepare_bioconda_release.py\n"
                "python3 tools/verify_bioconda_recipes.py --release-ready\n"
                "./tools/verify_package_install.sh\n"
                "cargo test --workspace\n"
                "bioconda-utils build --docker --mulled-test turbo-picard\n"
                "bioconda-utils build --docker --mulled-test turbo-picard-picard-shim\n"
                "benchmarks/real-data/gatk-na12878-mito/evidence/real-data-comparison.md\n"
                "benchmarks/real-data/gatk-na12878-mito/evidence/real-data-comparison.json\n"
                "https://github.com/broadinstitute/gatk/blob/e8c49f600b06c658e0fa9bf67256340ebb46bc48/src/test/resources/org/broadinstitute/hellbender/tools/mutect/mito/NA12878.bam\n"
                "e8c49f600b06c658e0fa9bf67256340ebb46bc48\n"
                "70ea2e429805a75ce6007a697a398e0c39a9d58aaaa30e1ed86c3\n"
                "GATK public NA12878 mitochondrial test BAM.\n"
                "1000000\n"
                "benchmarks/real-data/picard-snvq/evidence/real-data-comparison.md\n"
                "benchmarks/real-data/picard-snvq/evidence/real-data-comparison.json\n"
                "https://github.com/broadinstitute/picard/blob/fc0b08410d38a10afd08e467dab74bf5e2e71310/testdata/picard/sam/snvq_metrics_test.bam\n"
                "fc0b08410d38a10afd08e467dab74bf5e2e71310\n"
                "be0daa7cb8e9ce11f2f68ac3db8c229d530736aaf7b80df3669fdb00779c06b3\n"
                "Picard public SNVQ metrics test BAM.\n",
                encoding="utf-8",
            )
            (root / "README.md").write_text("missing command\n", encoding="utf-8")
            self.assertIn(
                "README.md missing real-data evidence verifier command",
                verify_bioconda_recipes.validate_release_evidence(root),
            )

            (root / "README.md").write_text(
                "python3 tools/verify_real_data_evidence.py\n"
                "python3 tools/verify_real_data_evidence.py --release-ready\n"
                "python3 tools/update_real_data_manifest.py\n",
                encoding="utf-8",
            )
            (root / "packaging" / "bioconda" / "BIOCONDA_PR.md").write_text(
                "python3 tools/verify_real_data_evidence.py\n"
                "python3 tools/verify_real_data_evidence.py --release-ready\n"
                "python3 tools/update_real_data_manifest.py\n"
                "python3 tools/bioconda_release_preflight.py\n"
                "python3 -m unittest discover tools\n"
                "python3 tools/prepare_bioconda_release.py\n"
                "python3 tools/verify_bioconda_recipes.py --release-ready\n"
                "./tools/verify_package_install.sh\n"
                "cargo test --workspace\n"
                "bioconda-utils build --docker --mulled-test turbo-picard\n"
                "benchmarks/real-data/gatk-na12878-mito/evidence/real-data-comparison.md\n"
                "benchmarks/real-data/gatk-na12878-mito/evidence/real-data-comparison.json\n"
                "https://github.com/broadinstitute/gatk/blob/e8c49f600b06c658e0fa9bf67256340ebb46bc48/src/test/resources/org/broadinstitute/hellbender/tools/mutect/mito/NA12878.bam\n"
                "e8c49f600b06c658e0fa9bf67256340ebb46bc48\n"
                "70ea2e429805a75ce6007a697a398e0c39a9d58aaaa30e1ed86c3\n"
                "GATK public NA12878 mitochondrial test BAM.\n"
                "1000000\n"
                "benchmarks/real-data/picard-snvq/evidence/real-data-comparison.md\n"
                "benchmarks/real-data/picard-snvq/evidence/real-data-comparison.json\n"
                "https://github.com/broadinstitute/picard/blob/fc0b08410d38a10afd08e467dab74bf5e2e71310/testdata/picard/sam/snvq_metrics_test.bam\n"
                "fc0b08410d38a10afd08e467dab74bf5e2e71310\n"
                "be0daa7cb8e9ce11f2f68ac3db8c229d530736aaf7b80df3669fdb00779c06b3\n"
                "Picard public SNVQ metrics test BAM.\n",
                encoding="utf-8",
            )
            self.assertIn(
                "packaging/bioconda/BIOCONDA_PR.md missing Bioconda PR evidence text: bioconda-utils lint recipes config.yml --packages turbo-picard turbo-picard-picard-shim",
                verify_bioconda_recipes.validate_release_evidence(root),
            )

            (root / "packaging" / "bioconda" / "BIOCONDA_PR.md").write_text(
                "python3 tools/verify_real_data_evidence.py\n"
                "python3 tools/verify_real_data_evidence.py --release-ready\n"
                "python3 tools/update_real_data_manifest.py\n"
                "python3 tools/prepare_bioconda_release.py\n"
                "python3 tools/verify_bioconda_recipes.py --release-ready\n"
                "./tools/verify_package_install.sh\n"
                "cargo test --workspace\n"
                "bioconda-utils build --docker --mulled-test turbo-picard\n"
                "bioconda-utils build --docker --mulled-test turbo-picard-picard-shim\n"
                "benchmarks/real-data/gatk-na12878-mito/evidence/real-data-comparison.md\n"
                "benchmarks/real-data/gatk-na12878-mito/evidence/real-data-comparison.json\n"
                "https://github.com/broadinstitute/gatk/blob/e8c49f600b06c658e0fa9bf67256340ebb46bc48/src/test/resources/org/broadinstitute/hellbender/tools/mutect/mito/NA12878.bam\n"
                "e8c49f600b06c658e0fa9bf67256340ebb46bc48\n"
                "70ea2e429805a75ce6007a697a398e0c39a9d58aaaa30e1ed86c3\n"
                "GATK public NA12878 mitochondrial test BAM.\n"
                "1000000\n"
                "benchmarks/real-data/picard-snvq/evidence/real-data-comparison.md\n"
                "benchmarks/real-data/picard-snvq/evidence/real-data-comparison.json\n"
                "https://github.com/broadinstitute/picard/blob/fc0b08410d38a10afd08e467dab74bf5e2e71310/testdata/picard/sam/snvq_metrics_test.bam\n"
                "fc0b08410d38a10afd08e467dab74bf5e2e71310\n"
                "be0daa7cb8e9ce11f2f68ac3db8c229d530736aaf7b80df3669fdb00779c06b3\n"
                "Picard public SNVQ metrics test BAM.\n",
                encoding="utf-8",
            )
            self.assertIn(
                "packaging/bioconda/BIOCONDA_PR.md missing Bioconda PR evidence text: python3 -m unittest discover tools",
                verify_bioconda_recipes.validate_release_evidence(root),
            )

            (root / "README.md").write_text(
                "python3 tools/verify_real_data_evidence.py\n"
                "python3 tools/verify_real_data_evidence.py --release-ready\n"
                "python3 tools/update_real_data_manifest.py\n",
                encoding="utf-8",
            )
            (root / "benchmarks" / "real-data" / "manifest.json").write_text(
                '{"datasets": [{"release_tier": "public_smoke"}]}',
                encoding="utf-8",
            )
            self.assertIn(
                "release evidence manifest has no release_candidate dataset",
                verify_bioconda_recipes.validate_release_evidence(root),
            )

    def test_release_evidence_rejects_malformed_manifest_shape_without_crashing(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "benchmarks" / "real-data").mkdir(parents=True)
            (root / "tools").mkdir()
            (root / "docs" / "site").mkdir(parents=True)
            (root / "packaging" / "bioconda" / "turbo-picard").mkdir(parents=True)
            (root / "packaging" / "bioconda").mkdir(parents=True, exist_ok=True)
            (root / "tools" / "verify_real_data_evidence.py").write_text(
                "# verifier\n", encoding="utf-8"
            )
            for path in (
                root / "README.md",
                root / "docs" / "site" / "index.html",
                root / "packaging" / "bioconda" / "turbo-picard" / "README.md",
                root / "packaging" / "bioconda" / "BIOCONDA_PR.md",
            ):
                path.write_text(
                    "python3 tools/verify_real_data_evidence.py\n"
                    "python3 tools/verify_real_data_evidence.py --release-ready\n"
                    "python3 tools/update_real_data_manifest.py\n",
                    encoding="utf-8",
                )

            manifest = root / "benchmarks" / "real-data" / "manifest.json"
            manifest.write_text('["not", "an", "object"]', encoding="utf-8")
            errors = verify_bioconda_recipes.validate_release_evidence(root)
            self.assertIn("release evidence manifest must be a JSON object", errors)
            self.assertIn("release evidence manifest has no release_candidate dataset", errors)

            manifest.write_text('{"datasets": "not-a-list"}', encoding="utf-8")
            errors = verify_bioconda_recipes.validate_release_evidence(root)
            self.assertIn("release evidence manifest datasets must be a list", errors)
            self.assertIn("release evidence manifest has no release_candidate dataset", errors)

    def test_bioconda_pr_requires_benchmark_receipts(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            asset_dir = root / "docs" / "site" / "assets"
            asset_dir.mkdir(parents=True)
            (asset_dir / "benchmark-data.json").write_text(
                """
{
  "source": "python3 tools/bench_suite.py --repeats 1 --skip-build",
  "date": "2026-05-31",
  "parity": "32/32 PASS",
  "summary": {
    "top_speedup": 112.07,
    "top_command": "UpdateVcfSequenceDictionary",
    "floor_speedup": 7.4,
    "floor_command": "RevertSam",
    "median_speedup": 26.24,
    "geometric_mean_speedup": 27.31
  },
  "benchmarks": [
    {"command": "IntervalListTools", "speedup": 30.62, "parity": "PASS"},
    {"command": "LiftoverVcf", "speedup": 17.11, "parity": "PASS"},
    {"command": "CollectMultipleMetrics", "speedup": 23.53, "parity": "PASS"},
    {"command": "CollectGcBiasMetrics", "speedup": 39.92, "parity": "PASS"}
  ],
  "source_artifact": "docs/site/assets/bench-suite-output.txt"
}
""",
                encoding="utf-8",
            )

            required = verify_bioconda_recipes.required_benchmark_pr_text(root)

        for needle in (
            "Benchmark evidence",
            "docs/site/assets/benchmark-data.json",
            "docs/site/assets/bench-suite-output.txt",
            "python3 tools/bench_suite.py --repeats 1 --skip-build",
            "2026-05-31",
            "32/32 PASS",
            "112.07x",
            "UpdateVcfSequenceDictionary",
            "7.40x",
            "RevertSam",
            "26.24x",
            "27.31x",
            "IntervalListTools",
            "LiftoverVcf",
            "CollectMultipleMetrics",
            "CollectGcBiasMetrics",
        ):
            self.assertIn(needle, required)

    def test_bioconda_pr_benchmark_requirements_report_malformed_data(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            asset_dir = root / "docs" / "site" / "assets"
            asset_dir.mkdir(parents=True)

            required, errors = verify_bioconda_recipes.benchmark_pr_text_requirements(root)
            self.assertEqual(required, ["docs/site/assets/benchmark-data.json"])
            self.assertEqual(errors, ["docs/site/assets/benchmark-data.json is missing"])

            (asset_dir / "benchmark-data.json").write_text("not json", encoding="utf-8")
            required, errors = verify_bioconda_recipes.benchmark_pr_text_requirements(root)
            self.assertEqual(required, ["docs/site/assets/benchmark-data.json"])
            self.assertTrue(
                errors[0].startswith(
                    "docs/site/assets/benchmark-data.json is not valid JSON:"
                )
            )

            (asset_dir / "benchmark-data.json").write_text(
                '{"summary": [], "benchmarks": "not-a-list"}',
                encoding="utf-8",
            )
            required, errors = verify_bioconda_recipes.benchmark_pr_text_requirements(root)
            self.assertIn("docs/site/assets/benchmark-data.json", required)
            self.assertIn(
                "docs/site/assets/benchmark-data.json missing summary object",
                errors,
            )
            self.assertIn(
                "docs/site/assets/benchmark-data.json missing benchmarks list",
                errors,
            )

    def test_main_reports_unknown_arguments(self) -> None:
        with redirect_stderr(io.StringIO()):
            self.assertEqual(verify_bioconda_recipes.main(["--unknown"]), 2)

    def test_main_does_not_skip_later_recipes_after_earlier_errors(self) -> None:
        original_recipes = verify_bioconda_recipes.RECIPES
        try:
            verify_bioconda_recipes.RECIPES = [
                {
                    "name": "missing-first",
                    "path": Path("/definitely/missing/first"),
                    "expected_bin": "first",
                    "is_shim": False,
                },
                {
                    "name": "missing-second",
                    "path": Path("/definitely/missing/second"),
                    "expected_bin": "second",
                    "is_shim": False,
                },
            ]
            stderr = io.StringIO()
            with redirect_stderr(stderr):
                self.assertEqual(verify_bioconda_recipes.main([]), 1)
            self.assertIn("missing-first missing meta.yaml", stderr.getvalue())
            self.assertIn("missing-second missing meta.yaml", stderr.getvalue())
        finally:
            verify_bioconda_recipes.RECIPES = original_recipes


if __name__ == "__main__":
    unittest.main()
