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


BENCHMARK_SPECS = [
    ("samtofastq", "bench_samtofastq.py", "samtofastq_reads"),
    ("fastqtosam", "bench_fastqtosam.py", "fastqtosam_reads"),
    ("fixmateinformation", "bench_fixmateinformation.py", "fixmateinformation_reads"),
    ("sortsam", "bench_sortsam.py", "sortsam_reads"),
    ("buildbamindex", "bench_buildbamindex.py", "buildbamindex_reads"),
    ("insertsize", "bench_insertsize.py", "insertsize_reads"),
    ("markduplicates", "bench_markduplicates_synthetic.py", "markduplicates_reads"),
    ("meanqualitybycycle", "bench_meanqualitybycycle.py", "meanqualitybycycle_reads"),
    ("mergesamfiles", "bench_mergesamfiles.py", "mergesamfiles_reads"),
    ("addorreplacereadgroups", "bench_addorreplacereadgroups.py", "addorreplacereadgroups_reads"),
    ("alignmentmetrics", "bench_alignmentmetrics.py", "alignmentmetrics_reads"),
    ("cleansam", "bench_cleansam.py", "cleansam_reads"),
    (
        "basedistributionbycycle",
        "bench_collectbasedistributionbycycle.py",
        "basedistributionbycycle_reads",
    ),
    ("collectgcbiasmetrics", "bench_collectgcbiasmetrics.py", "collectgcbiasmetrics_reads"),
    ("collectwgsmetrics", "bench_collectwgsmetrics.py", "collectwgsmetrics_reads"),
    (
        "qualityscoredistribution",
        "bench_qualityscoredistribution.py",
        "qualityscoredistribution_reads",
    ),
    ("qualityyield", "bench_qualityyield.py", "qualityyield_reads"),
    ("collectmultiplemetrics", "bench_collectmultiplemetrics.py", "collectmultiplemetrics_reads"),
    ("revertsam", "bench_revertsam.py", "revertsam_reads"),
    ("setnmmdanduqtags", "bench_setnmmdanduqtags.py", "setnmmdanduqtags_reads"),
    ("validatesamfile", "bench_validatesamfile.py", "validatesamfile_reads"),
    ("createdict", "bench_createdict.py", "createdict_reads"),
    ("normalizefasta", "bench_normalizefasta.py", "normalizefasta_reads"),
    ("bedtointervallist", "bench_bedtointervallist.py", "bedtointervallist_reads"),
    ("intervallisttools", "bench_intervallisttools.py", "intervallisttools_reads"),
    ("gathervcfs", "bench_gathervcfs.py", "gathervcfs_reads"),
    ("sortvcf", "bench_sortvcf.py", "sortvcf_reads"),
    ("mergevcfs", "bench_mergevcfs.py", "mergevcfs_reads"),
    ("liftovervcf", "bench_liftovervcf.py", "liftovervcf_reads"),
    ("viewsam", "bench_viewsam.py", "viewsam_reads"),
    ("replacesamheader", "bench_replacesamheader.py", "replacesamheader_reads"),
    ("updatevcfdict", "bench_updatevcfsequencedictionary.py", "updatevcfdict_reads"),
]


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
    parser.add_argument(
        "--only",
        action="append",
        default=[],
        metavar="LABEL[,LABEL...]",
        help=(
            "Run only selected benchmark labels. Repeat or comma-separate values; "
            "labels include revertsam, setnmmdanduqtags, and qualityscoredistribution."
        ),
    )
    args = parser.parse_args()

    if args.repeats < 1:
        raise SystemExit("--repeats must be >= 1")
    if not args.skip_build:
        run(["cargo", "build", "--release", "-p", "turbo-picard-cli", "--bin", "picard"])

    selected_labels = {
        label.strip().lower()
        for value in args.only
        for label in value.split(",")
        if label.strip()
    }
    known_labels = {label for label, _script, _reads_attr in BENCHMARK_SPECS}
    unknown_labels = selected_labels - known_labels
    if unknown_labels:
        raise SystemExit(
            "--only contains unknown benchmark label(s): "
            + ", ".join(sorted(unknown_labels))
            + ". Known labels: "
            + ", ".join(sorted(known_labels))
        )

    results = [
        run_benchmark(label, script, getattr(args, reads_attr), args.repeats)
        for label, script, reads_attr in BENCHMARK_SPECS
        if not selected_labels or label in selected_labels
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
