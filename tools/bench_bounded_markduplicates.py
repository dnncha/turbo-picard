#!/usr/bin/env python3
from __future__ import annotations

import argparse
import contextlib
import hashlib
import importlib
import json
import os
import platform
import re
import shutil
import signal
import statistics
import subprocess
import threading
import time
from collections.abc import Callable, Iterator
from dataclasses import dataclass
from datetime import datetime, timezone
from decimal import Decimal, InvalidOperation
from pathlib import Path
from typing import Any, TypedDict


class Histogram(TypedDict):
    columns: list[str] | None
    rows: list[list[str]]


class BamSummary(TypedDict):
    alignment_sha256: str
    records: int
    duplicate_records: int
    sq_rg_headers: list[list[str]]


class OutputSummary(BamSummary, total=False):
    duplication_metrics: list[list[str]]
    histograms: list[Histogram]
    histogram_error: str


class Measurement(TypedDict, total=False):
    argv: list[str]
    exit_code: int
    wall_seconds: float
    time_seconds: float
    user_seconds: float
    system_seconds: float
    peak_rss_bytes: int
    case: str
    version: str
    repeat: int
    warmup: bool
    output_summary: OutputSummary
    output_bytes: int
    external_plan_reported: bool
    parity_pass: bool


@dataclass(frozen=True)
class Case:
    name: str
    shape: str
    padding: int
    family_size: int = 2
    remove: str = "none"
    paired_padding: bool = False


def cases() -> list[Case]:
    return [
        Case("coordinate-ties-unplaced", "ties", 110_000),
        Case("same-name-same-library", "same-library", 110_000),
        Case("same-name-separate-libraries", "separate-libraries", 110_000),
        Case("optical-and-barcodes", "optical", 110_000),
        Case("remove-optical", "optical", 110_000, remove="optical"),
        Case("remove-all", "ties", 110_000, remove="all"),
        Case("large-duplicate-family", "family", 80_000, family_size=20_000),
        Case("spill-600k", "ties", 600_000),
        Case("spill-1200k", "ties", 1_200_000),
        Case("paired-600k", "ties", 300_000, paired_padding=True),
        Case("paired-spill-1200k", "ties", 600_000, paired_padding=True),
    ]


def sam_lines(case: Case) -> Iterator[str]:
    yield "@HD\tVN:1.6\tSO:coordinate\n"
    reference_length = 1_000_000_000 if case.paired_padding else 100_000_000
    yield f"@SQ\tSN:chr1\tLN:{reference_length}\n"
    yield "@RG\tID:rg1\tSM:sample\tLB:lib1\tPL:ILLUMINA\n"
    if case.shape in {"same-library", "separate-libraries", "optical"}:
        library = "lib2" if case.shape == "separate-libraries" else "lib1"
        yield f"@RG\tID:rg2\tSM:sample\tLB:{library}\tPL:ILLUMINA\n"
    if case.shape == "family":
        templates: list[tuple[str, str, int, str | None]] = [
            (f"t{i:08}", "rg1", 40 if i == 0 else 20, None) for i in range(case.family_size)
        ]
    elif case.shape == "optical":
        templates = [
            ("I:1:F:1:1101:100:100", "rg1", 40, "AAAA"),
            ("I:1:F:1:1101:105:105", "rg1", 30, "AAAA"),
            ("I:1:F:1:1101:900:900", "rg1", 20, "AAAA"),
            ("I:1:F:1:1101:106:106", "rg1", 35, "CCCC"),
            ("I:1:F:1:1101:100:100", "rg2", 25, "AAAA"),
        ]
    elif case.shape in {"same-library", "separate-libraries"}:
        templates = [("shared", "rg1", 40, None), ("shared", "rg2", 20, None)]
    else:
        templates = [("z-winner", "rg1", 40, None), ("a-duplicate", "rg1", 20, None)]
    sequence = "A" * 20
    for flag, position, mate, length in [(99, 101, 201, 120), (147, 201, 101, -120)]:
        for name, group, quality, barcode in templates:
            tag = "" if barcode is None else f"\tRX:Z:{barcode}"
            yield f"{name}\t{flag}\tchr1\t{position}\t60\t20M\t=\t{mate}\t{length}\t{sequence}\t{chr(quality + 33) * 20}\tRG:Z:{group}{tag}\n"
    for name, flag, position in [("secondary", 256, 301), ("supplementary", 2048, 321)]:
        yield f"{name}\t{flag}\tchr1\t{position}\t30\t20M\t*\t0\t0\t{sequence}\t{'?' * 20}\tRG:Z:rg1\n"
    for index in range(case.padding):
        if case.paired_padding:
            position = 1000 + 400 * index
            for flag, own, mate, length in [(99, position, position + 200, 350), (147, position + 200, position, -350)]:
                yield f"pair-{index:08}\t{flag}\tchr1\t{own}\t60\t150M\t=\t{mate}\t{length}\t{'ACGT' * 37 + 'AC'}\t{'?' * 150}\tRG:Z:rg1\n"
            continue
        yield f"pad-{index:08}\t0\tchr1\t{1000 + 30 * index}\t60\t20M\t*\t0\t0\t{sequence}\t{'?' * 20}\tRG:Z:rg1\n"
    if case.shape != "family":
        yield f"unplaced\t4\t*\t0\t0\t*\t*\t0\t0\t{sequence}\t{'?' * 20}\tRG:Z:rg1\n"


