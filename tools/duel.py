#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
import platform
import re
import shlex
import shutil
import statistics
import subprocess
import sys
import threading
import time
import uuid
from dataclasses import asdict, dataclass, field
from pathlib import Path
from typing import Any

SCHEMA = 1
TURBO = "turbo-picard"
PICARD = "picard"
UTILISATION_FLOOR = 0.6
CLOCK_TICKS = os.sysconf("SC_CLK_TCK") if hasattr(os, "sysconf") else 100
TASKS = ("wgs", "multi", "markdup-coord", "markdup-qgroup")
BUNDLE_PROGRAMS = (
    "CollectAlignmentSummaryMetrics",
    "CollectInsertSizeMetrics",
    "CollectBaseDistributionByCycle",
    "MeanQualityByCycle",
    "QualityScoreDistribution",
    "CollectGcBiasMetrics",
    "CollectWgsMetrics",
)


@dataclass(frozen=True)
class Tools:
    turbo: str
    picard: str
    riker: str
    dupblaster: str
    fastdup: str
    samtools: str


@dataclass(frozen=True)
class Context:
    ref: str
    tools: Tools
    threads: int


@dataclass(frozen=True)
class Dataset:
    name: str
    coord_bam: str
    qgroup_bam: str | None
    picard_reference: bool
    repeats: int
    warmup: bool


@dataclass(frozen=True)
class Trial:
    task: str
    dataset: str
    tool: str
    variant: str
    shell: str
    env: dict[str, str]
    outputs: dict[str, str]
    repeats: int
    warmup: bool
    version_shell: str


@dataclass
class Measurement:
    wall_s: float
    user_s: float
    sys_s: float
    peak_tree_rss_bytes: int
    exit_code: int
    thread_cpu_s: dict[str, float] = field(default_factory=dict)


def q(value: str) -> str:
    return shlex.quote(value)


def wgs_trials(ds: Dataset, ctx: Context, out: str) -> list[Trial]:
    ref, tools, threads = ctx.ref, ctx.tools, ctx.threads
    i = ds.coord_bam
    common = f"I={q(i)} R={q(ref)} VALIDATION_STRINGENCY=SILENT QUIET=true"
    trials = [
        Trial(
            "wgs",
            ds.name,
            TURBO,
            "default",
            f"{q(tools.turbo)} CollectWgsMetrics {common} O={q(out)}/turbo.wgs.txt",
            {"TURBO_PICARD_REQUIRE_NATIVE": "1"},
            {"wgs": f"{out}/turbo.wgs.txt"},
            ds.repeats,
            ds.warmup,
            f"{q(tools.turbo)} --version",
        ),
        Trial(
            "wgs",
            ds.name,
            TURBO,
            f"max-threads-{threads}",
            f"{q(tools.turbo)} CollectWgsMetrics {common} O={q(out)}/turbo-max.wgs.txt",
            {"TURBO_PICARD_REQUIRE_NATIVE": "1", "TURBO_PICARD_MAX_THREADS": str(threads)},
            {"wgs": f"{out}/turbo-max.wgs.txt"},
            ds.repeats,
            ds.warmup,
            f"{q(tools.turbo)} --version",
        ),
        Trial(
            "wgs",
            ds.name,
            "riker",
            f"threads-{threads}",
            f"{q(tools.riker)} wgs -i {q(i)} -r {q(ref)} -o {q(out)}/riker --threads {threads}",
            {},
            {"wgs-prefix": f"{out}/riker"},
            ds.repeats,
            ds.warmup,
            f"{q(tools.riker)} --version",
        ),
    ]
    if ds.picard_reference:
        trials.append(
            Trial(
                "wgs",
                ds.name,
                PICARD,
                "reference",
                f"{q(tools.picard)} CollectWgsMetrics {common} O={q(out)}/picard.wgs.txt",
                {},
                {"wgs": f"{out}/picard.wgs.txt"},
                1,
                False,
                f"{q(tools.picard)} CollectWgsMetrics --version",
            )
        )
    return trials


def multi_trials(ds: Dataset, ctx: Context, out: str) -> list[Trial]:
    ref, tools, threads = ctx.ref, ctx.tools, ctx.threads
    programs = " ".join(f"PROGRAM={p}" for p in BUNDLE_PROGRAMS)
    picard_programs = " ".join(f"PROGRAM={p}" for p in BUNDLE_PROGRAMS if p != "CollectWgsMetrics")
    common = f"I={q(ds.coord_bam)} REFERENCE_SEQUENCE={q(ref)} {programs} VALIDATION_STRINGENCY=SILENT QUIET=true"
    picard_common = (
        f"I={q(ds.coord_bam)} REFERENCE_SEQUENCE={q(ref)} {picard_programs} VALIDATION_STRINGENCY=SILENT QUIET=true"
    )
    trials = [
        Trial(
            "multi",
            ds.name,
            TURBO,
            "default",
            f"{q(tools.turbo)} CollectMultipleMetrics {common} O={q(out)}/turbo.bundle",
            {"TURBO_PICARD_REQUIRE_NATIVE": "1"},
            {"bundle-prefix": f"{out}/turbo.bundle"},
            ds.repeats,
            ds.warmup,
            f"{q(tools.turbo)} --version",
        ),
        Trial(
            "multi",
            ds.name,
            "riker",
            f"threads-{threads}",
            f"{q(tools.riker)} multi -i {q(ds.coord_bam)} -r {q(ref)} -o {q(out)}/riker"
            f" --threads {threads} --tools wgs alignment basic isize gcbias",
            {},
            {"bundle-prefix": f"{out}/riker"},
            ds.repeats,
            ds.warmup,
            f"{q(tools.riker)} --version",
        ),
    ]
    if ds.picard_reference:
        trials.append(
            Trial(
                "multi",
                ds.name,
                PICARD,
                "reference",
                f"{q(tools.picard)} CollectMultipleMetrics {picard_common} O={q(out)}/picard.bundle"
                f" && {q(tools.picard)} CollectWgsMetrics I={q(ds.coord_bam)} R={q(ref)}"
                f" O={q(out)}/picard.bundle.wgs_metrics COUNT_UNPAIRED=true VALIDATION_STRINGENCY=SILENT QUIET=true",
                {},
                {"bundle-prefix": f"{out}/picard.bundle"},
                1,
                False,
                f"{q(tools.picard)} CollectMultipleMetrics --version",
            )
        )
    return trials


