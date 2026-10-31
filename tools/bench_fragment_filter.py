#!/usr/bin/env python3

from __future__ import annotations

import argparse
import contextlib
import hashlib
import json
import platform
import re
import statistics
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Literal, TypedDict

MODULE_DESCRIPTION = (
    "A/B the MarkDuplicates fragment-stage filter on a synthetic mix.\n"
    "\n"
    "The mix has many pair-only loci and a small number of loci where an unpaired\n"
    "candidate taints the fragment key. Without the filter every pair read end is\n"
    "spilled into the fragment sorter; with it, only pairs whose key was tainted are.\n"
    "\n"
    "This is a native-path measurement on synthetic input. It is not a Picard\n"
    "comparison, not a whole-genome guarantee, and not evidence for a different\n"
    "input distribution. The tool refuses to report timings unless both binaries\n"
    "produce byte-identical BAM output and metrics, so a faster run that changes the\n"
    "decision cannot be presented as a win.\n"
)

READ_LEN = 100
PAIRS_PER_TAINTED = 3
SEQ = "A" * READ_LEN
QUAL = "I" * READ_LEN


class RunResult(TypedDict):
    wall_seconds: float | None
    cpu_seconds: float | None
    peak_rss_bytes: int | None
    bam_sha256: str
    metrics_sha256: str


class InputFacts(TypedDict):
    sam_sha256: str
    bam_sha256: str
    records: None


class Summary(TypedDict):
    label: str
    repeats: int
    median_wall_seconds: float | None
    median_cpu_seconds: float | None
    median_peak_rss_bytes: int | None
    bam_sha256: list[str]
    metrics_sha256: list[str]


def sam_lines(pairs: int, unpaired_loci: int) -> list[str]:
    if pairs < 1 or unpaired_loci < 1:
        raise ValueError("pairs and unpaired_loci must be positive")
    records: list[tuple[int, str]] = []
    for index in range(pairs):
        pos = 4 * index + 1
        qname = f"pair{index}"
        records.append(
            (pos, f"{qname}\t99\tchr1\t{pos}\t60\t{READ_LEN}M\t=\t{pos + 150}\t250\t{SEQ}\t{QUAL}\tRG:Z:rg1\n")
        )
        records.append(
            (
                pos + 150,
                f"{qname}\t147\tchr1\t{pos + 150}\t60\t{READ_LEN}M\t=\t{pos}\t-250\t{SEQ}\t{QUAL}\tRG:Z:rg1\n",
            )
        )
    base = 4 * pairs + 1000
    for index in range(unpaired_loci):
        pos = base + 8 * index
        records.append((pos, f"unpaired{index}\t9\tchr1\t{pos}\t60\t{READ_LEN}M\t*\t0\t0\t{SEQ}\t{QUAL}\tRG:Z:rg1\n"))
        for member in range(PAIRS_PER_TAINTED):
            qname = f"coll{index}_{member}"
            records.append(
                (pos, f"{qname}\t99\tchr1\t{pos}\t60\t{READ_LEN}M\t=\t{pos + 150}\t250\t{SEQ}\t{QUAL}\tRG:Z:rg1\n")
            )
            records.append(
                (
                    pos + 150,
                    f"{qname}\t147\tchr1\t{pos + 150}\t60\t{READ_LEN}M\t=\t{pos}\t-250\t{SEQ}\t{QUAL}\tRG:Z:rg1\n",
                )
            )
    records.sort(key=lambda item: item[0])
    header = [
        "@HD\tVN:1.6\tSO:coordinate\n",
        "@SQ\tSN:chr1\tLN:4000000\n",
        "@RG\tID:rg1\tSM:sample\tLB:lib1\tPL:ILLUMINA\n",
    ]
    return header + [line for _, line in records]


def digest(path: Path) -> str:
    hasher = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1 << 20), b""):
            hasher.update(block)
    return hasher.hexdigest()


def time_flags() -> list[str]:
    return ["-l"] if platform.system() == "Darwin" else ["-v"]


def parse_peak_rss(text: str) -> int | None:
    for line in text.splitlines():
        if "maximum resident set size" in line.lower():
            values = re.findall(r"\d+", line)
            if values:
                value = int(values[-1])
                return value if platform.system() == "Darwin" else value * 1024
    return None


