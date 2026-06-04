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


DETAIL_HEADER = (
    "ACCUMULATION_LEVEL\tREADS_USED\tGC\tWINDOWS\tREAD_STARTS\t"
    "MEAN_BASE_QUALITY\tNORMALIZED_COVERAGE\tERROR_BAR_WIDTH\tSAMPLE\tLIBRARY\tREAD_GROUP"
)
SUMMARY_HEADER = (
    "ACCUMULATION_LEVEL\tREADS_USED\tWINDOW_SIZE\tTOTAL_CLUSTERS\tALIGNED_READS\t"
    "AT_DROPOUT\tGC_DROPOUT\tGC_NC_0_19\tGC_NC_20_39\tGC_NC_40_59\t"
    "GC_NC_60_79\tGC_NC_80_100\tSAMPLE\tLIBRARY\tREAD_GROUP"
)


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
    return elapsed


def conda_runner():
    for name in ("mamba", "micromamba"):
        path = shutil.which(name)
        if path:
            return path
    raise SystemExit("mamba or micromamba is required to benchmark against Picard")


def write_rscript_stub(path):
    path.write_text("#!/usr/bin/env bash\nexit 0\n", encoding="utf-8")
    path.chmod(0o755)


def write_reference(path, length):
    with open(path, "w", encoding="utf-8") as handle:
        handle.write(">low\n")
        handle.write("A" * length + "\n")
        handle.write(">high\n")
        handle.write("C" * length + "\n")


def write_sam(path, reads, reference_length):
    low_sequence = "A" * 20
    high_sequence = "C" * 20
    qualities = "F" * 20
    with open(path, "w", encoding="utf-8") as handle:
        handle.write("@HD\tVN:1.6\tSO:coordinate\n")
        handle.write(f"@SQ\tSN:low\tLN:{reference_length}\n")
        handle.write(f"@SQ\tSN:high\tLN:{reference_length}\n")
        low_reads = reads // 2
        high_reads = reads - low_reads
        for index in range(1, low_reads + 1):
            flag = 1024 if index % 17 == 0 else 0
            handle.write(
                f"low{index:09d}\t{flag}\tlow\t{index}\t60\t20M\t*\t0\t0\t{low_sequence}\t{qualities}\n"
            )
        for index in range(1, high_reads + 1):
            flag = 1024 if (low_reads + index) % 17 == 0 else 0
            handle.write(
                f"high{index:09d}\t{flag}\thigh\t{index}\t60\t20M\t*\t0\t0\t{high_sequence}\t{qualities}\n"
            )


def stable_table(path, header):
    lines = [line.rstrip("\n") for line in open(path, encoding="utf-8")]
    for index, line in enumerate(lines):
        if line == header:
            rows = []
            cursor = index
            while cursor < len(lines) and lines[cursor]:
                rows.append(lines[cursor])
                cursor += 1
            return rows
    raise SystemExit(f"missing table {header!r} in {path}")


def main():
    parser = argparse.ArgumentParser(
        description="Benchmark turbo-picard CollectGcBiasMetrics against Picard."
    )
    parser.add_argument("--reads", type=int, default=100_000, help="reads to synthesize")
    parser.add_argument(
        "--conda-prefix",
        default=os.environ.get("TURBO_PICARD_CONDA_PREFIX", str(ROOT / ".conda-turbo-picard")),
        help="conda environment prefix containing Picard",
    )
    parser.add_argument("--skip-build", action="store_true", help="reuse existing release binary")
    args = parser.parse_args()

    if args.reads < 1:
        raise SystemExit("--reads must be >= 1")
    if not args.skip_build:
        run(["cargo", "build", "--release", "-p", "turbo-picard-cli", "--bin", "picard"])

    turbo = ROOT / "target" / "release" / "picard"
    if not turbo.exists():
        raise SystemExit(f"missing release binary: {turbo}")
    runner = conda_runner()

    with tempfile.TemporaryDirectory(prefix="turbo-picard-collectgcbiasmetrics-bench.") as tmp:
        workdir = pathlib.Path(tmp)
        rscript = workdir / "Rscript"
        reference = workdir / "ref.fa"
        input_sam = workdir / "input.sam"
        turbo_detail = workdir / "turbo.detail.txt"
        turbo_summary = workdir / "turbo.summary.txt"
        turbo_chart = workdir / "turbo.pdf"
        picard_detail = workdir / "picard.detail.txt"
        picard_summary = workdir / "picard.summary.txt"
        picard_chart = workdir / "picard.pdf"

        write_rscript_stub(rscript)
        reference_length = max(args.reads // 2 + 100, 1000)
        write_reference(reference, reference_length)
        write_sam(input_sam, args.reads, reference_length)

        common = [
            "CollectGcBiasMetrics",
            f"I={input_sam}",
            f"R={reference}",
            "SCAN_WINDOW_SIZE=20",
            "MINIMUM_GENOME_FRACTION=0",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ]
        turbo_time = run(
            [
                str(turbo),
                *common,
                f"O={turbo_detail}",
                f"S={turbo_summary}",
                f"CHART={turbo_chart}",
            ]
        )
        env = os.environ.copy()
        env["PATH"] = f"{workdir}:{args.conda_prefix}/bin:{env.get('PATH', '')}"
        picard_time = run(
            [
                runner,
                "run",
                "-p",
                args.conda_prefix,
                "env",
                f"PATH={env['PATH']}",
                "picard",
                *common,
                f"O={picard_detail}",
                f"S={picard_summary}",
                f"CHART={picard_chart}",
            ],
            env=env,
        )

        parity = (
            stable_table(turbo_detail, DETAIL_HEADER) == stable_table(picard_detail, DETAIL_HEADER)
            and stable_table(turbo_summary, SUMMARY_HEADER)
            == stable_table(picard_summary, SUMMARY_HEADER)
        )
        speedup = picard_time / turbo_time if turbo_time > 0 else float("inf")
        print("command=CollectGcBiasMetrics")
        print(f"reads={args.reads}")
        print(f"turbo_seconds={turbo_time:.6f}")
        print(f"picard_seconds={picard_time:.6f}")
        print(f"speedup={speedup:.2f}x")
        print(f"parity={'PASS' if parity else 'FAIL'}")
        if not parity:
            raise SystemExit("CollectGcBiasMetrics benchmark parity failed")


if __name__ == "__main__":
    main()
