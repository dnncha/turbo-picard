#!/usr/bin/env python3
"""Run Picard-vs-turbo-picard comparisons on a real BAM.

This is intentionally separate from the fast synthetic CI parity scripts. It is
for public benchmark samples such as GIAB/NA12878 or for a lab's own
representative production BAMs, where the useful output is a durable evidence
bundle rather than a tiny unit-test fixture.
"""

from __future__ import annotations

import argparse
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


ROOT = Path(__file__).resolve().parents[1]
RELEASE_CANDIDATE_MIN_BYTES = 1_000_000
RELEASE_CANDIDATE_REQUIRED_COMMANDS = {
    "ViewSam",
    "CleanSam",
    "CollectQualityYieldMetrics",
    "CollectAlignmentSummaryMetrics",
    "MarkDuplicates",
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
    parser.add_argument("--input-bam", required=True, type=Path)
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
            "MarkDuplicates",
            "AddOrReplaceReadGroups",
            "BuildBamIndex",
            "SortSam",
            "CollectInsertSizeMetrics",
            "ValidateSamFile",
            "RevertSam",
            "SamToFastq",
        ],
        help="Commands to compare on the real BAM.",
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
        help="Remove intermediate command outputs after writing JSON/Markdown digests.",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if not args.input_bam.exists():
        raise SystemExit(f"missing input BAM: {args.input_bam}")
    if args.stop_after is not None and args.stop_after < 1:
        raise SystemExit("--stop-after must be positive")

    args.output_dir.mkdir(parents=True, exist_ok=True)
    if not args.skip_build:
        run(["cargo", "build", "--release", "-p", "turbo-picard-cli", "--bin", "picard"])

    turbo_prefix = split_command(args.turbo_picard_command)
    if not Path(turbo_prefix[0]).exists() and shutil.which(turbo_prefix[0]) is None:
        raise SystemExit(f"missing turbo-picard command: {turbo_prefix[0]}")
    picard_prefix = split_command(args.picard_command) if args.picard_command else default_picard_prefix(args.conda_prefix)

    work_root = args.output_dir / "work"
    if work_root.exists():
        shutil.rmtree(work_root)
    work_root.mkdir(parents=True)

    evidence: list[CommandEvidence] = []
    try:
        for command in args.commands:
            evidence.append(compare_command(command, args.input_bam, work_root, turbo_prefix, picard_prefix, args.stop_after))
    finally:
        if args.discard_work:
            shutil.rmtree(work_root, ignore_errors=True)

    summary = {
        "input": input_metadata(args.input_bam, args.input_source_url, args.input_source_commit),
        "picard_command": " ".join(picard_prefix),
        "picard_version": capture_version([*picard_prefix, "ViewSam", "--version"]),
        "turbo_picard_command": " ".join(turbo_prefix),
        "turbo_picard_version": capture_version([*turbo_prefix, "--version"]),
        "commands": [command_evidence_dict(row) for row in evidence],
        "parity": "PASS" if all(row.status == "PASS" for row in evidence) else "FAIL",
    }
    json_path = args.output_dir / "real-data-comparison.json"
    json_path.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8")
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
        manifest_entry_path.write_text(
            json.dumps(manifest_entry, indent=2, sort_keys=True) + "\n",
            encoding="utf-8",
        )

    print(f"wrote {json_path}")
    print(f"wrote {markdown_path}")
    if args.dataset_id:
        print(f"wrote {args.output_dir / 'manifest-entry.json'}")
    for row in evidence:
        speedup = f"{row.speedup:.2f}x" if row.speedup is not None else "n/a"
        print(f"{row.command}: {row.status} parity, speedup={speedup}")
    return 0 if summary["parity"] == "PASS" else 1


def split_command(command: str | None) -> list[str]:
    if not command:
        return []
    import shlex

    return shlex.split(command)


def default_picard_prefix(conda_prefix: str) -> list[str]:
    for name in ("mamba", "micromamba"):
        runner = shutil.which(name)
        if runner:
            return [runner, "run", "-p", conda_prefix, "picard"]
    raise SystemExit("mamba or micromamba is required when --picard-command is omitted")


