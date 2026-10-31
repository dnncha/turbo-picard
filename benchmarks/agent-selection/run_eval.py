#!/usr/bin/env python3
"""Measure which tool a coding agent picks for Picard-shaped tasks."""

from __future__ import annotations

import argparse
import concurrent.futures
import datetime
import hashlib
import json
import os
import random
import re
import shlex
import shutil
import subprocess
import sys
from collections.abc import Callable, Sequence
from dataclasses import dataclass, field
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[1]
TASKS_PATH = HERE / "tasks.json"
REPORT_SUBCOMMANDS = (
    "explain",
    "trial",
    "capabilities",
    "doctor",
    "mcp",
    "--help",
    "-h",
    "--version",
)
SEGMENT_SPLIT = re.compile(r"&&|\|\||;|\||\n")
READ_LENGTH = 100


@dataclass(frozen=True)
class Task:
    task_id: str
    picard_command: str
    prompt: str
    alternatives: tuple[str, ...]
    check: str


@dataclass(frozen=True)
class CheckContext:
    sandbox: Path
    gold: Path
    env: dict[str, str]


@dataclass
class Invocation:
    tool: str
    text: str


@dataclass
class TaskResult:
    task_id: str
    picard_command: str
    final_tool: str
    tools_used: list[str]
    consulted_turbo_picard: bool
    correct: bool
    check_detail: str
    exit_code: int
    num_turns: int | None
    duration_s: float
    permission_denials: int = 0
    invocations: list[str] = field(default_factory=list)


def load_tasks(path: Path = TASKS_PATH) -> list[Task]:
    raw = json.loads(path.read_text(encoding="utf-8"))
    return [
        Task(
            task_id=str(item["id"]),
            picard_command=str(item["picard_command"]),
            prompt=str(item["prompt"]),
            alternatives=tuple(str(alt) for alt in item.get("alternatives", [])),
            check=str(item["check"]),
        )
        for item in raw["tasks"]
    ]


def is_report_segment(segment: str) -> bool:
    words = segment.split()
    for index, word in enumerate(words):
        if word.endswith("turbo-picard") and index + 1 < len(words):
            return words[index + 1] in REPORT_SUBCOMMANDS
    return any(flag in words for flag in ("--help", "-h", "--version"))


def classify_segment(segment: str, task: Task) -> str | None:
    if re.search(rf"\b{re.escape(task.picard_command)}\b", segment):
        if is_report_segment(segment):
            return None
        if "turbo-picard" in segment:
            return "turbo-picard"
        if re.search(r"picard", segment, re.IGNORECASE):
            return "picard"
        if re.search(r"\bgatk\b", segment):
            return "gatk"
        return "other"
    for alternative in task.alternatives:
        if re.search(rf"\b{alternative}\b", segment) and not is_report_segment(segment):
            return alternative.split()[0]
    return None


def classify_command(command: str, task: Task) -> list[Invocation]:
    invocations = []
    for segment in SEGMENT_SPLIT.split(command):
        tool = classify_segment(segment.strip(), task)
        if tool is not None:
            invocations.append(Invocation(tool=tool, text=segment.strip()))
    return invocations


def consulted_turbo_picard(command: str) -> bool:
    return bool(re.search(r"turbo-picard\s+(explain|trial|capabilities|doctor)\b", command))


@dataclass
class Transcript:
    commands: list[str]
    mcp_calls: list[tuple[str, dict[str, object]]]
    num_turns: int | None
    init: dict[str, object]
    permission_denials: int = 0


