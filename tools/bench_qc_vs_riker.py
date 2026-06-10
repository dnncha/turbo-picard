#!/usr/bin/env python3
"""Three-way QC benchmark: Picard vs turbo-picard vs riker.

Runs overlapping whole-genome and bundle QC profiles on the same BAM and
writes a TSV plus a short Markdown summary. Riker is required for canonical
three-way evidence; use --allow-missing-riker for Picard-vs-turbo smoke runs
when riker is not installed.

Smoke mode uses the pinned GATK NA12878 mitochondrial fixture in this repo.
For WGS-scale comparisons, point --input-bam at a staged 1000 Genomes BAM and
see benchmarks/riker-comparison/README.md.
"""

from __future__ import annotations

import argparse
import json
import os
import shlex
import shutil
import statistics
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Callable


ROOT = Path(__file__).resolve().parents[1]


@dataclass(frozen=True)
class ToolRun:
    family: str
    label: str
    command: list[str]
    workdir: Path
    env: dict[str, str] | None = None


@dataclass(frozen=True)
class Profile:
    name: str
    description: str
    picard_runs: Callable[[Path, Path, Path, Path], list[ToolRun]]
    riker_run: Callable[[Path, Path, Path, Path], ToolRun | None]
    turbo_runs: Callable[[Path, Path, Path, Path], list[ToolRun]]


def parse_command(value: str) -> list[str]:
    return shlex.split(value)


def which_or_none(name: str) -> str | None:
    return shutil.which(name)


def conda_runner() -> list[str] | None:
    for name in ("mamba", "micromamba"):
        if shutil.which(name):
            return [name]
    return None


def gnu_time_path() -> str | None:
    for candidate in ("/usr/bin/time", "/bin/time"):
        if Path(candidate).is_file():
            return candidate
    return None


def parse_max_rss_kb(stderr: str) -> int | None:
    for line in stderr.splitlines():
        if "Maximum resident set size" in line:
            parts = line.split(":", 1)
            if len(parts) == 2:
                value = parts[1].strip().split()[0]
                try:
                    return int(value)
                except ValueError:
                    return None
    return None


def run_timed(
    command: list[str],
    *,
    cwd: Path,
    env_overrides: dict[str, str] | None = None,
    measure_rss: bool = False,
) -> tuple[float, int, int | None]:
    env = os.environ.copy()
    if env_overrides:
        env.update(env_overrides)
    if measure_rss and (time_bin := gnu_time_path()) is not None:
        wrapped = [time_bin, "-v", *command]
        start = time.perf_counter()
        completed = subprocess.run(
            wrapped,
            cwd=cwd,
            env=env,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            check=False,
        )
        elapsed = time.perf_counter() - start
        max_rss_kb = parse_max_rss_kb(completed.stderr)
        if completed.returncode != 0:
            sys.stderr.write(completed.stdout)
            sys.stderr.write(completed.stderr)
            raise SystemExit(
                f"command failed with exit {completed.returncode}: {' '.join(command)}"
            )
        return elapsed, completed.returncode, max_rss_kb

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
        raise SystemExit(
            f"command failed with exit {completed.returncode}: {' '.join(command)}"
        )
    return elapsed, completed.returncode, None


def default_picard_command(conda_prefix: Path) -> list[str]:
    runner = conda_runner()
    if runner is None:
        picard = which_or_none("picard")
        if picard is None:
            raise SystemExit("mamba/micromamba or picard on PATH is required for Picard runs")
        return [picard]
    return [*runner, "run", "-p", str(conda_prefix), "picard"]


def default_turbo_command() -> list[str]:
    release = ROOT / "target" / "release" / "picard"
    if release.exists():
        return [str(release)]
    return ["cargo", "run", "-q", "-p", "turbo-picard-cli", "--bin", "picard", "--"]


def default_riker_command() -> list[str] | None:
    path = which_or_none("riker")
    if path is None:
        return None
    return [path]


def silent_args() -> list[str]:
    return ["VALIDATION_STRINGENCY=SILENT", "QUIET=true"]


def wgs_only_runs(workdir: Path, input_bam: Path, reference: Path, prefix: Path) -> list[ToolRun]:
    return [
        ToolRun(
            family="picard",
            label="CollectWgsMetrics",
            command=[
                *prefix_picard,
                "CollectWgsMetrics",
                f"I={input_bam}",
                f"O={workdir / 'picard.wgs.txt'}",
                f"R={reference}",
                "COUNT_UNPAIRED=true",
                *silent_args(),
            ],
            workdir=workdir,
        )
    ]


