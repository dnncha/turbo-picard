#!/usr/bin/env python3
"""Semantic comparison for Picard and Jeanluc MarkDuplicates outputs."""

from __future__ import annotations

import argparse
import csv
import json
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable


DUPLICATE_FLAG = 0x400


@dataclass(frozen=True, order=True)
class RecordKey:
    query_name: str
    duplicate: bool
    duplicate_type: str | None
    duplicate_set_size: int | None
    duplicate_set_index: int | None
    rx_barcode: str | None
    reference_name: str
    position: int
    mate_reference_name: str
    mate_position: int
    cigar: str
    template_length: int


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Compare Picard and Jeanluc MarkDuplicates outputs semantically.",
    )
    parser.add_argument("--picard-bam", required=True, type=Path)
    parser.add_argument("--jeanluc-bam", required=True, type=Path)
    parser.add_argument("--picard-metrics", required=True, type=Path)
    parser.add_argument("--jeanluc-metrics", required=True, type=Path)
    parser.add_argument(
        "--json",
        action="store_true",
        help="Emit a JSON summary instead of human-readable text.",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    differences: list[str] = []

    picard_records = sorted(read_alignment_records(args.picard_bam))
    jeanluc_records = sorted(read_alignment_records(args.jeanluc_bam))

    if picard_records != jeanluc_records:
        differences.append(
            "alignment semantic records differ "
            f"(picard={len(picard_records)} jeanluc={len(jeanluc_records)})"
        )

    picard_metrics = read_metrics(args.picard_metrics)
    jeanluc_metrics = read_metrics(args.jeanluc_metrics)
    if picard_metrics != jeanluc_metrics:
        differences.append("metrics files differ after comment/header normalization")

    result = {
        "ok": not differences,
        "differences": differences,
        "picard_records": len(picard_records),
        "jeanluc_records": len(jeanluc_records),
    }

    if args.json:
        print(json.dumps(result, indent=2, sort_keys=True))
    elif differences:
        for difference in differences:
            print(f"DIFF: {difference}", file=sys.stderr)
    else:
        print("MarkDuplicates outputs are semantically equivalent")

    return 0 if not differences else 1


def read_alignment_records(path: Path) -> Iterable[RecordKey]:
    suffix = path.suffix.lower()
    if suffix == ".sam":
        return read_sam_records(path)
    return read_hts_records(path)


def read_sam_records(path: Path) -> list[RecordKey]:
    records = []
    with path.open("r", encoding="utf-8") as handle:
        for line in handle:
            if not line.strip() or line.startswith("@"):
                continue
            fields = line.rstrip("\n").split("\t")
            if len(fields) < 11:
                raise ValueError(f"malformed SAM record in {path}: {line.rstrip()}")
            flag = int(fields[1])
            records.append(
                RecordKey(
                    query_name=fields[0],
                    duplicate=bool(flag & DUPLICATE_FLAG),
                    duplicate_type=optional_tag(fields[11:], "DT"),
                    duplicate_set_size=optional_int_tag(fields[11:], "DS"),
                    duplicate_set_index=optional_int_tag(fields[11:], "DI"),
                    rx_barcode=optional_tag(fields[11:], "RX"),
                    reference_name=fields[2],
                    position=int(fields[3]),
                    mate_reference_name=fields[6],
                    mate_position=int(fields[7]),
                    cigar=fields[5],
                    template_length=int(fields[8]),
                )
            )
    return records


def read_hts_records(path: Path) -> list[RecordKey]:
    try:
        import pysam  # type: ignore[import-not-found]
    except ImportError as error:
        raise SystemExit(
            "Reading BAM/CRAM requires pysam. Install pysam or provide SAM files."
        ) from error

    records = []
    with pysam.AlignmentFile(str(path), "rb") as handle:
        for record in handle.fetch(until_eof=True):
            records.append(
                RecordKey(
                    query_name=record.query_name,
                    duplicate=record.is_duplicate,
                    duplicate_type=record.get_tag("DT") if record.has_tag("DT") else None,
                    duplicate_set_size=record.get_tag("DS") if record.has_tag("DS") else None,
                    duplicate_set_index=record.get_tag("DI") if record.has_tag("DI") else None,
                    rx_barcode=record.get_tag("RX") if record.has_tag("RX") else None,
                    reference_name=handle.get_reference_name(record.reference_id)
                    if record.reference_id >= 0
                    else "*",
                    position=record.reference_start + 1
                    if record.reference_start >= 0
                    else 0,
                    mate_reference_name=handle.get_reference_name(record.next_reference_id)
                    if record.next_reference_id >= 0
                    else "*",
                    mate_position=record.next_reference_start + 1
                    if record.next_reference_start >= 0
                    else 0,
                    cigar=record.cigarstring or "*",
                    template_length=record.template_length,
                )
            )
    return records


def optional_tag(fields: list[str], tag: str) -> str | None:
    prefix = f"{tag}:Z:"
    for field in fields:
        if field.startswith(prefix):
            return field.removeprefix(prefix)
    return None


def optional_int_tag(fields: list[str], tag: str) -> int | None:
    prefix = f"{tag}:i:"
    for field in fields:
        if field.startswith(prefix):
            return int(field.removeprefix(prefix))
    return None


def read_metrics(path: Path) -> list[list[str]]:
    rows = []
    in_duplication_metrics = False
    with path.open("r", encoding="utf-8") as handle:
        for raw_line in handle:
            line = raw_line.strip()
            if not line or line.startswith("#"):
                if in_duplication_metrics and rows:
                    break
                continue
            if line.startswith("## METRICS CLASS"):
                in_duplication_metrics = "picard.sam.DuplicationMetrics" in line
                continue
            if line.startswith("## "):
                if in_duplication_metrics and rows:
                    break
                in_duplication_metrics = False
                continue
            if not in_duplication_metrics:
                continue
            rows.append(next(csv.reader([line], delimiter="\t")))
    return rows


if __name__ == "__main__":
    raise SystemExit(main())
