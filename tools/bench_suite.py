#!/usr/bin/env python3
import argparse
import json
import os
import platform
import resource
import shutil
import statistics
import subprocess
import sys
import time
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def run(command):
    stdout, _ = run_profiled(command)
    return stdout


def run_profiled(command):
    usage_before = resource.getrusage(resource.RUSAGE_CHILDREN)
    start = time.perf_counter()
    timed_command = time_wrapper(command)
    completed = subprocess.run(
        timed_command,
        cwd=ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        check=False,
    )
    elapsed = time.perf_counter() - start
    usage_after = resource.getrusage(resource.RUSAGE_CHILDREN)
    if completed.returncode != 0:
        sys.stderr.write(completed.stdout)
        sys.stderr.write(completed.stderr)
        raise SystemExit(f"command failed with exit {completed.returncode}: {' '.join(command)}")
    profile = {
        "command_line": command,
        "wall_seconds": elapsed,
        "cpu_user_seconds": usage_after.ru_utime - usage_before.ru_utime,
        "cpu_system_seconds": usage_after.ru_stime - usage_before.ru_stime,
        "max_rss_kb": parse_time_max_rss_kb(completed.stderr),
    }
    return completed.stdout, profile


def time_wrapper(command):
    time_bin = shutil.which("time")
    if not time_bin:
        return command
    if platform.system() == "Darwin":
        return [time_bin, "-l", *command]
    return [time_bin, "-v", *command]


def parse_time_max_rss_kb(stderr):
    for line in stderr.splitlines():
        if "Maximum resident set size" in line:
            value = line.split(":", 1)[1].strip().split()[0]
            try:
                return int(value)
            except ValueError:
                return None
        if "maximum resident set size" in line:
            value = line.strip().split()[0]
            try:
                return int(value) // 1024
            except ValueError:
                return None
    return None


def parse_key_values(text):
    values = {}
    for line in text.splitlines():
        if "=" not in line:
            continue
        key, value = line.split("=", 1)
        values[key.strip()] = value.strip()
    return values


def run_benchmark(label, script, reads, repeats):
    rows = []
    for _ in range(repeats):
        command = ["python3", str(ROOT / "tools" / script), "--reads", str(reads), "--skip-build"]
        output, profile = run_profiled(command)
        row = parse_key_values(output)
        row["label"] = label
        row["speedup_float"] = float(row["speedup"].removesuffix("x"))
        row["turbo_seconds_float"] = float(row["turbo_seconds"])
        row["picard_seconds_float"] = float(row["picard_seconds"])
        row["profile"] = profile
        if row.get("parity") != "PASS":
            raise SystemExit(f"{label} parity failed: {output}")
        rows.append(row)
    return rows


def summarize(rows):
    speedups = [row["speedup_float"] for row in rows]
    turbo = [row["turbo_seconds_float"] for row in rows]
    picard = [row["picard_seconds_float"] for row in rows]
    return {
        "command": rows[0]["command"],
        "reads": rows[0]["reads"],
        "runs": len(rows),
        "median_turbo_seconds": statistics.median(turbo),
        "median_picard_seconds": statistics.median(picard),
        "median_speedup": statistics.median(speedups),
        "best_speedup": max(speedups),
        "parity": "PASS",
    }


def benchmark_profile(rows):
    summary = summarize(rows)
    profiles = [row["profile"] for row in rows]
    cpu_seconds = [
        profile["cpu_user_seconds"] + profile["cpu_system_seconds"] for profile in profiles
    ]
    wall_seconds = [profile["wall_seconds"] for profile in profiles]
    max_rss_kb = [profile["max_rss_kb"] for profile in profiles if profile["max_rss_kb"]]
    return {
        **summary,
        "label": rows[0]["label"],
        "median_wrapper_wall_seconds": statistics.median(wall_seconds),
        "median_wrapper_cpu_seconds": statistics.median(cpu_seconds),
        "max_observed_rss_kb": max(max_rss_kb) if max_rss_kb else None,
        "turbo_threads": os.environ.get("TURBO_PICARD_THREADS", "auto"),
        "turbo_reader_threads": os.environ.get("TURBO_PICARD_READER_THREADS", ""),
        "turbo_pipeline_reader_threads": os.environ.get(
            "TURBO_PICARD_PIPELINE_READER_THREADS", ""
        ),
        "turbo_writer_threads": os.environ.get("TURBO_PICARD_WRITER_THREADS", ""),
        "turbo_index_threads": os.environ.get("TURBO_PICARD_INDEX_THREADS", ""),
        "turbo_cmm_threads": os.environ.get("TURBO_PICARD_CMM_THREADS", ""),
        "runs_detail": [
            {
                "turbo_seconds": row["turbo_seconds_float"],
                "picard_seconds": row["picard_seconds_float"],
                "speedup": row["speedup_float"],
                "profile": row["profile"],
            }
            for row in rows
        ],
    }