def markdup_coord_trials(ds: Dataset, ctx: Context, out: str) -> list[Trial]:
    tools, threads = ctx.tools, ctx.threads
    i = ds.coord_bam
    st = q(tools.samtools)
    scratch = f"{out}/scratch"
    trials = [
        Trial(
            "markdup-coord",
            ds.name,
            TURBO,
            "default",
            f"{q(tools.turbo)} MarkDuplicates I={q(i)} O={q(out)}/turbo.md.bam M={q(out)}/turbo.md.metrics"
            f" TMP_DIR={q(scratch)} VALIDATION_STRINGENCY=SILENT",
            {"TURBO_PICARD_REQUIRE_NATIVE": "1"},
            {"bam": f"{out}/turbo.md.bam", "metrics": f"{out}/turbo.md.metrics"},
            ds.repeats,
            ds.warmup,
            f"{q(tools.turbo)} --version",
        ),
        Trial(
            "markdup-coord",
            ds.name,
            "fastdup",
            f"threads-{threads}",
            f"{q(tools.fastdup)} --input {q(i)} --output {q(out)}/fastdup.md.bam"
            f" --metrics {q(out)}/fastdup.md.metrics --num-threads {threads}",
            {},
            {"bam": f"{out}/fastdup.md.bam", "metrics": f"{out}/fastdup.md.metrics"},
            ds.repeats,
            ds.warmup,
            f"{q(tools.fastdup)} --version",
        ),
        Trial(
            "markdup-coord",
            ds.name,
            "samtools",
            f"clearflags-collate-fixmate-sort-markdup-{threads}",
            f"{st} view -u --remove-flags 1024 {q(i)}"
            f" | {st} collate -@ {threads} -O -u - {q(scratch)}/st-collate"
            f" | {st} fixmate -@ {threads} -m -u - -"
            f" | {st} sort -@ {threads} -u -T {q(scratch)}/st-sort -"
            f" | {st} markdup -@ {threads} --no-PG -f {q(out)}/samtools.md.metrics -T {q(scratch)}/st-md"
            f" --output-fmt bam,level=5 - {q(out)}/samtools.md.bam",
            {},
            {"bam": f"{out}/samtools.md.bam", "metrics": f"{out}/samtools.md.metrics"},
            ds.repeats,
            ds.warmup,
            f"{st} --version",
        ),
        Trial(
            "markdup-coord",
            ds.name,
            "dupblaster",
            f"collate-dupblaster-sort-{threads}",
            f"{st} collate -@ {threads} -O -u {q(i)} {q(scratch)}/db-collate"
            f" | {q(tools.dupblaster)} --metrics-prefix {q(out)}/dupblaster -l 0"
            f" | {st} sort -@ {threads} -l 5 -T {q(scratch)}/db-sort -o {q(out)}/dupblaster.md.bam -",
            {},
            {"bam": f"{out}/dupblaster.md.bam", "metrics": f"{out}/dupblaster.duplicate-metrics.tsv"},
            ds.repeats,
            ds.warmup,
            f"{q(tools.dupblaster)} --version",
        ),
    ]
    if ds.picard_reference:
        trials.append(
            Trial(
                "markdup-coord",
                ds.name,
                PICARD,
                "reference",
                f"{q(tools.picard)} MarkDuplicates I={q(i)} O={q(out)}/picard.md.bam M={q(out)}/picard.md.metrics"
                f" TMP_DIR={q(scratch)} VALIDATION_STRINGENCY=SILENT",
                {},
                {"bam": f"{out}/picard.md.bam", "metrics": f"{out}/picard.md.metrics"},
                1,
                False,
                f"{q(tools.picard)} MarkDuplicates --version",
            )
        )
    return trials


