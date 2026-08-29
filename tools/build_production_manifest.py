#!/usr/bin/env python3
"""Build a production evidence manifest from a competitor-runner report.

The competitor runner owns raw measurements and parity comparison. This small
adapter turns that report into the repository's reviewed-manifest shape without
inventing missing provenance or promoting an incomplete run to release-ready
status.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
from pathlib import Path
from typing import Any


HEX40 = re.compile(r"^[0-9a-fA-F]{40}$")
SAM_TAG = re.compile(r"^[A-Za-z0-9]{2}$")
TIERS = {"release_candidate", "production_scale"}
REPRODUCTION_STATUSES = {"not_run", "pass", "fail"}
PROFILE_CHOICES = {
    "wgs_30x",
    "wes_capture",
    "rna_seq",
    "umi_panel",
    "cram_reference",
    "multi_library",
    "cohort_batch",
}


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--dataset-id", required=True)
    parser.add_argument("--scope-caveat", required=True)
    parser.add_argument("--turbo-picard-commit", required=True)
    parser.add_argument("--read-count", required=True, type=int)
    parser.add_argument("--tier", required=True, choices=sorted(TIERS))
    parser.add_argument("--compatibility-level", default="B", choices=["A", "B", "C"])
    parser.add_argument(
        "--independent-status",
        default="not_run",
        choices=sorted(REPRODUCTION_STATUSES),
    )
    parser.add_argument("--reviewer")
    parser.add_argument("--independent-host-profile")
    parser.add_argument("--independent-turbo-picard-commit")
    parser.add_argument("--independent-input-sha256")
    parser.add_argument("--independent-arguments-sha256")
    parser.add_argument("--evidence-url")
    parser.add_argument("--reference-fasta-sha256")
    return parser.parse_args(argv)


def load_report(path: Path) -> dict[str, Any]:
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise SystemExit(f"cannot read competitor report {path}: {exc}") from exc
    if not isinstance(payload, dict):
        raise SystemExit("competitor report root must be an object")
    return payload


def require_mapping(value: Any, label: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise SystemExit(f"{label} must be an object")
    return value


def require_text(mapping: dict[str, Any], key: str, label: str) -> str:
    value = mapping.get(key)
    if not isinstance(value, str) or not value.strip():
        raise SystemExit(f"{label}.{key} must be a non-empty string")
    return value.strip()


def require_non_negative_int(value: Any, label: str) -> int:
    if not isinstance(value, int) or isinstance(value, bool) or value < 0:
        raise SystemExit(f"{label} must be a non-negative integer")
    return value


def version_text(tool: dict[str, Any], label: str) -> str:
    version = require_mapping(tool.get("version"), f"tools.{label}.version")
    text = version.get("text")
    if not isinstance(text, str) or not text.strip():
        raise SystemExit(f"tools.{label}.version.text must be non-empty")
    exit_code = version.get("exit_code")
    picard_version_probe = (
        label == "picard"
        and exit_code == 1
        and re.match(r"^Version:\S+", text.strip()) is not None
    )
    if exit_code != 0 and not picard_version_probe:
        raise SystemExit(f"tools.{label}.version probe did not exit successfully")
    return text.strip().splitlines()[0]


def summary_value(summary: dict[str, Any], metric: str, statistic: str, label: str) -> float | int:
    metric_data = require_mapping(summary.get(metric), f"{label}.summary.{metric}")
    value = metric_data.get(statistic)
    if not isinstance(value, (int, float)) or isinstance(value, bool) or value < 0:
        raise SystemExit(f"{label}.summary.{metric}.{statistic} must be non-negative")
    return value


def arguments_sha256(report: dict[str, Any], turbo: dict[str, Any], picard: dict[str, Any]) -> str:
    protocol = require_mapping(report.get("protocol"), "protocol")
    payload = {
        "protocol": protocol,
        "turbo_command_template": turbo.get("command_template"),
        "turbo_environment_template": turbo.get("environment_template"),
        "picard_command_template": picard.get("command_template"),
        "picard_environment_template": picard.get("environment_template"),
    }
    encoded = json.dumps(payload, sort_keys=True, separators=(",", ":")).encode("utf-8")
    return hashlib.sha256(encoded).hexdigest()


def known_differences(parity: dict[str, Any]) -> list[str]:
    differences: list[str] = []
    if parity.get("alignment_mismatch") is not None:
        differences.append("alignment or duplicate-tag comparison mismatch")
    if parity.get("metrics_match") is False:
        differences.append("normalized DuplicationMetrics mismatch")
    if not differences and parity.get("reason"):
        differences.append(str(parity["reason"]))
    return differences


def build_manifest(args: argparse.Namespace, report: dict[str, Any]) -> dict[str, Any]:
    input_data = require_mapping(report.get("input"), "input")
    source_url = require_text(input_data, "source_url", "input")
    source_revision = require_text(input_data, "source_revision", "input")
    input_sha256 = require_text(input_data, "sha256", "input")
    if not re.fullmatch(r"[0-9a-fA-F]{64}", input_sha256):
        raise SystemExit("input.sha256 must be a 64-character hexadecimal SHA-256")
    input_bytes = require_non_negative_int(input_data.get("bytes"), "input.bytes")
    if args.tier == "production_scale" and input_bytes < 1:
        raise SystemExit("production_scale manifests require input.bytes greater than zero")
    if args.tier == "production_scale" and args.read_count < 1:
        raise SystemExit("production_scale manifests require --read-count greater than zero")
    input_format = str(
        input_data.get("format")
        or Path(str(input_data.get("path", "input.bam"))).suffix.lstrip(".")
        or "BAM"
    ).upper()
    if input_format not in {"BAM", "CRAM"}:
        raise SystemExit("input.format must be BAM or CRAM")
    reference_hash = args.reference_fasta_sha256
    report_reference = input_data.get("reference_fasta")
    if reference_hash is None and isinstance(report_reference, dict):
        candidate_hash = report_reference.get("sha256")
        if isinstance(candidate_hash, str):
            reference_hash = candidate_hash
    if reference_hash is not None and not re.fullmatch(r"[0-9a-fA-F]{64}", reference_hash):
        raise SystemExit("reference FASTA SHA-256 must be a 64-character hexadecimal SHA-256")
    if input_format == "CRAM" and reference_hash is None:
        raise SystemExit("CRAM production evidence requires a reference FASTA SHA-256")

    tools = require_mapping(report.get("tools"), "tools")
    turbo = require_mapping(tools.get("turbo-picard"), "tools.turbo-picard")
    picard = require_mapping(tools.get("picard"), "tools.picard")
    for label, tool in (("turbo-picard", turbo), ("picard", picard)):
        if tool.get("status") != "complete":
            raise SystemExit(f"tools.{label}.status must be complete")
        summary = require_mapping(tool.get("summary"), f"tools.{label}.summary")
        successful_repeats = summary.get("successful_repeats")
        if not isinstance(successful_repeats, int) or successful_repeats < 1:
            raise SystemExit(f"tools.{label}.summary.successful_repeats must be positive")

    turbo_parity = require_mapping(turbo.get("parity"), "tools.turbo-picard.parity")
    parity_status = turbo_parity.get("status")
    if parity_status not in {"PASS", "FAIL", "NOT_RUN"}:
        raise SystemExit("tools.turbo-picard.parity.status is invalid")
    comparator = turbo_parity.get("comparator") or "record identity and duplicate flags/tags"
    if not isinstance(comparator, str) or not comparator.strip():
        raise SystemExit("tools.turbo-picard.parity.comparator must be non-empty")

    protocol = require_mapping(report.get("protocol"), "protocol")
    repeats = protocol.get("repeats")
    if not isinstance(repeats, int) or repeats < 1:
        raise SystemExit("protocol.repeats must be positive")
    profile = protocol.get("profile")
    barcode_tags = {
        key: protocol.get(key)
        for key in ("barcode_tag", "read_one_barcode_tag", "read_two_barcode_tag")
        if protocol.get(key) is not None
    }
    for key, value in barcode_tags.items():
        if not isinstance(value, str) or not SAM_TAG.fullmatch(value):
            raise SystemExit(f"protocol.{key} must be a two-character SAM tag")
    if profile is not None:
        if profile not in PROFILE_CHOICES:
            raise SystemExit(f"protocol.profile must be one of {sorted(PROFILE_CHOICES)}")
        if profile == "umi_panel" and not barcode_tags:
            raise SystemExit("protocol.profile=umi_panel requires a barcode tag")
        if profile == "cram_reference" and input_format != "CRAM":
            raise SystemExit("protocol.profile=cram_reference requires CRAM input")
    elif args.tier == "production_scale":
        raise SystemExit("production_scale manifests require protocol.profile")

    turbo_summary = require_mapping(turbo.get("summary"), "tools.turbo-picard.summary")
    picard_summary = require_mapping(picard.get("summary"), "tools.picard.summary")
    command = {
        "name": "MarkDuplicates",
        "arguments_sha256": arguments_sha256(report, turbo, picard),
        "compatibility_level": args.compatibility_level,
        "comparator": comparator.strip(),
        "parity": parity_status,
        "repeats": repeats,
        "wall_seconds": {
            "picard_median": summary_value(picard_summary, "wall_seconds", "median", "tools.picard"),
            "picard_p95": summary_value(picard_summary, "wall_seconds", "p95", "tools.picard"),
            "turbo_picard_median": summary_value(turbo_summary, "wall_seconds", "median", "tools.turbo-picard"),
            "turbo_picard_p95": summary_value(turbo_summary, "wall_seconds", "p95", "tools.turbo-picard"),
        },
        "peak_rss_bytes": {
            "picard_max": summary_value(picard_summary, "peak_rss_bytes", "max", "tools.picard"),
            "turbo_picard_max": summary_value(turbo_summary, "peak_rss_bytes", "max", "tools.turbo-picard"),
        },
        "temporary_disk_bytes": {
            "picard_max": summary_value(picard_summary, "temporary_disk_peak_bytes", "max", "tools.picard"),
            "turbo_picard_max": summary_value(turbo_summary, "temporary_disk_peak_bytes", "max", "tools.turbo-picard"),
        },
        "known_differences": known_differences(turbo_parity),
    }
    if profile is not None:
        command["profile"] = profile
    if barcode_tags:
        command["barcode_tags"] = barcode_tags

    host_report = require_mapping(report.get("host"), "host")
    host = {
        "os": require_text(host_report, "os", "host"),
        "architecture": require_text(host_report, "architecture", "host"),
        "cpu_model": require_text(host_report, "cpu_model", "host"),
        "logical_cpus": require_non_negative_int(host_report.get("logical_cpus"), "host.logical_cpus"),
        "memory_bytes": require_non_negative_int(host_report.get("memory_bytes", 0), "host.memory_bytes"),
        "storage": require_text(
            host_report,
            "storage_note",
            "host",
        ),
    }
    if parity_status == "FAIL" and not command["known_differences"]:
        raise SystemExit("failed parity requires a known difference")

    turbo_commit = args.turbo_picard_commit.strip()
    if not HEX40.fullmatch(turbo_commit):
        raise SystemExit("--turbo-picard-commit must be a full 40-character commit SHA")
    independent_status = args.independent_status
    independent_host_profile = getattr(args, "independent_host_profile", None)
    independent_turbo_commit = getattr(args, "independent_turbo_picard_commit", None)
    independent_input_sha256 = getattr(args, "independent_input_sha256", None)
    independent_arguments_sha256 = getattr(args, "independent_arguments_sha256", None)
    if independent_status in {"pass", "fail"}:
        required_independent = {
            "reviewer": args.reviewer,
            "independent-host-profile": independent_host_profile,
            "evidence-url": args.evidence_url,
            "independent-turbo-picard-commit": independent_turbo_commit,
            "independent-input-sha256": independent_input_sha256,
            "independent-arguments-sha256": independent_arguments_sha256,
        }
        missing = sorted(key for key, value in required_independent.items() if not value)
        if missing:
            raise SystemExit(
                "independent reproduction status requires: " + ", ".join(missing)
            )
        if not str(independent_host_profile).strip() or not str(args.reviewer).strip():
            raise SystemExit("independent reviewer and host profile must be non-empty")
        if not str(args.evidence_url).startswith(("https://", "s3://", "gs://")):
            raise SystemExit("--evidence-url must be an immutable HTTPS, S3, or GCS URL")
        if not HEX40.fullmatch(str(independent_turbo_commit)):
            raise SystemExit(
                "--independent-turbo-picard-commit must be a full 40-character commit SHA"
            )
        if str(independent_turbo_commit).lower() != turbo_commit.lower():
            raise SystemExit(
                "--independent-turbo-picard-commit must match --turbo-picard-commit"
            )
        if not re.fullmatch(r"[0-9a-fA-F]{64}", str(independent_input_sha256)):
            raise SystemExit("--independent-input-sha256 must be a 64-character SHA-256")
        if str(independent_input_sha256).lower() != input_sha256.lower():
            raise SystemExit("--independent-input-sha256 must match the report input SHA-256")
        if not re.fullmatch(r"[0-9a-fA-F]{64}", str(independent_arguments_sha256)):
            raise SystemExit("--independent-arguments-sha256 must be a 64-character SHA-256")
        if str(independent_arguments_sha256).lower() != command["arguments_sha256"].lower():
            raise SystemExit(
                "--independent-arguments-sha256 must match the generated command protocol hash"
            )
    reproduction = {
        "status": independent_status,
        "reviewer": args.reviewer,
        "host_profile": independent_host_profile,
        "evidence_url": args.evidence_url,
        "turbo_picard_commit": independent_turbo_commit,
        "input_sha256": independent_input_sha256,
        "arguments_sha256": independent_arguments_sha256,
    }
    return {
        "schema_version": 1,
        "dataset_id": args.dataset_id,
        "tier": args.tier,
        "scope_caveat": args.scope_caveat,
        "input": {
            "format": input_format,
            "source_url": source_url,
            "source_revision": source_revision,
            "sha256": input_sha256,
            "bytes": input_bytes,
            "read_count": args.read_count,
            "reference_fasta_sha256": reference_hash,
        },
        "software": {
            "picard_version": version_text(picard, "picard"),
            "turbo_picard_version": version_text(turbo, "turbo-picard"),
            "turbo_picard_commit": turbo_commit,
        },
        "host": host,
        "commands": [command],
        "independent_reproduction": reproduction,
    }


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    if args.read_count < 0:
        raise SystemExit("--read-count must be non-negative")
    if not args.dataset_id.strip():
        raise SystemExit("--dataset-id must be non-empty")
    if not args.scope_caveat.strip():
        raise SystemExit("--scope-caveat must be non-empty")
    report = load_report(args.report)
    manifest = build_manifest(args, report)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"wrote {args.output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
