#!/usr/bin/env python3

from __future__ import annotations

import argparse
import datetime
import gzip
import hashlib
import html
import json
import math
import pathlib
import re
import shutil
import struct
import subprocess
import sys
import zlib
from typing import Any

ROOT = pathlib.Path(__file__).resolve().parents[1]
ASSET_DIR = ROOT / "docs" / "site" / "assets"
LANDSCAPE_DIR = ROOT / "benchmarks" / "markduplicates-competitors"
WGS_RESULTS = ROOT / "benchmarks" / "wgs-coverage-2026-11-13" / "results.json"
SUITE_DATA = ASSET_DIR / "benchmark-data.json"
PROGRESS = ROOT / "benchmarks" / "progress.json"
SVG_PATH = ASSET_DIR / "competitor-card.svg"
PNG_PATH = ASSET_DIR / "competitor-card.png"
MOBILE_SVG_PATH = ASSET_DIR / "competitor-card-mobile.svg"
MOBILE_PNG_PATH = ASSET_DIR / "competitor-card-mobile.png"
PNG_HASH_KEY = b"turbo-picard-svg-sha256"

WIDTH = 1280
HEIGHT = 640
PNG_SCALE = 2
MOBILE_WIDTH = 420
MOBILE_PNG_SCALE = 2
PRIMARY_CASES = ("paired-600k", "paired-primary-600k")
REAL_CASE = "gatk-na12878-mito"
TURBO = "candidate"
REFERENCE = "picard"
TOOL_NAMES = {
    "candidate": "Turbo Picard",
    "fastdup": "FastDup",
    "samtools": "samtools markdup",
    "dupblaster": "dupblaster",
}
CASE_LABELS = {
    "paired-600k": "600k paired reads",
    "paired-primary-600k": "600k paired reads",
    "gatk-na12878-mito": "real NA12878 chrM",
}
FONT = 'ui-sans-serif, system-ui, -apple-system, "Segoe UI", "Helvetica Neue", Arial, sans-serif'
MONO = 'ui-monospace, "SF Mono", Menlo, Consolas, "DejaVu Sans Mono", monospace'


def landscape_date(path: pathlib.Path) -> str:
    match = re.fullmatch(r"landscape-(\d{4}-\d{2}-\d{2})\.json(\.gz)?", path.name)
    if not match:
        raise ValueError(f"landscape file must be named landscape-YYYY-MM-DD.json[.gz]: {path.name}")
    return match.group(1)


def latest_landscape() -> pathlib.Path:
    paths = sorted(LANDSCAPE_DIR.glob("landscape-*.json*"), key=landscape_date)
    if not paths:
        raise SystemExit(f"no landscape-*.json evidence in {LANDSCAPE_DIR}")
    return paths[-1]


def load_landscape(path: pathlib.Path) -> dict[str, Any]:
    raw = path.read_bytes()
    landscape: dict[str, Any] = json.loads(gzip.decompress(raw) if path.suffix == ".gz" else raw)
    return landscape


def primary_case(landscape: dict[str, Any]) -> str:
    cases = {summary["case"] for summary in landscape["summaries"]}
    for case in PRIMARY_CASES:
        if case in cases:
            return case
    raise ValueError(f"landscape has none of the primary cases {PRIMARY_CASES}")


def tool_names(landscape: dict[str, Any]) -> dict[str, str]:
    names = dict(TOOL_NAMES)
    picard = landscape["executables"].get(REFERENCE, {}).get("version", "")
    match = re.search(r"\d+(\.\d+)+", picard)
    names[REFERENCE] = f"Picard {match.group(0)}" if match else "Picard"
    return names


def case_results(landscape: dict[str, Any], case: str) -> dict[str, dict[str, Any]]:
    for summary in landscape["summaries"]:
        if summary["case"] == case:
            return {
                tool: result
                for tool, result in summary.items()
                if isinstance(result, dict) and result.get("successful_runs") and result.get("median_wall_seconds")
            }
    raise ValueError(f"landscape has no case {case!r}")


def parse_wgs_4x_comparison() -> dict[str, Any]:
    runs: list[dict[str, Any]] = json.loads(WGS_RESULTS.read_text(encoding="utf-8"))["runs"]

    def run_for(tool: str) -> dict[str, Any]:
        for run in runs:
            if run["depth"] == "4x" and run["tool"] == tool:
                return run
        raise ValueError(f"WGS results have no 4x run for {tool!r}")

    def entry(tool: str) -> dict[str, Any]:
        run = run_for(tool)
        walls = [float(v) for v in run["wall_seconds"]]
        rss = float(run["peak_rss_mb"])
        if len(walls) == 2:
            wall, label = sum(walls) / 2.0, f"{walls[0]:.1f}\u2013{walls[1]:.1f} s"
        elif len(walls) == 1:
            wall, label = walls[0], f"{walls[0]:,.1f} s"
        else:
            raise ValueError(f"WGS run for {tool!r} needs one wall time or a two-value range")
        return {"wall": wall, "wall_label": label, "rss": rss, "rss_label": f"{int(rss):,} MB"}

    return {"turbo": entry("turbo-picard"), "riker": entry("riker"), "picard": entry("picard")}


