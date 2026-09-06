#!/usr/bin/env python3
"""Verify checked-in real-data comparison evidence and citations."""

from __future__ import annotations

import hashlib
import json
import decimal
import re
import sys
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Iterable
from urllib.parse import urlparse


ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "benchmarks" / "real-data" / "manifest.json"
README = ROOT / "benchmarks" / "README.md"
PROJECT_README = ROOT / "README.md"
SITE = ROOT / "docs" / "site" / "index.html"
BENCHMARK_DOCS = ROOT / "docs" / "benchmarks.rst"
ADOPTION_DOCS = ROOT / "docs" / "adoption.rst"
RELEASE_CANDIDATE_MIN_BYTES = 1_000_000
CRAM_RELEASE_CANDIDATE_MIN_BYTES = 500_000
RELEASE_CANDIDATE_PORTFOLIO_MIN_BYTES = 10_000_000
RELEASE_CANDIDATE_REQUIRED_COMMANDS = {
    "ViewSam",
    "CleanSam",
    "CollectQualityYieldMetrics",
    "CollectAlignmentSummaryMetrics",
    "MarkDuplicates",
}
CRAM_RELEASE_CANDIDATE_REQUIRED_COMMANDS = {
    "CleanSam",
    "CollectQualityYieldMetrics",
    "CollectInsertSizeMetrics",
    "MarkDuplicates",
    "SortSam",
    "AddOrReplaceReadGroups",
}
RELEASE_CANDIDATE_PORTFOLIO_REQUIRED_COMMANDS = {
    "AddOrReplaceReadGroups",
    "BuildBamIndex",
    "CleanSam",
    "CollectAlignmentSummaryMetrics",
    "CollectQualityYieldMetrics",
    "CollectInsertSizeMetrics",
    "MarkDuplicates",
    "RevertSam",
    "SamToFastq",
    "SortSam",
    "ValidateSamFile",
    "ViewSam",
}
RELEASE_CANDIDATE_PORTFOLIO_COMMAND_TEXT = ", ".join(
    sorted(RELEASE_CANDIDATE_PORTFOLIO_REQUIRED_COMMANDS)
)
EXPECTED_PICARD_VERSION = "Version:3.4.0"
KNOWN_COMPARISONS = {
    "BAI binary digest",
    "FASTQ trio digest",
    "SAM record digest",
    "SAM record digest plus read-group header digest",
    "coordinate-sorted SAM record multiset digest",
    "duplicate-marking semantic digest plus stable metrics digest",
    "post-command SAM record digest",
    "replacement header lines and record order digest",
    "reverted SAM record digest",
    "stable metrics digest",
    "stable metrics digest with insert-size histogram",
    "stable SAM digest after queryname sort and mate fixing",
    "stable SAM digest with NM/MD/UQ tags",
    "summary validation histogram plus exit code",
}
COMPARISON_MARKDOWN_NOTES = {
    "BAI binary digest": ("exact BAM index bytes", "BAI binary digest explanation"),
    "FASTQ trio digest": ("FASTQ outputs byte-for-byte", "FASTQ trio digest explanation"),
    "SAM record digest": ("ignores headers", "SAM record digest explanation"),
    "SAM record digest plus read-group header digest": (
        "sorted @RG header fields",
        "read-group header digest explanation",
    ),
    "coordinate-sorted SAM record multiset digest": (
        "tie-order differences",
        "coordinate sorted digest explanation",
    ),
    "duplicate-marking semantic digest plus stable metrics digest": (
        "duplicate flags",
        "duplicate-marking digest explanation",
    ),
    "post-command SAM record digest": (
        "after a BAM-writing command",
        "post-command SAM digest explanation",
    ),
    "reverted SAM record digest": (
        "RevertSam rewrites aligned records",
        "reverted SAM digest explanation",
    ),
    "replacement header lines and record order digest": (
        "replacement @HD/@SQ/@CO header lines and record name order",
        "ReplaceSamHeader digest explanation",
    ),
    "stable SAM digest after queryname sort and mate fixing": (
        "incidental @PG lines are ignored",
        "FixMateInformation stable SAM digest explanation",
    ),
    "stable SAM digest with NM/MD/UQ tags": (
        "incidental @PG lines are ignored",
        "SetNmMdAndUqTags stable SAM digest explanation",
    ),
    "stable metrics digest": (
        "generated headers do not affect parity",
        "stable metrics digest explanation",
    ),
    "stable metrics digest with insert-size histogram": (
        "generated headers do not affect parity",
        "insert-size metrics digest explanation",
    ),
    "summary validation histogram plus exit code": (
        "same Picard and turbo-picard exit code",
        "ValidateSamFile exit-code explanation",
    ),
}
OVERCLAIM_PHRASES = [
    "drop-in replacement",
    "production genomics workflows",
    "validated for all cohorts",
    "safe for all cohorts",
    "safe for all production",
    "proves safe to switch",
    "complete cohort-scale validation",
]


@dataclass(frozen=True, order=True)
class MarkDuplicateRecord:
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


def format_seconds(value: object) -> str | None:
    if not isinstance(value, (int, float)):
        return None
    return f"{float(value):.3f}s"


def format_speedup(value: object) -> str | None:
    if not isinstance(value, (int, float)):
        return None
    return f"{float(value):.2f}x"