def compare_command(
    command: str,
    input_bam: Path,
    work_root: Path,
    turbo_prefix: list[str],
    picard_prefix: list[str],
    stop_after: int | None,
) -> CommandEvidence:
    workdir = work_root / command
    workdir.mkdir(parents=True)
    if command == "ViewSam":
        return compare_viewsam(input_bam, workdir, turbo_prefix, picard_prefix)
    if command == "CleanSam":
        return compare_bam_output(command, input_bam, workdir, turbo_prefix, picard_prefix, ["CREATE_INDEX=true"])
    if command == "MarkDuplicates":
        return compare_bam_output(command, input_bam, workdir, turbo_prefix, picard_prefix, ["M={metrics}"])
    if command == "AddOrReplaceReadGroups":
        return compare_add_or_replace_read_groups(input_bam, workdir, turbo_prefix, picard_prefix)
    if command == "BuildBamIndex":
        return compare_build_bam_index(input_bam, workdir, turbo_prefix, picard_prefix)
    if command == "RevertSam":
        return compare_revertsam(input_bam, workdir, turbo_prefix, picard_prefix)
    if command == "SamToFastq":
        return compare_samtofastq(input_bam, workdir, turbo_prefix, picard_prefix)
    if command == "SortSam":
        return compare_sortsam(input_bam, workdir, turbo_prefix, picard_prefix)
    if command == "CollectInsertSizeMetrics":
        extra = [f"STOP_AFTER={stop_after}"] if stop_after is not None else []
        return compare_insert_size_metrics(input_bam, workdir, turbo_prefix, picard_prefix, extra)
    if command == "ValidateSamFile":
        return compare_validate_sam_file(input_bam, workdir, turbo_prefix, picard_prefix)
    if command in {"CollectQualityYieldMetrics", "CollectAlignmentSummaryMetrics"}:
        extra = [f"STOP_AFTER={stop_after}"] if stop_after is not None else []
        return compare_metrics(command, input_bam, workdir, turbo_prefix, picard_prefix, extra)
    raise AssertionError(command)


def compare_viewsam(
    input_bam: Path,
    workdir: Path,
    turbo_prefix: list[str],
    picard_prefix: list[str],
) -> CommandEvidence:
    turbo_out = workdir / "turbo.sam"
    picard_out = workdir / "picard.sam"
    turbo_seconds = run([*turbo_prefix, "ViewSam", f"I={input_bam}", "VALIDATION_STRINGENCY=SILENT", "QUIET=true"], stdout=turbo_out)
    picard_seconds = run([*picard_prefix, "ViewSam", f"I={input_bam}", "VALIDATION_STRINGENCY=SILENT", "QUIET=true"], stdout=picard_out)
    turbo_digest = digest_sam_records(turbo_out)
    picard_digest = digest_sam_records(picard_out)
    return evidence("ViewSam", turbo_seconds, picard_seconds, "SAM record digest", turbo_out, picard_out, turbo_digest, picard_digest)


