import argparse
import importlib.util
import sys
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("validate_production_dispatch.py")
SPEC = importlib.util.spec_from_file_location("validate_production_dispatch", MODULE_PATH)
module = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules[SPEC.name] = module
SPEC.loader.exec_module(module)


def valid_args(**overrides):
    values = {
        "dataset_id": "HG002-wgs-30x",
        "input_url": "https://example.org/HG002.bam",
        "input_format": "BAM",
        "input_sha256": "a" * 64,
        "reference_url": "",
        "reference_sha256": "",
        "source_revision": "accession-2026",
        "scope_caveat": "Pinned coordinate-sorted BAM; MarkDuplicates only.",
        "tier": "production_scale",
        "tools": "turbo-picard,picard,samtools",
        "require_tools": "turbo-picard,picard",
        "repeats": 5,
        "warmups": 1,
        "threads": 8,
        "read_name_regex": "null",
        "profile": "wgs_30x",
        "tag_duplicate_set_members": "false",
        "barcode_tag": "",
        "read_one_barcode_tag": "",
        "read_two_barcode_tag": "",
    }
    values.update(overrides)
    return argparse.Namespace(**values)


class ProductionDispatchTests(unittest.TestCase):
    def test_accepts_wgs_dispatch(self):
        module.validate_inputs(valid_args())

    def test_requires_picard_comparison_pair(self):
        with self.assertRaisesRegex(ValueError, "comparison pair"):
            module.validate_inputs(valid_args(tools="turbo-picard", require_tools="turbo-picard"))

    def test_requires_required_tools_to_be_selected(self):
        with self.assertRaisesRegex(ValueError, "subset of TOOLS"):
            module.validate_inputs(valid_args(tools="turbo-picard,picard", require_tools="turbo-picard,picard,samtools"))

    def test_requires_cram_reference_pair(self):
        with self.assertRaisesRegex(ValueError, "CRAM evidence"):
            module.validate_inputs(valid_args(input_format="CRAM", profile="cram_reference"))
        module.validate_inputs(
            valid_args(
                input_format="CRAM",
                profile="cram_reference",
                reference_url="https://example.org/reference.fa",
                reference_sha256="b" * 64,
            )
        )

    def test_requires_umi_barcode(self):
        with self.assertRaisesRegex(ValueError, "umi_panel"):
            module.validate_inputs(valid_args(profile="umi_panel"))
        module.validate_inputs(valid_args(profile="umi_panel", barcode_tag="RX"))

    def test_rejects_early_invalid_measurement_controls(self):
        for field, value, message in (
            ("input_sha256", "bad", "INPUT_SHA256"),
            ("input_url", "http://example.org/input.bam", "INPUT_URL"),
            ("read_name_regex", "", "READ_NAME_REGEX"),
            ("tag_duplicate_set_members", "yes", "TAG_DUPLICATE_SET_MEMBERS"),
        ):
            with self.subTest(field=field):
                with self.assertRaisesRegex(ValueError, message):
                    module.validate_inputs(valid_args(**{field: value}))

    def test_requires_five_repeats_and_positive_resources(self):
        for field, value in (("repeats", 4), ("warmups", -1), ("threads", 0)):
            with self.subTest(field=field):
                with self.assertRaisesRegex(ValueError, "REPEATS|WARMUPS|THREADS"):
                    module.validate_inputs(valid_args(**{field: value}))


if __name__ == "__main__":
    unittest.main()
