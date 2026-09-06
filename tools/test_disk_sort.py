"""Differential, resource-bound and failure-path tests for validation sorting."""
from __future__ import annotations
import io
from pathlib import Path
import random
import struct
import tempfile
import unittest
from unittest import mock
from tools import disk_sort


class DiskSortTests(unittest.TestCase):
    def test_matches_python_sort_with_duplicates_and_multilevel_merges(self):
        rng = random.Random(1907)
        rows = [rng.randbytes(rng.randrange(0, 60)) for _ in range(900)] + [b'repeated'] * 50
        for fan_in in (2, 3, 32):
            stats = disk_sort.SortStats()
            with disk_sort.sorted_records(rows, chunk_records=7, chunk_bytes=500, fan_in=fan_in, stats=stats) as ordered:
                self.assertEqual(list(ordered), sorted(rows))
            self.assertGreater(stats.merge_passes, 0)
            self.assertLessEqual(stats.max_open_runs, fan_in)
            self.assertLessEqual(stats.max_chunk_records, 7)
            self.assertLessEqual(stats.max_chunk_bytes, 500)
            self.assertEqual(stats.records, len(rows))

    def test_custom_key_stability_across_runs_and_merge_levels(self):
        rows = [f'{i % 5}:{i}'.encode() for i in range(120)]
        key = lambda row: int(row.split(b':')[0])
        with disk_sort.sorted_records(rows, key=key, chunk_records=3, fan_in=2) as ordered:
            self.assertEqual(list(ordered), sorted(rows, key=key))

    def test_empty_and_in_memory_input(self):
        for rows in ([], [b'b', b'\n\0', b'a', b'']):
            stats = disk_sort.SortStats()
            with disk_sort.sorted_records(rows, stats=stats) as ordered:
                self.assertEqual(list(ordered), sorted(rows))
            self.assertEqual(stats.spills, 0)

    def test_accepts_single_oversized_record(self):
        rows = [b'z' * 4096, b'a', b'z' * 4096]
        stats = disk_sort.SortStats()
        with disk_sort.sorted_records(rows, chunk_bytes=100, stats=stats) as ordered:
            self.assertEqual(list(ordered), sorted(rows))
        self.assertEqual(stats.max_chunk_records, 1)

    def test_cleanup_on_early_consumer_exit_and_consumer_exception(self):
        for fail in (False, True):
            with tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                try:
                    with disk_sort.sorted_records([b'b', b'a'] * 10, chunk_records=1, fan_in=2, temp_dir=root) as rows:
                        self.assertEqual(next(rows), b'a')
                        if fail:
                            raise RuntimeError('consumer stopped')
                except RuntimeError:
                    self.assertTrue(fail)
                self.assertEqual(list(root.iterdir()), [])

    def test_cleanup_on_input_and_spill_and_merge_errors(self):
        def broken_input():
            yield b'b'
            yield b'a'
            raise RuntimeError('input stopped')
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            with self.assertRaisesRegex(RuntimeError, 'input stopped'):
                with disk_sort.sorted_records(broken_input(), chunk_records=1, temp_dir=root):
                    self.fail('must not reach yield')
            self.assertEqual(list(root.iterdir()), [])
            original = disk_sort._write_records
            for fail_call in (1, 6):  # first spill and first intermediate merge
                calls = 0
                def faulty_write(path, rows):
                    nonlocal calls
                    calls += 1
                    if calls == fail_call:
                        path.write_bytes(b'partial')
                        raise OSError('disk full')
                    original(path, rows)
                with mock.patch.object(disk_sort, '_write_records', faulty_write):
                    with self.assertRaisesRegex(OSError, 'disk full'):
                        with disk_sort.sorted_records([b'b'] * 5, chunk_records=1, fan_in=2, temp_dir=root):
                            self.fail('must not reach yield')
                self.assertEqual(list(root.iterdir()), [])

    def test_rejects_truncation_and_impossible_lengths(self):
        valid = struct.pack('>Q', 3) + b'abc'
        self.assertEqual(list(disk_sort._read_records(io.BytesIO(valid), 3)), [b'abc'])
        self.assertEqual(list(disk_sort._read_records(io.BytesIO(b''), 3)), [])
        for end in range(1, len(valid)):
            with self.assertRaisesRegex(ValueError, 'truncated'):
                list(disk_sort._read_records(io.BytesIO(valid[:end]), 3))
        with self.assertRaisesRegex(ValueError, 'impossible'):
            list(disk_sort._read_records(io.BytesIO(struct.pack('>Q', 2**64-1)), 3))

    def test_invalid_configuration_and_nonbytes_records(self):
        for options in ({'chunk_bytes': 0}, {'chunk_records': -1}, {'fan_in': 1}):
            with self.assertRaises(ValueError):
                with disk_sort.sorted_records([], **options):
                    self.fail('invalid configuration accepted')
        with self.assertRaises(TypeError):
            with disk_sort.sorted_records(['text']):
                self.fail('invalid record accepted')


if __name__ == '__main__':
    unittest.main()
