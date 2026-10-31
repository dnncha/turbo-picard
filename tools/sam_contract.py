from __future__ import annotations

import decimal
import hashlib
import importlib
import json
import re
from collections.abc import Iterable
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import TypedDict

VERSION = "markduplicates-v2"
DESCRIPTION = "full scientific SAM fields and typed tags, sequence/read-group headers with explicit-zone run dates compared as exact instants, and complete metrics/histograms (PG and incidental record ordering excluded)"
COMPARISON = "markduplicates-v2: full SAM records, SQ/RG headers, complete metrics and histograms"


@dataclass(frozen=True, order=True)
class AlignmentRecord:
    query_name: str
    duplicate: bool
    duplicate_type: str | None
    duplicate_set_size: int | None
    duplicate_set_index: int | None
    rx_barcode: str | None
    bx_barcode: str | None
    by_barcode: str | None
    reference_name: str
    position: int
    mate_reference_name: str
    mate_position: int
    cigar: str
    template_length: int
    flags: int
    mapping_quality: int
    sequence: str
    qualities: str
    auxiliary_tags: tuple[str, ...]


def normalize_tag(tag: str) -> str:
    parts = tag.split(":", 2)
    if len(parts) != 3:
        raise ValueError(f"malformed SAM auxiliary tag: {tag!r}")
    if parts[1] == "f":
        try:
            value = decimal.Decimal(parts[2])
            parts[2] = format(value.normalize(), "f")
        except decimal.InvalidOperation as error:
            raise ValueError(f"malformed SAM float tag: {tag!r}") from error
    return ":".join(parts)


def record_from_fields(fields: list[str]) -> AlignmentRecord:
    if len(fields) < 11:
        raise ValueError("malformed SAM record: expected at least 11 fields")
    tags = tuple(sorted(normalize_tag(tag) for tag in fields[11:] if not tag.startswith("PG:Z:")))
    values = {tag[:2]: tag.split(":", 2)[2] for tag in tags}
    flags = int(fields[1])
    return AlignmentRecord(
        query_name=fields[0],
        duplicate=bool(flags & 0x400),
        duplicate_type=values.get("DT"),
        duplicate_set_size=int(values["DS"]) if "DS" in values else None,
        duplicate_set_index=int(values["DI"]) if "DI" in values else None,
        rx_barcode=values.get("RX"),
        bx_barcode=values.get("BX"),
        by_barcode=values.get("BY"),
        reference_name=fields[2],
        position=int(fields[3]),
        mate_reference_name=fields[2] if fields[6] == "=" else fields[6],
        mate_position=int(fields[7]),
        cigar=fields[5],
        template_length=int(fields[8]),
        flags=flags,
        mapping_quality=int(fields[4]),
        sequence=fields[9],
        qualities=fields[10],
        auxiliary_tags=tags,
    )


def sam_records(path: Path) -> Iterable[AlignmentRecord]:
    with path.open(encoding="utf-8") as handle:
        for line in handle:
            if line.strip() and not line.startswith("@"):
                yield record_from_fields(line.rstrip("\r\n").split("\t"))


def normalize_run_date(value: str) -> str:
    if not re.search(r"(?:Z|[+-]\d{2}:?\d{2})$", value) or re.search(r"[.,]\d{7,}", value):
        return value
    try:
        text = re.sub(r"([+-]\d{2})(\d{2})$", r"\1:\2", value.replace("Z", "+00:00"))
        date = datetime.fromisoformat(text)
    except ValueError:
        return value
    return date.astimezone(timezone.utc).isoformat()


class ScientificHeader(TypedDict):
    sequences: list[dict[str, str]]
    read_groups: list[list[list[str]]]


def scientific_header(lines: Iterable[str]) -> ScientificHeader:
    sequences: list[dict[str, str]] = []
    groups: list[list[list[str]]] = []
    for line in lines:
        if line.startswith("@SQ\t"):
            fields = dict(field.split(":", 1) for field in line.rstrip("\r\n").split("\t")[1:])
            sequences.append(
                {name: value for name, value in fields.items() if name != "UR" and (name, value) != ("AS", "null")}
            )
        elif line.startswith("@RG\t"):
            values = [field.split(":", 1) for field in line.rstrip("\r\n").split("\t")[1:]]
            groups.append(sorted([key, normalize_run_date(value) if key == "DT" else value] for key, value in values))
    return {"sequences": sequences, "read_groups": sorted(groups)}


def header_digest(path: Path) -> str:
    if path.suffix.lower() == ".sam":
        with path.open(encoding="utf-8") as handle:
            header = scientific_header(line for line in handle if line.startswith("@"))
    else:
        pysam = importlib.import_module("pysam")
        with pysam.AlignmentFile(str(path), "rc" if path.suffix.lower() == ".cram" else "rb") as handle:
            header = scientific_header(str(handle.header).splitlines())
    return hashlib.sha256(json.dumps(header, sort_keys=True, separators=(",", ":")).encode()).hexdigest()


def complete_metrics(path: Path) -> list[list[str]]:
    rows: list[list[str]] = []
    with path.open(encoding="utf-8") as handle:
        for raw in handle:
            line = raw.rstrip("\r\n")
            if not line.strip():
                continue
            if line.startswith(("## METRICS CLASS", "## HISTOGRAM")) or not line.startswith("#"):
                rows.append(line.split("\t"))
    return rows


def metrics_digest(path: Path) -> str:
    return hashlib.sha256(json.dumps(complete_metrics(path), separators=(",", ":")).encode()).hexdigest()
