import importlib.util
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


if __name__ == "__main__":
    unittest.main()
