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


def write_unsorted_sam(path, reads):
    sequence = "ACGT" * 25
    qualities = "F" * len(sequence)
    with open(path, "w", encoding="utf-8") as handle:
        handle.write("@HD\tVN:1.6\tSO:unsorted\n")
        handle.write(f"@SQ\tSN:chr1\tLN:{reads + len(sequence) + 100}\n")
        for index in range(reads, 0, -1):
            handle.write(
                f"read{index:09d}\t0\tchr1\t{index}\t60\t{len(sequence)}M\t*\t0\t0\t{sequence}\t{qualities}\n"
            )


def stable_rows(path):
    rows = []
    with open(path, encoding="utf-8") as handle:
        for line in handle:
            line = line.rstrip("\n")
            if line.startswith("@HD"):
                rows.append(line)
            elif not line.startswith("@"):
                rows.append(line)
    return rows


def main():
    parser = argparse.ArgumentParser(description="Benchmark turbo-picard SortSam against Picard.")
    parser.add_argument("--reads", type=int, default=200_000, help="reads to synthesize")
    parser.add_argument(
        "--max-records-in-ram",
        type=int,
        default=500_000,
        help="SortSam MAX_RECORDS_IN_RAM for both implementations",
    )
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

    with tempfile.TemporaryDirectory(prefix="turbo-picard-sortsam-bench.") as tmp:
        workdir = pathlib.Path(tmp)
        input_sam = workdir / "input.sam"
        turbo_out = workdir / "turbo.sam"
        picard_out = workdir / "picard.sam"
        write_unsorted_sam(input_sam, args.reads)

        common = [
            "SortSam",
            f"I={input_sam}",
            "SORT_ORDER=coordinate",
            f"MAX_RECORDS_IN_RAM={args.max_records_in_ram}",
            f"TMP_DIR={workdir}",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ]
        turbo_time = run([str(turbo), *common, f"O={turbo_out}"])
        picard_time = run([runner, "run", "-p", args.conda_prefix, "picard", *common, f"O={picard_out}"])

        parity = stable_rows(turbo_out) == stable_rows(picard_out)
        speedup = picard_time / turbo_time if turbo_time > 0 else float("inf")
        print("command=SortSam")
        print(f"reads={args.reads}")
        print(f"max_records_in_ram={args.max_records_in_ram}")
        print(f"turbo_seconds={turbo_time:.6f}")
        print(f"picard_seconds={picard_time:.6f}")
        print(f"speedup={speedup:.2f}x")
        print(f"parity={'PASS' if parity else 'FAIL'}")
        if not parity:
            raise SystemExit("SortSam benchmark parity failed")


if __name__ == "__main__":
    main()
