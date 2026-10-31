from __future__ import annotations

import importlib.util
import re
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("adoption_checkout_audit", ROOT / "tools/audit_public_adoption.py")
assert SPEC is not None
assert SPEC.loader is not None
audit = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = audit
SPEC.loader.exec_module(audit)


class AdoptionCheckoutTests(unittest.TestCase):
    def test_workflow_fetches_release_tags_and_runs_this_check(self) -> None:
        workflow = (ROOT / ".github/workflows/public-adoption-audit.yml").read_text()
        checkout = workflow.split("uses: actions/checkout@", 1)[1].split("\n      - ", 1)[0]
        assert re.search(r"fetch-depth:\s*0\b", checkout)
        assert "python3 -m unittest tools.test_adoption_checkout" in workflow

    @unittest.skipUnless(shutil.which("git"), "git is required for the local checkout regression")
    def test_full_tag_fetch_removes_artificial_missing_release_blocker(self) -> None:
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
            assert not shallow["release_source_ready"]
            assert "local release tag v0.1.12 is missing" in shallow["blockers"]
            git(clone, "fetch", "--unshallow", "--tags", "origin")
            complete = audit.collect_release_state(clone)
            assert complete["release_source_ready"], complete["blockers"]
            assert complete["blockers"] == []


if __name__ == "__main__":
    unittest.main()
