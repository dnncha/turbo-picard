from __future__ import annotations

import copy
import importlib.util
import json
import sys
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("verify_markduplicates_guardrails.py")
SPEC = importlib.util.spec_from_file_location("verify_markduplicates_guardrails", MODULE_PATH)
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


class VerifyMarkDuplicatesGuardrailsTests(unittest.TestCase):
    def setUp(self) -> None:
        self.readme = MODULE.README.read_text(encoding="utf-8")
        self.payload = json.loads(MODULE.GUARDRAILS[0].read_text(encoding="utf-8"))

    def test_checked_in_guardrails_pass(self) -> None:
        self.assertEqual([], MODULE.collect_errors())

    def test_rejects_failed_parity(self) -> None:
        payload = copy.deepcopy(self.payload)
        payload["protocol"]["parity"] = "FAIL"
        errors = MODULE.validate_payload(payload, "synthetic", self.readme)
        self.assertTrue(any("parity must be PASS" in error for error in errors))

    def test_rejects_inconsistent_ratio(self) -> None:
        payload = copy.deepcopy(self.payload)
        payload["median"]["picard_to_turbo_wall_ratio"] = 99.0
        errors = MODULE.validate_payload(payload, "synthetic", self.readme)
        self.assertTrue(any("wall ratio" in error for error in errors))

    def test_rejects_malformed_binary_hash(self) -> None:
        payload = copy.deepcopy(self.payload)
        payload["protocol"]["turbo_picard_binary_sha256"] = "stale"
        errors = MODULE.validate_payload(payload, "synthetic", self.readme)
        self.assertTrue(any("binary SHA-256" in error for error in errors))


if __name__ == "__main__":
    unittest.main()
