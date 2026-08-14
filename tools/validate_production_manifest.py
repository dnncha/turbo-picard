#!/usr/bin/env python3
"""Validate production-scale benchmark evidence manifests."""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

HEX64 = re.compile(r"^[0-9a-fA-F]{64}$")
HEX40 = re.compile(r"^[0-9a-fA-F]{40}$")
TIERS = {"smoke", "release_candidate", "production_scale", "independent_reproduction"}
LEVELS = {"A", "B", "C", "D", "X"}
PROFILES = {
    "wgs_30x",
    "wes_capture",
    "rna_seq",
    "umi_panel",
    "cram_reference",
    "multi_library",
    "cohort_batch",
}


class ManifestError(ValueError):
    pass


def require(mapping, keys, label):
    missing = sorted(key for key in keys if key not in mapping)
    if missing:
        raise ManifestError(f"{label} missing required field(s): {', '.join(missing)}")


def non_negative_int(value, label):
    if not isinstance(value, int) or isinstance(value, bool) or value < 0:
        raise ManifestError(f"{label} must be a non-negative integer")


def require_evidence_url(value, label):
    if not isinstance(value, str) or not value.strip():
        raise ManifestError(f"{label} must be a non-empty string")
    if not value.startswith(("https://", "s3://", "gs://")):
        raise ManifestError(f"{label} must be an immutable HTTPS, S3, or GCS URL")


