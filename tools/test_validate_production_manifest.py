import importlib.util
import json
import tempfile
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("validate_production_manifest.py")
SPEC = importlib.util.spec_from_file_location("validate_production_manifest", MODULE_PATH)
validate_production_manifest = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(validate_production_manifest)
ManifestError = validate_production_manifest.ManifestError
validate = validate_production_manifest.validate


def valid_manifest():
    return {
        "schema_version": 1,
        "dataset_id": "test",
        "tier": "production_scale",
        "scope_caveat": "test fixture",
        "input": {
            "format": "BAM",
            "source_url": "https://example.org/commit",
            "source_revision": "a" * 40,
            "sha256": "b" * 64,
            "bytes": 1,
            "read_count": 1,
        },
        "software": {
            "picard_version": "3.4.0",
            "turbo_picard_version": "0.1.10",
            "turbo_picard_commit": "c" * 40,
        },
        "host": {
            "os": "Linux",
            "architecture": "x86_64",
            "cpu_model": "test",
            "logical_cpus": 2,
            "memory_bytes": 1024,
            "storage": "test",
        },
        "commands": [{
            "name": "MarkDuplicates",
            "arguments_sha256": "d" * 64,
            "compatibility_level": "B",
            "comparator": "semantic",
            "parity": "PASS",
            "repeats": 5,
            "wall_seconds": {},
            "peak_rss_bytes": {},
            "temporary_disk_bytes": {},
        }],
        "independent_reproduction": {
            "status": "pass",
            "reviewer": "reviewer",
        },
    }


class ProductionManifestTests(unittest.TestCase):
    def write(self, payload):
        handle = tempfile.NamedTemporaryFile("w", suffix=".json", delete=False)
        with handle:
            json.dump(payload, handle)
        return Path(handle.name)

    def test_accepts_valid_release_manifest(self):
        self.assertEqual(validate(self.write(valid_manifest()), release_ready=True)["dataset_id"], "test")

    def test_rejects_missing_input_hash(self):
        payload = valid_manifest()
        del payload["input"]["sha256"]
        with self.assertRaises(ManifestError):
            validate(self.write(payload))

    def test_rejects_non_full_commit(self):
        payload = valid_manifest()
        payload["software"]["turbo_picard_commit"] = "abc"
        with self.assertRaises(ManifestError):
            validate(self.write(payload))

    def test_rejects_failed_parity_for_release(self):
        payload = valid_manifest()
        payload["commands"][0]["parity"] = "FAIL"
        payload["commands"][0]["known_differences"] = ["fixture mismatch"]
        with self.assertRaises(ManifestError):
            validate(self.write(payload), release_ready=True)
