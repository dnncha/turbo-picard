#!/usr/bin/env python3
"""Run auditable MarkDuplicates benchmarks against installed competitors.

The runner deliberately separates measurement from publication.  It writes raw
logs and a machine-readable report, including failed and unavailable tools, but
never turns a timing into a performance claim.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import re
import signal
import shlex
import shutil
import statistics
import subprocess
import sys
import threading
import time
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Iterator


ROOT = Path(__file__).resolve().parents[1]
GNU_TIME = Path("/usr/bin/time")
TIME_PREFIX = "TPBENCH"
PLACEHOLDERS = {"input", "output", "metrics", "tmp", "threads"}


@dataclass(frozen=True)
class ToolSpec:
    name: str
    command: tuple[str, ...]
    version_command: tuple[str, ...]
    metrics_kind: str
    environment: tuple[tuple[str, str], ...] = ()


@dataclass
class RunResult:
    repeat: int
    warmup: bool
    status: str
    exit_code: int | None
    wall_seconds: float | None
    user_cpu_seconds: float | None
    system_cpu_seconds: float | None
    peak_rss_bytes: int | None
    temporary_disk_peak_bytes: int
    output_bytes: int
    command: list[str]
    environment: dict[str, str]
    stdout_log: str
    stderr_log: str
    resource_log: str
    output: str
    metrics: str
    resource_backend: str
    error: str | None = None


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(8 * 1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def directory_size(path: Path) -> int:
    total = 0
    try:
        for entry in path.rglob("*"):
            try:
                if entry.is_file():
                    total += entry.stat().st_size
            except FileNotFoundError:
                pass
    except FileNotFoundError:
        pass
    return total


class DiskPeakMonitor:
    def __init__(self, path: Path, interval_seconds: float) -> None:
        self.path = path
        self.interval_seconds = interval_seconds
        self.peak = 0
        self._stop = threading.Event()
        self._thread = threading.Thread(target=self._sample, daemon=True)

    def _sample(self) -> None:
        while not self._stop.is_set():
            self.peak = max(self.peak, directory_size(self.path))
            self._stop.wait(self.interval_seconds)
        self.peak = max(self.peak, directory_size(self.path))

    def __enter__(self) -> "DiskPeakMonitor":
        self._thread.start()
        return self

    def __exit__(self, *_args: object) -> None:
        self._stop.set()
        self._thread.join()


def executable_path(name: str, local_candidates: tuple[Path, ...] = ()) -> str | None:
    for candidate in local_candidates:
        if candidate.is_file() and os.access(candidate, os.X_OK):
            return str(candidate.resolve())
    return shutil.which(name)


def preset_tools() -> dict[str, ToolSpec | None]:
    turbo = executable_path(
        "turbo-picard",
        (ROOT / "target/release/turbo-picard", ROOT / "target/release/picard"),
    )
    picard = executable_path("picard")
    samtools = executable_path("samtools")
    fastdup = executable_path("fastdup")
    common = (
        "ASSUME_SORTED=true",
        "VALIDATION_STRINGENCY=SILENT",
        "QUIET=true",
        "READ_NAME_REGEX=null",
        "ADD_PG_TAG_TO_READS=false",
        "CLEAR_DT=false",
    )
    return {
        "turbo-picard": ToolSpec(
            "turbo-picard",
            (turbo, "MarkDuplicates", "I={input}", "O={output}", "M={metrics}", "TMP_DIR={tmp}", *common),
            (turbo, "--version"),
            "picard",
            (("TURBO_PICARD_THREADS", "{threads}"),),
        ) if turbo else None,
        "picard": ToolSpec(
            "picard",
            (picard, "MarkDuplicates", "I={input}", "O={output}", "M={metrics}", "TMP_DIR={tmp}", *common),
            (picard, "--version"),
            "picard",
        ) if picard else None,
        "samtools": ToolSpec(
            "samtools",
            (samtools, "markdup", "-@", "{threads}", "--no-PG", "-f", "{metrics}", "{input}", "{output}"),
            (samtools, "--version"),
            "samtools",
        ) if samtools else None,
        "fastdup": ToolSpec(
            "fastdup",
            (fastdup, "--input", "{input}", "--output", "{output}", "--metrics", "{metrics}", "--num-threads", "{threads}"),
            (fastdup, "--version"),
            "picard",
        ) if fastdup else None,
    }


def parse_custom_tool(value: str) -> ToolSpec:
    if "=" not in value:
        raise argparse.ArgumentTypeError("--tool must be NAME=COMMAND_TEMPLATE")
    name, raw_command = value.split("=", 1)
    name = name.strip()
    if not re.fullmatch(r"[A-Za-z0-9_.-]+", name):
        raise argparse.ArgumentTypeError("tool name may contain only letters, digits, '.', '_' and '-'")
    try:
        command = tuple(shlex.split(raw_command))
    except ValueError as exc:
        raise argparse.ArgumentTypeError(f"invalid command template: {exc}") from exc
    if not command:
        raise argparse.ArgumentTypeError("tool command cannot be empty")
    fields = {match for token in command for match in re.findall(r"\{([^{}]+)\}", token)}
    unknown = fields - PLACEHOLDERS
    if unknown:
        raise argparse.ArgumentTypeError(f"unknown placeholders: {', '.join(sorted(unknown))}")
    for required in ("input", "output"):
        if required not in fields:
            raise argparse.ArgumentTypeError(f"command template must contain {{{required}}}")
    executable = executable_path(command[0])
    if executable:
        command = (executable, *command[1:])
    return ToolSpec(name, command, (command[0], "--version"), "unknown")


def expand_command(spec: ToolSpec, *, input_path: Path, output: Path, metrics: Path, tmp: Path, threads: int) -> list[str]:
    values = {
        "input": str(input_path.resolve()),
        "output": str(output.resolve()),
        "metrics": str(metrics.resolve()),
        "tmp": str(tmp.resolve()),
        "threads": str(threads),
    }
    return [token.format(**values) for token in spec.command]


def capture_version(spec: ToolSpec) -> dict[str, object]:
    executable = Path(spec.command[0])
    resolved = executable.resolve() if executable.exists() else executable
    result: dict[str, object] = {
        "executable": str(resolved),
        "executable_sha256": sha256_file(resolved) if resolved.is_file() else None,
    }
    try:
        completed = subprocess.run(
            spec.version_command,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            check=False,
            timeout=20,
        )
        lines = completed.stdout.strip().splitlines()
        result.update(exit_code=completed.returncode, text="\n".join(lines[:12]))
    except (OSError, subprocess.TimeoutExpired) as exc:
        result.update(exit_code=None, text="", error=str(exc))
    return result


def parse_time_file(path: Path) -> dict[str, float | int]:
    if not path.exists():
        raise ValueError("GNU time did not produce a resource record")
    line = next((line for line in reversed(path.read_text(encoding="utf-8").splitlines()) if line.startswith(TIME_PREFIX + "\t")), None)
    if line is None:
        raise ValueError("GNU time resource record is malformed")
    fields = line.split("\t")
    if len(fields) != 6:
        raise ValueError("GNU time resource record has the wrong field count")
    return {
        "wall_seconds": float(fields[1]),
        "user_cpu_seconds": float(fields[2]),
        "system_cpu_seconds": float(fields[3]),
        "peak_rss_bytes": int(fields[4]) * 1024,
        "exit_code": int(fields[5]),
    }


def gnu_time_available() -> bool:
    """Return whether the configured ``time`` executable is GNU time.

    macOS ships a BSD ``/usr/bin/time`` that accepts neither GNU's ``-f``
    format nor ``-o`` output options. Treating that executable as GNU time
    makes every measured run look failed before the fallback resource meter
    can be used.
    """
    if not GNU_TIME.is_file():
        return False
    try:
        result = subprocess.run(
            [str(GNU_TIME), "--version"],
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            check=False,
        )
    except OSError:
        return False
    return "GNU time" in result.stdout


def run_metered(command, stdout_handle, stderr_handle, resource_log, timeout, environment):
    """Run without a shell, preferring GNU time and disclosing a wait4 fallback."""
    if gnu_time_available():
        time_format = f"{TIME_PREFIX}\t%e\t%U\t%S\t%M\t%x"
        try:
            completed = subprocess.run(
                [str(GNU_TIME), "-f", time_format, "-o", str(resource_log), *command],
                stdout=stdout_handle,
                stderr=stderr_handle,
                check=False,
                timeout=timeout,
                env=environment,
            )
            return completed.returncode, parse_time_file(resource_log), None, "gnu-time"
        except (OSError, subprocess.TimeoutExpired, ValueError) as exc:
            return None, {}, str(exc), "gnu-time"

    start = time.perf_counter()
    try:
        process = subprocess.Popen(
            command,
            stdout=stdout_handle,
            stderr=stderr_handle,
            start_new_session=True,
            env=environment,
        )
    except OSError as exc:
        return None, {}, str(exc), "posix-wait4"
    deadline = start + timeout if timeout is not None else None
    while True:
        pid, wait_status, usage = os.wait4(process.pid, os.WNOHANG)
        if pid:
            break
        if deadline is not None and time.perf_counter() >= deadline:
            os.killpg(process.pid, signal.SIGKILL)
            _pid, wait_status, usage = os.wait4(process.pid, 0)
            process.returncode = os.waitstatus_to_exitcode(wait_status)
            return process.returncode, {}, f"command timed out after {timeout} seconds", "posix-wait4"
        time.sleep(0.01)
    process.returncode = os.waitstatus_to_exitcode(wait_status)
    rss_bytes = int(usage.ru_maxrss) * (1 if sys.platform == "darwin" else 1024)
    resources = {
        "wall_seconds": time.perf_counter() - start,
        "user_cpu_seconds": usage.ru_utime,
        "system_cpu_seconds": usage.ru_stime,
        "peak_rss_bytes": rss_bytes,
        "exit_code": process.returncode,
    }
    resource_log.write_text(
        "backend=posix-wait4\n" + "\n".join(f"{key}={value}" for key, value in resources.items()) + "\n",
        encoding="utf-8",
    )
    return process.returncode, resources, None, "posix-wait4"


def run_once(spec: ToolSpec, input_path: Path, root: Path, repeat: int, warmup: bool, threads: int, sample_seconds: float, timeout: float | None) -> RunResult:
    root.mkdir(parents=True, exist_ok=True)
    tmp = root / "tmp"
    tmp.mkdir()
    output = root / "output.bam"
    metrics = root / "metrics.txt"
    stdout = root / "stdout.log"
    stderr = root / "stderr.log"
    resource_log = root / "resources.tsv"
    command = expand_command(spec, input_path=input_path, output=output, metrics=metrics, tmp=tmp, threads=threads)
    run_environment = {
        key: value.format(threads=threads)
        for key, value in spec.environment
    }
    environment = os.environ.copy()
    environment.update(run_environment)
    (root / "command.json").write_text(json.dumps(command, indent=2) + "\n", encoding="utf-8")
    (root / "environment.json").write_text(json.dumps(run_environment, indent=2) + "\n", encoding="utf-8")
    exit_code: int | None = None
    error: str | None = None
    resources: dict[str, float | int] = {}
    resource_backend = "unknown"
    with stdout.open("wb") as stdout_handle, stderr.open("wb") as stderr_handle, DiskPeakMonitor(tmp, sample_seconds) as monitor:
        exit_code, resources, error, resource_backend = run_metered(
            command, stdout_handle, stderr_handle, resource_log, timeout, environment
        )
    status = "success" if exit_code == 0 and output.is_file() else "failed"
    if exit_code == 0 and not output.is_file():
        error = "command returned zero but did not create {output}"
    return RunResult(
        repeat=repeat,
        warmup=warmup,
        status=status,
        exit_code=exit_code,
        wall_seconds=resources.get("wall_seconds"),
        user_cpu_seconds=resources.get("user_cpu_seconds"),
        system_cpu_seconds=resources.get("system_cpu_seconds"),
        peak_rss_bytes=resources.get("peak_rss_bytes"),
        temporary_disk_peak_bytes=monitor.peak,
        output_bytes=output.stat().st_size if output.is_file() else 0,
        command=command,
        environment=run_environment,
        stdout_log=str(stdout.relative_to(root.parents[2])),
        stderr_log=str(stderr.relative_to(root.parents[2])),
        resource_log=str(resource_log.relative_to(root.parents[2])),
        output=str(output.relative_to(root.parents[2])),
        metrics=str(metrics.relative_to(root.parents[2])),
        resource_backend=resource_backend,
        error=error,
    )


def percentile(values: list[float], fraction: float) -> float:
    ordered = sorted(values)
    if len(ordered) == 1:
        return ordered[0]
    rank = fraction * (len(ordered) - 1)
    low = int(rank)
    high = min(low + 1, len(ordered) - 1)
    return ordered[low] + (ordered[high] - ordered[low]) * (rank - low)


def summarize_runs(runs: list[RunResult]) -> dict[str, object]:
    measured = [run for run in runs if not run.warmup and run.status == "success"]
    summary: dict[str, object] = {
        "successful_repeats": len(measured),
        "failed_repeats": sum(not run.warmup and run.status != "success" for run in runs),
    }
    for attribute in ("wall_seconds", "user_cpu_seconds", "system_cpu_seconds", "peak_rss_bytes", "temporary_disk_peak_bytes"):
        values = [float(getattr(run, attribute)) for run in measured if getattr(run, attribute) is not None]
        if values:
            summary[attribute] = {
                "median": statistics.median(values),
                "p95": percentile(values, 0.95),
                "min": min(values),
                "max": max(values),
            }
    return summary


def sam_fields(path: Path) -> Iterator[tuple[object, ...]]:
    if path.suffix.lower() == ".sam":
        with path.open(encoding="utf-8") as handle:
            for line in handle:
                if line.startswith("@") or not line.strip():
                    continue
                fields = line.rstrip("\n").split("\t")
                tags = {tag[:2]: tag[5:] for tag in fields[11:] if len(tag) >= 5 and tag[2] == ":"}
                yield (fields[0], int(fields[1]) & 0x400 != 0, fields[2], int(fields[3]), fields[5], fields[6], int(fields[7]), int(fields[8]), tags.get("DT"), tags.get("DS"), tags.get("DI"), tags.get("RX"), tags.get("BX"), tags.get("BY"))
        return
    try:
        import pysam  # type: ignore[import-not-found]
    except ImportError as exc:
        raise RuntimeError("BAM parity comparison requires pysam") from exc
    with pysam.AlignmentFile(str(path), "rb") as handle:
        for record in handle.fetch(until_eof=True):
            tag = lambda name: record.get_tag(name) if record.has_tag(name) else None
            yield (record.query_name, record.is_duplicate, record.reference_id, record.reference_start, record.cigarstring, record.next_reference_id, record.next_reference_start, record.template_length, tag("DT"), tag("DS"), tag("DI"), tag("RX"), tag("BX"), tag("BY"))


def has_picard_metrics(path: Path) -> bool:
    if not path.is_file():
        return False
    return "picard.sam.DuplicationMetrics" in path.read_text(encoding="utf-8", errors="replace")


def normalized_metrics(path: Path) -> list[str]:
    rows: list[str] = []
    active = False
    for raw in path.read_text(encoding="utf-8", errors="replace").splitlines():
        line = raw.strip()
        if line.startswith("## METRICS CLASS"):
            active = "picard.sam.DuplicationMetrics" in line
            continue
        if active and line and not line.startswith("#"):
            rows.append(line)
        elif active and rows and not line:
            break
    return rows


def compare_outputs(reference_output: Path, candidate_output: Path, reference_metrics: Path, candidate_metrics: Path) -> dict[str, object]:
    compared = 0
    mismatch: dict[str, object] | None = None
    try:
        left = sam_fields(reference_output)
        right = sam_fields(candidate_output)
        while True:
            a = next(left, None)
            b = next(right, None)
            if a is None or b is None:
                if a != b:
                    mismatch = {"record_index": compared, "reason": "record counts differ"}
                break
            if a != b:
                mismatch = {"record_index": compared, "reason": "duplicate-marking semantics differ", "reference": list(a), "candidate": list(b)}
                break
            compared += 1
    except (OSError, ValueError, RuntimeError) as exc:
        return {"status": "ERROR", "comparator": "ordered duplicate flags/tags and alignment identity", "records_compared": compared, "error": str(exc)}
    metrics_compared = has_picard_metrics(reference_metrics) and has_picard_metrics(candidate_metrics)
    metrics_match = None
    if metrics_compared:
        metrics_match = normalized_metrics(reference_metrics) == normalized_metrics(candidate_metrics)
    status = "PASS" if mismatch is None and metrics_match is not False else "FAIL"
    return {
        "status": status,
        "comparator": "ordered duplicate flags/tags and alignment identity" + (" plus normalized Picard DuplicationMetrics" if metrics_compared else " (metrics not comparable)"),
        "records_compared": compared,
        "alignment_mismatch": mismatch,
        "metrics_compared": metrics_compared,
        "metrics_match": metrics_match,
    }


def host_metadata() -> dict[str, object]:
    cpu_model = "unknown"
    cpuinfo = Path("/proc/cpuinfo")
    if cpuinfo.exists():
        match = re.search(r"^model name\s*:\s*(.+)$", cpuinfo.read_text(errors="replace"), re.MULTILINE)
        if match:
            cpu_model = match.group(1)
    return {
        "hostname": platform.node(),
        "os": platform.platform(),
        "architecture": platform.machine(),
        "cpu_model": cpu_model,
        "logical_cpus": os.cpu_count(),
        "python": sys.version.split()[0],
        "storage_note": "not automatically characterized; record device/filesystem externally",
    }


def markdown(report: dict[str, object]) -> str:
    lines = ["# MarkDuplicates competitor benchmark", "", "> Evidence bundle only. No SOTA or production claim is implied.", "", f"Input SHA-256: `{report['input']['sha256']}`", "", "| Tool | Status | Median wall | Median CPU | Peak RSS | Peak temp | Parity |", "|---|---|---:|---:|---:|---:|---|"]
    for name, tool in report["tools"].items():
        if tool["status"] == "unavailable":
            lines.append(f"| {name} | unavailable | — | — | — | — | NOT RUN |")
            continue
        summary = tool.get("summary", {})
        def median(key: str, suffix: str = "") -> str:
            value = summary.get(key, {}).get("median")
            return f"{value:.3f}{suffix}" if value is not None else "—"
        cpu = "—"
        if summary.get("user_cpu_seconds") and summary.get("system_cpu_seconds"):
            cpu = f"{summary['user_cpu_seconds']['median'] + summary['system_cpu_seconds']['median']:.3f}s"
        rss = summary.get("peak_rss_bytes", {}).get("max")
        temp = summary.get("temporary_disk_peak_bytes", {}).get("max")
        parity = tool.get("parity", {}).get("status", "NOT RUN")
        lines.append(f"| {name} | {tool['status']} | {median('wall_seconds', 's')} | {cpu} | {int(rss) if rss is not None else '—'} | {int(temp) if temp is not None else '—'} | {parity} |")
    lines.extend(["", "See `report.json` and per-run raw logs for commands, versions, failures and resource records.", ""])
    return "\n".join(lines)


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", required=True, type=Path, help="coordinate-sorted, duplicate-marking-ready BAM")
    parser.add_argument("--output-dir", required=True, type=Path)
    parser.add_argument("--tools", default="turbo-picard,picard,samtools,fastdup", help="comma-separated installed presets to attempt")
    parser.add_argument("--tool", action="append", default=[], type=parse_custom_tool, metavar="NAME=COMMAND", help="add/override a command template using {input}, {output}, {metrics}, {tmp}, {threads}")
    parser.add_argument("--reference-tool", default="picard")
    parser.add_argument("--require-tools", default="", help="comma-separated tools which must complete and pass/reference parity")
    parser.add_argument("--threads", type=int, default=min(8, os.cpu_count() or 1))
    parser.add_argument("--repeats", type=int, default=5)
    parser.add_argument("--warmups", type=int, default=1)
    parser.add_argument("--disk-sample-ms", type=int, default=100)
    parser.add_argument("--timeout-seconds", type=float)
    parser.add_argument("--source-url")
    parser.add_argument("--source-revision")
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    if not args.input.is_file():
        raise SystemExit(f"input does not exist: {args.input}")
    if args.threads < 1 or args.repeats < 1 or args.warmups < 0 or args.disk_sample_ms < 10:
        raise SystemExit("threads/repeats must be positive, warmups non-negative, and disk sample >= 10 ms")
    output_dir = args.output_dir.resolve()
    if output_dir.exists() and any(output_dir.iterdir()):
        raise SystemExit(f"output directory must be empty to preserve evidence: {output_dir}")
    output_dir.mkdir(parents=True, exist_ok=True)
    presets = preset_tools()
    names = [name.strip() for name in args.tools.split(",") if name.strip()]
    unknown = sorted(set(names) - set(presets))
    if unknown:
        raise SystemExit(f"unknown presets: {', '.join(unknown)}")
    specs: dict[str, ToolSpec | None] = {name: presets[name] for name in names}
    for custom in args.tool:
        specs[custom.name] = custom

    report: dict[str, object] = {
        "schema_version": 1,
        "created_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "claim_status": "evidence_only",
        "input": {
            "path": str(args.input.resolve()),
            "bytes": args.input.stat().st_size,
            "sha256": sha256_file(args.input),
            "source_url": args.source_url,
            "source_revision": args.source_revision,
            "precondition": "coordinate-sorted and prepared as required by every selected tool; runner does not transform input",
        },
        "protocol": {
            "threads": args.threads,
            "repeats": args.repeats,
            "warmups": args.warmups,
            "temporary_disk_sample_ms": args.disk_sample_ms,
            "temporary_disk_scope": "bytes recursively present under the per-run {tmp} directory",
            "resource_meter": "per-run resource_backend; GNU time preferred, POSIX wait4 fallback disclosed",
            "reference_tool": args.reference_tool,
        },
        "host": host_metadata(),
        "tools": {},
    }
    tools_report: dict[str, dict[str, object]] = report["tools"]
    for name, spec in specs.items():
        if spec is None or not Path(spec.command[0]).is_file():
            tools_report[name] = {"status": "unavailable", "reason": "executable not found", "runs": []}
            continue
        tool: dict[str, object] = {
            "status": "running",
            "metrics_kind": spec.metrics_kind,
            "version": capture_version(spec),
            "command_template": list(spec.command),
            "environment_template": dict(spec.environment),
            "runs": [],
        }
        tools_report[name] = tool
        runs: list[RunResult] = []
        for index in range(args.warmups + args.repeats):
            warmup = index < args.warmups
            repeat = index + 1 if warmup else index - args.warmups + 1
            label = f"warmup-{repeat}" if warmup else f"repeat-{repeat}"
            result = run_once(spec, args.input, output_dir / "runs" / name / label, repeat, warmup, args.threads, args.disk_sample_ms / 1000, args.timeout_seconds)
            runs.append(result)
        tool["runs"] = [asdict(run) for run in runs]
        tool["summary"] = summarize_runs(runs)
        tool["status"] = "complete" if tool["summary"]["successful_repeats"] == args.repeats else "incomplete"

    reference = tools_report.get(args.reference_tool)
    reference_run = None
    if reference and reference.get("status") in {"complete", "incomplete"}:
        reference_run = next((run for run in reference["runs"] if not run["warmup"] and run["status"] == "success"), None)
    for name, tool in tools_report.items():
        if tool.get("status") == "unavailable":
            continue
        candidate = next((run for run in tool["runs"] if not run["warmup"] and run["status"] == "success"), None)
        if not reference_run:
            tool["parity"] = {"status": "NOT_RUN", "reason": f"reference tool {args.reference_tool!r} has no successful measured output"}
        elif not candidate:
            tool["parity"] = {"status": "NOT_RUN", "reason": "candidate has no successful measured output"}
        elif name == args.reference_tool:
            tool["parity"] = {"status": "REFERENCE", "comparator": "self"}
        else:
            tool["parity"] = compare_outputs(output_dir / reference_run["output"], output_dir / candidate["output"], output_dir / reference_run["metrics"], output_dir / candidate["metrics"])

    required_names = [name.strip() for name in args.require_tools.split(",") if name.strip()]
    required_failures = []
    for name in required_names:
        tool = tools_report.get(name)
        if tool is None:
            required_failures.append(f"{name}: not selected")
        elif tool.get("status") != "complete":
            required_failures.append(f"{name}: status={tool.get('status')}")
        elif tool.get("parity", {}).get("status") not in {"PASS", "REFERENCE"}:
            required_failures.append(f"{name}: parity={tool.get('parity', {}).get('status', 'NOT_RUN')}")
    report["required_tool_gate"] = {
        "required": required_names,
        "status": "PASS" if not required_failures else "FAIL",
        "failures": required_failures,
    }
    (output_dir / "report.json").write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    (output_dir / "report.md").write_text(markdown(report), encoding="utf-8")
    print(f"wrote {output_dir / 'report.json'}")
    print(f"wrote {output_dir / 'report.md'}")
    if required_failures:
        return 3
    complete = [tool for tool in tools_report.values() if tool.get("status") == "complete"]
    return 0 if complete else 2


if __name__ == "__main__":
    raise SystemExit(main())
