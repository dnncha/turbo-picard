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
    return elapsed, completed


def conda_runner():
    for name in ("mamba", "micromamba"):
        path = shutil.which(name)
        if path:
            return path
    raise SystemExit("mamba or micromamba is required to benchmark against Picard")


def write_shard_sam(path, shard, shards, reads):
    sequence = "ACGT" * 25
    qualities = "F" * len(sequence)
    with open(path, "w", encoding="utf-8") as handle:
        handle.write("@HD\tVN:1.6\tSO:coordinate\n")
        handle.write(f"@SQ\tSN:chr1\tLN:{reads + len(sequence) + 100}\n")
        for position in range(shard + 1, reads + 1, shards):
            handle.write(
                f"read{position:09d}\t0\tchr1\t{position}\t60\t{len(sequence)}M\t*\t0\t0\t{sequence}\t{qualities}\n"
            )


def stable_records(path):
    rows = []
    with open(path, encoding="utf-8") as handle:
        for line in handle:
            if not line.startswith("@"):
                rows.append(line.rstrip("\n"))
    return rows


def main():
    parser = argparse.ArgumentParser(description="Benchmark turbo-picard MergeSamFiles against Picard.")
    parser.add_argument("--reads", type=int, default=200_000, help="total reads to synthesize")
    parser.add_argument("--shards", type=int, default=4, help="coordinate-sorted input shards")
    parser.add_argument(
        "--conda-prefix",
        default=os.environ.get("TURBO_PICARD_CONDA_PREFIX", str(ROOT / ".conda-turbo-picard")),
        help="conda environment prefix containing Picard",
    )
    parser.add_argument("--skip-build", action="store_true", help="reuse existing release binary")
    args = parser.parse_args()

    if args.shards < 2:
        raise SystemExit("--shards must be >= 2")
    if not args.skip_build:
        run(["cargo", "build", "--release", "-p", "turbo-picard-cli", "--bin", "picard"])

    turbo = ROOT / "target" / "release" / "picard"
    if not turbo.exists():
        raise SystemExit(f"missing release binary: {turbo}")
    runner = conda_runner()

    with tempfile.TemporaryDirectory(prefix="turbo-picard-mergesamfiles-bench.") as tmp:
        workdir = pathlib.Path(tmp)
        inputs = []
        for shard in range(args.shards):
            sam = workdir / f"input-{shard}.sam"
            bam = workdir / f"input-{shard}.bam"
            write_shard_sam(sam, shard, args.shards, args.reads)
            run(
                [
                    str(turbo),
                    "SortSam",
                    f"I={sam}",
                    f"O={bam}",
                    "SORT_ORDER=coordinate",
                    "VALIDATION_STRINGENCY=SILENT",
                    "QUIET=true",
                ]
            )
            inputs.append(bam)

        input_args = [f"I={path}" for path in inputs]
        common = [
            "MergeSamFiles",
            *input_args,
            "SORT_ORDER=coordinate",
            "ASSUME_SORTED=true",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ]
        turbo_out = workdir / "turbo.sam"
        picard_out = workdir / "picard.sam"
        turbo_time, _ = run([str(turbo), *common, f"O={turbo_out}"])
        picard_time, _ = run([runner, "run", "-p", args.conda_prefix, "picard", *common, f"O={picard_out}"])

        parity = stable_records(turbo_out) == stable_records(picard_out)
        speedup = picard_time / turbo_time if turbo_time > 0 else float("inf")
        print("command=MergeSamFiles")
        print(f"reads={args.reads}")
        print(f"shards={args.shards}")
        print(f"turbo_seconds={turbo_time:.6f}")
        print(f"picard_seconds={picard_time:.6f}")
        print(f"speedup={speedup:.2f}x")
        print(f"parity={'PASS' if parity else 'FAIL'}")
        if not parity:
            raise SystemExit("MergeSamFiles benchmark parity failed")


if __name__ == "__main__":
    main()
