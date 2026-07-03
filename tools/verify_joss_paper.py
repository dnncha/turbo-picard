#!/usr/bin/env python3
"""Check the local JOSS paper draft for basic submission hygiene."""

from __future__ import annotations

import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PAPER = ROOT / "paper" / "paper.md"
BIB = ROOT / "paper" / "paper.bib"
CHECKLIST = ROOT / "docs" / "joss-submission.rst"
README = ROOT / "README.md"
WORKFLOW = ROOT / ".github" / "workflows" / "joss-paper.yml"

REQUIRED_SECTIONS = [
    "Summary",
    "Statement of need",
    "State of the field",
    "Software design",
    "Research impact statement",
    "AI usage disclosure",
    "Acknowledgements",
    "References",
]


def strip_metadata(text: str) -> str:
    return re.sub(r"\A---\n.*?\n---\n", "", text, flags=re.S)


def word_count(markdown: str) -> int:
    body = strip_metadata(markdown)
    body = re.sub(r"```.*?```", " ", body, flags=re.S)
    body = re.sub(r"`[^`]+`", " ", body)
    body = re.sub(r"\[@[^\]]+\]", " ", body)
    body = re.sub(r"^# .*$", " ", body, flags=re.M)
    return len(re.findall(r"[A-Za-z0-9]+(?:[-'][A-Za-z0-9]+)?", body))


def bib_keys(text: str) -> set[str]:
    return set(re.findall(r"@\w+\{([^,\s]+)", text))


def citation_keys(text: str) -> set[str]:
    keys: set[str] = set()
    for cite_group in re.findall(r"\[([^\]]*@[^]]+)\]", text):
        for key in re.findall(r"@([A-Za-z0-9:_./-]+)", cite_group):
            keys.add(key.rstrip(";,."))
    return keys


def validate() -> list[str]:
    errors: list[str] = []
    if not PAPER.exists():
        return [f"missing {PAPER.relative_to(ROOT)}"]
    if not BIB.exists():
        return [f"missing {BIB.relative_to(ROOT)}"]
    for path in [CHECKLIST, README, WORKFLOW]:
        if not path.exists():
            errors.append(f"missing {path.relative_to(ROOT)}")

    paper = PAPER.read_text(encoding="utf-8")
    bib = BIB.read_text(encoding="utf-8")
    checklist = CHECKLIST.read_text(encoding="utf-8") if CHECKLIST.exists() else ""
    readme = README.read_text(encoding="utf-8") if README.exists() else ""
    workflow = WORKFLOW.read_text(encoding="utf-8") if WORKFLOW.exists() else ""

    if not paper.startswith("---\n"):
        errors.append("paper must start with YAML metadata")
    for field in ["title:", "authors:", "affiliations:", "bibliography: paper.bib"]:
        if field not in paper:
            errors.append(f"paper metadata missing {field}")
    if "Independent researcher" not in paper:
        errors.append("paper author affiliation must match current creator metadata")

    for section in REQUIRED_SECTIONS:
        if f"# {section}" not in paper:
            errors.append(f"paper missing required section: {section}")

    words = word_count(paper)
    if not 750 <= words <= 1750:
        errors.append(f"paper word count {words} outside expected JOSS range 750-1750")

    cited = citation_keys(paper)
    available = bib_keys(bib)
    for key in sorted(cited - available):
        errors.append(f"citation @{key} missing from paper.bib")
    for key in sorted(available - cited):
        errors.append(f"paper.bib entry @{key} is not cited")

    if "AI" not in paper or "Generative AI" not in paper:
        errors.append("paper must include a clear AI usage disclosure")
    if "No external funding" not in paper:
        errors.append("paper must include funding acknowledgement")
    if "docs/joss-submission.rst" not in readme:
        errors.append("README must link to the JOSS submission checklist")
    for needle in [
        "python3 tools/verify_joss_paper.py",
        "``paper/paper.md``",
        "https://doi.org/10.5281/zenodo.20541928",
    ]:
        if needle not in checklist:
            errors.append(f"JOSS submission checklist missing {needle}")
    for needle in ["openjournals/inara", "paper/paper.pdf", "actions/upload-artifact"]:
        if needle not in workflow:
            errors.append(f"JOSS paper workflow missing {needle}")

    return errors


def main() -> int:
    errors = validate()
    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1
    print(f"JOSS paper checks passed: {word_count(PAPER.read_text(encoding='utf-8'))} words")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