def markdup_qgroup_trials(ds: Dataset, ctx: Context, out: str) -> list[Trial]:
    tools = ctx.tools
    if ds.qgroup_bam is None:
        return []
    i = ds.qgroup_bam
    scratch = f"{out}/scratch"
    trials = []
    for level in (5, 0):
        trials.append(
            Trial(
                "markdup-qgroup",
                ds.name,
                TURBO,
                f"level-{level}",
                f"{q(tools.turbo)} MarkDuplicates I={q(i)} O={q(out)}/turbo.qg{level}.bam"
                f" M={q(out)}/turbo.qg{level}.metrics TMP_DIR={q(scratch)} COMPRESSION_LEVEL={level}"
                " VALIDATION_STRINGENCY=SILENT",
                {"TURBO_PICARD_REQUIRE_NATIVE": "1"},
                {"bam": f"{out}/turbo.qg{level}.bam", "metrics": f"{out}/turbo.qg{level}.metrics"},
                ds.repeats,
                ds.warmup,
                f"{q(tools.turbo)} --version",
            )
        )
        trials.append(
            Trial(
                "markdup-qgroup",
                ds.name,
                "dupblaster",
                f"level-{level}",
                f"{q(tools.dupblaster)} -i {q(i)} -o {q(out)}/dupblaster.qg{level}.bam"
                f" --metrics-prefix {q(out)}/dupblaster.qg{level} -l {level}",
                {},
                {
                    "bam": f"{out}/dupblaster.qg{level}.bam",
                    "metrics": f"{out}/dupblaster.qg{level}.duplicate-metrics.tsv",
                },
                ds.repeats,
                ds.warmup,
                f"{q(tools.dupblaster)} --version",
            )
        )
    if ds.picard_reference:
        trials.append(
            Trial(
                "markdup-qgroup",
                ds.name,
                PICARD,
                "reference",
                f"{q(tools.picard)} MarkDuplicates I={q(i)} O={q(out)}/picard.qg.bam M={q(out)}/picard.qg.metrics"
                f" TMP_DIR={q(scratch)} VALIDATION_STRINGENCY=SILENT",
                {},
                {"bam": f"{out}/picard.qg.bam", "metrics": f"{out}/picard.qg.metrics"},
                1,
                False,
                f"{q(tools.picard)} MarkDuplicates --version",
            )
        )
    return trials


BUILDERS = {
    "wgs": wgs_trials,
    "multi": multi_trials,
    "markdup-coord": markdup_coord_trials,
    "markdup-qgroup": markdup_qgroup_trials,
}


def build_plan(datasets: list[Dataset], ctx: Context, workdir: str, tasks: list[str]) -> list[Trial]:
    plan: list[Trial] = []
    for ds in datasets:
        for task in tasks:
            out = f"{workdir}/{ds.name}/{task}"
            plan.extend(BUILDERS[task](ds, ctx, out))
    return plan


def parse_stat_ticks(stat_text: str) -> tuple[str, int]:
    open_paren = stat_text.index("(")
    close_paren = stat_text.rindex(")")
    comm = stat_text[open_paren + 1 : close_paren]
    fields = stat_text[close_paren + 2 :].split()
    return comm, int(fields[11]) + int(fields[12])


def parse_status_rss(status_text: str) -> int:
    for line in status_text.splitlines():
        if line.startswith("VmRSS:"):
            return int(line.split()[1]) * 1024
    return 0


def process_tree(root: int) -> list[int]:
    children: dict[int, list[int]] = {}
    for entry in Path("/proc").iterdir():
        if not entry.name.isdigit():
            continue
        try:
            stat = (entry / "stat").read_text()
        except OSError:
            continue
        ppid = int(stat[stat.rindex(")") + 2 :].split()[1])
        children.setdefault(ppid, []).append(int(entry.name))
    tree, stack = [], [root]
    while stack:
        pid = stack.pop()
        tree.append(pid)
        stack.extend(children.get(pid, []))
    return tree


class TreeSampler:
    def __init__(self, root: int, interval_s: float = 0.2) -> None:
        self.root = root
        self.interval_s = interval_s
        self.peak_rss = 0
        self.thread_ticks: dict[tuple[int, int], tuple[str, int]] = {}
        self.stop = threading.Event()
        self.thread = threading.Thread(target=self.loop, daemon=True)

    def loop(self) -> None:
        while not self.stop.is_set():
            self.sample()
            self.stop.wait(self.interval_s)

    def sample(self) -> None:
        total = 0
        for pid in process_tree(self.root):
            base = Path(f"/proc/{pid}")
            try:
                total += parse_status_rss((base / "status").read_text())
                for task in (base / "task").iterdir():
                    comm, ticks = parse_stat_ticks((task / "stat").read_text())
                    self.thread_ticks[(pid, int(task.name))] = (comm, ticks)
            except (OSError, ValueError, IndexError):
                continue
        self.peak_rss = max(self.peak_rss, total)

    def thread_cpu_by_comm(self) -> dict[str, float]:
        by_comm: dict[str, float] = {}
        for comm, ticks in self.thread_ticks.values():
            by_comm[comm] = by_comm.get(comm, 0.0) + ticks / CLOCK_TICKS
        return dict(sorted(by_comm.items(), key=lambda item: -item[1])[:12])


def measure(shell: str, env: dict[str, str], log_path: Path) -> Measurement:
    full_env = {**os.environ, **env}
    with log_path.open("w") as log:
        start = time.monotonic()
        proc = subprocess.Popen(["bash", "-o", "pipefail", "-c", shell], env=full_env, stdout=log, stderr=log)
        sampler = TreeSampler(proc.pid)
        sampler.thread.start()
        _, status, usage = os.wait4(proc.pid, 0)
        wall = time.monotonic() - start
        sampler.stop.set()
        sampler.thread.join()
    proc.returncode = os.waitstatus_to_exitcode(status)
    peak = max(sampler.peak_rss, usage.ru_maxrss * 1024)
    return Measurement(
        wall_s=round(wall, 3),
        user_s=round(usage.ru_utime, 3),
        sys_s=round(usage.ru_stime, 3),
        peak_tree_rss_bytes=peak,
        exit_code=proc.returncode,
        thread_cpu_s=sampler.thread_cpu_by_comm(),
    )


