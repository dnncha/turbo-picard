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


def write_sam(path, reads):
    sequence = "ACGT" * 25
    qualities = "F" * len(sequence)
    original_qualities = "J" * len(sequence)
    with open(path, "w", encoding="utf-8") as handle:
        handle.write("@HD\tVN:1.6\tSO:coordinate\n")
        handle.write(f"@SQ\tSN:chr1\tLN:{reads + 200}\n")
        handle.write("@RG\tID:rg1\tSM:sample1\tLB:lib1\tPL:ILLUMINA\n")
        handle.write("@PG\tID:aligner\tPN:aligner\tVN:1.0\n")
        for index in range(1, reads + 1):
            flag = 1024 if index % 17 == 0 else 0
            handle.write(
                f"read{index:09d}\t{flag}\tchr1\t{index}\t60\t100M\t*\t0\t0\t"
                f"{sequence}\t{qualities}\tRG:Z:rg1\tPG:Z:aligner\tNM:i:0\tMD:Z:100\t"
                f"UQ:i:0\tMQ:i:60\tOQ:Z:{original_qualities}\n"
            )


def stable_sam(path):
    with open(path, encoding="utf-8") as handle:
        return [
            line.rstrip("\n")
            for line in handle
            if line.strip() and not line.startswith("@PG")
        ]


def main():
    parser = argparse.ArgumentParser(description="Benchmark turbo-picard RevertSam against Picard.")
    parser.add_argument("--reads", type=int, default=100_000, help="reads to synthesize")
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

    with tempfile.TemporaryDirectory(prefix="turbo-picard-revertsam-bench.") as tmp:
        workdir = pathlib.Path(tmp)
        input_sam = workdir / "input.sam"
        turbo_out = workdir / "turbo.bam"
        picard_out = workdir / "picard.bam"
        turbo_sam = workdir / "turbo.sam"
        picard_sam = workdir / "picard.sam"
        write_sam(input_sam, args.reads)

        common = [
            "RevertSam",
            f"I={input_sam}",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ]
        turbo_time = run([str(turbo), *common, f"O={turbo_out}"])
        picard_time = run(
            [runner, "run", "-p", args.conda_prefix, "picard", *common, f"O={picard_out}"]
        )
        run([str(turbo), "ViewSam", f"I={turbo_out}", f"O={turbo_sam}"])
        run([str(turbo), "ViewSam", f"I={picard_out}", f"O={picard_sam}"])

        parity = stable_sam(turbo_sam) == stable_sam(picard_sam)
        speedup = picard_time / turbo_time if turbo_time > 0 else float("inf")
        print("command=RevertSam")
        print(f"reads={args.reads}")
        print(f"turbo_seconds={turbo_time:.6f}")
        print(f"picard_seconds={picard_time:.6f}")
        print(f"speedup={speedup:.2f}x")
        print(f"parity={'PASS' if parity else 'FAIL'}")
        if not parity:
            raise SystemExit("RevertSam benchmark parity failed")


if __name__ == "__main__":
    main()