def digest_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def canonical_record(line: str) -> bytes:
    fields = line.rstrip("\n").split("\t")
    if len(fields) < 11:
        raise ValueError("malformed alignment record")
    tags = sorted(tag for tag in fields[11:] if not tag.startswith("PG:Z:"))
    return ("\t".join(fields[:11] + tags) + "\n").encode()


def canonical_header(line: str) -> list[str]:
    values = line.rstrip("\n").split("\t")
    if values[0] == "@RG":
        for index, value in enumerate(values[1:], 1):
            if value.startswith("DT:") and re.fullmatch(
                r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,6})?(?:Z|[+-](?:[01]\d|2[0-3]):?[0-5]\d)", value[3:]
            ):
                try:
                    raw = re.sub(r"([+-]\d{2})(\d{2})$", r"\1:\2", value[3:])
                    raw = raw[:-1] + "+00:00" if raw.endswith("Z") else raw
                    date = datetime.fromisoformat(raw)
                except ValueError:
                    continue
                values[index] = "DT:" + date.astimezone(timezone.utc).isoformat()
    return [values[0], *sorted(values[1:])]


def summarize_bam(path: Path, samtools: str, log_dir: Path) -> OutputSummary:
    subprocess.run([samtools, "quickcheck", "-v", str(path)], check=True, timeout=60)
    digest = hashlib.sha256()
    count = duplicates = 0
    headers: list[list[str]] = []
    with (
        (log_dir / "samtools-view.stderr").open("w") as errors,
        subprocess.Popen(
            [samtools, "view", "-h", str(path)], stdout=subprocess.PIPE, stderr=errors, text=True
        ) as process,
    ):
        assert process.stdout is not None
        for line in process.stdout:
            if line.startswith("@"):
                if line.startswith(("@SQ\t", "@RG\t")):
                    headers.append(canonical_header(line))
                continue
            record = canonical_record(line)
            digest.update(record)
            count += 1
            duplicates += bool(int(line.split("\t", 2)[1]) & 0x400)
        if process.wait(timeout=60):
            raise RuntimeError(f"samtools failed to decode {path}")
    return {
        "alignment_sha256": digest.hexdigest(),
        "records": count,
        "duplicate_records": duplicates,
        "sq_rg_headers": sorted(headers),
    }


def read_histograms(path: Path) -> list[Histogram]:
    sections: list[Histogram] = []
    section: Histogram | None = None
    for line in path.read_text().splitlines():
        if line.startswith("## HISTOGRAM"):
            section = {"columns": None, "rows": []}
            sections.append(section)
        elif line.startswith("#"):
            section = None
        elif section is not None and line.strip():
            fields = line.split("\t")
            if section["columns"] is None:
                if len(set(fields)) != len(fields):
                    raise ValueError("duplicate histogram columns")
                section["columns"] = fields
                continue
            if len(fields) != len(section["columns"]):
                raise ValueError("malformed histogram row width")
            values = []
            for field in fields:
                try:
                    value = Decimal(field)
                except InvalidOperation as exc:
                    raise ValueError("nonnumeric histogram value") from exc
                if not value.is_finite():
                    raise ValueError("nonfinite histogram value")
                values.append(format(value.normalize(), "f") if value else "0")
            section["rows"].append(values)
    for section in sections:
        if section["columns"] is None:
            raise ValueError("missing histogram header")
        bins = [row[0] for row in section["rows"]]
        if len(set(bins)) != len(bins):
            raise ValueError("duplicate histogram bins")
        section["rows"].sort(key=lambda row: Decimal(row[0]))
    return sections


