#!/usr/bin/env python3
"""Shared Picard-vs-turbo-picard output comparison helpers for parity scripts."""

from __future__ import annotations

import argparse
import csv
import importlib.util
import subprocess
import sys
from collections import Counter
from pathlib import Path


def _compare_real_data_module():
    module_path = Path(__file__).with_name("compare_real_data.py")
    spec = importlib.util.spec_from_file_location("compare_real_data", module_path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"unable to load {module_path}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def sam_records(path: Path, reference: str) -> list[str]:
    cmd = ["samtools", "view"]
    if reference:
        cmd.extend(["-T", reference])
    cmd.append(str(path))
    completed = subprocess.run(
        cmd,
        check=True,
        capture_output=True,
        text=True,
    )
    return [line for line in completed.stdout.splitlines() if line]


def load_metrics(path: Path) -> dict[str, list[str]]:
    rows: dict[str, list[str]] = {}
    with path.open(encoding="utf-8") as handle:
        for line in handle:
            if line.startswith("#") or not line.strip():
                continue
            parts = line.rstrip("\n").split("\t")
            if parts[0] == "LIBRARY":
                continue
            rows[parts[0]] = parts[1:]
    return rows


def compare_metrics(picard_path: Path, turbo_path: Path, label: str) -> None:
    picard = load_metrics(picard_path)
    turbo = load_metrics(turbo_path)
    if picard != turbo:
        raise SystemExit(f"{label} metrics differ between Picard and turbo-picard")


def compare_sam_record_lines(
    picard_path: Path,
    turbo_path: Path,
    reference: str,
    label: str,
) -> None:
    picard_records = sam_records(picard_path, reference)
    turbo_records = sam_records(turbo_path, reference)
    if picard_records != turbo_records:
        raise SystemExit(f"{label} SAM/CRAM record text differs from Picard")


def compare_sam_records_ignoring_md_nm(
    picard_path: Path,
    turbo_path: Path,
    reference: str,
    label: str,
) -> None:
    def strip_md_nm(records: list[str]) -> list[str]:
        stripped = []
        for record in records:
            fields = record.split("\t")
            tags = [
                field
                for field in fields[11:]
                if not (field.startswith("MD:Z:") or field.startswith("NM:i:"))
            ]
            stripped.append("\t".join([*fields[:11], *tags]))
        return stripped

    picard_records = strip_md_nm(sam_records(picard_path, reference))
    turbo_records = strip_md_nm(sam_records(turbo_path, reference))
    if picard_records != turbo_records:
        raise SystemExit(f"{label} SAM/CRAM record text differs from Picard")


def compare_clean_sam_fields(picard_path: Path, turbo_path: Path, label: str) -> None:
    def records(path: Path) -> dict[str, tuple[str, str]]:
        data: dict[str, tuple[str, str]] = {}
        with path.open(encoding="utf-8") as handle:
            for line in handle:
                if line.startswith("@"):
                    continue
                fields = line.rstrip("\n").split("\t")
                data[fields[0]] = (fields[4], fields[5])
        return data

    if records(picard_path) != records(turbo_path):
        raise SystemExit(f"{label} MAPQ/CIGAR differs from Picard")


def compare_stable_sam_lines(picard_path: Path, turbo_path: Path, label: str) -> None:
    compare_real_data = _compare_real_data_module()

    def stable_lines(path: Path) -> list[bytes]:
        lines = []
        with path.open("rb") as handle:
            for raw in handle:
                line = raw.rstrip(b"\n")
                if not line.strip() or line.startswith(b"@PG"):
                    continue
                if line.startswith(b"@"):
                    line = compare_real_data.normalize_stable_sam_header(line)
                else:
                    line = compare_real_data.normalize_sam_record(line)
                lines.append(line)
        return lines

    if stable_lines(picard_path) != stable_lines(turbo_path):
        raise SystemExit(f"{label} stable SAM output differs from Picard")


def compare_stable_sam_lines_with_sorted_tags(
    picard_path: Path,
    turbo_path: Path,
    label: str,
) -> None:
    def stable_lines(path: Path) -> list[str]:
        lines = []
        with path.open(encoding="utf-8") as handle:
            for line in handle:
                line = line.rstrip("\n")
                if not line.strip() or line.startswith("@PG"):
                    continue
                if line.startswith("@"):
                    lines.append(line)
                    continue
                fields = line.split("\t")
                lines.append("\t".join([*fields[:11], *sorted(fields[11:])]))
        return lines

    if stable_lines(picard_path) != stable_lines(turbo_path):
        raise SystemExit(f"{label} stable SAM output differs from Picard")


def compare_binary_files(picard_path: Path, turbo_path: Path, label: str) -> None:
    if picard_path.read_bytes() != turbo_path.read_bytes():
        raise SystemExit(f"{label} binary output differs from Picard")


def compare_fastq_trio(
    picard_r1: Path,
    picard_r2: Path,
    picard_unpaired: Path,
    turbo_r1: Path,
    turbo_r2: Path,
    turbo_unpaired: Path,
    label: str,
) -> None:
    compare_real_data = _compare_real_data_module()
    picard_digest = compare_real_data.digest_files([picard_r1, picard_r2, picard_unpaired])
    turbo_digest = compare_real_data.digest_files([turbo_r1, turbo_r2, turbo_unpaired])
    if picard_digest != turbo_digest:
        raise SystemExit(f"{label} FASTQ trio digest differs from Picard")


def compare_merge_multiset(picard_path: Path, turbo_path: Path, label: str) -> None:
    compare_real_data = _compare_real_data_module()
    picard_digest = compare_real_data.digest_coordinate_sorted_sam_multiset(picard_path)
    turbo_digest = compare_real_data.digest_coordinate_sorted_sam_multiset(turbo_path)
    if picard_digest != turbo_digest:
        picard_records = Counter(normalized_sam_records(picard_path, compare_real_data))
        turbo_records = Counter(normalized_sam_records(turbo_path, compare_real_data))
        picard_only = next((record for record, count in (picard_records - turbo_records).items() if count), None)
        turbo_only = next((record for record, count in (turbo_records - picard_records).items() if count), None)
        if picard_only is not None:
            print(f"{label} first Picard-only record: {picard_only.decode('utf-8', 'replace')}", file=sys.stderr)
        if turbo_only is not None:
            print(f"{label} first turbo-only record: {turbo_only.decode('utf-8', 'replace')}", file=sys.stderr)
        raise SystemExit(f"{label} coordinate-sorted SAM multiset differs from Picard")


def normalized_sam_records(path: Path, compare_real_data) -> list[bytes]:
    records: list[bytes] = []
    with path.open("rb") as handle:
        for raw in handle:
            raw = raw.rstrip(b"\n")
            if raw.startswith(b"@"):
                continue
            records.append(compare_real_data.normalize_sam_record(raw))
    return records


def compare_validate_summary(picard_path: Path, turbo_path: Path, label: str) -> None:
    def counts(path: Path) -> dict[str, str]:
        rows: dict[str, str] = {}
        with path.open(encoding="utf-8") as handle:
            reader = csv.reader(handle, delimiter="\t")
            for row in reader:
                if not row or row[0].startswith("#"):
                    continue
                if len(row) >= 2:
                    rows[row[0]] = row[1]
        return rows

    if counts(picard_path) != counts(turbo_path):
        raise SystemExit(f"{label} ValidateSamFile summary differs from Picard")


def compare_markduplicates(
    repo_root: Path,
    picard_alignment: Path,
    turbo_alignment: Path,
    picard_metrics: Path,
    turbo_metrics: Path,
    label: str,
) -> None:
    completed = subprocess.run(
        [
            sys.executable,
            str(repo_root / "tools" / "compare_markduplicates.py"),
            "--picard-bam",
            str(picard_alignment),
            "--turbo-picard-bam",
            str(turbo_alignment),
            "--picard-metrics",
            str(picard_metrics),
            "--turbo-picard-metrics",
            str(turbo_metrics),
        ],
        cwd=repo_root,
        check=False,
        capture_output=True,
        text=True,
    )
    if completed.returncode != 0:
        detail = completed.stdout.strip() or completed.stderr.strip() or "unknown error"
        raise SystemExit(f"{label} semantic MarkDuplicates comparison failed: {detail}")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)

    metrics = subparsers.add_parser("metrics")
    metrics.add_argument("--label", required=True)
    metrics.add_argument("--picard", required=True, type=Path)
    metrics.add_argument("--turbo", required=True, type=Path)

    records = subparsers.add_parser("records")
    records.add_argument("--label", required=True)
    records.add_argument("--reference", required=True)
    records.add_argument("--picard", required=True, type=Path)
    records.add_argument("--turbo", required=True, type=Path)

    records_ignore_md_nm = subparsers.add_parser("records-ignore-md-nm")
    records_ignore_md_nm.add_argument("--label", required=True)
    records_ignore_md_nm.add_argument("--reference", required=True)
    records_ignore_md_nm.add_argument("--picard", required=True, type=Path)
    records_ignore_md_nm.add_argument("--turbo", required=True, type=Path)

    cleansam = subparsers.add_parser("cleansam")
    cleansam.add_argument("--label", required=True)
    cleansam.add_argument("--picard", required=True, type=Path)
    cleansam.add_argument("--turbo", required=True, type=Path)

    validate = subparsers.add_parser("validate-summary")
    validate.add_argument("--label", required=True)
    validate.add_argument("--picard", required=True, type=Path)
    validate.add_argument("--turbo", required=True, type=Path)

    stable_sam = subparsers.add_parser("stable-sam")
    stable_sam.add_argument("--label", required=True)
    stable_sam.add_argument("--picard", required=True, type=Path)
    stable_sam.add_argument("--turbo", required=True, type=Path)

    stable_sam_sorted_tags = subparsers.add_parser("stable-sam-sorted-tags")
    stable_sam_sorted_tags.add_argument("--label", required=True)
    stable_sam_sorted_tags.add_argument("--picard", required=True, type=Path)
    stable_sam_sorted_tags.add_argument("--turbo", required=True, type=Path)

    binary = subparsers.add_parser("binary")
    binary.add_argument("--label", required=True)
    binary.add_argument("--picard", required=True, type=Path)
    binary.add_argument("--turbo", required=True, type=Path)

    markdup = subparsers.add_parser("markduplicates")
    markdup.add_argument("--label", required=True)
    markdup.add_argument("--repo-root", required=True, type=Path)
    markdup.add_argument("--picard-alignment", required=True, type=Path)
    markdup.add_argument("--turbo-alignment", required=True, type=Path)
    markdup.add_argument("--picard-metrics", required=True, type=Path)
    markdup.add_argument("--turbo-metrics", required=True, type=Path)

    merge_multiset = subparsers.add_parser("merge-multiset")
    merge_multiset.add_argument("--label", required=True)
    merge_multiset.add_argument("--picard", required=True, type=Path)
    merge_multiset.add_argument("--turbo", required=True, type=Path)

    fastq_trio = subparsers.add_parser("fastq-trio")
    fastq_trio.add_argument("--label", required=True)
    fastq_trio.add_argument("--picard-r1", required=True, type=Path)
    fastq_trio.add_argument("--picard-r2", required=True, type=Path)
    fastq_trio.add_argument("--picard-unpaired", required=True, type=Path)
    fastq_trio.add_argument("--turbo-r1", required=True, type=Path)
    fastq_trio.add_argument("--turbo-r2", required=True, type=Path)
    fastq_trio.add_argument("--turbo-unpaired", required=True, type=Path)

    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.command == "metrics":
        compare_metrics(args.picard, args.turbo, args.label)
    elif args.command == "records":
        compare_sam_record_lines(args.picard, args.turbo, args.reference, args.label)
    elif args.command == "records-ignore-md-nm":
        compare_sam_records_ignoring_md_nm(
            args.picard,
            args.turbo,
            args.reference,
            args.label,
        )
    elif args.command == "cleansam":
        compare_clean_sam_fields(args.picard, args.turbo, args.label)
    elif args.command == "validate-summary":
        compare_validate_summary(args.picard, args.turbo, args.label)
    elif args.command == "stable-sam":
        compare_stable_sam_lines(args.picard, args.turbo, args.label)
    elif args.command == "stable-sam-sorted-tags":
        compare_stable_sam_lines_with_sorted_tags(args.picard, args.turbo, args.label)
    elif args.command == "binary":
        compare_binary_files(args.picard, args.turbo, args.label)
    elif args.command == "markduplicates":
        compare_markduplicates(
            args.repo_root,
            args.picard_alignment,
            args.turbo_alignment,
            args.picard_metrics,
            args.turbo_metrics,
            args.label,
        )
    elif args.command == "merge-multiset":
        compare_merge_multiset(args.picard, args.turbo, args.label)
    elif args.command == "fastq-trio":
        compare_fastq_trio(
            args.picard_r1,
            args.picard_r2,
            args.picard_unpaired,
            args.turbo_r1,
            args.turbo_r2,
            args.turbo_unpaired,
            args.label,
        )
    print(f"{args.label} parity check passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