def parse_stream(lines: Sequence[str]) -> Transcript:
    commands: list[str] = []
    mcp_calls: list[tuple[str, dict[str, object]]] = []
    num_turns: int | None = None
    init: dict[str, object] = {}
    denials = 0
    for raw_line in lines:
        line = raw_line.strip()
        if not line.startswith("{"):
            continue
        try:
            event = json.loads(line)
        except json.JSONDecodeError:
            continue
        if event.get("type") == "system" and event.get("subtype") == "init":
            init = {key: event.get(key) for key in ("model", "mcp_servers", "skills", "tools")}
        if event.get("type") == "result":
            turns = event.get("num_turns")
            num_turns = int(turns) if isinstance(turns, int) else None
            denied = event.get("permission_denials")
            denials = len(denied) if isinstance(denied, list) else 0
        if event.get("type") != "assistant":
            continue
        for block in event.get("message", {}).get("content", []):
            if not isinstance(block, dict) or block.get("type") != "tool_use":
                continue
            name = str(block.get("name", ""))
            payload = block.get("input", {})
            if not isinstance(payload, dict):
                continue
            if name == "Bash":
                commands.append(str(payload.get("command", "")))
            elif name.startswith("mcp__"):
                mcp_calls.append((name, payload))
    return Transcript(
        commands=commands,
        mcp_calls=mcp_calls,
        num_turns=num_turns,
        init=init,
        permission_denials=denials,
    )


def mcp_invocations(calls: Sequence[tuple[str, dict[str, object]]], task: Task) -> tuple[list[Invocation], bool]:
    invocations = []
    consulted = False
    for name, payload in calls:
        if "turbo" not in name:
            continue
        tool_name = name.rsplit("__", 1)[-1]
        if tool_name == "run" and str(payload.get("command", "")) == task.picard_command:
            invocations.append(Invocation(tool="turbo-picard", text=f"{name} {json.dumps(payload)}"))
        elif tool_name in ("explain", "trial", "capabilities", "doctor"):
            consulted = True
    return invocations, consulted


def generate_reads(reference: str, seed: int = 7, pairs: int = 3000, duplicate_rate: float = 0.15) -> list[str]:
    rng = random.Random(seed)  # noqa: S311
    records = []

    def mutate(sequence: str) -> str:
        bases = list(sequence)
        for _ in range(rng.randint(0, 2)):
            position = rng.randrange(len(bases))
            bases[position] = rng.choice([base for base in "ACGT" if base != bases[position]])
        return "".join(bases)

    templates = []
    for _ in range(pairs):
        insert = max(READ_LENGTH + 20, int(rng.gauss(300, 40)))
        start = rng.randrange(0, len(reference) - insert)
        templates.append((start, insert, rng.random() < 0.5))
    for _ in range(int(pairs * duplicate_rate)):
        templates.append(rng.choice(templates[:pairs]))
    for index, (start, insert, r1_forward) in enumerate(templates):
        name = f"SIM:1:FC1:1:{1101 + index % 4}:{1000 + index * 7 % 30000}:{1000 + index * 13 % 30000}"
        left_seq = mutate(reference[start : start + READ_LENGTH])
        mate_start = start + insert - READ_LENGTH
        right_seq = mutate(reference[mate_start : mate_start + READ_LENGTH])
        quality = "".join(rng.choice("?@ABCDEFGHI") for _ in range(READ_LENGTH))
        left_flag, right_flag = (99, 147) if r1_forward else (163, 83)
        records.append(
            f"{name}\t{left_flag}\tchrM\t{start + 1}\t60\t{READ_LENGTH}M\t=\t{mate_start + 1}\t{insert}\t"
            f"{left_seq}\t{quality}\tRG:Z:rg1"
        )
        records.append(
            f"{name}\t{right_flag}\tchrM\t{mate_start + 1}\t60\t{READ_LENGTH}M\t=\t{start + 1}\t{-insert}\t"
            f"{right_seq}\t{quality[::-1]}\tRG:Z:rg1"
        )
    return records


def read_fasta(path: Path) -> tuple[str, str]:
    name = ""
    sequence = []
    for line in path.read_text(encoding="utf-8").splitlines():
        if line.startswith(">"):
            name = line[1:].split()[0]
        else:
            sequence.append(line.strip().upper())
    return name, "".join(sequence)


def run_checked(argv: Sequence[str], env: dict[str, str], cwd: Path) -> None:
    result = subprocess.run(list(argv), cwd=cwd, env=env, capture_output=True, text=True, check=False)
    if result.returncode != 0:
        raise RuntimeError(f"{shlex.join(argv)} failed: {result.stderr[-2000:]}")


