from __future__ import annotations

import importlib.util
from pathlib import Path
import sys
import tempfile
import unittest


MODULE_PATH = Path(__file__).with_name("verify_publish_workflows.py")
SPEC = importlib.util.spec_from_file_location("verify_publish_workflows", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
verify_publish_workflows = importlib.util.module_from_spec(SPEC)
sys.modules["verify_publish_workflows"] = verify_publish_workflows
SPEC.loader.exec_module(verify_publish_workflows)


VALID_WORKFLOW = """
workflow_dispatch:
jobs:
  publish:
    steps:
      - name: Validate release source metadata
        run: |
          expected_tag="v$(python3 - <<'PY'
          print("0.1.11")
          PY
          )"
          test "${GITHUB_REF_TYPE}" = "tag"
          test "${GITHUB_REF_NAME}" = "${expected_tag}"
      - uses: docker/login-action@v3
      - uses: docker/build-push-action@v6
      - name: Smoke-test published image
        run: |
          image="ghcr.io/${GITHUB_REPOSITORY}:${GITHUB_REF_NAME#v}"
          docker pull "${image}"
          docker run --rm "${image}" --version
          docker run --rm "${image}" doctor
          docker run --rm "${image}" trial MarkDuplicates
          -v "${GITHUB_WORKSPACE}:/workspace:ro"
          I=/workspace/fixtures/markduplicates/basic/input.bam
"""


VALID_PYPI_WORKFLOW = """
jobs:
  publish:
    steps:
      - name: Download wheel distributions
        uses: actions/download-artifact@v4
        with:
          pattern: wheels-*
          path: dist
          merge-multiple: true
      - name: Download source distribution
        uses: actions/download-artifact@v4
        with:
          name: sdist
          path: dist
      - name: Publish distributions
        uses: pypa/gh-action-pypi-publish@release/v1
"""


class VerifyPublishWorkflowsTests(unittest.TestCase):
    def write_workflow(self, text: str) -> Path:
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        root = Path(temp.name)
        path = root / ".github" / "workflows" / "publish-docker.yml"
        path.parent.mkdir(parents=True)
        path.write_text(text, encoding="utf-8")
        return root

    def write_pypi_workflow(self, text: str) -> Path:
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        root = Path(temp.name)
        path = root / ".github" / "workflows" / "publish-pypi.yml"
        path.parent.mkdir(parents=True)
        path.write_text(text, encoding="utf-8")
        return root

    def test_accepts_exact_tag_guard_before_login(self) -> None:
        root = self.write_workflow(VALID_WORKFLOW)
        self.assertEqual([], verify_publish_workflows.validate_docker_publish_workflow(root))

    def test_rejects_missing_ref_type_guard(self) -> None:
        root = self.write_workflow(
            VALID_WORKFLOW.replace('test "${GITHUB_REF_TYPE}" = "tag"\n', "")
        )
        errors = verify_publish_workflows.validate_docker_publish_workflow(root)
        self.assertIn("Docker publishing must require a tag ref", errors)

    def test_rejects_conditional_tag_guard(self) -> None:
        root = self.write_workflow(
            VALID_WORKFLOW.replace(
                'test "${GITHUB_REF_TYPE}" = "tag"\n',
                'if [[ "${GITHUB_REF_TYPE}" == "tag" ]]; then\n',
            )
        )
        errors = verify_publish_workflows.validate_docker_publish_workflow(root)
        self.assertIn(
            "Docker tag validation must not be conditional on an already-tagged ref",
            errors,
        )

    def test_accepts_explicit_pypi_distribution_artifacts(self) -> None:
        root = self.write_pypi_workflow(VALID_PYPI_WORKFLOW)
        self.assertEqual([], verify_publish_workflows.validate_pypi_publish_workflow(root))

    def test_rejects_unrestricted_pypi_artifact_download(self) -> None:
        root = self.write_pypi_workflow(
            VALID_PYPI_WORKFLOW.replace("          pattern: wheels-*\n", "")
        )
        errors = verify_publish_workflows.validate_pypi_publish_workflow(root)
        self.assertIn(
            "PyPI publish job must restrict wheel downloads to wheels-* artifacts",
            errors,
        )

    def test_rejects_release_manifest_in_pypi_dist(self) -> None:
        root = self.write_pypi_workflow(
            VALID_PYPI_WORKFLOW.replace("          name: sdist\n", "          name: turbo-picard-release-manifest\n")
        )
        errors = verify_publish_workflows.validate_pypi_publish_workflow(root)
        self.assertIn(
            "PyPI publish job must not download the release manifest into dist",
            errors,
        )


if __name__ == "__main__":
    unittest.main()
