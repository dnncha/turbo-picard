#!/usr/bin/env python3
"""Validate Bioconda-oriented recipe readiness for turbo-picard."""

from __future__ import annotations

import pathlib
import re
import sys
import json


ROOT = pathlib.Path(__file__).resolve().parents[1]
COMMAND_MATRIX = ROOT / "docs" / "command-matrix.yml"
RECIPES = [
    {
        "name": "turbo-picard",
        "path": ROOT / "packaging" / "bioconda" / "turbo-picard",
        "expected_bin": "turbo-picard",
        "is_shim": False,
    },
    {
        "name": "turbo-picard-picard-shim",
        "path": ROOT / "packaging" / "bioconda" / "turbo-picard-picard-shim",
        "expected_bin": "picard",
        "is_shim": True,
    },
]
RELEASE_URL_TEMPLATE = (
    "https://github.com/dnncha/turbo-picard/archive/refs/tags/v{version}.tar.gz"
)
RELEASE_CANDIDATE_PORTFOLIO_COMMAND_TEXT = (
    "AddOrReplaceReadGroups, BuildBamIndex, CleanSam, "
    "CollectAlignmentSummaryMetrics, CollectInsertSizeMetrics, "
    "CollectQualityYieldMetrics, MarkDuplicates, RevertSam, SamToFastq, "
    "SortSam, ValidateSamFile, ViewSam"
)
BIOCONDA_PR_OVERCLAIM_PHRASES = [
    "drop-in replacement",
    "production genomics workflows",
    "validated for all cohorts",
    "safe for all cohorts",
    "safe for all production",
    "proves safe to switch",
    "complete cohort-scale validation",
]


def has_cargo_install_flag(build_sh: str, flag_pattern: str) -> bool:
    compact = re.sub(r"\\\s*\n\s*", " ", build_sh)
    compact = re.sub(r"\s+", " ", compact)
    return bool(re.search(r"\bcargo install\b.*" + flag_pattern, compact))


def bioconda_pr_overclaims(text: str) -> list[str]:
    normalized = re.sub(r"\s+", " ", text).lower()
    overclaims: list[str] = []
    for phrase in BIOCONDA_PR_OVERCLAIM_PHRASES:
        start = 0
        while True:
            index = normalized.find(phrase, start)
            if index == -1:
                break
            context = normalized[max(0, index - 80) : index]
            if not (
                "do not describe" in context
                or "do not claim" in context
                or "not " in context[-16:]
            ):
                overclaims.append(phrase)
                break
            start = index + len(phrase)
    return overclaims


def replacement_overclaims(text: str) -> list[str]:
    return bioconda_pr_overclaims(text)


def recipe_version(meta_yaml: str) -> str | None:
    match = re.search(r'{%\s*set\s+version\s*=\s*"([^"]+)"\s*%}', meta_yaml)
    if match:
        return match.group(1)
    match = re.search(r"(?m)^\s+version:\s+([^\s#]+)\s*$", meta_yaml)
    if match:
        return match.group(1).strip("\"'")
    return None


def recipe_jinja_name(meta_yaml: str) -> str | None:
    match = re.search(r'{%\s*set\s+name\s*=\s*"([^"]+)"\s*%}', meta_yaml)
    if match:
        return match.group(1)
    return None


def recipe_package_name(meta_yaml: str) -> str | None:
    jinja_name = recipe_jinja_name(meta_yaml)
    match = re.search(r"(?m)^\s+name:\s+(.+?)\s*$", meta_yaml)
    if not match:
        return None
    value = match.group(1).strip().strip("\"'")
    if value == "{{ name|lower }}" and jinja_name:
        return jinja_name.lower()
    return value


def recipe_summary(meta_yaml: str) -> str | None:
    match = re.search(r"(?m)^\s+summary:\s+(.+?)\s*$", meta_yaml)
    if match:
        return match.group(1).strip().strip("\"'")
    return None


def recipe_source_url(meta_yaml: str) -> str | None:
    match = re.search(r"(?m)^\s+url:\s+(https://\S+)\s*$", meta_yaml)
    if match:
        return match.group(1)
    return None


def recipe_source_sha256(meta_yaml: str) -> str | None:
    match = re.search(r"(?m)^\s+sha256:\s+([0-9a-f]{64})\s*$", meta_yaml)
    if match:
        return match.group(1)
    return None