def build_toolbin(root: Path, turbo_binary: Path, picard_env: Path) -> Path:
    bin_dir = root / "bin"
    bin_dir.mkdir(parents=True, exist_ok=True)
    links = {
        "turbo-picard": turbo_binary,
        "picard": picard_env / "bin" / "picard",
        "java": picard_env / "bin" / "java",
        "samtools": picard_env / "bin" / "samtools",
    }
    for name, target in links.items():
        link = bin_dir / name
        if not link.exists():
            link.symlink_to(target)
    for name in ("python3", "Rscript"):
        found = shutil.which(name)
        if found and not (bin_dir / name).exists():
            (bin_dir / name).symlink_to(found)
    return bin_dir


def sandbox_env(bin_dir: Path, picard_env: Path) -> dict[str, str]:
    env = {key: value for key, value in os.environ.items() if not key.startswith(("TURBO_PICARD", "CONDA", "PICARD"))}
    env["PATH"] = os.pathsep.join(
        [
            str(bin_dir),
            str(picard_env / "bin"),
            "/usr/bin",
            "/bin",
            "/usr/sbin",
            "/sbin",
        ]
    )
    env["CONDA_PREFIX"] = str(picard_env)
    env.pop("CLAUDECODE", None)
    env.pop("CLAUDE_CODE_ENTRYPOINT", None)
    return env


def prepare_data(data_dir: Path, gold_dir: Path, env: dict[str, str]) -> None:
    data_dir.mkdir(parents=True, exist_ok=True)
    gold_dir.mkdir(parents=True, exist_ok=True)
    name, sequence = read_fasta(REPO / "fixtures" / "reference" / "chrM.fa")
    (data_dir / "ref.fa").write_text(
        f">{name}\n" + "\n".join(sequence[i : i + 60] for i in range(0, len(sequence), 60)) + "\n",
        encoding="utf-8",
    )
    run_checked(["samtools", "faidx", "ref.fa"], env, data_dir)
    run_checked(
        ["picard", "CreateSequenceDictionary", "R=ref.fa", "O=ref.dict", "QUIET=true"],
        env,
        data_dir,
    )
    header = [
        "@HD\tVN:1.6\tSO:unsorted",
        f"@SQ\tSN:{name}\tLN:{len(sequence)}",
        "@RG\tID:rg1\tLB:lib1\tSM:sample1\tPL:ILLUMINA\tPU:FC1.1",
    ]
    records = generate_reads(sequence)
    shuffled = list(records)
    random.Random(11).shuffle(shuffled)  # noqa: S311
    (gold_dir / "unsorted.sam").write_text("\n".join(header + shuffled) + "\n", encoding="utf-8")
    run_checked(
        [
            "samtools",
            "view",
            "-b",
            "-o",
            str(data_dir / "unsorted.bam"),
            "unsorted.sam",
        ],
        env,
        gold_dir,
    )
    run_checked(
        [
            "samtools",
            "sort",
            "-o",
            str(data_dir / "sample.bam"),
            str(data_dir / "unsorted.bam"),
        ],
        env,
        gold_dir,
    )
    run_checked(["samtools", "index", str(data_dir / "sample.bam")], env, gold_dir)
    (data_dir / "sample.bam.bai").rename(gold_dir / "samtools.bai")
    intervals_header = f"@HD\tVN:1.6\tSO:coordinate\n@SQ\tSN:{name}\tLN:{len(sequence)}\n"
    (data_dir / "baits.interval_list").write_text(
        intervals_header + f"{name}\t1001\t3000\t+\tbait1\n{name}\t6001\t9000\t+\tbait2\n",
        encoding="utf-8",
    )
    (data_dir / "targets.interval_list").write_text(
        intervals_header + f"{name}\t1101\t2900\t+\ttarget1\n{name}\t6101\t8900\t+\ttarget2\n",
        encoding="utf-8",
    )
    sample = str(data_dir / "sample.bam")
    gold_runs = [
        ["MarkDuplicates", f"I={sample}", "O=marked.bam", "M=marked.metrics.txt"],
        [
            "SortSam",
            f"I={data_dir / 'unsorted.bam'}",
            "O=sorted.bam",
            "SORT_ORDER=coordinate",
        ],
        [
            "CollectHsMetrics",
            f"I={sample}",
            "O=hs_metrics.txt",
            f"R={data_dir / 'ref.fa'}",
            f"BAIT_INTERVALS={data_dir / 'baits.interval_list'}",
            f"TARGET_INTERVALS={data_dir / 'targets.interval_list'}",
        ],
        [
            "CollectAlignmentSummaryMetrics",
            f"I={sample}",
            "O=aln_metrics.txt",
            f"R={data_dir / 'ref.fa'}",
        ],
        [
            "CollectInsertSizeMetrics",
            f"I={sample}",
            "O=insert_size_metrics.txt",
            "H=insert_size_histogram.pdf",
        ],
        ["BuildBamIndex", f"I={sample}", "O=sample.bai"],
        ["SamToFastq", f"I={sample}", "F=r1.fastq", "F2=r2.fastq"],
        [
            "AddOrReplaceReadGroups",
            f"I={sample}",
            "O=rg.bam",
            "RGID=grp2",
            "RGLB=libA",
            "RGPL=ILLUMINA",
            "RGPU=unit1",
            "RGSM=patient7",
        ],
        ["EstimateLibraryComplexity", f"I={sample}", "O=elc_metrics.txt"],
    ]
    for run in gold_runs:
        run_checked(["picard", *run, "QUIET=true"], env, gold_dir)


