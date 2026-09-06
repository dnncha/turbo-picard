"""Stable external sorting for validation records, without a process per run.

Only locally generated, length-prefixed byte records are written. No pickle,
network access, or shell is involved. Memory limits bound each input chunk's
estimated byte storage and record count, not the interpreter's total RSS. A
single oversized record is accepted. Merge fan-in bounds open input files.
"""
from __future__ import annotations

from contextlib import ExitStack, contextmanager
from dataclasses import dataclass
import heapq
from pathlib import Path
import struct
import sys
import tempfile
from typing import Any, BinaryIO, Callable, Iterable, Iterator

_LENGTH = struct.Struct('>Q')


@dataclass
class SortStats:
    records: int = 0
    spills: int = 0
    merge_passes: int = 0
    max_chunk_records: int = 0
    max_chunk_bytes: int = 0
    max_open_runs: int = 0


def _write_records(path: Path, rows: Iterable[bytes]) -> None:
    with path.open('xb') as stream:
        for row in rows:
            stream.write(_LENGTH.pack(len(row)))
            stream.write(row)


def _read_records(stream: BinaryIO, max_record_bytes: int) -> Iterator[bytes]:
    while True:
        header = stream.read(_LENGTH.size)
        if not header:
            return
        if len(header) != _LENGTH.size:
            raise ValueError('truncated validation sort run: incomplete length header')
        size, = _LENGTH.unpack(header)
        if size > max_record_bytes:
            raise ValueError('corrupt validation sort run: impossible record length')
        row = stream.read(size)
        if len(row) != size:
            raise ValueError('truncated validation sort run: incomplete record')
        yield row


@contextmanager
def sorted_records(
    rows: Iterable[bytes], *, key: Callable[[bytes], Any] | None = None,
    chunk_bytes: int = 8 * 1024 * 1024, chunk_records: int = 50_000,
    fan_in: int = 32, temp_dir: Path | None = None, stats: SortStats | None = None,
) -> Iterator[Iterator[bytes]]:
    """Yield a stable sorted iterator; always remove this call's scratch files.

    Callers must use ``with`` so early exits and consumer exceptions close all
    runs. Scratch storage defaults to Python's TMPDIR-aware temporary directory.
    Multiplicity is preserved: this is sorting, not deduplication or sampling.
    """
    if chunk_bytes < 1 or chunk_records < 1 or fan_in < 2:
        raise ValueError('sort limits must be positive and fan_in must be at least two')
    stats = stats if stats is not None else SortStats()
    chunk: list[bytes] = []
    resident = 0
    max_record = 0
    with tempfile.TemporaryDirectory(prefix='turbo-picard-compare-', dir=temp_dir) as directory:
        root = Path(directory)
        runs: list[Path] = []
        next_id = 0

        def new_path() -> Path:
            nonlocal next_id
            next_id += 1
            return root / f'{next_id}.run'

        def spill() -> None:
            nonlocal resident
            chunk.sort(key=key)
            path = new_path()
            _write_records(path, chunk)
            runs.append(path)
            stats.spills += 1
            chunk.clear()
            resident = 0

        for row in rows:
            if not isinstance(row, bytes):
                raise TypeError('validation sort records must be bytes')
            cost = sys.getsizeof(row) + 8  # byte object plus list-reference estimate
            if chunk and (len(chunk) >= chunk_records or resident + cost > chunk_bytes):
                spill()
            chunk.append(row)
            resident += cost
            max_record = max(max_record, len(row))
            stats.records += 1
            stats.max_chunk_records = max(stats.max_chunk_records, len(chunk))
            stats.max_chunk_bytes = max(stats.max_chunk_bytes, resident)

        if not runs:
            chunk.sort(key=key)
            yield iter(chunk)
            return
        if chunk:
            spill()

        @contextmanager
        def merge(paths: list[Path]) -> Iterator[Iterator[bytes]]:
            stats.max_open_runs = max(stats.max_open_runs, len(paths))
            with ExitStack() as stack:
                streams = [stack.enter_context(path.open('rb')) for path in paths]
                iterators = [_read_records(stream, max_record) for stream in streams]
                yield heapq.merge(*iterators, key=key)

        while len(runs) > fan_in:
            merged: list[Path] = []
            for offset in range(0, len(runs), fan_in):
                group = runs[offset:offset + fan_in]
                if len(group) == 1:
                    merged.extend(group)
                    continue
                output = new_path()
                with merge(group) as records:
                    _write_records(output, records)
                for path in group:
                    path.unlink()
                merged.append(output)
            runs = merged
            stats.merge_passes += 1
        with merge(runs) as records:
            yield records