def wgs_bundle_picard_runs(
    workdir: Path, input_bam: Path, reference: Path, prefix: Path
) -> list[ToolRun]:
    return [
        ToolRun(
            family="picard",
            label="CollectMultipleMetrics",
            command=[
                *prefix_picard,
                "CollectMultipleMetrics",
                f"I={input_bam}",
                f"O={workdir / 'picard.bundle'}",
                "PROGRAM=CollectAlignmentSummaryMetrics",
                "PROGRAM=CollectInsertSizeMetrics",
                "PROGRAM=CollectBaseDistributionByCycle",
                "PROGRAM=MeanQualityByCycle",
                "PROGRAM=QualityScoreDistribution",
                *silent_args(),
            ],
            workdir=workdir,
        ),
        ToolRun(
            family="picard",
            label="CollectGcBiasMetrics",
            command=[
                *prefix_picard,
                "CollectGcBiasMetrics",
                f"I={input_bam}",
                f"O={workdir / 'picard.gc.detail.txt'}",
                f"S={workdir / 'picard.gc.summary.txt'}",
                f"CHART={workdir / 'picard.gc.pdf'}",
                f"R={reference}",
                *silent_args(),
            ],
            workdir=workdir,
        ),
        ToolRun(
            family="picard",
            label="CollectWgsMetrics",
            command=[
                *prefix_picard,
                "CollectWgsMetrics",
                f"I={input_bam}",
                f"O={workdir / 'picard.wgs.txt'}",
                f"R={reference}",
                "COUNT_UNPAIRED=true",
                *silent_args(),
            ],
            workdir=workdir,
        ),
    ]


prefix_picard: list[str] = []
prefix_turbo: list[str] = []
prefix_riker: list[str] | None = None
riker_threads: int = 1


def bind_prefixes(
    picard_command: list[str],
    turbo_command: list[str],
    riker_command: list[str] | None,
    *,
    threads: int = 1,
) -> None:
    global prefix_picard, prefix_turbo, prefix_riker, riker_threads
    prefix_picard = picard_command
    prefix_turbo = turbo_command
    prefix_riker = riker_command
    riker_threads = max(1, threads)


def wgs_only_turbo_runs(workdir: Path, input_bam: Path, reference: Path, _: Path) -> list[ToolRun]:
    return [
        ToolRun(
            family="turbo-picard",
            label="CollectWgsMetrics",
            command=[
                *prefix_turbo,
                "CollectWgsMetrics",
                f"I={input_bam}",
                f"O={workdir / 'turbo.wgs.txt'}",
                f"R={reference}",
                "COUNT_UNPAIRED=true",
                *silent_args(),
            ],
            workdir=workdir,
        )
    ]


def wgs_only_turbo_fast_runs(
    workdir: Path, input_bam: Path, reference: Path, _: Path
) -> list[ToolRun]:
    return [
        ToolRun(
            family="turbo-picard",
            label="CollectWgsMetrics(use_fast_algorithm)",
            command=[
                *prefix_turbo,
                "CollectWgsMetrics",
                f"I={input_bam}",
                f"O={workdir / 'turbo.wgs.txt'}",
                f"R={reference}",
                "COUNT_UNPAIRED=true",
                "USE_FAST_ALGORITHM=true",
                *silent_args(),
            ],
            workdir=workdir,
        )
    ]


def wgs_only_turbo_env_fast_runs(
    workdir: Path, input_bam: Path, reference: Path, _: Path
) -> list[ToolRun]:
    return [
        ToolRun(
            family="turbo-picard",
            label="CollectWgsMetrics(env_fast_default)",
            command=[
                *prefix_turbo,
                "CollectWgsMetrics",
                f"I={input_bam}",
                f"O={workdir / 'turbo.wgs.txt'}",
                f"R={reference}",
                "COUNT_UNPAIRED=true",
                *silent_args(),
            ],
            workdir=workdir,
            env={"TURBO_PICARD_WGS_FAST_DEFAULT": "true"},
        )
    ]