def host_fingerprint() -> dict[str, Any]:
    model = ""
    cpuinfo = Path("/proc/cpuinfo")
    if cpuinfo.exists():
        for line in cpuinfo.read_text().splitlines():
            if line.startswith("model name"):
                model = line.split(":", 1)[1].strip()
                break
    mem_kb = 0
    meminfo = Path("/proc/meminfo")
    if meminfo.exists():
        mem_kb = int(meminfo.read_text().split()[1])
    return {
        "cpu_model": model,
        "nproc": os.cpu_count(),
        "mem_gb": round(mem_kb / 1024 / 1024),
        "os": platform.platform(),
    }


def host_class(host: dict[str, Any]) -> str:
    return f"{host.get('cpu_model', '?')} x{host.get('nproc', '?')}"


def tool_version(shell: str) -> str:
    try:
        done = subprocess.run(["bash", "-c", shell], capture_output=True, text=True, timeout=60, check=False)
    except subprocess.TimeoutExpired:
        return "unknown"
    text = (done.stdout + done.stderr).strip().splitlines()
    return text[0][:120] if text else "unknown"


def clean_outputs(trial: Trial) -> None:
    for path in trial.outputs.values():
        for candidate in Path(path).parent.glob(Path(path).name + "*"):
            if candidate.is_file():
                candidate.unlink()
    empty_scratch(Path(next(iter(trial.outputs.values()))).parent)


PERF_LINE = re.compile(r"^\s*([0-9.]+)%\s+(\S+)\s+(\S+)\s+\[[.k]\]\s+(.+?)\s*$")


def clean_symbol(symbol: str) -> str:
    return re.split(r"\s{2,}", symbol.strip())[0]


def parse_perf_report(text: str, limit: int = 25) -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
    for line in text.splitlines():
        match = PERF_LINE.match(line)
        if match:
            symbol = clean_symbol(match.group(4))
            rows.append({"pct": float(match.group(1)), "dso": match.group(3), "symbol": symbol})
    return sorted(rows, key=lambda row: -row["pct"])[:limit]


def profile_trial(trial: Trial, perf: str, out_dir: Path) -> list[dict[str, Any]]:
    data = out_dir / f"{trial.tool}.{trial.variant}.perf.data"
    record = f"{q(perf)} record -F 499 -q -o {q(str(data))} -- bash -o pipefail -c {q(trial.shell)}"
    done = subprocess.run(["bash", "-c", record], env={**os.environ, **trial.env}, capture_output=True, check=False)
    if done.returncode != 0 or not data.exists():
        return []
    report = subprocess.run(
        [
            perf,
            "report",
            "-i",
            str(data),
            "--stdio",
            "--no-children",
            "--sort",
            "comm,dso,symbol",
            "--percent-limit",
            "0.3",
        ],
        capture_output=True,
        text=True,
        check=False,
    )
    (out_dir / f"{trial.tool}.{trial.variant}.perf.txt").write_text(report.stdout)
    data.unlink()
    return parse_perf_report(report.stdout)


def group_plan(plan: list[Trial]) -> list[list[Trial]]:
    groups: dict[tuple[str, str], list[Trial]] = {}
    for trial in plan:
        groups.setdefault((trial.task, trial.dataset), []).append(trial)
    return list(groups.values())


def run_plan(
    plan: list[Trial],
    results: Path,
    *,
    git_sha: str,
    threads: int,
    run_id: str,
    perf: str | None = None,
    samtools: str = "samtools",
    repo: Path = Path(),
) -> None:
    for group in group_plan(plan):
        run_trials(group, results, git_sha=git_sha, threads=threads, run_id=run_id, perf=perf)
        with results.open("a") as sink:
            for row in accuracy_for_group(group, samtools=samtools, repo=repo):
                record = {"schema": SCHEMA, "kind": "accuracy", "run_id": run_id, "git_sha": git_sha, **row}
                sink.write(json.dumps(record) + "\n")
        drop_bulky_outputs(group)


def run_trials(plan: list[Trial], results: Path, *, git_sha: str, threads: int, run_id: str, perf: str | None) -> None:
    host = host_fingerprint()
    versions: dict[str, str] = {}
    with results.open("a") as sink:
        for trial in plan:
            out_dir = Path(next(iter(trial.outputs.values()))).parent
            (out_dir / "scratch").mkdir(parents=True, exist_ok=True)
            versions.setdefault(trial.version_shell, tool_version(trial.version_shell))
            rounds = (["warmup"] if trial.warmup else []) + [str(n) for n in range(1, trial.repeats + 1)]
            for label in rounds:
                clean_outputs(trial)
                log = out_dir / f"{trial.tool}.{trial.variant}.{label}.log"
                m = measure(trial.shell, trial.env, log)
                record = {
                    "schema": SCHEMA,
                    "kind": "timing",
                    "run_id": run_id,
                    "git_sha": git_sha,
                    "host": host,
                    "threads": threads,
                    "cache": "warm" if trial.warmup or label != "1" else "first",
                    "task": trial.task,
                    "dataset": trial.dataset,
                    "tool": trial.tool,
                    "variant": trial.variant,
                    "tool_version": versions[trial.version_shell],
                    "repeat": label,
                    "command": trial.shell,
                    "env": trial.env,
                    "outputs": trial.outputs,
                    **asdict(m),
                }
                if m.exit_code != 0:
                    record["log_tail"] = log.read_text(errors="replace")[-2000:]
                sink.write(json.dumps(record) + "\n")
                sink.flush()
                print(
                    f"{trial.dataset} {trial.task} {trial.tool}/{trial.variant} {label}: {m.wall_s}s rc={m.exit_code}"
                )
                if m.exit_code != 0:
                    break
            else:
                if perf and trial.tool == TURBO:
                    clean_outputs(trial)
                    hot = profile_trial(trial, perf, out_dir)
                    profile = {
                        "schema": SCHEMA,
                        "kind": "profile",
                        "run_id": run_id,
                        "git_sha": git_sha,
                        "host": host,
                        "task": trial.task,
                        "dataset": trial.dataset,
                        "tool": trial.tool,
                        "variant": trial.variant,
                        "hot": hot,
                    }
                    sink.write(json.dumps(profile) + "\n")
                    sink.flush()