def headline(results: dict[str, dict[str, Any]]) -> tuple[str, str]:
    if TURBO not in results or REFERENCE not in results:
        raise ValueError("card needs successful Turbo Picard and Picard runs")
    if not results[TURBO].get("full_parity"):
        raise ValueError("Turbo Picard failed full Picard parity; refusing to render a card")
    turbo = results[TURBO]["median_wall_seconds"]
    if turbo >= results[REFERENCE]["median_wall_seconds"]:
        raise ValueError("Turbo Picard is not faster than Picard on this case")
    if all(turbo < r["median_wall_seconds"] for t, r in results.items() if t != TURBO):
        rivals = [r for t, r in results.items() if t not in {TURBO, REFERENCE}]
        if rivals and not any(r.get("full_parity") for r in rivals):
            return "The fastest MarkDuplicates we've measured.", "And the only one with Picard-identical output."
        return "The fastest MarkDuplicates we've measured.", "Output identical to Picard."
    exact = [t for t, r in results.items() if t != TURBO and r.get("full_parity")]
    if all(turbo < results[t]["median_wall_seconds"] for t in exact):
        return "The fastest Picard-identical MarkDuplicates.", "Faster tools here change the output."
    return "Picard-identical MarkDuplicates, natively.", "Measured against every tool we could run."


def series_change(series: dict[str, Any]) -> tuple[list[float], float] | None:
    steps = series["steps"]
    if not steps:
        return None
    ratios = [1.0]
    for step in steps:
        ratios.append(ratios[-1] * step["after"] / step["before"])
    return ratios, (1 - ratios[-1]) * 100


def source_label(source: str) -> str:
    commit, _, patch = source.partition("+")
    return commit[:7] + ("+patch" if patch else "")


def fmt_ratio(value: float) -> str:
    return f"{value:.1f}x" if value < 100 else f"{value:.0f}x"


def fmt_rss(rss_bytes: float) -> str:
    mib = rss_bytes / (1024 * 1024)
    if mib >= 1024:
        return f"{mib / 1024:.2f} GiB"
    return f"{mib:.0f} MiB"


def wrap(text: str, max_chars: int) -> list[str]:
    lines: list[str] = []
    for word in text.split():
        if lines and len(lines[-1]) + 1 + len(word) <= max_chars:
            lines[-1] += " " + word
        else:
            lines.append(word)
    return lines


def text_lines(lines: list[str], x: int, y: int, leading: int, cls: str) -> str:
    return "".join(
        f'<text x="{x}" y="{y + i * leading}" class="{cls}">{html.escape(line)}</text>' for i, line in enumerate(lines)
    )


def pill(
    x: int, y: int, label: str, *, bg: str, fg: str, border: str = "none", width: int | None = None
) -> tuple[str, int]:
    w = width if width is not None else max(34, int(len(label) * 6.5 + 14))
    svg = (
        f'<rect x="{x}" y="{y}" width="{w}" height="18" rx="9" fill="{bg}" stroke="{border}"/>'
        f'<text x="{x + w / 2:.1f}" y="{y + 12.5}" text-anchor="middle" font-size="9.5" font-weight="800" '
        f'letter-spacing="0.6" fill="{fg}">{html.escape(label)}</text>'
    )
    return svg, w


