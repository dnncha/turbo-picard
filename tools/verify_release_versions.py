#!/usr/bin/env python3
"""Verify release version references stay aligned across packaging files."""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

import yaml


ROOT = Path(__file__).resolve().parents[1]

RECIPE_PATHS = [
    Path("packaging/bioconda/turbo-picard/meta.yaml"),
    Path("packaging/bioconda/turbo-picard-picard-shim/meta.yaml"),
]

CITATION_PATH = Path("CITATION.cff")

VERSIONED_DOC_PATHS = [
    Path("README.md"),
    Path("docs/citation.rst"),
    Path("docs/packaging.rst"),
    Path("docs/site/index.html"),
    Path("packaging/bioconda/BIOCONDA_PR.md"),
    Path("packaging/bioconda/turbo-picard/README.md"),
    Path("packaging/bioconda/turbo-picard-picard-shim/README.md"),
]


def read(path: Path, root: Path) -> str:
    return (root / path).read_text(encoding="utf-8")


def prose_text(text: str) -> str:
    return re.sub(r"\s+", " ", text)


def workspace_version(root: Path = ROOT) -> str:
    cargo_toml = read(Path("Cargo.toml"), root)
    match = re.search(
        r"(?ms)^\[workspace\.package\]\s+.*?^version\s*=\s*\"([^\"]+)\"",
        cargo_toml,
    )
    if not match:
        raise ValueError("Cargo.toml missing [workspace.package] version")
    return match.group(1)


def cargo_lock_package_versions(text: str) -> dict[str, str]:
    versions: dict[str, str] = {}
    for package in re.findall(r"(?ms)^\[\[package\]\]\s+(.*?)(?=^\[\[package\]\]|\Z)", text):
        name_match = re.search(r'(?m)^name\s*=\s*"([^"]+)"', package)
        version_match = re.search(r'(?m)^version\s*=\s*"([^"]+)"', package)
        if name_match and version_match:
            versions[name_match.group(1)] = version_match.group(1)
    return versions


def cff_scalar(text: str, key: str) -> str | None:
    match = re.search(rf'(?m)^{re.escape(key)}:\s*"?([^"\n]+)"?\s*$', text)
    if not match:
        return None
    return match.group(1).strip()


