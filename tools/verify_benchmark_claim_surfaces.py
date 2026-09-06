#!/usr/bin/env python3
"""Keep release-facing benchmark claim surfaces synchronized with benchmark data."""

from __future__ import annotations

import json
import sys
from pathlib import Path
from typing import Mapping


ROOT = Path(__file__).resolve().parents[1]
BENCHMARK_DATA = ROOT / "docs" / "site" / "assets" / "benchmark-data.json"

CLAIM_SURFACES = (
    "README.md",
    "benchmarks/README.md",
    "docs/benchmarks.rst",
    "docs/index.rst",
    "docs/performance.rst",
    "docs/picard-alternatives.rst",
    "docs/picard-vs-turbo-picard.rst",
    "docs/site/evidence/index.html",
    "packaging/bioconda/BIOCONDA_PR.md",
    "packaging/outreach/README.md",
    "packaging/outreach/biostars.md",
    "packaging/outreach/hacker-news-show-hn.md",
    "packaging/outreach/maintainer-note.md",
    "packaging/outreach/nf-core-slack.md",
    "packaging/outreach/reddit-bioinformatics.md",
    "packaging/outreach/rust-users-forum.md",
    "packaging/outreach/seqera-community-show-and-tell.md",
    "packaging/outreach/social-posts.md",
)
OUTREACH_SURFACES = frozenset(
    path for path in CLAIM_SURFACES if path.startswith("packaging/outreach/")
)


def format_speedup(value: float) -> str:
    return f"{value:.2f}x"


def required_claims(path: str, data: Mapping[str, object]) -> list[str]:
    summary = data["summary"]
    if not isinstance(summary, Mapping):
        raise ValueError("benchmark data summary must be a mapping")
    top = format_speedup(float(summary["top_speedup"]))
    floor = format_speedup(float(summary["floor_speedup"]))
    geometric_mean = format_speedup(float(summary["geometric_mean_speedup"]))

    if path == "packaging/bioconda/BIOCONDA_PR.md":
        parity = data.get("parity")
        return [
            f"Parity: {parity}.",
            f"Geometric mean speedup: {geometric_mean}.",
            f"Median speedup: {format_speedup(float(summary['median_speedup']))}.",
            f"Slowest saved speedup: {floor} on {summary['floor_command']}.",
            f"Fastest saved speedup: {top} on {summary['top_command']}.",
        ]
    if path in OUTREACH_SURFACES:
        return [geometric_mean, top]
    return [geometric_mean, floor, top]


def validate_claim_surfaces(
    surfaces: Mapping[str, str], data: Mapping[str, object]
) -> list[str]:
    errors: list[str] = []
    for path in CLAIM_SURFACES:
        text = surfaces.get(path)
        if text is None:
            errors.append(f"missing benchmark claim surface: {path}")
            continue
        for claim in required_claims(path, data):
            if claim not in text:
                errors.append(f"{path} missing current benchmark claim: {claim}")
    return errors


def main() -> int:
    data = json.loads(BENCHMARK_DATA.read_text(encoding="utf-8"))
    surfaces = {
        path: (ROOT / path).read_text(encoding="utf-8") for path in CLAIM_SURFACES
    }
    errors = validate_claim_surfaces(surfaces, data)
    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