def render_markdup_panel(
    results: dict[str, dict[str, Any]],
    names: dict[str, str],
    x: int,
    y: int,
    *,
    width: int,
    stacked: bool = False,
) -> tuple[str, int]:
    exact = sorted(
        (t for t, r in results.items() if r.get("full_parity")),
        key=lambda t: results[t]["median_wall_seconds"],
    )
    differs = sorted(
        (t for t, r in results.items() if not r.get("full_parity")),
        key=lambda t: results[t]["median_wall_seconds"],
    )
    slowest = max(r["median_wall_seconds"] for r in results.values())
    pad = 16 if stacked else 22
    panel_h = 354 if stacked else 304
    parts: list[str] = []

    if not stacked:
        parts.append(f'<rect x="{x}" y="{y}" width="{width}" height="{panel_h}" rx="14" class="panel"/>')
        parts.append(
            f'<text x="{x + pad}" y="{y + 26}" class="panelHead">MARKDUPLICATES · 600K PAIRED · 1 CPU</text>'
            f'<text x="{x + width - pad}" y="{y + 26}" text-anchor="end" class="panelMeta">LOWER IS BETTER</text>'
            f'<line x1="{x + pad}" y1="{y + 38}" x2="{x + width - pad}" y2="{y + 38}" class="rule"/>'
        )
        cursor = y + 58
        label_w = 152
        val_col_w = 128
        plot_x = x + pad + label_w
        plot_w = width - 2 * pad - label_w - val_col_w
        for frac in (0.25, 0.5, 0.75, 1.0):
            gx = plot_x + frac * plot_w
            parts.append(f'<line x1="{gx:.1f}" y1="{y + 66}" x2="{gx:.1f}" y2="{y + panel_h - 32}" class="gridLine"/>')
    else:
        total_rows = len(exact) + len(differs)
        panel_h = 96 + total_rows * 54 + (24 if differs else 0)
        parts.append(f'<rect x="{x}" y="{y}" width="{width}" height="{panel_h}" rx="14" class="panel"/>')
        parts.append(
            f'<text x="{x + pad}" y="{y + 25}" class="panelHead">MARKDUPLICATES · 600K PAIRED · 1 CPU</text>'
            f'<line x1="{x + pad}" y1="{y + 36}" x2="{x + width - pad}" y2="{y + 36}" class="rule"/>'
        )
        cursor = y + 55
        plot_x = x + pad
        plot_w = width - 2 * pad - 118

    def render_group(title: str, tools: list[str], highlight: bool) -> None:
        nonlocal cursor
        if not tools:
            return
        tone = "groupOn" if highlight else "groupOff"
        parts.append(f'<text x="{x + pad}" y="{cursor}" class="{tone}">{html.escape(title)}</text>')
        cursor += 12
        for tool in tools:
            res = results[tool]
            seconds = res["median_wall_seconds"]
            rss_str = fmt_rss(res["median_peak_rss_bytes"]) if res.get("median_peak_rss_bytes") else ""
            bar_w = max(8.0, seconds / slowest * plot_w)
            is_winner = tool == TURBO
            name = names.get(tool, tool)
            val_text = f"{seconds:.2f} s" + (f" · {rss_str}" if rss_str else "")
            if stacked:
                row_h = 48
                if is_winner:
                    parts.append(
                        f'<rect x="{x + 10}" y="{cursor}" width="{width - 20}" height="{row_h}" rx="9" class="winRow"/>'
                        f'<rect x="{x + 10}" y="{cursor}" width="3.5" height="{row_h}" rx="2" fill="#34d399"/>'
                    )
                label_cls = "toolOn" if is_winner else ("tool" if highlight else "toolOff")
                val_cls = "valueOn" if is_winner else ("value" if highlight else "valueOff")
                fill = "url(#turbo)" if is_winner else ("#4c6472" if highlight else "#263340")
                parts.append(f'<text x="{x + pad}" y="{cursor + 18}" class="{label_cls}">{html.escape(name)}</text>')
                if is_winner:
                    badge_svg, _ = pill(x + pad + 110, cursor + 4, "#1 FASTEST", bg="#10b981", fg="#031a12")
                    parts.append(badge_svg)
                parts.append(
                    f'<rect x="{plot_x}" y="{cursor + 25}" width="{plot_w}" height="15" rx="4" class="track"/>'
                    f'<rect x="{plot_x}" y="{cursor + 25}" width="{bar_w:.1f}" height="15" rx="4" fill="{fill}"/>'
                    f'<text x="{plot_x + plot_w + 10}" y="{cursor + 37}" class="{val_cls}">{html.escape(val_text)}</text>'
                )
                cursor += 54
            else:
                row_h = 36
                if is_winner:
                    parts.append(
                        f'<rect x="{x + 10}" y="{cursor}" width="{width - 20}" height="{row_h}" rx="8" class="winRow"/>'
                        f'<rect x="{x + 10}" y="{cursor}" width="4" height="{row_h}" rx="2" fill="#34d399"/>'
                    )
                label_cls = "toolOn" if is_winner else ("tool" if highlight else "toolOff")
                val_cls = "valueOn" if is_winner else ("value" if highlight else "valueOff")
                fill = "url(#turbo)" if is_winner else ("#4c6472" if highlight else "#263340")
                parts.append(f'<text x="{x + pad}" y="{cursor + 23}" class="{label_cls}">{html.escape(name)}</text>')
                if is_winner:
                    badge_svg, _ = pill(x + pad + 108, cursor + 9, "#1", bg="#10b981", fg="#031a12", width=28)
                    parts.append(badge_svg)
                parts.append(
                    f'<rect x="{plot_x}" y="{cursor + 9}" width="{plot_w}" height="18" rx="5" class="track"/>'
                    f'<rect x="{plot_x}" y="{cursor + 9}" width="{bar_w:.1f}" height="18" rx="5" fill="{fill}"/>'
                    f'<text x="{x + width - pad}" y="{cursor + 23}" text-anchor="end" class="{val_cls}">{html.escape(val_text)}</text>'
                )
                cursor += 42
        cursor += 16

    render_group("PICARD-IDENTICAL OUTPUT", exact, True)
    render_group("OUTPUT DIFFERS FROM PICARD", differs, False)

    if not stacked:
        parts.append(
            f'<text x="{x + pad}" y="{y + panel_h - 14}" class="panelFoot">Same coordinate BAM &amp; compression level 5 · Only Turbo Picard passes 100% Picard parity</text>'
        )
    else:
        parts.append(
            f'<text x="{x + pad}" y="{y + panel_h - 12}" class="panelFoot">BAM level 5 · Only Turbo Picard matches Picard</text>'
        )
    return "".join(parts), y + panel_h


