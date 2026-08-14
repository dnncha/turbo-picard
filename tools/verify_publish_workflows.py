#!/usr/bin/env python3
"""Verify provider-publishing workflows fail closed before authentication."""

from __future__ import annotations

from pathlib import Path
import sys


ROOT = Path(__file__).resolve().parents[1]
DOCKER_WORKFLOW = ROOT / ".github" / "workflows" / "publish-docker.yml"


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
    )
    for needle, message in required:
        if needle not in text:
            errors.append(message)

    if 'if [[ "${GITHUB_REF_TYPE}" == "tag" ]]; then' in text:
        errors.append("Docker tag validation must not be conditional on an already-tagged ref")

    validation_marker = "Validate release source metadata"
    login_marker = "docker/login-action@v3"
    if validation_marker in text and login_marker in text:
        if text.index(validation_marker) > text.index(login_marker):
            errors.append("Docker release validation must run before registry login")
    return errors


def main() -> int:
    errors = validate_docker_publish_workflow()
    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1
    print("Docker publish workflow is fail-closed on the exact release tag")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