def compare_metrics(
    command: str,
    input_bam: Path,
    workdir: Path,
    turbo_prefix: list[str],
    picard_prefix: list[str],
    extra: list[str],
) -> CommandEvidence:
    turbo_out = workdir / "turbo.metrics.txt"
    picard_out = workdir / "picard.metrics.txt"
    common = [command, f"I={input_bam}", "VALIDATION_STRINGENCY=SILENT", "QUIET=true", *extra]
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
    common = [command, f"I={input_bam}", "VALIDATION_STRINGENCY=SILENT", "QUIET=true", *extra]
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
) -> CommandEvidence:
    command = "BuildBamIndex"
    turbo_bai = workdir / "turbo.bai"
    picard_bai = workdir / "picard.bai"
    common = [
        command,
        f"I={input_bam}",
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
) -> CommandEvidence:
    command = "AddOrReplaceReadGroups"
    turbo_bam = workdir / "turbo.bam"
    picard_bam = workdir / "picard.bam"
    turbo_sam = workdir / "turbo.view.sam"
    picard_sam = workdir / "picard.view.sam"
    common = [
        command,
        f"I={input_bam}",
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
    run([*turbo_prefix, "ViewSam", f"I={turbo_bam}", "VALIDATION_STRINGENCY=SILENT", "QUIET=true"], stdout=turbo_sam)
    run([*picard_prefix, "ViewSam", f"I={picard_bam}", "VALIDATION_STRINGENCY=SILENT", "QUIET=true"], stdout=picard_sam)
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
) -> CommandEvidence:
    command = "RevertSam"
    turbo_bam = workdir / "turbo.bam"
    picard_bam = workdir / "picard.bam"
    turbo_sam = workdir / "turbo.view.sam"
    picard_sam = workdir / "picard.view.sam"
    common = [
        command,
        f"I={input_bam}",
        "VALIDATION_STRINGENCY=SILENT",
        "QUIET=true",
    ]
    turbo_seconds = run([*turbo_prefix, *common, f"O={turbo_bam}"])
    picard_seconds = run([*picard_prefix, *common, f"O={picard_bam}"])
    run([*turbo_prefix, "ViewSam", f"I={turbo_bam}", "VALIDATION_STRINGENCY=SILENT", "QUIET=true"], stdout=turbo_sam)
    run([*picard_prefix, "ViewSam", f"I={picard_bam}", "VALIDATION_STRINGENCY=SILENT", "QUIET=true"], stdout=picard_sam)
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
        f"I={input_bam}",
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
) -> CommandEvidence:
    turbo_bam = workdir / "turbo.bam"
    picard_bam = workdir / "picard.bam"
    turbo_metrics = workdir / "turbo.metrics.txt"
    picard_metrics = workdir / "picard.metrics.txt"
    turbo_sam = workdir / "turbo.view.sam"
    picard_sam = workdir / "picard.view.sam"

    turbo_extra = [value.format(metrics=turbo_metrics) for value in extra_templates]
    picard_extra = [value.format(metrics=picard_metrics) for value in extra_templates]
    common = [command, f"I={input_bam}", "VALIDATION_STRINGENCY=SILENT", "QUIET=true"]
    turbo_seconds = run([*turbo_prefix, *common, f"O={turbo_bam}", *turbo_extra])
    picard_seconds = run([*picard_prefix, *common, f"O={picard_bam}", *picard_extra])

    run([*turbo_prefix, "ViewSam", f"I={turbo_bam}", "VALIDATION_STRINGENCY=SILENT", "QUIET=true"], stdout=turbo_sam)
    run([*picard_prefix, "ViewSam", f"I={picard_bam}", "VALIDATION_STRINGENCY=SILENT", "QUIET=true"], stdout=picard_sam)
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