def main():
    parser = argparse.ArgumentParser(description="Run parity-checked turbo-picard benchmark suite.")
    parser.add_argument("--repeats", type=int, default=5)
    parser.add_argument("--samtofastq-reads", type=int, default=50_000)
    parser.add_argument("--fastqtosam-reads", type=int, default=100_000)
    parser.add_argument("--fixmateinformation-reads", type=int, default=100_000)
    parser.add_argument("--sortsam-reads", type=int, default=100_000)
    parser.add_argument("--buildbamindex-reads", type=int, default=50_000)
    parser.add_argument("--insertsize-reads", type=int, default=100_000)
    parser.add_argument("--markduplicates-reads", type=int, default=50_000)
    parser.add_argument("--meanqualitybycycle-reads", type=int, default=100_000)
    parser.add_argument("--mergesamfiles-reads", type=int, default=50_000)
    parser.add_argument("--addorreplacereadgroups-reads", type=int, default=100_000)
    parser.add_argument("--alignmentmetrics-reads", type=int, default=100_000)
    parser.add_argument("--cleansam-reads", type=int, default=50_000)
    parser.add_argument("--basedistributionbycycle-reads", type=int, default=100_000)
    parser.add_argument("--collectgcbiasmetrics-reads", type=int, default=100_000)
    parser.add_argument("--collectwgsmetrics-reads", type=int, default=100_000)
    parser.add_argument("--qualityscoredistribution-reads", type=int, default=100_000)
    parser.add_argument("--qualityyield-reads", type=int, default=100_000)
    parser.add_argument("--collectmultiplemetrics-reads", type=int, default=100_000)
    parser.add_argument("--revertsam-reads", type=int, default=100_000)
    parser.add_argument("--setnmmdanduqtags-reads", type=int, default=100_000)
    parser.add_argument("--validatesamfile-reads", type=int, default=100_000)
    parser.add_argument("--createdict-reads", type=int, default=10_000)
    parser.add_argument("--normalizefasta-reads", type=int, default=10_000)
    parser.add_argument("--bedtointervallist-reads", type=int, default=100_000)
    parser.add_argument("--intervallisttools-reads", type=int, default=100_000)
    parser.add_argument("--gathervcfs-reads", type=int, default=100_000)
    parser.add_argument("--sortvcf-reads", type=int, default=100_000)
    parser.add_argument("--mergevcfs-reads", type=int, default=100_000)
    parser.add_argument("--liftovervcf-reads", type=int, default=100_000)
    parser.add_argument("--viewsam-reads", type=int, default=50_000)
    parser.add_argument("--replacesamheader-reads", type=int, default=50_000)
    parser.add_argument("--updatevcfdict-reads", type=int, default=100_000)
    parser.add_argument("--skip-build", action="store_true")
    parser.add_argument(
        "--profile-output",
        type=Path,
        help="Write per-command benchmark profiling JSON with wall time, CPU, RSS, thread env, and parity.",
    )
    args = parser.parse_args()

    if args.repeats < 1:
        raise SystemExit("--repeats must be >= 1")
    if not args.skip_build:
        run(["cargo", "build", "--release", "-p", "turbo-picard-cli", "--bin", "picard"])

    results = [
        run_benchmark("samtofastq", "bench_samtofastq.py", args.samtofastq_reads, args.repeats),
        run_benchmark("fastqtosam", "bench_fastqtosam.py", args.fastqtosam_reads, args.repeats),
        run_benchmark(
            "fixmateinformation",
            "bench_fixmateinformation.py",
            args.fixmateinformation_reads,
            args.repeats,
        ),
        run_benchmark("sortsam", "bench_sortsam.py", args.sortsam_reads, args.repeats),
        run_benchmark(
            "buildbamindex",
            "bench_buildbamindex.py",
            args.buildbamindex_reads,
            args.repeats,
        ),
        run_benchmark(
            "insertsize",
            "bench_insertsize.py",
            args.insertsize_reads,
            args.repeats,
        ),
        run_benchmark(
            "markduplicates",
            "bench_markduplicates_synthetic.py",
            args.markduplicates_reads,
            args.repeats,
        ),
        run_benchmark(
            "meanqualitybycycle",
            "bench_meanqualitybycycle.py",
            args.meanqualitybycycle_reads,
            args.repeats,
        ),
        run_benchmark(
            "mergesamfiles",
            "bench_mergesamfiles.py",
            args.mergesamfiles_reads,
            args.repeats,
        ),
        run_benchmark(
            "addorreplacereadgroups",
            "bench_addorreplacereadgroups.py",
            args.addorreplacereadgroups_reads,
            args.repeats,
        ),
        run_benchmark(
            "alignmentmetrics",
            "bench_alignmentmetrics.py",
            args.alignmentmetrics_reads,
            args.repeats,
        ),
        run_benchmark("cleansam", "bench_cleansam.py", args.cleansam_reads, args.repeats),
        run_benchmark(
            "basedistributionbycycle",
            "bench_collectbasedistributionbycycle.py",
            args.basedistributionbycycle_reads,
            args.repeats,
        ),
        run_benchmark(
            "collectgcbiasmetrics",
            "bench_collectgcbiasmetrics.py",
            args.collectgcbiasmetrics_reads,
            args.repeats,
        ),
        run_benchmark(
            "collectwgsmetrics",
            "bench_collectwgsmetrics.py",
            args.collectwgsmetrics_reads,
            args.repeats,
        ),
        run_benchmark(
            "qualityscoredistribution",
            "bench_qualityscoredistribution.py",
            args.qualityscoredistribution_reads,
            args.repeats,
        ),
        run_benchmark("qualityyield", "bench_qualityyield.py", args.qualityyield_reads, args.repeats),
        run_benchmark(
            "collectmultiplemetrics",
            "bench_collectmultiplemetrics.py",
            args.collectmultiplemetrics_reads,
            args.repeats,
        ),
        run_benchmark("revertsam", "bench_revertsam.py", args.revertsam_reads, args.repeats),
        run_benchmark(
            "setnmmdanduqtags",
            "bench_setnmmdanduqtags.py",
            args.setnmmdanduqtags_reads,
            args.repeats,
        ),
        run_benchmark(
            "validatesamfile",
            "bench_validatesamfile.py",
            args.validatesamfile_reads,
            args.repeats,
        ),
        run_benchmark("createdict", "bench_createdict.py", args.createdict_reads, args.repeats),
        run_benchmark(
            "normalizefasta",
            "bench_normalizefasta.py",
            args.normalizefasta_reads,
            args.repeats,
        ),
        run_benchmark(
            "bedtointervallist",
            "bench_bedtointervallist.py",
            args.bedtointervallist_reads,
            args.repeats,
        ),
        run_benchmark(
            "intervallisttools",
            "bench_intervallisttools.py",
            args.intervallisttools_reads,
            args.repeats,
        ),
        run_benchmark("gathervcfs", "bench_gathervcfs.py", args.gathervcfs_reads, args.repeats),
        run_benchmark("sortvcf", "bench_sortvcf.py", args.sortvcf_reads, args.repeats),
        run_benchmark("mergevcfs", "bench_mergevcfs.py", args.mergevcfs_reads, args.repeats),
        run_benchmark(
            "liftovervcf",
            "bench_liftovervcf.py",
            args.liftovervcf_reads,
            args.repeats,
        ),
        run_benchmark("viewsam", "bench_viewsam.py", args.viewsam_reads, args.repeats),
        run_benchmark(
            "replacesamheader",
            "bench_replacesamheader.py",
            args.replacesamheader_reads,
            args.repeats,
        ),
        run_benchmark(
            "updatevcfdict",
            "bench_updatevcfsequencedictionary.py",
            args.updatevcfdict_reads,
            args.repeats,
        ),
    ]

    for rows in results:
        summary = summarize(rows)
        print(
            "command={command} reads={reads} runs={runs} "
            "median_turbo_seconds={median_turbo_seconds:.6f} "
            "median_picard_seconds={median_picard_seconds:.6f} "
            "median_speedup={median_speedup:.2f}x best_speedup={best_speedup:.2f}x "
            "parity={parity}".format(**summary)
        )

    if args.profile_output:
        profile = {
            "schema_version": 1,
            "source": "python3 tools/bench_suite.py",
            "host": {
                "platform": platform.platform(),
                "machine": platform.machine(),
                "python": platform.python_version(),
                "cpu_count": os.cpu_count(),
            },
            "environment": {
                "TURBO_PICARD_THREADS": os.environ.get("TURBO_PICARD_THREADS", ""),
                "TURBO_PICARD_READER_THREADS": os.environ.get("TURBO_PICARD_READER_THREADS", ""),
                "TURBO_PICARD_PIPELINE_READER_THREADS": os.environ.get(
                    "TURBO_PICARD_PIPELINE_READER_THREADS", ""
                ),
                "TURBO_PICARD_WRITER_THREADS": os.environ.get("TURBO_PICARD_WRITER_THREADS", ""),
                "TURBO_PICARD_INDEX_THREADS": os.environ.get("TURBO_PICARD_INDEX_THREADS", ""),
                "TURBO_PICARD_CMM_THREADS": os.environ.get("TURBO_PICARD_CMM_THREADS", ""),
            },
            "benchmarks": [benchmark_profile(rows) for rows in results],
        }
        args.profile_output.parent.mkdir(parents=True, exist_ok=True)
        args.profile_output.write_text(json.dumps(profile, indent=2, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