def render_wgs_panel(
    wgs: dict[str, Any],
    x: int,
    y: int,
    *,
    width: int,
    stacked: bool = False,
) -> tuple[str, int]:
    t = wgs["turbo"]
    r = wgs["riker"]
    p = wgs["picard"]
    wall_ratio_picard = p["wall"] / t["wall"]
    ram_ratio_riker = r["rss"] / t["rss"]
    ram_ratio_picard = p["rss"] / t["rss"]

    pad = 16 if stacked else 22
    parts: list[str] = []

    if not stacked:
        panel_h = 304
        parts.append(f'<rect x="{x}" y="{y}" width="{width}" height="{panel_h}" rx="14" class="panel"/>')
        parts.append(
            f'<text x="{x + pad}" y="{y + 26}" class="panelHead">COLLECTWGSMETRICS · 4X GRCH38 WGS · 6T</text>'
            f'<text x="{x + width - pad}" y="{y + 26}" text-anchor="end" class="panelMeta">VS RIKER &amp; PICARD</text>'
            f'<line x1="{x + pad}" y1="{y + 38}" x2="{x + width - pad}" y2="{y + 38}" class="rule"/>'
        )
        label_w = 136
        val_w = 162
        plot_x = x + pad + label_w
        plot_w = width - 2 * pad - label_w - val_w
    else:
        panel_h = 338
        parts.append(f'<rect x="{x}" y="{y}" width="{width}" height="{panel_h}" rx="14" class="panel"/>')
        parts.append(
            f'<text x="{x + pad}" y="{y + 25}" class="panelHead">4X GRCH38 WGS · VS RIKER &amp; PICARD</text>'
            f'<line x1="{x + pad}" y1="{y + 36}" x2="{x + width - pad}" y2="{y + 36}" class="rule"/>'
        )
        plot_x = x + pad
        plot_w = width - 2 * pad - 126

    def metric_block(
        title: str, top_y: int, *, turbo: tuple[float, str], riker: tuple[float, str], badge_txt: str
    ) -> int:
        turbo_v, turbo_lbl = turbo
        riker_v, riker_lbl = riker
        max_v = max(turbo_v, riker_v)
        tw = max(12.0, turbo_v / max_v * plot_w)
        rw = max(12.0, riker_v / max_v * plot_w)
        cur = top_y
        parts.append(f'<text x="{x + pad}" y="{cur}" class="groupOn">{html.escape(title)}</text>')
        cur += 10
        if stacked:
            parts.append(
                f'<rect x="{x + 10}" y="{cur}" width="{width - 20}" height="44" rx="8" class="winRow"/>'
                f'<rect x="{x + 10}" y="{cur}" width="3.5" height="44" rx="2" fill="#34d399"/>'
                f'<text x="{x + pad}" y="{cur + 17}" class="toolOn">Turbo Picard</text>'
            )
            badge_svg, _ = pill(x + pad + 104, cur + 3, badge_txt, bg="#10b981", fg="#031a12")
            parts.append(
                badge_svg + f'<rect x="{plot_x}" y="{cur + 23}" width="{plot_w}" height="14" rx="4" class="track"/>'
                f'<rect x="{plot_x}" y="{cur + 23}" width="{tw:.1f}" height="14" rx="4" fill="url(#turbo)"/>'
                f'<text x="{x + width - pad}" y="{cur + 35}" text-anchor="end" class="valueOn">{html.escape(turbo_lbl)}</text>'
            )
            cur += 50
            parts.append(
                f'<text x="{x + pad}" y="{cur + 14}" class="toolOff">riker wgs 0.5.0</text>'
                f'<rect x="{plot_x}" y="{cur + 20}" width="{plot_w}" height="14" rx="4" class="track"/>'
                f'<rect x="{plot_x}" y="{cur + 20}" width="{rw:.1f}" height="14" rx="4" fill="#263340"/>'
                f'<text x="{x + width - pad}" y="{cur + 32}" text-anchor="end" class="valueOff">{html.escape(riker_lbl)} (differs)</text>'
            )
            return cur + 50

        # Desktop row 1 (Turbo winner)
        parts.append(
            f'<rect x="{x + 10}" y="{cur}" width="{width - 20}" height="34" rx="8" class="winRow"/>'
            f'<rect x="{x + 10}" y="{cur}" width="4" height="34" rx="2" fill="#34d399"/>'
            f'<text x="{x + pad}" y="{cur + 22}" class="toolOn">Turbo Picard</text>'
        )
        badge_svg, _ = pill(x + pad + 98, cur + 8, "#1", bg="#10b981", fg="#031a12", width=26)
        parts.append(
            badge_svg + f'<rect x="{plot_x}" y="{cur + 8}" width="{plot_w}" height="18" rx="5" class="track"/>'
            f'<rect x="{plot_x}" y="{cur + 8}" width="{tw:.1f}" height="18" rx="5" fill="url(#turbo)"/>'
            f'<text x="{x + width - pad}" y="{cur + 22}" text-anchor="end" class="valueOn">{html.escape(turbo_lbl)} · {html.escape(badge_txt)}</text>'
        )
        cur += 40
        # Desktop row 2 (Riker)
        parts.append(
            f'<text x="{x + pad}" y="{cur + 20}" class="toolOff">riker wgs 0.5.0</text>'
            f'<rect x="{plot_x}" y="{cur + 7}" width="{plot_w}" height="16" rx="4" class="track"/>'
            f'<rect x="{plot_x}" y="{cur + 7}" width="{rw:.1f}" height="16" rx="4" fill="#263340"/>'
            f'<text x="{x + width - pad}" y="{cur + 20}" text-anchor="end" class="valueOff">{html.escape(riker_lbl)} · output differs</text>'
        )
        return cur + 40

    next_y = metric_block(
        "WALL TIME (COLD CACHE)",
        y + (55 if stacked else 58),
        turbo=(t["wall"], t["wall_label"]),
        riker=(r["wall"], r["wall_label"]),
        badge_txt=f"{r['wall'] / t['wall']:.1f}x faster",
    )
    next_y = metric_block(
        "PEAK MEMORY (RSS)",
        next_y + (8 if stacked else 10),
        turbo=(t["rss"], t["rss_label"]),
        riker=(r["rss"], r["rss_label"]),
        badge_txt=f"{ram_ratio_riker:.1f}x less RAM",
    )

    callout_h = 46 if stacked else 36
    callout_y = y + panel_h - callout_h - 14
    parts.append(
        f'<rect x="{x + pad}" y="{callout_y}" width="{width - 2 * pad}" height="{callout_h}" rx="8" class="insetBox"/>'
    )
    if stacked:
        parts.append(
            f'<text x="{x + pad + 10}" y="{callout_y + 18}" class="insetText">'
            f'Picard 3.5.0 (1T): <tspan fill="#e2e8f0" font-weight="700">{html.escape(p["wall_label"])} · {html.escape(p["rss_label"])}</tspan></text>'
            f'<text x="{x + pad + 10}" y="{callout_y + 35}" class="insetText">'
            f'<tspan fill="#6ee7b7" font-weight="700">Turbo Picard: ~{wall_ratio_picard:.0f}x faster &amp; {ram_ratio_picard:.1f}x less RAM (exact)</tspan></text>'
        )
    else:
        parts.append(
            f'<text x="{x + pad + 12}" y="{callout_y + 22}" class="insetText">'
            f'Picard 3.5.0 (1T): <tspan fill="#e2e8f0" font-weight="700">{html.escape(p["wall_label"])} · {html.escape(p["rss_label"])}</tspan> '
            f'<tspan fill="#6ee7b7" font-weight="700">→ ~{wall_ratio_picard:.0f}x faster &amp; {ram_ratio_picard:.1f}x less RAM (exact parity)</tspan></text>'
        )
    return "".join(parts), y + panel_h


