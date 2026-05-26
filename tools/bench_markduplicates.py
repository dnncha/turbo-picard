#!/usr/bin/env python3
"""Benchmark Picard and turbo-picard MarkDuplicates with reproducible JSONL output."""

from __future__ import annotations

import argparse
import json
import platform
import re
import shlex
import subprocess
import time
from dataclasses import asdict, dataclass
from pathlib import Path


@dataclass
class BenchmarkResult:
    tool: str
    phase: str
    iteration: int
    command: list[str]
    exit_code: int
    wall_seconds: float
    max_rss_bytes: int | None
    stdout_path: str
    stderr_path: str
    output_bam: str
    metrics_file: str


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Run repeatable Picard vs turbo-picard MarkDuplicates benchmarks.",
    )
    parser.add_argument("--picard-command", required=True, help="Base Picard command.")
    parser.add_argument("--turbo-picard-command", required=True, help="Base turbo-picard command.")
    parser.add_argument("--input-bam", required=True, type=Path)
    parser.add_argument("--output-dir", required=True, type=Path)
    parser.add_argument("--repeats", type=int, default=3)
    parser.add_argument("--warmup", type=int, default=1)
    parser.add_argument(
        "--results-jsonl",
        type=Path,
        help="Results path. Defaults to <output-dir>/markduplicates-benchmark.jsonl.",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.repeats < 1:
        raise SystemExit("--repeats must be >= 1")
    if args.warmup < 0:
        raise SystemExit("--warmup must be >= 0")

    args.output_dir.mkdir(parents=True, exist_ok=True)
    results_path = args.results_jsonl or args.output_dir / "markduplicates-benchmark.jsonl"

    results = []
    for tool, base_command in [
        ("picard", args.picard_command),
        ("turbo-picard", args.turbo_picard_command),
    ]:
        for phase, count in [("warmup", args.warmup), ("benchmark", args.repeats)]:
            for iteration in range(1, count + 1):
                results.append(
                    run_once(
                        tool=tool,
                        phase=phase,
                        iteration=iteration,
                        base_command=base_command,
                        input_bam=args.input_bam,
                        output_dir=args.output_dir,
                    )
                )

    with results_path.open("w", encoding="utf-8") as handle:
        for result in results:
            handle.write(json.dumps(asdict(result), sort_keys=True) + "\n")

    print(f"wrote {len(results)} benchmark records to {results_path}")
    return 0 if all(result.exit_code == 0 for result in results) else 1


def run_once(
    *,
    tool: str,
    phase: str,
    iteration: int,
    base_command: str,
    input_bam: Path,
    output_dir: Path,
) -> BenchmarkResult:
    stem = f"{tool}-{phase}-{iteration}"
    output_bam = output_dir / f"{stem}.bam"
    metrics_file = output_dir / f"{stem}.metrics.txt"
    stdout_path = output_dir / f"{stem}.stdout.txt"
    stderr_path = output_dir / f"{stem}.stderr.txt"

    command = shlex.split(base_command) + [
        f"I={input_bam}",
        f"O={output_bam}",
        f"M={metrics_file}",
    ]
    timed_command = time_command(command)

    start = time.perf_counter()
    with stdout_path.open("wb") as stdout, stderr_path.open("wb") as stderr:
        completed = subprocess.run(
            timed_command,
            stdout=stdout,
            stderr=stderr,
            check=False,
        )
    wall_seconds = time.perf_counter() - start
    max_rss_bytes = parse_max_rss(stderr_path)

    return BenchmarkResult(
        tool=tool,
        phase=phase,
        iteration=iteration,
        command=command,
        exit_code=completed.returncode,
        wall_seconds=wall_seconds,
        max_rss_bytes=max_rss_bytes,
        stdout_path=str(stdout_path),
        stderr_path=str(stderr_path),
        output_bam=str(output_bam),
        metrics_file=str(metrics_file),
    )


def time_command(command: list[str]) -> list[str]:
    time_binary = Path("/usr/bin/time")
    if not time_binary.exists():
        return command
    if platform.system() == "Darwin":
        return [str(time_binary), "-l", *command]
    return [str(time_binary), "-v", *command]


def parse_max_rss(stderr_path: Path) -> int | None:
    text = stderr_path.read_text(encoding="utf-8", errors="replace")
    mac_match = re.search(r"(\d+)\s+maximum resident set size", text)
    if mac_match:
        return int(mac_match.group(1))

    linux_match = re.search(r"Maximum resident set size .*?:\s*(\d+)", text)
    if linux_match:
        return int(linux_match.group(1)) * 1024

    return None


if __name__ == "__main__":
    raise SystemExit(main())
