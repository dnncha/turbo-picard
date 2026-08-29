#!/usr/bin/env python3
"""Verify the checked-in MarkDuplicates guardrails and their disclosures."""

from __future__ import annotations

import json
import math
import re
import sys
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
README = ROOT / "benchmarks" / "markduplicates-competitors" / "README.md"
GUARDRAILS = (
    ROOT / "benchmarks" / "markduplicates-competitors" / "synthetic-1m-external-guardrail.json",
    ROOT / "benchmarks" / "markduplicates-competitors" / "real-na12878-cram-external-guardrail.json",
)
HEX64 = re.compile(r"^[0-9a-f]{64}$")
HEX40 = re.compile(r"^[0-9a-f]{40}$")


def _number(value: Any, label: str, errors: list[str]) -> float | None:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        errors.append(f"{label} must be numeric")
        return None
    if not math.isfinite(float(value)):
        errors.append(f"{label} must be finite")
        return None
    return float(value)


def validate_payload(payload: dict[str, Any], label: str, readme: str) -> list[str]:
    errors: list[str] = []
    if payload.get("schema_version") != 1:
        errors.append(f"{label}: schema_version must be 1")
    if payload.get("claim_status") != "evidence_only":
        errors.append(f"{label}: claim_status must remain evidence_only")

    dataset = payload.get("dataset")
    if not isinstance(dataset, dict):
        errors.append(f"{label}: dataset must be an object")
        dataset = {}
    input_sha = dataset.get("input_sha256")
    if not isinstance(input_sha, str) or not HEX64.fullmatch(input_sha):
        errors.append(f"{label}: dataset input_sha256 must be 64 lowercase hex characters")
    if dataset.get("format") not in {"BAM", "CRAM"}:
        errors.append(f"{label}: dataset format must be BAM or CRAM")

    protocol = payload.get("protocol")
    if not isinstance(protocol, dict):
        errors.append(f"{label}: protocol must be an object")
        protocol = {}
    if protocol.get("runner") != "tools/bench_markduplicates_competitors.py":
        errors.append(f"{label}: protocol runner is not the auditable competitor runner")
    if protocol.get("threads") != 1 or protocol.get("warmups") != 1:
        errors.append(f"{label}: guardrail protocol must use one thread and one warm-up")
    if protocol.get("read_name_regex") != "null":
        errors.append(f"{label}: guardrail protocol must disclose READ_NAME_REGEX=null")
    sort_window = protocol.get("external_sort_window")
    if (
        not isinstance(sort_window, dict)
        or sort_window.get("max_records_in_ram") != 500_000
        or sort_window.get("max_bytes_in_ram") != 268_435_456
    ):
        errors.append(f"{label}: external sort window disclosure is inconsistent")
    if protocol.get("reference_tool") not in {None, "picard"}:
        errors.append(f"{label}: reference_tool must be picard")
    parity = protocol.get("turbo_picard_parity", protocol.get("parity"))
    if parity != "PASS":
        errors.append(f"{label}: turbo-picard parity must be PASS")
    if "required_tool_gate" in protocol and protocol.get("required_tool_gate") != "PASS":
        errors.append(f"{label}: required_tool_gate must be PASS")
    source_commit = protocol.get("turbo_picard_source_commit")
    if not isinstance(source_commit, str) or not HEX40.fullmatch(source_commit):
        errors.append(f"{label}: turbo-picard source commit must be a full 40-character hash")
    binary_sha = protocol.get("turbo_picard_binary_sha256")
    if not isinstance(binary_sha, str) or not HEX64.fullmatch(binary_sha):
        errors.append(f"{label}: turbo-picard binary SHA-256 must be 64 lowercase hex characters")
    worktree = protocol.get("turbo_picard_worktree")
    if not isinstance(worktree, str) or not worktree.strip():
        errors.append(f"{label}: turbo-picard worktree state must be disclosed")
    version = protocol.get("turbo_picard_version")
    if not isinstance(version, str) or not re.fullmatch(r"picard \d+\.\d+\.\d+", version):
        errors.append(f"{label}: turbo-picard version disclosure is malformed")
    if protocol.get("picard_version") != "Version:3.4.0":
        errors.append(f"{label}: Picard reference version must be 3.4.0")

    median = payload.get("median")
    if not isinstance(median, dict):
        errors.append(f"{label}: median must be an object")
        median = {}
    numeric_keys = (
        "turbo_picard_wall_seconds",
        "turbo_picard_p95_wall_seconds",
        "turbo_picard_peak_rss_bytes",
        "turbo_picard_peak_temporary_disk_bytes",
        "picard_wall_seconds",
        "picard_p95_wall_seconds",
        "picard_peak_rss_bytes",
        "picard_peak_temporary_disk_bytes",
        "picard_to_turbo_wall_ratio",
        "picard_to_turbo_peak_rss_ratio",
    )
    values = {
        key: _number(median.get(key), f"{label}: median.{key}", errors)
        for key in numeric_keys
    }
    for key, value in values.items():
        if value is not None and value < 0:
            errors.append(f"{label}: median.{key} must not be negative")
    turbo_wall = values["turbo_picard_wall_seconds"]
    picard_wall = values["picard_wall_seconds"]
    wall_ratio = values["picard_to_turbo_wall_ratio"]
    if (
        turbo_wall
        and picard_wall
        and wall_ratio is not None
        and not math.isclose(wall_ratio, picard_wall / turbo_wall, rel_tol=0.01)
    ):
        errors.append(f"{label}: wall ratio does not match the recorded medians")
    turbo_rss = values["turbo_picard_peak_rss_bytes"]
    picard_rss = values["picard_peak_rss_bytes"]
    rss_ratio = values["picard_to_turbo_peak_rss_ratio"]
    if (
        turbo_rss
        and picard_rss
        and rss_ratio is not None
        and not math.isclose(rss_ratio, picard_rss / turbo_rss, rel_tol=0.01)
    ):
        errors.append(f"{label}: RSS ratio does not match the recorded medians")

    interpretation = " ".join(str(value) for value in payload.get("interpretation", []))
    if (
        "evidence" not in interpretation.lower()
        or "independent" not in interpretation.lower()
        or "production" not in interpretation.lower()
    ):
        errors.append(
            f"{label}: interpretation must retain evidence, production, and independent-reproduction caveats"
        )

    marker_map = {
        "synthetic": (
            "0.531262 seconds versus 1.860894 seconds",
            "237,420,544 bytes of peak RSS, compared with 1,160,527,872",
        ),
        "cram": (
            "0.232844 seconds versus 0.826270 seconds",
            "39,452,672 versus 859,127,808 bytes",
        ),
    }
    marker_key = "cram" if "cram" in label else "synthetic"
    for marker in marker_map[marker_key]:
        if marker not in readme:
            errors.append(f"{label}: benchmark README is missing disclosure marker {marker!r}")
    return errors


def collect_errors(root: Path = ROOT) -> list[str]:
    del root  # Keep the function signature convenient for verifier tests.
    readme = README.read_text(encoding="utf-8")
    errors: list[str] = []
    payloads: list[tuple[str, dict[str, Any]]] = []
    for path in GUARDRAILS:
        label = path.stem
        try:
            payload = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as exc:
            errors.append(f"{label}: cannot read JSON: {exc}")
            continue
        if not isinstance(payload, dict):
            errors.append(f"{label}: top-level JSON must be an object")
            continue
        errors.extend(validate_payload(payload, label, readme))
        payloads.append((label, payload))

    if len(payloads) == len(GUARDRAILS):
        protocols = [payload["protocol"] for _, payload in payloads]
        for field in (
            "turbo_picard_source_commit",
            "turbo_picard_binary_sha256",
            "turbo_picard_version",
            "picard_version",
        ):
            if len({protocol.get(field) for protocol in protocols}) != 1:
                errors.append(f"guardrails: {field} must match across checked-in guardrails")
    return errors


def main() -> int:
    errors = collect_errors()
    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1
    print("MarkDuplicates guardrails and disclosures are consistent")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
