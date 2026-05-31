#!/usr/bin/env python3
"""Run Picard-vs-turbo-picard comparisons on a real BAM.

This is intentionally separate from the fast synthetic CI parity scripts. It is
for public benchmark samples such as GIAB/NA12878 or for a lab's own
representative production BAMs, where the useful output is a durable evidence
bundle rather than a tiny unit-test fixture.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Iterable


ROOT = Path(__file__).resolve().parents[1]


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
        "commands": [asdict(row) for row in evidence],
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
    turbo_digest = digest_stable_text(turbo_out)
    picard_digest = digest_stable_text(picard_out)
    label = "stable metrics digest" if not extra else f"stable metrics digest ({' '.join(extra)})"
    return evidence(command, turbo_seconds, picard_seconds, label, turbo_out, picard_out, turbo_digest, picard_digest)


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


def evidence(
    command: str,
    turbo_seconds: float,
    picard_seconds: float,
    comparison: str,
    turbo_artifact: Path,
    picard_artifact: Path,
    turbo_digest: str,
    picard_digest: str,
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
    )


def run(command: list[str], *, stdout: Path | None = None) -> float:
    start = time.perf_counter()
    with tempfile.TemporaryFile("w+b") as stderr_handle:
        if stdout is None:
            completed = subprocess.run(command, cwd=ROOT, stdout=subprocess.DEVNULL, stderr=stderr_handle, check=False)
        else:
            with stdout.open("wb") as stdout_handle:
                completed = subprocess.run(command, cwd=ROOT, stdout=stdout_handle, stderr=stderr_handle, check=False)
        stderr_handle.seek(0)
        stderr = stderr_handle.read().decode("utf-8", errors="replace")
    elapsed = time.perf_counter() - start
    if completed.returncode != 0:
        sys.stderr.write(stderr)
        raise SystemExit(f"command failed with exit {completed.returncode}: {' '.join(command)}")
    return elapsed


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
    return {
        "id": dataset_id,
        "description": scope_caveat,
        "input_path": relative_to_root(Path(input_summary["path"])),
        "evidence_json": relative_to_root(evidence_json),
        "evidence_markdown": relative_to_root(evidence_markdown),
        "source_url": input_summary["source_url"],
        "source_commit": input_summary["source_commit"],
        "sha256": input_summary["sha256"],
        "scope_caveat": scope_caveat,
        "release_tier": release_tier,
        "expected_commands": {
            row["command"]: row["comparison"]
            for row in summary["commands"]
            if row["status"] == "PASS"
        },
    }


def digest_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
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


def normalize_sam_record(raw: bytes) -> bytes:
    fields = raw.split(b"\t")
    if len(fields) <= 11:
        return raw
    return b"\t".join([*fields[:11], *sorted(fields[11:])])


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
        ]
    )
    path.write_text("\n".join(lines), encoding="utf-8")


def optional_input_source_lines(input_summary: dict) -> list[str]:
    lines = []
    if "source_url" in input_summary:
        lines.append(f"Input source: `{input_summary['source_url']}`")
    if "source_commit" in input_summary:
        lines.append(f"Input source commit: `{input_summary['source_commit']}`")
    return lines


if __name__ == "__main__":
    raise SystemExit(main())
