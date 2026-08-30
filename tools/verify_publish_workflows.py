#!/usr/bin/env python3
"""Verify provider-publishing workflows fail closed before authentication."""

from __future__ import annotations

from pathlib import Path
import sys


ROOT = Path(__file__).resolve().parents[1]
DOCKER_WORKFLOW = ROOT / ".github" / "workflows" / "publish-docker.yml"
PYPI_WORKFLOW = ROOT / ".github" / "workflows" / "publish-pypi.yml"


def validate_docker_publish_workflow(root: Path = ROOT) -> list[str]:
    path = root / ".github" / "workflows" / "publish-docker.yml"
    if not path.is_file():
        return ["Docker publish workflow is missing"]
    text = path.read_text(encoding="utf-8")
    errors: list[str] = []
    required = (
        ("workflow_dispatch:", "Docker publish workflow must support manual dispatch"),
        ("Validate release source metadata", "Docker publish workflow must validate source metadata"),
        ('expected_tag="v$(python3 - <<\'PY\'', "Docker publish workflow must derive the expected tag from Cargo metadata"),
        ('test "${GITHUB_REF_TYPE}" = "tag"', "Docker publishing must require a tag ref"),
        ('test "${GITHUB_REF_NAME}" = "${expected_tag}"', "Docker publishing must require the matching version tag"),
        ("docker/login-action@v3", "Docker publish workflow must authenticate with the Docker action"),
        ("docker/build-push-action@v6", "Docker publish workflow must build and push the image"),
        ("Smoke-test published image", "Docker publish workflow must smoke-test the pushed image"),
        ("docker pull", "Docker publish workflow must pull the exact published tag before smoke-testing"),
        ('"${image}" --version', "Docker publish workflow must execute the published image version command"),
        ('"${image}" doctor', "Docker publish workflow must execute the published image doctor command"),
        ('"${image}" trial MarkDuplicates', "Docker publish workflow must execute the published image trial contract"),
        ('-v "${GITHUB_WORKSPACE}:/workspace:ro"', "Docker publish workflow must mount the checked-in smoke fixture read-only"),
        ('I=/workspace/fixtures/markduplicates/basic/input.bam', "Docker publish workflow must run a real MarkDuplicates fixture"),
    )
    for needle, message in required:
        if needle not in text:
            errors.append(message)

    if 'if [[ "${GITHUB_REF_TYPE}" == "tag" ]]; then' in text:
        errors.append("Docker tag validation must not be conditional on an already-tagged ref")

    validation_marker = "Validate release source metadata"
    login_marker = "docker/login-action@v3"
    build_marker = "docker/build-push-action@v6"
    smoke_marker = "Smoke-test published image"
    if validation_marker in text and login_marker in text:
        if text.index(validation_marker) > text.index(login_marker):
            errors.append("Docker release validation must run before registry login")
    if build_marker in text and smoke_marker in text and text.index(smoke_marker) < text.index(build_marker):
        errors.append("Docker image smoke test must run after the image is pushed")
    return errors


def validate_pypi_publish_workflow(root: Path = ROOT) -> list[str]:
    path = root / ".github" / "workflows" / "publish-pypi.yml"
    if not path.is_file():
        return ["PyPI publish workflow is missing"]
    text = path.read_text(encoding="utf-8")
    publish_marker = "\n  publish:\n"
    if publish_marker not in text:
        return ["PyPI publish workflow is missing its publish job"]
    publish = text.split(publish_marker, 1)[1]
    errors: list[str] = []
    required = (
        (
            "- name: Download wheel distributions",
            "PyPI publish job must download wheel artifacts explicitly",
        ),
        (
            "pattern: wheels-*",
            "PyPI publish job must restrict wheel downloads to wheels-* artifacts",
        ),
        (
            "- name: Download source distribution",
            "PyPI publish job must download the source distribution explicitly",
        ),
        (
            "name: sdist",
            "PyPI publish job must select only the sdist artifact",
        ),
        (
            "pypa/gh-action-pypi-publish@release/v1",
            "PyPI publish job must use the guarded publishing action",
        ),
    )
    for needle, message in required:
        if needle not in publish:
            errors.append(message)
    if "name: turbo-picard-release-manifest" in publish:
        errors.append("PyPI publish job must not download the release manifest into dist")
    return errors


def main() -> int:
    errors = validate_docker_publish_workflow() + validate_pypi_publish_workflow()
    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1
    print("Publishing workflows are fail-closed on exact release artifacts")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
