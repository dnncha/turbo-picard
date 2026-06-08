#!/usr/bin/env python3
"""Verify workflow starter files keep key migration patterns visible."""

from __future__ import annotations

import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
WORKFLOWS = ROOT / "packaging" / "workflows"
README = ROOT / "README.md"
USE_CASES = ROOT / "docs" / "use-cases.rst"
EVALUATION_PLAYBOOK = ROOT / "docs" / "evaluation-playbook.rst"
FAQ = ROOT / "docs" / "faq.rst"


def _read(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def validate_workflow_starters() -> list[str]:
    errors: list[str] = []

    required_trial_pairs = [
        (
            WORKFLOWS / "trial-samtofastq.nf",
            WORKFLOWS / "trial-samtofastq.wdl",
            "SamToFastq",
        ),
        (
            WORKFLOWS / "trial-fastqtosam.nf",
            WORKFLOWS / "trial-fastqtosam.wdl",
            "FastqToSam",
        ),
        (
            WORKFLOWS / "trial-fixmateinformation.nf",
            WORKFLOWS / "trial-fixmateinformation.wdl",
            "FixMateInformation",
        ),
    ]
    for left, right, command in required_trial_pairs:
        if not left.exists():
            errors.append(f"{left.relative_to(ROOT)} missing tiny {command} trial")
        if not right.exists():
            errors.append(f"{right.relative_to(ROOT)} missing tiny {command} trial")

    checks: list[tuple[Path, str, str]] = [
        (WORKFLOWS / "samtofastq.nf", "OUTPUT_PER_RG=", "Nextflow SamToFastq per-read-group toggle"),
        (WORKFLOWS / "samtofastq.nf", "RG_TAG=", "Nextflow SamToFastq read-group tag selection"),
        (WORKFLOWS / "samtofastq.nf", "OUTPUT_DIR=", "Nextflow SamToFastq per-read-group output directory"),
        (WORKFLOWS / "samtofastq.nf", "def outputArgs = outputPerRg", "Nextflow SamToFastq conditional output args"),
        (WORKFLOWS / "samtofastq.wdl", "OUTPUT_PER_RG=", "WDL SamToFastq per-read-group toggle"),
        (WORKFLOWS / "samtofastq.wdl", "RG_TAG=", "WDL SamToFastq read-group tag selection"),
        (WORKFLOWS / "samtofastq.wdl", "OUTPUT_DIR=", "WDL SamToFastq per-read-group output directory"),
        (WORKFLOWS / "samtofastq.wdl", "if output_per_rg then \"OUTPUT_PER_RG=true\"", "WDL SamToFastq conditional output args"),
        (WORKFLOWS / "samtofastq.wdl", "File? fastq", "WDL SamToFastq optional single-FASTQ output"),
        (WORKFLOWS / "samtofastq.wdl", "Boolean output_per_rg = false", "WDL SamToFastq default single-FASTQ mode"),
        (WORKFLOWS / "fastqtosam.nf", "USE_SEQUENTIAL_FASTQS=", "Nextflow FastqToSam sequential FASTQ support"),
        (WORKFLOWS / "fastqtosam.wdl", "USE_SEQUENTIAL_FASTQS=", "WDL FastqToSam sequential FASTQ support"),
        (WORKFLOWS / "Snakefile", "rule sam_to_fastq_per_rg:", "Snakemake dedicated SamToFastq per-read-group rule"),
        (WORKFLOWS / "Snakefile", "OUTPUT_PER_RG=true", "Snakemake SamToFastq per-read-group example"),
        (WORKFLOWS / "Snakefile", "USE_SEQUENTIAL_FASTQS=", "Snakemake FastqToSam sequential FASTQ toggle"),
    ]

    for path, needle, description in checks:
        if not path.exists():
            continue
        text = _read(path)
        if needle not in text:
            errors.append(f"{path.relative_to(ROOT)} missing {description}")

    for path in [WORKFLOWS / "fixmateinformation.nf", WORKFLOWS / "fixmateinformation.wdl"]:
        text = _read(path)
        if "SORT_ORDER=coordinate" in text:
            errors.append(
                f"{path.relative_to(ROOT)} still hard-codes FixMateInformation SORT_ORDER=coordinate"
            )

    snakefile = _read(WORKFLOWS / "Snakefile")
    match = re.search(
        r"rule fix_mate_information:\n(?P<body>(?:[ \t].*\n)+)",
        snakefile,
    )
    if not match:
        errors.append("packaging/workflows/Snakefile missing fix_mate_information rule")
    elif "SORT_ORDER=coordinate" in match.group("body"):
        errors.append(
            "packaging/workflows/Snakefile still hard-codes FixMateInformation SORT_ORDER=coordinate"
        )

    readme = _read(README)
    for needle, description in [
        ("per-read-group `SamToFastq`", "per-read-group SamToFastq migration wording"),
        ("sequential-shard `FastqToSam`", "sequential FastqToSam migration wording"),
        ("mate-repair boundaries around `FixMateInformation`", "FixMateInformation migration wording"),
    ]:
        if needle not in readme:
            errors.append(f"README.md missing {description}")

    workflow_readme = _read(WORKFLOWS / "README.md")
    for needle, description in [
        ("fastqtosam.wdl", "WDL FastqToSam starter listing"),
        ("per-read-group export", "per-read-group starter wording"),
        ("sequential-ingest toggles", "sequential-ingest starter wording"),
        ("trial-samtofastq.nf", "Nextflow SamToFastq trial listing"),
        ("trial-samtofastq.wdl", "SamToFastq trial listing"),
        ("trial-fastqtosam.wdl", "WDL FastqToSam trial listing"),
        ("trial-fixmateinformation.nf", "Nextflow FixMateInformation trial listing"),
    ]:
        if needle not in workflow_readme:
            errors.append(f"packaging/workflows/README.md missing {description}")

    use_cases = _read(USE_CASES)
    for needle, description in [
        ("packaging/workflows/samtofastq.wdl", "WDL SamToFastq starter mention in use-cases"),
        ("packaging/workflows/fastqtosam.wdl", "WDL FastqToSam starter mention in use-cases"),
        ("packaging/workflows/fixmateinformation.wdl", "WDL FixMateInformation starter mention in use-cases"),
        ("packaging/workflows/samtofastq.nf", "Nextflow SamToFastq starter mention in use-cases"),
        ("packaging/workflows/fastqtosam.nf", "Nextflow FastqToSam starter mention in use-cases"),
        ("packaging/workflows/fixmateinformation.nf", "Nextflow FixMateInformation starter mention in use-cases"),
        ("SamToFastq", "SamToFastq workflow mention in Snakemake use-cases"),
        ("FastqToSam", "FastqToSam workflow mention in Snakemake use-cases"),
        ("FixMateInformation", "FixMateInformation workflow mention in Snakemake use-cases"),
    ]:
        if needle not in use_cases:
            errors.append(f"docs/use-cases.rst missing {description}")

    evaluation_playbook = _read(EVALUATION_PLAYBOOK)
    for needle, description in [
        ("samtofastq.wdl", "WDL SamToFastq starter mention in evaluation playbook"),
        ("fastqtosam.wdl", "WDL FastqToSam starter mention in evaluation playbook"),
        ("fixmateinformation.wdl", "WDL FixMateInformation starter mention in evaluation playbook"),
        ("fastqtosam.nf", "Nextflow FastqToSam starter mention in evaluation playbook"),
        ("fixmateinformation.nf", "Nextflow FixMateInformation starter mention in evaluation playbook"),
        ("trial-samtofastq.nf", "Nextflow SamToFastq trial mention in evaluation playbook"),
        ("trial-samtofastq.wdl", "SamToFastq trial mention in evaluation playbook"),
        ("trial-fastqtosam.wdl", "WDL FastqToSam trial mention in evaluation playbook"),
        ("trial-fixmateinformation.nf", "Nextflow FixMateInformation trial mention in evaluation playbook"),
    ]:
        if needle not in evaluation_playbook:
            errors.append(f"docs/evaluation-playbook.rst missing {description}")

    one_command_trial = _read(WORKFLOWS / "one-command-trial.md")
    for needle, description in [
        ("trial-samtofastq.nf", "Nextflow SamToFastq trial mention in one-command-trial"),
        ("trial-samtofastq.wdl", "SamToFastq trial mention in one-command-trial"),
        ("trial-fastqtosam.wdl", "WDL FastqToSam trial mention in one-command-trial"),
        ("trial-fixmateinformation.nf", "Nextflow FixMateInformation trial mention in one-command-trial"),
    ]:
        if needle not in one_command_trial:
            errors.append(f"packaging/workflows/one-command-trial.md missing {description}")

    trial_config = _read(WORKFLOWS / "trial-config.yaml")
    for needle, description in [
        ("output_per_rg:", "SamToFastq trial toggle in trial-config"),
        ("rg_tag:", "SamToFastq read-group tag toggle in trial-config"),
        ("use_sequential_fastqs:", "FastqToSam sequential toggle in trial-config"),
    ]:
        if needle not in trial_config:
            errors.append(f"packaging/workflows/trial-config.yaml missing {description}")

    trial_fixmate_wdl_path = WORKFLOWS / "trial-fixmateinformation.wdl"
    trial_fixmate_wdl = _read(trial_fixmate_wdl_path) if trial_fixmate_wdl_path.exists() else ""
    if "SORT_ORDER=coordinate" in trial_fixmate_wdl:
        errors.append(
            "packaging/workflows/trial-fixmateinformation.wdl still hard-codes FixMateInformation SORT_ORDER=coordinate"
        )

    trial_samtofastq_wdl_path = WORKFLOWS / "trial-samtofastq.wdl"
    if trial_samtofastq_wdl_path.exists():
        trial_samtofastq_wdl = _read(trial_samtofastq_wdl_path)
        if "File? fastq" not in trial_samtofastq_wdl:
            errors.append(
                "packaging/workflows/trial-samtofastq.wdl missing optional single-FASTQ output"
            )

    for path in [
        WORKFLOWS / "samtofastq.wdl",
        WORKFLOWS / "samtofastq.nf",
        WORKFLOWS / "trial-samtofastq.wdl",
        WORKFLOWS / "trial-samtofastq.nf",
    ]:
        if not path.exists():
            continue
        text = _read(path)
        if "FASTQ=" in text and "OUTPUT_PER_RG=true" in text and "if output_per_rg then \"OUTPUT_PER_RG=true\"" not in text and "def outputArgs = outputPerRg" not in text and "def outputArgs = meta.output_per_rg" not in text:
            errors.append(
                f"{path.relative_to(ROOT)} still mixes FASTQ output with OUTPUT_PER_RG=true in the same command path"
            )

    faq = _read(FAQ)
    for needle, description in [
        ("FastqToSam", "FastqToSam mention in FAQ shortlist"),
        ("FixMateInformation", "FixMateInformation mention in FAQ shortlist"),
    ]:
        if needle not in faq:
            errors.append(f"docs/faq.rst missing {description}")

    return errors


def main() -> int:
    errors = validate_workflow_starters()
    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
