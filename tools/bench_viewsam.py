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


def run_capture(command, *, cwd=ROOT):
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
    return elapsed, completed.stdout


def conda_runner():
    for name in ("mamba", "micromamba"):
        path = shutil.which(name)
        if path:
            return path
    raise SystemExit("mamba or micromamba is required to benchmark against Picard")


def write_sam(path, reads):
    sequence = "ACGT" * 25
    qualities = "F" * len(sequence)
    with open(path, "w", encoding="utf-8") as handle:
        handle.write("@HD\tVN:1.6\tSO:coordinate\n")
        handle.write(f"@SQ\tSN:chr1\tLN:{reads + len(sequence) + 100}\n")
        for index in range(reads):
            handle.write(
                f"read{index:09d}\t0\tchr1\t{index + 1}\t60\t{len(sequence)}M\t*\t0\t0\t{sequence}\t{qualities}\n"
            )


def stable_records(text):
    return [line.rstrip("\n") for line in text.splitlines() if not line.startswith("@")]


def main():
    parser = argparse.ArgumentParser(description="Benchmark turbo-picard ViewSam against Picard.")
    parser.add_argument("--reads", type=int, default=50_000, help="SAM records to synthesize")
    parser.add_argument(
        "--conda-prefix",
        default=os.environ.get("TURBO_PICARD_CONDA_PREFIX", str(ROOT / ".conda-turbo-picard")),
        help="conda environment prefix containing Picard",
    )
    parser.add_argument("--skip-build", action="store_true", help="reuse existing release binary")
    args = parser.parse_args()

    if not args.skip_build:
        run(["cargo", "build", "--release", "-p", "turbo-picard-cli", "--bin", "picard"])

    turbo = ROOT / "target" / "release" / "picard"
    if not turbo.exists():
        raise SystemExit(f"missing release binary: {turbo}")
    runner = conda_runner()

    with tempfile.TemporaryDirectory(prefix="turbo-picard-viewsam-bench.") as tmp:
        workdir = pathlib.Path(tmp)
        input_sam = workdir / "input.sam"
        input_bam = workdir / "input.bam"
        write_sam(input_sam, args.reads)

        run([str(turbo), "SortSam", f"I={input_sam}", f"O={input_bam}", "SORT_ORDER=coordinate"])

        common = [
            "ViewSam",
            f"I={input_bam}",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ]
        turbo_time, turbo_stdout = run_capture([str(turbo), *common])
        picard_time, picard_stdout = run_capture(
            [runner, "run", "-p", args.conda_prefix, "picard", *common]
        )

        parity = stable_records(turbo_stdout) == stable_records(picard_stdout)
        speedup = picard_time / turbo_time if turbo_time > 0 else float("inf")
        print("command=ViewSam")
        print(f"reads={args.reads}")
        print(f"turbo_seconds={turbo_time:.6f}")
        print(f"picard_seconds={picard_time:.6f}")
        print(f"speedup={speedup:.2f}x")
        print(f"parity={'PASS' if parity else 'FAIL'}")
        if not parity:
            raise SystemExit("ViewSam benchmark parity failed")


if __name__ == "__main__":
    main()