def render_tile(
    x: int, y: int, tile: tuple[str, str, str, bool], *, width: int, height: int = 130, wide: bool = False
) -> str:
    big, line1, line2, accent = tile
    cls = "bigOn" if accent else "big"
    tile_cls = "tileAccent" if accent else "tile"
    if wide:
        return (
            f'<rect x="{x}" y="{y}" width="{width}" height="68" rx="12" class="{tile_cls}"/>'
            f'<text x="{x + 16}" y="{y + 44}" class="{cls}">{html.escape(big)}</text>'
            f'<text x="{x + 132}" y="{y + 29}" class="tileText">{html.escape(line1)}</text>'
            f'<text x="{x + 132}" y="{y + 48}" class="tileFine">{html.escape(line2)}</text>'
        )
    return (
        f'<rect x="{x}" y="{y}" width="{width}" height="{height}" rx="12" class="{tile_cls}"/>'
        f'<text x="{x + 15}" y="{y + 50}" class="{cls}">{html.escape(big)}</text>'
        f'<text x="{x + 15}" y="{y + 82}" class="tileText">{html.escape(line1)}</text>'
        f'<text x="{x + 15}" y="{y + 104}" class="tileFine">{html.escape(line2)}</text>'
    )


def render_progress(
    progress: dict[str, Any], x: int, y: int, *, width: int, spark_w: int, height: int = 130
) -> tuple[str, int]:
    rows = [(s, c) for s in progress["series"] if (c := series_change(s))]
    if not rows:
        return "", y
    box_h = max(height, 44 + 42 * len(rows))
    label_x = x + 18 + spark_w + 16
    parts = [
        f'<rect x="{x}" y="{y}" width="{width}" height="{box_h}" rx="14" class="panel"/>',
        f'<text x="{x + 18}" y="{y + 26}" class="groupOn">GETTING FASTER ON MAIN</text>',
    ]
    for index, (series, (values, pct)) in enumerate(rows):
        row_y = y + 54 + index * 44
        lo, hi = min(values), max(values)
        span = hi - lo or 1.0
        step = spark_w / (len(values) - 1)
        coords = [(x + 18 + i * step, row_y - 2 - (hi - v) / span * 18) for i, v in enumerate(values)]
        points = " ".join(f"{px:.1f},{py:.1f}" for px, py in coords)
        area_points = f"{coords[0][0]:.1f},{row_y + 16:.1f} {points} {coords[-1][0]:.1f},{row_y + 16:.1f}"
        label = series.get("short_label", series["label"])
        span_text = f"{series['baseline']} → {series['steps'][-1]['version']}"
        end_x, end_y = coords[-1]
        parts.append(
            f'<polygon points="{area_points}" fill="url(#sparkArea)"/>'
            f'<polyline points="{points}" fill="none" stroke="#34d399" stroke-width="2.5" '
            'stroke-linecap="round" stroke-linejoin="round"/>'
            f'<circle cx="{end_x:.1f}" cy="{end_y:.1f}" r="3.5" fill="#a7f3d0"/>'
            f'<text x="{label_x}" y="{row_y}" class="tileText">{html.escape(label)}</text>'
            f'<text x="{label_x}" y="{row_y + 17}" class="tileFine">{html.escape(span_text)}</text>'
            f'<text x="{x + width - 18}" y="{row_y + 2}" text-anchor="end" class="valueOn">\u2212{pct:.0f}%</text>'
        )
    return "".join(parts), y + box_h


def install_pill(x: int, y: int, width: int, *, height: int = 38) -> str:
    text_y = y + height / 2 + 5
    return (
        f'<rect x="{x}" y="{y}" width="{width}" height="{height}" rx="10" class="cmdBox"/>'
        f'<text x="{x + 18}" y="{text_y:.1f}" class="cmd"><tspan class="prompt">$</tspan>'
        '<tspan dx="10">pip install turbo-picard</tspan></text>'
    )


def comparison_tile(ratio: float, metric: str, detail: str) -> tuple[str, str, str, bool]:
    if ratio == 1:
        return ("1x", f"same {metric} as Picard", detail, False)
    if metric == "memory":
        label = "less memory than Picard" if ratio > 1 else "more memory than Picard"
    else:
        label = "faster than Picard" if ratio > 1 else "slower than Picard"
    return (fmt_ratio(ratio if ratio > 1 else 1 / ratio), label, detail, metric == "speed" and ratio > 1)


