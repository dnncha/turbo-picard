"""Regression checks for release-tag visibility in the public adoption audit."""
from __future__ import annotations

import importlib.util
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("adoption_checkout_audit", ROOT / "tools/audit_public_adoption.py")
assert SPEC is not None and SPEC.loader is not None
audit = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = audit
SPEC.loader.exec_module(audit)


class AdoptionCheckoutTests(unittest.TestCase):
    def test_workflow_fetches_release_tags_and_runs_this_check(self) -> None:
        workflow = (ROOT / ".github/workflows/public-adoption-audit.yml").read_text()
        checkout = workflow.split("uses: actions/checkout@", 1)[1].split("\n      - ", 1)[0]
        self.assertRegex(checkout, r"fetch-depth:\s*0\b")
        self.assertIn("python3 -m unittest tools.test_adoption_checkout", workflow)

    @unittest.skipUnless(shutil.which("git"), "git is required for the local checkout regression")
    def test_full_tag_fetch_removes_artificial_missing_release_blocker(self) -> None:
        # All remotes are local file:// repositories; this test uses no network.
        def git(root: Path, *args: str) -> None:
            subprocess.run(["git", "-C", str(root), *args], check=True, capture_output=True, text=True)

        with tempfile.TemporaryDirectory(prefix="adoption-checkout-test-") as tmp:
            root = Path(tmp)
            origin, clone = root / "origin", root / "clone"
            origin.mkdir()
            git(origin, "init", "-b", "main")
            git(origin, "config", "user.name", "Checkout test")
            git(origin, "config", "user.email", "checkout-test@example.invalid")
            git(origin, "commit", "--allow-empty", "-m", "seed")
            (origin / "Cargo.toml").write_text('[workspace.package]\nversion = "0.1.12"\n')
            git(origin, "add", "Cargo.toml")
            git(origin, "commit", "-m", "release")
            git(origin, "tag", "v0.1.12")
            git(root, "clone", "--depth", "1", "--no-tags", origin.as_uri(), str(clone))
            shallow = audit.collect_release_state(clone)
            self.assertFalse(shallow["release_source_ready"])
            self.assertIn("local release tag v0.1.12 is missing", shallow["blockers"])
            git(clone, "fetch", "--unshallow", "--tags", "origin")
            complete = audit.collect_release_state(clone)
            self.assertTrue(complete["release_source_ready"], complete["blockers"])
            self.assertEqual(complete["blockers"], [])


if __name__ == "__main__":
    unittest.main()