def measure(argv: list[str], directory: Path, env: dict[str, str], timeout_seconds: float = 300) -> Measurement:
    directory.mkdir()
    (directory / "scratch").mkdir()
    resource = directory / "resources.txt"
    command = [shutil.which("time") or "/usr/bin/time", "-f", "%e %U %S %M", "-o", str(resource), *argv]
    started = time.perf_counter()
    timed_out = threading.Event()
    with (directory / "stdout.txt").open("w") as stdout, (directory / "stderr.txt").open("w") as stderr:
        with subprocess.Popen(command, stdout=stdout, stderr=stderr, env=env, start_new_session=True) as process:

            def expire() -> None:
                if process.poll() is None:
                    timed_out.set()
                    with contextlib.suppress(ProcessLookupError):
                        os.killpg(process.pid, signal.SIGKILL)

            deadline = threading.Timer(timeout_seconds, expire)
            deadline.daemon = True
            deadline.start()
            try:
                exit_code = process.wait()
            finally:
                deadline.cancel()
        if timed_out.is_set():
            exit_code = 124
            stderr.write(f"\nBenchmark process group timed out after {timeout_seconds} seconds.\n")
    row: Measurement = {"argv": argv, "exit_code": exit_code, "wall_seconds": time.perf_counter() - started}
    if exit_code == 0:
        elapsed, user, system, rss = resource.read_text().split()
        row["time_seconds"] = float(elapsed)
        row["user_seconds"] = float(user)
        row["system_seconds"] = float(system)
        row["peak_rss_bytes"] = int(rss) * 1024
    return row


_DESCRIPTION = (
    "Adversarial synthetic whole-command evidence; not WGS validation.\n"
    "\n"
    "Requires real Picard, samtools and built Turbo Picard executables. Existing\n"
    "outputs are never overwritten. Comparisons cover every SAM alignment field and\n"
    "auxiliary tag except tool-specific PG provenance, record order, SQ/RG header\n"
    "content, normalized DuplicationMetrics tables, and numeric histogram columns\n"
    "and bins (bounded-v4). RG run dates with explicit offsets are compared as exact\n"
    "instants; other header values are preserved. Plot rendering is not established.\n"
)


