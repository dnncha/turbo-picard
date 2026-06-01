#!/usr/bin/env python3
"""Switch local Bioconda recipes to a tagged release source."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import pathlib
import re
import sys
import tarfile

import yaml


ROOT = pathlib.Path(__file__).resolve().parents[1]
RECIPE_DIRS = [
    ROOT / "packaging" / "bioconda" / "turbo-picard",
    ROOT / "packaging" / "bioconda" / "turbo-picard-picard-shim",
]
BIOCONDA_PR = ROOT / "packaging" / "bioconda" / "BIOCONDA_PR.md"
RELEASE_URL_TEMPLATE = (
    "https://github.com/dnncha/turbo-picard/archive/refs/tags/v{version}.tar.gz"
)
RELEASE_CANDIDATE_PORTFOLIO_REQUIRED_COMMANDS = {
    "AddOrReplaceReadGroups",
    "BuildBamIndex",
    "CleanSam",
    "CollectAlignmentSummaryMetrics",
    "CollectInsertSizeMetrics",
    "CollectQualityYieldMetrics",
    "MarkDuplicates",
    "RevertSam",
    "SamToFastq",
    "SortSam",
    "ValidateSamFile",
    "ViewSam",
}
KNOWN_COMPARISONS = {
    "BAI binary digest",
    "FASTQ trio digest",
    "SAM record digest",
    "SAM record digest plus read-group header digest",
    "coordinate-sorted SAM record multiset digest",
    "duplicate-marking semantic digest plus stable metrics digest",
    "post-command SAM record digest",
    "reverted SAM record digest",
    "stable metrics digest",
    "stable metrics digest with insert-size histogram",
    "summary validation histogram plus exit code",
}


def recipe_version(meta_yaml: str) -> str:
    match = re.search(r'{%\s*set\s+version\s*=\s*"([^"]+)"\s*%}', meta_yaml)
    if not match:
        raise ValueError("meta.yaml is missing a Jinja version declaration")
    return match.group(1)


def release_source_block(version: str, sha256: str) -> str:
    return (
        "source:\n"
        f"  url: {RELEASE_URL_TEMPLATE.format(version=version)}\n"
        f"  sha256: {sha256}\n"
    )


def update_meta_yaml(meta_yaml: str, sha256: str) -> str:
    version = recipe_version(meta_yaml)
    source_pattern = re.compile(r"(?m)^source:\n(?:  [^\n]+\n)+\n?(?=build:\n)")
    if not source_pattern.search(meta_yaml):
        raise ValueError("meta.yaml is missing a source block before build")
    return source_pattern.sub(release_source_block(version, sha256), meta_yaml, count=1)


def update_bioconda_pr(text: str, version: str, sha256: str) -> str:
    url = RELEASE_URL_TEMPLATE.format(version=version)
    url_pattern = re.compile(
        r"https://github\.com/dnncha/turbo-picard/archive/refs/tags/v\d+\.\d+\.\d+\.tar\.gz"
    )
    sha_pattern = re.compile(
        r"(?m)^(\s*)`(?:<github-v\d+\.\d+\.\d+-source-archive-sha256>|[0-9a-f]{64})`"
    )
    if not url_pattern.search(text):
        raise ValueError("BIOCONDA_PR.md is missing the tagged archive URL")
    if not sha_pattern.search(text):
        raise ValueError("BIOCONDA_PR.md is missing the archive SHA-256 field")
    text = url_pattern.sub(url, text, count=1)
    text = sha_pattern.sub(lambda match: f"{match.group(1)}`{sha256}`", text, count=1)
    return text


def sha256_file(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def recipe_versions(recipe_dirs: list[pathlib.Path] | None = None) -> set[str]:
    recipe_dirs = RECIPE_DIRS if recipe_dirs is None else recipe_dirs
    versions = set()
    for recipe_dir in recipe_dirs:
        versions.add(recipe_version((recipe_dir / "meta.yaml").read_text(encoding="utf-8")))
    return versions


def validate_archive_name(path: pathlib.Path, versions: set[str]) -> str | None:
    if len(versions) != 1:
        return "Bioconda recipes do not agree on one release version"
    version = next(iter(versions))
    allowed = {f"v{version}.tar.gz", f"turbo-picard-{version}.tar.gz"}
    if path.name not in allowed:
        return (
            "--archive filename must match the recipe version: "
            + " or ".join(sorted(allowed))
        )
    return None


def validate_archive_contents(path: pathlib.Path, version: str) -> str | None:
    expected_prefix = f"turbo-picard-{version}/"
    required_paths = {
        f"{expected_prefix}Cargo.lock",
        f"{expected_prefix}Cargo.toml",
        f"{expected_prefix}CITATION.cff",
        f"{expected_prefix}benchmarks/real-data/manifest.json",
        f"{expected_prefix}docs/command-matrix.yml",
        f"{expected_prefix}docs/parity.rst",
        f"{expected_prefix}docs/site/assets/benchmark-data.json",
        f"{expected_prefix}packaging/bioconda/turbo-picard/meta.yaml",
        f"{expected_prefix}packaging/bioconda/turbo-picard-picard-shim/meta.yaml",
    }
    try:
        with tarfile.open(path, "r:gz") as archive:
            members = archive.getmembers()
    except tarfile.TarError as error:
        return f"--archive is not a readable gzip tar archive: {error}"

    names = [member.name for member in members]
    seen_names: set[str] = set()
    duplicate_names: set[str] = set()
    for name in names:
        if name in seen_names:
            duplicate_names.add(name)
        seen_names.add(name)
    if duplicate_names:
        return "--archive contains duplicate member path: " + sorted(duplicate_names)[0]

    for member in members:
        name = member.name
        member_path = pathlib.PurePosixPath(name)
        if member_path.is_absolute() or ".." in member_path.parts:
            return f"--archive contains unsafe member path: {name}"
        if not member.isfile() and not member.isdir():
            return f"--archive contains unsupported member type: {name}"

    name_set = set(names)
    if not any(name.startswith(expected_prefix) for name in name_set):
        return f"--archive does not contain expected top-level directory {expected_prefix}"
    expected_root = expected_prefix.rstrip("/")
    unexpected_prefixes = sorted(
        {
            name.split("/", 1)[0] + "/"
            for name in name_set
            if name and name != expected_root and not name.startswith(expected_prefix)
        }
    )
    if unexpected_prefixes:
        return (
            "--archive contains unexpected top-level entries outside "
            f"{expected_prefix}: "
            + ", ".join(unexpected_prefixes)
        )
    missing = sorted(required_paths - name_set)
    if missing:
        return "--archive is missing expected release source files: " + ", ".join(missing)
    members_by_name = {member.name: member for member in members}
    for required_path in sorted(required_paths):
        member = members_by_name[required_path]
        if not member.isfile():
            return f"--archive expected regular file for release source path: {required_path}"
        if member.size <= 0:
            return f"--archive expected non-empty release source file: {required_path}"
    try:
        with tarfile.open(path, "r:gz") as archive:
            contents = {
                required_path: archive.extractfile(required_path).read().decode("utf-8")
                for required_path in required_paths
            }
    except (AttributeError, KeyError, UnicodeDecodeError) as error:
        return f"--archive could not read expected release source file text: {error}"
    content_error = validate_archive_release_metadata(
        contents,
        expected_prefix,
        version,
    )
    if content_error:
        return content_error
    return None


def validate_archive_release_metadata(
    contents: dict[str, str],
    prefix: str,
    version: str,
) -> str | None:
    cargo_toml = contents[f"{prefix}Cargo.toml"]
    if not re.search(
        rf'(?ms)^\[workspace\.package\]\s+.*?^version\s*=\s*"{re.escape(version)}"',
        cargo_toml,
    ):
        return f"--archive Cargo.toml workspace version must be {version}"

    cargo_lock = contents[f"{prefix}Cargo.lock"]
    for crate_name in (
        "turbo-picard-cli",
        "turbo-picard-core",
        "turbo-picard-markdup",
    ):
        lock_entry = re.search(
            rf'(?ms)^\[\[package\]\]\s+.*?^name\s*=\s*"{re.escape(crate_name)}"'
            rf'.*?^version\s*=\s*"([^"]+)"',
            cargo_lock,
        )
        if not lock_entry:
            return f"--archive Cargo.lock missing {crate_name}"
        if lock_entry.group(1) != version:
            return f"--archive Cargo.lock {crate_name} version must be {version}"

    citation = contents[f"{prefix}CITATION.cff"]
    try:
        citation_yaml = yaml.safe_load(citation)
    except yaml.YAMLError as error:
        return f"--archive CITATION.cff is not valid YAML: {error}"
    if not isinstance(citation_yaml, dict):
        return "--archive CITATION.cff must parse as a YAML mapping"
    authors = citation_yaml.get("authors")
    if not isinstance(authors, list) or not authors:
        return "--archive CITATION.cff authors must be a non-empty list"
    if not any(
        isinstance(author, dict)
        and author.get("name") == "turbo-picard contributors"
        for author in authors
    ):
        return "--archive CITATION.cff authors must include turbo-picard contributors"
    keywords = citation_yaml.get("keywords")
    required_keywords = {
        "bioinformatics",
        "genomics",
        "Picard",
        "SAM",
        "BAM",
        "VCF",
        "Rust",
    }
    if not isinstance(keywords, list) or not required_keywords.issubset(set(keywords)):
        return "--archive CITATION.cff keywords must cover bioinformatics, Picard, SAM/BAM/VCF, and Rust"
    for needle, description in [
        ("cff-version: 1.2.0", "CITATION.cff cff-version"),
        ("type: software", "CITATION.cff software type"),
        (f'version: "{version}"', "CITATION.cff version"),
        ('repository-code: "https://github.com/dnncha/turbo-picard"', "CITATION.cff repository"),
        ("archived release", "CITATION.cff archived-release message"),
    ]:
        if needle not in citation:
            return f"--archive {description} must match release metadata"

    command_matrix = contents[f"{prefix}docs/command-matrix.yml"]
    if 'picard_reference: "3.4.0"' not in command_matrix:
        return "--archive command matrix must declare picard_reference 3.4.0"
    if "commands:" not in command_matrix:
        return "--archive command matrix must include commands"

    parity_docs = contents.get(f"{prefix}docs/parity.rst")
    if parity_docs is not None:
        for needle, description in [
            ("What Parity Means", "parity docs title"),
            ("specific command", "command-specific parity scope"),
            ("specific input shape", "input-specific parity scope"),
            ("comparison method", "named comparison method"),
            ("does not mean every Picard behavior", "not-full-Picard disclosure"),
            ("does not prove broad switching safety", "broad switching caveat"),
            ("representative real-data evidence", "representative-data guidance"),
            ("input SHA-256", "input SHA-256 guidance"),
            ("Picard version", "Picard version evidence guidance"),
            ("turbo-picard version", "turbo-picard version evidence guidance"),
            ("tools/compare_real_data.py", "real-data comparator command"),
            (
                "python3 tools/verify_real_data_evidence.py --release-ready",
                "release-ready verifier command",
            ),
        ]:
            if needle not in parity_docs:
                return f"--archive docs/parity.rst missing {description}"

    manifest = contents[f"{prefix}benchmarks/real-data/manifest.json"]
    try:
        manifest_json = json.loads(manifest)
    except ValueError as error:
        return f"--archive real-data manifest is not valid JSON: {error}"
    if not isinstance(manifest_json.get("datasets"), list):
        return "--archive real-data manifest must contain datasets list"
    release_candidate_commands: set[str] = set()
    for dataset in manifest_json["datasets"]:
        if not isinstance(dataset, dict):
            continue
        if dataset.get("release_tier") != "release_candidate":
            continue
        expected_commands = dataset.get("expected_commands", {})
        if isinstance(expected_commands, dict):
            unknown_comparisons = sorted(
                {
                    comparison
                    for comparison in expected_commands.values()
                    if comparison not in KNOWN_COMPARISONS
                }
            )
            if unknown_comparisons:
                return (
                    "--archive real-data manifest has unknown comparison labels: "
                    + ", ".join(unknown_comparisons)
                )
            release_candidate_commands.update(
                command
                for command in expected_commands
                if isinstance(command, str)
            )
    if not release_candidate_commands:
        return "--archive real-data manifest must contain release_candidate command evidence"
    missing_commands = sorted(
        RELEASE_CANDIDATE_PORTFOLIO_REQUIRED_COMMANDS - release_candidate_commands
    )
    if missing_commands:
        return (
            "--archive real-data manifest release_candidate portfolio missing commands: "
            + ", ".join(missing_commands)
        )

    benchmark_data = contents[f"{prefix}docs/site/assets/benchmark-data.json"]
    try:
        benchmark_json = json.loads(benchmark_data)
    except ValueError as error:
        return f"--archive benchmark-data.json is not valid JSON: {error}"
    if not isinstance(benchmark_json.get("benchmarks"), list):
        return "--archive benchmark-data.json must contain benchmarks list"
    if benchmark_json.get("parity") != "32/32 PASS":
        return "--archive benchmark-data.json must report 32/32 PASS parity"
    summary = benchmark_json.get("summary", {})
    if not isinstance(summary, dict):
        return "--archive benchmark-data.json must contain summary object"
    for key in ("floor_speedup", "geometric_mean_speedup", "top_speedup"):
        value = summary.get(key)
        if not isinstance(value, (int, float)):
            return f"--archive benchmark-data.json summary missing numeric {key}"
    benchmark_rows = benchmark_json["benchmarks"]
    command_count = summary.get("command_count")
    parity_pass_count = summary.get("parity_pass_count")
    if command_count != len(benchmark_rows):
        return "--archive benchmark-data.json summary command_count does not match benchmark rows"
    row_pass_count = sum(
        1
        for row in benchmark_rows
        if isinstance(row, dict) and row.get("parity") == "PASS"
    )
    if parity_pass_count != row_pass_count:
        return "--archive benchmark-data.json summary parity_pass_count does not match benchmark rows"
    seen_benchmark_commands: set[str] = set()
    speedups: list[float] = []
    for index, row in enumerate(benchmark_rows):
        if not isinstance(row, dict):
            return f"--archive benchmark-data.json row {index} must be an object"
        command = row.get("command")
        if not isinstance(command, str) or not command:
            return f"--archive benchmark-data.json row {index} missing command"
        if command in seen_benchmark_commands:
            return f"--archive benchmark-data.json duplicate command row: {command}"
        seen_benchmark_commands.add(command)
        speedup = row.get("speedup")
        if not isinstance(speedup, (int, float)) or speedup <= 0:
            return f"--archive benchmark-data.json {command} missing positive speedup"
        speedups.append(float(speedup))
    if speedups:
        if round(float(summary["floor_speedup"]), 2) != round(min(speedups), 2):
            return "--archive benchmark-data.json summary floor_speedup does not match benchmark rows"
        if round(float(summary["top_speedup"]), 2) != round(max(speedups), 2):
            return "--archive benchmark-data.json summary top_speedup does not match benchmark rows"
        geometric_mean = round(math.prod(speedups) ** (1 / len(speedups)), 2)
        if round(float(summary["geometric_mean_speedup"]), 2) != geometric_mean:
            return "--archive benchmark-data.json summary geometric_mean_speedup does not match benchmark rows"

    for recipe_rel in (
        "packaging/bioconda/turbo-picard/meta.yaml",
        "packaging/bioconda/turbo-picard-picard-shim/meta.yaml",
    ):
        recipe = contents[f"{prefix}{recipe_rel}"]
        if f'{{% set version = "{version}" %}}' not in recipe:
            return f"--archive {recipe_rel} version must be {version}"
        if "source:" not in recipe:
            return f"--archive {recipe_rel} must contain source block"

    return None


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description=(
            "Replace each Bioconda recipe's local source.path block with the "
            "GitHub v<version>.tar.gz URL and release archive sha256."
        )
    )
    source = parser.add_mutually_exclusive_group(required=True)
    source.add_argument(
        "--sha256",
        help="SHA-256 of the GitHub source archive for the recipe version tag.",
    )
    source.add_argument(
        "--archive",
        type=pathlib.Path,
        help="Path to the downloaded GitHub source archive; SHA-256 is computed from this file.",
    )
    parser.add_argument(
        "--check",
        action="store_true",
        help="Report recipes that would change without writing files.",
    )
    args = parser.parse_args(argv)

    if args.archive is not None:
        if not args.archive.is_file():
            print(f"--archive does not exist: {args.archive}", file=sys.stderr)
            return 2
        versions = recipe_versions()
        archive_name_error = validate_archive_name(args.archive, versions)
        if archive_name_error:
            print(archive_name_error, file=sys.stderr)
            return 2
        version = next(iter(versions))
        archive_contents_error = validate_archive_contents(args.archive, version)
        if archive_contents_error:
            print(archive_contents_error, file=sys.stderr)
            return 2
        sha256 = sha256_file(args.archive)
    else:
        sha256 = args.sha256

    if not re.fullmatch(r"[0-9a-f]{64}", sha256):
        print("--sha256 must be a lowercase 64-character hex digest", file=sys.stderr)
        return 2

    changed = []
    versions = recipe_versions()
    if len(versions) != 1:
        print("Bioconda recipes do not agree on one release version", file=sys.stderr)
        return 2
    version = next(iter(versions))
    for recipe_dir in RECIPE_DIRS:
        meta_path = recipe_dir / "meta.yaml"
        original = meta_path.read_text(encoding="utf-8")
        updated = update_meta_yaml(original, sha256)
        if updated != original:
            changed.append(meta_path.relative_to(ROOT))
            if not args.check:
                meta_path.write_text(updated, encoding="utf-8")

    if BIOCONDA_PR.is_file():
        original = BIOCONDA_PR.read_text(encoding="utf-8")
        try:
            updated = update_bioconda_pr(original, version, sha256)
        except ValueError as error:
            print(str(error), file=sys.stderr)
            return 2
        if updated != original:
            changed.append(BIOCONDA_PR.relative_to(ROOT))
            if not args.check:
                BIOCONDA_PR.write_text(updated, encoding="utf-8")

    if args.check and changed:
        for path in changed:
            print(f"would update {path}", file=sys.stderr)
        return 1

    for path in changed:
        print(f"updated {path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