def pr_source_url(text: str) -> str | None:
    match = re.search(
        r"https://github\.com/dnncha/turbo-picard/archive/refs/tags/v\d+\.\d+\.\d+\.tar\.gz",
        text,
    )
    if match:
        return match.group(0)
    return None


def pr_source_sha256(text: str) -> str | None:
    match = re.search(r"Archive SHA-256:\s*\n\s*`([0-9a-f]{64})`", text)
    if match:
        return match.group(1)
    return None


def has_meta_test_command(meta_yaml: str, command: str) -> bool:
    return bool(
        re.search(rf"(?m)^\s*-\s+{re.escape(command)}\s*$", meta_yaml)
    )


def benchmark_pr_text_requirements(root: pathlib.Path = ROOT) -> tuple[list[str], list[str]]:
    benchmark_data_path = root / "docs" / "site" / "assets" / "benchmark-data.json"
    if not benchmark_data_path.is_file():
        return ["docs/site/assets/benchmark-data.json"], [
            "docs/site/assets/benchmark-data.json is missing"
        ]
    try:
        data = json.loads(benchmark_data_path.read_text(encoding="utf-8"))
    except ValueError as error:
        return ["docs/site/assets/benchmark-data.json"], [
            f"docs/site/assets/benchmark-data.json is not valid JSON: {error}"
        ]
    errors: list[str] = []
    if not isinstance(data, dict):
        return ["docs/site/assets/benchmark-data.json"], [
            "docs/site/assets/benchmark-data.json must be a JSON object"
        ]
    summary = data.get("summary", {})
    if not isinstance(summary, dict):
        errors.append("docs/site/assets/benchmark-data.json missing summary object")
        summary = {}
    benchmarks = data.get("benchmarks", [])
    if not isinstance(benchmarks, list):
        errors.append("docs/site/assets/benchmark-data.json missing benchmarks list")
        benchmarks = []
    required = [
        "Benchmark evidence",
        "docs/site/assets/benchmark-data.json",
        str(data.get("source", "")),
        str(data.get("date", "")),
        str(data.get("parity", "")),
        str(data.get("source_artifact", "")),
    ]
    for key in (
        "top_speedup",
        "top_command",
        "floor_speedup",
        "floor_command",
        "median_speedup",
        "geometric_mean_speedup",
    ):
        value = summary.get(key)
        if isinstance(value, float):
            required.append(f"{value:.2f}x")
        elif value is not None:
            required.append(str(value))
    benchmark_commands = {
        row.get("command")
        for row in benchmarks
        if isinstance(row, dict) and isinstance(row.get("command"), str)
    }
    for promoted_command in (
        "IntervalListTools",
        "LiftoverVcf",
        "CollectMultipleMetrics",
        "CollectGcBiasMetrics",
    ):
        if promoted_command in benchmark_commands:
            required.append(promoted_command)
        else:
            required.append(promoted_command)
    return [needle for needle in required if needle], errors


def required_benchmark_pr_text(root: pathlib.Path = ROOT) -> list[str]:
    required, _errors = benchmark_pr_text_requirements(root)
    return required


def command_matrix_native_commands(matrix_text: str) -> list[str]:
    commands: list[str] = []
    current_name: str | None = None
    for line in matrix_text.splitlines():
        name_match = re.match(r"\s*-\s+name:\s+(\S+)", line)
        if name_match:
            current_name = name_match.group(1)
            continue
        status_match = re.match(r"\s+status:\s+(native|partial-native)\s*$", line)
        if status_match and current_name:
            commands.append(current_name)
            current_name = None
    return commands


def validate_main_run_test_command_surface(
    *,
    run_test_sh: str,
    matrix_text: str,
    expected_bin: str = "turbo-picard",
) -> list[str]:
    errors: list[str] = []
    for command in command_matrix_native_commands(matrix_text):
        if f"{expected_bin} {command}" not in run_test_sh:
            errors.append(f"turbo-picard run_test.sh missing command smoke: {command}")
    return errors


def validate_main_meta_test_command_surface(
    *,
    meta_yaml: str,
    matrix_text: str,
    expected_bin: str = "turbo-picard",
) -> list[str]:
    errors: list[str] = []
    for command in command_matrix_native_commands(matrix_text):
        smoke = f"{expected_bin} {command} --help"
        if not has_meta_test_command(meta_yaml, smoke):
            errors.append(f"turbo-picard meta.yaml missing command smoke: {command}")
    return errors


