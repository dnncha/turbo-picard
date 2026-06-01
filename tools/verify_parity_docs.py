#!/usr/bin/env python3
"""Verify parity documentation stays explicit and linked."""

from __future__ import annotations

import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PARITY_DOCS = ROOT / "docs" / "parity.rst"
DOCS_INDEX = ROOT / "docs" / "index.rst"
README = ROOT / "README.md"
SITE = ROOT / "docs" / "site" / "index.html"
ADOPTION_DOCS = ROOT / "docs" / "adoption.rst"
FALLBACK_DOCS = ROOT / "docs" / "fallback.rst"
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


def validate_parity_docs(
    parity_docs: str,
    docs_index: str,
    readme: str,
    site: str,
    adoption_docs: str,
    fallback_docs: str = "",
) -> list[str]:
    errors: list[str] = []
    parity_text = normalize(parity_docs)
    site_text = normalize(site)
    readme_text = normalize(readme)
    adoption_text = normalize(adoption_docs)
    fallback_text = normalize(fallback_docs)

    for phrase in OVERCLAIM_PHRASES:
        if phrase in parity_text:
            errors.append(f"parity docs contain unsupported overclaim: {phrase}")

    required_parity_terms = [
        ("specific command", "command-specific parity scope"),
        ("specific input shape", "input-specific parity scope"),
        ("comparison method", "named comparison method"),
        ("does not mean every picard behavior", "not-full-Picard disclosure"),
        ("does not prove broad switching safety", "broad switching caveat"),
        ("representative inputs", "representative-data guidance"),
        ("input sha-256", "input SHA-256 guidance"),
        ("picard version", "Picard version evidence guidance"),
        ("turbo-picard version", "turbo-picard version evidence guidance"),
        ("tools/compare_real_data.py", "real-data comparator command"),
        (
            "python3 tools/verify_real_data_evidence.py --release-ready",
            "release-ready verifier command",
        ),
        ("fallback", "upstream Picard fallback guidance"),
    ]
    for needle, description in required_parity_terms:
        if needle not in parity_text:
            errors.append(f"parity docs missing {description}")

    required_comparisons = [
        "markduplicates",
        "sortsam",
        "buildbamindex",
        "samtofastq",
        "validatesamfile",
        "metrics commands",
    ]
    for command in required_comparisons:
        if command not in parity_text:
            errors.append(f"parity docs missing comparison boundary for {command}")

    if "\n   parity\n" not in docs_index:
        errors.append("docs index missing parity page in user-guide toctree")
    if "parity.html" not in readme:
        errors.append("README missing parity guide link")
    if "parity guide" not in readme_text and "what parity means" not in readme_text:
        errors.append("README missing human-readable parity guide label")
    if "parity.html" not in site:
        errors.append("site missing parity guide link")
    if "what parity means" not in site_text:
        errors.append("site missing human-readable parity guide label")
    if ":doc:`parity`" not in adoption_docs and "parity.html" not in adoption_docs:
        errors.append("adoption docs missing parity page cross-reference")
    if "comparison boundary" not in adoption_text:
        errors.append("adoption docs missing comparison-boundary wording")
    if fallback_docs:
        for phrase in OVERCLAIM_PHRASES:
            if phrase in fallback_text:
                errors.append(f"fallback docs contain unsupported overclaim: {phrase}")
        for needle, description in [
            ("compatibility bridge", "fallback compatibility-bridge wording"),
            ("not proof", "fallback not-proof caveat"),
            (":doc:`parity`", "fallback parity cross-reference"),
            ("unsupported surfaces remain visible", "fallback unsupported-surface caveat"),
        ]:
            if needle not in fallback_docs and needle not in fallback_text:
                errors.append(f"fallback docs missing {description}")

    return errors


def main() -> int:
    inputs = {
        "parity docs": PARITY_DOCS,
        "docs index": DOCS_INDEX,
        "README": README,
        "site": SITE,
        "adoption docs": ADOPTION_DOCS,
        "fallback docs": FALLBACK_DOCS,
    }
    missing = [f"{label} missing: {path}" for label, path in inputs.items() if not path.exists()]
    if missing:
        for error in missing:
            print(error, file=sys.stderr)
        return 1
    errors = validate_parity_docs(
        PARITY_DOCS.read_text(encoding="utf-8"),
        DOCS_INDEX.read_text(encoding="utf-8"),
        README.read_text(encoding="utf-8"),
        SITE.read_text(encoding="utf-8"),
        ADOPTION_DOCS.read_text(encoding="utf-8"),
        FALLBACK_DOCS.read_text(encoding="utf-8"),
    )
    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