def parse_picard_metrics(path: Path) -> dict[str, str]:
    lines = path.read_text().splitlines()
    for index, line in enumerate(lines):
        if line.startswith("## METRICS CLASS"):
            header = lines[index + 1].split("\t")
            values = lines[index + 2].split("\t")
            return dict(zip(header, values, strict=False))
    return {}


def parse_tsv_first_row(path: Path) -> dict[str, str]:
    lines = [line for line in path.read_text().splitlines() if line and not line.startswith("#")]
    if len(lines) < 2:
        return {}
    return dict(zip(lines[0].split("\t"), lines[1].split("\t"), strict=False))


def numeric_deviation(reference: dict[str, str], other: dict[str, str]) -> list[tuple[str, float, float, float]]:
    rows = []
    for key, raw in reference.items():
        candidate = other.get(key.lower(), other.get(key))
        if candidate is None:
            continue
        try:
            ref_value, other_value = float(raw), float(candidate)
        except ValueError:
            continue
        denominator = abs(ref_value) if ref_value else 1.0
        rows.append((key, ref_value, other_value, abs(other_value - ref_value) / denominator))
    return sorted(rows, key=lambda row: -row[3])


def find_prefixed(prefix: str, suffix: str) -> Path | None:
    base = Path(prefix)
    matches = sorted(base.parent.glob(base.name + "*" + suffix))
    return matches[0] if matches else None


def duplicate_keys(samtools: str, bam: str, sink: Path) -> int:
    shell = (
        f"{q(samtools)} view -f 1024 {q(bam)} | awk -F'\\t' '{{print $1\"\\t\"and($2,2496)}}'"
        f" | LC_ALL=C sort -S 4G --parallel=4 > {q(str(sink))}"
    )
    subprocess.run(["bash", "-o", "pipefail", "-c", shell], check=True)
    with sink.open() as handle:
        return sum(1 for _ in handle)


def symmetric_difference(left: Path, right: Path) -> int:
    done = subprocess.run(
        ["bash", "-c", f"LC_ALL=C comm -3 {q(str(left))} {q(str(right))} | wc -l"],
        capture_output=True,
        text=True,
        check=True,
    )
    return int(done.stdout.strip())


def accuracy_markdup(task: str, dataset: str, trials: list[Trial], samtools: str) -> list[dict[str, Any]]:
    reference = next((t for t in trials if t.tool == PICARD), None)
    if reference is None or not Path(reference.outputs["bam"]).exists():
        return []
    ref_keys = Path(reference.outputs["bam"] + ".dupkeys")
    ref_count = duplicate_keys(samtools, reference.outputs["bam"], ref_keys)
    rows = []
    for trial in trials:
        if trial.tool == PICARD or not Path(trial.outputs["bam"]).exists():
            continue
        keys = Path(trial.outputs["bam"] + ".dupkeys")
        count = duplicate_keys(samtools, trial.outputs["bam"], keys)
        diff = symmetric_difference(ref_keys, keys)
        rows.append(
            {
                "task": task,
                "dataset": dataset,
                "tool": trial.tool,
                "variant": trial.variant,
                "against": PICARD,
                "verdict": "exact" if diff == 0 else "differs",
                "detail": {"picard_duplicates": ref_count, "duplicates": count, "flag_disagreements": diff},
            }
        )
    return rows


def accuracy_wgs(task: str, dataset: str, trials: list[Trial], repo: Path) -> list[dict[str, Any]]:
    reference = next((t for t in trials if t.tool == PICARD), None)
    if reference is None or not Path(reference.outputs["wgs"]).exists():
        return []
    picard_path = Path(reference.outputs["wgs"])
    picard = parse_picard_metrics(picard_path)
    rows = []
    for trial in trials:
        if trial.tool == PICARD:
            continue
        if trial.tool == TURBO:
            done = subprocess.run(
                [
                    sys.executable,
                    str(repo / "tools/parity_compare.py"),
                    "wgs-metrics",
                    "--label",
                    f"{dataset} {trial.variant}",
                    "--picard",
                    str(picard_path),
                    "--turbo",
                    trial.outputs["wgs"],
                ],
                capture_output=True,
                text=True,
                check=False,
            )
            verdict = "exact" if done.returncode == 0 else "differs"
            detail: dict[str, Any] = {"comparator": (done.stdout + done.stderr)[-1500:]}
        else:
            found = find_prefixed(trial.outputs["wgs-prefix"], "wgs-metrics.txt") or find_prefixed(
                trial.outputs["wgs-prefix"], ".txt"
            )
            if found is None:
                continue
            deviations = numeric_deviation(picard, parse_tsv_first_row(found))
            worst = [row for row in deviations if row[3] > 1e-9]
            verdict = "exact" if deviations and not worst else "differs"
            detail = {"fields_compared": len(deviations), "worst": [list(row) for row in worst[:8]]}
        rows.append(
            {
                "task": task,
                "dataset": dataset,
                "tool": trial.tool,
                "variant": trial.variant,
                "against": PICARD,
                "verdict": verdict,
                "detail": detail,
            }
        )
    return rows


