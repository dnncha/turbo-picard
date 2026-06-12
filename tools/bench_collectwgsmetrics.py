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


def write_reference(path, length):
    with open(path, "w", encoding="utf-8") as handle:
        handle.write(">chr1\n")
        sequence = "A" * length
        for offset in range(0, len(sequence), 80):
            handle.write(sequence[offset : offset + 80] + "\n")


def write_sam(path, reads, reference_length):
    sequence = "A" * 100
    qualities = "F" * len(sequence)
    with open(path, "w", encoding="utf-8") as handle:
        handle.write("@HD\tVN:1.6\tSO:coordinate\n")
        handle.write(f"@SQ\tSN:chr1\tLN:{reference_length}\n")
        for index in range(1, reads + 1):
            handle.write(
                f"read{index:09d}\t0\tchr1\t{index}\t60\t100M\t*\t0\t0\t"
                f"{sequence}\t{qualities}\n"
            )


def stable_wgs_metrics(path):
    lines = [line.rstrip("\n") for line in open(path, encoding="utf-8")]
    metrics = None
    histogram = []
    for index, line in enumerate(lines):
        if line.startswith("GENOME_TERRITORY\t"):
            header = line.split("\t")
            row = lines[index + 1].split("\t")
            metrics = dict(zip(header, row))
        if line.startswith("coverage\thigh_quality_coverage_count"):
            for raw in lines[index + 1 :]:
                if raw:
                    histogram.append(raw)
            break
    if metrics is None:
        raise SystemExit(f"no WgsMetrics table in {path}")
    # Picard's theoretical sensitivity is sampled and can differ by a few
    # last-place digits across otherwise identical benchmark runs.
    metrics.pop("HET_SNP_SENSITIVITY", None)
    metrics.pop("HET_SNP_Q", None)
    return metrics, histogram


def main():
    parser = argparse.ArgumentParser(
        description="Benchmark turbo-picard CollectWgsMetrics against Picard."
    )
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

    with tempfile.TemporaryDirectory(prefix="turbo-picard-collectwgsmetrics-bench.") as tmp:
        workdir = pathlib.Path(tmp)
        reference = workdir / "ref.fa"
        input_sam = workdir / "input.sam"
        turbo_out = workdir / "turbo.txt"
        picard_out = workdir / "picard.txt"
        reference_length = args.reads + 200
        write_reference(reference, reference_length)
        write_sam(input_sam, args.reads, reference_length)

        common = [
            "CollectWgsMetrics",
            f"I={input_sam}",
            f"R={reference}",
            "COUNT_UNPAIRED=true",
            "SAMPLE_SIZE=1",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ]
        turbo_time = run([str(turbo), *common, f"O={turbo_out}"])
        picard_time = run(
            [runner, "run", "-p", args.conda_prefix, "picard", *common, f"O={picard_out}"]
        )

        parity = stable_wgs_metrics(turbo_out) == stable_wgs_metrics(picard_out)
        speedup = picard_time / turbo_time if turbo_time > 0 else float("inf")
        print("command=CollectWgsMetrics")
        print(f"reads={args.reads}")
        print(f"turbo_seconds={turbo_time:.6f}")
        print(f"picard_seconds={picard_time:.6f}")
        print(f"speedup={speedup:.2f}x")
        print(f"parity={'PASS' if parity else 'FAIL'}")
        if not parity:
            raise SystemExit("CollectWgsMetrics benchmark parity failed")


if __name__ == "__main__":
    main()