def validate_recipe(
    *,
    name: str,
    meta_yaml: str,
    build_sh: str,
    run_test_sh: str,
    expected_bin: str,
    is_shim: bool,
    release_ready: bool = False,
) -> list[str]:
    errors: list[str] = []
    package_name = recipe_package_name(meta_yaml)
    if package_name != name:
        if package_name:
            errors.append(f"{name} meta.yaml package.name is {package_name}, expected {name}")
        else:
            errors.append(f"{name} meta.yaml missing package.name")
    summary = recipe_summary(meta_yaml)
    if not summary:
        errors.append(f"{name} meta.yaml missing non-empty summary")
    else:
        summary_lower = summary.lower()
        if is_shim:
            if "shim" not in summary_lower or "picard" not in summary_lower:
                errors.append(
                    f"{name} meta.yaml summary must describe the opt-in picard shim"
                )
        elif "picard-compatible" not in summary_lower:
            errors.append(
                f"{name} meta.yaml summary must describe the Picard-compatible toolkit"
            )
    if release_ready:
        version = recipe_version(meta_yaml)
        expected_url = RELEASE_URL_TEMPLATE.format(version=version) if version else None
        if re.search(r"(?m)^\s+path:\s+", meta_yaml):
            errors.append(f"{name} meta.yaml still uses local source.path")
        source_url = recipe_source_url(meta_yaml)
        if not source_url:
            errors.append(f"{name} meta.yaml missing release source url")
        elif expected_url and source_url != expected_url:
            errors.append(
                f"{name} meta.yaml release source url must be {expected_url}"
            )
        if not re.search(r"(?m)^\s+sha256:\s+[0-9a-f]{64}\s*$", meta_yaml):
            errors.append(f"{name} meta.yaml missing release source sha256")
    if "number: 0" not in meta_yaml:
        errors.append(f"{name} meta.yaml should use build number 0 before first submission")
    if "skip: true  # [win]" not in meta_yaml:
        errors.append(f"{name} meta.yaml missing Windows skip selector")
    if re.search(r"(?m)^\s*noarch\s*:", meta_yaml):
        errors.append(f"{name} meta.yaml must not use noarch for compiled Rust binaries")
    if "{{ compiler('rust') }}" not in meta_yaml:
        errors.append(f"{name} meta.yaml missing {{{{ compiler('rust') }}}}")
    if "cargo-bundle-licenses" not in meta_yaml:
        errors.append(f"{name} meta.yaml missing cargo-bundle-licenses")
    if "license_file:" not in meta_yaml:
        errors.append(f"{name} meta.yaml missing license_file metadata")
    if "    - LICENSE" not in meta_yaml:
        errors.append(f"{name} meta.yaml missing LICENSE license_file")
    if "    - THIRDPARTY.yml" not in meta_yaml:
        errors.append(f"{name} meta.yaml missing THIRDPARTY.yml license_file")
    if "license: MIT" not in meta_yaml:
        errors.append(f"{name} meta.yaml missing MIT license metadata")
    if "home: https://github.com/dnncha/turbo-picard" not in meta_yaml:
        errors.append(f"{name} meta.yaml missing project home URL")
    if "doc_url: https://turbo-picard.readthedocs.io/" not in meta_yaml:
        errors.append(f"{name} meta.yaml missing documentation URL")
    if "dev_url: https://github.com/dnncha/turbo-picard" not in meta_yaml:
        errors.append(f"{name} meta.yaml missing source development URL")
    if "CITATION.cff" not in meta_yaml:
        errors.append(f"{name} meta.yaml description missing CITATION.cff")
    normalized_meta = re.sub(r"\s+", " ", meta_yaml)
    if "real-data evidence citations" not in normalized_meta:
        errors.append(f"{name} meta.yaml description missing evidence citation boundary")
    if "replace-with" in meta_yaml or "placeholder" in meta_yaml:
        errors.append(f"{name} meta.yaml contains maintainer placeholder")
    for phrase in replacement_overclaims(meta_yaml):
        errors.append(f"{name} meta.yaml contains unsupported replacement overclaim: {phrase}")
    if "cargo-bundle-licenses" not in build_sh:
        errors.append(f"{name} build.sh missing cargo-bundle-licenses invocation")
    if "cargo-bundle-licenses --format yaml --output THIRDPARTY.yml" not in re.sub(
        r"\s+", " ", build_sh
    ):
        errors.append(
            f"{name} build.sh must write bundled Rust dependency licenses to THIRDPARTY.yml"
        )
    if "OPENSSL_NO_VENDOR=1" not in build_sh:
        errors.append(f"{name} build.sh missing OPENSSL_NO_VENDOR=1")
    if "CARGO_NET_GIT_FETCH_WITH_CLI=true" not in build_sh:
        errors.append(f"{name} build.sh missing CARGO_NET_GIT_FETCH_WITH_CLI=true")
    if not has_cargo_install_flag(build_sh, r"--locked\b"):
        errors.append(f"{name} build.sh missing cargo install --locked")
    if not has_cargo_install_flag(build_sh, r"--no-track\b"):
        errors.append(f"{name} build.sh missing cargo install --no-track")
    if not has_cargo_install_flag(build_sh, r"--root\s+\"?\$\{PREFIX\}\"?"):
        errors.append(f"{name} build.sh missing cargo install --root ${{PREFIX}}")
    if not has_cargo_install_flag(build_sh, rf"--bin\s+{re.escape(expected_bin)}\b"):
        errors.append(f"{name} build.sh missing cargo install --bin {expected_bin}")
    if f"{expected_bin} --version" not in run_test_sh:
        errors.append(f"{name} run_test.sh missing {expected_bin} smoke test")
    if is_shim:
        if not has_meta_test_command(meta_yaml, "picard --version"):
            errors.append(f"{name} meta.yaml test commands missing picard --version")
        if not has_meta_test_command(meta_yaml, "picard MarkDuplicates --help"):
            errors.append(
                f"{name} meta.yaml test commands missing picard MarkDuplicates --help"
            )
        if has_meta_test_command(meta_yaml, "turbo-picard --version"):
            errors.append(f"{name} meta.yaml shim tests must not use turbo-picard")
        if has_cargo_install_flag(build_sh, r"--bin\s+turbo-picard\b"):
            errors.append(f"{name} build.sh must not install turbo-picard")
        if "intentionally conflicts with upstream picard" not in meta_yaml:
            errors.append(
                f"{name} meta.yaml description must disclose upstream picard conflict"
            )
        if "shadows the" not in meta_yaml or "same command name" not in meta_yaml:
            errors.append(
                f"{name} meta.yaml description must disclose picard command shadowing"
            )
        if "{{ pin_subpackage('turbo-picard', exact=True) }}" not in meta_yaml:
            errors.append(
                f"{name} meta.yaml missing exact turbo-picard pin_subpackage run dependency"
            )
        if "picard ==0" not in meta_yaml:
            errors.append(f"{name} meta.yaml missing picard ==0 run_constrained conflict")
        if "picard MarkDuplicates --help" not in run_test_sh:
            errors.append(f"{name} run_test.sh missing picard MarkDuplicates smoke test")
    else:
        if not has_meta_test_command(meta_yaml, "turbo-picard --version"):
            errors.append(f"{name} meta.yaml test commands missing turbo-picard --version")
        if has_meta_test_command(meta_yaml, "picard --version") or has_meta_test_command(
            meta_yaml,
            "picard MarkDuplicates --help",
        ):
            errors.append(f"{name} meta.yaml main package tests must not use picard shim")
        if has_cargo_install_flag(build_sh, r"--bin\s+picard\b"):
            errors.append(f"{name} build.sh must not install picard shim")
        if "{{ pin_subpackage('turbo-picard', exact=True) }}" in meta_yaml:
            errors.append(f"{name} meta.yaml must not pin itself as a shim dependency")
        if "picard ==0" in meta_yaml:
            errors.append(f"{name} meta.yaml must not declare shim-only picard conflict")
        if "non-shadowing turbo-picard" not in normalized_meta:
            errors.append(
                f"{name} meta.yaml description must disclose non-shadowing entrypoint"
            )
        for command in ("MarkDuplicates", "SortSam", "CleanSam", "ViewSam"):
            smoke = f"{expected_bin} {command} --help"
            if smoke not in run_test_sh:
                errors.append(f"{name} run_test.sh missing {smoke} smoke test")
    return errors


