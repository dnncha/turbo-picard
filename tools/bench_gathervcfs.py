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


def write_vcf(path, start, count):
    with open(path, "w", encoding="utf-8") as handle:
        handle.write("##fileformat=VCFv4.2\n")
        handle.write("##contig=<ID=chr1,length=100000000>\n")
        handle.write("#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\n")
        for offset in range(count):
            pos = start + offset + 1
            handle.write(f"chr1\t{pos}\t.\tA\tC\t.\tPASS\t.\n")


def records(path):
    with open(path, encoding="utf-8") as handle:
        return [line.rstrip("\n") for line in handle if not line.startswith("#")]


def main():
    parser = argparse.ArgumentParser(description="Benchmark turbo-picard GatherVcfs against Picard.")
    parser.add_argument("--reads", type=int, default=100_000, help="VCF records to synthesize")
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

    with tempfile.TemporaryDirectory(prefix="turbo-picard-gathervcfs-bench.") as tmp:
        workdir = pathlib.Path(tmp)
        first = workdir / "first.vcf"
        second = workdir / "second.vcf"
        split = args.reads // 2
        write_vcf(first, 0, split)
        write_vcf(second, split, args.reads - split)
        turbo_out = workdir / "turbo.vcf"
        picard_out = workdir / "picard.vcf"

        common = [
            "GatherVcfs",
            f"I={first}",
            f"I={second}",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ]
        turbo_time = run([str(turbo), *common, f"O={turbo_out}"])
        picard_time = run([runner, "run", "-p", args.conda_prefix, "picard", *common, f"O={picard_out}"])

        parity = records(turbo_out) == records(picard_out)
        speedup = picard_time / turbo_time if turbo_time > 0 else float("inf")
        print("command=GatherVcfs")
        print(f"reads={args.reads}")
        print(f"turbo_seconds={turbo_time:.6f}")
        print(f"picard_seconds={picard_time:.6f}")
        print(f"speedup={speedup:.2f}x")
        print(f"parity={'PASS' if parity else 'FAIL'}")
        if not parity:
            raise SystemExit("GatherVcfs benchmark parity failed")


if __name__ == "__main__":
    main()
