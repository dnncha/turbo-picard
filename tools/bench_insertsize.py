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


def write_sam(path, pairs):
    sequence = "ACGT" * 25
    qualities = "F" * len(sequence)
    ref_len = pairs * 2 + len(sequence) + 100
    with open(path, "w", encoding="utf-8") as handle:
        handle.write("@HD\tVN:1.6\tSO:coordinate\n")
        handle.write(f"@SQ\tSN:chr1\tLN:{ref_len}\n")
        for index in range(1, pairs + 1):
            start = (index * 2) - 1
            insert_size = 150 + (index % 17)
            mate_start = start + insert_size - len(sequence)
            duplicate_flag = 1024 if index % 20 == 0 else 0
            handle.write(
                f"pair{index:09d}\t{99 | duplicate_flag}\tchr1\t{start}\t60\t100M\t=\t{mate_start}\t{insert_size}\t{sequence}\t{qualities}\n"
            )
            handle.write(
                f"pair{index:09d}\t{147 | duplicate_flag}\tchr1\t{mate_start}\t60\t100M\t=\t{start}\t-{insert_size}\t{sequence}\t{qualities}\n"
            )


def write_fake_rscript(path):
    path.write_text("#!/usr/bin/env sh\nexit 0\n", encoding="utf-8")
    path.chmod(0o755)


def stable_sections(path):
    lines = [line.rstrip("\n") for line in open(path, encoding="utf-8")]
    metrics = None
    histogram = []
    for index, line in enumerate(lines):
        if line.startswith("MEDIAN_INSERT_SIZE\t"):
            metrics = (line, lines[index + 1])
        if line == "insert_size\tAll_Reads.fr_count":
            cursor = index + 1
            while cursor < len(lines) and lines[cursor]:
                histogram.append(lines[cursor])
                cursor += 1
    if metrics is None:
        raise SystemExit(f"no insert-size metrics table in {path}")
    return metrics, histogram


def main():
    parser = argparse.ArgumentParser(
        description="Benchmark turbo-picard CollectInsertSizeMetrics against Picard."
    )
    parser.add_argument("--reads", type=int, default=100_000, help="reads to synthesize")
    parser.add_argument(
        "--conda-prefix",
        default=os.environ.get("TURBO_PICARD_CONDA_PREFIX", str(ROOT / ".conda-turbo-picard")),
        help="conda environment prefix containing Picard",
    )
    parser.add_argument("--skip-build", action="store_true", help="reuse existing release binary")
    args = parser.parse_args()

    if args.reads < 2:
        raise SystemExit("--reads must be >= 2")
    if not args.skip_build:
        run(["cargo", "build", "--release", "-p", "turbo-picard-cli", "--bin", "picard"])

    turbo = ROOT / "target" / "release" / "picard"
    if not turbo.exists():
        raise SystemExit(f"missing release binary: {turbo}")
    runner = conda_runner()

    pairs = args.reads // 2
    with tempfile.TemporaryDirectory(prefix="turbo-picard-insertsize-bench.") as tmp:
        workdir = pathlib.Path(tmp)
        input_sam = workdir / "input.sam"
        turbo_out = workdir / "turbo.txt"
        turbo_pdf = workdir / "turbo.pdf"
        picard_out = workdir / "picard.txt"
        picard_pdf = workdir / "picard.pdf"
        fake_rscript = workdir / "Rscript"
        write_sam(input_sam, pairs)
        write_fake_rscript(fake_rscript)

        common = [
            "CollectInsertSizeMetrics",
            f"I={input_sam}",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ]
        turbo_time = run([str(turbo), *common, f"O={turbo_out}", f"H={turbo_pdf}"])
        picard_time = run(
            [
                runner,
                "run",
                "-p",
                args.conda_prefix,
                "env",
                f"PATH={workdir}:{args.conda_prefix}/bin:{os.environ.get('PATH', '')}",
                "picard",
                *common,
                f"O={picard_out}",
                f"H={picard_pdf}",
            ]
        )

        parity = stable_sections(turbo_out) == stable_sections(picard_out)
        speedup = picard_time / turbo_time if turbo_time > 0 else float("inf")
        print("command=CollectInsertSizeMetrics")
        print(f"reads={pairs * 2}")
        print(f"turbo_seconds={turbo_time:.6f}")
        print(f"picard_seconds={picard_time:.6f}")
        print(f"speedup={speedup:.2f}x")
        print(f"parity={'PASS' if parity else 'FAIL'}")
        if not parity:
            raise SystemExit("CollectInsertSizeMetrics benchmark parity failed")


if __name__ == "__main__":
    main()