def main() -> int:
    read_metrics: Callable[[Path], list[list[str]]] = importlib.import_module("compare_markduplicates").read_metrics

    parser = argparse.ArgumentParser(description=_DESCRIPTION)
    parser.add_argument("--candidate", type=Path, required=True)
    parser.add_argument("--baseline", type=Path)
    parser.add_argument("--picard", required=True)
    parser.add_argument("--samtools", required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--repeats", type=int, default=3)
    parser.add_argument("--case", action="append", dest="selected")
    args = parser.parse_args()
    if args.repeats < 1:
        parser.error("--repeats must be positive")
    selected = [case for case in cases() if not args.selected or case.name in args.selected]
    if not selected or (args.selected and set(args.selected) - {case.name for case in cases()}):
        parser.error("unknown or empty case selection")
    root = args.output_dir.resolve()
    root.mkdir(parents=True, exist_ok=False)
    report: dict[str, Any] = {
        "scope": "adversarial synthetic whole MarkDuplicates commands; not WGS/cohort evidence",
        "comparison": "ordered full alignment fields and tags excluding PG; SQ/RG headers with explicit-zone RG dates compared as exact instants; normalized DuplicationMetrics tables and numeric histograms (bounded-v4; not charts)",
        "candidate_source": os.environ.get("HEAD_SHA"),
        "baseline_source": os.environ.get("BASE_SHA"),
        "harness_sha256": digest_file(Path(__file__)),
        "host": platform.platform(),
        "cpu_affinity": sorted(os.sched_getaffinity(0)) if hasattr(os, "sched_getaffinity") else None,
        "repeats": args.repeats,
        "warmups": 1,
        "cache_policy": "warm page cache; no fsync or cache dropping",
        "wall_measurement": "blocking wait with process-group timeout watchdog",
        "inputs": [],
        "runs": [],
        "summaries": [],
        "candidate_pass": True,
    }
    executables = {"candidate": str(args.candidate.resolve()), "picard": str(Path(args.picard).resolve())}
    if args.baseline:
        executables["baseline"] = str(args.baseline.resolve())
    report["executables"] = {}
    for name, path in executables.items():
        probe = subprocess.run([path, "--version"], capture_output=True, text=True, timeout=60, check=False)
        report["executables"][name] = {
            "path": path,
            "sha256": digest_file(Path(path)),
            "version_stdout": probe.stdout.strip(),
            "version_stderr": probe.stderr.strip(),
            "version_exit_code": probe.returncode,
        }
    env = {key: value for key, value in os.environ.items() if not key.startswith("TURBO_PICARD_")}
    env.update(
        TURBO_PICARD_THREADS="0", TURBO_PICARD_REQUIRE_NATIVE="1", JAVA_TOOL_OPTIONS="-Xmx2g -XX:ActiveProcessorCount=1"
    )
    affinity: list[str] = []
    if shutil.which("taskset") and hasattr(os, "sched_getaffinity"):
        affinity = ["taskset", "-c", str(min(os.sched_getaffinity(0)))]
    report["execution_policy"] = {
        "affinity_prefix": affinity,
        "turbo_hts_workers": 0,
        "java_active_processors": 1,
        "java_heap": "2g",
        "gnu_time": shutil.which("time") or "/usr/bin/time",
    }

    def save() -> None:
        (root / "report.json").write_text(json.dumps(report, indent=2) + "\n")

    for case in selected:
        case_dir = root / case.name
        case_dir.mkdir()
        sam = case_dir / "input.sam"
        with sam.open("x") as stream:
            stream.writelines(sam_lines(case))
        bam = case_dir / "input.bam"
        subprocess.run([args.samtools, "view", "-b", "-o", str(bam), str(sam)], check=True, timeout=120)
        report["inputs"].append(
            {
                "case": case.name,
                "shape": case.shape,
                "padding": case.padding,
                "paired_padding": case.paired_padding,
                "sam_sha256": digest_file(sam),
                "bam_sha256": digest_file(bam),
                "bam_bytes": bam.stat().st_size,
            }
        )
        sam.unlink()
        reference: OutputSummary | None = None
        for repeat in range(args.repeats + 1):
            order = ["picard", "baseline", "candidate"] if repeat % 2 == 0 else ["candidate", "baseline", "picard"]
            for version in (version for version in order if version in executables):
                destination = case_dir / f"{version}-{repeat}"
                output = destination / "output.bam"
                metrics = destination / "metrics.txt"
                scratch = destination / "scratch"
                tags = case.shape != "family" and case.remove == "none"
                options = [
                    "MarkDuplicates",
                    f"I={bam}",
                    f"O={output}",
                    f"M={metrics}",
                    f"TMP_DIR={scratch}",
                    "ASSUME_SORTED=true",
                    "VALIDATION_STRINGENCY=SILENT",
                    "QUIET=false",
                    "ADD_PG_TAG_TO_READS=false",
                    "COMPRESSION_LEVEL=1",
                    "CLEAR_DT=true",
                    f"TAG_DUPLICATE_SET_MEMBERS={str(tags).lower()}",
                    f"TAGGING_POLICY={'All' if case.shape == 'optical' else 'DontTag'}",
                ]
                if case.shape == "optical":
                    options += ["BARCODE_TAG=RX", "OPTICAL_DUPLICATE_PIXEL_DISTANCE=100"]
                else:
                    options += ["READ_NAME_REGEX=null"]
                if case.remove == "all":
                    options += ["REMOVE_DUPLICATES=true"]
                elif case.remove == "optical":
                    options += ["REMOVE_SEQUENCING_DUPLICATES=true"]
                row = measure([*affinity, executables[version], *options], destination, env)
                row["case"] = case.name
                row["version"] = version
                row["repeat"] = repeat
                row["warmup"] = repeat == 0
                report["runs"].append(row)
                if row["exit_code"] == 0:
                    summary = summarize_bam(output, args.samtools, destination)
                    table = read_metrics(metrics)
                    if not table:
                        raise RuntimeError(f"missing duplication metrics: {metrics}")
                    summary["duplication_metrics"] = [table[0], *sorted(table[1:])]
                    try:
                        summary["histograms"] = read_histograms(metrics)
                    except ValueError as error:
                        summary["histogram_error"] = str(error)
                        if version == "picard":
                            row["output_summary"] = summary
                            row["parity_pass"] = False
                            report["candidate_pass"] = False
                            save()
                            raise RuntimeError("Picard oracle histogram is invalid") from error
                    row["output_summary"] = summary
                    row["output_bytes"] = output.stat().st_size
                    row["external_plan_reported"] = (
                        "using external-sort plan" in (destination / "stderr.txt").read_text()
                    )
                    if reference is None:
                        if version != "picard":
                            raise RuntimeError("Picard oracle must run first")
                        reference = summary
                    row["parity_pass"] = summary == reference
                    if row["parity_pass"]:
                        output.unlink()
                    if version in {"candidate", "picard"} and not row["parity_pass"]:
                        report["candidate_pass"] = False
                    if version == "candidate" and not row["external_plan_reported"]:
                        report["candidate_pass"] = False
                else:
                    row["parity_pass"] = False
                    if version in {"candidate", "picard"}:
                        report["candidate_pass"] = False
                save()
        case_summary: dict[str, Any] = {"case": case.name}
        for version in executables:
            rows = [
                row
                for row in report["runs"]
                if row["case"] == case.name and row["version"] == version and not row["warmup"]
            ]
            passed = all(row["parity_pass"] for row in rows)
            case_summary[version] = {
                "parity_pass": passed,
                "median_wall_seconds": statistics.median(row["wall_seconds"] for row in rows),
                "median_peak_rss_bytes": statistics.median(row.get("peak_rss_bytes", 0) for row in rows),
            }
        report["summaries"].append(case_summary)
        save()
        print(json.dumps(case_summary), flush=True)
    return 0 if report["candidate_pass"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
