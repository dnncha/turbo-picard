#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import re
import sys
from collections import Counter
from dataclasses import asdict, dataclass, field
from pathlib import Path
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from collections.abc import Iterator, Sequence

RUNG_NAMES: dict[int, str] = {
    1: "impossible",
    2: "deleted path",
    3: "rule",
    4: "sweep",
    5: "assertion",
    6: "property or fuzz",
    7: "regression test",
    8: "production",
}
CLASS_KILLING = frozenset({1, 2, 3, 4})
UNWIRED = re.compile(
    r"\b(by hand|manual(ly)?|not scheduled|unscheduled|on demand|ad hoc|when someone remembers|occasional(ly)?)\b",
    re.IGNORECASE,
)
SCHEDULED_TIERS = re.compile(
    r"^(nightly|weekly|daily|hourly|occasional|on demand|bench|scheduled|canary)", re.IGNORECASE
)
WIRED_TRIGGERS = re.compile(
    r"\b(compiler|editor|save|hook|pre-commit|commit|inner|gate|merge|ci|release|deploy|scheduled|cron|launchd"
    r"|live|alert|human|per release|every change)\b",
    re.IGNORECASE,
)
SCHEDULED_RUNS = re.compile(r"scheduled\b.{3,}", re.IGNORECASE)
REASON = re.compile(r"above:\s*\S.{10,}", re.IGNORECASE)
CLIMBABLE = re.compile(r"climbable:", re.IGNORECASE)
RUNG_NUMBER = re.compile(r"\s*([1-8])\b")

Row = tuple[int, list[str]]


@dataclass(frozen=True)
class Problem:
    line: int
    message: str


@dataclass(frozen=True)
class Signal:
    line: int
    tier: str
    runs: str
    signal: str


@dataclass(frozen=True)
class Escape:
    line: int
    rung: int
    bug_class: str
    cell: str


def no_signals() -> list[Signal]:
    return []


def no_escapes() -> list[Escape]:
    return []


def no_problems() -> list[Problem]:
    return []


@dataclass
class Net:
    signals: list[Signal] = field(default_factory=no_signals)
    escapes: list[Escape] = field(default_factory=no_escapes)
    problems: list[Problem] = field(default_factory=no_problems)


def cells(line: str) -> list[str]:
    body = line.strip().removeprefix("|").removesuffix("|")
    return [c.strip() for c in re.split(r"(?<!\\)\|", body)]


def tables(text: str) -> Iterator[tuple[str, list[Row]]]:
    heading = ""
    rows: list[Row] = []
    for number, line in enumerate(text.splitlines(), 1):
        if line.startswith("#"):
            if rows:
                yield heading, rows
                rows = []
            heading = line.lstrip("#").strip()
            continue
        if line.lstrip().startswith("|"):
            rows.append((number, cells(line)))
        elif rows:
            yield heading, rows
            rows = []
    if rows:
        yield heading, rows


def is_separator(row: list[str]) -> bool:
    return all(set(c) <= set("-: ") for c in row)


def column(header: list[str], *names: str) -> int | None:
    for index, name in enumerate(header):
        if any(name == n or name.startswith(n) for n in names):
            return index
    return None


def cell(row: list[str], index: int | None) -> str:
    return row[index] if index is not None and index < len(row) else ""


def check_signal(tier: str, runs: str, label: str) -> str | None:
    if tier.lower().startswith("human"):
        return None
    if not runs:
        return f"signal '{label}' has an empty Runs cell"
    if UNWIRED.search(runs):
        return f"signal '{label}' is written but not wired: Runs says '{runs[:60]}'. Schedule it, gate it, or move it to Holes"
    if SCHEDULED_TIERS.match(tier) and not SCHEDULED_RUNS.match(runs):
        return f"signal '{label}' is tier '{tier}' but Runs does not say 'scheduled: <where, when>'"
    if not WIRED_TRIGGERS.search(runs):
        return (
            f"signal '{label}' Runs cell '{runs[:60]}' names no trigger (hook, inner, gate, release, scheduled, live)"
        )
    return None


def read_signals(net: Net, heading: str, rows: list[Row]) -> None:
    header = [h.lower() for h in rows[0][1]]
    tier_i = column(header, "tier")
    runs_i = column(header, "runs")
    name_i = column(header, "signal", "command")
    if runs_i is None:
        net.problems.append(
            Problem(
                rows[0][0], f"signals table under '{heading}' has no 'Runs' column naming what triggers each signal"
            )
        )
        return
    for number, row in rows[1:]:
        if is_separator(row):
            continue
        signal = Signal(number, cell(row, tier_i), cell(row, runs_i), cell(row, name_i))
        net.signals.append(signal)
        message = check_signal(signal.tier, signal.runs, (signal.signal or signal.tier)[:70])
        if message:
            net.problems.append(Problem(number, message))


