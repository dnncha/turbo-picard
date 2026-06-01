#!/usr/bin/env python3
import argparse
import os
import pathlib
import shutil
import struct
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


def read_u32(data, offset):
    return struct.unpack_from("<I", data, offset)[0], offset + 4


def read_u64(data, offset):
    return struct.unpack_from("<Q", data, offset)[0], offset + 8


def bai_idxstats_summary(path):
    data = path.read_bytes()
    if len(data) < 8 or data[:4] != b"BAI\1":
        raise SystemExit(f"{path} is not a BAI index")
    offset = 4
    reference_count, offset = read_u32(data, offset)
    references = []
    for _ in range(reference_count):
        bin_count, offset = read_u32(data, offset)
        mapped = None
        unmapped = None
        regular_bin_count = 0
        for _ in range(bin_count):
            bin_id, offset = read_u32(data, offset)
            chunk_count, offset = read_u32(data, offset)
            chunks = []
            for _ in range(chunk_count):
                chunk_begin, offset = read_u64(data, offset)
                chunk_end, offset = read_u64(data, offset)
                chunks.append((chunk_begin, chunk_end))
            if bin_id == 37450 and len(chunks) >= 2:
                mapped, unmapped = chunks[1]
            elif bin_id != 37450:
                regular_bin_count += 1
        linear_count, offset = read_u32(data, offset)
        for _ in range(linear_count):
            _, offset = read_u64(data, offset)
        if regular_bin_count == 0 or linear_count == 0:
            raise SystemExit(f"{path} has no usable BAI bins for a mapped reference")
        references.append((mapped, unmapped))
    trailing = data[offset:]
    no_coordinate_count = struct.unpack("<Q", trailing[:8])[0] if len(trailing) == 8 else None
    return reference_count, tuple(references), no_coordinate_count


def main():
    parser = argparse.ArgumentParser(description="Benchmark turbo-picard BuildBamIndex against Picard.")
    parser.add_argument("--reads", type=int, default=300_000, help="reads to synthesize")
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

        turbo_summary = bai_idxstats_summary(turbo_index)
        picard_summary = bai_idxstats_summary(picard_index)
        parity = turbo_summary == picard_summary and turbo_summary[0] > 0
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
