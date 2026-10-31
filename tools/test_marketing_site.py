from __future__ import annotations

import json
import shutil
import subprocess
import tempfile
import unittest
from dataclasses import dataclass
from pathlib import Path
from unittest.mock import patch

import pytest

from tools import build_marketing_site as build
from tools import verify_marketing_site as verify


@dataclass
class FakeBuilder:
    name: str


@dataclass
class FakeApp:
    srcdir: str
    outdir: str
    builder: FakeBuilder


class MarketingSiteTests(unittest.TestCase):
    def test_all_pages_and_provenance(self) -> None:
        assert verify.validate() == []

    def test_real_data_provenance_is_generated_from_pinned_manifest(self) -> None:
        manifest = json.loads((build.ROOT / "benchmarks/real-data/manifest.json").read_text())
        text = (build.ROOT / "docs/site/evidence/index.html").read_text()
        for dataset in manifest["datasets"]:
            for key in ("id", "source_commit", "sha256", "evidence_markdown"):
                assert dataset[key] in text

    def test_full_real_data_release_gate_includes_evidence_page(self) -> None:
        subprocess.run(
            ["python3", "tools/verify_real_data_evidence.py", "--release-ready"],
            cwd=build.ROOT,
            check=True,
            capture_output=True,
        )

    def test_generation_is_deterministic(self) -> None:
        assert build.render_all() == build.render_all()
        assert build.build(check=True) == []

    def test_prefix_depth(self) -> None:
        assert build.prefix("") == "./"
        assert build.prefix("commands/") == "../"
        assert build.prefix("solutions/bam-sorting/") == "../../"

    def test_catalogue_preserves_native_and_fallback_boundary(self) -> None:
        page = verify.Page((build.ROOT / "docs/site/commands/index.html").read_text())
        assert page.commands["CollectRnaSeqMetrics"] == "fallback-only"
        assert page.commands["MarkDuplicates"] == "partial-native"
        assert len(page.commands) == 126

    def test_home_is_not_a_large_ratio_advertisement(self) -> None:
        home = (build.ROOT / "docs/site/index.html").read_text()
        assert "272.12x" not in home
        assert "Small-fixture results" in home
        assert "evidence/" in home

    def test_site_source_never_contains_submitting_form(self) -> None:
        page = verify.Page((build.ROOT / "docs/site/evaluate/index.html").read_text())
        assert page.forms == 0
        assert all(x.get("type") == "button" for x in page.buttons)

    @unittest.skipUnless(
        shutil.which("node") and shutil.which("bash"), "Node and Bash required for planner contract test"
    )
    def test_js_planner_argument_integrity(self) -> None:
        subprocess.run(["node", "website/test-planner.cjs"], cwd=build.ROOT, check=True, capture_output=True)

    def test_sphinx_overlay_preserves_docs_and_runs_on_rebuild(self) -> None:
        with tempfile.TemporaryDirectory() as d:
            out = Path(d)
            (out / "quickstart.html").write_text("docs are preserved")
            app = FakeApp(srcdir=str(build.ROOT / "docs"), outdir=str(out), builder=FakeBuilder(name="html"))
            for _ in range(2):
                (out / "index.html").write_text("Sphinx default")
                build.deploy_to_sphinx(app, None)
                assert "Run your Picard task." in (out / "index.html").read_text()
                assert (out / "quickstart.html").read_text() == "docs are preserved"
                assert (out / "solutions/bam-to-fastq/index.html").is_file()

    def test_failed_or_non_html_build_does_not_publish(self) -> None:
        with tempfile.TemporaryDirectory() as d:
            app = FakeApp(srcdir=str(build.ROOT / "docs"), outdir=d, builder=FakeBuilder(name="html"))
            build.deploy_to_sphinx(app, RuntimeError("failed"))
            app.builder.name = "latex"
            build.deploy_to_sphinx(app, None)
            assert list(Path(d).iterdir()) == []

    def test_stale_site_refuses_deployment(self) -> None:
        app = FakeApp(srcdir=str(build.ROOT / "docs"), outdir="/not-written", builder=FakeBuilder(name="html"))
        with patch.object(build, "build", return_value=["index.html"]), pytest.raises(RuntimeError, match="stale"):
            build.deploy_to_sphinx(app, None)


if __name__ == "__main__":
    unittest.main()
