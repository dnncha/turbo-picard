from __future__ import annotations

import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest


MODULE_PATH = Path(__file__).with_name("build_release_manifest.py")
SPEC = importlib.util.spec_from_file_location("build_release_manifest", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
build_release_manifest = importlib.util.module_from_spec(SPEC)
sys.modules["build_release_manifest"] = build_release_manifest
SPEC.loader.exec_module(build_release_manifest)


class BuildReleaseManifestTests(unittest.TestCase):
    def test_summarizes_parity_profile(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "profile.json"
            path.write_text(
                json.dumps(
                    {
                        "benchmarks": [
                            {"command": "MarkDuplicates", "median_speedup": 4.0, "parity": "PASS"},
                            {"command": "SortSam", "median_speedup": 9.0, "parity": "PASS"},
                        ]
                    }
                ),
                encoding="utf-8",
            )
            summary = build_release_manifest.summarize_benchmark(path)
        self.assertEqual(summary["command_count"], 2)
        self.assertEqual(summary["markduplicates_speedup"], 4.0)
        self.assertEqual(summary["minimum_speedup"], 4.0)
        self.assertEqual(summary["maximum_speedup"], 9.0)
        self.assertAlmostEqual(summary["geometric_mean_speedup"], 6.0)

    def test_rejects_benchmark_mismatch(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "profile.json"
            path.write_text(
                '{"benchmarks": [{"command": "MarkDuplicates", "median_speedup": 0, "parity": "PASS"}]}',
                encoding="utf-8",
            )
            with self.assertRaisesRegex(ValueError, "median_speedup must be positive"):
                build_release_manifest.summarize_benchmark(path)

    def test_collects_hashes_and_source_state_without_paths(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "Cargo.toml").write_text(
                '[workspace.package]\nversion = "0.1.12"\n',
                encoding="utf-8",
            )
            dist = root / "dist"
            dist.mkdir()
            artifact = dist / "turbo_picard-0.1.12.tar.gz"
            artifact.write_bytes(b"candidate")
            commit = "a" * 40
            responses = {
                ("status", "--porcelain"): (0, []),
                ("rev-parse", "HEAD"): (0, [commit]),
                ("rev-parse", "--abbrev-ref", "HEAD"): (0, ["main"]),
                ("rev-list", "-n", "1", "v0.1.12"): (0, [commit]),
                ("ls-remote", "--tags", "origin", "v0.1.12*"): (
                    0,
                    [commit + "\trefs/tags/v0.1.12^{}"],
                ),
            }

            def fake_git(args: list[str], _root: Path) -> tuple[int, list[str]]:
                return responses[tuple(args)]

            manifest = build_release_manifest.build_manifest(
                root=root,
                dist=dist,
                git_runner=fake_git,
                artifact_validator=lambda _dist, _root: [],
            )
        self.assertEqual(manifest["workspace_version"], "0.1.12")
        self.assertTrue(manifest["source"]["release_source_ready"])
        self.assertEqual(manifest["artifacts"][0]["filename"], "turbo_picard-0.1.12.tar.gz")
        self.assertEqual(manifest["source"]["ignored_generated_path_count"], 0)
        self.assertNotIn("/private", json.dumps(manifest))

    def test_ignores_generated_distributions_but_not_source_changes(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "Cargo.toml").write_text(
                '[workspace.package]\nversion = "0.1.12"\n',
                encoding="utf-8",
            )
            dist = root / "dist"
            dist.mkdir()
            (dist / "turbo_picard-0.1.12.tar.gz").write_bytes(b"candidate")
            commit = "b" * 40
            responses = {
                ("status", "--porcelain"): (
                    0,
                    ["?? dist/turbo_picard-0.1.12.tar.gz", " M README.md"],
                ),
                ("rev-parse", "HEAD"): (0, [commit]),
                ("rev-parse", "--abbrev-ref", "HEAD"): (0, ["main"]),
                ("rev-list", "-n", "1", "v0.1.12"): (0, [commit]),
                ("ls-remote", "--tags", "origin", "v0.1.12*"): (
                    0,
                    [commit + "\trefs/tags/v0.1.12"],
                ),
            }

            def fake_git(args: list[str], _root: Path) -> tuple[int, list[str]]:
                return responses[tuple(args)]

            manifest = build_release_manifest.build_manifest(
                root=root,
                dist=dist,
                git_runner=fake_git,
                artifact_validator=lambda _dist, _root: [],
            )

        self.assertFalse(manifest["source"]["worktree_clean"])
        self.assertEqual(manifest["source"]["changed_path_count"], 1)
        self.assertEqual(manifest["source"]["ignored_generated_path_count"], 1)
        self.assertEqual(
            manifest["source"]["blockers"],
            ["worktree has uncommitted changes"],
        )

    def test_generated_distributions_do_not_dirty_source_state(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "Cargo.toml").write_text(
                '[workspace.package]\nversion = "0.1.12"\n',
                encoding="utf-8",
            )
            dist = root / "dist"
            dist.mkdir()
            commit = "c" * 40
            responses = {
                ("status", "--porcelain"): (
                    0,
                    ["?? dist/turbo_picard-0.1.12.tar.gz"],
                ),
                ("rev-parse", "HEAD"): (0, [commit]),
                ("rev-parse", "--abbrev-ref", "HEAD"): (0, ["main"]),
                ("rev-list", "-n", "1", "v0.1.12"): (0, [commit]),
                ("ls-remote", "--tags", "origin", "v0.1.12*"): (
                    0,
                    [commit + "\trefs/tags/v0.1.12"],
                ),
            }

            def fake_git(args: list[str], _root: Path) -> tuple[int, list[str]]:
                return responses[tuple(args)]

            source = build_release_manifest.collect_source_state(
                root,
                git_runner=fake_git,
                ignored_generated_paths=(dist,),
            )

        self.assertTrue(source["worktree_clean"])
        self.assertTrue(source["release_source_ready"])
        self.assertEqual(source["changed_path_count"], 0)
        self.assertEqual(source["ignored_generated_path_count"], 1)
        self.assertEqual(source["blockers"], [])


if __name__ == "__main__":
    unittest.main()
