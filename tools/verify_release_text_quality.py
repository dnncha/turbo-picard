#!/usr/bin/env python3
"""Check release-facing prose for hype, vague claims, and missing reader cues."""

from __future__ import annotations

import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]

RELEASE_TEXT_PATHS = [
    Path("README.md"),
    Path("docs/index.rst"),
    Path("docs/is-this-for-you.rst"),
    Path("docs/quickstart.rst"),
    Path("docs/adoption.rst"),
    Path("docs/benchmarks.rst"),
    Path("docs/citation.rst"),
    Path("docs/compatibility-contract.rst"),
    Path("docs/packaging.rst"),
    Path("docs/parity.rst"),
    Path("docs/picard-alternatives.rst"),
    Path("docs/performance.rst"),
    Path("docs/production-readiness.rst"),
    Path("docs/troubleshooting.rst"),
    Path("docs/turbo-picard-vs-riker.rst"),
    Path("docs/site/index.html"),
    Path("paper/paper.md"),
    Path("packaging/bioconda/BIOCONDA_PR.md"),
    Path("packaging/bioconda/turbo-picard/README.md"),
    Path("packaging/bioconda/turbo-picard-picard-shim/README.md"),
    Path("packaging/outreach/README.md"),
    Path("packaging/outreach/biostars.md"),
    Path("packaging/outreach/hacker-news-show-hn.md"),
    Path("packaging/outreach/nf-core-slack.md"),
    Path("packaging/outreach/reddit-bioinformatics.md"),
    Path("packaging/outreach/rust-users-forum.md"),
    Path("packaging/outreach/seqera-community-show-and-tell.md"),
    Path("packaging/nf-core/README.md"),
    Path("packaging/nf-core/tests/README.md"),
    Path("benchmarks/markduplicates-competitors/README.md"),
    Path("benchmarks/production/README.md"),
]

BANNED_PHRASES = [
    "ai " + "slop",
    "ai-powered",
    "adoption " + "evidence",
    "adoption " + "trust",
    "battle-tested",
    "blazing fast",
    "big " + "wins",
    "comprehensive solution",
    "cutting-edge",
    "delve",
    "drop-in replacement",
    "evidence-" + "bounded",
    "effortless",
    "empower",
    "first choice",
    "game changer",
    "game-" + "changing",
    "harness",
    "industry " + "exposure",
    "leverage",
    "massive scale",
    "massive industry " + "impact",
    "next " + "wins",
    "pilot " + "conversations",
    "private " + "feedback",
    "production genomics workflows",
    "quote-" + "approved",
    "revolution" + "ary",
    "robust solution",
    "seamless",
    "state-of-the-art",
    "turning private evaluation into public adoption " + "evidence",
    "unlock",
    "users",
    "utilize",
    "without turning private feedback into public " + "evidence",
]

REQUIRED_READER_CUES = {
    Path("README.md"): [
        "The full docs are on Read the Docs",
        "When It Helps",
        "When To Stay With Picard",
        "Use the explicit `turbo-picard` command while testing",
        "Cite the archived",
    ],
    Path("docs/index.rst"): [
        "Start here",
        "New user",
        "Pipeline owner",
        "Packaging",
    ],
    Path("docs/quickstart.rst"): [
        "Start with ``turbo-picard``",
        "Use the shim only after",
        "Before changing a workflow",
    ],
    Path("docs/adoption.rst"): [
        "Practical path",
        "choose data that looks like the run you want to switch",
        "Treat a failure as useful information",
    ],
    Path("docs/citation.rst"): [
        "For a methods section",
        "The project citation is for the software",
        "does not cite the benchmark inputs",
    ],
    Path("docs/packaging.rst"): [
        "Main package",
        "Compatibility shim package",
        "Bioconda release path",
    ],
    Path("docs/troubleshooting.rst"): [
        "Start by separating the two entrypoints",
        "Output differs from Picard",
        "Bioconda recipe still uses source.path",
    ],
    Path("packaging/bioconda/BIOCONDA_PR.md"): [
        "This PR adds `turbo-picard`",
        "separate optional compatibility shim",
        "turbo-picard is not a full Picard replacement",
        "Expected Bioconda checkout checks",
        "bioconda-utils build --docker --mulled-test turbo-picard",
    ],
}


def normalize(text: str) -> str:
    text = re.sub(r"<[^>]+>", " ", text)
    text = (
        text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&nbsp;", " ")
    )
    return re.sub(r"\s+", " ", text).strip().lower()


def validate_release_text(root: Path = ROOT) -> list[str]:
    errors: list[str] = []
    internal_docs = root / "docs/superpowers"
    if internal_docs.exists() and any(path.is_file() for path in internal_docs.rglob("*")):
        errors.append(
            "docs/superpowers contains internal planning notes; keep release-facing docs public"
        )

    for rel_path in RELEASE_TEXT_PATHS:
        path = root / rel_path
        if not path.is_file():
            errors.append(f"{rel_path} is missing")
            continue
        raw = path.read_text(encoding="utf-8")
        text = normalize(raw)
        for phrase in BANNED_PHRASES:
            if phrase in text:
                errors.append(f"{rel_path} contains release-text banned phrase: {phrase}")

        for cue in REQUIRED_READER_CUES.get(rel_path, []):
            if normalize(cue) not in text:
                errors.append(f"{rel_path} missing reader cue: {cue}")

    return errors


def main() -> int:
    errors = validate_release_text()
    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