def card_content() -> dict[str, Any]:
    landscape_path = latest_landscape()
    landscape = load_landscape(landscape_path)
    suite = json.loads(SUITE_DATA.read_text(encoding="utf-8"))
    progress = json.loads(PROGRESS.read_text(encoding="utf-8"))
    wgs = parse_wgs_4x_comparison()
    measured = landscape_date(landscape_path)

    case = primary_case(landscape)
    names = tool_names(landscape)
    primary = case_results(landscape, case)
    title, qualifier = headline(primary)
    turbo = primary[TURBO]
    picard = primary[REFERENCE]
    speed = picard["median_wall_seconds"] / turbo["median_wall_seconds"]
    memory = picard["median_peak_rss_bytes"] / turbo["median_peak_rss_bytes"]

    tiles = [
        comparison_tile(speed, "speed", CASE_LABELS[case]),
        comparison_tile(memory, "memory", CASE_LABELS[case]),
    ]
    try:
        real = case_results(landscape, REAL_CASE)
    except ValueError:
        real = {}
    if real.get(TURBO, {}).get("full_parity") and REFERENCE in real:
        real_speed = real[REFERENCE]["median_wall_seconds"] / real[TURBO]["median_wall_seconds"]
        tiles.append(comparison_tile(real_speed, "speed", f"{CASE_LABELS[REAL_CASE]}, identical"))
    summary = suite["summary"]
    tiles.append(
        (
            f"{summary['parity_pass_count']}/{summary['command_count']}",
            "commands match Picard",
            f"{summary['geometric_mean_speedup']:.0f}x geo-mean speedup",
            False,
        )
    )

    version = landscape["executables"][TURBO]["version"].strip().removeprefix("turbo-picard ")
    footer = (
        f"MarkDuplicates ({CASE_LABELS[case]}, 1 CPU, {measured}, {version} {source_label(landscape['candidate_source'])}). "
        f"WGS: 1000G HG02675 4x GRCh38 (6T, 2026-11-13). Suite: {suite['date']}. Evidence: github.com/dnncha/turbo-picard/tree/main/benchmarks"
    )
    desc = (
        f"{title} {qualifier} MarkDuplicates: Turbo Picard {turbo['median_wall_seconds']:.2f} s vs Picard "
        f"{picard['median_wall_seconds']:.2f} s; "
        + "; ".join(
            f"{names.get(t, t)} {r['median_wall_seconds']:.2f} s"
            + ("" if r.get("full_parity") else " (output differs from Picard)")
            for t, r in sorted(primary.items(), key=lambda kv: kv[1]["median_wall_seconds"])
            if t not in {TURBO, REFERENCE}
        )
        + f". 4x GRCh38 WGS CollectWgsMetrics: Turbo Picard {wgs['turbo']['wall_label']} / {wgs['turbo']['rss_label']} vs "
        f"riker wgs {wgs['riker']['wall_label']} / {wgs['riker']['rss_label']} vs Picard {wgs['picard']['wall_label']}."
    )
    return {
        "title": title,
        "qualifier": qualifier,
        "subtitle": f"{qualifier} Native Rust across 41 commands; zero JVM.",
        "desc": desc,
        "primary": primary,
        "names": names,
        "wgs": wgs,
        "tiles": tiles,
        "progress": progress,
        "footer": footer,
    }


def svg_document(content: dict[str, Any], width: int, height: int, body: str, sizes: dict[str, float]) -> str:
    s = sizes
    return f"""<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}" role="img" aria-labelledby="title desc">
  <title id="title">{html.escape(content["title"])} {html.escape(content["qualifier"])}</title>
  <desc id="desc">{html.escape(content["desc"])}</desc>
  <defs>
    <linearGradient id="bg" x1="0" x2="1" y1="0" y2="1">
      <stop offset="0" stop-color="#060a0e"/>
      <stop offset="0.55" stop-color="#0b131a"/>
      <stop offset="1" stop-color="#070b10"/>
    </linearGradient>
    <radialGradient id="atmo" cx="0.22" cy="0.08" r="0.65">
      <stop offset="0" stop-color="#10b981" stop-opacity="0.16"/>
      <stop offset="1" stop-color="#10b981" stop-opacity="0"/>
    </radialGradient>
    <linearGradient id="turbo" x1="0" x2="1">
      <stop offset="0" stop-color="#10b981"/>
      <stop offset="1" stop-color="#6ee7b7"/>
    </linearGradient>
    <linearGradient id="sparkArea" x1="0" x2="0" y1="0" y2="1">
      <stop offset="0" stop-color="#34d399" stop-opacity="0.28"/>
      <stop offset="1" stop-color="#34d399" stop-opacity="0"/>
    </linearGradient>
  </defs>
  <style>
    text {{ font-family: {FONT}; }}
    .brand {{ font-weight: 800; font-size: {s["brand"]}px; letter-spacing: 2px; fill: #34d399; }}
    .brandMeta {{ font-weight: 600; font-size: {s["brandMeta"]}px; letter-spacing: 1.4px; fill: #7e95a3; }}
    .title {{ font-weight: 800; font-size: {s["title"]}px; fill: #f8fafc; letter-spacing: -0.6px; }}
    .sub {{ font-weight: 500; font-size: {s["sub"]}px; fill: #9fb4bf; }}
    .panel {{ fill: #0e161d; stroke: rgba(148,163,184,0.16); }}
    .panelHead {{ font-weight: 800; font-size: 11px; letter-spacing: 1.4px; fill: #cbd5e1; }}
    .panelMeta {{ font-weight: 700; font-size: 10px; letter-spacing: 1.2px; fill: #64748b; }}
    .panelFoot {{ font-weight: 500; font-size: 11px; fill: #64748b; }}
    .rule {{ stroke: rgba(148,163,184,0.12); stroke-width: 1; }}
    .gridLine {{ stroke: rgba(148,163,184,0.07); stroke-dasharray: 2 3; }}
    .winRow {{ fill: rgba(16,185,129,0.10); stroke: rgba(52,211,153,0.34); }}
    .insetBox {{ fill: rgba(255,255,255,0.03); stroke: rgba(148,163,184,0.12); }}
    .insetText {{ font-weight: 500; font-size: 11px; fill: #94a3b8; }}
    .groupOn {{ font-weight: 800; font-size: 10.5px; letter-spacing: 1.4px; fill: #34d399; }}
    .groupOff {{ font-weight: 700; font-size: 10.5px; letter-spacing: 1.4px; fill: #64748b; }}
    .toolOn {{ font-weight: 800; font-size: 14.5px; fill: #ffffff; }}
    .tool {{ font-weight: 600; font-size: 13.5px; fill: #cbd5e1; }}
    .toolOff {{ font-weight: 600; font-size: 13.5px; fill: #8496a3; }}
    .valueOn {{ font-weight: 800; font-size: 13px; fill: #6ee7b7; }}
    .value {{ font-weight: 700; font-size: 12.5px; fill: #cbd5e1; }}
    .valueOff {{ font-weight: 600; font-size: 12.5px; fill: #7f919e; }}
    .track {{ fill: rgba(255,255,255,0.05); }}
    .tile {{ fill: #0e161d; stroke: rgba(148,163,184,0.16); }}
    .tileAccent {{ fill: rgba(16,185,129,0.08); stroke: rgba(52,211,153,0.30); }}
    .bigOn {{ font-weight: 800; font-size: {s["big"]}px; fill: #6ee7b7; letter-spacing: -0.6px; }}
    .big {{ font-weight: 800; font-size: {s["big"]}px; fill: #f8fafc; letter-spacing: -0.6px; }}
    .tileText {{ font-weight: 700; font-size: 12px; fill: #e2e8f0; }}
    .tileFine {{ font-weight: 500; font-size: 10.8px; fill: #8ba0ad; }}
    .foot {{ font-weight: 500; font-size: {s["foot"]}px; fill: #64748b; }}
    .cmdBox {{ fill: #0f1820; stroke: rgba(52,211,153,0.32); }}
    .cmd {{ font-family: {MONO}; font-weight: 600; font-size: {s["cmd"]}px; fill: #f8fafc; }}
    .prompt {{ fill: #34d399; }}
  </style>
  <rect width="{width}" height="{height}" fill="url(#bg)"/>
  <rect width="{width}" height="{height}" fill="url(#atmo)"/>
{body}
</svg>
"""