def read_escapes(net: Net, heading: str, rows: list[Row]) -> None:
    header = [h.lower() for h in rows[0][1]]
    rung_i = column(header, "rung")
    class_i = column(header, "class", "bug class")
    if rung_i is None:
        net.problems.append(Problem(rows[0][0], f"escape log under '{heading}' has no 'Rung' column"))
        return
    for number, row in rows[1:]:
        if is_separator(row):
            continue
        text = cell(row, rung_i)
        found = RUNG_NUMBER.match(text)
        if not found:
            net.problems.append(Problem(number, f"escape row has no rung number 1-8 (got '{text[:40]}')"))
            continue
        rung = int(found.group(1))
        net.escapes.append(Escape(number, rung, cell(row, class_i), text))
        if rung not in CLASS_KILLING and not REASON.search(text):
            net.problems.append(
                Problem(
                    number,
                    f"escape guarded at rung {rung} ({RUNG_NAMES[rung]}) "
                    "without 'above: <why rungs 1-4 are impossible here>'",
                )
            )


def check(text: str) -> Net:
    net = Net()
    saw_signals = saw_escapes = False
    for heading, rows in tables(text):
        lowered = heading.lower()
        if "signal" in lowered or lowered.startswith("commands"):
            saw_signals = True
            read_signals(net, heading, rows)
        if "escape" in lowered:
            saw_escapes = True
            read_escapes(net, heading, rows)
    if not saw_signals:
        net.problems.append(Problem(0, "no signals table found"))
    if not saw_escapes:
        net.problems.append(Problem(0, "no escape log found"))
    return net


def class_key(text: str) -> str:
    return re.sub(r"[^a-z0-9 ]", "", text.lower()).strip()


def open_classes(escapes: Sequence[Escape]) -> list[list[Escape]]:
    groups: dict[str, list[Escape]] = {}
    for escape in escapes:
        if escape.bug_class:
            groups.setdefault(class_key(escape.bug_class), []).append(escape)
    return [rows for rows in groups.values() if len(rows) >= 2 and min(r.rung for r in rows) not in CLASS_KILLING]


def say(line: str) -> None:
    sys.stdout.write(line + "\n")


def report(path: str, as_json: bool) -> bool:
    net = check(Path(path).read_text())
    rungs = Counter(e.rung for e in net.escapes)
    killed = sum(rungs[r] for r in CLASS_KILLING)
    climbable = [e for e in net.escapes if CLIMBABLE.search(e.cell)]
    groups = open_classes(net.escapes)
    if as_json:
        say(
            json.dumps(
                {
                    "file": path,
                    "signals": len(net.signals),
                    "escapes": len(net.escapes),
                    "rungs": {str(k): rungs[k] for k in sorted(rungs)},
                    "class_killing": killed,
                    "climbable": [asdict(e) for e in climbable],
                    "open_classes": [[asdict(e) for e in g] for g in groups],
                    "problems": [asdict(p) for p in net.problems],
                },
                indent=2,
            )
        )
        return not net.problems
    say(f"{path}: {len(net.signals)} signals, {len(net.escapes)} escapes")
    if net.escapes:
        say("  rungs: " + ", ".join(f"{k} {RUNG_NAMES[k]}: {rungs[k]}" for k in sorted(rungs)))
        say(f"  class-killing guards (rungs 1-4): {killed} of {len(net.escapes)}")
    if climbable:
        say(f"  deferred climbs (row says a higher rung is possible): {len(climbable)}")
        for e in climbable:
            say(f"    {path}:{e.line}: rung {e.rung}: {e.cell[:110]}")
    for group in groups:
        say(
            f"  open class (recurs, no class-killing guard): '{group[0].bug_class[:70]}' "
            f"at lines {[e.line for e in group]}, best rung {min(e.rung for e in group)}"
        )
    for problem in net.problems:
        say(f"{path}:{problem.line}: {problem.message}")
    return not net.problems


def main(argv: Sequence[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Check a NET.md: every signal wired to a trigger, every escape guard on a "
        "named rung, with a reason when it is below rung 4."
    )
    parser.add_argument("paths", nargs="*", default=["NET.md"])
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args(argv)
    results = [report(p, args.json) for p in args.paths]
    return 0 if all(results) else 1


if __name__ == "__main__":
    sys.exit(main())
