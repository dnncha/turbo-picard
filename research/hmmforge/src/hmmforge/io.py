"""Strict streaming FASTA and atomic, no-clobber JSON output."""
from __future__ import annotations

import gzip
import hashlib
import json
import os
import tempfile
from contextlib import contextmanager
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable, Iterator, TextIO

AMINO = frozenset("ACDEFGHIKLMNPQRSTVWYBXZJUO")


@dataclass(frozen=True)
class Protein:
    index: int
    name: str
    description: str
    sequence: str

    @property
    def key(self) -> str:
        return f"q{self.index:012d}"


def read_fasta(path: str | Path, max_length: int = 100_000) -> Iterator[Protein]:
    """Allow repeated names; stable ordinal identity prevents conflation.

    Sequence whitespace and letter case are normalised. Stops, gaps, digits,
    empty records and non-ASCII symbols are rejected rather than repaired.
    """
    if max_length < 1:
        raise ValueError("max_length must be positive")
    path = Path(path)
    opener = gzip.open if path.suffix.lower() == ".gz" else open
    header = None
    parts: list[str] = []
    length = count = 0

    def record() -> Protein:
        if not length:
            raise ValueError(f"empty protein: {header!r}")
        fields = header.split(maxsplit=1)
        return Protein(count, fields[0], fields[1] if len(fields) == 2 else "", "".join(parts))

    with opener(path, "rt", encoding="utf-8") as handle:
        for line_number, raw in enumerate(handle, 1):
            line = raw.strip()
            if not line:
                continue
            if line.startswith(">"):
                if header is not None:
                    yield record()
                    count += 1
                header, parts, length = line[1:].strip(), [], 0
                if not header:
                    raise ValueError(f"empty FASTA identifier at line {line_number}")
            else:
                if header is None:
                    raise ValueError(f"sequence before FASTA header at line {line_number}")
                seq = "".join(line.split()).upper()
                invalid = set(seq) - AMINO
                if invalid:
                    raise ValueError(f"invalid amino-acid symbols at line {line_number}: {sorted(invalid)!r}")
                length += len(seq)
                if length > max_length:
                    raise ValueError(f"protein {header!r} exceeds max_length={max_length}")
                parts.append(seq)
        if header is None:
            raise ValueError("FASTA contains no proteins")
        yield record()


def batches(records: Iterable[Protein], residues: int = 1_000_000, count: int = 4096):
    """A single protein exceeding the residue budget occupies its own batch."""
    if residues < 1 or count < 1:
        raise ValueError("batch limits must be positive")
    batch, size = [], 0
    for record in records:
        if batch and (size + len(record.sequence) > residues or len(batch) >= count):
            yield batch
            batch, size = [], 0
        batch.append(record)
        size += len(record.sequence)
    if batch:
        yield batch


def sha256(path: str | Path) -> str:
    with open(path, "rb") as handle:
        return hashlib.file_digest(handle, "sha256").hexdigest()


def dump_json(value, handle: TextIO):
    handle.write(json.dumps(value, sort_keys=True, allow_nan=False, separators=(",", ":")) + "\n")


@contextmanager
def atomic_output(path: str | Path):
    """Publish only complete files. Existing output is never overwritten.

    Same-directory hard-link creation atomically claims the destination; unlike
    check-then-rename, this does not race another producer. Requires a local
    filesystem supporting hard links. Failed publication preserves old output.
    """
    path = Path(path)
    if path.exists() or path.is_symlink():
        raise FileExistsError(f"output already exists: {path}")
    fd, temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    try:
        with os.fdopen(fd, "w", encoding="utf-8", newline="\n") as handle:
            yield handle
            handle.flush()
            os.fsync(handle.fileno())
        os.link(temporary, path)
    finally:
        os.unlink(temporary)
