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


def write_reference(path, bases):
    with open(path, "w", encoding="utf-8") as handle:
        handle.write(">chr1\n")
        for offset in range(0, len(bases), 80):
            handle.write(bases[offset : offset + 80] + "\n")


def write_dictionary(path, length):
    with open(path, "w", encoding="utf-8") as handle:
        handle.write("@HD\tVN:1.6\n")
        handle.write(f"@SQ\tSN:chr1\tLN:{length}\n")


def write_chain(path, length):
    with open(path, "w", encoding="utf-8") as handle:
        handle.write(f"chain 100 chr1 {length} + 0 {length} chr1 {length} + 0 {length} 1\n")
        handle.write(f"{length}\n")


def write_vcf(path, records, reference_length):
    alleles = [("A", "C"), ("C", "G"), ("G", "T"), ("T", "A")]
    with open(path, "w", encoding="utf-8") as handle:
        handle.write("##fileformat=VCFv4.2\n")
        handle.write(f"##contig=<ID=chr1,length={reference_length}>\n")
        handle.write("##source=turbo-picard-benchmark\n")
        handle.write("#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\n")
        for index in range(records):
            pos = index + 1
            ref, alt = alleles[index % len(alleles)]
            handle.write(f"chr1\t{pos}\t.\t{ref}\t{alt}\t.\tPASS\t.\n")


def stable_records(path):
    with open(path, encoding="utf-8") as handle:
        return [line.rstrip("\n") for line in handle if not line.startswith("#")]


def stable_reject_filters(path):
    with open(path, encoding="utf-8") as handle:
        return [line.rstrip("\n") for line in handle if line.startswith("##FILTER=")]


def main():
    parser = argparse.ArgumentParser(description="Benchmark turbo-picard LiftoverVcf against Picard.")
    parser.add_argument("--reads", type=int, default=100_000, help="VCF records to synthesize")
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

    with tempfile.TemporaryDirectory(prefix="turbo-picard-liftovervcf-bench.") as tmp:
        workdir = pathlib.Path(tmp)
        reference_length = max(args.reads + 10, 100)
        bases = "ACGT" * ((reference_length + 3) // 4)
        bases = bases[:reference_length]
        reference = workdir / "ref.fa"
        dictionary = workdir / "ref.dict"
        chain = workdir / "identity.chain"
        input_vcf = workdir / "input.vcf"
        turbo_out = workdir / "turbo.vcf"
        turbo_reject = workdir / "turbo-reject.vcf"
        picard_out = workdir / "picard.vcf"
        picard_reject = workdir / "picard-reject.vcf"

        write_reference(reference, bases)
        write_dictionary(dictionary, reference_length)
        write_chain(chain, reference_length)
        write_vcf(input_vcf, args.reads, reference_length)

        common = [
            "LiftoverVcf",
            f"I={input_vcf}",
            f"CHAIN={chain}",
            f"R={reference}",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ]
        turbo_time = run([str(turbo), *common, f"O={turbo_out}", f"REJECT={turbo_reject}"])
        picard_time = run(
            [
                runner,
                "run",
                "-p",
                args.conda_prefix,
                "picard",
                *common,
                f"O={picard_out}",
                f"REJECT={picard_reject}",
            ]
        )

        parity = (
            stable_records(turbo_out) == stable_records(picard_out)
            and stable_records(turbo_reject) == stable_records(picard_reject)
            and stable_reject_filters(turbo_reject) == stable_reject_filters(picard_reject)
        )
        speedup = picard_time / turbo_time if turbo_time > 0 else float("inf")
        print("command=LiftoverVcf")
        print(f"reads={args.reads}")
        print(f"turbo_seconds={turbo_time:.6f}")
        print(f"picard_seconds={picard_time:.6f}")
        print(f"speedup={speedup:.2f}x")
        print(f"parity={'PASS' if parity else 'FAIL'}")
        if not parity:
            raise SystemExit("LiftoverVcf benchmark parity failed")


if __name__ == "__main__":
    main()
