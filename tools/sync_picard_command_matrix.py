#!/usr/bin/env python3
"""Sync docs/command-matrix.yml with the Picard 3.4.0 command surface.

Accelerated commands (native / partial-native) are preserved from the existing
matrix. Every upstream Picard command missing from that set is recorded as
fallback-only so turbo-picard can claim full Picard 3.4.0 CLI parity.
"""

from __future__ import annotations

import argparse
import pathlib
import re
import subprocess
import sys


ROOT = pathlib.Path(__file__).resolve().parents[1]
MATRIX = ROOT / "docs" / "command-matrix.yml"
COMMANDS_LIST = ROOT / "docs" / "picard-3.4.0-commands.txt"
EXPECTED_REFERENCE = "3.4.0"
ACCELERATED_STATUSES = {"native", "partial-native"}
FALLBACK_NATIVE_SCOPE = (
    "Delegated to upstream Picard 3.4.0. turbo-picard runs the native "
    "accelerated implementation when one exists; otherwise it transparently "
    "forwards to upstream Picard when fallback is configured or auto-discovered."
)
FALLBACK_SCOPE = (
    "Transparent upstream Picard 3.4.0 delegation. No native fast path yet."
)
TURBO_ONLY_NATIVE_SCOPE = {
    "AccelerationStatus": (
        "turbo-picard utility command that reports the effective CPU accelerator "
        "policy, HTSlib worker-thread count, detected GPU runtime, and fails when "
        "TURBO_PICARD_ACCELERATOR=gpu-required is set without a production GPU backend."
    ),
}
TURBO_ONLY_FALLBACK_SCOPE = {
    "AccelerationStatus": (
        "This is not an upstream Picard command and does not delegate to fallback."
    ),
}


def load_picard_commands(picard_command: str | None) -> list[str]:
    if picard_command:
        completed = subprocess.run(
            [picard_command, "--list-commands"],
            check=False,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        if completed.returncode != 0:
            raise SystemExit(
                f"failed to list commands from {picard_command}: {completed.stderr.strip()}"
            )
        commands = [line.strip() for line in completed.stdout.splitlines() if line.strip()]
    elif COMMANDS_LIST.is_file():
        commands = [
            line.strip()
            for line in COMMANDS_LIST.read_text(encoding="utf-8").splitlines()
            if line.strip()
        ]
    else:
        raise SystemExit(
            "provide --picard-command or check in docs/picard-3.4.0-commands.txt"
        )
    if not commands:
        raise SystemExit("no Picard commands discovered")
    return sorted(set(commands))


def parse_matrix_entries(text: str) -> list[dict[str, str]]:
    entries: list[dict[str, str]] = []
    current: dict[str, str] | None = None
    for line in text.splitlines():
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


def yaml_quote(value: str) -> str:
    if not value or any(ch in value for ch in ':"{}[],&*#?|-<>=!%@`'):
        return f'"{value.replace(chr(34), chr(92) + chr(34))}"'
    return value


def render_entry(entry: dict[str, str]) -> str:
    lines = [f"  - name: {entry['name']}", f"    status: {entry['status']}"]
    if entry.get("parity_script") not in {None, "", "null"}:
        lines.append(f"    parity_script: {entry['parity_script']}")
    lines.append(f"    native_scope: {yaml_quote(entry['native_scope'])}")
    lines.append(f"    fallback_scope: {yaml_quote(entry['fallback_scope'])}")
    return "\n".join(lines)


def sync_matrix(picard_commands: list[str], dry_run: bool = False) -> int:
    existing = parse_matrix_entries(MATRIX.read_text(encoding="utf-8"))
    by_name = {entry["name"]: entry for entry in existing}

    accelerated = {
        name: entry
        for name, entry in by_name.items()
        if entry.get("status") in ACCELERATED_STATUSES
    }
    turbo_only = {
        name: entry
        for name, entry in by_name.items()
        if name not in picard_commands and entry.get("status") in ACCELERATED_STATUSES
    }

    missing_upstream = [name for name in picard_commands if name not in by_name]
    stale_accelerated = sorted(
        name
        for name, entry in accelerated.items()
        if name in picard_commands and entry.get("status") == "scaffold"
    )

    if stale_accelerated:
        print(
            "warning: accelerated scaffold commands should be migrated manually:",
            ", ".join(stale_accelerated),
            file=sys.stderr,
        )

    merged: list[dict[str, str]] = []
    for name in sorted(by_name):
        if name in picard_commands or name in turbo_only:
            entry = dict(by_name[name])
            if entry.get("status") == "scaffold" and name in picard_commands:
                entry["status"] = "fallback-only"
                entry.pop("parity_script", None)
                entry["native_scope"] = FALLBACK_NATIVE_SCOPE
                entry["fallback_scope"] = FALLBACK_SCOPE
            merged.append(entry)

    for name in picard_commands:
        if name in by_name:
            continue
        merged.append(
            {
                "name": name,
                "status": "fallback-only",
                "native_scope": FALLBACK_NATIVE_SCOPE,
                "fallback_scope": FALLBACK_SCOPE,
            }
        )

    merged.sort(key=lambda entry: entry["name"].casefold())

    rendered = [
        f'picard_reference: "{EXPECTED_REFERENCE}"',
        "commands:",
    ]
    rendered.extend(render_entry(entry) for entry in merged)
    output = "\n".join(rendered) + "\n"

    COMMANDS_LIST.write_text("\n".join(picard_commands) + "\n", encoding="utf-8")

    if dry_run:
        print(output)
        return 0

    MATRIX.write_text(output, encoding="utf-8")
    print(
        f"synced {len(merged)} matrix commands "
        f"({len(accelerated)} accelerated, "
        f"{sum(1 for e in merged if e['status'] == 'fallback-only')} fallback-only); "
        f"added {len(missing_upstream)} upstream commands"
    )
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--picard-command",
        help="Path to upstream picard executable for --list-commands",
    )
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()

    picard_commands = load_picard_commands(args.picard_command)
    return sync_matrix(picard_commands, dry_run=args.dry_run)


if __name__ == "__main__":
    raise SystemExit(main())