def parse_cpu_seconds(text: str) -> float | None:
    user = system = None
    for line in text.splitlines():
        stripped = line.strip()
        if stripped.startswith("User time (seconds):"):
            user = float(stripped.split(":")[-1].strip())
        elif stripped.startswith("System time (seconds):"):
            system = float(stripped.split(":")[-1].strip())
        else:
            parts = stripped.split()
            if "user" in parts:
                with contextlib.suppress(ValueError):
                    user = float(parts[parts.index("user") - 1])
            if "sys" in parts:
                with contextlib.suppress(ValueError):
                    system = float(parts[parts.index("sys") - 1])
    if user is None and system is None:
        return None
    return (user or 0.0) + (system or 0.0)


GNU_ELAPSED = re.compile(r"Elapsed \(wall clock\) time.*:\s+([\d:.]+)\s*$")


def parse_gnu_elapsed(value: str) -> float:
    seconds = 0.0
    for field in value.split(":"):
        seconds = seconds * 60 + float(field)
    return seconds


def parse_wall_seconds(text: str) -> float | None:
    for line in text.splitlines():
        gnu = GNU_ELAPSED.search(line)
        if gnu:
            try:
                return parse_gnu_elapsed(gnu.group(1))
            except ValueError:
                continue
        parts = line.split()
        if "real" in parts:
            index = parts.index("real")
            if index > 0:
                try:
                    return float(parts[index - 1])
                except ValueError:
                    continue
    return None


def run_once(
    binary: Path,
    bam: Path,
    workdir: Path,
    timeout_s: int,
) -> RunResult:
    workdir.mkdir(parents=True, exist_ok=True)
    out_bam = workdir / "out.bam"
    metrics = workdir / "metrics.txt"
    tmp = workdir / "tmp"
    tmp.mkdir()
    command = [
        str(binary),
        "MarkDuplicates",
        f"I={bam}",
        f"O={out_bam}",
        f"M={metrics}",
        "READ_NAME_REGEX=null",
        f"TMP_DIR={tmp}",
        "VALIDATION_STRINGENCY=LENIENT",
        "ASSUME_SORT_ORDER=coordinate",
    ]
    timed = ["/usr/bin/time", *time_flags(), *command]
    completed = subprocess.run(
        timed,
        capture_output=True,
        text=True,
        timeout=timeout_s,
        env={"TMPDIR": str(workdir), "PATH": "/usr/bin:/bin"},
        check=False,
    )
    if completed.returncode != 0:
        raise RuntimeError(f"{binary} failed with {completed.returncode}: {completed.stderr[-400:]}")
    return {
        "wall_seconds": parse_wall_seconds(completed.stderr),
        "cpu_seconds": parse_cpu_seconds(completed.stderr),
        "peak_rss_bytes": parse_peak_rss(completed.stderr),
        "bam_sha256": digest(out_bam),
        "metrics_sha256": digest(metrics),
    }


def build_input(candidate: Path, workdir: Path, pairs: int, unpaired_loci: int) -> tuple[Path, InputFacts]:
    sam = workdir / "input.sam"
    sam.write_text("".join(sam_lines(pairs, unpaired_loci)), encoding="utf-8")
    bam = workdir / "input.bam"
    subprocess.run(
        [
            str(candidate),
            "SortSam",
            f"I={sam}",
            f"O={bam}",
            "SO=coordinate",
            "CREATE_INDEX=false",
            "VALIDATION_STRINGENCY=LENIENT",
        ],
        check=True,
        capture_output=True,
        text=True,
        timeout=1800,
        env={"TMPDIR": str(workdir), "PATH": "/usr/bin:/bin"},
    )
    return bam, {"sam_sha256": digest(sam), "bam_sha256": digest(bam), "records": None}


def summarize(label: str, runs: list[RunResult]) -> Summary:
    walls = [run["wall_seconds"] for run in runs if run["wall_seconds"] is not None]
    cpus = [run["cpu_seconds"] for run in runs if run["cpu_seconds"] is not None]
    rss = [run["peak_rss_bytes"] for run in runs if run["peak_rss_bytes"] is not None]
    return {
        "label": label,
        "repeats": len(runs),
        "median_wall_seconds": statistics.median(walls) if walls else None,
        "median_cpu_seconds": statistics.median(cpus) if cpus else None,
        "median_peak_rss_bytes": int(statistics.median(rss)) if rss else None,
        "bam_sha256": sorted({run["bam_sha256"] for run in runs}),
        "metrics_sha256": sorted({run["metrics_sha256"] for run in runs}),
    }


