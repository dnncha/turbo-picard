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


def write_coordinate_sam(path, reads):
    sequence = "ACGT" * 25
    qualities = "F" * len(sequence)
    with open(path, "w", encoding="utf-8") as handle:
        handle.write("@HD\tVN:1.6\tSO:coordinate\n")
        handle.write(f"@SQ\tSN:chr1\tLN:{reads + len(sequence) + 100}\n")
        for index in range(1, reads + 1):
            handle.write(
                f"read{index:09d}\t0\tchr1\t{index}\t60\t{len(sequence)}M\t*\t0\t0\t{sequence}\t{qualities}\n"
            )


def idxstats(runner, conda_prefix, bam, index):
    default_index = bam.with_suffix(".bai")
    if default_index.exists():
        default_index.unlink()
    shutil.copyfile(index, default_index)
    _, completed = run([runner, "run", "-p", conda_prefix, "samtools", "idxstats", str(bam)])
    return completed.stdout


def main():
    parser = argparse.ArgumentParser(description="Benchmark turbo-picard BuildBamIndex against Picard.")
    parser.add_argument("--reads", type=int, default=300_000, help="reads to synthesize")
    parser.add_argument(
        "--conda-prefix",
        default=os.environ.get("TURBO_PICARD_CONDA_PREFIX", str(ROOT / ".conda-turbo-picard")),
        help="conda environment prefix containing Picard and samtools",
    )
    parser.add_argument("--skip-build", action="store_true", help="reuse existing release binary")
    args = parser.parse_args()

    if not args.skip_build:
        run(["cargo", "build", "--release", "-p", "turbo-picard-cli", "--bin", "picard"])

    turbo = ROOT / "target" / "release" / "picard"
    if not turbo.exists():
        raise SystemExit(f"missing release binary: {turbo}")
    runner = conda_runner()

    with tempfile.TemporaryDirectory(prefix="turbo-picard-buildbamindex-bench.") as tmp:
        workdir = pathlib.Path(tmp)
        input_sam = workdir / "input.sam"
        input_bam = workdir / "input.bam"
        turbo_index = workdir / "turbo.bai"
        picard_index = workdir / "picard.bai"
        write_coordinate_sam(input_sam, args.reads)
        run(
            [
                str(turbo),
                "SortSam",
                f"I={input_sam}",
                f"O={input_bam}",
                "SORT_ORDER=coordinate",
                "VALIDATION_STRINGENCY=SILENT",
                "QUIET=true",
            ]
        )

        common = ["BuildBamIndex", f"I={input_bam}", "VALIDATION_STRINGENCY=SILENT", "QUIET=true"]
        turbo_time, _ = run([str(turbo), *common, f"O={turbo_index}"])
        picard_time, _ = run([runner, "run", "-p", args.conda_prefix, "picard", *common, f"O={picard_index}"])

        turbo_stats = idxstats(runner, args.conda_prefix, input_bam, turbo_index)
        picard_stats = idxstats(runner, args.conda_prefix, input_bam, picard_index)
        parity = turbo_stats == picard_stats and "\t" in turbo_stats
        speedup = picard_time / turbo_time if turbo_time > 0 else float("inf")
        print("command=BuildBamIndex")
        print(f"reads={args.reads}")
        print(f"turbo_seconds={turbo_time:.6f}")
        print(f"picard_seconds={picard_time:.6f}")
        print(f"speedup={speedup:.2f}x")
        print(f"parity={'PASS' if parity else 'FAIL'}")
        if not parity:
            raise SystemExit("BuildBamIndex benchmark parity failed")


if __name__ == "__main__":
    main()
