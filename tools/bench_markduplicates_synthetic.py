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


def write_sam(path, reads, duplicate_family_size):
    sequence = "ACGT" * 25
    qualities = "F" * len(sequence)
    with open(path, "w", encoding="utf-8") as handle:
        handle.write("@HD\tVN:1.6\tSO:coordinate\n")
        handle.write(f"@SQ\tSN:chr1\tLN:{reads + 1000}\n")
        for index in range(reads):
            group = index // duplicate_family_size
            pos = 1 + group * 2
            member = index % duplicate_family_size
            name = f"INST:RUN:FLOW:1:{group + 1}:{member % 101}:{member // 101}"
            handle.write(
                f"{name}\t0\tchr1\t{pos}\t60\t100M\t*\t0\t0\t{sequence}\t{qualities}\n"
            )


def main():
    parser = argparse.ArgumentParser(
        description="Benchmark turbo-picard MarkDuplicates against Picard on synthetic SAM/BAM."
    )
    parser.add_argument("--reads", type=int, default=50_000, help="reads to synthesize")
    parser.add_argument(
        "--duplicate-family-size",
        type=int,
        default=4,
        help="records sharing a duplicate key; use 1024+ to stress large families",
    )
    parser.add_argument(
        "--input-format",
        choices=("bam", "sam"),
        default="bam",
        help="benchmark input format; BAM exercises the production native path",
    )
    parser.add_argument(
        "--conda-prefix",
        default=os.environ.get("TURBO_PICARD_CONDA_PREFIX", str(ROOT / ".conda-turbo-picard")),
        help="conda environment prefix containing Picard",
    )
    parser.add_argument("--skip-build", action="store_true", help="reuse existing release binary")
    args = parser.parse_args()
    if args.reads < 1:
        parser.error("--reads must be positive")
    if args.duplicate_family_size < 1:
        parser.error("--duplicate-family-size must be positive")

    if not args.skip_build:
        run(["cargo", "build", "--release", "-p", "turbo-picard-cli", "--bin", "picard"])

    turbo = ROOT / "target" / "release" / "picard"
    if not turbo.exists():
        raise SystemExit(f"missing release binary: {turbo}")
    runner = conda_runner()

    with tempfile.TemporaryDirectory(prefix="turbo-picard-markduplicates-bench.") as tmp:
        workdir = pathlib.Path(tmp)
        input_sam = workdir / "input.sam"
        input_bam = workdir / "input.bam"
        turbo_out = workdir / "turbo.sam"
        picard_out = workdir / "picard.sam"
        turbo_metrics = workdir / "turbo.metrics.txt"
        picard_metrics = workdir / "picard.metrics.txt"
        write_sam(input_sam, args.reads, args.duplicate_family_size)
        benchmark_input = input_sam
        if args.input_format == "bam":
            run(
                [
                    str(turbo),
                    "SortSam",
                    f"I={input_sam}",
                    f"O={input_bam}",
                    "SORT_ORDER=coordinate",
                    "QUIET=true",
                ]
            )
            benchmark_input = input_bam

        common = [
            "MarkDuplicates",
            f"I={benchmark_input}",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ]
        turbo_time = run([str(turbo), *common, f"O={turbo_out}", f"M={turbo_metrics}"])
        picard_time = run(
            [
                runner,
                "run",
                "-p",
                args.conda_prefix,
                "picard",
                *common,
                f"O={picard_out}",
                f"M={picard_metrics}",
            ]
        )
        compare = subprocess.run(
            [
                "python3",
                str(ROOT / "tools" / "compare_markduplicates.py"),
                "--picard-bam",
                str(picard_out),
                "--turbo-picard-bam",
                str(turbo_out),
                "--picard-metrics",
                str(picard_metrics),
                "--turbo-picard-metrics",
                str(turbo_metrics),
            ],
            cwd=ROOT,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            check=False,
        )
        parity = compare.returncode == 0
        speedup = picard_time / turbo_time if turbo_time > 0 else float("inf")
        print("command=MarkDuplicates")
        print(f"reads={args.reads}")
        print(f"input_format={args.input_format}")
        print(f"duplicate_family_size={args.duplicate_family_size}")
        print(f"turbo_seconds={turbo_time:.6f}")
        print(f"picard_seconds={picard_time:.6f}")
        print(f"speedup={speedup:.2f}x")
        print(f"parity={'PASS' if parity else 'FAIL'}")
        if not parity:
            sys.stderr.write(compare.stdout)
            sys.stderr.write(compare.stderr)
            raise SystemExit("MarkDuplicates benchmark parity failed")


if __name__ == "__main__":
    main()
