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
    run_dependency = (
        "    - {{ pin_subpackage('turbo-picard', exact=True) }}\n" if shim else ""
    )
    run_constrained = "  run_constrained:\n    - picard ==0\n" if shim else ""
    summary = (
        "Opt-in picard command shim for turbo-picard."
        if shim
        else "Fast Picard-compatible toolkit."
    )
    return f"""
{{% set version = "0.1.0" %}}
package:
  name: turbo-picard
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
  license: MIT
  license_file:
    - LICENSE
    - THIRDPARTY.yml
  summary: {summary}
extra:
  recipe-maintainers:
    - dnncha
"""


class BiocondaRecipeTests(unittest.TestCase):
    def test_main_recipe_accepts_required_rust_packaging_bits(self) -> None:
        build_sh = """
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
        self.assertIn("turbo-picard meta.yaml missing MIT license metadata", errors)
        self.assertIn("turbo-picard meta.yaml contains maintainer placeholder", errors)
        self.assertIn("turbo-picard build.sh missing cargo-bundle-licenses invocation", errors)
        self.assertIn("turbo-picard build.sh missing cargo install --locked", errors)
        self.assertIn("turbo-picard build.sh missing cargo install --no-track", errors)
        self.assertIn("turbo-picard build.sh missing cargo install --root ${PREFIX}", errors)
        self.assertIn("turbo-picard run_test.sh missing turbo-picard smoke test", errors)

    def test_shim_recipe_requires_main_package_pin_and_picard_conflict(self) -> None:
        errors = verify_bioconda_recipes.validate_recipe(
            name="turbo-picard-picard-shim",
            meta_yaml=recipe_meta(shim=True),
            build_sh="cargo-bundle-licenses --format yaml --output THIRDPARTY.yml\ncargo install --locked --no-track --root \"${PREFIX}\" --path crates/turbo-picard-cli --bin picard",
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
            build_sh="cargo-bundle-licenses --format yaml --output THIRDPARTY.yml\ncargo install --locked --no-track --root \"${PREFIX}\" --path crates/turbo-picard-cli --bin picard",
            run_test_sh="picard --version",
            expected_bin="picard",
            is_shim=True,
        )

        self.assertIn(
            "turbo-picard-picard-shim meta.yaml missing exact turbo-picard pin_subpackage run dependency",
            errors,
        )
        self.assertIn(
            "turbo-picard-picard-shim meta.yaml missing picard ==0 run_constrained conflict",
            errors,
        )

    def test_release_ready_mode_rejects_local_source_path(self) -> None:
        recipe = recipe_meta(source="""
source:
  path: ../../..
""")
        errors = verify_bioconda_recipes.validate_recipe(
            name="turbo-picard",
            meta_yaml=recipe,
            build_sh="cargo-bundle-licenses --format yaml --output THIRDPARTY.yml\ncargo install --locked --no-track --root \"${PREFIX}\" --path crates/turbo-picard-cli --bin turbo-picard",
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
            build_sh="cargo-bundle-licenses --format yaml --output THIRDPARTY.yml\ncargo install --locked --no-track --root \"${PREFIX}\" --path crates/turbo-picard-cli --bin turbo-picard",
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
            build_sh="cargo-bundle-licenses --format yaml --output THIRDPARTY.yml\ncargo install --locked --no-track --root \"${PREFIX}\" --path crates/turbo-picard-cli --bin turbo-picard",
            run_test_sh="turbo-picard --version\nturbo-picard MarkDuplicates --help\nturbo-picard SortSam --help\nturbo-picard CleanSam --help\nturbo-picard ViewSam --help",
            expected_bin="turbo-picard",
            is_shim=False,
            release_ready=True,
        )

        self.assertIn(
            "turbo-picard meta.yaml release source url must use refs/tags/v0.1.0.tar.gz",
            errors,
        )

    def test_release_evidence_requires_manifest_verifier_and_docs(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "benchmarks" / "real-data").mkdir(parents=True)
            (root / "tools").mkdir()
            (root / "docs" / "site").mkdir(parents=True)
            (root / "packaging" / "bioconda" / "turbo-picard").mkdir(parents=True)
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
                root / "packaging" / "bioconda" / "turbo-picard" / "README.md",
            ):
                path.write_text(
                    "python3 tools/verify_real_data_evidence.py\n"
                    "python3 tools/verify_real_data_evidence.py --release-ready\n"
                    "python3 tools/update_real_data_manifest.py\n",
                    encoding="utf-8",
                )

            self.assertEqual(verify_bioconda_recipes.validate_release_evidence(root), [])

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
            (root / "benchmarks" / "real-data" / "manifest.json").write_text(
                '{"datasets": [{"release_tier": "public_smoke"}]}',
                encoding="utf-8",
            )
            self.assertIn(
                "release evidence manifest has no release_candidate dataset",
                verify_bioconda_recipes.validate_release_evidence(root),
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