def wgs_bundle_turbo_runs(workdir: Path, input_bam: Path, reference: Path, _: Path) -> list[ToolRun]:
    return [
        ToolRun(
            family="turbo-picard",
            label="CollectMultipleMetrics",
            command=[
                *prefix_turbo,
                "CollectMultipleMetrics",
                f"I={input_bam}",
                f"O={workdir / 'turbo.bundle'}",
                "PROGRAM=CollectAlignmentSummaryMetrics",
                "PROGRAM=CollectInsertSizeMetrics",
                "PROGRAM=CollectBaseDistributionByCycle",
                "PROGRAM=MeanQualityByCycle",
                "PROGRAM=QualityScoreDistribution",
                "PROGRAM=CollectGcBiasMetrics",
                "PROGRAM=CollectWgsMetrics",
                f"REFERENCE_SEQUENCE={reference}",
                *silent_args(),
            ],
            workdir=workdir,
        )
    ]


def wgs_bundle_turbo_fast_runs(
    workdir: Path, input_bam: Path, reference: Path, _: Path
) -> list[ToolRun]:
    return [
        ToolRun(
            family="turbo-picard",
            label="CollectMultipleMetrics(wgs_fast)",
            command=[
                *prefix_turbo,
                "CollectMultipleMetrics",
                f"I={input_bam}",
                f"O={workdir / 'turbo.bundle'}",
                "PROGRAM=CollectAlignmentSummaryMetrics",
                "PROGRAM=CollectInsertSizeMetrics",
                "PROGRAM=CollectBaseDistributionByCycle",
                "PROGRAM=MeanQualityByCycle",
                "PROGRAM=QualityScoreDistribution",
                "PROGRAM=CollectGcBiasMetrics",
                "PROGRAM=CollectWgsMetrics",
                f"REFERENCE_SEQUENCE={reference}",
                "USE_FAST_ALGORITHM=true",
                *silent_args(),
            ],
            workdir=workdir,
        )
    ]


def wgs_bundle_turbo_env_fast_runs(
    workdir: Path, input_bam: Path, reference: Path, _: Path
) -> list[ToolRun]:
    return [
        ToolRun(
            family="turbo-picard",
            label="CollectMultipleMetrics(env_wgs_fast_default)",
            command=[
                *prefix_turbo,
                "CollectMultipleMetrics",
                f"I={input_bam}",
                f"O={workdir / 'turbo.bundle'}",
                "PROGRAM=CollectAlignmentSummaryMetrics",
                "PROGRAM=CollectInsertSizeMetrics",
                "PROGRAM=CollectBaseDistributionByCycle",
                "PROGRAM=MeanQualityByCycle",
                "PROGRAM=QualityScoreDistribution",
                "PROGRAM=CollectGcBiasMetrics",
                "PROGRAM=CollectWgsMetrics",
                f"REFERENCE_SEQUENCE={reference}",
                *silent_args(),
            ],
            workdir=workdir,
            env={"TURBO_PICARD_WGS_FAST_DEFAULT": "true"},
        )
    ]


def riker_wgs_only(workdir: Path, input_bam: Path, reference: Path, _: Path) -> ToolRun | None:
    if prefix_riker is None:
        return None
    return ToolRun(
        family="riker",
        label="wgs",
        command=[
            *prefix_riker,
            "wgs",
            "-i",
            str(input_bam),
            "-r",
            str(reference),
            "-o",
            str(workdir / "riker"),
        ],
        workdir=workdir,
    )


def riker_wgs_bundle(workdir: Path, input_bam: Path, reference: Path, _: Path) -> ToolRun | None:
    if prefix_riker is None:
        return None
    return ToolRun(
        family="riker",
        label="multi",
        command=[
            *prefix_riker,
            "multi",
            "-i",
            str(input_bam),
            "-r",
            str(reference),
            "-o",
            str(workdir / "riker"),
            "--tools",
            "wgs",
            "alignment",
            "basic",
            "isize",
            "gcbias",
            "--threads",
            str(riker_threads),
        ],
        workdir=workdir,
    )


