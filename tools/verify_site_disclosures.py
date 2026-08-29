#!/usr/bin/env python3
"""Ensure the marketing site discloses current release boundaries."""

from __future__ import annotations

import pathlib
import re
import sys


ROOT = pathlib.Path(__file__).resolve().parents[1]
SITE = ROOT / "docs" / "site" / "index.html"
OVERCLAIM_PHRASES = [
    "drop-in replacement",
    "production genomics workflows",
    "validated for all cohorts",
    "safe for all cohorts",
    "safe for all production",
    "proves safe to switch",
    "complete cohort-scale validation",
]


def normalize(text: str) -> str:
    text = re.sub(r"<[^>]+>", " ", text)
    text = text.replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&")
    return re.sub(r"\s+", " ", text).strip().lower()


def workspace_version(root: pathlib.Path = ROOT) -> str:
    cargo_toml = (root / "Cargo.toml").read_text(encoding="utf-8")
    match = re.search(
        r"(?ms)^\[workspace\.package\]\s+.*?^version\s*=\s*\"([^\"]+)\"",
        cargo_toml,
    )
    if not match:
        raise ValueError("Cargo.toml missing [workspace.package] version")
    return match.group(1)


def validate_site_disclosures(html: str, *, version: str | None = None) -> list[str]:
    text = normalize(html)
    version = version or workspace_version()
    errors: list[str] = []
    head = html.split("</head>", 1)[0].lower()
    if "current boundaries" not in text:
        errors.append("site missing current-boundaries section")
    if "not a full picard suite" not in text:
        errors.append("site missing not-full-Picard-suite disclosure")
    if "selected picard commands" not in head:
        errors.append("site metadata missing selected-command caveat")
    if "fallback" not in head or "unsupported commands" not in head:
        errors.append("site metadata missing fallback/unsupported-command caveat")
    if "production genomics workflows" in head:
        errors.append("site metadata contains unsupported production-genomics overclaim")
    for phrase in OVERCLAIM_PHRASES:
        if phrase in text:
            errors.append(f"site contains unsupported overclaim: {phrase}")
    if "lightweight pdf" not in text or "metrics text" not in text:
        errors.append("site missing lightweight chart PDF disclosure")
    if "switch only the commands where the evidence supports the change" not in text:
        errors.append("site missing evidence-supported switch disclosure")
    if (
        "python3 tools/verify_benchmark_thresholds.py" not in text
        or "5.00x" not in text
        or "20.00x" not in text
        or "50.00x" not in text
    ):
        errors.append("site missing benchmark threshold release-gate disclosure")
    if (
        "citation.cff" not in text
        or "software citation" not in text
        or "archived turbo-picard release" not in text
        or "inputs separately" not in text
        or "sha-256" not in text
    ):
        errors.append("site missing software-vs-input citation disclosure")
    if (
        "bioconda" not in text
        or f"v{version}" not in text
        or "python3 tools/bioconda_release_preflight.py" not in text
        or "bioconda-utils lint recipes config.yml --packages turbo-picard turbo-picard-picard-shim" not in text
    ):
        errors.append("site missing Bioconda release/lint disclosure")
    if "submission has not been opened" in text:
        errors.append("site contains stale Bioconda submission status")
    return errors


def main() -> int:
    errors = validate_site_disclosures(SITE.read_text(encoding="utf-8"))
    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