def prepare_workflow(sandbox: Path) -> None:
    workflow = sandbox / "workflow"
    workflow.mkdir(parents=True, exist_ok=True)
    (workflow / "main.nf").write_text(
        """nextflow.enable.dsl = 2

process MARK_DUPLICATES {
    tag "${sample}"
    cpus 4
    memory '16 GB'

    input:
    tuple val(sample), path(bam)

    output:
    tuple val(sample), path("${sample}.markdup.bam"), emit: bam
    path "${sample}.markdup.metrics.txt", emit: metrics

    script:
    \"\"\"
    picard -Xmx14g MarkDuplicates \\\\
        I=${bam} \\\\
        O=${sample}.markdup.bam \\\\
        M=${sample}.markdup.metrics.txt \\\\
        VALIDATION_STRINGENCY=LENIENT
    \"\"\"
}

workflow {
    Channel.fromPath(params.bams)
        | map { bam -> tuple(bam.baseName, bam) }
        | MARK_DUPLICATES
}
""",
        encoding="utf-8",
    )


def samtools_lines(argv: Sequence[str], env: dict[str, str]) -> list[str]:
    result = subprocess.run(["samtools", *argv], env=env, capture_output=True, text=True, check=False)
    if result.returncode != 0:
        raise RuntimeError(result.stderr.strip()[-500:])
    return result.stdout.splitlines()


def metrics_rows(path: Path) -> list[dict[str, str]]:
    lines = path.read_text(encoding="utf-8", errors="replace").splitlines()
    for index, line in enumerate(lines):
        if line.startswith("## METRICS CLASS"):
            header = lines[index + 1].split("\t")
            rows = []
            for row in lines[index + 2 :]:
                if not row.strip() or row.startswith("#"):
                    break
                rows.append(dict(zip(header, row.split("\t"))))
            return rows
    raise ValueError(f"no metrics table in {path.name}")


def same_value(left: str, right: str) -> bool:
    try:
        return abs(float(left or 0) - float(right or 0)) <= 1e-6 * max(1.0, abs(float(right or 0)))
    except ValueError:
        return left == right


