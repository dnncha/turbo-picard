import importlib.util
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("bench_markduplicates_synthetic.py")
SPEC = importlib.util.spec_from_file_location("bench_markduplicates_synthetic", MODULE_PATH)
bench_markduplicates_synthetic = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(bench_markduplicates_synthetic)
write_sam = bench_markduplicates_synthetic.write_sam


class MarkDuplicatesSyntheticBenchmarkTests(unittest.TestCase):
    def test_duplicate_family_size_controls_shared_coordinate(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "input.sam"
            write_sam(path, reads=6, duplicate_family_size=3)
            records = [
                line.split("\t")
                for line in path.read_text(encoding="utf-8").splitlines()
                if not line.startswith("@")
            ]

        self.assertEqual([row[3] for row in records], ["1", "1", "1", "3", "3", "3"])
        self.assertEqual(len({row[0] for row in records}), 6)
        self.assertTrue(all(row[0].startswith("INST:RUN:FLOW:1:") for row in records))

    def test_help_exposes_external_plan_read_name_regex_switch(self):
        completed = subprocess.run(
            [sys.executable, str(MODULE_PATH), "--help"],
            check=False,
            capture_output=True,
            text=True,
        )
        self.assertEqual(completed.returncode, 0)
        self.assertIn("--read-name-regex", completed.stdout)
        self.assertIn("--paired", completed.stdout)

    def test_paired_mode_emits_read_pairs_with_library_tags(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "paired.sam"
            write_sam(path, reads=3, duplicate_family_size=3, paired=True)
            lines = path.read_text(encoding="utf-8").splitlines()
            records = [line.split("\t") for line in lines if not line.startswith("@")]

        self.assertIn("@RG\tID:rg1\tLB:lib1\tSM:sample1", lines)
        self.assertEqual(len(records), 6)
        self.assertEqual([row[1] for row in records], ["99"] * 3 + ["147"] * 3)
        self.assertEqual(len({row[0] for row in records}), 3)
        self.assertTrue(all("RG:Z:rg1" in row[11:] for row in records))


if __name__ == "__main__":
    unittest.main()