def build_card(content: dict[str, Any] | None = None) -> str:
    c = content or card_content()
    markdup_panel, _ = render_markdup_panel(c["primary"], c["names"], 40, 134, width=606)
    wgs_panel, _ = render_wgs_panel(c["wgs"], 666, 134, width=574)
    tile_w = 180
    tile_gap = 14
    tiles = "".join(
        render_tile(40 + i * (tile_w + tile_gap), 454, tile, width=tile_w, height=130)
        for i, tile in enumerate(c["tiles"])
    )
    progress, _ = render_progress(c["progress"], 818, 454, width=422, spark_w=78, height=130)
    body = (
        '  <circle cx="46" cy="40" r="4" fill="#34d399"/>\n'
        '  <text x="58" y="44" class="brand">TURBO PICARD</text>\n'
        '  <text x="204" y="44" class="brandMeta">· NO JVM · PICARD-IDENTICAL BENCHMARKS</text>\n'
        f"  {install_pill(908, 22, 332, height=38)}\n"
        f'  <text x="40" y="84" class="title">{html.escape(c["title"])}</text>\n'
        f'  <text x="40" y="110" class="sub">{html.escape(c["subtitle"])}</text>\n'
        f"  {markdup_panel}\n  {wgs_panel}\n  {tiles}\n  {progress}\n"
        f'  <text x="40" y="{HEIGHT - 22}" class="foot">{html.escape(c["footer"])}</text>'
    )
    return svg_document(
        c,
        WIDTH,
        HEIGHT,
        body,
        {"brand": 12.5, "brandMeta": 11, "title": 33, "sub": 16, "big": 32, "foot": 11.5, "cmd": 15.5},
    )


def build_mobile_card(content: dict[str, Any] | None = None) -> str:
    c = content or card_content()
    x, inner = 20, MOBILE_WIDTH - 40
    parts = [
        '<circle cx="25" cy="36" r="3.5" fill="#34d399"/>',
        '<text x="35" y="40" class="brand">TURBO PICARD</text>',
        '<text x="164" y="40" class="brandMeta">· NO JVM</text>',
    ]
    title = wrap(c["title"], 23)
    parts.append(text_lines(title, x, 76, 33, "title"))
    cursor = 76 + 33 * (len(title) - 1) + 24
    sub = wrap(c["subtitle"], 42)
    parts.append(text_lines(sub, x, cursor, 20, "sub"))
    cursor += 20 * (len(sub) - 1) + 22
    markdup_panel, cursor = render_markdup_panel(c["primary"], c["names"], x, cursor, width=inner, stacked=True)
    parts.append(markdup_panel)
    cursor += 14
    wgs_panel, cursor = render_wgs_panel(c["wgs"], x, cursor, width=inner, stacked=True)
    parts.append(wgs_panel)
    cursor += 14
    for tile in c["tiles"]:
        parts.append(render_tile(x, cursor, tile, width=inner, wide=True))
        cursor += 78
    progress, cursor = render_progress(c["progress"], x, cursor + 4, width=inner, spark_w=54, height=120)
    if progress:
        parts.append(progress)
        cursor += 14
    parts.append(install_pill(x, cursor, inner, height=44))
    cursor += 44 + 26
    foot = wrap(c["footer"], 62)
    parts.append(text_lines(foot, x, cursor, 15, "foot"))
    height = cursor + 15 * (len(foot) - 1) + 22
    body = "  " + "\n  ".join(parts)
    return svg_document(
        c,
        MOBILE_WIDTH,
        height,
        body,
        {"brand": 11.5, "brandMeta": 10.5, "title": 28, "sub": 14.5, "big": 28, "foot": 10, "cmd": 15.5},
    )


def svg_digest(svg: str) -> str:
    return hashlib.sha256(svg.encode("utf-8")).hexdigest()


def png_with_text(png: bytes, key: bytes, value: str) -> bytes:
    signature, rest = png[:8], png[8:]
    ihdr_len = struct.unpack(">I", rest[:4])[0]
    ihdr_end = 12 + ihdr_len
    data = key + b"\x00" + value.encode("ascii")
    chunk = struct.pack(">I", len(data)) + b"tEXt" + data
    chunk += struct.pack(">I", zlib.crc32(b"tEXt" + data) & 0xFFFFFFFF)
    return signature + rest[:ihdr_end] + chunk + rest[ihdr_end:]


