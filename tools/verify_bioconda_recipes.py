#!/usr/bin/env python3
"""Validate Bioconda-oriented recipe readiness for turbo-picard."""

from __future__ import annotations

import pathlib
import re
import sys


ROOT = pathlib.Path(__file__).resolve().parents[1]
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


def has_cargo_install_flag(build_sh: str, flag_pattern: str) -> bool:
    compact = re.sub(r"\\\s*\n\s*", " ", build_sh)
    compact = re.sub(r"\s+", " ", compact)
    return bool(re.search(r"\bcargo install\b.*" + flag_pattern, compact))


def recipe_version(meta_yaml: str) -> str | None:
    match = re.search(r'{%\s*set\s+version\s*=\s*"([^"]+)"\s*%}', meta_yaml)
    if match:
        return match.group(1)
    match = re.search(r"(?m)^\s+version:\s+([^\s#]+)\s*$", meta_yaml)
    if match:
        return match.group(1).strip("\"'")
    return None


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
    if release_ready:
        version = recipe_version(meta_yaml)
        if re.search(r"(?m)^\s+path:\s+", meta_yaml):
            errors.append(f"{name} meta.yaml still uses local source.path")
        if not re.search(r"(?m)^\s+url:\s+https://", meta_yaml):
            errors.append(f"{name} meta.yaml missing release source url")
        elif version and f"/refs/tags/v{version}.tar.gz" not in meta_yaml:
            errors.append(
                f"{name} meta.yaml release source url must use refs/tags/v{version}.tar.gz"
            )
        if not re.search(r"(?m)^\s+sha256:\s+[0-9a-f]{64}\s*$", meta_yaml):
            errors.append(f"{name} meta.yaml missing release source sha256")
    if "number: 0" not in meta_yaml:
        errors.append(f"{name} meta.yaml should use build number 0 before first submission")
    if "skip: true  # [win]" not in meta_yaml:
        errors.append(f"{name} meta.yaml missing Windows skip selector")
    if "{{ compiler('rust') }}" not in meta_yaml:
        errors.append(f"{name} meta.yaml missing {{{{ compiler('rust') }}}}")
    if "cargo-bundle-licenses" not in meta_yaml:
        errors.append(f"{name} meta.yaml missing cargo-bundle-licenses")
    if "THIRDPARTY.yml" not in meta_yaml:
        errors.append(f"{name} meta.yaml missing THIRDPARTY.yml license_file")
    if "license: MIT" not in meta_yaml:
        errors.append(f"{name} meta.yaml missing MIT license metadata")
    if "home: https://github.com/dnncha/turbo-picard" not in meta_yaml:
        errors.append(f"{name} meta.yaml missing project home URL")
    if not re.search(r"(?m)^\s+summary:\s+\S", meta_yaml):
        errors.append(f"{name} meta.yaml missing non-empty summary")
    if "replace-with" in meta_yaml or "placeholder" in meta_yaml:
        errors.append(f"{name} meta.yaml contains maintainer placeholder")
    if "cargo-bundle-licenses" not in build_sh:
        errors.append(f"{name} build.sh missing cargo-bundle-licenses invocation")
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
        if has_cargo_install_flag(build_sh, r"--bin\s+turbo-picard\b"):
            errors.append(f"{name} build.sh must not install turbo-picard")
        if "{{ pin_subpackage('turbo-picard', exact=True) }}" not in meta_yaml:
            errors.append(
                f"{name} meta.yaml missing exact turbo-picard pin_subpackage run dependency"
            )
        if "picard ==0" not in meta_yaml:
            errors.append(f"{name} meta.yaml missing picard ==0 run_constrained conflict")
        if "picard MarkDuplicates --help" not in run_test_sh:
            errors.append(f"{name} run_test.sh missing picard MarkDuplicates smoke test")
    else:
        if has_cargo_install_flag(build_sh, r"--bin\s+picard\b"):
            errors.append(f"{name} build.sh must not install picard shim")
        for command in ("MarkDuplicates", "SortSam", "CleanSam", "ViewSam"):
            smoke = f"{expected_bin} {command} --help"
            if smoke not in run_test_sh:
                errors.append(f"{name} run_test.sh missing {smoke} smoke test")
    return errors


def validate_release_evidence(root: pathlib.Path = ROOT) -> list[str]:
    errors: list[str] = []
    manifest_path = root / "benchmarks" / "real-data" / "manifest.json"
    verifier_path = root / "tools" / "verify_real_data_evidence.py"
    readme_path = root / "README.md"
    site_path = root / "docs" / "site" / "index.html"
    packaging_readme_path = root / "packaging" / "bioconda" / "turbo-picard" / "README.md"
    if not manifest_path.is_file():
        errors.append("release evidence manifest missing benchmarks/real-data/manifest.json")
    else:
        manifest_text = manifest_path.read_text(encoding="utf-8")
        if '"datasets"' not in manifest_text:
            errors.append("release evidence manifest missing datasets list")
        try:
            manifest = __import__("json").loads(manifest_text)
        except ValueError:
            errors.append("release evidence manifest is not valid JSON")
        else:
            if not any(
                dataset.get("release_tier") == "release_candidate"
                for dataset in manifest.get("datasets", [])
                if isinstance(dataset, dict)
            ):
                errors.append(
                    "release evidence manifest has no release_candidate dataset"
                )
    if not verifier_path.is_file():
        errors.append("release evidence verifier missing tools/verify_real_data_evidence.py")
    for path in (readme_path, site_path, packaging_readme_path):
        if not path.is_file():
            errors.append(f"release evidence documentation missing {path.relative_to(root)}")
            continue
        text = path.read_text(encoding="utf-8")
        if "python3 tools/verify_real_data_evidence.py" not in text:
            errors.append(
                f"{path.relative_to(root)} missing real-data evidence verifier command"
            )
        if "python3 tools/verify_real_data_evidence.py --release-ready" not in text:
            errors.append(
                f"{path.relative_to(root)} missing release-ready real-data verifier command"
            )
        if "python3 tools/update_real_data_manifest.py" not in text:
            errors.append(
                f"{path.relative_to(root)} missing real-data manifest update command"
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
    for recipe in RECIPES:
        recipe_dir = recipe["path"]
        missing_files = []
        for filename in ("meta.yaml", "build.sh", "run_test.sh"):
            if not (recipe_dir / filename).is_file():
                missing_files.append(f"{recipe['name']} missing {filename}")
        errors.extend(missing_files)
        if missing_files:
            continue
        errors.extend(
            validate_recipe(
                name=str(recipe["name"]),
                meta_yaml=(recipe_dir / "meta.yaml").read_text(encoding="utf-8"),
                build_sh=(recipe_dir / "build.sh").read_text(encoding="utf-8"),
                run_test_sh=(recipe_dir / "run_test.sh").read_text(encoding="utf-8"),
                expected_bin=str(recipe["expected_bin"]),
                is_shim=bool(recipe["is_shim"]),
                release_ready=release_ready,
            )
        )
    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
