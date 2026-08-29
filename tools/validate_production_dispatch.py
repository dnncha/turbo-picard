#!/usr/bin/env python3
"""Validate inputs for a production-evidence benchmark dispatch."""

from __future__ import annotations

import argparse
import re
from typing import Iterable


PROFILE_CHOICES = (
    "wgs_30x",
    "wes_capture",
    "rna_seq",
    "umi_panel",
    "cram_reference",
    "multi_library",
    "cohort_batch",
)
TOOL_LIST_PATTERN = re.compile(r"[a-z0-9_.-]+(?:,[a-z0-9_.-]+)*")
DATASET_ID_PATTERN = re.compile(r"[A-Za-z0-9_.-]+")
SAM_TAG_PATTERN = re.compile(r"[A-Za-z0-9]{2}")
SHA256_PATTERN = re.compile(r"[0-9a-fA-F]{64}")


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(description=__doc__)
    result.add_argument("--dataset-id", required=True)
    result.add_argument("--input-url", required=True)
    result.add_argument("--input-format", required=True)
    result.add_argument("--input-sha256", required=True)
    result.add_argument("--reference-url", required=True)
    result.add_argument("--reference-sha256", required=True)
    result.add_argument("--source-revision", required=True)
    result.add_argument("--scope-caveat", required=True)
    result.add_argument("--tier", required=True)
    result.add_argument("--tools", required=True)
    result.add_argument("--require-tools", required=True)
    result.add_argument("--repeats", required=True, type=int)
    result.add_argument("--warmups", required=True, type=int)
    result.add_argument("--threads", required=True, type=int)
    result.add_argument("--read-name-regex", required=True)
    result.add_argument("--profile", required=True)
    result.add_argument("--tag-duplicate-set-members", required=True)
    result.add_argument("--barcode-tag", required=True)
    result.add_argument("--read-one-barcode-tag", required=True)
    result.add_argument("--read-two-barcode-tag", required=True)
    return result


def _tool_names(value: str, label: str) -> set[str]:
    if not TOOL_LIST_PATTERN.fullmatch(value):
        raise ValueError(f"{label} contains an invalid preset list")
    return set(value.split(","))


def _validate_optional_sam_tag(value: str, label: str) -> None:
    if value and not SAM_TAG_PATTERN.fullmatch(value):
        raise ValueError(f"{label} must be a two-character SAM tag")


def validate_inputs(args: argparse.Namespace) -> None:
    if not SHA256_PATTERN.fullmatch(args.input_sha256):
        raise ValueError("INPUT_SHA256 must be a 64-character hexadecimal SHA-256")

    input_format = args.input_format.upper()
    if input_format not in {"BAM", "CRAM"}:
        raise ValueError("INPUT_FORMAT must be BAM or CRAM")
    if not args.input_url.startswith("https://"):
        raise ValueError("INPUT_URL must use https://")

    reference_url = args.reference_url.strip()
    reference_sha256 = args.reference_sha256.strip()
    if input_format == "CRAM":
        if not reference_url.startswith("https://"):
            raise ValueError("CRAM evidence requires an HTTPS REFERENCE_URL")
        if not SHA256_PATTERN.fullmatch(reference_sha256):
            raise ValueError("CRAM evidence requires a 64-character REFERENCE_SHA256")
    elif reference_url or reference_sha256:
        raise ValueError("reference URL and hash are only valid for CRAM input")

    if not DATASET_ID_PATTERN.fullmatch(args.dataset_id):
        raise ValueError("DATASET_ID may contain only letters, digits, '.', '_' and '-'")
    if not args.source_revision.strip():
        raise ValueError("SOURCE_REVISION must be non-empty")
    if not args.scope_caveat.strip():
        raise ValueError("SCOPE_CAVEAT must be non-empty")
    if args.tier not in {"release_candidate", "production_scale"}:
        raise ValueError("TIER must be release_candidate or production_scale")

    selected_tools = _tool_names(args.tools, "TOOLS")
    required_tools = _tool_names(args.require_tools, "REQUIRE_TOOLS")
    if not required_tools.issubset(selected_tools):
        missing = ", ".join(sorted(required_tools - selected_tools))
        raise ValueError(f"REQUIRE_TOOLS must be a subset of TOOLS; missing: {missing}")
    comparison_tools = {"turbo-picard", "picard"}
    if not comparison_tools.issubset(selected_tools):
        missing = ", ".join(sorted(comparison_tools - selected_tools))
        raise ValueError(f"TOOLS must include the Picard comparison pair; missing: {missing}")
    if not comparison_tools.issubset(required_tools):
        missing = ", ".join(sorted(comparison_tools - required_tools))
        raise ValueError(f"REQUIRE_TOOLS must include the Picard comparison pair; missing: {missing}")

    if not args.read_name_regex.strip():
        raise ValueError("READ_NAME_REGEX must be non-empty; use null or default explicitly")
    if args.profile not in PROFILE_CHOICES:
        raise ValueError("PROFILE is not a supported production evidence profile")
    if args.profile == "umi_panel" and not any(
        (args.barcode_tag, args.read_one_barcode_tag, args.read_two_barcode_tag)
    ):
        raise ValueError("umi_panel requires BARCODE_TAG or mate-specific barcode tags")
    if args.profile == "cram_reference" and input_format != "CRAM":
        raise ValueError("cram_reference requires CRAM input")

    for value, label in (
        (args.barcode_tag, "BARCODE_TAG"),
        (args.read_one_barcode_tag, "READ_ONE_BARCODE_TAG"),
        (args.read_two_barcode_tag, "READ_TWO_BARCODE_TAG"),
    ):
        _validate_optional_sam_tag(value, label)
    if args.tag_duplicate_set_members not in {"true", "false"}:
        raise ValueError("TAG_DUPLICATE_SET_MEMBERS must be true or false")
    if args.repeats < 5 or args.warmups < 0 or args.threads < 1:
        raise ValueError("REPEATS must be at least 5; WARMUPS non-negative; THREADS positive")


def main(argv: Iterable[str] | None = None) -> int:
    argument_parser = parser()
    args = argument_parser.parse_args(argv)
    try:
        validate_inputs(args)
    except ValueError as error:
        argument_parser.error(str(error))
    print("production evidence dispatch inputs valid")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
