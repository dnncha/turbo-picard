#!/usr/bin/env python3
"""Tests for Bioconda release source preparation."""

from __future__ import annotations

import importlib.util
import contextlib
import io
import json
import sys
import tarfile
import tempfile
import unittest
from pathlib import Path
from unittest import mock


MODULE_PATH = Path(__file__).with_name("prepare_bioconda_release.py")
SPEC = importlib.util.spec_from_file_location("prepare_bioconda_release", MODULE_PATH)
assert SPEC is not None
prepare_bioconda_release = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules["prepare_bioconda_release"] = prepare_bioconda_release
SPEC.loader.exec_module(prepare_bioconda_release)

REAL_DATA_MODULE_PATH = Path(__file__).with_name("verify_real_data_evidence.py")
REAL_DATA_SPEC = importlib.util.spec_from_file_location(
    "verify_real_data_evidence",
    REAL_DATA_MODULE_PATH,
)
assert REAL_DATA_SPEC is not None
verify_real_data_evidence = importlib.util.module_from_spec(REAL_DATA_SPEC)
assert REAL_DATA_SPEC.loader is not None
sys.modules["verify_real_data_evidence"] = verify_real_data_evidence
REAL_DATA_SPEC.loader.exec_module(verify_real_data_evidence)


def cargo_lock_bytes(version: str = "0.1.0") -> bytes:
    return (
        b"version = 4\n\n"
        b"[[package]]\n"
        b'name = "turbo-picard-cli"\n'
        + f'version = "{version}"\n\n'.encode("utf-8")
        + b"[[package]]\n"
        b'name = "turbo-picard-core"\n'
        + f'version = "{version}"\n\n'.encode("utf-8")
        + b"[[package]]\n"
        b'name = "turbo-picard-markdup"\n'
        + f'version = "{version}"\n'.encode("utf-8")
    )


def cargo_lock_text(version: str = "0.1.0") -> str:
    return cargo_lock_bytes(version).decode("utf-8")


def citation_text(version: str = "0.1.0", *, include_type: bool = True) -> str:
    type_line = "type: software\n" if include_type else ""
    return (
        "cff-version: 1.2.0\n"
        f"{type_line}"
        'message: "Cite the archived release and Picard parity evidence."\n'
        f'version: "{version}"\n'
        'repository-code: "https://github.com/dnncha/turbo-picard"\n'
        "authors:\n"
        '  - name: "turbo-picard contributors"\n'
        "keywords:\n"
        "  - bioinformatics\n"
        "  - genomics\n"
        "  - Picard\n"
        "  - SAM\n"
        "  - BAM\n"
        "  - VCF\n"
        "  - Rust\n"
    )


def citation_bytes(version: str = "0.1.0", *, include_type: bool = True) -> bytes:
    return citation_text(version, include_type=include_type).encode("utf-8")


PARITY_DOCS_BYTES = (
    b"What Parity Means\n"
    b"specific command\n"
    b"specific input shape\n"
    b"comparison method\n"
    b"does not mean every Picard behavior\n"
    b"does not prove broad replacement safety\n"
    b"representative inputs\n"
    b"input SHA-256\n"
    b"Picard version\n"
    b"turbo-picard version\n"
    b"tools/compare_real_data.py\n"
    b"python3 tools/verify_real_data_evidence.py --release-ready\n"
)


