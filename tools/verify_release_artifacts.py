#!/usr/bin/env python3
"""Verify built PyPI artifacts match the checked-out release source."""

from __future__ import annotations

import argparse
from email.parser import BytesParser
from email.policy import compat32
from pathlib import Path, PurePosixPath
import re
import sys
import tarfile
import zipfile


ROOT = Path(__file__).resolve().parents[1]
PACKAGE_NAME = "turbo-picard"
WHEEL_NAME = "turbo_picard"
ELF_MACHINES = {62: "x86_64", 183: "aarch64"}
MACHO_CPU_TYPES = {0x01000007: "x86_64", 0x0100000C: "arm64"}


def workspace_version(root: Path = ROOT) -> str:
    text = (root / "Cargo.toml").read_text(encoding="utf-8")
    match = re.search(
        r'(?ms)^\[workspace\.package\]\s+.*?^version\s*=\s*"([^"]+)"',
        text,
    )
    if not match:
        raise ValueError("Cargo.toml missing [workspace.package] version")
    return match.group(1)


def normalize_text(text: str) -> str:
    return text.replace("\r\n", "\n").replace("\r", "\n").rstrip()


def metadata_from_bytes(
    raw: bytes, artifact: Path
) -> tuple[str | None, str | None, str | None, list[str]]:
    message = BytesParser(policy=compat32).parsebytes(raw)
    errors: list[str] = []
    name = message.get("Name")
    version = message.get("Version")
    payload = message.get_payload(decode=True)
    if not isinstance(payload, bytes):
        errors.append(f"{artifact.name} metadata has no UTF-8 long description")
        description = None
    else:
        try:
            description = payload.decode("utf-8")
        except UnicodeDecodeError as error:
            errors.append(f"{artifact.name} metadata long description is not UTF-8: {error}")
            description = None
    return name, version, description, errors


def validate_metadata(
    artifact: Path,
    raw: bytes,
    expected_version: str,
    readme: str,
) -> list[str]:
    name, version, description, errors = metadata_from_bytes(raw, artifact)
    if name != PACKAGE_NAME:
        errors.append(f"{artifact.name} metadata Name must be {PACKAGE_NAME}")
    if version != expected_version:
        errors.append(
            f"{artifact.name} metadata Version {version or '<missing>'} "
            f"must be {expected_version}"
        )
    message = BytesParser(policy=compat32).parsebytes(raw)
    if not (message.get("Description-Content-Type") or "").startswith("text/markdown"):
        errors.append(f"{artifact.name} metadata must declare a Markdown long description")
    if description is not None and normalize_text(description) != normalize_text(readme):
        errors.append(f"{artifact.name} long description must match the checked-out README.md")
    return errors


def wheel_platform_architecture(artifact: Path) -> str | None:
    filename = artifact.name[:-4] if artifact.name.endswith(".whl") else artifact.name
    platform = filename.rsplit("-", 1)[-1]
    for marker in ("x86_64", "aarch64", "arm64"):
        if marker in platform:
            return marker
    return None


def binary_architecture(raw: bytes) -> str | None:
    if raw.startswith(b"\x7fELF") and len(raw) >= 20:
        return ELF_MACHINES.get(int.from_bytes(raw[18:20], "little"))
    if raw.startswith(b"\xcf\xfa\xed\xfe") and len(raw) >= 8:
        return MACHO_CPU_TYPES.get(int.from_bytes(raw[4:8], "little"))
    return None


def validate_wheel(
    artifact: Path,
    expected_version: str,
    readme: str,
) -> list[str]:
    errors: list[str] = []
    expected_prefix = f"{WHEEL_NAME}-{expected_version.replace('-', '_')}-"
    if not artifact.name.startswith(expected_prefix) or not artifact.name.endswith(".whl"):
        errors.append(f"{artifact.name} filename must contain release version {expected_version}")
    try:
        with zipfile.ZipFile(artifact) as archive:
            names = archive.namelist()
            metadata_paths = [name for name in names if name.endswith(".dist-info/METADATA")]
            if len(metadata_paths) != 1:
                errors.append(
                    f"{artifact.name} must contain exactly one dist-info/METADATA file"
                )
            else:
                errors.extend(
                    validate_metadata(
                        artifact,
                        archive.read(metadata_paths[0]),
                        expected_version,
                        readme,
                    )
                )
                dist_info = metadata_paths[0].split("/", 1)[0]
                for required in (f"{dist_info}/WHEEL", f"{dist_info}/RECORD"):
                    if required not in names:
                        errors.append(f"{artifact.name} missing {required}")
            for script in ("turbo-picard", "picard"):
                if not any(PurePosixPath(name).name == script for name in names):
                    errors.append(f"{artifact.name} missing {script} entrypoint")
            script_paths = [
                name for name in names if PurePosixPath(name).name == "turbo-picard"
            ]
            expected_architecture = wheel_platform_architecture(artifact)
            if expected_architecture is not None and len(script_paths) == 1:
                actual_architecture = binary_architecture(archive.read(script_paths[0]))
                if actual_architecture != expected_architecture:
                    errors.append(
                        f"{artifact.name} entrypoint architecture "
                        f"{actual_architecture or '<unknown>'} must match wheel "
                        f"platform {expected_architecture}"
                    )
    except (OSError, zipfile.BadZipFile) as error:
        errors.append(f"{artifact.name} is not a readable wheel: {error}")
    return errors