PROFILES = [
    Profile(
        name="wgs-only",
        description="Whole-genome coverage metrics",
        picard_runs=wgs_only_runs,
        riker_run=riker_wgs_only,
        turbo_runs=wgs_only_turbo_runs,
    ),
    Profile(
        name="wgs-only-fast",
        description="Whole-genome coverage metrics with turbo-picard USE_FAST_ALGORITHM=true",
        picard_runs=wgs_only_runs,
        riker_run=riker_wgs_only,
        turbo_runs=wgs_only_turbo_fast_runs,
    ),
    Profile(
        name="wgs-only-env-fast",
        description="Whole-genome coverage metrics with TURBO_PICARD_WGS_FAST_DEFAULT=true",
        picard_runs=wgs_only_runs,
        riker_run=riker_wgs_only,
        turbo_runs=wgs_only_turbo_env_fast_runs,
    ),
    Profile(
        name="wgs-bundle",
        description="Alignment, insert size, basic cycle metrics, GC bias, and WGS coverage",
        picard_runs=wgs_bundle_picard_runs,
        riker_run=riker_wgs_bundle,
        turbo_runs=wgs_bundle_turbo_runs,
    ),
    Profile(
        name="wgs-bundle-fast",
        description="Alignment, insert size, basic cycle metrics, GC bias, and WGS coverage with turbo-picard USE_FAST_ALGORITHM=true",
        picard_runs=wgs_bundle_picard_runs,
        riker_run=riker_wgs_bundle,
        turbo_runs=wgs_bundle_turbo_fast_runs,
    ),
    Profile(
        name="wgs-bundle-env-fast",
        description="Alignment, insert size, basic cycle metrics, GC bias, and WGS coverage with TURBO_PICARD_WGS_FAST_DEFAULT=true",
        picard_runs=wgs_bundle_picard_runs,
        riker_run=riker_wgs_bundle,
        turbo_runs=wgs_bundle_turbo_env_fast_runs,
    ),
]


def smoke_defaults() -> tuple[Path, Path]:
    input_bam = ROOT / "benchmarks/real-data/gatk-na12878-mito/input.bam"
    reference = ROOT / "fixtures/reference/chrM.fa"
    if not input_bam.exists():
        raise SystemExit(f"smoke fixture missing: {input_bam}")
    if not reference.exists():
        raise SystemExit(f"smoke reference missing: {reference}")
    return input_bam, reference


def write_outputs(
    *,
    output_dir: Path,
    sample_id: str,
    input_bam: Path,
    rows: list[dict[str, object]],
    riker_thread_count: int | None = None,
) -> None:
    output_dir.mkdir(parents=True, exist_ok=True)
    tsv_path = output_dir / "bench_qc_vs_riker.tsv"
    md_path = output_dir / "bench_qc_vs_riker.md"
    json_path = output_dir / "bench_qc_vs_riker.json"

    headers = [
        "sample",
        "profile",
        "tool_family",
        "tool_label",
        "wall_s",
        "max_rss_kb",
        "threads",
        "picard_bundle_role",
        "input_bytes",
    ]
    with open(tsv_path, "w", encoding="utf-8") as handle:
        handle.write("\t".join(headers) + "\n")
        for row in rows:
            handle.write("\t".join(str(row[key]) for key in headers) + "\n")

    by_profile: dict[str, list[dict[str, object]]] = {}
    for row in rows:
        by_profile.setdefault(str(row["profile"]), []).append(row)

    has_riker = any(row["tool_family"] == "riker" for row in rows)
    title = (
        "# QC benchmark: Picard vs turbo-picard vs riker"
        if has_riker
        else "# QC benchmark: Picard vs turbo-picard"
    )
    lines = [
        title,
        "",
        f"- sample: `{sample_id}`",
        f"- input: `{input_bam}`",
        f"- input bytes: `{input_bam.stat().st_size}`",
        "",
    ]
    if not has_riker:
        lines.extend(
            [
                "- note: generated without riker via `--allow-missing-riker`; this is a two-tool smoke run, not canonical three-way evidence.",
                "",
            ]
        )
    for profile_name, profile_rows in by_profile.items():
        lines.append(f"## {profile_name}")
        lines.append("")
        lines.append("| tool | label | wall (s) | max RSS (GB) | vs Picard |")
        lines.append("| --- | --- | ---: | ---: | ---: |")
        picard_total = sum(
            float(row["wall_s"])
            for row in profile_rows
            if row["tool_family"] == "picard"
        )
        for row in sorted(profile_rows, key=lambda item: (item["tool_family"], item["tool_label"])):
            wall = float(row["wall_s"])
            if row["tool_family"] == "picard":
                ratio = "1.00x"
            else:
                ratio = f"{picard_total / wall:.2f}x" if wall > 0 else "inf"
            rss = row.get("max_rss_kb")
            rss_gb = f"{int(rss) / (1024 * 1024):.2f}" if rss not in (None, "") else "n/a"
            lines.append(
                f"| {row['tool_family']} | {row['tool_label']} | {wall:.3f} | {rss_gb} | {ratio} |"
            )
        lines.append("")
        turbo_total = sum(
            float(row["wall_s"])
            for row in profile_rows
            if row["tool_family"] == "turbo-picard"
        )
        riker_total = sum(
            float(row["wall_s"])
            for row in profile_rows
            if row["tool_family"] == "riker"
        )
        if picard_total > 0 and turbo_total > 0:
            lines.append(
                f"- turbo-picard profile speedup vs Picard: **{picard_total / turbo_total:.2f}x**"
            )
        if picard_total > 0 and riker_total > 0:
            lines.append(f"- riker profile speedup vs Picard: **{picard_total / riker_total:.2f}x**")
        if turbo_total > 0 and riker_total > 0:
            lines.append(f"- turbo-picard vs riker: **{riker_total / turbo_total:.2f}x**")
        lines.append("")

    md_path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    json_path.write_text(
        json.dumps(
            {
                "sample": sample_id,
                "input_bam": str(input_bam),
                "input_bytes": input_bam.stat().st_size,
                "host": {
                    "hostname": os.uname().nodename,
                    "cpu_count": os.cpu_count(),
                },
                "has_riker": has_riker,
                "riker_threads": riker_thread_count,
                "rows": rows,
            },
            indent=2,
        )
        + "\n",
        encoding="utf-8",
    )


