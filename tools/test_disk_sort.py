from __future__ import annotations

import io
import random
import struct
import tempfile
import unittest
from collections.abc import Callable, Iterable, Iterator
from pathlib import Path
from typing import cast
from unittest import mock

import pytest

from tools import disk_sort


def faulty_writer(
    original: Callable[[Path, Iterable[bytes]], None], fail_call: int
) -> Callable[[Path, Iterable[bytes]], None]:
    calls = 0

    def faulty_write(path: Path, rows: Iterable[bytes]) -> None:
        nonlocal calls
        calls += 1
        if calls == fail_call:
            path.write_bytes(b"partial")
            raise OSError("disk full")
        original(path, rows)

    return faulty_write


class DiskSortTests(unittest.TestCase):
    def test_matches_python_sort_with_duplicates_and_multilevel_merges(self) -> None:
        rng = random.Random(1907)
        rows = [rng.randbytes(rng.randrange(0, 60)) for _ in range(900)] + [b"repeated"] * 50
        for fan_in in (2, 3, 32):
            stats = disk_sort.SortStats()
            with disk_sort.sorted_records(
                rows, chunk_records=7, chunk_bytes=500, fan_in=fan_in, stats=stats
            ) as ordered:
                assert list(ordered) == sorted(rows)
            assert stats.merge_passes > 0
            assert stats.max_open_runs <= fan_in
            assert stats.max_chunk_records <= 7
            assert stats.max_chunk_bytes <= 500
            assert stats.records == len(rows)

    def test_custom_key_stability_across_runs_and_merge_levels(self) -> None:
        rows = [f"{i % 5}:{i}".encode() for i in range(120)]

        def key(row: bytes) -> int:
            return int(row.split(b":")[0])

        with disk_sort.sorted_records(rows, key=key, chunk_records=3, fan_in=2) as ordered:
            assert list(ordered) == sorted(rows, key=key)

    def test_empty_and_in_memory_input(self) -> None:
        for rows in ([], [b"b", b"\n\0", b"a", b""]):
            stats = disk_sort.SortStats()
            with disk_sort.sorted_records(rows, stats=stats) as ordered:
                assert list(ordered) == sorted(rows)
            assert stats.spills == 0

    def test_accepts_single_oversized_record(self) -> None:
        rows = [b"z" * 4096, b"a", b"z" * 4096]
        stats = disk_sort.SortStats()
        with disk_sort.sorted_records(rows, chunk_bytes=100, stats=stats) as ordered:
            assert list(ordered) == sorted(rows)
        assert stats.max_chunk_records == 1

    def test_cleanup_on_early_consumer_exit_and_consumer_exception(self) -> None:
        for fail in (False, True):
            with tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                try:
                    with disk_sort.sorted_records([b"b", b"a"] * 10, chunk_records=1, fan_in=2, temp_dir=root) as rows:
                        assert next(rows) == b"a"
                        if fail:
                            raise RuntimeError("consumer stopped")
                except RuntimeError:
                    assert fail
                assert list(root.iterdir()) == []

    def test_cleanup_on_input_and_spill_and_merge_errors(self) -> None:
        def broken_input() -> Iterator[bytes]:
            yield b"b"
            yield b"a"
            raise RuntimeError("input stopped")

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            with (
                pytest.raises(RuntimeError, match="input stopped"),
                disk_sort.sorted_records(broken_input(), chunk_records=1, temp_dir=root),
            ):
                self.fail("must not reach yield")
            assert list(root.iterdir()) == []
            original = disk_sort._write_records
            for fail_call in (1, 6):
                with (
                    mock.patch.object(disk_sort, "_write_records", faulty_writer(original, fail_call)),
                    pytest.raises(OSError, match="disk full"),
                    disk_sort.sorted_records([b"b"] * 5, chunk_records=1, fan_in=2, temp_dir=root),
                ):
                    self.fail("must not reach yield")
                assert list(root.iterdir()) == []

    def test_rejects_truncation_and_impossible_lengths(self) -> None:
        valid = struct.pack(">Q", 3) + b"abc"
        assert list(disk_sort._read_records(io.BytesIO(valid), 3)) == [b"abc"]
        assert list(disk_sort._read_records(io.BytesIO(b""), 3)) == []
        for end in range(1, len(valid)):
            with pytest.raises(ValueError, match="truncated"):
                list(disk_sort._read_records(io.BytesIO(valid[:end]), 3))
        with pytest.raises(ValueError, match="impossible"):
            list(disk_sort._read_records(io.BytesIO(struct.pack(">Q", 2**64 - 1)), 3))

    def test_invalid_configuration_and_nonbytes_records(self) -> None:
        default_chunk_bytes = 8 * 1024 * 1024
        default_chunk_records = 50_000
        default_fan_in = 32
        for chunk_bytes, chunk_records, fan_in in (
            (0, default_chunk_records, default_fan_in),
            (default_chunk_bytes, -1, default_fan_in),
            (default_chunk_bytes, default_chunk_records, 1),
        ):
            with (
                pytest.raises(ValueError, match="sort limits"),
                disk_sort.sorted_records([], chunk_bytes=chunk_bytes, chunk_records=chunk_records, fan_in=fan_in),
            ):
                self.fail("invalid configuration accepted")
        with pytest.raises(TypeError), disk_sort.sorted_records(cast("Iterable[bytes]", ["text"])):
            self.fail("invalid record accepted")


if __name__ == "__main__":
    unittest.main()
