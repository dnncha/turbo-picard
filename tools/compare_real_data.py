#!/usr/bin/env python3
"""Run Picard-vs-turbo-picard comparisons on a real BAM.

This is intentionally separate from the fast synthetic CI parity scripts. It is
for public benchmark samples such as GIAB/NA12878 or for a lab's own
representative production BAMs, where the useful output is a durable evidence
bundle rather than a tiny unit-test fixture.
"""

from __future__ import annotations

import argparse
import datetime as dt
import decimal
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Iterable
from urllib.parse import urlparse

if __package__:
    from .disk_sort import sorted_records
else:
    from disk_sort import sorted_records


ROOT = Path(__file__).resolve().parents[1]
RELEASE_CANDIDATE_MIN_BYTES = 1_000_000
CRAM_RELEASE_CANDIDATE_MIN_BYTES = 500_000
RELEASE_CANDIDATE_REQUIRED_COMMANDS = {
    "ViewSam",
    "CleanSam",
    "CollectQualityYieldMetrics",
    "CollectAlignmentSummaryMetrics",
    "MarkDuplicates",
}
CRAM_RELEASE_CANDIDATE_REQUIRED_COMMANDS = {
    "CleanSam",
    "CollectQualityYieldMetrics",
    "CollectInsertSizeMetrics",
    "MarkDuplicates",
    "SortSam",
    "AddOrReplaceReadGroups",
}


@dataclass
class CommandEvidence:
    command: str
    status: str
    turbo_seconds: float
    picard_seconds: float
    speedup: float | None
    comparison: str
    turbo_artifact: str
    picard_artifact: str
    turbo_digest: str
    picard_digest: str
    turbo_exit_code: int | None = None
    picard_exit_code: int | None = None


