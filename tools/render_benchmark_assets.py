#!/usr/bin/env python3
"""Render static benchmark assets for the README and marketing site."""

from __future__ import annotations

import argparse
import html
import json
import math
import pathlib


ROOT = pathlib.Path(__file__).resolve().parents[1]
ASSET_DIR = ROOT / "docs" / "site" / "assets"
DEFAULT_SUITE_OUTPUT = ASSET_DIR / "bench-suite-output.txt"


def parse_key_values(line: str) -> dict[str, str]:
    values = {}
    for chunk in line.split():
        if "=" not in chunk:
            continue
        key, value = chunk.split("=", 1)
        values[key] = value
    return values


def build_benchmark_data_from_rows(
    rows: list[dict[str, object]],
    *,
    source: str,
    date: str,
    source_artifact: str | None = None,
) -> dict:
    if not rows:
        raise ValueError("benchmark data requires at least one row")
    seen_commands: set[str] = set()
    for index, row in enumerate(rows):
        command = row.get("command")
        if not isinstance(command, str) or not command:
            raise ValueError(f"benchmark row {index} missing command")
        if command in seen_commands:
            raise ValueError(f"duplicate benchmark command: {command}")
        seen_commands.add(command)
        if row.get("parity") not in {"PASS", "FAIL"}:
            raise ValueError(f"benchmark row {command} has invalid parity")
        try:
            speedup = float(row["speedup"])
        except (KeyError, TypeError, ValueError) as error:
            raise ValueError(f"benchmark row {command} missing numeric speedup") from error
        if speedup <= 0:
            raise ValueError(f"benchmark row {command} speedup must be positive")

    speedups = [float(row["speedup"]) for row in rows]
    top = max(rows, key=lambda row: float(row["speedup"]))
    floor = min(rows, key=lambda row: float(row["speedup"]))
    geometric_mean = math.prod(speedups) ** (1 / len(speedups))
    ordered = sorted(rows, key=lambda row: float(row["speedup"]), reverse=True)
    pass_count = sum(1 for row in rows if row["parity"] == "PASS")
    data = {
        "source": source,
        "date": date,
        "parity": f"{pass_count}/{len(rows)} PASS",
        "summary": {
            "command_count": len(rows),
            "parity_pass_count": pass_count,
            "top_speedup": float(top["speedup"]),
            "top_command": top["command"],
            "floor_speedup": float(floor["speedup"]),
            "floor_command": floor["command"],
            "median_speedup": round(sorted(speedups)[len(speedups) // 2], 2),
            "geometric_mean_speedup": round(geometric_mean, 2),
        },
        "benchmarks": [
            {
                "rank": rank,
                "command": row["command"],
                "speedup": float(row["speedup"]),
                "parity": row["parity"],
            }
            for rank, row in enumerate(ordered, start=1)
        ],
    }
    if source_artifact:
        data["source_artifact"] = source_artifact
    return data


def build_benchmark_data() -> dict:
    text = DEFAULT_SUITE_OUTPUT.read_text(encoding="utf-8")
    return build_benchmark_data_from_suite_output(
        text,
        source_artifact="docs/site/assets/bench-suite-output.txt",
    )


def parse_suite_metadata(text: str) -> dict[str, str]:
    for line in text.splitlines():
        if "benchmark_date=" not in line:
            continue
        values = parse_key_values(line)
        if "source=" in line:
            values["source"] = line.split("source=", 1)[1].strip()
        return values
    return {}


def build_benchmark_data_from_suite_output(
    text: str,
    *,
    source: str | None = None,
    date: str | None = None,
    source_artifact: str | None = None,
) -> dict:
    metadata = parse_suite_metadata(text)
    resolved_source = source or metadata.get(
        "source", "python3 tools/bench_suite.py --repeats 1 --skip-build"
    )
    resolved_date = date or metadata.get("benchmark_date", "2026-05-29")
    rows = []
    for line in text.splitlines():
        values = parse_key_values(line)
        if not values:
            continue
        if {"command", "median_speedup", "parity"} - values.keys():
            continue
        rows.append(
            {
                "command": values["command"],
                "speedup": float(values["median_speedup"].removesuffix("x")),
                "parity": values["parity"],
            }
        )
    return build_benchmark_data_from_rows(
        rows, source=resolved_source, date=resolved_date, source_artifact=source_artifact
    )


def write_json(data: dict) -> None:
    ASSET_DIR.mkdir(parents=True, exist_ok=True)
    (ASSET_DIR / "benchmark-data.json").write_text(
        json.dumps(data, indent=2) + "\n", encoding="utf-8"
    )


def render_speedup_chart(data: dict) -> str:
    benchmarks = data["benchmarks"]
    summary = data["summary"]
    width = 1280
    row_h = 38
    top = 110
    left = 310
    plot_w = 800
    height = top + len(benchmarks) * row_h + 72
    max_speedup = max(10, math.ceil(summary["top_speedup"] / 5) * 5)
    rows = []
    for index, benchmark in enumerate(benchmarks):
        command = benchmark["command"]
        speedup = benchmark["speedup"]
        y = top + index * row_h
        bar_w = max(3, speedup / max_speedup * plot_w)
        color = "#36d7c9" if speedup >= 20 else "#f4b860" if speedup >= 10 else "#ff7a7a"
        rows.append(
            f"""
  <text x="{left - 18}" y="{y + 22}" text-anchor="end" class="label">{html.escape(command)}</text>
  <rect x="{left}" y="{y}" width="{plot_w}" height="24" rx="6" class="track"/>
  <rect x="{left}" y="{y}" width="{bar_w:.1f}" height="24" rx="6" fill="{color}"/>
  <text x="{left + bar_w + 12:.1f}" y="{y + 18}" class="value">{speedup:.2f}x</text>"""
        )

    ticks = []
    tick_step = 10 if max_speedup <= 50 else 20
    for tick in range(tick_step, max_speedup + 1, tick_step):
        x = left + tick / max_speedup * plot_w
        ticks.append(
            f'<line x1="{x:.1f}" y1="92" x2="{x:.1f}" y2="{height - 48}" class="grid"/>'
            f'<text x="{x:.1f}" y="{height - 22}" text-anchor="middle" class="tick">{tick}x</text>'
        )

    command_count = summary["command_count"]
    pass_count = summary["parity_pass_count"]
    floor_speedup = summary["floor_speedup"]
    top_speedup = summary["top_speedup"]
    source = data["source"]
    date = data["date"]
    return f"""<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}" role="img" aria-labelledby="title desc">
  <title id="title">Same Picard checks. Less waiting.</title>
  <desc id="desc">Bar chart showing {command_count} Picard-compatible commands with parity passing and speedups from {floor_speedup:.2f}x to {top_speedup:.2f}x.</desc>
  <defs>
    <linearGradient id="bg" x1="0" x2="1" y1="0" y2="1">
      <stop offset="0" stop-color="#071013"/>
      <stop offset="0.55" stop-color="#10212a"/>
      <stop offset="1" stop-color="#0b1117"/>
    </linearGradient>
    <filter id="softGlow" x="-20%" y="-20%" width="140%" height="140%">
      <feGaussianBlur stdDeviation="8" result="blur"/>
      <feMerge><feMergeNode in="blur"/><feMergeNode in="SourceGraphic"/></feMerge>
    </filter>
  </defs>
  <style>
    .title {{ font: 700 34px ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; fill: #f4fbfb; }}
    .sub {{ font: 500 16px ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; fill: #b7c9cf; }}
    .label {{ font: 600 14px ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; fill: #d9e7ea; }}
    .value {{ font: 700 14px ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; fill: #f8fbfb; }}
    .tick {{ font: 600 12px ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; fill: #8ba4ad; }}
    .track {{ fill: rgba(255,255,255,0.075); }}
    .grid {{ stroke: rgba(255,255,255,0.11); stroke-width: 1; }}
  </style>
  <rect width="{width}" height="{height}" rx="28" fill="url(#bg)"/>
  <path d="M1035 62 C1125 94 1165 154 1200 252" fill="none" stroke="#36d7c9" stroke-width="2" opacity="0.34" filter="url(#softGlow)"/>
  <text x="56" y="54" class="title">Same Picard checks. Less waiting.</text>
  <text x="56" y="82" class="sub">{pass_count}/{command_count} parity checks passing, from {floor_speedup:.2f}x to {top_speedup:.2f}x in the saved benchmark suite.</text>
  {"".join(ticks)}
  {"".join(rows)}
</svg>
"""


def render_hero() -> str:
    return """<svg xmlns="http://www.w3.org/2000/svg" width="1600" height="900" viewBox="0 0 1600 900" role="img" aria-labelledby="title desc">
  <title id="title">accelerated genomic pipeline hero artwork</title>
  <desc id="desc">Abstract genomic read streams flowing through a high-performance compute pipeline into benchmark bars.</desc>
  <defs>
    <linearGradient id="heroBg" x1="0" x2="1" y1="0" y2="1">
      <stop offset="0" stop-color="#071013"/>
      <stop offset="0.45" stop-color="#10212a"/>
      <stop offset="1" stop-color="#061115"/>
    </linearGradient>
    <linearGradient id="read" x1="0" x2="1">
      <stop offset="0" stop-color="#33d6c6" stop-opacity="0"/>
      <stop offset="0.2" stop-color="#33d6c6"/>
      <stop offset="0.74" stop-color="#9ef8f1"/>
      <stop offset="1" stop-color="#33d6c6" stop-opacity="0"/>
    </linearGradient>
    <linearGradient id="amber" x1="0" x2="1">
      <stop offset="0" stop-color="#f4b860"/>
      <stop offset="1" stop-color="#ffde9a"/>
    </linearGradient>
    <filter id="glow" x="-25%" y="-25%" width="150%" height="150%">
      <feGaussianBlur stdDeviation="9" result="blur"/>
      <feMerge><feMergeNode in="blur"/><feMergeNode in="SourceGraphic"/></feMerge>
    </filter>
  </defs>
  <rect width="1600" height="900" fill="url(#heroBg)"/>
  <g opacity="0.23">
    <path d="M-50 695 C255 557 477 618 734 472 C975 335 1207 302 1660 185" fill="none" stroke="#7ceee5" stroke-width="2"/>
    <path d="M-50 740 C345 592 493 678 842 485 C1093 347 1314 377 1660 246" fill="none" stroke="#7ceee5" stroke-width="1.5"/>
    <path d="M-50 782 C225 676 520 742 796 585 C1106 409 1250 458 1660 330" fill="none" stroke="#f4b860" stroke-width="1.3"/>
  </g>
  <g filter="url(#glow)">
    <rect x="700" y="172" width="650" height="9" rx="4.5" fill="url(#read)" opacity="0.9"/>
    <rect x="620" y="224" width="790" height="9" rx="4.5" fill="url(#read)" opacity="0.65"/>
    <rect x="760" y="276" width="545" height="9" rx="4.5" fill="url(#read)" opacity="0.86"/>
    <rect x="520" y="328" width="850" height="9" rx="4.5" fill="url(#read)" opacity="0.58"/>
    <rect x="680" y="380" width="680" height="9" rx="4.5" fill="url(#read)" opacity="0.78"/>
    <rect x="815" y="432" width="455" height="9" rx="4.5" fill="url(#read)" opacity="0.95"/>
  </g>
  <g transform="translate(1030 520)">
    <rect x="0" y="0" width="88" height="210" rx="12" fill="#18333b"/>
    <rect x="112" y="-82" width="88" height="292" rx="12" fill="#214b53"/>
    <rect x="224" y="-176" width="88" height="386" rx="12" fill="#2fcfc2"/>
    <rect x="336" y="-24" width="88" height="234" rx="12" fill="url(#amber)"/>
    <line x1="-30" y1="210" x2="470" y2="210" stroke="#c6f7f3" opacity="0.45"/>
  </g>
  <g opacity="0.58">
    <rect x="1010" y="122" width="258" height="58" rx="12" fill="none" stroke="#90fff7" stroke-opacity="0.42"/>
    <rect x="1086" y="204" width="330" height="58" rx="12" fill="none" stroke="#90fff7" stroke-opacity="0.28"/>
    <rect x="970" y="286" width="242" height="58" rx="12" fill="none" stroke="#f4b860" stroke-opacity="0.34"/>
  </g>
</svg>
"""


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--suite-output",
        type=pathlib.Path,
        help="Read python3 tools/bench_suite.py output and render assets from it.",
    )
    parser.add_argument("--date")
    args = parser.parse_args()

    ASSET_DIR.mkdir(parents=True, exist_ok=True)
    if args.suite_output:
        data = build_benchmark_data_from_suite_output(
            args.suite_output.read_text(encoding="utf-8"),
            date=args.date,
            source_artifact=str(args.suite_output),
        )
    else:
        data = build_benchmark_data()
    write_json(data)
    (ASSET_DIR / "benchmark-speedups.svg").write_text(
        render_speedup_chart(data), encoding="utf-8"
    )
    (ASSET_DIR / "hero-pipeline.svg").write_text(render_hero(), encoding="utf-8")


if __name__ == "__main__":
    main()