def paired_ratio(
    base_runs: list[RunResult], candidate_runs: list[RunResult], key: Literal["cpu_seconds", "wall_seconds"]
) -> float | None:
    ratios = []
    for base, candidate in zip(base_runs, candidate_runs):
        base_value = base[key]
        candidate_value = candidate[key]
        if base_value and candidate_value:
            ratios.append(base_value / candidate_value)
    return statistics.median(ratios) if ratios else None


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=MODULE_DESCRIPTION)
    parser.add_argument("--base-binary", required=True, type=Path)
    parser.add_argument("--candidate-binary", required=True, type=Path)
    parser.add_argument("--pairs", type=int, default=400_000)
    parser.add_argument("--unpaired-loci", type=int, default=5_000)
    parser.add_argument("--warmups", type=int, default=1)
    parser.add_argument("--repeats", type=int, default=3)
    parser.add_argument("--timeout-seconds", type=int, default=3600)
    parser.add_argument("--keep", type=Path, help="keep inputs and outputs in this directory")
    args = parser.parse_args(argv)

    for binary in (args.base_binary, args.candidate_binary):
        if not binary.is_file():
            parser.error(f"binary not found: {binary}")
    if args.pairs < 1 or args.unpaired_loci < 1 or args.repeats < 1:
        parser.error("pairs and unpaired-loci must be positive and repeats must be at least 1")

    keep = args.keep
    if keep is None:
        temporary = tempfile.TemporaryDirectory(prefix="turbo-picard-fragment-filter.")
        workdir = Path(temporary.name)
        keep = workdir
    else:
        temporary = None
        workdir = keep
        if workdir.exists() and any(workdir.iterdir()):
            parser.error(f"--keep directory must be empty: {workdir}")
        workdir.mkdir(parents=True, exist_ok=True)

    bam, input_facts = build_input(args.candidate_binary, workdir, args.pairs, args.unpaired_loci)

    runs: dict[str, list[RunResult]] = {"base": [], "candidate": []}
    order = [("candidate", args.candidate_binary), ("base", args.base_binary)]
    for label, binary in order:
        for index in range(args.warmups):
            run_once(binary, bam, workdir / f"{label}-warmup-{index}", args.timeout_seconds)
    for index in range(args.repeats):
        ordered = order if index % 2 == 0 else list(reversed(order))
        for label, binary in ordered:
            runs[label].append(run_once(binary, bam, workdir / f"{label}-{index}", args.timeout_seconds))

    base = summarize("base", runs["base"])
    candidate = summarize("candidate", runs["candidate"])
    if base["bam_sha256"] != candidate["bam_sha256"] or base["metrics_sha256"] != candidate["metrics_sha256"]:
        print("refusing to report timings: outputs differ", file=sys.stderr)
        print(json.dumps({"base": base, "candidate": candidate}, indent=2), file=sys.stderr)
        return 2

    report = {
        "scope": "synthetic-native-path-only",
        "not_a_picard_comparison": True,
        "input": {
            "pairs": args.pairs,
            "unpaired_loci": args.unpaired_loci,
            "pairs_per_tainted": PAIRS_PER_TAINTED,
            **input_facts,
        },
        "host": platform.platform(),
        "base": base,
        "candidate": candidate,
        "wall_speedup": (
            base["median_wall_seconds"] / candidate["median_wall_seconds"]
            if base["median_wall_seconds"] and candidate["median_wall_seconds"]
            else None
        ),
        "cpu_speedup": (
            base["median_cpu_seconds"] / candidate["median_cpu_seconds"]
            if base["median_cpu_seconds"] and candidate["median_cpu_seconds"]
            else None
        ),
        "paired_cpu_speedup": paired_ratio(runs["base"], runs["candidate"], "cpu_seconds"),
        "paired_wall_speedup": paired_ratio(runs["base"], runs["candidate"], "wall_seconds"),
        "rss_reduction_fraction": (
            1 - candidate["median_peak_rss_bytes"] / base["median_peak_rss_bytes"]
            if base["median_peak_rss_bytes"] and candidate["median_peak_rss_bytes"]
            else None
        ),
    }
    print(json.dumps(report, indent=2))
    if temporary is not None:
        temporary.cleanup()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
