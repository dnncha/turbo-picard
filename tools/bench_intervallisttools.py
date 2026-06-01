#!/usr/bin/env python3
import argparse
import os
import pathlib
import shutil
import subprocess
import sys
import tempfile
import time


ROOT = pathlib.Path(__file__).resolve().parents[1]


def run(command, *, cwd=ROOT):
    start = time.perf_counter()
    completed = subprocess.run(
        command,
        cwd=cwd,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        check=False,
    )
    elapsed = time.perf_counter() - start
    if completed.returncode != 0:
        sys.stderr.write(completed.stdout)
        sys.stderr.write(completed.stderr)
        raise SystemExit(f"command failed with exit {completed.returncode}: {' '.join(command)}")
    return elapsed


def conda_runner():
    for name in ("mamba", "micromamba"):
        path = shutil.which(name)
        if path:
            return path
    raise SystemExit("mamba or micromamba is required to benchmark against Picard")


def write_interval_list(path, *, start_index, intervals):
    with open(path, "w", encoding="utf-8") as handle:
        handle.write("@HD\tVN:1.6\tSO:coordinate\n")
        handle.write("@SQ\tSN:chr1\tLN:100000000\n")
        for index in range(intervals):
            # Overlap adjacent shards so UNIQUE=true has real merging work to do.
            start = max(1, start_index + index * 7)
            end = start + 12
            handle.write(f"chr1\t{start}\t{end}\t+\tinterval_{start_index}_{index:09d}\n")


def stable_output(path):
    return [
        line.rstrip("\n")
        for line in path.read_text(encoding="utf-8").splitlines()
        if not line.startswith("@PG")
    ]


def main():
    parser = argparse.ArgumentParser(description="Benchmark turbo-picard IntervalListTools against Picard.")
    parser.add_argument("--reads", type=int, default=100_000, help="intervals to synthesize")
    parser.add_argument(
        "--conda-prefix",
        default=os.environ.get("TURBO_PICARD_CONDA_PREFIX", str(ROOT / ".conda-turbo-picard")),
        help="conda environment prefix containing Picard",
    )
    parser.add_argument("--skip-build", action="store_true", help="reuse existing release binary")
    args = parser.parse_args()

    if args.reads < 2:
        raise SystemExit("--reads must be >= 2")
    if not args.skip_build:
        run(["cargo", "build", "--release", "-p", "turbo-picard-cli", "--bin", "picard"])

    turbo = ROOT / "target" / "release" / "picard"
    if not turbo.exists():
        raise SystemExit(f"missing release binary: {turbo}")
    runner = conda_runner()

    with tempfile.TemporaryDirectory(prefix="turbo-picard-intervallisttools-bench.") as tmp:
        workdir = pathlib.Path(tmp)
        first = workdir / "first.interval_list"
        second = workdir / "second.interval_list"
        turbo_out = workdir / "turbo.interval_list"
        picard_out = workdir / "picard.interval_list"
        first_count = args.reads // 2
        second_count = args.reads - first_count
        write_interval_list(first, start_index=1, intervals=first_count)
        write_interval_list(second, start_index=4, intervals=second_count)

        common = [
            "IntervalListTools",
            f"I={first}",
            f"I={second}",
            "ACTION=CONCAT",
            "SORT=true",
            "UNIQUE=true",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ]
        turbo_time = run([str(turbo), *common, f"O={turbo_out}"])
        picard_time = run([runner, "run", "-p", args.conda_prefix, "picard", *common, f"O={picard_out}"])

        parity = stable_output(turbo_out) == stable_output(picard_out)
        speedup = picard_time / turbo_time if turbo_time > 0 else float("inf")
        print("command=IntervalListTools")
        print(f"reads={args.reads}")
        print(f"turbo_seconds={turbo_time:.6f}")
        print(f"picard_seconds={picard_time:.6f}")
        print(f"speedup={speedup:.2f}x")
        print(f"parity={'PASS' if parity else 'FAIL'}")
        if not parity:
            raise SystemExit("IntervalListTools benchmark parity failed")


if __name__ == "__main__":
    main()
