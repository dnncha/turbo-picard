#!/usr/bin/env python3
"""Verify the PyPI packaging metadata and release notes stay useful."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import re
import sys
import time
import urllib.request

try:
    import tomllib
except ModuleNotFoundError:  # pragma: no cover - exercised on older local Python.
    tomllib = None


ROOT = Path(__file__).resolve().parents[1]
PACKAGE_NAME = "turbo-picard"
PYPROJECT = ROOT / "pyproject.toml"
CARGO = ROOT / "Cargo.toml"
CLI_CARGO = ROOT / "crates" / "turbo-picard-cli" / "Cargo.toml"
README = ROOT / "README.md"
PACKAGING_DOCS = ROOT / "docs" / "packaging.rst"
PUBLISH_WORKFLOW = ROOT / ".github" / "workflows" / "publish-pypi.yml"
PYPI_JSON_URL = "https://pypi.org/pypi/turbo-picard/json"


def normalize_text(text: str) -> str:
    return text.replace("\r\n", "\n").replace("\r", "\n").rstrip()


def workspace_version(root: Path = ROOT) -> str:
    text = (root / "Cargo.toml").read_text(encoding="utf-8")
    match = re.search(
        r"(?ms)^\[workspace\.package\]\s+.*?^version\s*=\s*\"([^\"]+)\"",
        text,
    )
    if not match:
        raise ValueError("Cargo.toml missing [workspace.package] version")
    return match.group(1)


def inline_table_value(text: str, key: str, field: str) -> str | None:
    match = re.search(rf'(?m)^{re.escape(key)}\s*=\s*\{{([^}}]+)\}}', text)
    if not match:
        return None
    field_match = re.search(rf'{re.escape(field)}\s*=\s*"([^"]+)"', match.group(1))
    if not field_match:
        return None
    return field_match.group(1)


def string_value(text: str, section: str, key: str) -> str | None:
    section_match = re.search(
        rf"(?ms)^\[{re.escape(section)}\]\s+(.*?)(?=^\[|\Z)",
        text,
    )
    if not section_match:
        return None
    match = re.search(rf'(?m)^{re.escape(key)}\s*=\s*"([^"]+)"', section_match.group(1))
    if not match:
        return None
    return match.group(1)


def string_array_value(text: str, section: str, key: str) -> list[str]:
    section_match = re.search(
        rf"(?ms)^\[{re.escape(section)}\]\s+(.*?)(?=^\[|\Z)",
        text,
    )
    if not section_match:
        return []
    match = re.search(rf"(?ms)^{re.escape(key)}\s*=\s*\[(.*?)\]", section_match.group(1))
    if not match:
        return []
    return re.findall(r'"([^"]+)"', match.group(1))


def bool_value(text: str, section: str, key: str) -> bool | None:
    section_match = re.search(
        rf"(?ms)^\[{re.escape(section)}\]\s+(.*?)(?=^\[|\Z)",
        text,
    )
    if not section_match:
        return None
    match = re.search(rf"(?m)^{re.escape(key)}\s*=\s*(true|false)\s*$", section_match.group(1))
    if not match:
        return None
    return match.group(1) == "true"


def load_pyproject(root: Path = ROOT) -> dict:
    path = root / "pyproject.toml"
    if not path.is_file():
        raise FileNotFoundError("pyproject.toml is required for PyPI packaging")
    text = path.read_text(encoding="utf-8")
    if tomllib is not None:
        return tomllib.loads(text)
    return {
        "build-system": {
            "requires": string_array_value(text, "build-system", "requires"),
            "build-backend": string_value(text, "build-system", "build-backend"),
        },
        "project": {
            "name": string_value(text, "project", "name"),
            "version": string_value(text, "project", "version"),
            "readme": string_value(text, "project", "readme"),
            "requires-python": string_value(text, "project", "requires-python"),
            "license": {"text": inline_table_value(text, "license", "text")},
            "authors": [{"name": name} for name in re.findall(r'name\s*=\s*"([^"]+)"', text)],
            "keywords": string_array_value(text, "project", "keywords"),
            "urls": {
                "Documentation": string_value(text, "project.urls", "Documentation"),
                "Source": string_value(text, "project.urls", "Source"),
                "Issues": string_value(text, "project.urls", "Issues"),
            },
        },
        "tool": {
            "maturin": {
                "manifest-path": string_value(text, "tool.maturin", "manifest-path"),
                "bindings": string_value(text, "tool.maturin", "bindings"),
                "strip": bool_value(text, "tool.maturin", "strip"),
                "include": [
                    {"path": "LICENSE", "format": "sdist"}
                    if 'include = [{ path = "LICENSE", format = "sdist" }]' in text
                    else {}
                ],
            }
        },
    }


def validate_pyproject(root: Path = ROOT) -> list[str]:
    errors: list[str] = []
    try:
        data = load_pyproject(root)
    except FileNotFoundError as error:
        return [str(error)]
    except Exception as error:
        if tomllib is None or not isinstance(error, tomllib.TOMLDecodeError):
            raise
        return [str(error)]

    build_system = data.get("build-system", {})
    requires = build_system.get("requires", [])
    if build_system.get("build-backend") != "maturin":
        errors.append("pyproject.toml build-backend must be maturin")
    if not any(str(requirement).startswith("maturin>=") for requirement in requires):
        errors.append("pyproject.toml build-system.requires must pin maturin")

    project = data.get("project", {})
    version = workspace_version(root)
    expected_project = {
        "name": "turbo-picard",
        "version": version,
        "readme": "README.md",
        "requires-python": ">=3.8",
    }
    for key, expected in expected_project.items():
        if project.get(key) != expected:
            errors.append(f"pyproject.toml project.{key} must be {expected}")
    if project.get("license", {}).get("text") != "MIT":
        errors.append("pyproject.toml project.license must be MIT")
    if "Donncha O'Toole" not in {
        author.get("name") for author in project.get("authors", []) if isinstance(author, dict)
    }:
        errors.append("pyproject.toml authors must include Donncha O'Toole")

    keywords = set(project.get("keywords", []))
    required_keywords = {"bioinformatics", "genomics", "Picard", "SAM", "BAM", "CRAM", "VCF", "Rust"}
    if not required_keywords.issubset(keywords):
        missing = ", ".join(sorted(required_keywords - keywords))
        errors.append(f"pyproject.toml missing PyPI keywords: {missing}")

    urls = project.get("urls", {})
    required_urls = {
        "Documentation": "https://turbo-picard.readthedocs.io/",
        "Source": "https://github.com/dnncha/turbo-picard",
        "Issues": "https://github.com/dnncha/turbo-picard/issues",
    }
    for key, expected in required_urls.items():
        if urls.get(key) != expected:
            errors.append(f"pyproject.toml project.urls.{key} must be {expected}")

    maturin = data.get("tool", {}).get("maturin", {})
    if maturin.get("manifest-path") != "crates/turbo-picard-cli/Cargo.toml":
        errors.append("pyproject.toml tool.maturin.manifest-path must point at the CLI crate")
    if maturin.get("bindings") != "bin":
        errors.append("pyproject.toml tool.maturin.bindings must be bin")
    if maturin.get("strip") is not True:
        errors.append("pyproject.toml tool.maturin.strip must be true")
    include = maturin.get("include", [])
    if not any(
        isinstance(item, dict)
        and item.get("path") == "LICENSE"
        and item.get("format") == "sdist"
        for item in include
    ):
        errors.append("pyproject.toml tool.maturin.include must put LICENSE in the sdist")
    return errors


def validate_cli_bins(root: Path = ROOT) -> list[str]:
    text = (root / "crates" / "turbo-picard-cli" / "Cargo.toml").read_text(
        encoding="utf-8"
    )
    bins = set(re.findall(r'(?m)^name\s*=\s*"([^"]+)"', text))
    errors: list[str] = []
    for binary in ["turbo-picard", "picard"]:
        if binary not in bins:
            errors.append(f"CLI Cargo.toml must keep the {binary} binary for PyPI wheels")
    return errors


def validate_docs(root: Path = ROOT) -> list[str]:
    readme = (root / "README.md").read_text(encoding="utf-8")
    quickstart = (root / "docs" / "quickstart.rst").read_text(encoding="utf-8")
    packaging = (root / "docs" / "packaging.rst").read_text(encoding="utf-8")
    checks = [
        (readme, "python3 -m pip install turbo-picard", "README missing pip install command"),
        (readme, "Installing from PyPI currently gives you both", "README missing PyPI shim warning"),
        (quickstart, "python3 -m pip install turbo-picard", "quickstart missing pip install command"),
        (quickstart, "Start with ``turbo-picard``", "quickstart missing explicit entrypoint guidance"),
        (packaging, "PyPI", "packaging docs missing PyPI section"),
        (packaging, "https://pypi.org/project/turbo-picard/", "packaging docs missing PyPI project link"),
        (packaging, "python3 -m maturin build --release --compatibility pypi --out dist", "packaging docs missing maturin build command"),
        (packaging, "python3 -m twine check dist/*", "packaging docs missing twine check command"),
        (packaging, "Trusted Publishing", "packaging docs missing PyPI publishing note"),
        (packaging, ".github/workflows/publish-pypi.yml", "packaging docs missing PyPI workflow filename"),
        (packaging, "build_release_manifest.py", "packaging docs missing release handoff manifest command"),
        (packaging, "picard", "packaging docs must mention the shim command"),
    ]
    return [message for text, needle, message in checks if needle not in text]


def validate_publish_workflow(root: Path = ROOT) -> list[str]:
    path = root / ".github" / "workflows" / "publish-pypi.yml"
    if not path.is_file():
        return ["PyPI publish workflow is missing"]
    text = path.read_text(encoding="utf-8")
    checks = [
        ("release:", "PyPI workflow must run from GitHub releases"),
        ("workflow_dispatch:", "PyPI workflow must support manual dispatch"),
        ("Validate publishing ref", "PyPI workflow must validate the publishing ref"),
        ("expected_ref=\"v$(python3 - <<'PY'", "PyPI workflow must derive the expected tag from the workspace version"),
        ('test "${GITHUB_REF_TYPE}" = "tag"', "PyPI workflow must publish only from a tag"),
        ('test "${GITHUB_REF_NAME}" = "${expected_ref}"', "PyPI workflow must publish only from the matching version tag"),
        ("Build Linux wheels", "PyPI workflow must build Linux wheels"),
        ("wheels-linux-x86_64", "PyPI workflow must upload Linux wheel artifacts"),
        ("linux-aarch64:", "PyPI workflow must build Linux ARM64 wheels"),
        ("Build Linux ARM64 wheels", "PyPI workflow must name the Linux ARM64 build"),
        ("wheels-linux-aarch64", "PyPI workflow must upload Linux ARM64 wheel artifacts"),
        ("apt-get install -y --no-install-recommends perl libclang-dev", "PyPI workflow must install ARM64 cross-build dependencies"),
        ("find /usr/lib -name libclang.so", "PyPI workflow must locate libclang in the ARM64 cross image"),
        ("BINDGEN_EXTRA_CLANG_ARGS_aarch64_unknown_linux_gnu", "PyPI workflow must provide bindgen target arguments for Linux ARM64"),
        ("--target=aarch64-unknown-linux-gnu --sysroot=$target_sysroot", "PyPI workflow must bindgen against the Linux ARM64 sysroot"),
        ("macos-x86_64:", "PyPI workflow must build macOS Intel wheels"),
        ("Build macOS Intel wheels", "PyPI workflow must name the macOS Intel build"),
        ("macos-15-intel", "PyPI workflow must use an Intel macOS runner"),
        ("wheels-macos-x86_64", "PyPI workflow must upload macOS Intel wheel artifacts"),
        ("needs: [linux, linux-aarch64, macos, macos-x86_64, sdist]", "PyPI publishing must wait for all wheel architectures"),
        ("manylinux: 2014", "PyPI workflow must pin manylinux2014-compatible Linux wheels"),
        ("perl-core", "PyPI workflow must install Perl core in manylinux for vendored OpenSSL"),
        ("llvm-toolset-7.0-clang-devel", "PyPI workflow must install libclang for bindgen in manylinux"),
        ("LIBCLANG_PATH", "PyPI workflow must expose libclang to bindgen in manylinux"),
        ("PyO3/maturin-action@v1", "PyPI workflow must build with maturin-action"),
        ("--compatibility pypi", "PyPI workflow must run maturin's PyPI compatibility check"),
        ("pypa/gh-action-pypi-publish@release/v1", "PyPI workflow must publish with the PyPA action"),
        ("skip-existing: true", "PyPI workflow must skip already-uploaded files"),
        ("id-token: write", "PyPI workflow must allow Trusted Publishing OIDC"),
        ("environment: pypi", "PyPI workflow must use the pypi environment"),
        ("validate:", "PyPI workflow must validate distributions before publishing"),
        ("Validate distributions", "PyPI workflow must name the distribution validation job"),
        ("python3 tools/verify_release_artifacts.py --dist dist", "PyPI workflow must validate built artifact contents"),
        ("Build release handoff manifest", "PyPI workflow must build a release handoff manifest"),
        ("python3 tools/build_release_manifest.py", "PyPI workflow must run the release handoff manifest builder"),
        ("Upload release handoff manifest", "PyPI workflow must upload the release handoff manifest"),
        ("turbo-picard-release-manifest", "PyPI workflow must name the release handoff manifest artifact"),
        ("Smoke-test Linux x86_64 wheel", "PyPI workflow must execute an installed Linux wheel"),
        ("python3 -m venv", "PyPI workflow must create an isolated install smoke environment"),
        ("dist/turbo_picard-*-manylinux_2_17_x86_64*.whl", "PyPI workflow must install the Linux x86_64 wheel"),
        ("-m pip install --no-deps", "PyPI workflow must smoke-test the built wheel without network dependencies"),
        ("turbo-picard\" --version", "PyPI workflow must execute the installed turbo-picard entrypoint"),
        ("turbo-picard\" doctor", "PyPI workflow must smoke-test the installed doctor command"),
        ("turbo-picard\" trial MarkDuplicates I=input.bam O=marked.bam M=metrics.txt", "PyPI workflow must smoke-test the trial contract"),
        ("picard\" --version", "PyPI workflow must execute the installed compatibility shim"),
        ("bash tools/verify_install_smoke.sh", "PyPI workflow must execute a real data-path install smoke"),
        ("needs: [linux, linux-aarch64, macos, macos-x86_64, sdist, validate]", "PyPI publishing must wait for artifact validation"),
        ("Verify live PyPI metadata", "PyPI workflow must verify the live record after upload"),
        ("verify_pypi_package.py --live", "PyPI workflow must compare live metadata with README"),
        ("--retries 12 --retry-delay 5", "PyPI workflow must allow for PyPI metadata propagation"),
    ]
    return [message for needle, message in checks if needle not in text]


def validate_live_metadata(
    payload: object,
    expected_version: str,
    readme: str,
) -> list[str]:
    """Validate the public PyPI JSON record against the release source."""

    if not isinstance(payload, dict):
        return ["live PyPI metadata response must be a JSON object"]
    info = payload.get("info")
    if not isinstance(info, dict):
        return ["live PyPI metadata response is missing its info object"]

    errors: list[str] = []
    if info.get("name") != PACKAGE_NAME:
        errors.append(f"live PyPI package name must be {PACKAGE_NAME}")
    version = info.get("version")
    if version != expected_version:
        errors.append(
            f"live PyPI version {version or '<missing>'} must be {expected_version}"
        )
    content_type = str(info.get("description_content_type") or "")
    if not content_type.startswith("text/markdown"):
        errors.append("live PyPI long description must declare Markdown content")
    description = info.get("description")
    if not isinstance(description, str):
        errors.append("live PyPI metadata has no long description")
    elif normalize_text(description) != normalize_text(readme):
        errors.append("live PyPI long description must match the checked-out README.md")
    return errors


def fetch_live_metadata(timeout: float = 15.0) -> object:
    request = urllib.request.Request(
        PYPI_JSON_URL,
        headers={
            "Accept": "application/json",
            "User-Agent": "turbo-picard-release-verifier",
        },
    )
    with urllib.request.urlopen(request, timeout=timeout) as response:
        return json.load(response)


def validate_live_pypi(
    root: Path = ROOT,
    *,
    retries: int = 1,
    retry_delay: float = 0.0,
    timeout: float = 15.0,
) -> list[str]:
    """Fetch and validate the public PyPI record, retrying eventual consistency."""

    if retries < 1:
        return ["--retries must be at least 1"]
    if retry_delay < 0:
        return ["--retry-delay must not be negative"]
    if timeout <= 0:
        return ["--timeout must be greater than 0"]

    expected_version = workspace_version(root)
    readme = (root / "README.md").read_text(encoding="utf-8")
    last_errors: list[str] = []
    for attempt in range(retries):
        try:
            errors = validate_live_metadata(
                fetch_live_metadata(timeout=timeout), expected_version, readme
            )
        except (OSError, ValueError, json.JSONDecodeError) as error:
            errors = [f"live PyPI metadata request failed: {error}"]
        if not errors:
            return []
        last_errors = errors
        if attempt + 1 < retries:
            time.sleep(retry_delay)
    return last_errors


def collect_errors(root: Path = ROOT) -> list[str]:
    errors: list[str] = []
    errors.extend(validate_pyproject(root))
    errors.extend(validate_cli_bins(root))
    errors.extend(validate_docs(root))
    errors.extend(validate_publish_workflow(root))
    return errors


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--live",
        action="store_true",
        help="also fetch and validate the public PyPI JSON record",
    )
    parser.add_argument(
        "--retries",
        type=int,
        default=1,
        help="number of live PyPI attempts (default: 1)",
    )
    parser.add_argument(
        "--retry-delay",
        type=float,
        default=0.0,
        help="seconds between live PyPI attempts (default: 0)",
    )
    parser.add_argument(
        "--timeout",
        type=float,
        default=15.0,
        help="live PyPI request timeout in seconds (default: 15)",
    )
    args = parser.parse_args(argv)

    errors = collect_errors()
    if args.live:
        errors.extend(
            validate_live_pypi(
                retries=args.retries,
                retry_delay=args.retry_delay,
                timeout=args.timeout,
            )
        )
    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1
    print(
        "PyPI packaging metadata is ready"
        + ("; live PyPI metadata matches the checked-out README" if args.live else "")
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