def validate_sdist(
    artifact: Path,
    expected_version: str,
    readme: str,
) -> list[str]:
    errors: list[str] = []
    expected_root = f"{WHEEL_NAME}-{expected_version}/"
    if artifact.name != f"{WHEEL_NAME}-{expected_version}.tar.gz":
        errors.append(f"{artifact.name} filename must be turbo_picard-{expected_version}.tar.gz")
    try:
        with tarfile.open(artifact, "r:gz") as archive:
            members = archive.getmembers()
            for member in members:
                path = PurePosixPath(member.name)
                if path.is_absolute() or ".." in path.parts:
                    errors.append(f"{artifact.name} contains unsafe path {member.name}")
                if not member.name.startswith(expected_root):
                    errors.append(
                        f"{artifact.name} contains member outside {expected_root}: {member.name}"
                    )
            files = {member.name for member in members if member.isfile()}
            required = {
                f"{expected_root}README.md",
                f"{expected_root}PKG-INFO",
                f"{expected_root}Cargo.toml",
                f"{expected_root}pyproject.toml",
            }
            for path in sorted(required - files):
                errors.append(f"{artifact.name} missing {path}")
            readme_path = f"{expected_root}README.md"
            if readme_path in files:
                packaged_readme = archive.extractfile(readme_path)
                packaged_text = (
                    None
                    if packaged_readme is None
                    else normalize_text(packaged_readme.read().decode("utf-8"))
                )
                if packaged_text != normalize_text(readme):
                    errors.append(
                        f"{artifact.name} README.md must match the checked-out README.md"
                    )
            pkg_info_path = f"{expected_root}PKG-INFO"
            if pkg_info_path in files:
                pkg_info = archive.extractfile(pkg_info_path)
                if pkg_info is not None:
                    errors.extend(
                        validate_metadata(artifact, pkg_info.read(), expected_version, readme)
                    )
    except (OSError, tarfile.TarError, UnicodeDecodeError) as error:
        errors.append(f"{artifact.name} is not a readable source distribution: {error}")
    return errors


def validate_repository(root: Path, version: str) -> list[str]:
    errors: list[str] = []
    readme = (root / "README.md").read_text(encoding="utf-8")
    normalized_readme = re.sub(r"\s+", " ", normalize_text(readme))
    required_markers = (
        f"The current source release is `{version}`",
    )
    for marker in required_markers:
        if marker not in normalized_readme:
            errors.append(f"README.md must contain current release marker: {marker}")
    return errors


def collect_errors(dist: Path, root: Path = ROOT) -> list[str]:
    version = workspace_version(root)
    errors = validate_repository(root, version)
    readme = (root / "README.md").read_text(encoding="utf-8")
    artifacts = sorted(path for path in dist.iterdir() if path.is_file()) if dist.is_dir() else []
    if not artifacts:
        return errors + [f"{dist} contains no release artifacts"]
    for artifact in artifacts:
        if artifact.name.endswith(".whl"):
            errors.extend(validate_wheel(artifact, version, readme))
        elif artifact.name.endswith(".tar.gz"):
            errors.extend(validate_sdist(artifact, version, readme))
        else:
            errors.append(f"unexpected release artifact: {artifact.name}")
    return errors


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dist", type=Path, default=ROOT / "dist")
    parser.add_argument(
        "--source-only",
        action="store_true",
        help="validate release markers without requiring built distributions",
    )
    args = parser.parse_args(argv)
    version = workspace_version()
    errors = validate_repository(ROOT, version) if args.source_only else collect_errors(args.dist)
    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1
    print(
        "release source markers are consistent"
        if args.source_only
        else "release artifacts match the checked-out version and README"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