def validate_recipe_set_consistency(recipe_metadata: list[tuple[str, str | None]]) -> list[str]:
    versions = {
        name: version
        for name, version in recipe_metadata
        if version is not None
    }
    if len(versions) < len(recipe_metadata):
        missing = [
            name
            for name, version in recipe_metadata
            if version is None
        ]
        return [f"{name} meta.yaml missing version" for name in missing]
    unique_versions = sorted(set(versions.values()))
    if len(unique_versions) > 1:
        details = ", ".join(
            f"{name}={version}" for name, version in sorted(versions.items())
        )
        return [f"Bioconda recipe versions differ: {details}"]
    return []


def validate_release_evidence(root: pathlib.Path = ROOT) -> list[str]:
    errors: list[str] = []
    release_candidates: list[dict] = []
    recipe_sources: list[tuple[str, str | None, str | None]] = []
    for recipe in RECIPES:
        meta_yaml_path = pathlib.Path(recipe["path"])
        if not meta_yaml_path.is_absolute():
            meta_yaml_path = root / meta_yaml_path
        else:
            try:
                meta_yaml_path = root / meta_yaml_path.relative_to(ROOT)
            except ValueError:
                pass
        meta_yaml_path = meta_yaml_path / "meta.yaml"
        if not meta_yaml_path.is_file():
            continue
        meta_yaml = meta_yaml_path.read_text(encoding="utf-8")
        recipe_sources.append(
            (
                str(recipe["name"]),
                recipe_source_url(meta_yaml),
                recipe_source_sha256(meta_yaml),
            )
        )
    manifest_path = root / "benchmarks" / "real-data" / "manifest.json"
    verifier_path = root / "tools" / "verify_real_data_evidence.py"
    readme_path = root / "README.md"
    site_path = root / "docs" / "site" / "index.html"
    docs_packaging_path = root / "docs" / "packaging.rst"
    packaging_readme_path = root / "packaging" / "bioconda" / "turbo-picard" / "README.md"
    packaging_shim_readme_path = (
        root / "packaging" / "bioconda" / "turbo-picard-picard-shim" / "README.md"
    )
    packaging_pr_path = root / "packaging" / "bioconda" / "BIOCONDA_PR.md"
    if not manifest_path.is_file():
        errors.append("release evidence manifest missing benchmarks/real-data/manifest.json")
    else:
        manifest_text = manifest_path.read_text(encoding="utf-8")
        if '"datasets"' not in manifest_text:
            errors.append("release evidence manifest missing datasets list")
        try:
            manifest = json.loads(manifest_text)
        except ValueError:
            errors.append("release evidence manifest is not valid JSON")
        else:
            if not isinstance(manifest, dict):
                errors.append("release evidence manifest must be a JSON object")
                manifest = {}
            datasets = manifest.get("datasets", [])
            if not isinstance(datasets, list):
                errors.append("release evidence manifest datasets must be a list")
                datasets = []
            release_candidates = [
                dataset
                for dataset in datasets
                if isinstance(dataset, dict)
                and dataset.get("release_tier") == "release_candidate"
            ]
            if not release_candidates:
                errors.append(
                    "release evidence manifest has no release_candidate dataset"
                )
    if not verifier_path.is_file():
        errors.append("release evidence verifier missing tools/verify_real_data_evidence.py")
    for path in (
        readme_path,
        site_path,
        docs_packaging_path,
        packaging_readme_path,
        packaging_shim_readme_path,
        packaging_pr_path,
    ):
        if not path.is_file():
            errors.append(f"release evidence documentation missing {path.relative_to(root)}")
            continue
        text = path.read_text(encoding="utf-8")
        is_shim_readme = path == packaging_shim_readme_path
        if not is_shim_readme and "python3 tools/verify_real_data_evidence.py" not in text:
            errors.append(
                f"{path.relative_to(root)} missing real-data evidence verifier command"
            )
        if not is_shim_readme and "python3 tools/verify_real_data_evidence.py --release-ready" not in text:
            errors.append(
                f"{path.relative_to(root)} missing release-ready real-data verifier command"
            )
        if not is_shim_readme and "python3 tools/update_real_data_manifest.py" not in text:
            errors.append(
                f"{path.relative_to(root)} missing real-data manifest update command"
            )
        if path in {
            docs_packaging_path,
            packaging_readme_path,
            packaging_shim_readme_path,
            packaging_pr_path,
        }:
            normalized = re.sub(r"\s+", " ", text)
            normalized_lower = normalized.lower()
            for phrase in replacement_overclaims(text):
                errors.append(
                    f"{path.relative_to(root)} contains unsupported replacement overclaim: {phrase}"
                )
            for needle in (
                "CITATION.cff",
                "archived turbo-picard release",
                "SHA-256",
            ):
                if needle not in text:
                    errors.append(
                        f"{path.relative_to(root)} missing citation boundary text: {needle}"
                    )
            if "benchmark" not in normalized or "input" not in normalized or "separate" not in normalized:
                errors.append(
                    f"{path.relative_to(root)} missing software-vs-input citation boundary"
                )
            for needle in (
                "cp -R packaging/bioconda/turbo-picard recipes/turbo-picard",
                "cp -R packaging/bioconda/turbo-picard-picard-shim recipes/turbo-picard-picard-shim",
                "bioconda-utils lint recipes config.yml --packages turbo-picard turbo-picard-picard-shim",
                "bioconda-utils build --docker --mulled-test turbo-picard",
                "bioconda-utils build --docker --mulled-test turbo-picard-picard-shim",
            ):
                if needle not in text:
                    errors.append(
                        f"{path.relative_to(root)} missing Bioconda submission command: {needle}"
                    )
            for needle in (
                "python3 tools/verify_benchmark_thresholds.py",
                "5.00x",
                "20.00x",
                "50.00x",
            ):
                if needle not in text:
                    errors.append(
                        f"{path.relative_to(root)} missing benchmark threshold release gate: {needle}"
                    )
            if RELEASE_CANDIDATE_PORTFOLIO_COMMAND_TEXT not in normalized:
                errors.append(
                    f"{path.relative_to(root)} missing release-candidate command portfolio"
                )
            if path in {docs_packaging_path, packaging_readme_path}:
                if (
                    "do not open a bioconda pr" not in normalized_lower
                    or "source.path" not in normalized_lower
                    or "release-ready verifier" not in normalized_lower
                ):
                    errors.append(
                        f"{path.relative_to(root)} missing source.path Bioconda PR stop sign"
                    )
        if path == packaging_pr_path:
            if re.search(r"<github-v\d+\.\d+\.\d+-source-archive-sha256>", text):
                errors.append(
                    "packaging/bioconda/BIOCONDA_PR.md still contains source archive SHA placeholder"
                )
            pr_url = pr_source_url(text)
            pr_sha256 = pr_source_sha256(text)
            if not pr_url:
                errors.append(
                    "packaging/bioconda/BIOCONDA_PR.md missing tagged source archive URL"
                )
            if not pr_sha256:
                errors.append(
                    "packaging/bioconda/BIOCONDA_PR.md missing concrete source archive SHA-256"
                )
            for recipe_name, recipe_url, recipe_sha256 in recipe_sources:
                if recipe_url and pr_url and recipe_url != pr_url:
                    errors.append(
                        f"packaging/bioconda/BIOCONDA_PR.md source URL does not match {recipe_name} recipe"
                    )
                if recipe_sha256 and pr_sha256 and recipe_sha256 != pr_sha256:
                    errors.append(
                        f"packaging/bioconda/BIOCONDA_PR.md source SHA-256 does not match {recipe_name} recipe"
                    )
            required_pr_text = [
                "python3 tools/bioconda_release_preflight.py",
                "python3 -m unittest discover tools",
                "python3 tools/update_real_data_manifest.py",
                "python3 tools/prepare_bioconda_release.py",
                "--archive ~/Downloads/turbo-picard-",
                "Prefer `--archive` for release submission",
                "python3 tools/verify_bioconda_recipes.py --release-ready",
                "python3 tools/verify_release_versions.py",
                "python3 tools/verify_benchmark_suite_coverage.py",
                "python3 tools/verify_benchmark_thresholds.py",
                "python3 tools/verify_ci_coverage.py",
                "python3 tools/verify_parity_docs.py",
                "python3 tools/verify_readme_links.py",
                "python3 tools/verify_site_links.py",
                "./tools/verify_package_install.sh",
                "cargo test --workspace",
                "cp -R packaging/bioconda/turbo-picard recipes/turbo-picard",
                "cp -R packaging/bioconda/turbo-picard-picard-shim recipes/turbo-picard-picard-shim",
                "bioconda-utils lint recipes config.yml --packages turbo-picard turbo-picard-picard-shim",
                "bioconda-utils build --docker --mulled-test turbo-picard",
                "bioconda-utils build --docker --mulled-test turbo-picard-picard-shim",
                "Recipe notes",
                "not `noarch`",
                "skip: true  # [win]",
                "cargo-bundle-licenses --format yaml --output THIRDPARTY.yml",
                "license_file",
                "THIRDPARTY.yml",
                "{{ pin_subpackage('turbo-picard', exact=True) }}",
                "picard ==0",
                "run_constrained",
            ]
            benchmark_requirements, benchmark_errors = benchmark_pr_text_requirements(root)
            errors.extend(benchmark_errors)
            required_pr_text.extend(benchmark_requirements)
            for dataset in release_candidates:
                dataset_id = dataset.get("id", "<missing>")
                for key in (
                    "evidence_markdown",
                    "evidence_json",
                    "source_url",
                    "source_commit",
                    "sha256",
                    "scope_caveat",
                    "minimum_input_bytes",
                ):
                    value = dataset.get(key)
                    if not value:
                        errors.append(
                            f"release_candidate {dataset_id} missing manifest key for Bioconda PR: {key}"
                        )
                    else:
                        required_pr_text.append(str(value))
                expected_commands = dataset.get("expected_commands", {})
                if isinstance(expected_commands, dict):
                    for command, comparison in expected_commands.items():
                        required_pr_text.append(str(command))
                        required_pr_text.append(str(comparison))
            for needle in required_pr_text:
                if needle not in text:
                    errors.append(
                        f"{path.relative_to(root)} missing Bioconda PR evidence text: {needle}"
                    )
    return errors