def collect_errors(root: Path = ROOT) -> list[str]:
    errors: list[str] = []
    version = workspace_version(root)
    tag = f"v{version}"
    archive_url = (
        "https://github.com/dnncha/turbo-picard/archive/refs/tags/"
        f"{tag}.tar.gz"
    )
    archive_sha_placeholder = f"<github-{tag}-source-archive-sha256>"
    archive_filename = f"turbo-picard-{version}.tar.gz"
    github_archive_filename = f"{tag}.tar.gz"
    github_archive_prefix = f"turbo-picard-{version}/"
    repository = "https://github.com/dnncha/turbo-picard"

    lock_path = root / "Cargo.lock"
    if not lock_path.is_file():
        errors.append("Cargo.lock is required for reproducible release builds")
    else:
        lock_versions = cargo_lock_package_versions(lock_path.read_text(encoding="utf-8"))
        for crate_name in [
            "turbo-picard-cli",
            "turbo-picard-core",
            "turbo-picard-markdup",
        ]:
            lock_version = lock_versions.get(crate_name)
            if lock_version is None:
                errors.append(f"Cargo.lock missing {crate_name} package")
            elif lock_version != version:
                errors.append(
                    f"Cargo.lock {crate_name} version {lock_version} "
                    f"must match workspace {version}"
                )

    for cargo_toml in sorted((root / "crates").glob("*/Cargo.toml")):
        text = cargo_toml.read_text(encoding="utf-8")
        rel = cargo_toml.relative_to(root)
        package_block = re.search(r"(?ms)^\[package\]\s+(.*?)(?:^\[|\Z)", text)
        if not package_block:
            errors.append(f"{rel} missing [package] section")
            continue
        package_text = package_block.group(1)
        if not re.search(r"(?m)^version\.workspace\s*=\s*true\s*$", package_text):
            errors.append(f"{rel} package version must inherit workspace version")
        package_version = re.search(r"(?m)^version\s*=\s*\"([^\"]+)\"", text)
        if package_version and package_version.group(1) != version:
            errors.append(f"{rel} package version must match workspace {version}")

        for dep_name, dep_version in re.findall(
            r"(turbo-picard-[\w-]+)\s*=\s*\{[^\n}]*version\s*=\s*\"([^\"]+)\"",
            text,
        ):
            if dep_version != version:
                errors.append(
                    f"{rel} dependency {dep_name} version {dep_version} "
                    f"must match workspace {version}"
                )

    for recipe in RECIPE_PATHS:
        text = read(recipe, root)
        match = re.search(r'{%\s*set\s+version\s*=\s*"([^"]+)"\s*%}', text)
        if not match:
            errors.append(f"{recipe} missing Jinja version declaration")
        elif match.group(1) != version:
            errors.append(
                f"{recipe} version {match.group(1)} must match workspace {version}"
            )

    citation_file = root / CITATION_PATH
    if not citation_file.exists():
        errors.append("CITATION.cff is required for release citation metadata")
    else:
        citation = citation_file.read_text(encoding="utf-8")
        try:
            citation_yaml = yaml.safe_load(citation)
        except yaml.YAMLError as error:
            errors.append(f"CITATION.cff is not valid YAML: {error}")
            citation_yaml = None
        if not isinstance(citation_yaml, dict):
            errors.append("CITATION.cff must parse as a YAML mapping")
            citation_yaml = {}
        authors = citation_yaml.get("authors")
        if not isinstance(authors, list) or not authors:
            errors.append("CITATION.cff authors must be a non-empty list")
        elif not any(
            isinstance(author, dict)
            and author.get("name") == "turbo-picard contributors"
            for author in authors
        ):
            errors.append("CITATION.cff authors must include turbo-picard contributors")
        keywords = citation_yaml.get("keywords")
        if not isinstance(keywords, list) or not {
            "bioinformatics",
            "genomics",
            "Picard",
            "SAM",
            "BAM",
            "VCF",
            "Rust",
        }.issubset(set(keywords)):
            errors.append("CITATION.cff keywords must cover bioinformatics, Picard, SAM/BAM/VCF, and Rust")
        if cff_scalar(citation, "cff-version") != "1.2.0":
            errors.append("CITATION.cff cff-version must be 1.2.0")
        if cff_scalar(citation, "type") != "software":
            errors.append("CITATION.cff type must be software")
        if cff_scalar(citation, "title") != "turbo-picard":
            errors.append("CITATION.cff title must be turbo-picard")
        citation_version = cff_scalar(citation, "version")
        if citation_version != version:
            errors.append(
                f"CITATION.cff version {citation_version or '<missing>'} "
                f"must match workspace {version}"
            )
        if cff_scalar(citation, "repository-code") != repository:
            errors.append(f"CITATION.cff repository-code must be {repository}")
        if cff_scalar(citation, "url") != "https://turbo-picard.readthedocs.io/":
            errors.append("CITATION.cff url must be https://turbo-picard.readthedocs.io/")
        if cff_scalar(citation, "license") != "MIT":
            errors.append("CITATION.cff license must match workspace MIT")
        if "authors:" not in citation or "turbo-picard contributors" not in citation:
            errors.append("CITATION.cff must include turbo-picard contributors author")
        if "archived release" not in prose_text(citation):
            errors.append("CITATION.cff message must ask users to cite the archived release")
        required_terms = ["Picard", "parity", "evidence"]
        citation_prose = prose_text(citation)
        missing_terms = [term for term in required_terms if term not in citation_prose]
        if missing_terms:
            errors.append(
                "CITATION.cff message or abstract must mention "
                + ", ".join(missing_terms)
            )
        input_citation_terms = [
            "source_url",
            "source URL",
            "source commit",
            "full Git commit",
            "SHA-256",
            "sha256",
            "benchmark input",
            "validation input",
            "NA12878",
            "snvq_metrics_test.bam",
        ]
        leaked_terms = [term for term in input_citation_terms if term in citation]
        if leaked_terms:
            errors.append(
                "CITATION.cff must cite only the software release; "
                "move input-data citation details to evidence manifests/docs: "
                + ", ".join(leaked_terms)
            )

    for doc in VERSIONED_DOC_PATHS:
        text = read(doc, root)
        prose = prose_text(text)
        if doc in {Path("README.md"), Path("docs/citation.rst")}:
            if "CITATION.cff" not in text:
                errors.append(f"{doc} must mention CITATION.cff")
            if "input" not in prose or "SHA-256" not in text:
                errors.append(
                    f"{doc} must distinguish software citation from pinned input data"
                )
        if doc == Path("docs/citation.rst"):
            for needle, description in [
                ("archived release", "archived-release citation rule"),
                ("command-level", "command-level parity evidence rule"),
                ("Picard 3.4.0", "Picard evidence version"),
                ("exact command surfaces", "methods command-surface rule"),
                ("unsupported surfaces", "methods fallback disclosure rule"),
                ("evidence reports", "methods evidence-report rule"),
                ("full Git commit", "full Git commit citation rule"),
                ("does not cite", "CITATION.cff input-data boundary"),
            ]:
                if needle not in prose:
                    errors.append(f"{doc} must mention {description}")
        if re.search(r"github-v\d+\.\d+\.\d+-source-archive-sha256", text):
            if archive_sha_placeholder not in text:
                errors.append(f"{doc} archive sha256 placeholder must use {tag}")
        if "archive/refs/tags/" in text and archive_url not in text:
            errors.append(f"{doc} archive URL must use {tag}")
        for found_archive in sorted(
            set(re.findall(r"turbo-picard-\d+\.\d+\.\d+\.tar\.gz", text))
        ):
            if found_archive != archive_filename:
                errors.append(f"{doc} archive command must use {version}")
        for found_archive in sorted(set(re.findall(r"v\d+\.\d+\.\d+\.tar\.gz", text))):
            if found_archive != github_archive_filename:
                errors.append(f"{doc} GitHub archive filename must use {tag}")
        if "prepare_bioconda_release.py" in text and "filename" in text:
            if archive_filename not in text:
                errors.append(
                    f"{doc} release-helper filename note must mention {archive_filename}"
                )
            if github_archive_filename not in text:
                errors.append(
                    f"{doc} release-helper filename note must mention {github_archive_filename}"
                )
        if "prepare_bioconda_release.py" in text and "top-level" in text:
            if github_archive_prefix not in text:
                errors.append(
                    f"{doc} release-helper archive layout note must mention {github_archive_prefix}"
                )
            required_archive_paths = [
                "Cargo.toml",
                "Cargo.lock",
                "CITATION.cff",
                "docs/command-matrix.yml",
                "docs/parity.rst",
                "benchmarks/real-data/manifest.json",
                "docs/site/assets/benchmark-data.json",
                "packaging/bioconda/turbo-picard/meta.yaml",
                "packaging/bioconda/turbo-picard-picard-shim/meta.yaml",
            ]
            missing_archive_paths = [
                path for path in required_archive_paths if path not in text
            ]
            if missing_archive_paths:
                errors.append(
                    f"{doc} release-helper archive layout note must mention "
                    + ", ".join(missing_archive_paths)
                )
            required_policy_terms = [
                "unsafe paths",
                "duplicate",
                "unsupported tar member types",
                "empty required source files",
            ]
            missing_policy_terms = [
                term for term in required_policy_terms if term not in prose
            ]
            if missing_policy_terms:
                errors.append(
                    f"{doc} release-helper archive policy note must mention "
                    + ", ".join(missing_policy_terms)
                )
            required_metadata_terms = [
                ("workspace version", "workspace version"),
                ("CITATION.cff", "CITATION.cff"),
                ("archived-release", "archived-release", "archived release"),
                ("picard_reference", "picard_reference"),
                ("Picard 3.4.0", "Picard 3.4.0"),
                ("datasets", "datasets"),
                ("benchmarks", "benchmarks"),
                ("recipe version", "recipe version"),
                ("source block", "source block"),
            ]
            missing_metadata_terms = [
                label
                for label, *terms in required_metadata_terms
                if all(term not in text and term not in prose for term in terms)
            ]
            if missing_metadata_terms:
                errors.append(
                    f"{doc} release-helper archive metadata note must mention "
                    + ", ".join(missing_metadata_terms)
                )
        if "prepare_bioconda_release.py" in text and "PR body" in text:
            if "BIOCONDA_PR.md" not in text:
                errors.append(
                    f"{doc} release-helper PR body note must mention BIOCONDA_PR.md"
                )
        if "prepare_bioconda_release.py" in text and "--sha256" in text:
            if "Prefer" not in text or "--archive" not in text:
                errors.append(
                    f"{doc} release-helper sha256 fallback must prefer --archive"
                )
            if "downloaded GitHub source archive" not in prose:
                errors.append(
                    f"{doc} release-helper sha256 fallback must tie digest to "
                    "the downloaded GitHub source archive"
                )
            if "skips archive filename and content validation" not in prose:
                errors.append(
                    f"{doc} release-helper sha256 fallback must disclose skipped "
                    "archive validation"
                )

    return errors


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.parse_args(argv)

    errors = collect_errors()
    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1
    print("release version references are consistent")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
