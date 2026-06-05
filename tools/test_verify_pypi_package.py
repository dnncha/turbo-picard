from __future__ import annotations

import importlib.util
from pathlib import Path
import sys
import tempfile
import textwrap
import unittest


MODULE_PATH = Path(__file__).with_name("verify_pypi_package.py")
SPEC = importlib.util.spec_from_file_location("verify_pypi_package", MODULE_PATH)
assert SPEC is not None
verify_pypi_package = importlib.util.module_from_spec(SPEC)
sys.modules["verify_pypi_package"] = verify_pypi_package
assert SPEC.loader is not None
SPEC.loader.exec_module(verify_pypi_package)


class VerifyPyPiPackageTests(unittest.TestCase):
    def write_minimal_repo(self, root: Path) -> None:
        (root / "crates" / "turbo-picard-cli").mkdir(parents=True)
        (root / "docs").mkdir()
        (root / "Cargo.toml").write_text(
            textwrap.dedent(
                """
                [workspace.package]
                version = "0.1.1"
                """
            ).strip()
            + "\n",
            encoding="utf-8",
        )
        (root / "crates" / "turbo-picard-cli" / "Cargo.toml").write_text(
            textwrap.dedent(
                """
                [[bin]]
                name = "turbo-picard"

                [[bin]]
                name = "picard"
                """
            ).strip()
            + "\n",
            encoding="utf-8",
        )
        (root / "pyproject.toml").write_text(
            textwrap.dedent(
                """
                [build-system]
                requires = ["maturin>=1.8,<2"]
                build-backend = "maturin"

                [project]
                name = "turbo-picard"
                version = "0.1.1"
                readme = "README.md"
                requires-python = ">=3.8"
                license = { text = "MIT" }
                authors = [{ name = "Donncha O'Toole" }]
                keywords = ["bioinformatics", "genomics", "Picard", "SAM", "BAM", "CRAM", "VCF", "Rust"]

                [project.urls]
                Documentation = "https://turbo-picard.readthedocs.io/"
                Source = "https://github.com/dnncha/turbo-picard"
                Issues = "https://github.com/dnncha/turbo-picard/issues"

                [tool.maturin]
                manifest-path = "crates/turbo-picard-cli/Cargo.toml"
                bindings = "bin"
                strip = true
                """
            ).strip()
            + "\n",
            encoding="utf-8",
        )
        (root / "README.md").write_text(
            "python3 -m pip install turbo-picard\n"
            "Installing from PyPI currently gives you both commands.\n",
            encoding="utf-8",
        )
        (root / "docs" / "packaging.rst").write_text(
            "PyPI\npython3 -m maturin build --release --compatibility pypi --out dist\n"
            "python3 -m twine check dist/*\nTrusted Publishing\n"
            ".github/workflows/publish-pypi.yml\npicard\n",
            encoding="utf-8",
        )
        (root / ".github" / "workflows").mkdir(parents=True)
        (root / ".github" / "workflows" / "publish-pypi.yml").write_text(
            "release:\nworkflow_dispatch:\nPyO3/maturin-action@v1\n"
            "--compatibility pypi\npypa/gh-action-pypi-publish@release/v1\n"
            "id-token: write\nenvironment: pypi\n",
            encoding="utf-8",
        )

    def test_valid_repo_passes(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.write_minimal_repo(root)
            self.assertEqual(verify_pypi_package.collect_errors(root), [])

    def test_version_mismatch_is_reported(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.write_minimal_repo(root)
            (root / "pyproject.toml").write_text(
                (root / "pyproject.toml")
                .read_text(encoding="utf-8")
                .replace('version = "0.1.1"', 'version = "0.2.0"', 1),
                encoding="utf-8",
            )
            self.assertIn(
                "pyproject.toml project.version must be 0.1.1",
                verify_pypi_package.collect_errors(root),
            )

    def test_missing_docs_are_reported(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.write_minimal_repo(root)
            (root / "docs" / "packaging.rst").write_text("PyPI\n", encoding="utf-8")
            self.assertIn(
                "packaging docs missing maturin build command",
                verify_pypi_package.collect_errors(root),
            )


if __name__ == "__main__":
    unittest.main()