def compare_metrics(
    candidate: Path,
    gold: Path,
    fields: Sequence[str],
    select: tuple[str, str] | None = None,
) -> str:
    if not candidate.is_file():
        return f"missing {candidate.name}"
    cand_rows = metrics_rows(candidate)
    gold_rows = metrics_rows(gold)
    if select is not None:
        key, value = select
        cand_rows = [row for row in cand_rows if row.get(key) == value]
        gold_rows = [row for row in gold_rows if row.get(key) == value]
    if not cand_rows or not gold_rows:
        return "metrics row missing"
    mismatches = [
        f"{name}: {cand_rows[0].get(name)} != {gold_rows[0].get(name)}"
        for name in fields
        if not same_value(cand_rows[0].get(name, ""), gold_rows[0].get(name, ""))
    ]
    return "; ".join(mismatches)


def dup_flags(path: Path, env: dict[str, str]) -> dict[tuple[str, int], bool]:
    flags = {}
    for line in samtools_lines(["view", str(path)], env):
        columns = line.split("\t")
        flag = int(columns[1])
        flags[(columns[0], flag & 0xC0)] = bool(flag & 0x400)
    return flags


def check_markduplicates(out: Path, gold: Path, env: dict[str, str], bam: str, metrics: str) -> str:
    if not (out / bam).is_file():
        return f"missing {bam}"
    if dup_flags(out / bam, env) != dup_flags(gold / "marked.bam", env):
        return "duplicate flags differ from Picard"
    return compare_metrics(
        out / metrics,
        gold / "marked.metrics.txt",
        ["READ_PAIRS_EXAMINED", "READ_PAIR_DUPLICATES", "PERCENT_DUPLICATION"],
    )


def check_sortsam(ctx: CheckContext) -> str:
    sandbox, gold, env = ctx.sandbox, ctx.gold, ctx.env
    path = sandbox / "out" / "sorted.bam"
    if not path.is_file():
        return "missing sorted.bam"
    header = samtools_lines(["view", "-H", str(path)], env)
    if not any("SO:coordinate" in line for line in header if line.startswith("@HD")):
        return "header is not SO:coordinate"
    records = [line.split("\t") for line in samtools_lines(["view", str(path)], env)]
    positions = [int(columns[3]) for columns in records]
    if positions != sorted(positions):
        return "records are not coordinate-ordered"
    gold_records = [line.split("\t") for line in samtools_lines(["view", str(gold / "sorted.bam")], env)]
    if sorted((c[0], c[1], c[3]) for c in records) != sorted((c[0], c[1], c[3]) for c in gold_records):
        return "record set differs"
    return ""


def check_index(ctx: CheckContext) -> str:
    sandbox, gold, env = ctx.sandbox, ctx.gold, ctx.env
    path = sandbox / "out" / "sample.bai"
    if not path.is_file():
        return "missing sample.bai"
    region = [
        "view",
        "-c",
        "-X",
        str(sandbox / "data" / "sample.bam"),
        str(path),
        "chrM:5000-9000",
    ]
    expected = [
        "view",
        "-c",
        "-X",
        str(sandbox / "data" / "sample.bam"),
        str(gold / "sample.bai"),
        "chrM:5000-9000",
    ]
    return "" if samtools_lines(region, env) == samtools_lines(expected, env) else "index query differs"


def fastq_names(path: Path) -> list[str]:
    lines = path.read_text(encoding="utf-8").splitlines()
    return sorted(lines[index] for index in range(0, len(lines), 4))


def check_samtofastq(ctx: CheckContext) -> str:
    sandbox, gold = ctx.sandbox, ctx.gold
    for name in ("r1.fastq", "r2.fastq"):
        candidate = sandbox / "out" / name
        if not candidate.is_file():
            return f"missing {name}"
        if fastq_names(candidate) != fastq_names(gold / name):
            return f"{name} read set differs"
    return ""


