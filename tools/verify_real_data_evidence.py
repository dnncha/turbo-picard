#!/usr/bin/env python3
"""Verify checked-in real-data comparison evidence and citations."""

from __future__ import annotations

import hashlib
import json
import sys
from pathlib import Path
from urllib.parse import urlparse


ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "benchmarks" / "real-data" / "manifest.json"
README = ROOT / "benchmarks" / "README.md"
SITE = ROOT / "docs" / "site" / "index.html"
RELEASE_CANDIDATE_MIN_BYTES = 1_000_000
RELEASE_CANDIDATE_REQUIRED_COMMANDS = {
    "ViewSam",
    "CleanSam",
    "CollectQualityYieldMetrics",
    "CollectAlignmentSummaryMetrics",
    "MarkDuplicates",
}


def digest_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def load_manifest(path: Path = MANIFEST) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def validate_source_citation(dataset_id: str, source_url: str, source_commit: str) -> list[str]:
    errors: list[str] = []
    parsed = urlparse(source_url)
    if parsed.scheme != "https" or not parsed.netloc:
        errors.append(f"{dataset_id} source_url must be an https URL")
    if source_commit in {"develop", "main", "master"}:
        errors.append(f"{dataset_id} source_commit is not pinned")
    if not source_commit or len(source_commit) < 3:
        errors.append(f"{dataset_id} source_commit is too short to identify a source")

    if parsed.netloc == "raw.githubusercontent.com":
        errors.append(
            f"{dataset_id} source_url must not use raw.githubusercontent.com moving branch URLs"
        )
    if parsed.netloc == "github.com":
        marker = f"/blob/{source_commit}/"
        if marker not in parsed.path:
            errors.append(
                f"{dataset_id} GitHub source_url must include /blob/{source_commit}/"
            )
    elif source_commit and source_commit not in source_url:
        errors.append(
            f"{dataset_id} non-GitHub source_url must include source_commit/accession identifier"
        )
    return errors


def validate_manifest(manifest: dict) -> list[str]:
    errors = []
    datasets = manifest.get("datasets")
    if not isinstance(datasets, list) or not datasets:
        return ["real-data manifest has no datasets"]

    seen = set()
    required = {
        "id",
        "input_path",
        "evidence_json",
        "evidence_markdown",
        "source_url",
        "source_commit",
        "sha256",
        "scope_caveat",
        "release_tier",
        "expected_commands",
    }
    for dataset in datasets:
        dataset_id = dataset.get("id", "<missing>")
        missing = sorted(required - dataset.keys())
        for key in missing:
            errors.append(f"{dataset_id} missing manifest key: {key}")
        if dataset_id in seen:
            errors.append(f"duplicate real-data dataset id: {dataset_id}")
        seen.add(dataset_id)
        errors.extend(
            validate_source_citation(
                dataset_id,
                str(dataset.get("source_url", "")),
                str(dataset.get("source_commit", "")),
            )
        )
        if not isinstance(dataset.get("expected_commands"), dict) or not dataset.get("expected_commands"):
            errors.append(f"{dataset_id} has no expected commands")
        if dataset.get("release_tier") not in {"public_smoke", "release_candidate"}:
            errors.append(f"{dataset_id} has invalid release_tier")
    return errors


def validate_release_candidate_dataset(dataset: dict, input_summary: dict) -> list[str]:
    errors: list[str] = []
    if dataset.get("release_tier") != "release_candidate":
        return errors
    dataset_id = dataset["id"]
    expected_commands = set(dataset.get("expected_commands", {}))
    missing_commands = sorted(RELEASE_CANDIDATE_REQUIRED_COMMANDS - expected_commands)
    if missing_commands:
        errors.append(
            f"{dataset_id} release_candidate missing required commands: "
            + ", ".join(missing_commands)
        )
    min_bytes = int(dataset.get("minimum_input_bytes", RELEASE_CANDIDATE_MIN_BYTES))
    size_bytes = int(input_summary.get("size_bytes", 0))
    if size_bytes < min_bytes:
        errors.append(
            f"{dataset_id} release_candidate input too small: {size_bytes} bytes < {min_bytes}"
        )
    return errors