def accuracy_for_group(trials: list[Trial], *, samtools: str, repo: Path) -> list[dict[str, Any]]:
    task, dataset = trials[0].task, trials[0].dataset
    if task.startswith("markdup"):
        return accuracy_markdup(task, dataset, trials, samtools)
    if task == "wgs":
        return accuracy_wgs(task, dataset, trials, repo)
    return []


def empty_scratch(out_dir: Path) -> None:
    scratch = out_dir / "scratch"
    if scratch.is_dir():
        shutil.rmtree(scratch, ignore_errors=True)
    scratch.mkdir(parents=True, exist_ok=True)


def drop_bulky_outputs(trials: list[Trial]) -> None:
    for trial in trials:
        for path in trial.outputs.values():
            out_dir = Path(path).parent
            for candidate in out_dir.glob("*"):
                if candidate.suffix in {".bam", ".bai", ".dupkeys"} and candidate.is_file():
                    candidate.unlink()
            empty_scratch(out_dir)


def load_records(paths: list[Path]) -> list[dict[str, Any]]:
    records = []
    for path in paths:
        if not path.exists():
            continue
        for number, line in enumerate(path.read_text().splitlines(), start=1):
            if not line.strip():
                continue
            record = json.loads(line)
            if record.get("schema") != SCHEMA or record.get("kind") not in ("timing", "accuracy", "profile"):
                raise ValueError(f"{path}:{number}: unknown record schema/kind")
            records.append(record)
    return records


@dataclass
class Entry:
    tool: str
    variant: str
    wall_s: float
    rss: int
    cpu_s: float
    threads: int
    failed: bool
    thread_cpu_s: dict[str, float]
    git_sha: str
    inherited_from: str = ""