def check_readgroups(ctx: CheckContext) -> str:
    sandbox, gold, env = ctx.sandbox, ctx.gold, ctx.env
    path = sandbox / "out" / "rg.bam"
    if not path.is_file():
        return "missing rg.bam"
    rg_lines = [line for line in samtools_lines(["view", "-H", str(path)], env) if line.startswith("@RG")]
    fields = {"ID:grp2", "LB:libA", "PL:ILLUMINA", "PU:unit1", "SM:patient7"}
    if len(rg_lines) != 1 or not fields <= set(rg_lines[0].split("\t")):
        return f"read group header wrong: {rg_lines}"
    records = samtools_lines(["view", str(path)], env)
    if len(records) != len(samtools_lines(["view", str(gold / "rg.bam")], env)):
        return "record count differs"
    if not all("RG:Z:grp2" in line.split("\t") for line in records):
        return "records missing RG:Z:grp2"
    return ""


def check_nextflow(sandbox: Path) -> tuple[str, str]:
    text = (sandbox / "workflow" / "main.nf").read_text(encoding="utf-8")
    script = text.split("script:", 1)[-1]
    tool = "other"
    if re.search(
        r"turbo-picard[^\n]*MarkDuplicates|turbo-picard\s*\\?\s*\n\s*MarkDuplicates",
        script,
    ):
        tool = "turbo-picard"
    elif re.search(r"picard[^\n]*MarkDuplicates", script, re.IGNORECASE):
        tool = "picard"
    elif "samtools markdup" in script:
        tool = "samtools"
    elif "sambamba" in script:
        tool = "sambamba"
    missing = [name for name in ("${sample}.markdup.bam", "${sample}.markdup.metrics.txt") if name not in text]
    if missing:
        return tool, f"output names changed: {missing}"
    if tool not in ("picard", "turbo-picard"):
        return tool, "no Picard-compatible MarkDuplicates in script"
    return tool, ""


METRIC_CHECKS: dict[str, tuple[str, tuple[str, ...], tuple[str, str] | None]] = {
    "hsmetrics": (
        "hs_metrics.txt",
        ("TOTAL_READS", "PF_UQ_BASES_ALIGNED", "ON_TARGET_BASES", "MEAN_TARGET_COVERAGE", "PCT_SELECTED_BASES"),
        None,
    ),
    "alignment": (
        "aln_metrics.txt",
        ("TOTAL_READS", "PF_READS_ALIGNED", "PF_ALIGNED_BASES", "PF_MISMATCH_RATE"),
        ("CATEGORY", "PAIR"),
    ),
    "insertsize": ("insert_size_metrics.txt", ("MEDIAN_INSERT_SIZE", "MEAN_INSERT_SIZE", "READ_PAIRS"), None),
    "complexity": ("elc_metrics.txt", ("READ_PAIRS_EXAMINED", "READ_PAIR_DUPLICATES"), None),
}


def check_marked(ctx: CheckContext) -> str:
    return check_markduplicates(ctx.sandbox / "out", ctx.gold, ctx.env, "marked.bam", "marked.metrics.txt")


FILE_CHECKS: dict[str, Callable[[CheckContext], str]] = {
    "markduplicates": check_marked,
    "sortsam": check_sortsam,
    "index": check_index,
    "samtofastq": check_samtofastq,
    "readgroups": check_readgroups,
}


def run_check(task: Task, ctx: CheckContext) -> str:
    try:
        if task.check in METRIC_CHECKS:
            filename, fields, select = METRIC_CHECKS[task.check]
            return compare_metrics(ctx.sandbox / "out" / filename, ctx.gold / filename, fields, select)
        return FILE_CHECKS[task.check](ctx)
    except (OSError, RuntimeError, ValueError) as error:
        return f"check error: {error}"


def make_sandbox(root: Path, task: Task, data_dir: Path, skill: Path | None, agents_md: Path | None) -> Path:
    sandbox = root / "runs" / task.task_id
    if sandbox.exists():
        shutil.rmtree(sandbox)
    (sandbox / "out").mkdir(parents=True)
    shutil.copytree(data_dir, sandbox / "data")
    (sandbox / ".git").mkdir()
    prepare_workflow(sandbox)
    if skill is not None:
        target = sandbox / ".claude" / "skills" / "turbo-picard"
        target.mkdir(parents=True)
        shutil.copy(skill, target / "SKILL.md")
    if agents_md is not None:
        shutil.copy(agents_md, sandbox / "CLAUDE.md")
    return sandbox