def validate(path: Path, release_ready: bool = False) -> dict:
    try:
        payload = json.loads(path.read_text())
    except (OSError, json.JSONDecodeError) as exc:
        raise ManifestError(f"{path}: cannot read valid JSON: {exc}") from exc
    if not isinstance(payload, dict):
        raise ManifestError("manifest root must be an object")
    if payload.get("schema_version") != 1:
        raise ManifestError("schema_version must be 1")
    if not isinstance(payload.get("dataset_id"), str) or not payload["dataset_id"]:
        raise ManifestError("dataset_id must be a non-empty string")
    if payload.get("tier") not in TIERS:
        raise ManifestError(f"tier must be one of {sorted(TIERS)}")
    if not payload.get("scope_caveat"):
        raise ManifestError("scope_caveat must be explicit")

    input_data = payload.get("input")
    software = payload.get("software")
    host = payload.get("host")
    if not all(isinstance(value, dict) for value in (input_data, software, host)):
        raise ManifestError("input, software, and host must be objects")
    require(input_data, {"format", "source_url", "source_revision", "sha256", "bytes", "read_count"}, "input")
    require(software, {"picard_version", "turbo_picard_version", "turbo_picard_commit"}, "software")
    require(host, {"os", "architecture", "cpu_model", "logical_cpus", "memory_bytes", "storage"}, "host")

    if not HEX64.fullmatch(input_data["sha256"]):
        raise ManifestError("input.sha256 must be a 64-character hexadecimal SHA-256")
    input_format = str(input_data["format"]).upper()
    if input_format not in {"BAM", "CRAM"}:
        raise ManifestError("input.format must be BAM or CRAM")
    reference_hash = input_data.get("reference_fasta_sha256")
    if reference_hash is not None and not HEX64.fullmatch(str(reference_hash)):
        raise ManifestError("input.reference_fasta_sha256 must be a 64-character hexadecimal SHA-256")
    if input_format == "CRAM" and reference_hash is None:
        raise ManifestError("CRAM evidence requires input.reference_fasta_sha256")
    if not isinstance(input_data["source_url"], str) or not input_data["source_url"].startswith(("https://", "s3://", "gs://")):
        raise ManifestError("input.source_url must be an immutable HTTPS, S3, or GCS URL")
    non_negative_int(input_data["bytes"], "input.bytes")
    non_negative_int(input_data["read_count"], "input.read_count")
    if payload["tier"] in {"production_scale", "independent_reproduction"}:
        if input_data["bytes"] < 1:
            raise ManifestError("production-scale evidence requires input.bytes greater than zero")
        if input_data["read_count"] < 1:
            raise ManifestError("production-scale evidence requires input.read_count greater than zero")
    if not input_data["source_revision"]:
        raise ManifestError("input.source_revision must be an accession, release, or full commit")
    if not HEX40.fullmatch(software["turbo_picard_commit"]):
        raise ManifestError("software.turbo_picard_commit must be a full 40-character commit SHA")
    for key in ("picard_version", "turbo_picard_version"):
        if not isinstance(software[key], str) or not software[key]:
            raise ManifestError(f"software.{key} must be a non-empty string")
    for key in ("logical_cpus", "memory_bytes"):
        non_negative_int(host[key], f"host.{key}")
    for key in ("os", "architecture", "cpu_model", "storage"):
        if not host[key]:
            raise ManifestError(f"host.{key} is required")

    commands = payload.get("commands")
    if not isinstance(commands, list) or not commands:
        raise ManifestError("commands must be a non-empty list")
    for index, command in enumerate(commands):
        label = f"commands[{index}]"
        if not isinstance(command, dict):
            raise ManifestError(f"{label} must be an object")
        require(command, {"name", "compatibility_level", "comparator", "parity", "repeats"}, label)
        if command["compatibility_level"] not in LEVELS:
            raise ManifestError(f"{label}.compatibility_level must be one of {sorted(LEVELS)}")
        if "profile" in command and command["profile"] not in PROFILES:
            raise ManifestError(f"{label}.profile must be one of {sorted(PROFILES)}")
        barcode_tags = command.get("barcode_tags")
        if barcode_tags is not None:
            if not isinstance(barcode_tags, dict) or not barcode_tags:
                raise ManifestError(f"{label}.barcode_tags must be a non-empty object")
            for tag_name, tag_value in barcode_tags.items():
                if tag_name not in {"barcode_tag", "read_one_barcode_tag", "read_two_barcode_tag"}:
                    raise ManifestError(f"{label}.barcode_tags contains unknown field {tag_name!r}")
                if not isinstance(tag_value, str) or not re.fullmatch(r"[A-Za-z0-9]{2}", tag_value):
                    raise ManifestError(f"{label}.barcode_tags.{tag_name} must be a two-character SAM tag")
        if command.get("profile") == "umi_panel" and not barcode_tags:
            raise ManifestError(f"{label}.profile=umi_panel requires barcode_tags")
        if payload["tier"] in {"production_scale", "independent_reproduction"} and not command.get("profile"):
            raise ManifestError(f"{label}.profile is required for production-scale evidence")
        if command["parity"] not in {"PASS", "FAIL", "NOT_RUN"}:
            raise ManifestError(f"{label}.parity must be PASS, FAIL, or NOT_RUN")
        if not command["comparator"]:
            raise ManifestError(f"{label}.comparator must be explicit")
        if not isinstance(command["repeats"], int) or command["repeats"] < 1:
            raise ManifestError(f"{label}.repeats must be a positive integer")
        if command["parity"] == "FAIL" and not command.get("known_differences"):
            raise ManifestError(f"{label} FAIL requires known_differences")
        if release_ready:
            if payload["tier"] != "production_scale":
                raise ManifestError("release-ready evidence must have tier production_scale")
            if command["parity"] != "PASS":
                raise ManifestError(f"{label} is not parity PASS")
            if command["compatibility_level"] in {"D", "X"}:
                raise ManifestError(f"{label} cannot be delegated or unsupported")
            if command["repeats"] < 5:
                raise ManifestError(f"{label} must have at least five repeats")
            if not HEX64.fullmatch(command.get("arguments_sha256", "")):
                raise ManifestError(f"{label}.arguments_sha256 must be a 64-character SHA-256")
            for group in ("wall_seconds", "peak_rss_bytes", "temporary_disk_bytes"):
                if not isinstance(command.get(group), dict):
                    raise ManifestError(f"{label}.{group} must be an object")

    reproduction = payload.get("independent_reproduction")
    if not isinstance(reproduction, dict):
        raise ManifestError("independent_reproduction must be an object")
    reproduction_status = reproduction.get("status")
    if reproduction_status not in {"not_run", "pass", "fail"}:
        raise ManifestError("independent_reproduction.status must be not_run, pass, or fail")
    if reproduction_status in {"pass", "fail"}:
        require(
            reproduction,
            {
                "reviewer",
                "host_profile",
                "evidence_url",
                "turbo_picard_commit",
                "input_sha256",
                "arguments_sha256",
            },
            "independent_reproduction",
        )
        for key in ("reviewer", "host_profile"):
            if not isinstance(reproduction[key], str) or not reproduction[key].strip():
                raise ManifestError(f"independent_reproduction.{key} must be a non-empty string")
        require_evidence_url(reproduction["evidence_url"], "independent_reproduction.evidence_url")
        if not HEX40.fullmatch(str(reproduction["turbo_picard_commit"])):
            raise ManifestError(
                "independent_reproduction.turbo_picard_commit must be a 40-character hexadecimal SHA"
            )
        if reproduction["turbo_picard_commit"].lower() != software["turbo_picard_commit"].lower():
            raise ManifestError(
                "independent_reproduction.turbo_picard_commit must match software.turbo_picard_commit"
            )
        if not HEX64.fullmatch(str(reproduction["input_sha256"])):
            raise ManifestError(
                "independent_reproduction.input_sha256 must be a 64-character hexadecimal SHA-256"
            )
        if reproduction["input_sha256"].lower() != input_data["sha256"].lower():
            raise ManifestError(
                "independent_reproduction.input_sha256 must match input.sha256"
            )
        if not HEX64.fullmatch(str(reproduction["arguments_sha256"])):
            raise ManifestError(
                "independent_reproduction.arguments_sha256 must be a 64-character hexadecimal SHA-256"
            )
        if len(commands) != 1:
            raise ManifestError(
                "independent_reproduction.arguments_sha256 currently requires exactly one command"
            )
        if reproduction["arguments_sha256"].lower() != str(
            commands[0].get("arguments_sha256", "")
        ).lower():
            raise ManifestError(
                "independent_reproduction.arguments_sha256 must match commands[0].arguments_sha256"
            )
    if release_ready and reproduction_status != "pass":
        raise ManifestError("release-ready evidence requires independent_reproduction.status=pass")
    return payload


def main(argv=None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("manifest", type=Path)
    parser.add_argument("--release-ready", action="store_true")
    args = parser.parse_args(argv)
    try:
        validate(args.manifest, release_ready=args.release_ready)
    except ManifestError as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        return 1
    print(f"PASS: {args.manifest}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