def main(argv: list[str] | None = None) -> int:
    argv = sys.argv[1:] if argv is None else argv
    allowed_args = {"--release-ready"}
    unknown_args = [arg for arg in argv if arg not in allowed_args]
    if unknown_args:
        print(
            f"usage: {pathlib.Path(sys.argv[0]).name} [--release-ready]",
            file=sys.stderr,
        )
        return 2
    release_ready = "--release-ready" in argv
    errors: list[str] = []
    if release_ready:
        errors.extend(validate_release_evidence())
    recipe_metadata: list[tuple[str, str | None]] = []
    for recipe in RECIPES:
        recipe_dir = recipe["path"]
        missing_files = []
        for filename in ("meta.yaml", "build.sh", "run_test.sh"):
            if not (recipe_dir / filename).is_file():
                missing_files.append(f"{recipe['name']} missing {filename}")
        errors.extend(missing_files)
        if missing_files:
            continue
        meta_yaml = (recipe_dir / "meta.yaml").read_text(encoding="utf-8")
        recipe_metadata.append((str(recipe["name"]), recipe_version(meta_yaml)))
        errors.extend(
            validate_recipe(
                name=str(recipe["name"]),
                meta_yaml=meta_yaml,
                build_sh=(recipe_dir / "build.sh").read_text(encoding="utf-8"),
                run_test_sh=(recipe_dir / "run_test.sh").read_text(encoding="utf-8"),
                expected_bin=str(recipe["expected_bin"]),
                is_shim=bool(recipe["is_shim"]),
                release_ready=release_ready,
            )
        )
        if not bool(recipe["is_shim"]):
            if not COMMAND_MATRIX.is_file():
                errors.append("missing docs/command-matrix.yml for package smoke coverage")
            else:
                errors.extend(
                    validate_main_run_test_command_surface(
                        run_test_sh=(recipe_dir / "run_test.sh").read_text(
                            encoding="utf-8"
                        ),
                        matrix_text=COMMAND_MATRIX.read_text(encoding="utf-8"),
                        expected_bin=str(recipe["expected_bin"]),
                    )
                )
                errors.extend(
                    validate_main_meta_test_command_surface(
                        meta_yaml=meta_yaml,
                        matrix_text=COMMAND_MATRIX.read_text(encoding="utf-8"),
                        expected_bin=str(recipe["expected_bin"]),
                    )
                )
    errors.extend(validate_recipe_set_consistency(recipe_metadata))
    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