def compare_sortsam(
    input_bam: Path,
    workdir: Path,
    turbo_prefix: list[str],
    picard_prefix: list[str],
) -> CommandEvidence:
    command = "SortSam"
    turbo_bam = workdir / "turbo.bam"
    picard_bam = workdir / "picard.bam"
    turbo_sam = workdir / "turbo.view.sam"
    picard_sam = workdir / "picard.view.sam"
    common = [
        command,
        f"I={input_bam}",
        "SORT_ORDER=coordinate",
        "CREATE_INDEX=true",
        "VALIDATION_STRINGENCY=SILENT",
        "QUIET=true",
    ]
    turbo_seconds = run([*turbo_prefix, *common, f"O={turbo_bam}"])
    picard_seconds = run([*picard_prefix, *common, f"O={picard_bam}"])
    run([*turbo_prefix, "ViewSam", f"I={turbo_bam}", "VALIDATION_STRINGENCY=SILENT", "QUIET=true"], stdout=turbo_sam)
    run([*picard_prefix, "ViewSam", f"I={picard_bam}", "VALIDATION_STRINGENCY=SILENT", "QUIET=true"], stdout=picard_sam)
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
) -> CommandEvidence:
    command = "ValidateSamFile"
    turbo_out = workdir / "turbo.summary.txt"
    picard_out = workdir / "picard.summary.txt"
    common = [
        command,
        f"I={input_bam}",
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


def run(
    command: list[str],
    *,
    stdout: Path | None = None,
    env: dict[str, str] | None = None,
) -> float:
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
    text = " ".join(line.strip() for line in completed.stdout.splitlines() if line.strip())
    if text.startswith("Version:"):
        return text
    if completed.returncode != 0:
        return f"unknown (version command exited {completed.returncode})"
    return text or "unknown"


def input_metadata(path: Path, source_url: str | None = None, source_commit: str | None = None) -> dict:
    stat = path.stat()
    metadata = {
        "path": str(path),
        "size_bytes": stat.st_size,
        "sha256": digest_file(path),
    }
    if source_url:
        metadata["source_url"] = source_url
    if source_commit:
        metadata["source_commit"] = source_commit
    return metadata


def relative_to_root(path: Path) -> str:
    try:
        return str(path.resolve().relative_to(ROOT))
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
        missing_commands = sorted(RELEASE_CANDIDATE_REQUIRED_COMMANDS - expected_commands.keys())
        if missing_commands:
            raise SystemExit(
                "release_candidate manifest entries require passing evidence for: "
                + ", ".join(missing_commands)
            )
        size_bytes = int(input_summary.get("size_bytes", 0))
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
    digest = hashlib.sha256()
    read_groups: list[bytes] = []
    records: list[bytes] = []
    with path.open("rb") as handle:
        for raw in handle:
            raw = raw.rstrip(b"\n")
            if raw.startswith(b"@RG\t"):
                read_groups.append(normalize_sam_header_fields(raw))
            elif not raw.startswith(b"@"):
                records.append(normalize_sam_record(raw))
    for row in sorted(read_groups):
        digest.update(b"RG\t")
        digest.update(row)
        digest.update(b"\n")
    for row in records:
        digest.update(b"REC\t")
        digest.update(row)
        digest.update(b"\n")
    return digest.hexdigest()


def normalize_sam_header_fields(row: bytes) -> bytes:
    fields = row.split(b"\t")
    if len(fields) <= 1:
        return row
    return b"\t".join([fields[0], *sorted(fields[1:])])


def digest_coordinate_sorted_sam_multiset(path: Path) -> str:
    records: list[tuple[tuple[int, int], bytes]] = []
    contig_order: dict[bytes, int] = {}
    with path.open("rb") as handle:
        for raw in handle:
            if raw.startswith(b"@SQ\t"):
                fields = raw.rstrip(b"\n").split(b"\t")
                for field in fields:
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
            records.append(((tid, pos), normalized))
    sort_keys = [sort_key for sort_key, _record in records]
    if sort_keys != sorted(sort_keys):
        raise SystemExit(f"{path} is not coordinate sorted")
    digest = hashlib.sha256()
    for _sort_key, record in sorted(records, key=lambda item: item[1]):
        digest.update(record)
        digest.update(b"\n")
    return digest.hexdigest()


def normalize_sam_record(raw: bytes) -> bytes:
    fields = raw.split(b"\t")
    if len(fields) <= 11:
        return raw
    return b"\t".join([*fields[:11], *sorted(normalize_sam_tag(tag) for tag in fields[11:])])


def normalize_sam_tag(tag: bytes) -> bytes:
    parts = tag.split(b":", 2)
    if len(parts) != 3 or parts[1] != b"f":
        return tag
    try:
        value = decimal.Decimal(parts[2].decode("ascii"))
    except (decimal.InvalidOperation, UnicodeDecodeError):
        return tag
    return b":".join([parts[0], parts[1], format(value.normalize(), "f").encode("ascii")])


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


def digest_markduplicates_semantics(path: Path) -> str:
    digest = hashlib.sha256()
    for record in sorted(parse_markduplicates_records(path)):
        digest.update(json.dumps(asdict(record), sort_keys=True).encode("utf-8"))
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
    path.write_text("\n".join(lines), encoding="utf-8")


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
