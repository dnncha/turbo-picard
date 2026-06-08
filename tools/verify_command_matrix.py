#!/usr/bin/env python3
import pathlib
import re
import sys
import json


ROOT = pathlib.Path(__file__).resolve().parents[1]
MATRIX = ROOT / "docs" / "command-matrix.yml"
PICARD_COMMANDS = ROOT / "docs" / "picard-3.4.0-commands.txt"
COMMAND_DOCS = ROOT / "docs" / "commands.rst"
CLI = ROOT / "crates" / "turbo-picard-cli" / "src" / "lib.rs"
CI = ROOT / ".github" / "workflows" / "ci.yml"
REAL_DATA_MANIFEST = ROOT / "benchmarks" / "real-data" / "manifest.json"
CRAM_PARITY_SCRIPT = ROOT / "tools" / "verify_basic_cram_parity.sh"
EXPECTED_PICARD_REFERENCE = "3.4.0"
VALID_STATUSES = {"native", "partial-native", "scaffold", "fallback-only"}
ACCELERATED_STATUSES = {"native", "partial-native", "scaffold"}


def matrix_documented_commands():
    commands = []
    current_name = None
    for line in MATRIX.read_text(encoding="utf-8").splitlines():
        name_match = re.match(r"\s+- name: (.+)", line)
        if name_match:
            current_name = name_match.group(1)
            continue
        status_match = re.match(
            r"\s+status: (native|partial-native|scaffold|fallback-only)", line
        )
        if status_match and current_name:
            commands.append(current_name)
    return set(commands)


def matrix_picard_reference(text=None):
    text = MATRIX.read_text(encoding="utf-8") if text is None else text
    for line in text.splitlines():
        reference_match = re.match(r'picard_reference:\s+"?([^"]+)"?\s*$', line)
        if reference_match:
            return reference_match.group(1)
    return None


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


def matrix_accelerated_commands():
    commands = []
    current_name = None
    for line in MATRIX.read_text(encoding="utf-8").splitlines():
        name_match = re.match(r"\s+- name: (.+)", line)
        if name_match:
            current_name = name_match.group(1)
            continue
        status_match = re.match(
            r"\s+status: (native|partial-native|scaffold)", line
        )
        if status_match and current_name:
            commands.append(current_name)
    return set(commands)


def picard_reference_commands():
    if not PICARD_COMMANDS.is_file():
        return set()
    return {
        line.strip()
        for line in PICARD_COMMANDS.read_text(encoding="utf-8").splitlines()
        if line.strip()
    }


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


def validate_matrix_entry_structure(entries):
    errors = []
    seen = set()
    for entry in entries:
        name = entry.get("name", "").strip()
        if not name:
            errors.append("command matrix entry missing name")
            continue
        if name in seen:
            errors.append(f"command matrix has duplicate command entry: {name}")
        seen.add(name)
        status = entry.get("status", "").strip()
        if status not in VALID_STATUSES:
            errors.append(f"{name} has invalid status: {status or '<missing>'}")
        parity_script = entry.get("parity_script", "").strip()
        if status in {"native", "partial-native", "scaffold"} and not parity_script:
            errors.append(f"{name} missing parity_script")
        if status == "fallback-only" and parity_script not in {"", "null"}:
            errors.append(f"{name} fallback-only entry should not declare parity_script")
    return errors


def validate_picard_reference(reference, expected=EXPECTED_PICARD_REFERENCE):
    if reference is None:
        return ["command matrix missing picard_reference"]
    if reference != expected:
        return [
            f"command matrix picard_reference must be {expected}, got {reference}"
        ]
    return []


def release_candidate_real_data_commands(manifest_text=None):
    text = REAL_DATA_MANIFEST.read_text(encoding="utf-8") if manifest_text is None else manifest_text
    try:
        manifest = json.loads(text)
    except ValueError:
        return set()
    commands = set()
    for dataset in manifest.get("datasets", []):
        if not isinstance(dataset, dict):
            continue
        if dataset.get("release_tier") != "release_candidate":
            continue
        expected_commands = dataset.get("expected_commands", {})
        if isinstance(expected_commands, dict):
            commands.update(expected_commands)
    return commands