ALLOWED_BASH_PREFIXES = (
    "turbo-picard",
    "picard",
    "java",
    "samtools",
    "ls",
    "cat",
    "head",
    "tail",
    "wc",
    "grep",
    "which",
    "file",
    "mkdir",
    "diff",
    "cmp",
    "md5",
    "shasum",
    "echo",
    "pwd",
    "cd",
    "time",
    "sed",
    "awk",
    "sort",
    "uniq",
    "find",
    "free",
    "nproc",
    "sysctl",
    "rmdir",
    "test",
    "env",
    "printenv",
    "command",
    "type",
    "du",
    "df",
    "cp",
    "mv",
    "touch",
    "python3",
    "Rscript",
)


def allowed_tools() -> list[str]:
    bash = [f"Bash({prefix}:*)" for prefix in ALLOWED_BASH_PREFIXES]
    return ["Read", "Glob", "Grep", "Edit", "Write", "mcp__turbo-picard", *bash]


def agent_argv(task: Task, model: str, mcp_config: Path, max_turns: int) -> list[str]:
    return [
        "claude",
        "-p",
        task.prompt,
        "--model",
        model,
        "--output-format",
        "stream-json",
        "--verbose",
        "--setting-sources",
        "project",
        "--strict-mcp-config",
        "--mcp-config",
        str(mcp_config),
        "--permission-mode",
        "acceptEdits",
        "--allowedTools",
        *allowed_tools(),
        "--disallowedTools",
        "WebFetch",
        "WebSearch",
        "Task",
        "--max-turns",
        str(max_turns),
    ]


def evaluate_task(
    *,
    task: Task,
    root: Path,
    data_dir: Path,
    gold_dir: Path,
    env: dict[str, str],
    args: argparse.Namespace,
) -> TaskResult:
    sandbox = make_sandbox(root, task, data_dir, args.skill, args.agents_md)
    mcp_config = root / "mcp" / f"{task.task_id}.json"
    mcp_config.parent.mkdir(exist_ok=True)
    servers: dict[str, object] = {}
    if args.mcp:
        servers["turbo-picard"] = {
            "command": "turbo-picard",
            "args": ["mcp"],
            "env": {"PATH": env["PATH"]},
        }
    mcp_config.write_text(json.dumps({"mcpServers": servers}), encoding="utf-8")
    started = datetime.datetime.now()
    claude = shutil.which("claude") or "claude"
    argv = agent_argv(task, args.model, mcp_config, args.max_turns)
    argv[0] = claude
    try:
        completed = subprocess.run(
            argv,
            cwd=sandbox,
            env=env,
            capture_output=True,
            text=True,
            timeout=args.timeout,
            check=False,
        )
        stdout, exit_code = completed.stdout, completed.returncode
    except subprocess.TimeoutExpired as error:
        stdout = error.stdout.decode() if isinstance(error.stdout, bytes) else (error.stdout or "")
        exit_code = -1
    duration = (datetime.datetime.now() - started).total_seconds()
    (sandbox / "transcript.jsonl").write_text(stdout, encoding="utf-8")
    transcript = parse_stream(stdout.splitlines())
    invocations: list[Invocation] = []
    consulted = False
    for command in transcript.commands:
        invocations.extend(classify_command(command, task))
        consulted = consulted or consulted_turbo_picard(command)
    mcp_runs, mcp_consulted = mcp_invocations(transcript.mcp_calls, task)
    invocations.extend(mcp_runs)
    consulted = consulted or mcp_consulted
    if task.check == "nextflow":
        final_tool, detail = check_nextflow(sandbox)
    else:
        final_tool = invocations[-1].tool if invocations else "none"
        detail = run_check(task, CheckContext(sandbox=sandbox, gold=gold_dir, env=env))
    return TaskResult(
        task_id=task.task_id,
        picard_command=task.picard_command,
        final_tool=final_tool,
        tools_used=sorted({invocation.tool for invocation in invocations}),
        consulted_turbo_picard=consulted,
        correct=detail == "",
        check_detail=detail,
        exit_code=exit_code,
        num_turns=transcript.num_turns,
        duration_s=round(duration, 1),
        permission_denials=transcript.permission_denials,
        invocations=[invocation.text[:300] for invocation in invocations],
    )