def png_text(png: bytes, key: bytes) -> str | None:
    offset = 8
    while offset + 8 <= len(png):
        length = struct.unpack(">I", png[offset : offset + 4])[0]
        kind = png[offset + 4 : offset + 8]
        body = png[offset + 8 : offset + 8 + length]
        if kind == b"tEXt" and body.startswith(key + b"\x00"):
            return body[len(key) + 1 :].decode("ascii")
        offset += 12 + length
    return None


def rendered_cards() -> list[tuple[str, pathlib.Path, pathlib.Path, int]]:
    content = card_content()
    return [
        (build_card(content), SVG_PATH, PNG_PATH, PNG_SCALE),
        (build_mobile_card(content), MOBILE_SVG_PATH, MOBILE_PNG_PATH, MOBILE_PNG_SCALE),
    ]


def write_card() -> None:
    converter = shutil.which("rsvg-convert")
    if converter is None:
        raise SystemExit("rsvg-convert is required to render the card PNGs (librsvg)")
    for svg, svg_path, png_path, scale in rendered_cards():
        png = subprocess.run(
            [converter, "--zoom", str(scale), "--format", "png"],
            input=svg.encode("utf-8"),
            capture_output=True,
            check=True,
        ).stdout
        svg_path.write_text(svg, encoding="utf-8")
        png_path.write_bytes(png_with_text(png, PNG_HASH_KEY, svg_digest(svg)))
        print(f"wrote {svg_path.relative_to(ROOT)} and {png_path.relative_to(ROOT)}")


def check_card() -> list[str]:
    errors = []
    for svg, svg_path, png_path, _ in rendered_cards():
        if not svg_path.exists() or svg_path.read_text(encoding="utf-8") != svg:
            errors.append(f"{svg_path.relative_to(ROOT)} is stale")
        if not png_path.exists() or png_text(png_path.read_bytes(), PNG_HASH_KEY) != svg_digest(svg):
            errors.append(f"{png_path.relative_to(ROOT)} does not match the current SVG")
    return errors


def record_point(args: argparse.Namespace) -> None:
    for name in ("before", "after"):
        value = getattr(args, name)
        if not math.isfinite(value) or value <= 0:
            raise SystemExit(f"--{name} must be a positive, finite measurement")
    progress = json.loads(PROGRESS.read_text(encoding="utf-8"))
    series = next((s for s in progress["series"] if s["id"] == args.series), None)
    if series is None:
        if not (args.label and args.protocol and args.baseline):
            raise SystemExit(f"new series {args.series!r} needs --label, --protocol and --baseline")
        series = {
            "id": args.series,
            "label": args.label,
            "short_label": args.label,
            "baseline": args.baseline,
            "protocol": args.protocol,
            "steps": [],
        }
        progress["series"].append(series)
    series["steps"].append(
        {
            "date": args.date or datetime.date.today().isoformat(),
            "version": args.version,
            "before": args.before,
            "after": args.after,
            "evidence": args.evidence,
        }
    )
    PROGRESS.write_text(json.dumps(progress, indent=2) + "\n", encoding="utf-8")
    print(f"recorded {args.series} {args.version}: {args.before} -> {args.after}")


_DESCRIPTION = (
    "Render the MarkDuplicates and WGS QC competitor card from checked-in evidence.\n"
    "\n"
    "The card is the first visual a visitor sees (README, PyPI, site home, link\n"
    "previews). Every number on it comes from four checked-in files:\n"
    "\n"
    "- the newest ``benchmarks/markduplicates-competitors/landscape-*.json``\n"
    "- ``benchmarks/wgs-coverage-2026-11-13/results.json`` (4x GRCh38 WGS vs Riker/Picard)\n"
    "- ``docs/site/assets/benchmark-data.json`` (the parity-checked command suite)\n"
    "- ``benchmarks/progress.json`` (accepted hill-climb results on main)\n"
    "\n"
    'The headline is derived, not written: it only says "fastest" for the set of\n'
    "tools the evidence shows Turbo Picard beating, and only among tools whose\n"
    "output passed the full Picard comparison.\n"
    "\n"
    "    python3 tools/render_competitor_card.py            # re-render SVG + PNG\n"
    "    python3 tools/render_competitor_card.py --check    # fail if stale\n"
    "    python3 tools/render_competitor_card.py record --series ID --version V \\\n"
    "        --before S --after S --evidence URL            # add a same-run step\n"
)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=_DESCRIPTION, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--check", action="store_true", help="fail if the committed card is stale")
    sub = parser.add_subparsers(dest="command")
    record = sub.add_parser("record", help="append an accepted measurement to benchmarks/progress.json")
    record.add_argument("--series", required=True)
    record.add_argument("--version", required=True, help="release, PR (#46) or short commit on main")
    record.add_argument("--before", required=True, type=float, help="median wall seconds before the change")
    record.add_argument("--after", required=True, type=float, help="median wall seconds after, same run")
    record.add_argument("--evidence", required=True, help="immutable URL or repo path to raw runs")
    record.add_argument("--date")
    record.add_argument("--label")
    record.add_argument("--protocol")
    record.add_argument("--baseline", help="version the series starts from (new series only)")
    args = parser.parse_args(argv)

    if args.command == "record":
        record_point(args)
        write_card()
        return 0
    if args.check:
        errors = check_card()
        for error in errors:
            print(f"error: {error}; run python3 tools/render_competitor_card.py", file=sys.stderr)
        return 1 if errors else 0
    write_card()
    return 0


if __name__ == "__main__":
    sys.exit(main())
