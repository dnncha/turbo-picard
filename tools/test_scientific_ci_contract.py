from __future__ import annotations

import unittest
from pathlib import Path
from typing import TYPE_CHECKING

if TYPE_CHECKING or __package__:
    from .verify_ci_coverage import validate_scientific_parity_ci
else:
    from verify_ci_coverage import validate_scientific_parity_ci


class ScientificCiContractTests(unittest.TestCase):
    def test_checked_in_workflow_enforces_scientific_gates(self) -> None:
        workflow = (Path(__file__).resolve().parents[1] / ".github/workflows/ci.yml").read_text()
        assert validate_scientific_parity_ci(workflow) == []

    def test_disabling_any_scientific_guard_is_detected(self) -> None:
        workflow = (Path(__file__).resolve().parents[1] / ".github/workflows/ci.yml").read_text()
        mutations = [
            ('picard: ["3.4.0", "3.5.0"]', 'picard: ["3.4.0"]'),
            ("python3 tools/verify_adversarial_parity.py", "# python3 tools/verify_adversarial_parity.py"),
            ("export TURBO_PICARD_REQUIRE_NATIVE=1", "export TURBO_PICARD_REQUIRE_NATIVE=0"),
            ('--expected-picard-version "${{ matrix.picard }}"', '--expected-picard-version "3.4.0"'),
            ('export PATH="${MAMBA_ROOT_PREFIX}/envs/turbo-picard-ci/bin:$PATH"', 'export PATH="$PATH"'),
            (
                "- name: Retain adversarial parity evidence\n        if: always()",
                "- name: Retain adversarial parity evidence\n        if: success()",
            ),
            (
                "- name: Verify adversarial scientific boundaries\n",
                "- name: Verify adversarial scientific boundaries\n        continue-on-error: true\n",
            ),
        ]
        for old, new in mutations:
            with self.subTest(mutation=old):
                assert old in workflow
                assert validate_scientific_parity_ci(workflow.replace(old, new))


if __name__ == "__main__":
    unittest.main()