def summarise(records: list[dict[str, Any]], run_id: str) -> dict[tuple[str, str, str], list[Entry]]:
    grouped: dict[tuple[str, str, str, str, str], list[dict[str, Any]]] = {}
    for record in records:
        if record["kind"] != "timing" or record["run_id"] != run_id or record["task"].startswith("_"):
            continue
        key = (host_class(record["host"]), record["task"], record["dataset"], record["tool"], record["variant"])
        grouped.setdefault(key, []).append(record)
    table: dict[tuple[str, str, str], list[Entry]] = {}
    for (host, task, dataset, tool, variant), rows in grouped.items():
        measured = [r for r in rows if r["repeat"] != "warmup"] or rows
        failed = any(r["exit_code"] != 0 for r in measured)
        ok = [r for r in measured if r["exit_code"] == 0] or measured
        median_row = sorted(ok, key=lambda r: r["wall_s"])[len(ok) // 2]
        table.setdefault((host, task, dataset), []).append(
            Entry(
                tool=tool,
                variant=variant,
                wall_s=statistics.median(r["wall_s"] for r in ok),
                rss=int(statistics.median(r["peak_tree_rss_bytes"] for r in ok)),
                cpu_s=statistics.median(r["user_s"] + r["sys_s"] for r in ok),
                threads=int(median_row["threads"]),
                failed=failed,
                thread_cpu_s=median_row.get("thread_cpu_s", {}),
                git_sha=median_row["git_sha"],
            )
        )
    return table


def summarise_with_carryover(records: list[dict[str, Any]], run_id: str) -> dict[tuple[str, str, str], list[Entry]]:
    table = summarise(records, run_id)
    earlier = sorted(
        {str(r["run_id"]) for r in records if r["kind"] == "timing" and str(r["run_id"]) < run_id}, reverse=True
    )
    for previous in earlier:
        for key, entries in summarise(records, previous).items():
            if key not in table:
                continue
            present = {e.tool for e in table[key]}
            for entry in entries:
                if entry.tool not in present and entry.tool != TURBO:
                    entry.inherited_from = previous
                    table[key].append(entry)
    return table


def best_per_tool(entries: list[Entry]) -> dict[str, Entry]:
    best: dict[str, Entry] = {}
    for entry in entries:
        if entry.failed:
            continue
        current = best.get(entry.tool)
        if current is None or entry.wall_s < current.wall_s:
            best[entry.tool] = entry
    return best


def diagnose(entry: Entry) -> str:
    if entry.wall_s <= 0:
        return "no timing"
    utilisation = entry.cpu_s / (entry.wall_s * max(entry.threads, 1))
    hottest = ", ".join(f"{name} {seconds:.0f}s" for name, seconds in list(entry.thread_cpu_s.items())[:3])
    shape = "under-parallel" if utilisation < UTILISATION_FLOOR else "cpu-saturated"
    return f"{shape}: {utilisation:.0%} of {entry.threads} cores busy; hottest threads {hottest or 'n/a'}"


def find_losses(table: dict[tuple[str, str, str], list[Entry]], accuracy: list[dict[str, Any]]) -> list[dict[str, Any]]:
    losses: list[dict[str, Any]] = []
    for (host, task, dataset), entries in table.items():
        turbo_rows = [e for e in entries if e.tool == TURBO]
        if turbo_rows and all(e.failed for e in turbo_rows):
            losses.append(
                {
                    "kind": "refused",
                    "severity": 100.0,
                    "host": host,
                    "task": task,
                    "dataset": dataset,
                    "summary": "turbo-picard failed or refused the native path; competitors ran",
                }
            )
            continue
        best = best_per_tool(entries)
        turbo = best.get(TURBO)
        rivals = {tool: e for tool, e in best.items() if tool not in (TURBO, PICARD)}
        if turbo is None or not rivals:
            continue
        fastest = min(rivals.values(), key=lambda e: e.wall_s)
        if fastest.wall_s < turbo.wall_s:
            ratio = turbo.wall_s / fastest.wall_s
            losses.append(
                {
                    "kind": "speed",
                    "severity": round(ratio, 3),
                    "host": host,
                    "task": task,
                    "dataset": dataset,
                    "summary": f"{fastest.tool} ({fastest.variant}) {fastest.wall_s:.1f}s beats turbo-picard"
                    f" ({turbo.variant}) {turbo.wall_s:.1f}s: {ratio:.2f}x slower",
                    "diagnosis": diagnose(turbo),
                }
            )
        leanest = min(rivals.values(), key=lambda e: e.rss)
        if leanest.rss < turbo.rss:
            ratio = turbo.rss / max(leanest.rss, 1)
            losses.append(
                {
                    "kind": "memory",
                    "severity": round(ratio, 3),
                    "host": host,
                    "task": task,
                    "dataset": dataset,
                    "summary": f"{leanest.tool} peak {leanest.rss / 2**20:.0f} MiB vs turbo-picard"
                    f" {turbo.rss / 2**20:.0f} MiB: {ratio:.2f}x more memory",
                }
            )
    for row in accuracy:
        if row["tool"] == TURBO and row["verdict"] != "exact":
            losses.append(
                {
                    "kind": "accuracy",
                    "severity": 1000.0,
                    "host": "any",
                    "task": row["task"],
                    "dataset": row["dataset"],
                    "summary": f"turbo-picard ({row['variant']}) differs from Picard",
                    "detail": row["detail"],
                }
            )
    return sorted(losses, key=lambda loss: -float(loss["severity"]))


def previous_turbo(records: list[dict[str, Any]], run_id: str, key: tuple[str, str, str]) -> float | None:
    earlier = [
        r
        for r in records
        if r["kind"] == "timing"
        and r["run_id"] != run_id
        and r["tool"] == TURBO
        and r["exit_code"] == 0
        and r["repeat"] != "warmup"
        and (host_class(r["host"]), r["task"], r["dataset"]) == key
    ]
    if not earlier:
        return None
    last_run = max(str(r["run_id"]) for r in earlier)
    return min(
        float(statistics.median([r["wall_s"] for r in earlier if r["run_id"] == last_run and r["variant"] == v]))
        for v in {r["variant"] for r in earlier if r["run_id"] == last_run}
    )


def mib(value: int) -> str:
    return f"{value / 2**20:,.0f}"


def render_scoreboard(records: list[dict[str, Any]], run_id: str) -> tuple[str, list[dict[str, Any]]]:
    table = summarise_with_carryover(records, run_id)
    accuracy = [r for r in records if r["kind"] == "accuracy" and r["run_id"] == run_id]
    verdicts = {(r["task"], r["dataset"], r["tool"], r["variant"]): r["verdict"] for r in accuracy}
    losses = find_losses(table, accuracy)
    profiles = {
        (r["task"], r["dataset"], r["variant"]): r["hot"]
        for r in records
        if r["kind"] == "profile" and r["run_id"] == run_id
    }
    for loss in losses:
        hot = next((h for (t, d, _), h in profiles.items() if t == loss["task"] and d == loss["dataset"] and h), None)
        if hot:
            loss["hot"] = [f"{row['pct']:.1f}% {clean_symbol(row['symbol'])}" for row in hot[:5]]
    sha = next((r["git_sha"] for r in records if r["run_id"] == run_id), "?")
    lines = [
        "# Duel scoreboard",
        "",
        f"Run `{run_id}`, turbo-picard `{sha[:12]}`. Generated by `tools/duel.py report`;"
        " do not edit by hand. Medians of measured repeats, warm page cache.",
        "",
        f"## Losses to fix ({len(losses)})",
        "",
    ]
    if not losses:
        lines.append("None. turbo-picard is fastest, leanest and Picard-exact on every measured case.")
    for loss in losses:
        extra = f" — {loss['diagnosis']}" if "diagnosis" in loss else ""
        lines.append(
            f"- **{loss['kind']}** · {loss['task']} · {loss['dataset']} · {loss['host']}: {loss['summary']}{extra}"
        )
        if loss.get("hot"):
            lines.append(f"  - hottest turbo-picard symbols: {'; '.join(loss['hot'])}")
    for (host, task, dataset), entries in sorted(table.items()):
        lines += [
            "",
            f"## {task} · {dataset} · {host}",
            "",
            "| Rank | Tool | Variant | Wall s | CPU s | Peak RSS MiB | vs Picard | Accuracy vs Picard |",
            "| ---: | :--- | :--- | ---: | ---: | ---: | ---: | :--- |",
        ]
        picard = next((e for e in entries if e.tool == PICARD and not e.failed), None)
        ranked = sorted(entries, key=lambda e: (e.failed, e.wall_s))
        for rank, entry in enumerate(ranked, start=1):
            speedup = f"{picard.wall_s / entry.wall_s:.1f}x" if picard and not entry.failed else ""
            verdict = (
                "reference"
                if entry.tool == PICARD
                else verdicts.get((task, dataset, entry.tool, entry.variant), "not compared")
            )
            wall = "FAILED" if entry.failed else f"{entry.wall_s:,.1f}"
            name = f"**{entry.tool}**" if entry.tool == TURBO else entry.tool
            if entry.inherited_from:
                name += f" (from {entry.inherited_from})"
            lines.append(
                f"| {rank if not entry.failed else '-'} | {name} | {entry.variant} | {wall} | {entry.cpu_s:,.1f} | {mib(entry.rss)} | {speedup} | {verdict} |"
            )
        before = previous_turbo(records, run_id, (host, task, dataset))
        now = best_per_tool(entries).get(TURBO)
        if before and now:
            lines.append("")
            lines.append(
                f"turbo-picard trend on this host: {before:,.1f}s → {now.wall_s:,.1f}s ({before / now.wall_s:.2f}x)."
            )
    return "\n".join(lines) + "\n", losses


def latest_run_id(records: list[dict[str, Any]]) -> str:
    run_ids = [str(r["run_id"]) for r in records if r["kind"] == "timing"]
    if not run_ids:
        raise ValueError("no timing records")
    return max(run_ids)


def command_report(args: argparse.Namespace) -> int:
    ledger = Path(args.ledger)
    if args.add:
        existing = ledger.read_text() if ledger.exists() else ""
        known = set(existing.splitlines())
        fresh = [line for line in Path(args.add).read_text().splitlines() if line.strip() and line not in known]
        load_records([Path(args.add)])
        with ledger.open("a") as sink:
            sink.writelines(line + "\n" for line in fresh)
    records = load_records([ledger])
    run_id = args.run_id or latest_run_id(records)
    markdown, losses = render_scoreboard(records, run_id)
    Path(args.scoreboard).write_text(markdown)
    Path(args.losses).write_text(json.dumps({"run_id": run_id, "losses": losses}, indent=2) + "\n")
    print(markdown)
    return 1 if any(loss["kind"] == "accuracy" for loss in losses) else 0


def parse_dataset(spec: str) -> Dataset:
    fields = dict(part.split("=", 1) for part in spec.split(","))
    return Dataset(
        name=fields["name"],
        coord_bam=fields["coord"],
        qgroup_bam=fields.get("qgroup"),
        picard_reference=fields.get("picard", "false") == "true",
        repeats=int(fields.get("repeats", "3")),
        warmup=fields.get("warmup", "true") == "true",
    )


def command_run(args: argparse.Namespace) -> int:
    tools = Tools(
        turbo=args.turbo,
        picard=args.picard,
        riker=args.riker,
        dupblaster=args.dupblaster,
        fastdup=args.fastdup,
        samtools=args.samtools,
    )
    datasets = [parse_dataset(spec) for spec in args.dataset]
    plan = build_plan(datasets, Context(args.reference, tools, args.threads), args.workdir, args.tasks.split(","))
    if args.only_tools:
        keep = set(args.only_tools.split(","))
        plan = [t for t in plan if t.tool in keep]
    run_id = args.run_id or f"{time.strftime('%Y%m%dT%H%M%SZ', time.gmtime())}-{uuid.uuid4().hex[:6]}"
    results = Path(args.results)
    with results.open("a") as sink:
        sink.write(
            json.dumps(
                {
                    "schema": SCHEMA,
                    "kind": "timing",
                    "run_id": run_id,
                    "started": run_id,
                    "git_sha": args.git_sha,
                    "host": host_fingerprint(),
                    "threads": args.threads,
                    "task": "_start",
                    "dataset": "_",
                    "tool": "_",
                    "variant": "_",
                    "repeat": "warmup",
                    "wall_s": 0,
                    "user_s": 0,
                    "sys_s": 0,
                    "peak_tree_rss_bytes": 0,
                    "exit_code": 0,
                }
            )
            + "\n"
        )
    if args.dry_run:
        for trial in plan:
            print(json.dumps(asdict(trial)))
        return 0
    perf = args.perf or shutil.which("perf-real") or shutil.which("perf")
    run_plan(
        plan,
        results,
        git_sha=args.git_sha,
        threads=args.threads,
        run_id=run_id,
        perf=perf,
        samtools=args.samtools,
        repo=Path(args.repo),
    )
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Race turbo-picard against Picard and competitors and keep a ledger.")
    sub = parser.add_subparsers(dest="command", required=True)
    run = sub.add_parser("run")
    run.add_argument(
        "--dataset", action="append", required=True, help="name=..,coord=..,qgroup=..,picard=true,repeats=3"
    )
    run.add_argument("--reference", required=True)
    run.add_argument("--threads", type=int, required=True)
    run.add_argument("--workdir", required=True)
    run.add_argument("--results", required=True)
    run.add_argument("--tasks", default=",".join(TASKS))
    run.add_argument("--git-sha", required=True)
    run.add_argument("--run-id")
    run.add_argument("--repo", default=str(Path(__file__).resolve().parent.parent))
    run.add_argument("--dry-run", action="store_true")
    run.add_argument("--only-tools", help="rerun only these tools; the report carries rivals over from earlier runs")
    run.add_argument("--perf", help="perf binary for one extra profiled turbo-picard run per trial")
    for name in ("turbo", "picard", "riker", "dupblaster", "fastdup", "samtools"):
        run.add_argument(f"--{name}", default=name)
    report = sub.add_parser("report")
    report.add_argument("--ledger", default="benchmarks/duel/ledger.jsonl")
    report.add_argument("--add")
    report.add_argument("--run-id")
    report.add_argument("--scoreboard", default="benchmarks/duel/SCOREBOARD.md")
    report.add_argument("--losses", default="benchmarks/duel/losses.json")
    args = parser.parse_args(argv)
    return command_run(args) if args.command == "run" else command_report(args)


if __name__ == "__main__":
    raise SystemExit(main())
