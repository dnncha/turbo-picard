#!/usr/bin/env python3
"""Semantic comparison for Picard and turbo-picard MarkDuplicates outputs."""

from __future__ import annotations

import argparse
import csv
import json
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable


if __package__:
    from .sam_contract import AlignmentRecord as RecordKey, VERSION, DESCRIPTION, record_from_fields, header_digest, complete_metrics
else:
    from sam_contract import AlignmentRecord as RecordKey, VERSION, DESCRIPTION, record_from_fields, header_digest, complete_metrics


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Compare Picard and turbo-picard MarkDuplicates outputs semantically.",
    )
    parser.add_argument("--picard-bam", required=True, type=Path)
    parser.add_argument("--turbo-picard-bam", required=True, type=Path)
    parser.add_argument("--picard-metrics", required=True, type=Path)
    parser.add_argument("--turbo-picard-metrics", required=True, type=Path)
    parser.add_argument(
        "--json",
        action="store_true",
        help="Emit a JSON summary instead of human-readable text.",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    differences: list[str] = []

    # Optional values must have a total order; a missing DT is not comparable
    # with a string in Python's default dataclass ordering.
    def record_key(record):
        from dataclasses import astuple
        return tuple((value is not None, value) for value in astuple(record))
    picard_records = sorted(read_alignment_records(args.picard_bam), key=record_key)
    turbo_picard_records = sorted(read_alignment_records(args.turbo_picard_bam), key=record_key)
    if header_digest(args.picard_bam) != header_digest(args.turbo_picard_bam):
        differences.append("sequence/read-group header definitions differ")

    if picard_records != turbo_picard_records:
        differences.append(
            "alignment semantic records differ "
            f"(picard={len(picard_records)} turbo-picard={len(turbo_picard_records)})"
        )

    picard_metrics = read_metrics(args.picard_metrics)
    turbo_picard_metrics = read_metrics(args.turbo_picard_metrics)
    differences.extend(
        metric_differences(
            picard_metrics,
            turbo_picard_metrics,
            args.picard_metrics,
            args.turbo_picard_metrics,
        )
    )

    result = {
        "ok": not differences,
        "comparator": VERSION,
        "contract": DESCRIPTION,
        "differences": differences,
        "picard_records": len(picard_records),
        "turbo_picard_records": len(turbo_picard_records),
    }

    if args.json:
        print(json.dumps(result, indent=2, sort_keys=True))
    elif differences:
        for difference in differences:
            print(f"DIFF: {difference}", file=sys.stderr)
    else:
        print("MarkDuplicates outputs are semantically equivalent")

    return 0 if not differences else 1


def metric_differences(
    picard_metrics: list[list[str]],
    turbo_picard_metrics: list[list[str]],
    picard_metrics_path: Path,
    turbo_picard_metrics_path: Path,
) -> list[str]:
    differences = []
    if not picard_metrics:
        differences.append(
            "Picard metrics file has no picard.sam.DuplicationMetrics block: "
            f"{picard_metrics_path}"
        )
    if not turbo_picard_metrics:
        differences.append(
            "turbo-picard metrics file has no picard.sam.DuplicationMetrics block: "
            f"{turbo_picard_metrics_path}"
        )
    if picard_metrics and turbo_picard_metrics and picard_metrics != turbo_picard_metrics:
        differences.append("metrics files differ after comment/header normalization")
    if picard_metrics and turbo_picard_metrics and complete_metrics(picard_metrics_path) != complete_metrics(turbo_picard_metrics_path):
        differences.append("complete metrics or histogram blocks differ")
    return differences


def read_alignment_records(path: Path) -> Iterable[RecordKey]:
    suffix = path.suffix.lower()
    if suffix == ".sam":
        return read_sam_records(path)
    return read_hts_records(path)


def read_sam_records(path: Path) -> list[RecordKey]:
    with path.open(encoding="utf-8") as handle:
        return [record_from_fields(line.rstrip("\r\n").split("\t"))
                for line in handle if line.strip() and not line.startswith("@")]


def read_hts_records(path: Path) -> list[RecordKey]:
    try:
        import pysam
    except ImportError as error:
        raise SystemExit("Reading BAM/CRAM requires pysam. Install pysam or provide SAM files.") from error
    with pysam.AlignmentFile(str(path), "rc" if path.suffix.lower() == ".cram" else "rb") as handle:
        return [record_from_fields(record.to_string().split("\t"))
                for record in handle.fetch(until_eof=True)]


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
            if line.startswith("## METRICS CLASS"):
                in_duplication_metrics = "picard.sam.DuplicationMetrics" in line
                continue
            if not line or line.startswith("#"):
                if in_duplication_metrics and rows:
                    break
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