def validate_real_data_scope_claims(entries, release_candidate_commands):
    errors = []
    for entry in entries:
        scope = entry.get("native_scope", "")
        if "real-data parity" not in scope.lower():
            continue
        command = entry["name"]
        if command not in release_candidate_commands:
            errors.append(
                f"{command} command matrix claims real-data parity but has no release_candidate manifest evidence"
            )
    return errors


def validate_release_candidate_scope_mentions(entries, release_candidate_commands):
    entries_by_name = {
        entry.get("name", ""): entry
        for entry in entries
        if isinstance(entry.get("name", ""), str)
    }
    errors = []
    for command in sorted(release_candidate_commands):
        entry = entries_by_name.get(command)
        if entry is None:
            errors.append(
                f"{command} has release_candidate manifest evidence but is missing from command matrix"
            )
            continue
        if "real-data parity" not in entry.get("native_scope", "").lower():
            errors.append(
                f"{command} has release_candidate manifest evidence but command matrix native_scope does not mention real-data parity"
            )
    return errors


def cram_parity_commands(script_text=None):
    text = CRAM_PARITY_SCRIPT.read_text(encoding="utf-8") if script_text is None else script_text
    match = re.search(r"CRAM hot-path parity passed for:\s*([^\n\"]+)", text)
    if not match:
        return set()
    return {
        command.strip()
        for command in match.group(1).split(",")
        if command.strip()
    }


def validate_cram_scope_mentions(entries, cram_commands):
    entries_by_name = {
        entry.get("name", ""): entry
        for entry in entries
        if isinstance(entry.get("name", ""), str)
    }
    errors = []
    for command in sorted(cram_commands):
        entry = entries_by_name.get(command)
        if entry is None:
            errors.append(
                f"{command} has CRAM parity coverage but is missing from command matrix"
            )
            continue
        if "cram" not in entry.get("native_scope", "").lower():
            errors.append(
                f"{command} has CRAM parity coverage but command matrix native_scope does not mention CRAM"
            )
    return errors


def validate_command_docs_scope_language(text):
    errors = []
    if "Common native command examples" in text:
        errors.append(
            "commands docs must not label mixed native/partial-native examples as native"
        )
    for needle, description in [
        ("Picard 3.4.0", "Picard 3.4.0 compatibility wording"),
        ("--list-commands", "list-commands pointer"),
        ("machine-readable matrix", "matrix pointer"),
        ("accelerated", "accelerated command wording"),
        ("delegated", "delegated command wording"),
    ]:
        if needle not in text:
            errors.append(f"commands docs missing {description}")
    return errors


def validate_command_docs_examples(entries, text):
    errors = []
    if re.search(r"\bpicard\s+ViewSam\b[^\n]*\bO=", text):
        errors.append(
            "commands docs must show upstream ViewSam writing to stdout, not O= output"
        )
    for entry in entries:
        status = entry.get("status", "")
        command = entry.get("name", "")
        if status in ACCELERATED_STATUSES and command not in text:
            errors.append(f"commands docs missing example for matrix command: {command}")
    return errors


def validate_command_docs_status_summary(entries, text):
    errors = []
    for entry in entries:
        command = entry.get("name", "")
        status = entry.get("status", "")
        if not command or not status:
            continue
        if status not in ACCELERATED_STATUSES:
            continue
        status_line = f"* ``{command}``: ``{status}``"
        if status_line not in text:
            errors.append(
                f"commands docs missing matrix status summary for {command}: {status}"
            )
    accelerated_count = sum(
        1 for entry in entries if entry.get("status") in ACCELERATED_STATUSES
    )
    fallback_count = sum(
        1 for entry in entries if entry.get("status") == "fallback-only"
    )
    if f"{accelerated_count} accelerated" not in text:
        errors.append("commands docs missing accelerated command count summary")
    if f"{fallback_count} delegated" not in text:
        errors.append("commands docs missing delegated command count summary")
    return errors