class PrepareBiocondaReleaseTests(unittest.TestCase):
    def test_release_archive_evidence_policy_matches_real_data_verifier(self) -> None:
        self.assertEqual(
            prepare_bioconda_release.RELEASE_CANDIDATE_PORTFOLIO_REQUIRED_COMMANDS,
            verify_real_data_evidence.RELEASE_CANDIDATE_PORTFOLIO_REQUIRED_COMMANDS,
        )
        self.assertEqual(
            prepare_bioconda_release.KNOWN_COMPARISONS,
            verify_real_data_evidence.KNOWN_COMPARISONS,
        )

    def write_recipe_tree(
        self,
        root: Path,
        *,
        source_block: str = "  path: ../../..",
        blank_after_source: bool = True,
        versions: tuple[str, str] = ("0.1.0", "0.1.0"),
    ) -> list[Path]:
        recipe_dirs = [
            root / "packaging" / "bioconda" / "turbo-picard",
            root / "packaging" / "bioconda" / "turbo-picard-picard-shim",
        ]
        for recipe_dir, version in zip(recipe_dirs, versions):
            recipe_dir.mkdir(parents=True)
            source_separator = "\n\n" if blank_after_source else "\n"
            (recipe_dir / "meta.yaml").write_text(
                f"{{% set version = \"{version}\" %}}\n"
                "package:\n"
                "  name: test\n"
                "  version: {{ version }}\n\n"
                "source:\n"
                f"{source_block}"
                f"{source_separator}"
                "build:\n"
                "  number: 0\n",
                encoding="utf-8",
            )
        return recipe_dirs

    def test_replaces_local_source_path_with_tagged_release_source(self) -> None:
        digest = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        meta_yaml = """{% set version = "0.1.0" %}
package:
  name: turbo-picard
  version: {{ version }}

source:
  path: ../../..

build:
  number: 0
"""

        updated = prepare_bioconda_release.update_meta_yaml(meta_yaml, digest)

        self.assertIn(
            "url: https://github.com/dnncha/turbo-picard/archive/refs/tags/v0.1.0.tar.gz",
            updated,
        )
        self.assertIn(f"sha256: {digest}", updated)
        self.assertNotIn("path: ../../..", updated)

    def test_updates_bioconda_pr_release_source(self) -> None:
        digest = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        pr_text = """
- Tagged archive URL:
  `https://github.com/dnncha/turbo-picard/archive/refs/tags/v0.0.9.tar.gz`
- Archive SHA-256:
  `<github-v0.1.0-source-archive-sha256>`
"""

        updated = prepare_bioconda_release.update_bioconda_pr(
            pr_text,
            "0.1.0",
            digest,
        )

        self.assertIn(
            "https://github.com/dnncha/turbo-picard/archive/refs/tags/v0.1.0.tar.gz",
            updated,
        )
        self.assertIn(f"`{digest}`", updated)
        self.assertNotIn("github-v0.1.0-source-archive-sha256", updated)

    def test_bioconda_pr_update_requires_release_source_fields(self) -> None:
        digest = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        with self.assertRaisesRegex(ValueError, "tagged archive URL"):
            prepare_bioconda_release.update_bioconda_pr(
                "Archive SHA-256:\n  `<github-v0.1.0-source-archive-sha256>`\n",
                "0.1.0",
                digest,
            )
        with self.assertRaisesRegex(ValueError, "archive SHA-256 field"):
            prepare_bioconda_release.update_bioconda_pr(
                "https://github.com/dnncha/turbo-picard/archive/refs/tags/v0.1.0.tar.gz\n",
                "0.1.0",
                digest,
            )

    def test_requires_version_declaration(self) -> None:
        with self.assertRaisesRegex(ValueError, "version"):
            prepare_bioconda_release.update_meta_yaml(
                "source:\n  path: ../../..\nbuild:\n  number: 0\n",
                "0" * 64,
            )

    def test_can_compute_sha256_from_archive(self) -> None:
        import hashlib
        import tempfile

        with tempfile.TemporaryDirectory() as tmp:
            archive = Path(tmp) / "v0.1.0.tar.gz"
            archive.write_bytes(b"release archive")

            self.assertEqual(
                prepare_bioconda_release.sha256_file(archive),
                hashlib.sha256(b"release archive").hexdigest(),
            )

    def write_release_archive(self, archive: Path, version: str = "0.1.0") -> None:
        prefix = f"turbo-picard-{version}"
        release_candidate_commands = {
            command: "SAM record digest"
            for command in sorted(
                prepare_bioconda_release.RELEASE_CANDIDATE_PORTFOLIO_REQUIRED_COMMANDS
            )
        }
        manifest = {
            "datasets": [
                {
                    "id": "release-candidate-fixture",
                    "release_tier": "release_candidate",
                    "expected_commands": release_candidate_commands,
                }
            ]
        }
        benchmark_data = {
            "parity": "32/32 PASS",
            "summary": {
                "command_count": 32,
                "parity_pass_count": 32,
                "floor_speedup": 27.31,
                "geometric_mean_speedup": 27.31,
                "top_speedup": 27.31,
            },
            "benchmarks": [
                {
                    "command": f"BenchmarkCommand{index:02d}",
                    "speedup": 27.31,
                    "parity": "PASS",
                }
                for index in range(32)
            ],
        }
        content_by_name = {
            f"{prefix}/Cargo.lock": cargo_lock_bytes(version),
            f"{prefix}/Cargo.toml": (
                b"[workspace.package]\n"
                + f'version = "{version}"\n'.encode("utf-8")
            ),
            f"{prefix}/CITATION.cff": citation_bytes(version),
            f"{prefix}/benchmarks/real-data/manifest.json": (
                json.dumps(manifest).encode("utf-8") + b"\n"
            ),
            f"{prefix}/docs/command-matrix.yml": (
                b'picard_reference: "3.4.0"\ncommands: []\n'
            ),
            f"{prefix}/docs/parity.rst": PARITY_DOCS_BYTES,
            f"{prefix}/docs/site/assets/benchmark-data.json": (
                json.dumps(benchmark_data).encode("utf-8") + b"\n"
            ),
            f"{prefix}/packaging/bioconda/turbo-picard/meta.yaml": (
                f'{{% set version = "{version}" %}}\nsource:\n  path: ../../..\n'.encode("utf-8")
            ),
            f"{prefix}/packaging/bioconda/turbo-picard-picard-shim/meta.yaml": (
                f'{{% set version = "{version}" %}}\nsource:\n  path: ../../..\n'.encode("utf-8")
            ),
        }
        with tarfile.open(archive, "w:gz") as handle:
            for member_name, data in content_by_name.items():
                info = tarfile.TarInfo(member_name)
                info.size = len(data)
                handle.addfile(info, io.BytesIO(data))

    def test_archive_argument_requires_existing_file(self) -> None:
        with contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(
                prepare_bioconda_release.main(["--archive", "/does/not/exist"]),
                2,
            )

    def test_archive_name_must_match_recipe_version(self) -> None:
        self.assertIsNone(
            prepare_bioconda_release.validate_archive_name(
                Path("turbo-picard-0.1.0.tar.gz"),
                {"0.1.0"},
            )
        )
        self.assertIsNone(
            prepare_bioconda_release.validate_archive_name(
                Path("v0.1.0.tar.gz"),
                {"0.1.0"},
            )
        )
        self.assertEqual(
            prepare_bioconda_release.validate_archive_name(
                Path("turbo-picard-0.2.0.tar.gz"),
                {"0.1.0"},
            ),
            "--archive filename must match the recipe version: "
            "turbo-picard-0.1.0.tar.gz or v0.1.0.tar.gz",
        )

    def test_archive_argument_rejects_wrong_release_filename(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            archive = Path(tmp) / "turbo-picard-0.2.0.tar.gz"
            archive.write_bytes(b"release archive")
            with (
                contextlib.redirect_stderr(io.StringIO()) as stderr,
                mock.patch.object(
                    prepare_bioconda_release,
                    "recipe_versions",
                    return_value={"0.1.0"},
                ),
            ):
                status = prepare_bioconda_release.main(["--archive", str(archive)])

        self.assertEqual(status, 2)
        self.assertIn("filename must match the recipe version", stderr.getvalue())

    def test_sha256_check_rejects_mismatched_recipe_versions(self) -> None:
        digest = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            recipe_dirs = self.write_recipe_tree(root, versions=("0.1.0", "0.2.0"))
            bioconda_pr = root / "packaging" / "bioconda" / "BIOCONDA_PR.md"
            bioconda_pr.write_text(
                "- Tagged archive URL:\n"
                "  `https://github.com/dnncha/turbo-picard/archive/refs/tags/v0.1.0.tar.gz`\n"
                "- Archive SHA-256:\n"
                "  `<github-v0.1.0-source-archive-sha256>`\n",
                encoding="utf-8",
            )
            originals = [
                (recipe_dir / "meta.yaml").read_text(encoding="utf-8")
                for recipe_dir in recipe_dirs
            ]
            with (
                contextlib.redirect_stderr(io.StringIO()) as stderr,
                mock.patch.object(prepare_bioconda_release, "ROOT", root),
                mock.patch.object(prepare_bioconda_release, "RECIPE_DIRS", recipe_dirs),
                mock.patch.object(prepare_bioconda_release, "BIOCONDA_PR", bioconda_pr),
            ):
                status = prepare_bioconda_release.main(["--sha256", digest, "--check"])
            updated = [
                (recipe_dir / "meta.yaml").read_text(encoding="utf-8")
                for recipe_dir in recipe_dirs
            ]

        self.assertEqual(status, 2)
        self.assertEqual(updated, originals)
        self.assertIn("Bioconda recipes do not agree on one release version", stderr.getvalue())

    def test_archive_contents_must_match_release_source_shape(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            archive = Path(tmp) / "v0.1.0.tar.gz"
            self.write_release_archive(archive)

            self.assertIsNone(
                prepare_bioconda_release.validate_archive_contents(archive, "0.1.0")
            )

    def test_archive_contents_reject_wrong_top_level_directory(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            archive = Path(tmp) / "v0.1.0.tar.gz"
            self.write_release_archive(archive, version="0.2.0")

            self.assertEqual(
                prepare_bioconda_release.validate_archive_contents(archive, "0.1.0"),
                "--archive does not contain expected top-level directory turbo-picard-0.1.0/",
            )

    def test_archive_contents_reject_unsafe_member_paths(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            archive = Path(tmp) / "v0.1.0.tar.gz"
            with tarfile.open(archive, "w:gz") as handle:
                data = b"fixture"
                for member_name in (
                    "turbo-picard-0.1.0/Cargo.toml",
                    "../outside",
                ):
                    info = tarfile.TarInfo(member_name)
                    info.size = len(data)
                    handle.addfile(info, io.BytesIO(data))

            self.assertEqual(
                prepare_bioconda_release.validate_archive_contents(archive, "0.1.0"),
                "--archive contains unsafe member path: ../outside",
            )

    def test_archive_contents_rejects_duplicate_member_paths(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            archive = Path(tmp) / "v0.1.0.tar.gz"
            prefix = "turbo-picard-0.1.0"
            with tarfile.open(archive, "w:gz") as handle:
                data = b"fixture"
                for member_name in (
                    f"{prefix}/Cargo.toml",
                    f"{prefix}/Cargo.toml",
                    f"{prefix}/Cargo.lock",
                    f"{prefix}/CITATION.cff",
                    f"{prefix}/benchmarks/real-data/manifest.json",
                    f"{prefix}/docs/command-matrix.yml",
                    f"{prefix}/docs/parity.rst",
                    f"{prefix}/docs/site/assets/benchmark-data.json",
                    f"{prefix}/packaging/bioconda/turbo-picard/meta.yaml",
                    f"{prefix}/packaging/bioconda/turbo-picard-picard-shim/meta.yaml",
                ):
                    info = tarfile.TarInfo(member_name)
                    info.size = len(data)
                    handle.addfile(info, io.BytesIO(data))

            self.assertEqual(
                prepare_bioconda_release.validate_archive_contents(archive, "0.1.0"),
                "--archive contains duplicate member path: turbo-picard-0.1.0/Cargo.toml",
            )

    def test_archive_contents_rejects_symlinks(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            archive = Path(tmp) / "v0.1.0.tar.gz"
            prefix = "turbo-picard-0.1.0"
            with tarfile.open(archive, "w:gz") as handle:
                data = b"fixture"
                for member_name in (
                    f"{prefix}/Cargo.toml",
                    f"{prefix}/Cargo.lock",
                    f"{prefix}/CITATION.cff",
                    f"{prefix}/benchmarks/real-data/manifest.json",
                    f"{prefix}/docs/command-matrix.yml",
                    f"{prefix}/docs/parity.rst",
                    f"{prefix}/docs/site/assets/benchmark-data.json",
                    f"{prefix}/packaging/bioconda/turbo-picard-picard-shim/meta.yaml",
                ):
                    info = tarfile.TarInfo(member_name)
                    info.size = len(data)
                    handle.addfile(info, io.BytesIO(data))
                link = tarfile.TarInfo(f"{prefix}/packaging/bioconda/turbo-picard/meta.yaml")
                link.type = tarfile.SYMTYPE
                link.linkname = "../meta.yaml"
                handle.addfile(link)

            self.assertEqual(
                prepare_bioconda_release.validate_archive_contents(archive, "0.1.0"),
                "--archive contains unsupported member type: "
                "turbo-picard-0.1.0/packaging/bioconda/turbo-picard/meta.yaml",
            )

    def test_archive_contents_rejects_empty_required_files(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            archive = Path(tmp) / "v0.1.0.tar.gz"
            prefix = "turbo-picard-0.1.0"
            with tarfile.open(archive, "w:gz") as handle:
                for member_name, data in (
                    (f"{prefix}/Cargo.toml", b""),
                    (f"{prefix}/Cargo.lock", b"fixture"),
                    (f"{prefix}/CITATION.cff", b"fixture"),
                    (f"{prefix}/benchmarks/real-data/manifest.json", b"fixture"),
                    (f"{prefix}/docs/command-matrix.yml", b"fixture"),
                    (f"{prefix}/docs/parity.rst", b"fixture"),
                    (f"{prefix}/docs/site/assets/benchmark-data.json", b"fixture"),
                    (f"{prefix}/packaging/bioconda/turbo-picard/meta.yaml", b"fixture"),
                    (
                        f"{prefix}/packaging/bioconda/turbo-picard-picard-shim/meta.yaml",
                        b"fixture",
                    ),
                ):
                    info = tarfile.TarInfo(member_name)
                    info.size = len(data)
                    handle.addfile(info, io.BytesIO(data))

            self.assertEqual(
                prepare_bioconda_release.validate_archive_contents(archive, "0.1.0"),
                "--archive expected non-empty release source file: "
                "turbo-picard-0.1.0/Cargo.toml",
            )

    def test_archive_contents_reject_extra_top_level_entries(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            archive = Path(tmp) / "v0.1.0.tar.gz"
            prefix = "turbo-picard-0.1.0"
            with tarfile.open(archive, "w:gz") as handle:
                data = b"fixture"
                for member_name in (
                    f"{prefix}/Cargo.toml",
                    f"{prefix}/Cargo.lock",
                    f"{prefix}/CITATION.cff",
                    f"{prefix}/benchmarks/real-data/manifest.json",
                    f"{prefix}/docs/command-matrix.yml",
                    f"{prefix}/docs/parity.rst",
                    f"{prefix}/docs/site/assets/benchmark-data.json",
                    f"{prefix}/packaging/bioconda/turbo-picard/meta.yaml",
                    f"{prefix}/packaging/bioconda/turbo-picard-picard-shim/meta.yaml",
                    "extra-file",
                ):
                    info = tarfile.TarInfo(member_name)
                    info.size = len(data)
                    handle.addfile(info, io.BytesIO(data))

            self.assertEqual(
                prepare_bioconda_release.validate_archive_contents(archive, "0.1.0"),
                "--archive contains unexpected top-level entries outside turbo-picard-0.1.0/: extra-file/",
            )

    def test_archive_contents_requires_citation_metadata(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            archive = Path(tmp) / "v0.1.0.tar.gz"
            prefix = "turbo-picard-0.1.0"
            with tarfile.open(archive, "w:gz") as handle:
                data = b"fixture"
                for member_name in (
                    f"{prefix}/Cargo.toml",
                    f"{prefix}/Cargo.lock",
                    f"{prefix}/benchmarks/real-data/manifest.json",
                    f"{prefix}/docs/command-matrix.yml",
                    f"{prefix}/docs/parity.rst",
                    f"{prefix}/docs/site/assets/benchmark-data.json",
                    f"{prefix}/packaging/bioconda/turbo-picard/meta.yaml",
                    f"{prefix}/packaging/bioconda/turbo-picard-picard-shim/meta.yaml",
                ):
                    info = tarfile.TarInfo(member_name)
                    info.size = len(data)
                    handle.addfile(info, io.BytesIO(data))

            self.assertEqual(
                prepare_bioconda_release.validate_archive_contents(archive, "0.1.0"),
                "--archive is missing expected release source files: "
                "turbo-picard-0.1.0/CITATION.cff",
            )

    def test_archive_contents_requires_claim_evidence_files(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            archive = Path(tmp) / "v0.1.0.tar.gz"
            prefix = "turbo-picard-0.1.0"
            with tarfile.open(archive, "w:gz") as handle:
                data = b"fixture"
                for member_name in (
                    f"{prefix}/Cargo.toml",
                    f"{prefix}/Cargo.lock",
                    f"{prefix}/CITATION.cff",
                    f"{prefix}/packaging/bioconda/turbo-picard/meta.yaml",
                    f"{prefix}/packaging/bioconda/turbo-picard-picard-shim/meta.yaml",
                ):
                    info = tarfile.TarInfo(member_name)
                    info.size = len(data)
                    handle.addfile(info, io.BytesIO(data))

            self.assertEqual(
                prepare_bioconda_release.validate_archive_contents(archive, "0.1.0"),
                "--archive is missing expected release source files: "
                "turbo-picard-0.1.0/benchmarks/real-data/manifest.json, "
                "turbo-picard-0.1.0/docs/command-matrix.yml, "
                "turbo-picard-0.1.0/docs/parity.rst, "
                "turbo-picard-0.1.0/docs/site/assets/benchmark-data.json",
            )

    def test_archive_contents_rejects_stale_internal_release_metadata(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            archive = Path(tmp) / "v0.1.0.tar.gz"
            prefix = "turbo-picard-0.1.0"
            with tarfile.open(archive, "w:gz") as handle:
                content_by_name = {
                    f"{prefix}/Cargo.lock": cargo_lock_bytes(),
                    f"{prefix}/Cargo.toml": b'[workspace.package]\nversion = "0.2.0"\n',
                    f"{prefix}/CITATION.cff": citation_bytes(),
                    f"{prefix}/benchmarks/real-data/manifest.json": b'{"datasets": []}\n',
                    f"{prefix}/docs/command-matrix.yml": b'picard_reference: "3.4.0"\ncommands: []\n',
                    f"{prefix}/docs/parity.rst": PARITY_DOCS_BYTES,
                    f"{prefix}/docs/site/assets/benchmark-data.json": b'{"benchmarks": []}\n',
                    f"{prefix}/packaging/bioconda/turbo-picard/meta.yaml": (
                        b'{% set version = "0.1.0" %}\nsource:\n  path: ../../..\n'
                    ),
                    f"{prefix}/packaging/bioconda/turbo-picard-picard-shim/meta.yaml": (
                        b'{% set version = "0.1.0" %}\nsource:\n  path: ../../..\n'
                    ),
                }
                for member_name, data in content_by_name.items():
                    info = tarfile.TarInfo(member_name)
                    info.size = len(data)
                    handle.addfile(info, io.BytesIO(data))

            self.assertEqual(
                prepare_bioconda_release.validate_archive_contents(archive, "0.1.0"),
                "--archive Cargo.toml workspace version must be 0.1.0",
            )

    def test_archive_contents_requires_lockfile(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            archive = Path(tmp) / "v0.1.0.tar.gz"
            prefix = "turbo-picard-0.1.0"
            with tarfile.open(archive, "w:gz") as handle:
                data = b"fixture"
                for member_name in (
                    f"{prefix}/Cargo.toml",
                    f"{prefix}/CITATION.cff",
                    f"{prefix}/benchmarks/real-data/manifest.json",
                    f"{prefix}/docs/command-matrix.yml",
                    f"{prefix}/docs/parity.rst",
                    f"{prefix}/docs/site/assets/benchmark-data.json",
                    f"{prefix}/packaging/bioconda/turbo-picard/meta.yaml",
                    f"{prefix}/packaging/bioconda/turbo-picard-picard-shim/meta.yaml",
                ):
                    info = tarfile.TarInfo(member_name)
                    info.size = len(data)
                    handle.addfile(info, io.BytesIO(data))

            self.assertEqual(
                prepare_bioconda_release.validate_archive_contents(archive, "0.1.0"),
                "--archive is missing expected release source files: "
                "turbo-picard-0.1.0/Cargo.lock",
            )

    def test_archive_contents_rejects_stale_lockfile_crate_version(self) -> None:
        contents = {
            "turbo-picard-0.1.0/Cargo.lock": cargo_lock_text("0.2.0"),
            "turbo-picard-0.1.0/Cargo.toml": '[workspace.package]\nversion = "0.1.0"\n',
            "turbo-picard-0.1.0/CITATION.cff": citation_text(),
            "turbo-picard-0.1.0/docs/command-matrix.yml": (
                'picard_reference: "3.4.0"\ncommands: []\n'
            ),
            "turbo-picard-0.1.0/benchmarks/real-data/manifest.json": (
                '{"datasets": []}'
            ),
            "turbo-picard-0.1.0/docs/site/assets/benchmark-data.json": (
                '{"benchmarks": []}'
            ),
            "turbo-picard-0.1.0/packaging/bioconda/turbo-picard/meta.yaml": (
                '{% set version = "0.1.0" %}\nsource:\n  path: ../../..\n'
            ),
            "turbo-picard-0.1.0/packaging/bioconda/turbo-picard-picard-shim/meta.yaml": (
                '{% set version = "0.1.0" %}\nsource:\n  path: ../../..\n'
            ),
        }

        self.assertEqual(
            prepare_bioconda_release.validate_archive_release_metadata(
                contents,
                "turbo-picard-0.1.0/",
                "0.1.0",
            ),
            "--archive Cargo.lock turbo-picard-cli version must be 0.1.0",
        )

    def test_archive_contents_rejects_missing_lockfile_crate(self) -> None:
        contents = {
            "turbo-picard-0.1.0/Cargo.lock": (
                'version = 4\n\n'
                '[[package]]\nname = "turbo-picard-cli"\nversion = "0.1.0"\n\n'
                '[[package]]\nname = "turbo-picard-core"\nversion = "0.1.0"\n'
            ),
            "turbo-picard-0.1.0/Cargo.toml": '[workspace.package]\nversion = "0.1.0"\n',
            "turbo-picard-0.1.0/CITATION.cff": citation_text(),
            "turbo-picard-0.1.0/docs/command-matrix.yml": (
                'picard_reference: "3.4.0"\ncommands: []\n'
            ),
            "turbo-picard-0.1.0/benchmarks/real-data/manifest.json": (
                '{"datasets": []}'
            ),
            "turbo-picard-0.1.0/docs/site/assets/benchmark-data.json": (
                '{"benchmarks": []}'
            ),
            "turbo-picard-0.1.0/packaging/bioconda/turbo-picard/meta.yaml": (
                '{% set version = "0.1.0" %}\nsource:\n  path: ../../..\n'
            ),
            "turbo-picard-0.1.0/packaging/bioconda/turbo-picard-picard-shim/meta.yaml": (
                '{% set version = "0.1.0" %}\nsource:\n  path: ../../..\n'
            ),
        }

        self.assertEqual(
            prepare_bioconda_release.validate_archive_release_metadata(
                contents,
                "turbo-picard-0.1.0/",
                "0.1.0",
            ),
            "--archive Cargo.lock missing turbo-picard-markdup",
        )

    def test_archive_contents_requires_citation_software_type(self) -> None:
        contents = {
            "turbo-picard-0.1.0/Cargo.lock": cargo_lock_text(),
            "turbo-picard-0.1.0/Cargo.toml": '[workspace.package]\nversion = "0.1.0"\n',
            "turbo-picard-0.1.0/CITATION.cff": (
                "cff-version: 1.2.0\n"
                'message: "Cite the archived release and Picard parity evidence."\n'
                'version: "0.1.0"\n'
                'repository-code: "https://github.com/dnncha/turbo-picard"\n'
                "authors:\n"
                '  - name: "turbo-picard contributors"\n'
                "keywords:\n"
                "  - bioinformatics\n"
                "  - genomics\n"
                "  - Picard\n"
                "  - SAM\n"
                "  - BAM\n"
                "  - VCF\n"
                "  - Rust\n"
            ),
            "turbo-picard-0.1.0/docs/command-matrix.yml": (
                'picard_reference: "3.4.0"\ncommands: []\n'
            ),
            "turbo-picard-0.1.0/benchmarks/real-data/manifest.json": (
                '{"datasets": []}'
            ),
            "turbo-picard-0.1.0/docs/site/assets/benchmark-data.json": (
                '{"benchmarks": []}'
            ),
            "turbo-picard-0.1.0/packaging/bioconda/turbo-picard/meta.yaml": (
                '{% set version = "0.1.0" %}\nsource:\n  path: ../../..\n'
            ),
            "turbo-picard-0.1.0/packaging/bioconda/turbo-picard-picard-shim/meta.yaml": (
                '{% set version = "0.1.0" %}\nsource:\n  path: ../../..\n'
            ),
        }

        self.assertEqual(
            prepare_bioconda_release.validate_archive_release_metadata(
                contents,
                "turbo-picard-0.1.0/",
                "0.1.0",
            ),
            "--archive CITATION.cff software type must match release metadata",
        )

    def test_archive_contents_rejects_unparseable_citation_yaml(self) -> None:
        contents = {
            "turbo-picard-0.1.0/Cargo.lock": cargo_lock_text(),
            "turbo-picard-0.1.0/Cargo.toml": '[workspace.package]\nversion = "0.1.0"\n',
            "turbo-picard-0.1.0/CITATION.cff": (
                "cff-version: 1.2.0\n"
                'message: "Cite the archived release and Picard parity evidence."\n'
                "type: software\n"
                'version: "0.1.0"\n'
                "authors: [\n"
                'repository-code: "https://github.com/dnncha/turbo-picard"\n'
            ),
            "turbo-picard-0.1.0/docs/command-matrix.yml": (
                'picard_reference: "3.4.0"\ncommands: []\n'
            ),
            "turbo-picard-0.1.0/benchmarks/real-data/manifest.json": (
                '{"datasets": []}'
            ),
            "turbo-picard-0.1.0/docs/site/assets/benchmark-data.json": (
                '{"benchmarks": []}'
            ),
            "turbo-picard-0.1.0/packaging/bioconda/turbo-picard/meta.yaml": (
                '{% set version = "0.1.0" %}\nsource:\n  path: ../../..\n'
            ),
            "turbo-picard-0.1.0/packaging/bioconda/turbo-picard-picard-shim/meta.yaml": (
                '{% set version = "0.1.0" %}\nsource:\n  path: ../../..\n'
            ),
        }

        error = prepare_bioconda_release.validate_archive_release_metadata(
            contents,
            "turbo-picard-0.1.0/",
            "0.1.0",
        )
        self.assertIsNotNone(error)
        assert error is not None
        self.assertTrue(error.startswith("--archive CITATION.cff is not valid YAML:"))

    def test_archive_contents_requires_structured_citation_metadata(self) -> None:
        contents = {
            "turbo-picard-0.1.0/Cargo.lock": cargo_lock_text(),
            "turbo-picard-0.1.0/Cargo.toml": '[workspace.package]\nversion = "0.1.0"\n',
            "turbo-picard-0.1.0/CITATION.cff": (
                "cff-version: 1.2.0\n"
                'message: "Cite the archived release and Picard parity evidence."\n'
                "type: software\n"
                'version: "0.1.0"\n'
                'repository-code: "https://github.com/dnncha/turbo-picard"\n'
                'authors: "turbo-picard contributors"\n'
                "keywords:\n"
                "  - genomics\n"
            ),
            "turbo-picard-0.1.0/docs/command-matrix.yml": (
                'picard_reference: "3.4.0"\ncommands: []\n'
            ),
            "turbo-picard-0.1.0/benchmarks/real-data/manifest.json": (
                '{"datasets": []}'
            ),
            "turbo-picard-0.1.0/docs/site/assets/benchmark-data.json": (
                '{"benchmarks": []}'
            ),
            "turbo-picard-0.1.0/packaging/bioconda/turbo-picard/meta.yaml": (
                '{% set version = "0.1.0" %}\nsource:\n  path: ../../..\n'
            ),
            "turbo-picard-0.1.0/packaging/bioconda/turbo-picard-picard-shim/meta.yaml": (
                '{% set version = "0.1.0" %}\nsource:\n  path: ../../..\n'
            ),
        }

        self.assertEqual(
            prepare_bioconda_release.validate_archive_release_metadata(
                contents,
                "turbo-picard-0.1.0/",
                "0.1.0",
            ),
            "--archive CITATION.cff authors must be a non-empty list",
        )

    def test_archive_contents_rejects_invalid_evidence_json(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            archive = Path(tmp) / "v0.1.0.tar.gz"
            prefix = "turbo-picard-0.1.0"
            with tarfile.open(archive, "w:gz") as handle:
                content_by_name = {
                    f"{prefix}/Cargo.lock": cargo_lock_bytes(),
                    f"{prefix}/Cargo.toml": b'[workspace.package]\nversion = "0.1.0"\n',
                    f"{prefix}/CITATION.cff": citation_bytes(),
                    f"{prefix}/benchmarks/real-data/manifest.json": b'{"not_datasets": []}\n',
                    f"{prefix}/docs/command-matrix.yml": b'picard_reference: "3.4.0"\ncommands: []\n',
                    f"{prefix}/docs/parity.rst": PARITY_DOCS_BYTES,
                    f"{prefix}/docs/site/assets/benchmark-data.json": b'{"benchmarks": []}\n',
                    f"{prefix}/packaging/bioconda/turbo-picard/meta.yaml": (
                        b'{% set version = "0.1.0" %}\nsource:\n  path: ../../..\n'
                    ),
                    f"{prefix}/packaging/bioconda/turbo-picard-picard-shim/meta.yaml": (
                        b'{% set version = "0.1.0" %}\nsource:\n  path: ../../..\n'
                    ),
                }
                for member_name, data in content_by_name.items():
                    info = tarfile.TarInfo(member_name)
                    info.size = len(data)
                    handle.addfile(info, io.BytesIO(data))

            self.assertEqual(
                prepare_bioconda_release.validate_archive_contents(archive, "0.1.0"),
                "--archive real-data manifest must contain datasets list",
            )

    def test_archive_contents_requires_release_candidate_portfolio(self) -> None:
        contents = {
            "turbo-picard-0.1.0/Cargo.lock": cargo_lock_text(),
            "turbo-picard-0.1.0/Cargo.toml": '[workspace.package]\nversion = "0.1.0"\n',
            "turbo-picard-0.1.0/CITATION.cff": citation_text(),
            "turbo-picard-0.1.0/docs/command-matrix.yml": (
                'picard_reference: "3.4.0"\ncommands: []\n'
            ),
            "turbo-picard-0.1.0/benchmarks/real-data/manifest.json": (
                '{"datasets": [{"release_tier": "release_candidate", '
                '"expected_commands": {"ViewSam": "SAM record digest"}}]}'
            ),
            "turbo-picard-0.1.0/docs/site/assets/benchmark-data.json": (
                '{"parity": "32/32 PASS", "summary": {"floor_speedup": 7.4, '
                '"geometric_mean_speedup": 27.31, "top_speedup": 112.07}, '
                '"benchmarks": []}'
            ),
            "turbo-picard-0.1.0/packaging/bioconda/turbo-picard/meta.yaml": (
                '{% set version = "0.1.0" %}\nsource:\n  path: ../../..\n'
            ),
            "turbo-picard-0.1.0/packaging/bioconda/turbo-picard-picard-shim/meta.yaml": (
                '{% set version = "0.1.0" %}\nsource:\n  path: ../../..\n'
            ),
        }

        self.assertEqual(
            prepare_bioconda_release.validate_archive_release_metadata(
                contents,
                "turbo-picard-0.1.0/",
                "0.1.0",
            ),
            "--archive real-data manifest release_candidate portfolio missing commands: "
            "AddOrReplaceReadGroups, BuildBamIndex, CleanSam, "
            "CollectAlignmentSummaryMetrics, CollectInsertSizeMetrics, "
            "CollectQualityYieldMetrics, MarkDuplicates, RevertSam, SamToFastq, "
            "SortSam, ValidateSamFile",
        )

    def test_archive_contents_rejects_unknown_real_data_comparison_labels(self) -> None:
        contents = {
            "turbo-picard-0.1.0/Cargo.lock": cargo_lock_text(),
            "turbo-picard-0.1.0/Cargo.toml": '[workspace.package]\nversion = "0.1.0"\n',
            "turbo-picard-0.1.0/CITATION.cff": citation_text(),
            "turbo-picard-0.1.0/docs/command-matrix.yml": (
                'picard_reference: "3.4.0"\ncommands: []\n'
            ),
            "turbo-picard-0.1.0/benchmarks/real-data/manifest.json": json.dumps(
                {
                    "datasets": [
                        {
                            "release_tier": "release_candidate",
                            "expected_commands": {
                                command: "SAM record digest"
                                for command in prepare_bioconda_release.RELEASE_CANDIDATE_PORTFOLIO_REQUIRED_COMMANDS
                            }
                            | {"ViewSam": "looks similar enough"},
                        }
                    ]
                }
            ),
            "turbo-picard-0.1.0/docs/site/assets/benchmark-data.json": (
                '{"parity": "32/32 PASS", "summary": {"floor_speedup": 7.4, '
                '"geometric_mean_speedup": 27.31, "top_speedup": 112.07}, '
                '"benchmarks": []}'
            ),
            "turbo-picard-0.1.0/packaging/bioconda/turbo-picard/meta.yaml": (
                '{% set version = "0.1.0" %}\nsource:\n  path: ../../..\n'
            ),
            "turbo-picard-0.1.0/packaging/bioconda/turbo-picard-picard-shim/meta.yaml": (
                '{% set version = "0.1.0" %}\nsource:\n  path: ../../..\n'
            ),
        }

        self.assertEqual(
            prepare_bioconda_release.validate_archive_release_metadata(
                contents,
                "turbo-picard-0.1.0/",
                "0.1.0",
            ),
            "--archive real-data manifest has unknown comparison labels: looks similar enough",
        )

    def test_archive_contents_requires_public_benchmark_summary(self) -> None:
        contents = {
            "turbo-picard-0.1.0/Cargo.lock": cargo_lock_text(),
            "turbo-picard-0.1.0/Cargo.toml": '[workspace.package]\nversion = "0.1.0"\n',
            "turbo-picard-0.1.0/CITATION.cff": citation_text(),
            "turbo-picard-0.1.0/docs/command-matrix.yml": (
                'picard_reference: "3.4.0"\ncommands: []\n'
            ),
            "turbo-picard-0.1.0/benchmarks/real-data/manifest.json": json.dumps(
                {
                    "datasets": [
                        {
                            "release_tier": "release_candidate",
                            "expected_commands": {
                                command: "SAM record digest"
                                for command in prepare_bioconda_release.RELEASE_CANDIDATE_PORTFOLIO_REQUIRED_COMMANDS
                            },
                        }
                    ]
                }
            ),
            "turbo-picard-0.1.0/docs/site/assets/benchmark-data.json": (
                '{"parity": "31/32 PASS", "summary": {}, "benchmarks": []}'
            ),
            "turbo-picard-0.1.0/packaging/bioconda/turbo-picard/meta.yaml": (
                '{% set version = "0.1.0" %}\nsource:\n  path: ../../..\n'
            ),
            "turbo-picard-0.1.0/packaging/bioconda/turbo-picard-picard-shim/meta.yaml": (
                '{% set version = "0.1.0" %}\nsource:\n  path: ../../..\n'
            ),
        }

        self.assertEqual(
            prepare_bioconda_release.validate_archive_release_metadata(
                contents,
                "turbo-picard-0.1.0/",
                "0.1.0",
            ),
            "--archive benchmark-data.json must report 32/32 PASS parity",
        )

    def test_archive_contents_rejects_stale_public_benchmark_summary(self) -> None:
        contents = {
            "turbo-picard-0.1.0/Cargo.lock": cargo_lock_text(),
            "turbo-picard-0.1.0/Cargo.toml": '[workspace.package]\nversion = "0.1.0"\n',
            "turbo-picard-0.1.0/CITATION.cff": citation_text(),
            "turbo-picard-0.1.0/docs/command-matrix.yml": (
                'picard_reference: "3.4.0"\ncommands: []\n'
            ),
            "turbo-picard-0.1.0/benchmarks/real-data/manifest.json": json.dumps(
                {
                    "datasets": [
                        {
                            "release_tier": "release_candidate",
                            "expected_commands": {
                                command: "SAM record digest"
                                for command in prepare_bioconda_release.RELEASE_CANDIDATE_PORTFOLIO_REQUIRED_COMMANDS
                            },
                        }
                    ]
                }
            ),
            "turbo-picard-0.1.0/docs/site/assets/benchmark-data.json": json.dumps(
                {
                    "parity": "32/32 PASS",
                    "summary": {
                        "command_count": 32,
                        "parity_pass_count": 32,
                        "floor_speedup": 7.4,
                        "geometric_mean_speedup": 27.31,
                        "top_speedup": 112.07,
                    },
                    "benchmarks": [
                        {"command": "ViewSam", "parity": "PASS", "speedup": 15.2}
                    ],
                }
            ),
            "turbo-picard-0.1.0/packaging/bioconda/turbo-picard/meta.yaml": (
                '{% set version = "0.1.0" %}\nsource:\n  path: ../../..\n'
            ),
            "turbo-picard-0.1.0/packaging/bioconda/turbo-picard-picard-shim/meta.yaml": (
                '{% set version = "0.1.0" %}\nsource:\n  path: ../../..\n'
            ),
        }

        self.assertEqual(
            prepare_bioconda_release.validate_archive_release_metadata(
                contents,
                "turbo-picard-0.1.0/",
                "0.1.0",
            ),
            "--archive benchmark-data.json summary command_count does not match benchmark rows",
        )

    def test_archive_argument_rejects_wrong_release_contents(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            archive = Path(tmp) / "v0.1.0.tar.gz"
            archive.write_bytes(b"not a gzip tarball")
            with (
                contextlib.redirect_stderr(io.StringIO()) as stderr,
                mock.patch.object(
                    prepare_bioconda_release,
                    "recipe_versions",
                    return_value={"0.1.0"},
                ),
            ):
                status = prepare_bioconda_release.main(["--archive", str(archive)])

        self.assertEqual(status, 2)
        self.assertIn("not a readable gzip tar archive", stderr.getvalue())

    def test_check_reports_changes_without_mutating_recipes(self) -> None:
        digest = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            recipe_dirs = self.write_recipe_tree(root)
            bioconda_pr = root / "packaging" / "bioconda" / "BIOCONDA_PR.md"
            bioconda_pr.write_text(
                "- Tagged archive URL:\n"
                "  `https://github.com/dnncha/turbo-picard/archive/refs/tags/v0.1.0.tar.gz`\n"
                "- Archive SHA-256:\n"
                "  `<github-v0.1.0-source-archive-sha256>`\n",
                encoding="utf-8",
            )
            originals = [
                (recipe_dir / "meta.yaml").read_text(encoding="utf-8")
                for recipe_dir in recipe_dirs
            ]
            original_pr = bioconda_pr.read_text(encoding="utf-8")
            with (
                contextlib.redirect_stderr(io.StringIO()) as stderr,
                mock.patch.object(prepare_bioconda_release, "ROOT", root),
                mock.patch.object(prepare_bioconda_release, "RECIPE_DIRS", recipe_dirs),
                mock.patch.object(prepare_bioconda_release, "BIOCONDA_PR", bioconda_pr),
            ):
                status = prepare_bioconda_release.main(["--sha256", digest, "--check"])

            updated = [
                (recipe_dir / "meta.yaml").read_text(encoding="utf-8")
                for recipe_dir in recipe_dirs
            ]
            updated_pr = bioconda_pr.read_text(encoding="utf-8")

        self.assertEqual(status, 1)
        self.assertEqual(updated, originals)
        self.assertEqual(updated_pr, original_pr)
        self.assertIn("would update packaging/bioconda/turbo-picard/meta.yaml", stderr.getvalue())
        self.assertIn(
            "would update packaging/bioconda/turbo-picard-picard-shim/meta.yaml",
            stderr.getvalue(),
        )
        self.assertIn("would update packaging/bioconda/BIOCONDA_PR.md", stderr.getvalue())

    def test_check_passes_when_recipes_already_match_release_source(self) -> None:
        digest = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        source_block = (
            "  url: https://github.com/dnncha/turbo-picard/archive/refs/tags/v0.1.0.tar.gz\n"
            f"  sha256: {digest}"
        )
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            recipe_dirs = self.write_recipe_tree(
                root,
                source_block=source_block,
                blank_after_source=False,
            )
            bioconda_pr = root / "packaging" / "bioconda" / "BIOCONDA_PR.md"
            bioconda_pr.write_text(
                "- Tagged archive URL:\n"
                "  `https://github.com/dnncha/turbo-picard/archive/refs/tags/v0.1.0.tar.gz`\n"
                "- Archive SHA-256:\n"
                f"  `{digest}`\n",
                encoding="utf-8",
            )
            with (
                contextlib.redirect_stdout(io.StringIO()) as stdout,
                contextlib.redirect_stderr(io.StringIO()) as stderr,
                mock.patch.object(prepare_bioconda_release, "ROOT", root),
                mock.patch.object(prepare_bioconda_release, "RECIPE_DIRS", recipe_dirs),
                mock.patch.object(prepare_bioconda_release, "BIOCONDA_PR", bioconda_pr),
            ):
                status = prepare_bioconda_release.main(["--sha256", digest, "--check"])

        self.assertEqual(status, 0)
        self.assertEqual(stdout.getvalue(), "")
        self.assertEqual(stderr.getvalue(), "")

    def test_updates_recipes_and_bioconda_pr(self) -> None:
        digest = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            recipe_dirs = self.write_recipe_tree(root)
            bioconda_pr = root / "packaging" / "bioconda" / "BIOCONDA_PR.md"
            bioconda_pr.write_text(
                "- Tagged archive URL:\n"
                "  `https://github.com/dnncha/turbo-picard/archive/refs/tags/v0.1.0.tar.gz`\n"
                "- Archive SHA-256:\n"
                "  `<github-v0.1.0-source-archive-sha256>`\n",
                encoding="utf-8",
            )
            with (
                contextlib.redirect_stdout(io.StringIO()) as stdout,
                mock.patch.object(prepare_bioconda_release, "ROOT", root),
                mock.patch.object(prepare_bioconda_release, "RECIPE_DIRS", recipe_dirs),
                mock.patch.object(prepare_bioconda_release, "BIOCONDA_PR", bioconda_pr),
            ):
                status = prepare_bioconda_release.main(["--sha256", digest])

            self.assertEqual(status, 0)
            for recipe_dir in recipe_dirs:
                text = (recipe_dir / "meta.yaml").read_text(encoding="utf-8")
                self.assertIn(f"sha256: {digest}", text)
                self.assertNotIn("path: ../../..", text)
            self.assertIn(f"`{digest}`", bioconda_pr.read_text(encoding="utf-8"))
            self.assertIn("updated packaging/bioconda/BIOCONDA_PR.md", stdout.getvalue())


if __name__ == "__main__":
    unittest.main()