def ensure_riker_rows(rows: list[dict[str, object]]) -> None:
    if any(row["tool_family"] == "riker" for row in rows):
        return
    raise SystemExit(
        "riker benchmark evidence requires riker results; install riker or pass --allow-missing-riker for Picard-vs-turbo smoke runs"
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input-bam", type=Path, help="Coordinate-sorted BAM or SAM input.")
    parser.add_argument("--reference-fasta", type=Path, help="Reference FASTA for WGS/GC metrics.")
    parser.add_argument("--sample-id", default="sample")
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=ROOT / "benchmarks/riker-comparison/evidence",
    )
    parser.add_argument(
        "--picard-command",
        default=os.environ.get("TURBO_PICARD_BENCH_PICARD_COMMAND", ""),
        help="Picard entrypoint. Defaults to mamba run against .conda-turbo-picard.",
    )
    parser.add_argument(
        "--turbo-picard-command",
        default=os.environ.get("TURBO_PICARD_BENCH_TURBO_COMMAND", ""),
        help="turbo-picard entrypoint. Defaults to target/release/picard.",
    )
    parser.add_argument(
        "--riker-command",
        default=os.environ.get("TURBO_PICARD_BENCH_RIKER_COMMAND", ""),
        help="riker entrypoint. Defaults to riker on PATH; omit to skip riker runs.",
    )
    parser.add_argument(
        "--conda-prefix",
        default=os.environ.get(
            "TURBO_PICARD_CONDA_PREFIX", str(ROOT / ".conda-turbo-picard")
        ),
        help="Conda prefix used when --picard-command is not set.",
    )
    parser.add_argument(
        "--repeats",
        type=int,
        default=None,
        help="Number of repeats per tool/profile. Defaults to 5 for --smoke and 1 otherwise.",
    )
    parser.add_argument(
        "--profiles",
        nargs="+",
        choices=[profile.name for profile in PROFILES],
        default=[profile.name for profile in PROFILES],
    )
    parser.add_argument("--smoke", action="store_true", help="Use the GATK mito smoke fixture.")
    parser.add_argument(
        "--riker-threads",
        type=int,
        default=int(os.environ.get("TURBO_PICARD_BENCH_RIKER_THREADS", "0")) or None,
        help="Threads for riker multi (default: min(4, CPU count)).",
    )
    parser.add_argument(
        "--measure-rss",
        action="store_true",
        help="Wrap timed commands with GNU time -v when available.",
    )
    parser.add_argument(
        "--allow-missing-riker",
        action="store_true",
        help="Allow writing benchmark outputs when riker is unavailable.",
    )
    parser.add_argument("--skip-build", action="store_true")
    args = parser.parse_args()

    if args.repeats is None:
        args.repeats = 5 if args.smoke else 1

    if args.repeats < 1:
        raise SystemExit("--repeats must be >= 1")

    if args.smoke:
        input_bam, reference = smoke_defaults()
        sample_id = "gatk-na12878-mito"
    else:
        if args.input_bam is None or args.reference_fasta is None:
            raise SystemExit("--input-bam and --reference-fasta are required unless --smoke is set")
        input_bam = args.input_bam
        reference = args.reference_fasta
        sample_id = args.sample_id
        if not input_bam.exists():
            raise SystemExit(f"missing input BAM: {input_bam}")
        if not reference.exists():
            raise SystemExit(f"missing reference FASTA: {reference}")

    picard_command = (
        parse_command(args.picard_command)
        if args.picard_command
        else default_picard_command(Path(args.conda_prefix))
    )
    turbo_command = (
        parse_command(args.turbo_picard_command)
        if args.turbo_picard_command
        else default_turbo_command()
    )
    riker_command = None
    if args.riker_command:
        riker_command = parse_command(args.riker_command)
    else:
        riker_command = default_riker_command()

    riker_thread_count = args.riker_threads
    if riker_thread_count is None:
        riker_thread_count = min(4, os.cpu_count() or 1)
    bind_prefixes(
        picard_command,
        turbo_command,
        riker_command,
        threads=riker_thread_count,
    )

    if not args.skip_build and turbo_command[:2] == ["cargo", "run"]:
        subprocess.run(
            ["cargo", "build", "--release", "-p", "turbo-picard-cli", "--bin", "picard"],
            cwd=ROOT,
            check=True,
        )
        bind_prefixes(
            picard_command,
            [str(ROOT / "target" / "release" / "picard")],
            riker_command,
            threads=riker_thread_count,
        )

    rows: list[dict[str, object]] = []
    selected_profiles = [profile for profile in PROFILES if profile.name in args.profiles]

    with tempfile.TemporaryDirectory(prefix="turbo-picard-qc-bench.") as tmp:
        work_root = Path(tmp)
        for profile in selected_profiles:
            for repeat in range(1, args.repeats + 1):
                profile_dir = work_root / profile.name / f"rep{repeat}"
                profile_dir.mkdir(parents=True, exist_ok=True)

                runs: list[ToolRun] = []
                runs.extend(profile.picard_runs(profile_dir, input_bam, reference, work_root))
                runs.extend(profile.turbo_runs(profile_dir, input_bam, reference, work_root))
                riker = profile.riker_run(profile_dir, input_bam, reference, work_root)
                if riker is not None:
                    runs.append(riker)

                for run in runs:
                    wall, _, max_rss_kb = run_timed(
                        run.command,
                        cwd=run.workdir,
                        env_overrides=run.env,
                        measure_rss=args.measure_rss,
                    )
                    rows.append(
                        {
                            "sample": sample_id,
                            "profile": profile.name,
                            "tool_family": run.family,
                            "tool_label": run.label,
                            "wall_s": f"{wall:.6f}",
                            "max_rss_kb": "" if max_rss_kb is None else str(max_rss_kb),
                            "threads": str(
                                riker_thread_count
                                if run.family == "riker" and run.label == "multi"
                                else 1
                            ),
                            "picard_bundle_role": "main"
                            if run.family == "picard"
                            else "",
                            "input_bytes": input_bam.stat().st_size,
                        }
                    )

    if args.repeats > 1:
        collapsed: list[dict[str, object]] = []
        groups: dict[tuple[str, str, str], list[float]] = {}
        for row in rows:
            key = (str(row["profile"]), str(row["tool_family"]), str(row["tool_label"]))
            groups.setdefault(key, []).append(float(row["wall_s"]))
        for (profile_name, family, label), walls in sorted(groups.items()):
            collapsed.append(
                {
                    "sample": sample_id,
                    "profile": profile_name,
                    "tool_family": family,
                    "tool_label": label,
                    "wall_s": f"{statistics.median(walls):.6f}",
                    "max_rss_kb": "",
                    "threads": "",
                    "picard_bundle_role": "main" if family == "picard" else "",
                    "input_bytes": input_bam.stat().st_size,
                }
            )
        rows = collapsed

    if riker_command is None and not args.allow_missing_riker:
        raise SystemExit(
            "riker not found on PATH; install riker or rerun with --allow-missing-riker"
        )
    if not args.allow_missing_riker:
        ensure_riker_rows(rows)

    write_outputs(
        output_dir=args.output_dir,
        sample_id=sample_id,
        input_bam=input_bam,
        rows=rows,
        riker_thread_count=riker_thread_count,
    )
    print(f"wrote {args.output_dir / 'bench_qc_vs_riker.tsv'}")
    if riker_command is None:
        print("riker not found on PATH; Picard vs turbo-picard rows only", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
