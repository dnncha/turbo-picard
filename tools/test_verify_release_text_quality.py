#!/usr/bin/env python3
"""Tests for release-facing prose quality checks."""

from __future__ import annotations

import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("verify_release_text_quality.py")
SPEC = importlib.util.spec_from_file_location("verify_release_text_quality", MODULE_PATH)
assert SPEC is not None
verify_release_text_quality = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules["verify_release_text_quality"] = verify_release_text_quality
SPEC.loader.exec_module(verify_release_text_quality)


def write(root: Path, path: Path, text: str) -> None:
    target = root / path
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(text, encoding="utf-8")


class ReleaseTextQualityTests(unittest.TestCase):
    def make_tree(self) -> Path:
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        root = Path(temp.name)
        for path in verify_release_text_quality.RELEASE_TEXT_PATHS:
            cues = "\n".join(verify_release_text_quality.REQUIRED_READER_CUES.get(path, []))
            write(
                root,
                path,
                f"""
Plain release-facing text.
{cues}
Evidence, citation, fallback, and Bioconda boundaries stay explicit.
""",
            )
        return root

    def test_accepts_current_reader_cues(self) -> None:
        root = self.make_tree()
        self.assertEqual([], verify_release_text_quality.validate_release_text(root))

    def test_rejects_hype_phrase(self) -> None:
        root = self.make_tree()
        write(
            root,
            Path("README.md"),
            """
The full docs are on Read the Docs
Why Use It
Do not flip a whole production pipeline at once
Cite the archived release.
This is a seamless comprehensive solution.
""",
        )

        errors = verify_release_text_quality.validate_release_text(root)
        self.assertIn(
            "README.md contains release-text banned phrase: seamless",
            errors,
        )
        self.assertIn(
            "README.md contains release-text banned phrase: comprehensive solution",
            errors,
        )

    def test_rejects_missing_reader_cue(self) -> None:
        root = self.make_tree()
        write(
            root,
            Path("packaging/bioconda/BIOCONDA_PR.md"),
            """
separate optional compatibility shim
turbo-picard is not a full Picard replacement
Expected Bioconda checkout checks
bioconda-utils build --docker --mulled-test turbo-picard
""",
        )

        errors = verify_release_text_quality.validate_release_text(root)
        self.assertIn(
            "packaging/bioconda/BIOCONDA_PR.md missing reader cue: "
            "This PR adds `turbo-picard`",
            errors,
        )

    def test_rejects_internal_planning_docs(self) -> None:
        root = self.make_tree()
        write(
            root,
            Path("docs/superpowers/plans/example.md"),
            "Internal implementation checklist.",
        )

        errors = verify_release_text_quality.validate_release_text(root)
        self.assertIn(
            "docs/superpowers contains internal planning notes; "
            "keep release-facing docs public",
            errors,
        )

    def test_rejects_internal_process_language(self) -> None:
        root = self.make_tree()
        phrase = "pilot " + "conversations"
        readme = root / "benchmarks/production/README.md"
        readme.write_text(
            readme.read_text(encoding="utf-8") + f"\nTrack {phrase} here.\n",
            encoding="utf-8",
        )

        errors = verify_release_text_quality.validate_release_text(root)
        self.assertIn(
            "benchmarks/production/README.md contains release-text banned phrase: "
            + phrase,
            errors,
        )


if __name__ == "__main__":
    unittest.main()
