"""Attempt user-space native sampling; record unavailable/denied, never fake it.

Usage: python scripts/native_profile.py OUTPUT_DIR -- COMMAND [ARGUMENTS ...]
No sudo, privilege changes, package installation or outbound network requests.
Profiling timings are not used for uninstrumented speedup claims.
"""
import argparse
import json
import shutil
import subprocess
from pathlib import Path


def capture(output, command, perf=None):
    if not command:
        raise ValueError("a command is required")
    output.mkdir(parents=True, exist_ok=False)
    executable = perf or shutil.which("perf")
    report = dict(schema="hmmforge.native-profile.v1", command=command,
                  status="unavailable", valid_native_profile=False)
    if executable:
        cmd = [executable, "record", "-F", "99", "-e", "cpu-clock:u", "--call-graph", "dwarf,4096",
               "-o", str(output/"perf.data"), "--", *command]
        try:
            with open(output/"record.stdout.txt", "w") as stdout, open(output/"record.stderr.txt", "w") as stderr:
                process = subprocess.run(cmd, stdout=stdout, stderr=stderr, timeout=1800, check=False)
            report["record_returncode"] = process.returncode
            report["status"] = "failed_or_denied"
            if process.returncode == 0:
                result = subprocess.run([executable, "report", "--stdio", "--no-children", "--percent-limit", "0.5",
                                         "--sort=dso,symbol", "-i", str(output/"perf.data")],
                                        text=True, capture_output=True, timeout=60, check=False)
                (output/"report.txt").write_text(result.stdout)
                (output/"report.stderr.txt").write_text(result.stderr)
                has_samples = "# Samples:" in result.stdout
                report.update(report_returncode=result.returncode,
                              status="captured" if result.returncode == 0 and has_samples else "empty_or_unreadable",
                              valid_native_profile=result.returncode == 0 and has_samples)
        except (OSError, subprocess.TimeoutExpired) as exc:
            report.update(status="failed_or_denied", error=str(exc))
    (output/"status.json").write_text(json.dumps(report, indent=2)+"\n")
    return report


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("output", type=Path)
    p.add_argument("--perf")
    p.add_argument("command", nargs=argparse.REMAINDER)
    args = p.parse_args()
    command = args.command[1:] if args.command[:1] == ["--"] else args.command
    print(json.dumps(capture(args.output, command, args.perf), sort_keys=True))
