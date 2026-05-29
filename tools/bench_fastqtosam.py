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


def run(command, *, cwd=ROOT, env=None):
    start = time.perf_counter()
    completed = subprocess.run(
        command,
        cwd=cwd,
        env=env,
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


def write_fastq_pair(workdir, reads):
    r1 = workdir / "r1.fastq"
    r2 = workdir / "r2.fastq"
    bases = ("ACGT" * 38).encode()
    mate_bases = ("TGCA" * 38).encode()
    qual = ("F" * 152).encode()
    with r1.open("wb") as first, r2.open("wb") as second:
        for index in range(reads):
            name = f"read{index:09d}".encode()
            first.write(b"@" + name + b"\n" + bases + b"\n+\n" + qual + b"\n")
            second.write(b"@" + name + b"\n" + mate_bases + b"\n+\n" + qual + b"\n")
    return r1, r2


def stable_sam(path):
    rows = []
    with open(path, encoding="utf-8") as handle:
        for line in handle:
            line = line.rstrip("\n")
            if line.startswith("@HD") or line.startswith("@RG") or not line.startswith("@"):
                rows.append(line)
    return rows


def main():
    parser = argparse.ArgumentParser(description="Benchmark turbo-picard FastqToSam against Picard.")
    parser.add_argument("--reads", type=int, default=100_000, help="paired reads to synthesize")
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

    with tempfile.TemporaryDirectory(prefix="turbo-picard-fastqtosam-bench.") as tmp:
        workdir = pathlib.Path(tmp)
        r1, r2 = write_fastq_pair(workdir, args.reads)
        turbo_out = workdir / "turbo.sam"
        picard_out = workdir / "picard.sam"

        common = [
            f"F1={r1}",
            f"F2={r2}",
            "SM=sample",
            "RG=rg1",
            "LB=lib",
            "PL=ILLUMINA",
            "PU=unit",
            "QUALITY_FORMAT=Standard",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ]
        turbo_time, _ = run([str(turbo), "FastqToSam", f"O={turbo_out}", *common])
        picard_time, _ = run(
            [runner, "run", "-p", args.conda_prefix, "picard", "FastqToSam", f"O={picard_out}", *common]
        )

        parity = stable_sam(turbo_out) == stable_sam(picard_out)
        speedup = picard_time / turbo_time if turbo_time > 0 else float("inf")
        print(f"command=FastqToSam")
        print(f"reads={args.reads}")
        print(f"turbo_seconds={turbo_time:.6f}")
        print(f"picard_seconds={picard_time:.6f}")
        print(f"speedup={speedup:.2f}x")
        print(f"parity={'PASS' if parity else 'FAIL'}")
        if not parity:
            raise SystemExit("FastqToSam benchmark parity failed")


if __name__ == "__main__":
    main()
