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


class ManifestError(ValueError):
    pass


def require(mapping, keys, label):
    missing = sorted(key for key in keys if key not in mapping)
    if missing:
        raise ManifestError(f"{label} missing required field(s): {', '.join(missing)}")


def non_negative_int(value, label):
    if not isinstance(value, int) or isinstance(value, bool) or value < 0:
        raise ManifestError(f"{label} must be a non-negative integer")


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
    if not isinstance(input_data["source_url"], str) or not input_data["source_url"].startswith(("https://", "s3://", "gs://")):
        raise ManifestError("input.source_url must be an immutable HTTPS, S3, or GCS URL")
    non_negative_int(input_data["bytes"], "input.bytes")
    non_negative_int(input_data["read_count"], "input.read_count")
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
    if reproduction.get("status") not in {"not_run", "pass", "fail"}:
        raise ManifestError("independent_reproduction.status must be not_run, pass, or fail")
    if release_ready and reproduction["status"] != "pass":
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