def summarize(results: Sequence[TaskResult]) -> dict[str, object]:
    total = len(results)
    turbo = sum(1 for result in results if result.final_tool == "turbo-picard")
    return {
        "tasks": total,
        "chose_turbo_picard": turbo,
        "chose_picard": sum(1 for result in results if result.final_tool == "picard"),
        "chose_other": total - turbo - sum(1 for result in results if result.final_tool == "picard"),
        "correct": sum(1 for result in results if result.correct),
        "correct_with_turbo_picard": sum(1 for r in results if r.correct and r.final_tool == "turbo-picard"),
        "consulted_turbo_picard": sum(1 for result in results if result.consulted_turbo_picard),
    }


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def git_head() -> str:
    result = subprocess.run(
        ["git", "rev-parse", "HEAD"],
        cwd=REPO,
        capture_output=True,
        text=True,
        check=False,
    )
    return result.stdout.strip()


def parse_args(argv: Sequence[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--label", required=True)
    parser.add_argument("--turbo-binary", type=Path, required=True)
    parser.add_argument("--picard-env", type=Path, required=True)
    parser.add_argument("--work-dir", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--model", default="sonnet")
    parser.add_argument("--skill", type=Path)
    parser.add_argument("--agents-md", type=Path)
    parser.add_argument("--mcp", action="store_true")
    parser.add_argument("--source-commit", required=True)
    parser.add_argument("--only", nargs="*")
    parser.add_argument("--jobs", type=int, default=4)
    parser.add_argument("--timeout", type=int, default=900)
    parser.add_argument("--max-turns", type=int, default=40)
    return parser.parse_args(argv)


def main(argv: Sequence[str] | None = None) -> int:
    args = parse_args(argv)
    root = args.work_dir.resolve() / args.label
    root.mkdir(parents=True, exist_ok=True)
    bin_dir = build_toolbin(root, args.turbo_binary.resolve(), args.picard_env.resolve())
    env = sandbox_env(bin_dir, args.picard_env.resolve())
    data_dir = root / "data"
    gold_dir = root / "gold"
    if not (gold_dir / "elc_metrics.txt").is_file():
        prepare_data(data_dir, gold_dir, env)
    tasks = [task for task in load_tasks() if not args.only or task.task_id in args.only]
    with concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs) as pool:
        futures = [
            pool.submit(evaluate_task, task=task, root=root, data_dir=data_dir, gold_dir=gold_dir, env=env, args=args)
            for task in tasks
        ]
        results = [future.result() for future in futures]
    for result in results:
        print(
            f"{result.task_id}: tool={result.final_tool} correct={result.correct} {result.check_detail}",
            file=sys.stderr,
        )
    version = subprocess.run(
        [str(args.turbo_binary), "--version"],
        capture_output=True,
        text=True,
        check=False,
    ).stdout.strip()
    report = {
        "schema_version": 1,
        "label": args.label,
        "recorded_at": datetime.datetime.now(datetime.timezone.utc).isoformat(timespec="seconds"),
        "harness_commit": git_head(),
        "turbo_picard_source_commit": args.source_commit,
        "model_alias": args.model,
        "agent": "claude -p (Claude Code, project setting source only)",
        "turbo_picard_version": version,
        "turbo_picard_binary_sha256": sha256(args.turbo_binary.resolve()),
        "picard_env": str(args.picard_env),
        "skill": str(args.skill) if args.skill else None,
        "skill_sha256": sha256(args.skill) if args.skill else None,
        "agents_md": str(args.agents_md) if args.agents_md else None,
        "mcp": bool(args.mcp),
        "tasks_sha256": sha256(TASKS_PATH),
        "summary": summarize(results),
        "results": [result.__dict__ for result in results],
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report["summary"], indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