@dataclass(frozen=True, order=True)
class MarkDuplicateRecord:
    query_name: str
    duplicate: bool
    duplicate_type: str | None
    duplicate_set_size: int | None
    duplicate_set_index: int | None
    rx_barcode: str | None
    bx_barcode: str | None
    by_barcode: str | None
    reference_name: str
    position: int
    mate_reference_name: str
    mate_position: int
    cigar: str
    template_length: int


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Compare turbo-picard and Picard on a real input BAM.",
    )
    parser.add_argument(
        "--input-bam",
        required=True,
        type=Path,
        help="Input BAM or CRAM alignment file.",
    )
    parser.add_argument(
        "--merge-input-bam",
        type=Path,
        help="Second alignment for MergeSamFiles; defaults to --input-bam.",
    )
    parser.add_argument(
        "--reference-fasta",
        type=Path,
        help="Reference FASTA required when --input-bam is CRAM.",
    )
    parser.add_argument(
        "--input-source-url",
        help="Optional public source URL or accession for the input BAM.",
    )
    parser.add_argument(
        "--input-source-commit",
        help="Optional source repository commit for URL-based public fixtures.",
    )
    parser.add_argument("--output-dir", required=True, type=Path)
    parser.add_argument(
        "--dataset-id",
        help="Dataset id to include in a generated manifest-entry.json artifact.",
    )
    parser.add_argument(
        "--scope-caveat",
        default="representative real-data comparison",
        help="Scope caveat to include in generated manifest-entry.json.",
    )
    parser.add_argument(
        "--release-tier",
        choices=["public_smoke", "release_candidate"],
        default="public_smoke",
        help="Evidence tier for generated manifest-entry.json.",
    )
    parser.add_argument(
        "--commands",
        nargs="+",
        default=["ViewSam", "CollectQualityYieldMetrics", "CollectAlignmentSummaryMetrics"],
        choices=[
            "ViewSam",
            "CleanSam",
            "CollectQualityYieldMetrics",
            "CollectAlignmentSummaryMetrics",
            "CollectHsMetrics",
            "MarkDuplicates",
            "AddOrReplaceReadGroups",
            "BuildBamIndex",
            "SortSam",
            "CollectInsertSizeMetrics",
            "ValidateSamFile",
            "RevertSam",
            "SamToFastq",
            "FixMateInformation",
            "SetNmMdAndUqTags",
            "MergeSamFiles",
            "ReplaceSamHeader",
            "MeanQualityByCycle",
            "QualityScoreDistribution",
            "CollectBaseDistributionByCycle",
            "CollectGcBiasMetrics",
            "CollectWgsMetrics",
            "CollectMultipleMetrics",
        ],
        help="Commands to compare on the real BAM.",
    )
    parser.add_argument(
        "--bait-interval-list",
        type=Path,
        help=(
            "BAIT interval-list required for CollectHsMetrics; provide the same "
            "pinned capture design used by the workflow."
        ),
    )
    parser.add_argument(
        "--target-interval-list",
        type=Path,
        help=(
            "TARGET interval-list required for CollectHsMetrics; provide the same "
            "pinned capture design used by the workflow."
        ),
    )
    parser.add_argument(
        "--markduplicates-arg",
        dest="markduplicates_args",
        action="append",
        default=[],
        type=parse_markduplicates_arg,
        metavar="KEY=VALUE",
        help=(
            "Additional MarkDuplicates KEY=VALUE argument, repeatable; "
            "I/O/M are reserved by the comparator"
        ),
    )
    parser.add_argument(
        "--collecthsmetrics-arg",
        dest="collecthsmetrics_args",
        action="append",
        default=[],
        type=parse_collecthsmetrics_arg,
        metavar="KEY=VALUE",
        help=(
            "Additional CollectHsMetrics KEY=VALUE argument, repeatable; "
            "input, output, reference, interval, and sidecar paths are reserved"
        ),
    )
    parser.add_argument(
        "--picard-command",
        default=None,
        help="Picard command prefix. Defaults to '<mamba|micromamba> run -p <conda-prefix> picard'.",
    )
    parser.add_argument(
        "--turbo-picard-command",
        default=str(ROOT / "target" / "release" / "picard"),
        help="turbo-picard command prefix.",
    )
    parser.add_argument(
        "--conda-prefix",
        default=os.environ.get("TURBO_PICARD_CONDA_PREFIX", str(ROOT / ".conda-turbo-picard")),
        help="Conda prefix containing upstream Picard when --picard-command is omitted.",
    )
    parser.add_argument("--skip-build", action="store_true")
    parser.add_argument(
        "--stop-after",
        type=int,
        help="Optional STOP_AFTER for metric commands that support it. Omit for full-file evidence.",
    )
    parser.add_argument(
        "--discard-work",
        action="store_true",
        help="Remove this run's intermediates only after passing parity and writing reports. Failed runs are retained.",
    )
    parser.add_argument(
        "--shareable-report",
        type=Path,
        help=(
            "Write an issue-ready Markdown summary that omits local paths, input "
            "hashes, command arguments, generated artifacts, and raw data."
        ),
    )
    parser.add_argument(
        "--include-public-source",
        action="store_true",
        help=(
            "Include the supplied source URL and revision in --shareable-report; "
            "use only when both are public and safe to disclose."
        ),
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.include_public_source and args.shareable_report is None:
        raise SystemExit("--include-public-source requires --shareable-report")
    if not args.input_bam.exists():
        raise SystemExit(f"missing input alignment: {args.input_bam}")
    if args.input_bam.suffix.lower() == ".cram":
        if args.reference_fasta is None:
            raise SystemExit("CRAM input requires --reference-fasta")
        if not args.reference_fasta.exists():
            raise SystemExit(f"missing reference FASTA: {args.reference_fasta}")
    if args.stop_after is not None and args.stop_after < 1:
        raise SystemExit("--stop-after must be positive")
    merge_input = args.merge_input_bam or args.input_bam
    if not merge_input.exists():
        raise SystemExit(f"missing merge input alignment: {merge_input}")
    if merge_input.suffix.lower() == ".cram" and args.reference_fasta is None:
        raise SystemExit("CRAM merge input requires --reference-fasta")

    validate_shareable_destination(args.output_dir, args.shareable_report)
    if len(set(args.commands)) != len(args.commands):
        raise SystemExit("comparison command list contains duplicate commands")

    validate_collecthsmetrics_request(args)

    validate_manifest_request(args)

    if not args.skip_build:
        run(["cargo", "build", "--release", "-p", "turbo-picard-cli", "--bin", "picard"])

    turbo_prefix = split_command(args.turbo_picard_command)
    if not Path(turbo_prefix[0]).exists() and shutil.which(turbo_prefix[0]) is None:
        raise SystemExit(f"missing turbo-picard command: {turbo_prefix[0]}")
    picard_prefix = split_command(args.picard_command) if args.picard_command else default_picard_prefix(args.conda_prefix)

    work_root = prepare_comparison_workspace(args.output_dir)

    # Failed runs deliberately keep their own intermediates for inspection.
    evidence: list[CommandEvidence] = []
    for command in args.commands:
        evidence.append(
            compare_command(
                command,
                args.input_bam,
                work_root,
                turbo_prefix,
                picard_prefix,
                args.stop_after,
                args.reference_fasta,
                merge_input,
                args.markduplicates_args,
                args.bait_interval_list,
                args.target_interval_list,
                args.collecthsmetrics_args,
            )
        )

    summary = {
        "input": input_metadata(
            args.input_bam,
            args.input_source_url,
            args.input_source_commit,
            args.reference_fasta,
        ),
        "picard_command": " ".join(picard_prefix),
        "picard_version": capture_version([*picard_prefix, "ViewSam", "--version"]),
        "turbo_picard_command": " ".join(turbo_prefix),
        "turbo_picard_version": capture_version([*turbo_prefix, "--version"]),
        "markduplicates_args": args.markduplicates_args,
        "collecthsmetrics_args": args.collecthsmetrics_args,
        "commands": [command_evidence_dict(row) for row in evidence],
        "parity": "PASS" if all(row.status == "PASS" for row in evidence) else "FAIL",
    }
    if "CollectHsMetrics" in args.commands:
        summary["collecthsmetrics_inputs"] = collecthsmetrics_input_metadata(
            args.bait_interval_list,
            args.target_interval_list,
        )
    json_path = args.output_dir / "real-data-comparison.json"
    write_new_text(json_path, json.dumps(summary, indent=2, sort_keys=True) + "\n")
    markdown_path = args.output_dir / "real-data-comparison.md"
    write_markdown(markdown_path, summary)
    if args.dataset_id:
        manifest_entry_path = args.output_dir / "manifest-entry.json"
        manifest_entry = build_manifest_entry(
            summary=summary,
            dataset_id=args.dataset_id,
            evidence_json=json_path,
            evidence_markdown=markdown_path,
            scope_caveat=args.scope_caveat,
            release_tier=args.release_tier,
        )
        write_new_text(
            manifest_entry_path,
            json.dumps(manifest_entry, indent=2, sort_keys=True) + "\n",
        )
    if args.shareable_report is not None:
        args.shareable_report.parent.mkdir(parents=True, exist_ok=True)
        write_shareable_markdown(
            args.shareable_report,
            summary,
            include_public_source=args.include_public_source,
        )

    print(f"wrote {json_path}")
    print(f"wrote {markdown_path}")
    if args.dataset_id:
        print(f"wrote {args.output_dir / 'manifest-entry.json'}")
    if args.shareable_report is not None:
        print(f"wrote {args.shareable_report}")
    for row in evidence:
        speedup = f"{row.speedup:.2f}x" if row.speedup is not None else "n/a"
        print(f"{row.command}: {row.status} parity, speedup={speedup}")
    if args.discard_work and summary["parity"] == "PASS":
        shutil.rmtree(work_root)
    return 0 if summary["parity"] == "PASS" else 1


def write_new_text(path: Path, text: str) -> None:
    """Never replace a file or follow a target symlink, including after preflight."""
    with path.open("x", encoding="utf-8") as handle:
        handle.write(text)


def validate_shareable_destination(output_dir: Path, destination: Path | None) -> None:
    if destination is None:
        return
    target = destination.resolve()
    reserved = {(output_dir / name).resolve() for name in (
        "real-data-comparison.json", "real-data-comparison.md", "manifest-entry.json")}
    if destination.exists() or destination.is_symlink():
        raise SystemExit(f"refusing to overwrite existing shareable report: {destination}")
    if target in reserved or target.is_relative_to((output_dir / "work").resolve()):
        raise SystemExit("--shareable-report must not replace a command output or evidence manifest")


def prepare_comparison_workspace(output_dir: Path) -> Path:
    """Claim a new workspace without deleting or replacing a previous run."""
    for name in ("work", "real-data-comparison.json", "real-data-comparison.md", "manifest-entry.json"):
        path = output_dir / name
        if path.exists() or path.is_symlink():
            raise SystemExit(
                f"refusing to overwrite existing evaluation artifact: {path}; "
                "choose a fresh --output-dir (the previous run has been preserved)"
            )
    output_dir.mkdir(parents=True, exist_ok=True)
    work_root = output_dir / "work"
    try:
        work_root.mkdir()  # Exclusive claim also rejects concurrent evaluators.
    except FileExistsError:
        raise SystemExit(f"evaluation workspace is already in use: {work_root}; choose a fresh --output-dir")
    return work_root


def validate_manifest_request(args: argparse.Namespace) -> None:
    """Reject invalid manifest requests before running expensive comparisons."""

    if not args.dataset_id:
        return

    if args.output_dir.name != "evidence":
        raise SystemExit(
            "manifest output directory must end in "
            "benchmarks/real-data/<dataset-id>/evidence when --dataset-id is set"
        )
    try:
        require_manifest_path(
            "manifest evidence JSON",
            args.output_dir / "real-data-comparison.json",
        )
    except SystemExit as error:
        raise SystemExit(
            "manifest output directory must be under "
            "benchmarks/real-data/<dataset-id>/evidence; "
            f"got {args.output_dir}"
        ) from error
    if not args.input_source_url or not args.input_source_commit:
        raise SystemExit(
            "manifest entries require input citation fields: source_url, source_commit "
            "(pass --input-source-url and --input-source-commit)"
        )
    validate_source_citation(
        str(args.dataset_id),
        str(args.input_source_url),
        str(args.input_source_commit),
    )

    commands = list(args.commands)
    duplicate_commands = sorted(
        command for command in set(commands) if commands.count(command) > 1
    )
    if duplicate_commands:
        raise SystemExit(
            "comparison command list contains duplicate commands: "
            + ", ".join(duplicate_commands)
        )

    if args.release_tier != "release_candidate":
        return

    required_commands = RELEASE_CANDIDATE_REQUIRED_COMMANDS
    if args.input_bam.suffix.lower() == ".cram":
        required_commands = CRAM_RELEASE_CANDIDATE_REQUIRED_COMMANDS
    missing_commands = sorted(set(required_commands) - set(commands))
    if missing_commands:
        raise SystemExit(
            "release_candidate manifest entries require commands: "
            + ", ".join(missing_commands)
        )

    size_bytes = args.input_bam.stat().st_size
    minimum_bytes = (
        CRAM_RELEASE_CANDIDATE_MIN_BYTES
        if args.input_bam.suffix.lower() == ".cram"
        else RELEASE_CANDIDATE_MIN_BYTES
    )
    if size_bytes < minimum_bytes:
        label = (
            "release_candidate CRAM"
            if args.input_bam.suffix.lower() == ".cram"
            else "release_candidate"
        )
        raise SystemExit(
            f"{label} manifest entries require input size >= {minimum_bytes} bytes; "
            f"got {size_bytes}"
        )


def split_command(command: str | None) -> list[str]:
    if not command:
        return []
    import shlex

    return shlex.split(command)


def parse_markduplicates_arg(value: str) -> str:
    key, separator, argument_value = value.partition("=")
    if not separator or not key or not argument_value:
        raise argparse.ArgumentTypeError("MarkDuplicates arguments must be KEY=VALUE")
    if key.upper() in {"I", "INPUT", "O", "OUTPUT", "M", "METRICS_FILE"}:
        raise argparse.ArgumentTypeError(
            "MarkDuplicates I, O, and M are owned by the comparator"
        )
    return value


def parse_collecthsmetrics_arg(value: str) -> str:
    key, separator, argument_value = value.partition("=")
    if not separator or not key or not argument_value:
        raise argparse.ArgumentTypeError("CollectHsMetrics arguments must be KEY=VALUE")
    if key.upper() in {
        "I",
        "INPUT",
        "O",
        "OUTPUT",
        "R",
        "REFERENCE_SEQUENCE",
        "BAIT",
        "BAIT_INTERVALS",
        "TARGET",
        "TARGET_INTERVALS",
        "PER_TARGET_COVERAGE",
        "PER_BASE_COVERAGE",
    }:
        raise argparse.ArgumentTypeError(
            "CollectHsMetrics input, output, reference, interval, and sidecar "
            "paths are owned by the comparator"
        )
    return value


def validate_collecthsmetrics_request(args: argparse.Namespace) -> None:
    if "CollectHsMetrics" not in args.commands:
        return
    if args.reference_fasta is None:
        raise SystemExit("CollectHsMetrics requires --reference-fasta")
    if not args.reference_fasta.exists():
        raise SystemExit(f"missing reference FASTA: {args.reference_fasta}")
    for label, path in (
        ("--bait-interval-list", args.bait_interval_list),
        ("--target-interval-list", args.target_interval_list),
    ):
        if path is None:
            raise SystemExit(f"CollectHsMetrics requires {label}")
        if not path.exists():
            raise SystemExit(f"missing {label}: {path}")


def default_picard_prefix(conda_prefix: str) -> list[str]:
    for name in ("mamba", "micromamba"):
        runner = shutil.which(name)
        if runner:
            return [runner, "run", "-p", conda_prefix, "picard"]
    picard = Path(conda_prefix) / "bin" / "picard"
    if picard.exists():
        return [str(picard)]
    raise SystemExit(
        "mamba, micromamba, or a picard binary under --conda-prefix is required when "
        "--picard-command is omitted"
    )


def alignment_io_args(input_alignment: Path, reference_fasta: Path | None) -> list[str]:
    args = [f"I={input_alignment}"]
    if input_alignment.suffix.lower() == ".cram":
        if reference_fasta is None:
            raise SystemExit("CRAM input requires --reference-fasta")
        args.append(f"R={reference_fasta}")
    return args


def materialize_alignment_sam(
    input_alignment: Path,
    output_sam: Path,
    command_prefix: list[str],
    reference_fasta: Path | None,
) -> float:
    command = [
        *command_prefix,
        "ViewSam",
        *alignment_io_args(input_alignment, reference_fasta),
        "VALIDATION_STRINGENCY=SILENT",
        "QUIET=true",
    ]
    return run(command, stdout=output_sam)


def write_comparison_sams(
    turbo_alignment: Path,
    picard_alignment: Path,
    turbo_sam: Path,
    picard_sam: Path,
    turbo_prefix: list[str],
    picard_prefix: list[str],
    reference_fasta: Path | None,
) -> None:
    materialize_alignment_sam(turbo_alignment, turbo_sam, turbo_prefix, reference_fasta)
    materialize_alignment_sam(picard_alignment, picard_sam, picard_prefix, reference_fasta)


def output_container_path(workdir: Path, prefix: str, input_alignment: Path) -> Path:
    stem = prefix.rstrip(".")
    suffix = input_alignment.suffix.lower()
    if suffix in {".bam", ".cram", ".sam"}:
        return workdir / f"{stem}{input_alignment.suffix}"
    return workdir / f"{stem}.bam"


def compare_command(
    command: str,
    input_bam: Path,
    work_root: Path,
    turbo_prefix: list[str],
    picard_prefix: list[str],
    stop_after: int | None,
    reference_fasta: Path | None,
    merge_input_bam: Path,
    markduplicates_args: list[str] | None = None,
    bait_interval_list: Path | None = None,
    target_interval_list: Path | None = None,
    collecthsmetrics_args: list[str] | None = None,
) -> CommandEvidence:
    markduplicates_args = markduplicates_args or []
    collecthsmetrics_args = collecthsmetrics_args or []
    workdir = work_root / command
    workdir.mkdir(parents=True)
    if command == "ViewSam":
        return compare_viewsam(input_bam, workdir, turbo_prefix, picard_prefix, reference_fasta)
    if command == "CleanSam":
        clean_sam_extra = (
            []
            if input_bam.suffix.lower() == ".cram"
            else ["CREATE_INDEX=true"]
        )
        return compare_bam_output(
            command,
            input_bam,
            workdir,
            turbo_prefix,
            picard_prefix,
            clean_sam_extra,
            reference_fasta,
        )
    if command == "MarkDuplicates":
        return compare_bam_output(
            command,
            input_bam,
            workdir,
            turbo_prefix,
            picard_prefix,
            ["M={metrics}", *markduplicates_args],
            reference_fasta,
        )
    if command == "CollectHsMetrics":
        return compare_hs_metrics(
            input_bam,
            workdir,
            turbo_prefix,
            picard_prefix,
            reference_fasta,
            bait_interval_list,
            target_interval_list,
            collecthsmetrics_args,
        )
    if command == "AddOrReplaceReadGroups":
        return compare_add_or_replace_read_groups(
            input_bam, workdir, turbo_prefix, picard_prefix, reference_fasta
        )
    if command == "BuildBamIndex":
        return compare_build_bam_index(
            input_bam, workdir, turbo_prefix, picard_prefix, reference_fasta
        )
    if command == "RevertSam":
        return compare_revertsam(input_bam, workdir, turbo_prefix, picard_prefix, reference_fasta)
    if command == "SamToFastq":
        return compare_samtofastq(input_bam, workdir, turbo_prefix, picard_prefix, reference_fasta)
    if command == "SortSam":
        return compare_sortsam(input_bam, workdir, turbo_prefix, picard_prefix, reference_fasta)
    if command == "FixMateInformation":
        return compare_fix_mate_information(
            input_bam, workdir, turbo_prefix, picard_prefix, reference_fasta
        )
    if command == "SetNmMdAndUqTags":
        return compare_set_nm_md_and_uq_tags(
            input_bam, workdir, turbo_prefix, picard_prefix, reference_fasta
        )
    if command == "MergeSamFiles":
        return compare_merge_sam_files(
            input_bam,
            merge_input_bam,
            workdir,
            turbo_prefix,
            picard_prefix,
            reference_fasta,
        )
    if command == "ReplaceSamHeader":
        return compare_replace_sam_header(
            input_bam, workdir, turbo_prefix, picard_prefix, reference_fasta
        )
    if command == "CollectInsertSizeMetrics":
        extra = [f"STOP_AFTER={stop_after}"] if stop_after is not None else []
        return compare_insert_size_metrics(
            input_bam, workdir, turbo_prefix, picard_prefix, extra, reference_fasta
        )
    if command == "ValidateSamFile":
        return compare_validate_sam_file(
            input_bam, workdir, turbo_prefix, picard_prefix, reference_fasta
        )
    if command in {"CollectQualityYieldMetrics", "CollectAlignmentSummaryMetrics"}:
        extra = [f"STOP_AFTER={stop_after}"] if stop_after is not None else []
        return compare_metrics(
            command, input_bam, workdir, turbo_prefix, picard_prefix, extra, reference_fasta
        )
    if command in {
        "MeanQualityByCycle",
        "QualityScoreDistribution",
        "CollectBaseDistributionByCycle",
    }:
        extra = [f"STOP_AFTER={stop_after}"] if stop_after is not None else []
        return compare_chart_metrics(
            command, input_bam, workdir, turbo_prefix, picard_prefix, extra, reference_fasta
        )
    if command == "CollectGcBiasMetrics":
        if reference_fasta is None:
            raise SystemExit("CollectGcBiasMetrics requires --reference-fasta")
        extra = [f"STOP_AFTER={stop_after}"] if stop_after is not None else []
        return compare_gc_bias_metrics(
            input_bam, workdir, turbo_prefix, picard_prefix, extra, reference_fasta
        )
    if command == "CollectWgsMetrics":
        if reference_fasta is None:
            raise SystemExit("CollectWgsMetrics requires --reference-fasta")
        extra = [f"STOP_AFTER={stop_after}"] if stop_after is not None else []
        return compare_wgs_metrics(
            input_bam, workdir, turbo_prefix, picard_prefix, extra, reference_fasta
        )
    if command == "CollectMultipleMetrics":
        extra = [f"STOP_AFTER={stop_after}"] if stop_after is not None else []
        return compare_collect_multiple_metrics(
            input_bam, workdir, turbo_prefix, picard_prefix, extra, reference_fasta
        )
    raise AssertionError(command)


def compare_viewsam(
    input_bam: Path,
    workdir: Path,
    turbo_prefix: list[str],
    picard_prefix: list[str],
    reference_fasta: Path | None,
) -> CommandEvidence:
    turbo_out = workdir / "turbo.sam"
    picard_out = workdir / "picard.sam"
    io_args = alignment_io_args(input_bam, reference_fasta)
    turbo_seconds = run(
        [*turbo_prefix, "ViewSam", *io_args, "VALIDATION_STRINGENCY=SILENT", "QUIET=true"],
        stdout=turbo_out,
    )
    picard_seconds = run(
        [*picard_prefix, "ViewSam", *io_args, "VALIDATION_STRINGENCY=SILENT", "QUIET=true"],
        stdout=picard_out,
    )
    turbo_digest = digest_sam_records(turbo_out)
    picard_digest = digest_sam_records(picard_out)
    return evidence("ViewSam", turbo_seconds, picard_seconds, "SAM record digest", turbo_out, picard_out, turbo_digest, picard_digest)


def compare_hs_metrics(
    input_bam: Path,
    workdir: Path,
    turbo_prefix: list[str],
    picard_prefix: list[str],
    reference_fasta: Path | None,
    bait_interval_list: Path | None,
    target_interval_list: Path | None,
    extra: list[str],
) -> CommandEvidence:
    command = "CollectHsMetrics"
    reference = require_reference_fasta(reference_fasta, command)
    if bait_interval_list is None or target_interval_list is None:
        raise SystemExit(
            "CollectHsMetrics requires --bait-interval-list and "
            "--target-interval-list"
        )
    for label, path in (
        ("bait interval-list", bait_interval_list),
        ("target interval-list", target_interval_list),
    ):
        if not path.exists():
            raise SystemExit(f"missing {label}: {path}")

    turbo_metrics = workdir / "turbo.metrics.txt"
    picard_metrics = workdir / "picard.metrics.txt"
    turbo_per_target = workdir / "turbo.per-target.txt"
    picard_per_target = workdir / "picard.per-target.txt"
    turbo_per_base = workdir / "turbo.per-base.txt"
    picard_per_base = workdir / "picard.per-base.txt"

    reference_args = alignment_io_args(input_bam, reference)
    if input_bam.suffix.lower() != ".cram":
        reference_args.append(f"R={reference}")
    turbo_common = [
        command,
        *reference_args,
        f"BAIT={bait_interval_list}",
        f"TARGET={target_interval_list}",
        "BAIT_SET_NAME=capture-audit",
        "SAMPLE_SIZE=0",
        "VALIDATION_STRINGENCY=SILENT",
        "QUIET=true",
        *extra,
    ]
    picard_common = [
        command,
        *reference_args,
        f"BAIT_INTERVALS={bait_interval_list}",
        f"TARGET_INTERVALS={target_interval_list}",
        "BAIT_SET_NAME=capture-audit",
        "SAMPLE_SIZE=0",
        "VALIDATION_STRINGENCY=SILENT",
        "QUIET=true",
        *extra,
    ]
    turbo_seconds = run(
        [
            *turbo_prefix,
            *turbo_common,
            f"O={turbo_metrics}",
            f"PER_TARGET_COVERAGE={turbo_per_target}",
            f"PER_BASE_COVERAGE={turbo_per_base}",
        ]
    )
    picard_seconds = run(
        [
            *picard_prefix,
            *picard_common,
            f"O={picard_metrics}",
            f"PER_TARGET_COVERAGE={picard_per_target}",
            f"PER_BASE_COVERAGE={picard_per_base}",
        ]
    )
    turbo_digest = digest_hsmetrics_outputs(
        turbo_metrics,
        turbo_per_target,
        turbo_per_base,
        "turbo-picard",
    )
    picard_digest = digest_hsmetrics_outputs(
        picard_metrics,
        picard_per_target,
        picard_per_base,
        "Picard",
    )
    return evidence(
        command,
        turbo_seconds,
        picard_seconds,
        "stable HsMetrics digest plus per-target/per-base sidecar digests",
        turbo_metrics,
        picard_metrics,
        turbo_digest,
        picard_digest,
    )


def compare_wgs_metrics(
    input_bam: Path,
    workdir: Path,
    turbo_prefix: list[str],
    picard_prefix: list[str],
    extra: list[str],
    reference_fasta: Path,
) -> CommandEvidence:
    turbo_out = workdir / "turbo.wgs.metrics.txt"
    picard_out = workdir / "picard.wgs.metrics.txt"
    common = [
        "CollectWgsMetrics",
        *alignment_io_args(input_bam, reference_fasta),
        f"R={reference_fasta}",
        "VALIDATION_STRINGENCY=SILENT",
        "QUIET=true",
        *extra,
    ]
    turbo_seconds = run([*turbo_prefix, *common, f"O={turbo_out}"])
    picard_seconds = run([*picard_prefix, *common, f"O={picard_out}"])
    turbo_digest = digest_stable_text_or_missing(turbo_out, "turbo-picard WGS metrics")
    picard_digest = digest_stable_text_or_missing(picard_out, "Picard WGS metrics")
    label = "stable metrics digest" if not extra else f"stable metrics digest ({' '.join(extra)})"
    return evidence(
        "CollectWgsMetrics",
        turbo_seconds,
        picard_seconds,
        label,
        turbo_out,
        picard_out,
        turbo_digest,
        picard_digest,
    )


def compare_collect_multiple_metrics(
    input_bam: Path,
    workdir: Path,
    turbo_prefix: list[str],
    picard_prefix: list[str],
    extra: list[str],
    reference_fasta: Path | None,
) -> CommandEvidence:
    turbo_prefix_path = workdir / "turbo.multi"
    picard_prefix_path = workdir / "picard.multi"
    common = [
        "CollectMultipleMetrics",
        *alignment_io_args(input_bam, reference_fasta),
        "PROGRAM=null",
        "PROGRAM=CollectQualityYieldMetrics",
        "PROGRAM=CollectInsertSizeMetrics",
        "VALIDATION_STRINGENCY=SILENT",
        "QUIET=true",
        f"TMP_DIR={workdir}",
        *extra,
    ]
    turbo_seconds = run([*turbo_prefix, *common, f"O={turbo_prefix_path}"])
    picard_seconds = run([*picard_prefix, *common, f"O={picard_prefix_path}"])
    turbo_quality = Path(f"{turbo_prefix_path}.quality_yield_metrics")
    picard_quality = Path(f"{picard_prefix_path}.quality_yield_metrics")
    turbo_digest = digest_stable_text_or_missing(
        turbo_quality,
        "turbo-picard CollectMultipleMetrics quality yield",
    )
    picard_digest = digest_stable_text_or_missing(
        picard_quality,
        "Picard CollectMultipleMetrics quality yield",
    )
    label = (
        "stable CollectMultipleMetrics quality-yield digest"
        if not extra
        else f"stable CollectMultipleMetrics quality-yield digest ({' '.join(extra)})"
    )
    return evidence(
        "CollectMultipleMetrics",
        turbo_seconds,
        picard_seconds,
        label,
        turbo_quality,
        picard_quality,
        turbo_digest,
        picard_digest,
    )


def compare_gc_bias_metrics(
    input_bam: Path,
    workdir: Path,
    turbo_prefix: list[str],
    picard_prefix: list[str],
    extra: list[str],
    reference_fasta: Path,
) -> CommandEvidence:
    turbo_detail = workdir / "turbo.detail.txt"
    picard_detail = workdir / "picard.detail.txt"
    turbo_summary = workdir / "turbo.summary.txt"
    picard_summary = workdir / "picard.summary.txt"
    turbo_chart = workdir / "turbo.chart.pdf"
    picard_chart = workdir / "picard.chart.pdf"
    fake_rscript = workdir / "Rscript"
    write_fake_rscript(fake_rscript)
    common = [
        *alignment_io_args(input_bam, reference_fasta),
        f"R={reference_fasta}",
        "VALIDATION_STRINGENCY=SILENT",
        "QUIET=true",
        *extra,
    ]
    picard_env = rscript_shim_env(workdir)
    shimmed_picard = picard_prefix_with_rscript_shim(picard_prefix, workdir)
    turbo_seconds = run(
        [
            *turbo_prefix,
            "CollectGcBiasMetrics",
            *common,
            f"O={turbo_detail}",
            f"S={turbo_summary}",
            f"CHART={turbo_chart}",
        ],
        env=picard_env,
    )
    picard_seconds = run(
        [
            *shimmed_picard,
            "CollectGcBiasMetrics",
            *common,
            f"O={picard_detail}",
            f"S={picard_summary}",
            f"CHART={picard_chart}",
        ],
        env=picard_env,
    )
    turbo_digest = digest_stable_text_or_missing(
        turbo_detail,
        "turbo-picard GC bias detail",
    )
    picard_digest = digest_stable_text_or_missing(
        picard_detail,
        "Picard GC bias detail",
    )
    label = "stable metrics digest" if not extra else f"stable metrics digest ({' '.join(extra)})"
    return evidence(
        "CollectGcBiasMetrics",
        turbo_seconds,
        picard_seconds,
        label,
        turbo_detail,
        picard_detail,
        turbo_digest,
        picard_digest,
    )


def compare_chart_metrics(
    command: str,
    input_bam: Path,
    workdir: Path,
    turbo_prefix: list[str],
    picard_prefix: list[str],
    extra: list[str],
    reference_fasta: Path | None,
) -> CommandEvidence:
    turbo_out = workdir / "turbo.metrics.txt"
    picard_out = workdir / "picard.metrics.txt"
    turbo_chart = workdir / "turbo.chart.pdf"
    picard_chart = workdir / "picard.chart.pdf"
    fake_rscript = workdir / "Rscript"
    write_fake_rscript(fake_rscript)
    common = [
        *alignment_io_args(input_bam, reference_fasta),
        "VALIDATION_STRINGENCY=SILENT",
        "QUIET=true",
        *extra,
    ]
    turbo_seconds = run(
        [
            *turbo_prefix,
            command,
            *common,
            f"O={turbo_out}",
            f"CHART={turbo_chart}",
        ]
    )
    picard_seconds = run(
        [
            *picard_prefix,
            command,
            *common,
            f"O={picard_out}",
            f"CHART={picard_chart}",
        ]
    )
    turbo_digest = digest_stable_text_or_missing(turbo_out, "turbo-picard metrics")
    picard_digest = digest_stable_text_or_missing(picard_out, "Picard metrics")
    label = "stable metrics digest" if not extra else f"stable metrics digest ({' '.join(extra)})"
    return evidence(
        command,
        turbo_seconds,
        picard_seconds,
        label,
        turbo_out,
        picard_out,
        turbo_digest,
        picard_digest,
    )


def compare_metrics(
    command: str,
    input_bam: Path,
    workdir: Path,
    turbo_prefix: list[str],
    picard_prefix: list[str],
    extra: list[str],
    reference_fasta: Path | None,
) -> CommandEvidence:
    turbo_out = workdir / "turbo.metrics.txt"
    picard_out = workdir / "picard.metrics.txt"
    common = [
        command,
        *alignment_io_args(input_bam, reference_fasta),
        "VALIDATION_STRINGENCY=SILENT",
        "QUIET=true",
        *extra,
    ]
    turbo_seconds = run([*turbo_prefix, *common, f"O={turbo_out}"])
    picard_seconds = run([*picard_prefix, *common, f"O={picard_out}"])
    turbo_digest = digest_stable_text_or_missing(turbo_out, "turbo-picard metrics")
    picard_digest = digest_stable_text_or_missing(picard_out, "Picard metrics")
    label = "stable metrics digest" if not extra else f"stable metrics digest ({' '.join(extra)})"
    return evidence(command, turbo_seconds, picard_seconds, label, turbo_out, picard_out, turbo_digest, picard_digest)


def compare_insert_size_metrics(
    input_bam: Path,
    workdir: Path,
    turbo_prefix: list[str],
    picard_prefix: list[str],
    extra: list[str],
    reference_fasta: Path | None,
) -> CommandEvidence:
    command = "CollectInsertSizeMetrics"
    turbo_out = workdir / "turbo.metrics.txt"
    picard_out = workdir / "picard.metrics.txt"
    turbo_histogram = workdir / "turbo.insert-size.pdf"
    picard_histogram = workdir / "picard.insert-size.pdf"
    fake_rscript = workdir / "Rscript"
    write_fake_rscript(fake_rscript)
    picard_env = rscript_shim_env(workdir)
    picard_prefix = picard_prefix_with_rscript_shim(picard_prefix, workdir)
    common = [
        command,
        *alignment_io_args(input_bam, reference_fasta),
        "VALIDATION_STRINGENCY=SILENT",
        "QUIET=true",
        *extra,
    ]
    turbo_seconds = run([*turbo_prefix, *common, f"O={turbo_out}", f"H={turbo_histogram}"])
    picard_seconds = run(
        [*picard_prefix, *common, f"O={picard_out}", f"H={picard_histogram}"],
        env=picard_env,
    )
    turbo_digest = digest_stable_text_or_missing(turbo_out, "turbo-picard metrics")
    picard_digest = digest_stable_text_or_missing(picard_out, "Picard metrics")
    label = "stable metrics digest with insert-size histogram" if not extra else (
        f"stable metrics digest with insert-size histogram ({' '.join(extra)})"
    )
    return evidence(command, turbo_seconds, picard_seconds, label, turbo_out, picard_out, turbo_digest, picard_digest)


def compare_build_bam_index(
    input_bam: Path,
    workdir: Path,
    turbo_prefix: list[str],
    picard_prefix: list[str],
    reference_fasta: Path | None,
) -> CommandEvidence:
    command = "BuildBamIndex"
    turbo_bai = workdir / "turbo.bai"
    picard_bai = workdir / "picard.bai"
    common = [
        command,
        *alignment_io_args(input_bam, reference_fasta),
        "VALIDATION_STRINGENCY=SILENT",
        "QUIET=true",
    ]
    turbo_seconds = run([*turbo_prefix, *common, f"O={turbo_bai}"])
    picard_seconds = run([*picard_prefix, *common, f"O={picard_bai}"])
    turbo_digest = digest_file(turbo_bai)
    picard_digest = digest_file(picard_bai)
    return evidence(
        command,
        turbo_seconds,
        picard_seconds,
        "BAI binary digest",
        turbo_bai,
        picard_bai,
        turbo_digest,
        picard_digest,
    )


def compare_add_or_replace_read_groups(
    input_bam: Path,
    workdir: Path,
    turbo_prefix: list[str],
    picard_prefix: list[str],
    reference_fasta: Path | None,
) -> CommandEvidence:
    command = "AddOrReplaceReadGroups"
    turbo_bam = output_container_path(workdir, "turbo.", input_bam)
    picard_bam = output_container_path(workdir, "picard.", input_bam)
    turbo_sam = workdir / "turbo.view.sam"
    picard_sam = workdir / "picard.view.sam"
    common = [
        command,
        *alignment_io_args(input_bam, reference_fasta),
        "RGID=turbo",
        "RGLB=library",
        "RGPL=ILLUMINA",
        "RGPU=unit",
        "RGSM=sample",
        "CREATE_INDEX=true",
        "VALIDATION_STRINGENCY=SILENT",
        "QUIET=true",
    ]
    turbo_seconds = run([*turbo_prefix, *common, f"O={turbo_bam}"])
    picard_seconds = run([*picard_prefix, *common, f"O={picard_bam}"])
    write_comparison_sams(
        turbo_bam,
        picard_bam,
        turbo_sam,
        picard_sam,
        turbo_prefix,
        picard_prefix,
        reference_fasta,
    )
    turbo_digest = digest_sam_records_and_read_groups(turbo_sam)
    picard_digest = digest_sam_records_and_read_groups(picard_sam)
    return evidence(
        command,
        turbo_seconds,
        picard_seconds,
        "SAM record digest plus read-group header digest",
        turbo_bam,
        picard_bam,
        turbo_digest,
        picard_digest,
    )


def compare_revertsam(
    input_bam: Path,
    workdir: Path,
    turbo_prefix: list[str],
    picard_prefix: list[str],
    reference_fasta: Path | None,
) -> CommandEvidence:
    command = "RevertSam"
    turbo_bam = output_container_path(workdir, "turbo.", input_bam)
    picard_bam = output_container_path(workdir, "picard.", input_bam)
    turbo_sam = workdir / "turbo.view.sam"
    picard_sam = workdir / "picard.view.sam"
    common = [
        command,
        *alignment_io_args(input_bam, reference_fasta),
        "VALIDATION_STRINGENCY=SILENT",
        "QUIET=true",
    ]
    turbo_seconds = run([*turbo_prefix, *common, f"O={turbo_bam}"])
    picard_seconds = run([*picard_prefix, *common, f"O={picard_bam}"])
    write_comparison_sams(
        turbo_bam,
        picard_bam,
        turbo_sam,
        picard_sam,
        turbo_prefix,
        picard_prefix,
        reference_fasta,
    )
    turbo_digest = digest_sam_records(turbo_sam)
    picard_digest = digest_sam_records(picard_sam)
    return evidence(
        command,
        turbo_seconds,
        picard_seconds,
        "reverted SAM record digest",
        turbo_bam,
        picard_bam,
        turbo_digest,
        picard_digest,
    )


def compare_samtofastq(
    input_bam: Path,
    workdir: Path,
    turbo_prefix: list[str],
    picard_prefix: list[str],
    reference_fasta: Path | None,
) -> CommandEvidence:
    command = "SamToFastq"
    turbo_r1 = workdir / "turbo-r1.fastq"
    turbo_r2 = workdir / "turbo-r2.fastq"
    turbo_unpaired = workdir / "turbo-unpaired.fastq"
    picard_r1 = workdir / "picard-r1.fastq"
    picard_r2 = workdir / "picard-r2.fastq"
    picard_unpaired = workdir / "picard-unpaired.fastq"
    common = [
        command,
        *alignment_io_args(input_bam, reference_fasta),
        "VALIDATION_STRINGENCY=SILENT",
        "QUIET=true",
    ]
    turbo_seconds = run(
        [
            *turbo_prefix,
            *common,
            f"FASTQ={turbo_r1}",
            f"SECOND_END_FASTQ={turbo_r2}",
            f"UNPAIRED_FASTQ={turbo_unpaired}",
        ]
    )
    picard_seconds = run(
        [
            *picard_prefix,
            *common,
            f"FASTQ={picard_r1}",
            f"SECOND_END_FASTQ={picard_r2}",
            f"UNPAIRED_FASTQ={picard_unpaired}",
        ]
    )
    turbo_digest = digest_files([turbo_r1, turbo_r2, turbo_unpaired])
    picard_digest = digest_files([picard_r1, picard_r2, picard_unpaired])
    return evidence(
        command,
        turbo_seconds,
        picard_seconds,
        "FASTQ trio digest",
        turbo_r1,
        picard_r1,
        turbo_digest,
        picard_digest,
    )


def compare_bam_output(
    command: str,
    input_bam: Path,
    workdir: Path,
    turbo_prefix: list[str],
    picard_prefix: list[str],
    extra_templates: list[str],
    reference_fasta: Path | None,
) -> CommandEvidence:
    turbo_bam = output_container_path(workdir, "turbo.", input_bam)
    picard_bam = output_container_path(workdir, "picard.", input_bam)
    turbo_metrics = workdir / "turbo.metrics.txt"
    picard_metrics = workdir / "picard.metrics.txt"
    turbo_sam = workdir / "turbo.view.sam"
    picard_sam = workdir / "picard.view.sam"

    turbo_extra = [value.replace("{metrics}", str(turbo_metrics)) for value in extra_templates]
    picard_extra = [value.replace("{metrics}", str(picard_metrics)) for value in extra_templates]
    common = [
        command,
        *alignment_io_args(input_bam, reference_fasta),
        "VALIDATION_STRINGENCY=SILENT",
        "QUIET=true",
    ]
    turbo_seconds = run([*turbo_prefix, *common, f"O={turbo_bam}", *turbo_extra])
    picard_seconds = run([*picard_prefix, *common, f"O={picard_bam}", *picard_extra])

    write_comparison_sams(
        turbo_bam,
        picard_bam,
        turbo_sam,
        picard_sam,
        turbo_prefix,
        picard_prefix,
        reference_fasta,
    )
    turbo_digest = digest_sam_records(turbo_sam)
    picard_digest = digest_sam_records(picard_sam)
    comparison = "post-command SAM record digest"
    if command == "MarkDuplicates":
        turbo_metric_digest = digest_stable_text(turbo_metrics)
        picard_metric_digest = digest_stable_text(picard_metrics)
        turbo_digest = (
            f"{digest_markduplicates_semantics(turbo_sam)};metrics={turbo_metric_digest}"
        )
        picard_digest = (
            f"{digest_markduplicates_semantics(picard_sam)};metrics={picard_metric_digest}"
        )
        comparison = "duplicate-marking semantic digest plus stable metrics digest"
    return evidence(command, turbo_seconds, picard_seconds, comparison, turbo_bam, picard_bam, turbo_digest, picard_digest)


def require_reference_fasta(reference_fasta: Path | None, command: str) -> Path:
    if reference_fasta is None:
        raise SystemExit(f"{command} requires --reference-fasta")
    if not reference_fasta.exists():
        raise SystemExit(f"missing reference FASTA: {reference_fasta}")
    return reference_fasta


def reference_io_args(input_alignment: Path, reference_fasta: Path) -> list[str]:
    args = alignment_io_args(input_alignment, reference_fasta)
    if input_alignment.suffix.lower() != ".cram":
        args.append(f"R={reference_fasta}")
    return args


def cram_reference_arg(reference_fasta: Path | None, *paths: Path) -> list[str]:
    if reference_fasta is None:
        return []
    if any(path.suffix.lower() == ".cram" for path in paths):
        return [f"R={reference_fasta}"]
    return []


def compare_fix_mate_information(
    input_bam: Path,
    workdir: Path,
    turbo_prefix: list[str],
    picard_prefix: list[str],
    reference_fasta: Path | None,
) -> CommandEvidence:
    command = "FixMateInformation"
    turbo_prep = output_container_path(workdir, "turbo.queryname.", input_bam)
    picard_prep = output_container_path(workdir, "picard.queryname.", input_bam)
    turbo_out = output_container_path(workdir, "turbo.", input_bam)
    picard_out = output_container_path(workdir, "picard.", input_bam)
    turbo_sam = workdir / "turbo.view.sam"
    picard_sam = workdir / "picard.view.sam"
    sort_common = [
        "SortSam",
        *alignment_io_args(input_bam, reference_fasta),
        "SORT_ORDER=queryname",
        "VALIDATION_STRINGENCY=SILENT",
        "QUIET=true",
    ]
    fix_common = [
        command,
        "ASSUME_SORTED=true",
        "SORT_ORDER=queryname",
        "VALIDATION_STRINGENCY=SILENT",
        "QUIET=true",
    ]
    turbo_seconds = run([*turbo_prefix, *sort_common, f"O={turbo_prep}"])
    turbo_seconds += run(
        [
            *turbo_prefix,
            *fix_common,
            *alignment_io_args(turbo_prep, reference_fasta),
            f"O={turbo_out}",
        ]
    )
    picard_seconds = run([*picard_prefix, *sort_common, f"O={picard_prep}"])
    picard_seconds += run(
        [
            *picard_prefix,
            *fix_common,
            *alignment_io_args(picard_prep, reference_fasta),
            f"O={picard_out}",
        ]
    )
    write_comparison_sams(
        turbo_out,
        picard_out,
        turbo_sam,
        picard_sam,
        turbo_prefix,
        picard_prefix,
        reference_fasta,
    )
    turbo_digest = digest_stable_sam(turbo_sam)
    picard_digest = digest_stable_sam(picard_sam)
    return evidence(
        command,
        turbo_seconds,
        picard_seconds,
        "stable SAM digest after queryname sort and mate fixing",
        turbo_out,
        picard_out,
        turbo_digest,
        picard_digest,
    )


def compare_set_nm_md_and_uq_tags(
    input_bam: Path,
    workdir: Path,
    turbo_prefix: list[str],
    picard_prefix: list[str],
    reference_fasta: Path | None,
) -> CommandEvidence:
    command = "SetNmMdAndUqTags"
    reference = require_reference_fasta(reference_fasta, command)
    turbo_out = output_container_path(workdir, "turbo.", input_bam)
    picard_out = output_container_path(workdir, "picard.", input_bam)
    turbo_sam = workdir / "turbo.view.sam"
    picard_sam = workdir / "picard.view.sam"
    common = [
        command,
        *reference_io_args(input_bam, reference),
        "VALIDATION_STRINGENCY=SILENT",
        "QUIET=true",
    ]
    turbo_seconds = run([*turbo_prefix, *common, f"O={turbo_out}"])
    picard_seconds = run([*picard_prefix, *common, f"O={picard_out}"])
    write_comparison_sams(
        turbo_out,
        picard_out,
        turbo_sam,
        picard_sam,
        turbo_prefix,
        picard_prefix,
        reference,
    )
    turbo_digest = digest_stable_sam(turbo_sam)
    picard_digest = digest_stable_sam(picard_sam)
    return evidence(
        command,
        turbo_seconds,
        picard_seconds,
        "stable SAM digest with NM/MD/UQ tags",
        turbo_out,
        picard_out,
        turbo_digest,
        picard_digest,
    )


def compare_merge_sam_files(
    input_bam: Path,
    merge_input_bam: Path,
    workdir: Path,
    turbo_prefix: list[str],
    picard_prefix: list[str],
    reference_fasta: Path | None,
) -> CommandEvidence:
    command = "MergeSamFiles"
    turbo_out = output_container_path(workdir, "turbo.", input_bam)
    picard_out = output_container_path(workdir, "picard.", input_bam)
    turbo_sam = workdir / "turbo.view.sam"
    picard_sam = workdir / "picard.view.sam"
    tail = [
        "SORT_ORDER=coordinate",
        "ASSUME_SORTED=true",
        "VALIDATION_STRINGENCY=SILENT",
        "QUIET=true",
        *cram_reference_arg(
            reference_fasta,
            input_bam,
            merge_input_bam,
            turbo_out,
            picard_out,
        ),
    ]
    turbo_seconds = run(
        [
            *turbo_prefix,
            command,
            f"I={input_bam}",
            f"I={merge_input_bam}",
            f"O={turbo_out}",
            *tail,
        ]
    )
    picard_seconds = run(
        [
            *picard_prefix,
            command,
            f"I={input_bam}",
            f"I={merge_input_bam}",
            f"O={picard_out}",
            *tail,
        ]
    )
    write_comparison_sams(
        turbo_out,
        picard_out,
        turbo_sam,
        picard_sam,
        turbo_prefix,
        picard_prefix,
        reference_fasta,
    )
    turbo_digest = digest_coordinate_sorted_sam_multiset(turbo_sam)
    picard_digest = digest_coordinate_sorted_sam_multiset(picard_sam)
    return evidence(
        command,
        turbo_seconds,
        picard_seconds,
        "coordinate-sorted SAM record multiset digest",
        turbo_out,
        picard_out,
        turbo_digest,
        picard_digest,
    )


def compare_replace_sam_header(
    input_bam: Path,
    workdir: Path,
    turbo_prefix: list[str],
    picard_prefix: list[str],
    reference_fasta: Path | None,
) -> CommandEvidence:
    command = "ReplaceSamHeader"
    replacement_header = workdir / "replacement-header.sam"
    turbo_out = output_container_path(workdir, "turbo.", input_bam)
    picard_out = output_container_path(workdir, "picard.", input_bam)
    turbo_sam = workdir / "turbo.view.sam"
    picard_sam = workdir / "picard.view.sam"
    header_source = workdir / "input-header.sam"
    run(
        [
            *turbo_prefix,
            "ViewSam",
            *alignment_io_args(input_bam, reference_fasta),
            "HEADER_ONLY=true",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ],
        stdout=header_source,
    )
    write_replacement_header(header_source, replacement_header)
    common_tail = [
        f"HEADER={replacement_header}",
        "VALIDATION_STRINGENCY=SILENT",
        "QUIET=true",
    ]
    turbo_seconds = run(
        [
            *turbo_prefix,
            command,
            *alignment_io_args(input_bam, reference_fasta),
            f"O={turbo_out}",
            *common_tail,
        ]
    )
    picard_seconds = run(
        [
            *picard_prefix,
            command,
            *alignment_io_args(input_bam, reference_fasta),
            f"O={picard_out}",
            *common_tail,
        ]
    )
    write_comparison_sams(
        turbo_out,
        picard_out,
        turbo_sam,
        picard_sam,
        turbo_prefix,
        picard_prefix,
        reference_fasta,
    )
    turbo_digest = digest_replace_sam_header(turbo_sam)
    picard_digest = digest_replace_sam_header(picard_sam)
    return evidence(
        command,
        turbo_seconds,
        picard_seconds,
        "replacement header lines and record order digest",
        turbo_out,
        picard_out,
        turbo_digest,
        picard_digest,
    )


def write_replacement_header(source: Path, destination: Path) -> None:
    lines: list[str] = []
    with source.open(encoding="utf-8") as handle:
        for line in handle:
            if line.startswith("@HD"):
                lines.append("@HD\tVN:1.6\tSO:coordinate\n")
            elif line.startswith("@SQ"):
                lines.append(line)
            elif line.startswith("@RG"):
                continue
            elif line.startswith("@"):
                continue
            else:
                break
    lines.append("@CO\tturbo-picard real-data ReplaceSamHeader parity header\n")
    destination.write_text("".join(lines), encoding="utf-8")


def digest_replace_sam_header(path: Path) -> str:
    digest = hashlib.sha256()
    header_lines: list[bytes] = []
    record_names: list[bytes] = []
    with path.open("rb") as handle:
        for raw in handle:
            raw = raw.rstrip(b"\n")
            if raw.startswith(b"@"):
                header_lines.append(raw)
            elif raw:
                record_names.append(raw.split(b"\t", 1)[0])
    for row in header_lines:
        digest.update(row)
        digest.update(b"\n")
    for row in record_names:
        digest.update(row)
        digest.update(b"\n")
    return digest.hexdigest()


def compare_sortsam(
    input_bam: Path,
    workdir: Path,
    turbo_prefix: list[str],
    picard_prefix: list[str],
    reference_fasta: Path | None,
) -> CommandEvidence:
    command = "SortSam"
    turbo_bam = output_container_path(workdir, "turbo.", input_bam)
    picard_bam = output_container_path(workdir, "picard.", input_bam)
    turbo_sam = workdir / "turbo.view.sam"
    picard_sam = workdir / "picard.view.sam"
    common = [
        command,
        *alignment_io_args(input_bam, reference_fasta),
        "SORT_ORDER=coordinate",
        "VALIDATION_STRINGENCY=SILENT",
        "QUIET=true",
    ]
    if input_bam.suffix.lower() != ".cram":
        common.insert(3, "CREATE_INDEX=true")
    turbo_seconds = run([*turbo_prefix, *common, f"O={turbo_bam}"])
    picard_seconds = run([*picard_prefix, *common, f"O={picard_bam}"])
    write_comparison_sams(
        turbo_bam,
        picard_bam,
        turbo_sam,
        picard_sam,
        turbo_prefix,
        picard_prefix,
        reference_fasta,
    )
    turbo_digest = digest_coordinate_sorted_sam_multiset(turbo_sam)
    picard_digest = digest_coordinate_sorted_sam_multiset(picard_sam)
    return evidence(
        command,
        turbo_seconds,
        picard_seconds,
        "coordinate-sorted SAM record multiset digest",
        turbo_bam,
        picard_bam,
        turbo_digest,
        picard_digest,
    )


def compare_validate_sam_file(
    input_bam: Path,
    workdir: Path,
    turbo_prefix: list[str],
    picard_prefix: list[str],
    reference_fasta: Path | None,
) -> CommandEvidence:
    command = "ValidateSamFile"
    turbo_out = workdir / "turbo.summary.txt"
    picard_out = workdir / "picard.summary.txt"
    common = [
        command,
        *alignment_io_args(input_bam, reference_fasta),
        "MODE=SUMMARY",
        "VALIDATION_STRINGENCY=SILENT",
        "QUIET=true",
    ]
    turbo_seconds, turbo_exit = run_allowing_exit([*turbo_prefix, *common, f"O={turbo_out}"])
    picard_seconds, picard_exit = run_allowing_exit([*picard_prefix, *common, f"O={picard_out}"])
    turbo_digest = digest_validate_sam_summary(turbo_out, turbo_exit)
    picard_digest = digest_validate_sam_summary(picard_out, picard_exit)
    return evidence(
        command,
        turbo_seconds,
        picard_seconds,
        "summary validation histogram plus exit code",
        turbo_out,
        picard_out,
        turbo_digest,
        picard_digest,
        turbo_exit,
        picard_exit,
    )


def evidence(
    command: str,
    turbo_seconds: float,
    picard_seconds: float,
    comparison: str,
    turbo_artifact: Path,
    picard_artifact: Path,
    turbo_digest: str,
    picard_digest: str,
    turbo_exit_code: int | None = None,
    picard_exit_code: int | None = None,
) -> CommandEvidence:
    return CommandEvidence(
        command=command,
        status="PASS" if turbo_digest == picard_digest else "FAIL",
        turbo_seconds=turbo_seconds,
        picard_seconds=picard_seconds,
        speedup=picard_seconds / turbo_seconds if turbo_seconds > 0 else None,
        comparison=comparison,
        turbo_artifact=str(turbo_artifact),
        picard_artifact=str(picard_artifact),
        turbo_digest=turbo_digest,
        picard_digest=picard_digest,
        turbo_exit_code=turbo_exit_code,
        picard_exit_code=picard_exit_code,
    )


def command_evidence_dict(row: CommandEvidence) -> dict:
    data = asdict(row)
    if data.get("turbo_artifact"):
        data["turbo_artifact"] = relative_to_root(Path(data["turbo_artifact"]))
    if data.get("picard_artifact"):
        data["picard_artifact"] = relative_to_root(Path(data["picard_artifact"]))
    return {key: value for key, value in data.items() if value is not None}


def write_fake_rscript(path: Path) -> None:
    path.write_text("#!/usr/bin/env sh\nexit 0\n", encoding="utf-8")
    path.chmod(0o755)


def rscript_shim_env(workdir: Path) -> dict[str, str]:
    env = os.environ.copy()
    env["PATH"] = f"{workdir}{os.pathsep}{env.get('PATH', '')}"
    return env


def picard_prefix_with_rscript_shim(prefix: list[str], workdir: Path) -> list[str]:
    if len(prefix) >= 3 and Path(prefix[0]).name in {"mamba", "micromamba"}:
        try:
            run_index = prefix.index("run")
        except ValueError:
            run_index = -1
        if run_index >= 0:
            path_parts = [str(workdir)]
            for flag in ("-p", "--prefix"):
                if flag in prefix:
                    prefix_index = prefix.index(flag)
                    if prefix_index + 1 < len(prefix):
                        path_parts.append(str(Path(prefix[prefix_index + 1]) / "bin"))
                    break
            path_parts.append(os.environ.get("PATH", ""))
            return [
                *prefix[:-1],
                "env",
                f"PATH={os.pathsep.join(path_parts)}",
                prefix[-1],
            ]
    return prefix


def native_evaluation_env(env: dict[str, str] | None = None) -> dict[str, str]:
    """Never mistake delegation to Java for evidence of native execution.

    Also disable both legacy fallback paths so older candidate binaries cannot
    silently delegate. Explicitly launched upstream Picard remains unaffected.
    Copy the environment; never mutate the caller or global process settings.
    """
    result = dict(os.environ if env is None else env)
    result.pop("TURBO_PICARD_FALLBACK_COMMAND", None)
    result["TURBO_PICARD_DISABLE_AUTO_FALLBACK"] = "1"
    result["TURBO_PICARD_REQUIRE_NATIVE"] = "1"
    return result


def run(
    command: list[str],
    *,
    stdout: Path | None = None,
    env: dict[str, str] | None = None,
) -> float:
    env = native_evaluation_env(env)
    start = time.perf_counter()
    with tempfile.TemporaryFile("w+b") as stderr_handle:
        if stdout is None:
            completed = subprocess.run(
                command,
                cwd=ROOT,
                stdout=subprocess.DEVNULL,
                stderr=stderr_handle,
                env=env,
                check=False,
            )
        else:
            with stdout.open("wb") as stdout_handle:
                completed = subprocess.run(
                    command,
                    cwd=ROOT,
                    stdout=stdout_handle,
                    stderr=stderr_handle,
                    env=env,
                    check=False,
                )
        stderr_handle.seek(0)
        stderr = stderr_handle.read().decode("utf-8", errors="replace")
    elapsed = time.perf_counter() - start
    if completed.returncode != 0:
        sys.stderr.write(stderr)
        raise SystemExit(f"command failed with exit {completed.returncode}: {' '.join(command)}")
    return elapsed


def run_allowing_exit(
    command: list[str],
    *,
    stdout: Path | None = None,
    env: dict[str, str] | None = None,
) -> tuple[float, int]:
    env = native_evaluation_env(env)
    start = time.perf_counter()
    with tempfile.TemporaryFile("w+b") as stderr_handle:
        if stdout is None:
            completed = subprocess.run(
                command,
                cwd=ROOT,
                stdout=subprocess.DEVNULL,
                stderr=stderr_handle,
                env=env,
                check=False,
            )
        else:
            with stdout.open("wb") as stdout_handle:
                completed = subprocess.run(
                    command,
                    cwd=ROOT,
                    stdout=stdout_handle,
                    stderr=stderr_handle,
                    env=env,
                    check=False,
                )
    return time.perf_counter() - start, completed.returncode


def capture_version(command: list[str]) -> str:
    completed = subprocess.run(
        command,
        cwd=ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        check=False,
    )
    lines = [line.strip() for line in completed.stdout.splitlines() if line.strip()]
    for line in lines:
        if re.fullmatch(r"Version:\s*\S+", line):
            return line
    text = " ".join(lines)
    if text.startswith("Version:"):
        return text
    if completed.returncode != 0:
        return f"unknown (version command exited {completed.returncode})"
    return text or "unknown"


def input_metadata(
    path: Path,
    source_url: str | None = None,
    source_commit: str | None = None,
    reference_fasta: Path | None = None,
) -> dict:
    stat = path.stat()
    metadata = {
        "path": relative_to_root(path),
        "size_bytes": stat.st_size,
        "sha256": digest_file(path),
    }
    if path.suffix.lower() == ".cram":
        metadata["format"] = "CRAM"
    elif path.suffix.lower() == ".bam":
        metadata["format"] = "BAM"
    if source_url:
        metadata["source_url"] = source_url
    if source_commit:
        metadata["source_commit"] = source_commit
    if reference_fasta is not None:
        metadata["reference_fasta"] = relative_to_root(reference_fasta)
        metadata["reference_sha256"] = digest_file(reference_fasta)
    return metadata


def collecthsmetrics_input_metadata(
    bait_interval_list: Path | None,
    target_interval_list: Path | None,
) -> dict[str, dict[str, str | int]]:
    if bait_interval_list is None or target_interval_list is None:
        raise SystemExit(
            "CollectHsMetrics evidence requires bait and target interval-list metadata"
        )
    return {
        "bait_interval_list": {
            "path": relative_to_root(bait_interval_list),
            "size_bytes": bait_interval_list.stat().st_size,
            "sha256": digest_file(bait_interval_list),
        },
        "target_interval_list": {
            "path": relative_to_root(target_interval_list),
            "size_bytes": target_interval_list.stat().st_size,
            "sha256": digest_file(target_interval_list),
        },
    }


def relative_to_root(path: Path) -> str:
    try:
        return str(path.resolve().relative_to(ROOT.resolve()))
    except ValueError:
        return str(path)


def require_manifest_path(label: str, path: Path) -> str:
    relative = relative_to_root(path)
    parts = Path(relative).parts
    if Path(relative).is_absolute() or ".." in parts:
        raise SystemExit(f"{label} must be repository-relative under benchmarks/real-data: {relative}")
    try:
        Path(relative).relative_to("benchmarks/real-data")
    except ValueError:
        raise SystemExit(f"{label} must be under benchmarks/real-data: {relative}")
    return relative


def validate_source_citation(dataset_id: str, source_url: str, source_commit: str) -> None:
    parsed = urlparse(source_url)
    if parsed.scheme != "https" or not parsed.netloc:
        raise SystemExit(f"{dataset_id} source_url must be an https URL")
    if source_commit in {"develop", "main", "master"}:
        raise SystemExit(f"{dataset_id} source_commit is not pinned")
    if not source_commit or len(source_commit) < 3:
        raise SystemExit(f"{dataset_id} source_commit is too short to identify a source")
    if parsed.netloc == "raw.githubusercontent.com":
        raise SystemExit(
            f"{dataset_id} source_url must not use raw.githubusercontent.com moving branch URLs"
        )
    if parsed.netloc == "github.com":
        if not re.fullmatch(r"[0-9a-f]{40}", source_commit):
            raise SystemExit(
                f"{dataset_id} GitHub source_commit must be a full 40-character SHA"
            )
        marker = f"/blob/{source_commit}/"
        if marker not in parsed.path:
            raise SystemExit(f"{dataset_id} GitHub source_url must include {marker}")
    elif source_commit not in source_url:
        raise SystemExit(
            f"{dataset_id} non-GitHub source_url must include source_commit/accession identifier"
        )


def build_manifest_entry(
    *,
    summary: dict,
    dataset_id: str,
    evidence_json: Path,
    evidence_markdown: Path,
    scope_caveat: str,
    release_tier: str,
) -> dict:
    if summary.get("parity") != "PASS":
        raise SystemExit("refusing to write manifest entry for failing comparison")
    for label, path, expected_name in (
        ("evidence JSON", evidence_json, "real-data-comparison.json"),
        ("evidence Markdown", evidence_markdown, "real-data-comparison.md"),
    ):
        if path.parent.name != "evidence":
            raise SystemExit(
                f"{label} must be written under a dataset evidence/ directory: {path}"
            )
        if path.name != expected_name:
            raise SystemExit(f"{label} must be named {path.parent / expected_name}")
    input_summary = summary["input"]
    missing = [
        key
        for key in ("source_url", "source_commit")
        if key not in input_summary or not input_summary[key]
    ]
    if missing:
        raise SystemExit(
            "manifest entries require input citation fields: "
            + ", ".join(missing)
            + " (pass --input-source-url and --input-source-commit)"
        )
    validate_source_citation(
        dataset_id,
        str(input_summary["source_url"]),
        str(input_summary["source_commit"]),
    )
    expected_commands: dict[str, str] = {}
    seen_commands: set[str] = set()
    command_rows = summary.get("commands", [])
    if not isinstance(command_rows, list):
        raise SystemExit("comparison summary commands must be a list")
    for index, row in enumerate(command_rows):
        if not isinstance(row, dict):
            raise SystemExit(f"comparison summary command row {index} must be an object")
        command = row.get("command")
        if not isinstance(command, str) or not command:
            raise SystemExit(f"comparison summary command row {index} missing command")
        if command in seen_commands:
            raise SystemExit(f"comparison summary has duplicate command evidence: {command}")
        seen_commands.add(command)
        if row.get("status") == "PASS":
            comparison = row.get("comparison")
            if not isinstance(comparison, str) or not comparison:
                raise SystemExit(f"comparison summary command {command} missing comparison")
            expected_commands[command] = comparison
    if release_tier == "release_candidate":
        required_commands = RELEASE_CANDIDATE_REQUIRED_COMMANDS
        if input_summary.get("format") == "CRAM":
            required_commands = CRAM_RELEASE_CANDIDATE_REQUIRED_COMMANDS
        missing_commands = sorted(required_commands - expected_commands.keys())
        if missing_commands:
            raise SystemExit(
                "release_candidate manifest entries require passing evidence for: "
                + ", ".join(missing_commands)
            )
        size_bytes = int(input_summary.get("size_bytes", 0))
        if input_summary.get("format") == "CRAM":
            if size_bytes < CRAM_RELEASE_CANDIDATE_MIN_BYTES:
                raise SystemExit(
                    "release_candidate CRAM manifest entries require input size >= "
                    f"{CRAM_RELEASE_CANDIDATE_MIN_BYTES} bytes; got {size_bytes}"
                )
            minimum_input_bytes = size_bytes
        else:
            if size_bytes < RELEASE_CANDIDATE_MIN_BYTES:
                raise SystemExit(
                    "release_candidate manifest entries require input size >= "
                    f"{RELEASE_CANDIDATE_MIN_BYTES} bytes; got {size_bytes}"
                )
            minimum_input_bytes = RELEASE_CANDIDATE_MIN_BYTES
    else:
        minimum_input_bytes = None
    entry = {
        "id": dataset_id,
        "description": scope_caveat,
        "input_path": require_manifest_path("input path", Path(input_summary["path"])),
        "evidence_json": require_manifest_path("evidence JSON", evidence_json),
        "evidence_markdown": require_manifest_path("evidence Markdown", evidence_markdown),
        "source_url": input_summary["source_url"],
        "source_commit": input_summary["source_commit"],
        "sha256": input_summary["sha256"],
        "scope_caveat": scope_caveat,
        "release_tier": release_tier,
        "expected_commands": expected_commands,
    }
    if minimum_input_bytes is not None:
        entry["minimum_input_bytes"] = minimum_input_bytes
    return entry


def digest_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def digest_files(paths: Iterable[Path]) -> str:
    digest = hashlib.sha256()
    for path in paths:
        with path.open("rb") as handle:
            for chunk in iter(lambda: handle.read(1024 * 1024), b""):
                digest.update(chunk)
        digest.update(b"\0")
    return digest.hexdigest()


def digest_sam_records(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for raw in handle:
            if raw.startswith(b"@"):
                continue
            digest.update(normalize_sam_record(raw.rstrip(b"\n")))
            digest.update(b"\n")
    return digest.hexdigest()


def digest_sam_records_and_read_groups(path: Path) -> str:
    # Two streaming passes preserve the existing digest contract (all read
    # groups before all records) without retaining every alignment in memory.
    digest = hashlib.sha256()
    read_groups: list[bytes] = []
    with path.open("rb") as handle:
        for raw in handle:
            if raw.startswith(b"@RG\t"):
                read_groups.append(normalize_sam_header_fields(raw.rstrip(b"\n")))
    for row in sorted(read_groups):
        digest.update(b"RG\t")
        digest.update(row)
        digest.update(b"\n")
    with path.open("rb") as handle:
        for raw in handle:
            if not raw.startswith(b"@"):
                digest.update(b"REC\t")
                digest.update(normalize_sam_record(raw.rstrip(b"\n")))
                digest.update(b"\n")
    return digest.hexdigest()


def normalize_sam_header_fields(row: bytes) -> bytes:
    fields = row.split(b"\t")
    if len(fields) <= 1:
        return row
    return b"\t".join([fields[0], *sorted(fields[1:])])


def digest_coordinate_sorted_sam_multiset(path: Path) -> str:
    def records() -> Iterable[bytes]:
        contig_order: dict[bytes, int] = {}
        previous: tuple[int, int] | None = None
        with path.open("rb") as handle:
            for raw in handle:
                if raw.startswith(b"@SQ\t"):
                    for field in raw.rstrip(b"\n").split(b"\t"):
                        if field.startswith(b"SN:"):
                            contig_order.setdefault(field.removeprefix(b"SN:"), len(contig_order))
                            break
                    continue
                if raw.startswith(b"@"):
                    continue
                normalized = normalize_sam_record(raw.rstrip(b"\n"))
                fields = normalized.split(b"\t")
                if len(fields) < 4:
                    raise SystemExit(f"malformed SAM record in {path}")
                tid = 1_000_000_000 if fields[2] == b"*" else contig_order.get(fields[2])
                if tid is None:
                    raise SystemExit(f"SAM record references contig missing from header in {path}: {fields[2]!r}")
                try:
                    pos = int(fields[3])
                except ValueError:
                    raise SystemExit(f"malformed SAM position in {path}: {fields[3]!r}")
                current = (tid, pos)
                if previous is not None and current < previous:
                    raise SystemExit(f"{path} is not coordinate sorted")
                previous = current
                yield normalized

    digest = hashlib.sha256()
    with sorted_records(records()) as ordered:
        for record in ordered:
            digest.update(record)
            digest.update(b"\n")
    return digest.hexdigest()


def normalize_sam_record(raw: bytes) -> bytes:
    fields = raw.split(b"\t")
    if len(fields) <= 11:
        return raw
    tags = [
        normalize_sam_tag(tag)
        for tag in fields[11:]
        if not tag.startswith(b"PG:Z:")
    ]
    return b"\t".join([*fields[:11], *sorted(tags)])


def normalize_sam_tag(tag: bytes) -> bytes:
    parts = tag.split(b":", 2)
    if len(parts) != 3 or parts[1] != b"f":
        return tag
    try:
        value = decimal.Decimal(parts[2].decode("ascii"))
    except (decimal.InvalidOperation, UnicodeDecodeError):
        return tag
    return b":".join([parts[0], parts[1], format(value.normalize(), "f").encode("ascii")])


def digest_stable_sam(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for raw in handle:
            stripped = raw.strip()
            if not stripped or stripped.startswith(b"@PG"):
                continue
            if stripped.startswith(b"@"):
                stripped = normalize_stable_sam_header(stripped)
            else:
                stripped = normalize_sam_record(stripped)
            digest.update(stripped)
            digest.update(b"\n")
    return digest.hexdigest()


def normalize_stable_sam_header(row: bytes) -> bytes:
    fields = row.split(b"\t")
    if not fields:
        return row
    normalized_fields = [
        normalize_stable_sam_header_field(field)
        for field in fields[1:]
        if not (fields[0] == b"@HD" and field.startswith(b"VN:"))
    ]
    return b"\t".join([fields[0], *normalized_fields])


def normalize_stable_sam_header_field(field: bytes) -> bytes:
    if not field.startswith(b"DT:"):
        return field
    value = field[3:].decode("ascii", "ignore")
    for fmt in ("%Y-%m-%dT%H:%M:%S%z", "%Y-%m-%d"):
        try:
            parsed = dt.datetime.strptime(value, fmt)
        except ValueError:
            continue
        if parsed.tzinfo is None:
            return field
        utc_value = parsed.astimezone(dt.timezone.utc).strftime("%Y-%m-%dT%H:%M:%S+0000")
        return b"DT:" + utc_value.encode("ascii")
    return field


def digest_stable_text(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for raw in handle:
            stripped = raw.strip()
            if not stripped or stripped.startswith(b"#"):
                continue
            digest.update(stripped)
            digest.update(b"\n")
    return digest.hexdigest()


def digest_stable_text_or_missing(path: Path, label: str) -> str:
    if not path.exists():
        return f"missing:{label}:{path.name}"
    return digest_stable_text(path)


def digest_hsmetrics_outputs(
    metrics: Path,
    per_target: Path,
    per_base: Path,
    tool_label: str,
) -> str:
    """Digest the stable metrics table and exact coverage sidecars."""

    def exact_or_missing(path: Path, label: str) -> str:
        if not path.exists():
            return f"missing:{label}:{path.name}"
        return digest_file(path)

    return ";".join(
        [
            "metrics="
            + digest_stable_text_or_missing(
                metrics,
                f"{tool_label} CollectHsMetrics metrics",
            ),
            "per-target="
            + exact_or_missing(per_target, f"{tool_label} per-target coverage"),
            "per-base="
            + exact_or_missing(per_base, f"{tool_label} per-base coverage"),
        ]
    )


def digest_validate_sam_summary(path: Path, exit_code: int) -> str:
    digest = hashlib.sha256()
    digest.update(f"exit={exit_code}\n".encode("ascii"))
    if path.exists():
        with path.open("rb") as handle:
            for raw in handle:
                stripped = raw.strip()
                if not stripped or stripped.startswith(b"#"):
                    continue
                digest.update(stripped)
                digest.update(b"\n")
    else:
        digest.update(f"missing:{path.name}\n".encode("utf-8"))
    return digest.hexdigest()


def markduplicates_sort_key(encoded: bytes) -> tuple:
    record = json.loads(encoded)
    # Preserve dataclass field ordering. A missing tag sorts before a present
    # one, defining cases in which Python's previous None-vs-value sort raised
    # TypeError. Digests for previously sortable inputs remain unchanged.
    return tuple((record[name] is not None, record[name])
                 for name in MarkDuplicateRecord.__dataclass_fields__)


def digest_markduplicates_semantics(path: Path) -> str:
    digest = hashlib.sha256()
    rows = (json.dumps(asdict(record), sort_keys=True).encode("utf-8")
            for record in parse_markduplicates_records(path))
    with sorted_records(rows, key=markduplicates_sort_key) as ordered:
        for row in ordered:
            digest.update(row)
            digest.update(b"\n")
    return digest.hexdigest()


def parse_markduplicates_records(path: Path) -> Iterable[MarkDuplicateRecord]:
    with path.open("r", encoding="utf-8") as handle:
        for raw in handle:
            if not raw.strip() or raw.startswith("@"):
                continue
            fields = raw.rstrip("\n").split("\t")
            if len(fields) < 11:
                raise ValueError(f"malformed SAM record in {path}: {raw.rstrip()}")
            flag = int(fields[1])
            tags = fields[11:]
            yield MarkDuplicateRecord(
                query_name=fields[0],
                duplicate=bool(flag & 0x400),
                duplicate_type=optional_tag(tags, "DT"),
                duplicate_set_size=optional_int_tag(tags, "DS"),
                duplicate_set_index=optional_int_tag(tags, "DI"),
                rx_barcode=optional_tag(tags, "RX"),
                bx_barcode=optional_tag(tags, "BX"),
                by_barcode=optional_tag(tags, "BY"),
                reference_name=fields[2],
                position=int(fields[3]),
                mate_reference_name=fields[6],
                mate_position=int(fields[7]),
                cigar=fields[5],
                template_length=int(fields[8]),
            )


def optional_tag(fields: list[str], tag: str) -> str | None:
    prefix = f"{tag}:Z:"
    for field in fields:
        if field.startswith(prefix):
            return field.removeprefix(prefix)
    return None


def optional_int_tag(fields: list[str], tag: str) -> int | None:
    prefix = f"{tag}:i:"
    for field in fields:
        if field.startswith(prefix):
            return int(field.removeprefix(prefix))
    return None


def write_markdown(path: Path, summary: dict) -> None:
    lines = [
        "# turbo-picard real-data comparison",
        "",
        f"Input BAM: `{summary['input']['path']}`",
        f"Input SHA-256: `{summary['input']['sha256']}`",
        f"Input size: `{summary['input']['size_bytes']}` bytes",
        *optional_input_source_lines(summary["input"]),
        f"Picard: `{summary['picard_version']}`",
        f"turbo-picard: `{summary['turbo_picard_version']}`",
        "",
        "| Command | Status | Comparison | turbo-picard | Picard | Speedup |",
        "| --- | --- | --- | ---: | ---: | ---: |",
    ]
    for row in summary["commands"]:
        speedup = f"{row['speedup']:.2f}x" if row["speedup"] is not None else "n/a"
        lines.append(
            f"| {row['command']} | {row['status']} | {row['comparison']} | "
            f"{row['turbo_seconds']:.3f}s | {row['picard_seconds']:.3f}s | {speedup} |"
        )
    lines.extend(
        [
            "",
            "A PASS means the command-specific stable digest matched Picard on this input. "
            "Keep the JSON file with the raw digests when sharing results.",
            "",
            "## Comparison details",
            "",
        ]
    )
    lines.extend(comparison_detail_lines(summary["commands"]))
    lines.extend(["", "## Artifact digests", ""])
    lines.extend(artifact_digest_lines(summary["commands"]))
    lines.append("")
    write_new_text(path, "\n".join(lines))


def markdown_cell(value: object) -> str:
    """Return a single safe Markdown table cell for tool-produced text."""

    return str(value).replace("`", "'").replace("|", "\\|").replace("\n", " ").strip()


def human_size(size_bytes: object) -> str:
    """Render an input size without exposing its path or content hash."""

    try:
        size = int(size_bytes)
    except (TypeError, ValueError):
        return "unknown size"
    if size < 1024:
        return f"{size} bytes"
    value = float(size)
    for unit in ("KiB", "MiB", "GiB", "TiB"):
        value /= 1024.0
        if value < 1024.0 or unit == "TiB":
            return f"{value:.1f} {unit}"
    return "unknown size"


def write_shareable_markdown(
    path: Path,
    summary: dict,
    *,
    include_public_source: bool = False,
) -> None:
    """Write a privacy-conscious summary suitable for a public trial issue.

    The full comparison Markdown remains the audit artifact. This report is a
    separate, intentionally lossy view: it keeps the evidence a workflow owner
    needs to describe a trial while excluding local paths, hashes, command
    arguments, generated artifact names, and raw output.
    """

    input_summary = summary.get("input", {})
    input_format = input_summary.get("format", "alignment input")
    lines = [
        "# turbo-picard trial report",
        "",
        "> Review this summary before posting. It intentionally omits local paths, "
        "input hashes, command arguments, generated artifacts, and raw data.",
        "",
        f"- Overall parity: `{markdown_cell(summary.get('parity', 'UNKNOWN'))}`",
        f"- turbo-picard: `{markdown_cell(summary.get('turbo_picard_version', 'unknown'))}`",
        f"- Picard: `{markdown_cell(summary.get('picard_version', 'unknown'))}`",
        f"- Input shape: `{markdown_cell(input_format)}`, about {human_size(input_summary.get('size_bytes'))}",
    ]
    if include_public_source:
        source_url = input_summary.get("source_url")
        source_commit = input_summary.get("source_commit")
        if source_url:
            lines.append(f"- Public source URL: `{markdown_cell(source_url)}`")
        if source_commit:
            lines.append(f"- Public source revision: `{markdown_cell(source_commit)}`")
    lines.extend(
        [
            "",
            "| Command | Status | Comparison | turbo-picard | Picard | Speedup |",
            "| --- | --- | --- | ---: | ---: | ---: |",
        ]
    )
    for row in summary.get("commands", []):
        speedup = row.get("speedup")
        speedup_text = "n/a" if speedup is None else f"{float(speedup):.2f}x"
        lines.append(
            f"| {markdown_cell(row.get('command', 'unknown'))} | "
            f"{markdown_cell(row.get('status', 'UNKNOWN'))} | "
            f"{markdown_cell(row.get('comparison', 'not recorded'))} | "
            f"{float(row.get('turbo_seconds', 0.0)):.3f}s | "
            f"{float(row.get('picard_seconds', 0.0)):.3f}s | {speedup_text} |"
        )
    lines.extend(
        [
            "",
            "A PASS means the command-specific comparison digest matched Picard "
            "on this input. This is a command-level trial, not approval for a "
            "whole cohort or production workflow.",
            "",
            "If the trial did not match, report the command, input shape, "
            "comparison result, and next blocker without attaching private data.",
            "",
        ]
    )
    write_new_text(path, "\n".join(lines))


def comparison_detail_lines(rows: list[dict]) -> list[str]:
    comparisons = {row.get("comparison") for row in rows}
    details: list[str] = []
    if "SAM record digest" in comparisons:
        details.append(
            "- `SAM record digest` compares normalized SAM records and ignores headers."
        )
    if "post-command SAM record digest" in comparisons:
        details.append(
            "- `post-command SAM record digest` compares normalized SAM records after a BAM-writing command."
        )
    if "reverted SAM record digest" in comparisons:
        details.append(
            "- `reverted SAM record digest` compares normalized SAM records after RevertSam rewrites aligned records to unmapped output."
        )
    if "FASTQ trio digest" in comparisons:
        details.append(
            "- `FASTQ trio digest` compares SamToFastq first-end, second-end, and unpaired FASTQ outputs byte-for-byte."
        )
    if "SAM record digest plus read-group header digest" in comparisons:
        details.append(
            "- `SAM record digest plus read-group header digest` compares normalized SAM records and sorted @RG header fields after AddOrReplaceReadGroups."
        )
    if "coordinate-sorted SAM record multiset digest" in comparisons:
        details.append(
            "- `coordinate-sorted SAM record multiset digest` verifies coordinate sorting while allowing tie-order differences at the same position."
        )
    if "BAI binary digest" in comparisons:
        details.append(
            "- `BAI binary digest` compares the exact BAM index bytes produced by BuildBamIndex."
        )
    if any(
        isinstance(comparison, str) and comparison.startswith("stable metrics digest")
        for comparison in comparisons
    ):
        details.append(
            "- `stable metrics digest` compares non-comment, non-blank metrics rows so generated headers do not affect parity."
        )
    if "stable HsMetrics digest plus per-target/per-base sidecar digests" in comparisons:
        details.append(
            "- `stable HsMetrics digest plus per-target/per-base sidecar digests` compares the non-comment HsMetrics tables and histogram, then requires exact per-target and per-base coverage sidecar bytes."
        )
    if "duplicate-marking semantic digest plus stable metrics digest" in comparisons:
        details.append(
            "- `duplicate-marking semantic digest plus stable metrics digest` compares duplicate flags, duplicate tags, duplicate-set metadata, barcode tags, key coordinates, and duplicate metrics."
        )
    if "summary validation histogram plus exit code" in comparisons:
        details.append(
            "- `summary validation histogram plus exit code` compares the ValidateSamFile summary histogram and requires the same Picard and turbo-picard exit code."
        )
    return details


def artifact_digest_lines(rows: list[dict]) -> list[str]:
    lines = [
        "| Command | turbo-picard artifact | Picard artifact | Digest | Exit codes |",
        "| --- | --- | --- | --- | --- |",
    ]
    for row in rows:
        turbo_digest = str(row.get("turbo_digest", ""))
        picard_digest = str(row.get("picard_digest", ""))
        digest = digest_summary(turbo_digest, picard_digest)
        exit_codes = exit_code_summary(row)
        lines.append(
            f"| {row.get('command', '')} | `{row.get('turbo_artifact', '')}` | "
            f"`{row.get('picard_artifact', '')}` | `{digest}` | {exit_codes} |"
        )
    return lines


def exit_code_summary(row: dict) -> str:
    turbo_exit = row.get("turbo_exit_code")
    picard_exit = row.get("picard_exit_code")
    if isinstance(turbo_exit, int) and isinstance(picard_exit, int):
        return f"turbo-picard `{turbo_exit}`, Picard `{picard_exit}`"
    return "n/a"


def digest_summary(turbo_digest: str, picard_digest: str) -> str:
    if turbo_digest != picard_digest:
        return "mismatch"
    if len(turbo_digest) <= 32:
        return turbo_digest
    return f"{turbo_digest[:12]}...{turbo_digest[-12:]}"


def optional_input_source_lines(input_summary: dict) -> list[str]:
    lines = []
    if "source_url" in input_summary:
        lines.append(f"Input source: `{input_summary['source_url']}`")
    if "source_commit" in input_summary:
        lines.append(f"Input source commit: `{input_summary['source_commit']}`")
    return lines


if __name__ == "__main__":
    raise SystemExit(main())
