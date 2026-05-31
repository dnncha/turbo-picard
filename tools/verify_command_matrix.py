#!/usr/bin/env python3
import pathlib
import re
import sys


ROOT = pathlib.Path(__file__).resolve().parents[1]
MATRIX = ROOT / "docs" / "command-matrix.yml"
CLI = ROOT / "crates" / "turbo-picard-cli" / "src" / "lib.rs"
CI = ROOT / ".github" / "workflows" / "ci.yml"


def matrix_native_commands():
    commands = []
    current_name = None
    for line in MATRIX.read_text(encoding="utf-8").splitlines():
        name_match = re.match(r"\s+- name: (.+)", line)
        if name_match:
            current_name = name_match.group(1)
            continue
        status_match = re.match(r"\s+status: (native|partial-native)", line)
        if status_match and current_name:
            commands.append(current_name)
    return set(commands)


def matrix_parity_scripts():
    scripts = []
    for line in MATRIX.read_text(encoding="utf-8").splitlines():
        script_match = re.match(r"\s+parity_script: (.+)", line)
        if script_match:
            script = script_match.group(1).strip().strip('"')
            if script != "null":
                scripts.append(script)
    return scripts


def matrix_entries():
    entries = []
    current = None
    for line in MATRIX.read_text(encoding="utf-8").splitlines():
        name_match = re.match(r"\s+- name: (.+)", line)
        if name_match:
            if current:
                entries.append(current)
            current = {"name": name_match.group(1)}
            continue
        field_match = re.match(r"\s+([a-z_]+): (.+)", line)
        if field_match and current is not None:
            key = field_match.group(1)
            value = field_match.group(2).strip().strip('"')
            current[key] = value
    if current:
        entries.append(current)
    return entries


def dispatcher_commands():
    text = CLI.read_text(encoding="utf-8")
    text = text.split("fn print_top_level_help", 1)[0]
    return set(re.findall(r'Some\("([^"]+)"\) =>', text))


def validate_scope_notes(entries):
    errors = []
    for entry in entries:
        for key in ("native_scope", "fallback_scope"):
            value = entry.get(key, "").strip()
            if not value:
                errors.append(f"{entry['name']} missing {key}")
            elif re.search(r"\b(TBD|TODO|unknown)\b", value, re.IGNORECASE):
                errors.append(f"{entry['name']} has vague {key}: {value}")
    return errors


def main():
    matrix = matrix_native_commands()
    dispatch = dispatcher_commands()
    missing = sorted(dispatch - matrix)
    stale = sorted(matrix - dispatch)
    if missing or stale:
        if missing:
            print("dispatcher commands missing from matrix:", ", ".join(missing), file=sys.stderr)
        if stale:
            print("matrix native commands missing from dispatcher:", ", ".join(stale), file=sys.stderr)
        return 1
    missing_scripts = [
        script for script in matrix_parity_scripts() if not (ROOT / script).is_file()
    ]
    if missing_scripts:
        print(
            "matrix parity scripts missing from repository: "
            + ", ".join(missing_scripts),
            file=sys.stderr,
        )
        return 1
    scope_errors = validate_scope_notes(matrix_entries())
    if scope_errors:
        for error in scope_errors:
            print(error, file=sys.stderr)
        return 1
    ci_text = CI.read_text(encoding="utf-8")
    scripts_missing_from_ci = [
        script for script in matrix_parity_scripts() if script not in ci_text
    ]
    if scripts_missing_from_ci:
        print(
            "matrix parity scripts missing from CI: "
            + ", ".join(scripts_missing_from_ci),
            file=sys.stderr,
        )
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