def validate_picard_reference_coverage(entries):
    errors = []
    matrix_names = {entry.get("name", "") for entry in entries}
    for command in sorted(picard_reference_commands()):
        if command not in matrix_names:
            errors.append(
                f"command matrix missing upstream Picard 3.4.0 command: {command}"
            )
    for entry in entries:
        name = entry.get("name", "")
        status = entry.get("status", "")
        if name in picard_reference_commands() and status == "scaffold":
            errors.append(
                f"{name} is still scaffold; migrate to accelerated or fallback-only"
            )
    return errors


def validate_parity_script_files(scripts, root=ROOT):
    missing_scripts = [script for script in scripts if not (root / script).is_file()]
    if missing_scripts:
        return [
            "matrix parity scripts missing from repository: "
            + ", ".join(missing_scripts)
        ]
    return []


def validate_parity_script_ci_coverage(scripts, ci_text):
    scripts_missing_from_ci = [script for script in scripts if script not in ci_text]
    if scripts_missing_from_ci:
        return [
            "matrix parity scripts missing from CI: "
            + ", ".join(scripts_missing_from_ci)
        ]
    return []


def main():
    reference_errors = validate_picard_reference(matrix_picard_reference())
    if reference_errors:
        for error in reference_errors:
            print(error, file=sys.stderr)
        return 1
    matrix = matrix_documented_commands()
    accelerated = matrix_accelerated_commands()
    dispatch = dispatcher_commands()
    missing = sorted(dispatch - matrix)
    stale = sorted(accelerated - dispatch)
    unexpected_dispatch = sorted(dispatch - matrix - {"AccelerationStatus"})
    if missing or stale or unexpected_dispatch:
        if missing:
            print("dispatcher commands missing from matrix:", ", ".join(missing), file=sys.stderr)
        if stale:
            print(
                "matrix accelerated commands missing from dispatcher:",
                ", ".join(stale),
                file=sys.stderr,
            )
        if unexpected_dispatch:
            print(
                "dispatcher includes undocumented commands:",
                ", ".join(unexpected_dispatch),
                file=sys.stderr,
            )
        return 1
    entries = matrix_entries()
    reference_errors = validate_picard_reference_coverage(entries)
    if reference_errors:
        for error in reference_errors:
            print(error, file=sys.stderr)
        return 1
    structure_errors = validate_matrix_entry_structure(entries)
    if structure_errors:
        for error in structure_errors:
            print(error, file=sys.stderr)
        return 1
    parity_scripts = matrix_parity_scripts()
    script_file_errors = validate_parity_script_files(parity_scripts)
    if script_file_errors:
        for error in script_file_errors:
            print(error, file=sys.stderr)
        return 1
    scope_errors = validate_scope_notes(matrix_entries())
    if scope_errors:
        for error in scope_errors:
            print(error, file=sys.stderr)
        return 1
    real_data_scope_errors = validate_real_data_scope_claims(
        matrix_entries(),
        release_candidate_real_data_commands(),
    )
    if real_data_scope_errors:
        for error in real_data_scope_errors:
            print(error, file=sys.stderr)
        return 1
    release_candidate_scope_errors = validate_release_candidate_scope_mentions(
        matrix_entries(),
        release_candidate_real_data_commands(),
    )
    if release_candidate_scope_errors:
        for error in release_candidate_scope_errors:
            print(error, file=sys.stderr)
        return 1
    cram_scope_errors = validate_cram_scope_mentions(
        matrix_entries(),
        cram_parity_commands(),
    )
    if cram_scope_errors:
        for error in cram_scope_errors:
            print(error, file=sys.stderr)
        return 1
    command_docs_errors = validate_command_docs_scope_language(
        COMMAND_DOCS.read_text(encoding="utf-8")
    )
    command_docs_errors.extend(
        validate_command_docs_examples(
            matrix_entries(),
            COMMAND_DOCS.read_text(encoding="utf-8"),
        )
    )
    command_docs_errors.extend(
        validate_command_docs_status_summary(
            matrix_entries(),
            COMMAND_DOCS.read_text(encoding="utf-8"),
        )
    )
    if command_docs_errors:
        for error in command_docs_errors:
            print(error, file=sys.stderr)
        return 1
    ci_text = CI.read_text(encoding="utf-8")
    script_ci_errors = validate_parity_script_ci_coverage(parity_scripts, ci_text)
    if script_ci_errors:
        for error in script_ci_errors:
            print(error, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