def validate_dataset(dataset: dict, readme: str, site: str) -> list[str]:
    errors = []
    dataset_id = dataset["id"]
    input_path = ROOT / dataset["input_path"]
    evidence_json_path = ROOT / dataset["evidence_json"]
    evidence_markdown_path = ROOT / dataset["evidence_markdown"]

    if not input_path.exists():
        errors.append(f"{dataset_id} missing input: {dataset['input_path']}")
        return errors
    if not evidence_json_path.exists():
        errors.append(f"{dataset_id} missing evidence JSON: {dataset['evidence_json']}")
        return errors
    if not evidence_markdown_path.exists():
        errors.append(f"{dataset_id} missing evidence Markdown: {dataset['evidence_markdown']}")
        return errors

    data = json.loads(evidence_json_path.read_text(encoding="utf-8"))
    markdown = evidence_markdown_path.read_text(encoding="utf-8")
    input_summary = data.get("input", {})

    checks = [
        (data.get("parity") == "PASS", f"{dataset_id} evidence parity is not PASS"),
        (input_summary.get("sha256") == dataset["sha256"], f"{dataset_id} evidence SHA-256 changed"),
        (digest_file(input_path) == dataset["sha256"], f"{dataset_id} local input SHA-256 changed"),
        (
            input_summary.get("source_url") == dataset["source_url"],
            f"{dataset_id} evidence source URL changed",
        ),
        (
            input_summary.get("source_commit") == dataset["source_commit"],
            f"{dataset_id} evidence source commit changed",
        ),
        ("Picard: `Version:3.4.0`" in markdown, f"{dataset_id} Markdown missing Picard version"),
    ]
    for ok, message in checks:
        if not ok:
            errors.append(message)
    errors.extend(validate_release_candidate_dataset(dataset, input_summary))

    rows = {row["command"]: row for row in data.get("commands", [])}
    for command, comparison in dataset["expected_commands"].items():
        row = rows.get(command)
        if row is None:
            errors.append(f"{dataset_id} missing command evidence: {command}")
            continue
        if row.get("status") != "PASS":
            errors.append(f"{dataset_id} command did not pass: {command}")
        if row.get("comparison") != comparison:
            errors.append(f"{dataset_id} comparison changed for {command}: {row.get('comparison')}")

        markdown_row = f"| {command} | PASS | {comparison} |"
        readme_row = f"| {command} | PASS | {comparison} |"
        if markdown_row not in markdown:
            errors.append(f"{dataset_id} Markdown missing row: {command}")
        if readme_row not in readme:
            errors.append(f"{dataset_id} benchmarks README missing row: {command}")
        if command not in site:
            errors.append(f"{dataset_id} site missing command: {command}")

    for text, target, label in [
        (readme, "benchmarks README", "README"),
        (site, "site", "site"),
    ]:
        for needle, description in [
            (dataset["source_url"], "pinned source URL"),
            (dataset["source_commit"], "source commit"),
            (dataset["sha256"], "input SHA-256"),
            (dataset["evidence_markdown"], "evidence Markdown path"),
            (dataset["scope_caveat"], "scope caveat"),
        ]:
            if needle not in text:
                errors.append(f"{dataset_id} {target} missing {description}")
        if label == "site" and "python3 tools/verify_real_data_evidence.py" not in text:
            errors.append(f"{dataset_id} site missing real-data verifier command")

    return errors


def validate_workflow_docs(readme: str, site: str) -> list[str]:
    errors: list[str] = []
    required = [
        ("python3 tools/compare_real_data.py", "real-data comparator command"),
        ("python3 tools/update_real_data_manifest.py", "manifest update command"),
        ("python3 tools/verify_real_data_evidence.py", "real-data verifier command"),
        (
            "python3 tools/verify_real_data_evidence.py --release-ready",
            "release-ready real-data verifier command",
        ),
        ("release_candidate", "release-candidate tier"),
        ("manifest-entry.json", "manifest-entry artifact"),
    ]
    for text, target in ((readme, "benchmarks README"), (site, "site")):
        for needle, description in required:
            if needle not in text:
                errors.append(f"{target} missing {description}")
    return errors


def validate_real_data_evidence(
    manifest: dict,
    readme: str,
    site: str,
    *,
    release_ready: bool = False,
) -> list[str]:
    errors = validate_manifest(manifest)
    if errors:
        return errors
    errors.extend(validate_workflow_docs(readme, site))
    if release_ready and not any(
        dataset.get("release_tier") == "release_candidate"
        for dataset in manifest["datasets"]
    ):
        errors.append(
            "real-data manifest has no release_candidate dataset for scientist-facing release"
        )
    for dataset in manifest["datasets"]:
        errors.extend(validate_dataset(dataset, readme, site))
    return errors


def main(argv: list[str] | None = None) -> int:
    argv = sys.argv[1:] if argv is None else argv
    allowed_args = {"--release-ready"}
    unknown_args = [arg for arg in argv if arg not in allowed_args]
    if unknown_args:
        print(
            f"usage: {Path(sys.argv[0]).name} [--release-ready]",
            file=sys.stderr,
        )
        return 2
    release_ready = "--release-ready" in argv
    manifest = load_manifest()
    readme = README.read_text(encoding="utf-8")
    site = SITE.read_text(encoding="utf-8")
    errors = validate_real_data_evidence(
        manifest,
        readme,
        site,
        release_ready=release_ready,
    )
    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
