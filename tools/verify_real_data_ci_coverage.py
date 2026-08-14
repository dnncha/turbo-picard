#!/usr/bin/env python3
"""Verify CI covers release-critical helper scripts."""

from __future__ import annotations

import pathlib
import re
import sys


ROOT = pathlib.Path(__file__).resolve().parents[1]
CI = ROOT / ".github" / "workflows" / "ci.yml"
PACKAGE_INSTALL = ROOT / "tools" / "verify_package_install.sh"

REQUIRED_SNIPPETS = [
    "python3 -m unittest tools/test_compare_real_data.py",
    "python3 -m unittest tools/test_prepare_bioconda_release.py",
    "python3 -m unittest tools/test_update_real_data_manifest.py",
    "python3 -m unittest tools/test_verify_benchmark_suite_coverage.py",
    "python3 -m unittest tools/test_verify_benchmark_thresholds.py",
    "python3 -m unittest tools/test_verify_bioconda_recipes.py",
    "python3 -m unittest tools/test_verify_readme_benchmark_evidence.py",
    "python3 -m unittest tools/test_verify_release_versions.py",
    "python3 -m unittest tools/test_verify_real_data_evidence.py",
    "python3 -m unittest tools/test_verify_site_benchmark_evidence.py",
    'python3 -m pip install "snakemake==7.32.4" "PuLP==2.7.0"',
    "bash tools/verify_snakemake_starter.sh",
    "python3 -m unittest tools/test_verify_site_disclosures.py",
    "python3 -m unittest tools/test_verify_site_links.py",
    "tools/compare_real_data.py",
    "tools/prepare_bioconda_release.py \\",
    "tools/update_real_data_manifest.py",
    "tools/verify_benchmark_suite_coverage.py",
    "tools/verify_benchmark_thresholds.py",
    "tools/bench_intervallisttools.py",
    "tools/verify_bioconda_recipes.py",
    "tools/verify_readme_benchmark_evidence.py",
    "tools/verify_release_versions.py",
    "tools/verify_real_data_evidence.py",
    "tools/verify_site_benchmark_evidence.py",
    "tools/verify_site_disclosures.py",
    "tools/verify_site_links.py",
    "tools/verify_basic_validatesamfile_parity.sh",
    "tools/test_compare_real_data.py",
    "tools/test_prepare_bioconda_release.py",
    "tools/test_update_real_data_manifest.py",
    "tools/test_verify_benchmark_suite_coverage.py",
    "tools/test_verify_benchmark_thresholds.py",
    "tools/test_verify_bioconda_recipes.py",
    "tools/test_verify_readme_benchmark_evidence.py",
    "tools/test_verify_release_versions.py",
    "tools/test_verify_real_data_evidence.py",
    "tools/test_verify_site_benchmark_evidence.py",
    "python3 tools/verify_benchmark_suite_coverage.py",
    "python3 tools/verify_benchmark_thresholds.py",
    "python3 tools/verify_bioconda_recipes.py",
    "python3 tools/verify_readme_benchmark_evidence.py",
    "python3 tools/verify_real_data_evidence.py",
    "python3 tools/verify_real_data_evidence.py --release-ready",
    "python3 tools/verify_release_versions.py",
    "python3 tools/verify_site_benchmark_evidence.py",
    "python3 tools/verify_site_disclosures.py",
    "python3 tools/verify_site_links.py",
    "./tools/verify_basic_validatesamfile_parity.sh",
    "python3 tools/prepare_bioconda_release.py --sha256",
    "release_helper_check_status",
    'test "$release_helper_check_status" -eq 1',
    "./tools/verify_package_install.sh",
]

PACKAGE_INSTALL_SNIPPETS = [
    "CITATION.cff",
    "docs/command-matrix.yml",
    "benchmarks/real-data/manifest.json",
    "docs/site/assets/benchmark-data.json",
    "repository-code: \"https://github.com/dnncha/turbo-picard\"",
    "^cff-version: 1.2.0$",
    "^type: software$",
    "archived release",
    "^picard_reference: \"3.4.0\"$",
    "\"release_tier\": \"release_candidate\"",
    "\"gatk-na12878-mito\"",
    "\"parity\": \"32/32 PASS\"",
    "\"geometric_mean_speedup\"",
    "required_portfolio = {",
    "release_candidate manifest missing package-smoke command evidence",
    "benchmark-data summary missing numeric",
    "benchmark-data command_count and parity_pass_count differ",
    "packaging/bioconda/turbo-picard/run_test.sh",
    "packaging/bioconda/turbo-picard-picard-shim/run_test.sh",
    "PATH=\"${install_root}/bin:/usr/bin:/bin\"",
    "PATH=\"${shim_install_root}/bin:/usr/bin:/bin\"",
    "test ! -e \"${install_root}/bin/picard\"",
    "picard MarkDuplicates \\",
    "picard ViewSam \\",
    "grep -q 'UNPAIRED_READ_DUPLICATES' \"${shim_metrics}\"",
    "installed turbo-picard help missing commands",
    "installed picard shim help missing commands",
]


def validate_ci_coverage(ci_text: str) -> list[str]:
    return [
        f"CI missing release-critical helper coverage: {snippet}"
        for snippet in REQUIRED_SNIPPETS
        if snippet not in ci_text
    ]


def validate_package_install_coverage(package_install_text: str) -> list[str]:
    return [
        f"package install smoke missing release-critical behavior: {snippet}"
        for snippet in PACKAGE_INSTALL_SNIPPETS
        if snippet not in package_install_text
    ]


def validate_python_tool_compile_coverage(
    ci_text: str,
    tools_dir: pathlib.Path = ROOT / "tools",
) -> list[str]:
    errors: list[str] = []
    for path in sorted(tools_dir.glob("*.py")):
        if path.name.startswith("__"):
            continue
        try:
            relative = path.relative_to(ROOT)
        except ValueError:
            relative = pathlib.Path("tools") / path.name
        snippet = str(relative)
        if snippet not in ci_text:
            errors.append(f"CI missing Python compile coverage: {snippet}")
    return errors


def command_matrix_parity_scripts(matrix_text: str) -> list[str]:
    scripts: list[str] = []
    for line in matrix_text.splitlines():
        match = re.match(r"\s+parity_script:\s+(\S+)\s*$", line)
        if match:
            scripts.append(match.group(1))
    return scripts


def validate_parity_script_ci_coverage(ci_text: str, matrix_text: str) -> list[str]:
    errors: list[str] = []
    for script in command_matrix_parity_scripts(matrix_text):
        if f"./{script}" not in ci_text:
            errors.append(f"CI missing command-matrix parity script: ./{script}")
    return errors


def main() -> int:
    errors = validate_ci_coverage(CI.read_text(encoding="utf-8"))
    errors.extend(validate_python_tool_compile_coverage(CI.read_text(encoding="utf-8")))
    errors.extend(
        validate_parity_script_ci_coverage(
            CI.read_text(encoding="utf-8"),
            (ROOT / "docs" / "command-matrix.yml").read_text(encoding="utf-8"),
        )
    )
    errors.extend(
        validate_package_install_coverage(PACKAGE_INSTALL.read_text(encoding="utf-8"))
    )
    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