def prose_text(text: str) -> str:
    return re.sub(r"\s+", " ", text)


def digest_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def digest_sam_records(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for raw in handle:
            if raw.startswith(b"@"):
                continue
            digest.update(normalize_sam_record(raw.rstrip(b"\n")))
            digest.update(b"\n")
    return digest.hexdigest()


def digest_coordinate_sorted_sam_multiset(path: Path) -> str:
    records: list[tuple[tuple[int, int], bytes]] = []
    contig_order: dict[bytes, int] = {}
    with path.open("rb") as handle:
        for raw in handle:
            if raw.startswith(b"@SQ\t"):
                fields = raw.rstrip(b"\n").split(b"\t")
                for field in fields:
                    if field.startswith(b"SN:"):
                        contig_order.setdefault(field.removeprefix(b"SN:"), len(contig_order))
                        break
                continue
            if raw.startswith(b"@"):
                continue
            normalized = normalize_sam_record(raw.rstrip(b"\n"))
            fields = normalized.split(b"\t")
            if len(fields) < 4:
                return ""
            if fields[2] == b"*":
                tid = 1_000_000_000
            elif fields[2] in contig_order:
                tid = contig_order[fields[2]]
            else:
                return ""
            try:
                pos = int(fields[3])
            except ValueError:
                return ""
            records.append(((tid, pos), normalized))
    sort_keys = [sort_key for sort_key, _record in records]
    if sort_keys != sorted(sort_keys):
        return ""
    digest = hashlib.sha256()
    for _sort_key, record in sorted(records, key=lambda item: item[1]):
        digest.update(record)
        digest.update(b"\n")
    return digest.hexdigest()


def normalize_sam_record(raw: bytes) -> bytes:
    fields = raw.split(b"\t")
    if len(fields) <= 11:
        return raw
    return b"\t".join([*fields[:11], *sorted(normalize_sam_tag(tag) for tag in fields[11:])])


def normalize_sam_tag(tag: bytes) -> bytes:
    parts = tag.split(b":", 2)
    if len(parts) != 3 or parts[1] != b"f":
        return tag
    try:
        value = decimal.Decimal(parts[2].decode("ascii"))
    except (decimal.InvalidOperation, UnicodeDecodeError):
        return tag
    return b":".join([parts[0], parts[1], format(value.normalize(), "f").encode("ascii")])


def digest_stable_text(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for raw in handle:
            stripped = raw.strip()
            if not stripped or stripped.startswith(b"#"):
                continue
            digest.update(stripped)
            digest.update(b"\n")
    return digest.hexdigest()


def digest_stable_sam(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for raw in handle:
            stripped = raw.strip()
            if not stripped or stripped.startswith(b"@PG"):
                continue
            digest.update(stripped)
            digest.update(b"\n")
    return digest.hexdigest()


def digest_replace_sam_header(path: Path) -> str:
    digest = hashlib.sha256()
    header_lines: list[bytes] = []
    record_names: list[bytes] = []
    with path.open("rb") as handle:
        for raw in handle:
            raw = raw.rstrip(b"\n")
            if raw.startswith(b"@"):
                header_lines.append(raw)
            elif raw:
                record_names.append(raw.split(b"\t", 1)[0])
    for row in header_lines:
        digest.update(row)
        digest.update(b"\n")
    for row in record_names:
        digest.update(row)
        digest.update(b"\n")
    return digest.hexdigest()


def digest_markduplicates_semantics(path: Path) -> str:
    digest = hashlib.sha256()
    for record in sorted(parse_markduplicates_records(path)):
        digest.update(json.dumps(asdict(record), sort_keys=True).encode("utf-8"))
        digest.update(b"\n")
    return digest.hexdigest()


def parse_markduplicates_records(path: Path) -> Iterable[MarkDuplicateRecord]:
    with path.open("r", encoding="utf-8") as handle:
        for raw in handle:
            if not raw.strip() or raw.startswith("@"):
                continue
            fields = raw.rstrip("\n").split("\t")
            if len(fields) < 11:
                raise ValueError(f"malformed SAM record in {path}: {raw.rstrip()}")
            flag = int(fields[1])
            tags = fields[11:]
            yield MarkDuplicateRecord(
                query_name=fields[0],
                duplicate=bool(flag & 0x400),
                duplicate_type=optional_tag(tags, "DT"),
                duplicate_set_size=optional_int_tag(tags, "DS"),
                duplicate_set_index=optional_int_tag(tags, "DI"),
                rx_barcode=optional_tag(tags, "RX"),
                bx_barcode=optional_tag(tags, "BX"),
                by_barcode=optional_tag(tags, "BY"),
                reference_name=fields[2],
                position=int(fields[3]),
                mate_reference_name=fields[6],
                mate_position=int(fields[7]),
                cigar=fields[5],
                template_length=int(fields[8]),
            )


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


def markduplicates_sidecars(path: Path) -> tuple[Path, Path]:
    return path.with_suffix(".view.sam"), path.with_name(f"{path.stem}.metrics.txt")


def recomputable_artifact_digest(
    path: Path,
    comparison: str,
    exit_code: int | None = None,
) -> str | None:
    if path.suffix == ".sam" and comparison in {
        "SAM record digest",
        "post-command SAM record digest",
    }:
        return digest_sam_records(path)
    if path.suffix == ".sam" and comparison == "coordinate-sorted SAM record multiset digest":
        return digest_coordinate_sorted_sam_multiset(path)
    if path.suffix == ".txt" and comparison.startswith("stable metrics digest"):
        return digest_stable_text(path)
    if comparison == "duplicate-marking semantic digest plus stable metrics digest":
        view_sam, metrics = markduplicates_sidecars(path)
        if view_sam.exists() and metrics.exists():
            return f"{digest_markduplicates_semantics(view_sam)};metrics={digest_stable_text(metrics)}"
    if path.suffix == ".sam" and comparison in {
        "stable SAM digest after queryname sort and mate fixing",
        "stable SAM digest with NM/MD/UQ tags",
    }:
        return digest_stable_sam(path)
    if path.suffix == ".sam" and comparison == "replacement header lines and record order digest":
        return digest_replace_sam_header(path)
    if comparison == "summary validation histogram plus exit code":
        if exit_code is None:
            return None
        return digest_validate_sam_summary(path, exit_code)
    return None


def digest_validate_sam_summary(path: Path, exit_code: int) -> str:
    digest = hashlib.sha256()
    digest.update(f"exit={exit_code}\n".encode("ascii"))
    if path.exists():
        with path.open("rb") as handle:
            for raw in handle:
                stripped = raw.strip()
                if not stripped or stripped.startswith(b"#"):
                    continue
                digest.update(stripped)
                digest.update(b"\n")
    else:
        digest.update(f"missing:{path.name}\n".encode("utf-8"))
    return digest.hexdigest()


def required_markdown_comparison_notes(comparisons: set[str]) -> list[tuple[str, str]]:
    notes: list[tuple[str, str]] = [("## Comparison details", "comparison details section")]
    for comparison in sorted(comparisons):
        note = COMPARISON_MARKDOWN_NOTES.get(comparison)
        if note is not None and note not in notes:
            notes.append(note)
    return notes


def load_manifest(path: Path = MANIFEST) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def validate_source_citation(dataset_id: str, source_url: str, source_commit: str) -> list[str]:
    errors: list[str] = []
    parsed = urlparse(source_url)
    if parsed.scheme != "https" or not parsed.netloc:
        errors.append(f"{dataset_id} source_url must be an https URL")
    if source_commit in {"develop", "main", "master"}:
        errors.append(f"{dataset_id} source_commit is not pinned")
    if not source_commit or len(source_commit) < 3:
        errors.append(f"{dataset_id} source_commit is too short to identify a source")

    if parsed.netloc == "raw.githubusercontent.com":
        errors.append(
            f"{dataset_id} source_url must not use raw.githubusercontent.com moving branch URLs"
        )
    if parsed.netloc == "github.com":
        if not re.fullmatch(r"[0-9a-f]{40}", source_commit):
            errors.append(
                f"{dataset_id} GitHub source_commit must be a full 40-character SHA"
            )
        marker = f"/blob/{source_commit}/"
        if marker not in parsed.path:
            errors.append(
                f"{dataset_id} GitHub source_url must include /blob/{source_commit}/"
            )
    elif source_commit and source_commit not in source_url:
        errors.append(
            f"{dataset_id} non-GitHub source_url must include source_commit/accession identifier"
        )
    return errors


def validate_manifest_path(dataset_id: str, key: str, value: object) -> list[str]:
    if not isinstance(value, str) or not value:
        return [f"{dataset_id} {key} must be a non-empty repository-relative path"]
    path = Path(value)
    if path.is_absolute():
        return [f"{dataset_id} {key} must be repository-relative: {value}"]
    if ".." in path.parts:
        return [f"{dataset_id} {key} must not contain path traversal: {value}"]
    try:
        path.relative_to("benchmarks/real-data")
    except ValueError:
        return [f"{dataset_id} {key} must stay under benchmarks/real-data: {value}"]
    if key == "evidence_json":
        if path.parent.name != "evidence" or path.name != "real-data-comparison.json":
            return [
                f"{dataset_id} evidence_json must use evidence/real-data-comparison.json: {value}"
            ]
    if key == "evidence_markdown":
        if path.parent.name != "evidence" or path.name != "real-data-comparison.md":
            return [
                f"{dataset_id} evidence_markdown must use evidence/real-data-comparison.md: {value}"
            ]
    return []


def validate_manifest(manifest: dict) -> list[str]:
    errors = []
    datasets = manifest.get("datasets")
    if not isinstance(datasets, list) or not datasets:
        return ["real-data manifest has no datasets"]

    seen = set()
    required = {
        "id",
        "input_path",
        "evidence_json",
        "evidence_markdown",
        "source_url",
        "source_commit",
        "sha256",
        "scope_caveat",
        "release_tier",
        "expected_commands",
    }
    for dataset in datasets:
        dataset_id = dataset.get("id", "<missing>")
        missing = sorted(required - dataset.keys())
        for key in missing:
            errors.append(f"{dataset_id} missing manifest key: {key}")
        if dataset_id in seen:
            errors.append(f"duplicate real-data dataset id: {dataset_id}")
        seen.add(dataset_id)
        errors.extend(
            validate_source_citation(
                dataset_id,
                str(dataset.get("source_url", "")),
                str(dataset.get("source_commit", "")),
            )
        )
        for key in ("input_path", "evidence_json", "evidence_markdown"):
            errors.extend(validate_manifest_path(dataset_id, key, dataset.get(key)))
        if not re.fullmatch(r"[0-9a-f]{64}", str(dataset.get("sha256", ""))):
            errors.append(f"{dataset_id} sha256 must be a lowercase 64-character hex digest")
        expected_commands = dataset.get("expected_commands")
        if not isinstance(expected_commands, dict) or not expected_commands:
            errors.append(f"{dataset_id} has no expected commands")
        elif any(
            not isinstance(command, str)
            or not command
            or not isinstance(comparison, str)
            or not comparison
            for command, comparison in expected_commands.items()
        ):
            errors.append(
                f"{dataset_id} expected_commands must map non-empty command names to non-empty comparison labels"
            )
        elif unknown_comparisons := sorted(
            {
                comparison
                for comparison in expected_commands.values()
                if comparison not in KNOWN_COMPARISONS
            }
        ):
            errors.append(
                f"{dataset_id} expected_commands use unknown comparison labels: "
                + ", ".join(unknown_comparisons)
            )
        if dataset.get("release_tier") not in {"public_smoke", "release_candidate"}:
            errors.append(f"{dataset_id} has invalid release_tier")
    return errors


def validate_release_candidate_dataset(dataset: dict, input_summary: dict) -> list[str]:
    errors: list[str] = []
    if dataset.get("release_tier") != "release_candidate":
        return errors
    dataset_id = dataset["id"]
    source_url = str(dataset.get("source_url", ""))
    source_commit = str(dataset.get("source_commit", ""))
    if urlparse(source_url).netloc == "github.com" and not re.fullmatch(
        r"[0-9a-f]{40}", source_commit
    ):
        errors.append(
            f"{dataset_id} release_candidate GitHub source_commit must be a full 40-character SHA"
        )
    expected_commands = set(dataset.get("expected_commands", {}))
    required_commands = RELEASE_CANDIDATE_REQUIRED_COMMANDS
    if str(dataset.get("input_path", "")).endswith(".cram"):
        required_commands = CRAM_RELEASE_CANDIDATE_REQUIRED_COMMANDS
    missing_commands = sorted(required_commands - expected_commands)
    if missing_commands:
        errors.append(
            f"{dataset_id} release_candidate missing required commands: "
            + ", ".join(missing_commands)
        )
    if "minimum_input_bytes" not in dataset:
        errors.append(f"{dataset_id} release_candidate missing minimum_input_bytes")
    default_min_bytes = (
        CRAM_RELEASE_CANDIDATE_MIN_BYTES
        if str(dataset.get("input_path", "")).endswith(".cram")
        else RELEASE_CANDIDATE_MIN_BYTES
    )
    min_bytes = dataset.get("minimum_input_bytes", default_min_bytes)
    if not isinstance(min_bytes, int) or min_bytes <= 0:
        errors.append(f"{dataset_id} release_candidate minimum_input_bytes must be a positive integer")
        min_bytes = default_min_bytes
    size_bytes = int(input_summary.get("size_bytes", 0))
    if size_bytes < min_bytes:
        errors.append(
            f"{dataset_id} release_candidate input too small: {size_bytes} bytes < {min_bytes}"
        )
    return errors


def validate_manifest_entry_artifact(dataset: dict) -> list[str]:
    errors: list[str] = []
    dataset_id = dataset["id"]
    entry_path = ROOT / dataset["evidence_json"]
    entry_path = entry_path.parent / "manifest-entry.json"
    if not entry_path.exists():
        if dataset.get("release_tier") == "release_candidate":
            errors.append(f"{dataset_id} release_candidate missing manifest-entry.json")
        return errors
    try:
        entry = json.loads(entry_path.read_text(encoding="utf-8"))
    except ValueError:
        return [f"{dataset_id} manifest-entry.json is not valid JSON"]
    if entry != dataset:
        errors.append(f"{dataset_id} manifest-entry.json does not match manifest entry")
    return errors


def validate_dataset(
    dataset: dict,
    readme: str,
    site: str,
    benchmark_docs: str = "",
) -> list[str]:
    errors = []
    dataset_id = dataset["id"]
    input_path = ROOT / dataset["input_path"]
    evidence_json_path = ROOT / dataset["evidence_json"]
    evidence_markdown_path = ROOT / dataset["evidence_markdown"]

    if not input_path.exists():
        errors.append(f"{dataset_id} missing input: {dataset['input_path']}")
        return errors
    if not evidence_json_path.exists():
        errors.append(f"{dataset_id} missing evidence JSON: {dataset['evidence_json']}")
        return errors
    if not evidence_markdown_path.exists():
        errors.append(f"{dataset_id} missing evidence Markdown: {dataset['evidence_markdown']}")
        return errors

    data = json.loads(evidence_json_path.read_text(encoding="utf-8"))
    if not isinstance(data, dict):
        errors.append(f"{dataset_id} evidence JSON must be an object")
        data = {}
    markdown = evidence_markdown_path.read_text(encoding="utf-8")
    input_summary = data.get("input", {})
    if not isinstance(input_summary, dict):
        errors.append(f"{dataset_id} evidence input must be an object")
        input_summary = {}

    checks = [
        (data.get("parity") == "PASS", f"{dataset_id} evidence parity is not PASS"),
        (
            data.get("picard_version") == EXPECTED_PICARD_VERSION,
            f"{dataset_id} evidence Picard version changed",
        ),
        (
            isinstance(data.get("turbo_picard_version"), str)
            and data.get("turbo_picard_version", "").strip(),
            f"{dataset_id} evidence missing turbo-picard version",
        ),
        (
            input_summary.get("path") == dataset["input_path"],
            f"{dataset_id} evidence input path changed",
        ),
        (
            f"Input BAM: `{dataset['input_path']}`" in markdown,
            f"{dataset_id} Markdown missing input path",
        ),
        (input_summary.get("sha256") == dataset["sha256"], f"{dataset_id} evidence SHA-256 changed"),
        (digest_file(input_path) == dataset["sha256"], f"{dataset_id} local input SHA-256 changed"),
        (
            input_summary.get("size_bytes") == input_path.stat().st_size,
            f"{dataset_id} evidence input size changed",
        ),
        (
            input_summary.get("source_url") == dataset["source_url"],
            f"{dataset_id} evidence source URL changed",
        ),
        (
            input_summary.get("source_commit") == dataset["source_commit"],
            f"{dataset_id} evidence source commit changed",
        ),
        (
            f"Picard: `{EXPECTED_PICARD_VERSION}`" in markdown,
            f"{dataset_id} Markdown missing Picard version",
        ),
    ]
    for ok, message in checks:
        if not ok:
            errors.append(message)
    errors.extend(validate_release_candidate_dataset(dataset, input_summary))
    errors.extend(validate_manifest_entry_artifact(dataset))

    rows: dict[str, dict] = {}
    command_rows = data.get("commands", [])
    if not isinstance(command_rows, list):
        errors.append(f"{dataset_id} evidence commands must be a list")
        command_rows = []
    for index, row in enumerate(command_rows):
        if not isinstance(row, dict):
            errors.append(f"{dataset_id} evidence command row {index} must be an object")
            continue
        command = row.get("command")
        if not isinstance(command, str) or not command:
            errors.append(f"{dataset_id} evidence command row missing command name")
            continue
        if command in rows:
            errors.append(f"{dataset_id} duplicate command evidence: {command}")
        rows[command] = row
    extra_commands = sorted(set(rows) - set(dataset["expected_commands"]))
    for command in extra_commands:
        errors.append(f"{dataset_id} unreviewed extra command evidence: {command}")
    for command, comparison in dataset["expected_commands"].items():
        row = rows.get(command)
        if row is None:
            errors.append(f"{dataset_id} missing command evidence: {command}")
            continue
        if row.get("status") != "PASS":
            errors.append(f"{dataset_id} command did not pass: {command}")
        if row.get("comparison") != comparison:
            errors.append(f"{dataset_id} comparison changed for {command}: {row.get('comparison')}")
        turbo_digest = row.get("turbo_digest")
        picard_digest = row.get("picard_digest")
        if not isinstance(turbo_digest, str) or not turbo_digest:
            errors.append(f"{dataset_id} {command} missing turbo-picard digest")
        if not isinstance(picard_digest, str) or not picard_digest:
            errors.append(f"{dataset_id} {command} missing Picard digest")
        if (
            isinstance(turbo_digest, str)
            and turbo_digest
            and isinstance(picard_digest, str)
            and picard_digest
            and turbo_digest != picard_digest
        ):
            errors.append(f"{dataset_id} {command} turbo-picard/Picard digests differ")
        if comparison == "summary validation histogram plus exit code":
            for field, label in [
                ("turbo_exit_code", "turbo-picard exit code"),
                ("picard_exit_code", "Picard exit code"),
            ]:
                if not isinstance(row.get(field), int):
                    errors.append(f"{dataset_id} {command} missing {label}")
            if (
                isinstance(row.get("turbo_exit_code"), int)
                and isinstance(row.get("picard_exit_code"), int)
                and row["turbo_exit_code"] != row["picard_exit_code"]
            ):
                errors.append(f"{dataset_id} {command} turbo-picard/Picard exit codes differ")
        for artifact_key, label in [
            ("turbo_artifact", "turbo-picard artifact"),
            ("picard_artifact", "Picard artifact"),
        ]:
            artifact = row.get(artifact_key)
            if not isinstance(artifact, str) or not artifact:
                errors.append(f"{dataset_id} {command} missing {label}")
                continue
            if Path(artifact).is_absolute():
                errors.append(
                    f"{dataset_id} {command} {label} must be repository-relative: {artifact}"
                )
                continue
            artifact_path = ROOT / artifact
            try:
                artifact_path.resolve().relative_to(evidence_json_path.parent.resolve())
            except ValueError:
                errors.append(
                    f"{dataset_id} {command} {label} must stay under evidence directory: {artifact}"
                )
            if not artifact_path.exists():
                if "/evidence/work/" in artifact.replace("\\", "/"):
                    continue
                errors.append(f"{dataset_id} {command} missing {label} file: {artifact}")
                continue
            if comparison == "duplicate-marking semantic digest plus stable metrics digest":
                view_sam, metrics = markduplicates_sidecars(artifact_path)
                for sidecar_path, sidecar_label in [
                    (view_sam, "view-SAM sidecar"),
                    (metrics, "metrics sidecar"),
                ]:
                    if not sidecar_path.exists():
                        errors.append(
                            f"{dataset_id} {command} missing {label} {sidecar_label}: "
                            f"{sidecar_path.relative_to(ROOT)}"
                        )
                if not view_sam.exists() or not metrics.exists():
                    continue
            expected_digest = (
                turbo_digest if artifact_key == "turbo_artifact" else picard_digest
            )
            if isinstance(expected_digest, str) and expected_digest:
                exit_code = None
                if comparison == "summary validation histogram plus exit code":
                    exit_code_key = (
                        "turbo_exit_code"
                        if artifact_key == "turbo_artifact"
                        else "picard_exit_code"
                    )
                    exit_code = row.get(exit_code_key)
                    if not isinstance(exit_code, int):
                        exit_code = None
                artifact_digest = recomputable_artifact_digest(
                    artifact_path,
                    comparison,
                    exit_code,
                )
                if artifact_digest is not None and artifact_digest != expected_digest:
                    errors.append(
                        f"{dataset_id} {command} {label} digest does not match artifact"
                    )

        turbo_seconds = row.get("turbo_seconds")
        picard_seconds = row.get("picard_seconds")
        speedup = row.get("speedup")
        if not isinstance(turbo_seconds, (int, float)) or turbo_seconds <= 0:
            errors.append(f"{dataset_id} {command} missing positive turbo-picard timing")
        if not isinstance(picard_seconds, (int, float)) or picard_seconds <= 0:
            errors.append(f"{dataset_id} {command} missing positive Picard timing")
        if not isinstance(speedup, (int, float)) or speedup <= 0:
            errors.append(f"{dataset_id} {command} missing positive speedup")
        if (
            isinstance(turbo_seconds, (int, float))
            and turbo_seconds > 0
            and isinstance(picard_seconds, (int, float))
            and picard_seconds > 0
            and isinstance(speedup, (int, float))
            and speedup > 0
        ):
            expected_speedup = picard_seconds / turbo_seconds
            if abs(speedup - expected_speedup) > 0.005:
                errors.append(
                    f"{dataset_id} {command} speedup does not match timing ratio: "
                    f"{speedup:.4f} != {expected_speedup:.4f}"
                )

        markdown_row = f"| {command} | PASS | {comparison} |"
        readme_row = f"| {command} | PASS | {comparison} |"
        if markdown_row not in markdown:
            errors.append(f"{dataset_id} Markdown missing row: {command}")
        expected_timing_row = (
            f"| {command} | PASS | {comparison} | "
            f"{format_seconds(turbo_seconds)} | {format_seconds(picard_seconds)} | "
            f"{format_speedup(speedup)} |"
        )
        if "None" in expected_timing_row or expected_timing_row not in markdown:
            errors.append(f"{dataset_id} Markdown missing timing row: {command}")
        if readme_row not in readme:
            errors.append(f"{dataset_id} benchmarks README missing row: {command}")
        if command not in site:
            errors.append(f"{dataset_id} site missing command: {command}")

    expected_comparisons = set(dataset["expected_commands"].values())
    for needle, description in required_markdown_comparison_notes(expected_comparisons):
        if needle not in markdown:
            errors.append(f"{dataset_id} Markdown missing {description}")
    if "## Artifact digests" not in markdown:
        errors.append(f"{dataset_id} Markdown missing artifact digest section")
    for command, comparison in dataset["expected_commands"].items():
        row = rows.get(command)
        if not isinstance(row, dict):
            continue
        for artifact_key, description in [
            ("turbo_artifact", "turbo-picard artifact path"),
            ("picard_artifact", "Picard artifact path"),
        ]:
            artifact = row.get(artifact_key)
            if isinstance(artifact, str) and artifact and artifact not in markdown:
                errors.append(f"{dataset_id} Markdown missing {command} {description}")
        if comparison == "summary validation histogram plus exit code":
            for field, label in [
                ("turbo_exit_code", "turbo-picard exit code"),
                ("picard_exit_code", "Picard exit code"),
            ]:
                value = row.get(field)
                if isinstance(value, int) and f"`{value}`" not in markdown:
                    errors.append(f"{dataset_id} Markdown missing {command} {label}")

    for text, target, label in [
        (readme, "benchmarks README", "README"),
        (site, "site", "site"),
    ]:
        for needle, description in [
            (dataset["evidence_markdown"], "evidence Markdown path"),
        ]:
            if needle not in text:
                errors.append(f"{dataset_id} {target} missing {description}")
        if target == "benchmarks README":
            for needle, description in [
                (dataset["source_url"], "pinned source URL"),
                (dataset["source_commit"], "source commit"),
                (dataset["sha256"], "input SHA-256"),
                (dataset["scope_caveat"], "scope caveat"),
            ]:
                if needle not in text:
                    errors.append(f"{dataset_id} {target} missing {description}")
        else:
            for needle, description in [
                (dataset["source_commit"], "source commit"),
                (dataset["sha256"], "input SHA-256"),
            ]:
                if needle not in text:
                    errors.append(f"{dataset_id} {target} missing {description}")
        if label == "site" and "python3 tools/verify_real_data_evidence.py" not in text:
            errors.append(f"{dataset_id} site missing real-data verifier command")

    if dataset.get("release_tier") == "release_candidate":
        for text, target in [
            (readme, "benchmarks README"),
            (site, "site"),
        ]:
            threshold = str(dataset.get("minimum_input_bytes", ""))
            if threshold not in text:
                errors.append(f"{dataset_id} {target} missing minimum input threshold")
        for needle, description in [
            (dataset["source_url"], "pinned source URL"),
            (dataset["source_commit"], "source commit"),
            (dataset["sha256"], "input SHA-256"),
            (dataset["evidence_markdown"], "evidence Markdown path"),
            (dataset["scope_caveat"], "scope caveat"),
            (str(dataset.get("minimum_input_bytes", "")), "minimum input threshold"),
        ]:
            if needle not in benchmark_docs:
                errors.append(f"{dataset_id} benchmark docs missing {description}")
        for command in dataset.get("expected_commands", {}):
            if command not in benchmark_docs:
                errors.append(f"{dataset_id} benchmark docs missing command: {command}")

    return errors


def validate_workflow_docs(
    readme: str,
    site: str,
    benchmark_docs: str = "",
    adoption_docs: str = "",
) -> list[str]:
    errors: list[str] = []
    required = [
        ("python3 tools/compare_real_data.py", "real-data comparator command"),
        ("python3 tools/update_real_data_manifest.py", "manifest update command"),
        ("python3 tools/verify_real_data_evidence.py", "real-data verifier command"),
        (
            "python3 tools/verify_real_data_evidence.py --release-ready",
            "release-ready real-data verifier command",
        ),
        ("release_candidate", "release-candidate tier"),
        ("manifest-entry.json", "manifest-entry artifact"),
        ("/evidence/manifest-entry.json", "manifest-entry evidence-subdirectory path"),
    ]
    for text, target in ((readme, "benchmarks README"), (site, "site")):
        prose = prose_text(text)
        for needle, description in required:
            if needle not in text:
                errors.append(f"{target} missing {description}")
        for needle, description in [
            ("scientific release", "scientific release wording"),
            ("not proof for every dataset", "broad-dataset scope caveat"),
            ("12-command release set", "release command set requirement"),
            ("full 40-character Git commit SHA", "full Git commit citation rule"),
            ("one tiny fixture", "minimum input-size warning"),
        ]:
            if needle == "12-command release set" and (
                needle in prose or RELEASE_CANDIDATE_PORTFOLIO_COMMAND_TEXT in prose
            ):
                continue
            if needle not in prose:
                errors.append(f"{target} missing {description}")
        lower_text = text.lower()
        for phrase in OVERCLAIM_PHRASES:
            if phrase in lower_text:
                errors.append(f"{target} contains unsupported overclaim: {phrase}")
    if RELEASE_CANDIDATE_PORTFOLIO_COMMAND_TEXT not in prose_text(readme):
        errors.append(
            "benchmarks README missing release command set"
        )
    if benchmark_docs:
        benchmark_prose = prose_text(benchmark_docs)
        for needle, description in [
            ("python3 tools/verify_real_data_evidence.py", "real-data verifier command"),
            (
                "python3 tools/verify_real_data_evidence.py --release-ready",
                "release-ready real-data verifier command",
            ),
            ("python3 tools/verify_benchmark_thresholds.py", "benchmark threshold verifier command"),
            ("release evidence", "release evidence wording"),
            ("not proof for every dataset", "broad-dataset scope caveat"),
            (
                RELEASE_CANDIDATE_PORTFOLIO_COMMAND_TEXT,
                "release command set requirement",
            ),
            ("full 40-character Git commit SHA", "full Git commit citation rule"),
            ("one tiny fixture", "minimum input-size warning"),
        ]:
            if needle not in benchmark_prose:
                errors.append(f"benchmark docs missing {description}")
        lower_docs = benchmark_docs.lower()
        for phrase in OVERCLAIM_PHRASES:
            if phrase in lower_docs:
                errors.append(f"benchmark docs contains unsupported overclaim: {phrase}")
    if adoption_docs:
        adoption_prose = prose_text(adoption_docs)
        for needle, description in [
            ("change one command at a time", "one-command-at-a-time caveat"),
            ("Run it beside Picard first", "side-by-side comparison caveat"),
            ("python3 tools/verify_real_data_evidence.py --release-ready", "release-ready real-data verifier command"),
            ("not proof of every workflow", "broad-workflow scope caveat"),
            (
                RELEASE_CANDIDATE_PORTFOLIO_COMMAND_TEXT,
                "release command set requirement",
            ),
            ("full 40-character Git commit SHA", "full Git commit citation rule"),
        ]:
            if needle not in adoption_prose:
                errors.append(f"adoption docs missing {description}")
        lower_adoption = adoption_docs.lower()
        for phrase in OVERCLAIM_PHRASES:
            if phrase in lower_adoption:
                errors.append(f"adoption docs contains unsupported overclaim: {phrase}")
    return errors


def validate_project_readme_real_data_summary(manifest: dict, project_readme: str) -> list[str]:
    errors: list[str] = []
    lower_readme = project_readme.lower()
    for phrase in OVERCLAIM_PHRASES:
        if phrase in lower_readme:
            errors.append(f"project README contains unsupported overclaim: {phrase}")
    for needle, description in [
        ("benchmarks/real-data/", "real-data evidence directory"),
        (
            "https://turbo-picard.readthedocs.io/en/latest/benchmarks.html",
            "benchmark documentation link",
        ),
        ("SHA-256", "input SHA-256 guidance"),
        ("python3 tools/update_real_data_manifest.py", "manifest update command"),
        ("python3 tools/verify_real_data_evidence.py", "real-data verifier command"),
        (
            "python3 tools/verify_real_data_evidence.py --release-ready",
            "release-ready real-data verifier command",
        ),
    ]:
        if needle not in project_readme:
            errors.append(f"project README missing {description}")
    for dataset in manifest.get("datasets", []):
        if not isinstance(dataset, dict) or dataset.get("release_tier") != "release_candidate":
            continue
        dataset_id = str(dataset.get("id", "<missing>"))
        if dataset_id not in project_readme:
            errors.append(f"{dataset_id} project README missing dataset id")
    return errors


def validate_real_data_evidence(
    manifest: dict,
    readme: str,
    site: str,
    benchmark_docs: str = "",
    project_readme: str = "",
    adoption_docs: str = "",
    *,
    release_ready: bool = False,
) -> list[str]:
    errors = validate_manifest(manifest)
    if errors:
        return errors
    errors.extend(validate_workflow_docs(readme, site, benchmark_docs, adoption_docs))
    has_release_candidate = any(
        dataset.get("release_tier") == "release_candidate"
        for dataset in manifest["datasets"]
    )
    stale_release_ready_text = (
        "fails until the manifest contains at least one pinned release-candidate dataset"
    )
    if has_release_candidate and stale_release_ready_text in readme:
        errors.append(
            "benchmarks README still says release-ready verification fails before release candidates"
        )
    if release_ready and not has_release_candidate:
        errors.append(
            "real-data manifest has no release_candidate dataset for scientist-facing release"
        )
    if release_ready and has_release_candidate:
        release_candidate_commands = {
            command
            for dataset in manifest["datasets"]
            if dataset.get("release_tier") == "release_candidate"
            for command in dataset.get("expected_commands", {})
        }
        release_candidate_bytes = 0
        for dataset in manifest["datasets"]:
            if dataset.get("release_tier") != "release_candidate":
                continue
            evidence_json_path = ROOT / dataset.get("evidence_json", "")
            if not evidence_json_path.exists():
                continue
            try:
                evidence = json.loads(evidence_json_path.read_text(encoding="utf-8"))
            except ValueError:
                continue
            input_summary = evidence.get("input", {})
            if isinstance(input_summary, dict) and isinstance(
                input_summary.get("size_bytes"), int
            ):
                release_candidate_bytes += input_summary["size_bytes"]
        if release_candidate_bytes < RELEASE_CANDIDATE_PORTFOLIO_MIN_BYTES:
            errors.append(
                "real-data release_candidate portfolio input too small: "
                f"{release_candidate_bytes} bytes < "
                f"{RELEASE_CANDIDATE_PORTFOLIO_MIN_BYTES}"
            )
        missing_portfolio_commands = sorted(
            RELEASE_CANDIDATE_PORTFOLIO_REQUIRED_COMMANDS - release_candidate_commands
        )
        if missing_portfolio_commands:
            errors.append(
                "real-data release_candidate portfolio missing required command evidence: "
                + ", ".join(missing_portfolio_commands)
            )
        has_cram_release_candidate = any(
            dataset.get("release_tier") == "release_candidate"
            and str(dataset.get("input_path", "")).endswith(".cram")
            for dataset in manifest["datasets"]
        )
        if not has_cram_release_candidate:
            errors.append(
                "real-data manifest has no release_candidate CRAM dataset; "
                "run tools/bootstrap_gatk_mito_cram_evidence.sh"
            )
    if project_readme:
        errors.extend(validate_project_readme_real_data_summary(manifest, project_readme))
    for dataset in manifest["datasets"]:
        errors.extend(validate_dataset(dataset, readme, site, benchmark_docs))
    return errors


def main(argv: list[str] | None = None) -> int:
    argv = sys.argv[1:] if argv is None else argv
    allowed_args = {"--release-ready"}
    unknown_args = [arg for arg in argv if arg not in allowed_args]
    if unknown_args:
        print(
            f"usage: {Path(sys.argv[0]).name} [--release-ready]",
            file=sys.stderr,
        )
        return 2
    release_ready = "--release-ready" in argv
    manifest = load_manifest()
    readme = README.read_text(encoding="utf-8")
    project_readme = PROJECT_README.read_text(encoding="utf-8")
    # Provenance belongs on the linked evidence page, not in the marketing hero.
    site = SITE.read_text(encoding="utf-8") + (SITE.parent / "evidence/index.html").read_text(encoding="utf-8")
    benchmark_docs = BENCHMARK_DOCS.read_text(encoding="utf-8")
    adoption_docs = ADOPTION_DOCS.read_text(encoding="utf-8")
    errors = validate_real_data_evidence(
        manifest,
        readme,
        site,
        benchmark_docs,
        project_readme,
        adoption_docs,
        release_ready=release_ready,
    )
    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
