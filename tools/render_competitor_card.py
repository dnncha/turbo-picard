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
SUITE_DATA = ASSET_DIR / "benchmark-data.json"
PROGRESS = ROOT / "benchmarks" / "progress.json"
SVG_PATH = ASSET_DIR / "competitor-card.svg"
PNG_PATH = ASSET_DIR / "competitor-card.png"
MOBILE_SVG_PATH = ASSET_DIR / "competitor-card-mobile.svg"
MOBILE_PNG_PATH = ASSET_DIR / "competitor-card-mobile.png"
PNG_HASH_KEY = b"turbo-picard-svg-sha256"

WIDTH = 1280
HEIGHT = 640
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


def render_bars(
    results: dict[str, dict[str, Any]],
    names: dict[str, str],
    x: int,
    y: int,
    *,
    label_w: int,
    plot_w: int,
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
    row_h = 58 if stacked else 46
    parts: list[str] = []
    cursor = y

    def group(title: str, tools: list[str], highlight: bool) -> None:
        nonlocal cursor
        if not tools:
            return
        tone = "groupOn" if highlight else "groupOff"
        parts.append(f'<text x="{x}" y="{cursor}" class="{tone}">{html.escape(title)}</text>')
        cursor += 14
        for tool in tools:
            seconds = results[tool]["median_wall_seconds"]
            width = max(4.0, seconds / slowest * plot_w)
            if tool == TURBO:
                fill, label_cls, value_cls = "url(#turbo)", "toolOn", "valueOn"
            elif highlight:
                fill, label_cls, value_cls = "#3a5a63", "tool", "value"
            else:
                fill, label_cls, value_cls = "#2a3a40", "toolOff", "valueOff"
            name = names.get(tool, tool)
            if stacked:
                label_y, bar_x, bar_y = cursor + 20, x, cursor + 28
            else:
                label_y, bar_x, bar_y = cursor + 26, x + label_w, cursor + 6
            parts.append(
                f'<text x="{x}" y="{label_y}" class="{label_cls}">{html.escape(name)}</text>'
                f'<rect x="{bar_x}" y="{bar_y}" width="{plot_w}" height="{22 if stacked else 28}" rx="7" class="track"/>'
                f'<rect x="{bar_x}" y="{bar_y}" width="{width:.1f}" height="{22 if stacked else 28}" rx="7" fill="{fill}"'
                + (' filter="url(#glow)"' if tool == TURBO else "")
                + "/>"
                f'<text x="{bar_x + width + 10:.1f}" y="{bar_y + (16 if stacked else 20)}" class="{value_cls}">{seconds:.2f} s</text>'
            )
            cursor += row_h
        cursor += 20

    group("PICARD-IDENTICAL OUTPUT", exact, True)
    group("OUTPUT DIFFERS FROM PICARD", differs, False)
    return "".join(parts), cursor


def render_tile(x: int, y: int, tile: tuple[str, str, str, bool], *, width: int, wide: bool = False) -> str:
    big, line1, line2, accent = tile
    cls = "bigOn" if accent else "big"
    if wide:
        return (
            f'<rect x="{x}" y="{y}" width="{width}" height="68" rx="14" class="tile"/>'
            f'<text x="{x + 18}" y="{y + 46}" class="{cls}">{html.escape(big)}</text>'
            f'<text x="{x + 138}" y="{y + 30}" class="tileText">{html.escape(line1)}</text>'
            f'<text x="{x + 138}" y="{y + 50}" class="tileFine">{html.escape(line2)}</text>'
        )
    return (
        f'<rect x="{x}" y="{y}" width="{width}" height="112" rx="14" class="tile"/>'
        f'<text x="{x + 18}" y="{y + 50}" class="{cls}">{html.escape(big)}</text>'
        f'<text x="{x + 18}" y="{y + 78}" class="tileText">{html.escape(line1)}</text>'
        f'<text x="{x + 18}" y="{y + 97}" class="tileFine">{html.escape(line2)}</text>'
    )


def render_progress(progress: dict[str, Any], x: int, y: int, *, width: int, spark_w: int) -> tuple[str, int]:
    rows = [(s, c) for s in progress["series"] if (c := series_change(s))]
    if not rows:
        return "", y
    height = 44 + 46 * len(rows)
    label_x = x + 18 + spark_w + 16
    parts = [
        f'<rect x="{x}" y="{y}" width="{width}" height="{height}" rx="14" class="tile"/>',
        f'<text x="{x + 18}" y="{y + 28}" class="groupOn">GETTING FASTER ON MAIN</text>',
    ]
    for index, (series, (values, pct)) in enumerate(rows):
        row_y = y + 58 + index * 46
        lo, hi = min(values), max(values)
        span = hi - lo or 1.0
        step = spark_w / (len(values) - 1)
        points = " ".join(
            f"{x + 18 + i * step:.1f},{row_y - 2 - (hi - v) / span * 18:.1f}" for i, v in enumerate(values)
        )
        label = series.get("short_label", series["label"])
        span_text = f"{series['baseline']} \u2192 {series['steps'][-1]['version']}"
        end_x, end_y = points.split()[-1].split(",")
        parts.append(
            f'<polyline points="{points}" fill="none" stroke="#36d7c9" stroke-width="2.5" '
            'stroke-linecap="round" stroke-linejoin="round"/>'
            f'<circle cx="{end_x}" cy="{end_y}" r="3.5" fill="#9ef8f1"/>'
            f'<text x="{label_x}" y="{row_y}" class="tileText">{html.escape(label)}</text>'
            f'<text x="{label_x}" y="{row_y + 17}" class="tileFine">{html.escape(span_text)}</text>'
            f'<text x="{x + width - 18}" y="{row_y}" text-anchor="end" class="valueOn">\u2212{pct:.0f}%</text>'
        )
    return "".join(parts), y + height


def install_pill(x: int, y: int, width: int) -> str:
    return (
        f'<rect x="{x}" y="{y}" width="{width}" height="48" rx="12" class="tile"/>'
        f'<text x="{x + 20}" y="{y + 31}" class="cmd"><tspan class="prompt">$</tspan>'
        '<tspan dx="12">pip install turbo-picard</tspan></text>'
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
        f"MarkDuplicates, {CASE_LABELS[case]}, 1 CPU, median of {landscape['repeats']}. "
        f"Turbo Picard {version} ({source_label(landscape['candidate_source'])}), measured {measured}. "
        f"Command suite {suite['date']}. Evidence: github.com/dnncha/turbo-picard/tree/main/benchmarks"
    )
    desc = (
        f"{title} {qualifier} Turbo Picard {turbo['median_wall_seconds']:.2f} s versus Picard "
        f"{picard['median_wall_seconds']:.2f} s; "
        + "; ".join(
            f"{names.get(t, t)} {r['median_wall_seconds']:.2f} s"
            + ("" if r.get("full_parity") else " (output differs from Picard)")
            for t, r in sorted(primary.items(), key=lambda kv: kv[1]["median_wall_seconds"])
            if t not in {TURBO, REFERENCE}
        )
        + "."
    )
    return {
        "title": title,
        "qualifier": qualifier,
        "subtitle": f"{qualifier} Same commands, same arguments, no JVM.",
        "desc": desc,
        "primary": primary,
        "names": names,
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
      <stop offset="0" stop-color="#071013"/>
      <stop offset="0.55" stop-color="#10212a"/>
      <stop offset="1" stop-color="#0b1117"/>
    </linearGradient>
    <linearGradient id="turbo" x1="0" x2="1">
      <stop offset="0" stop-color="#2fcfc2"/>
      <stop offset="1" stop-color="#9ef8f1"/>
    </linearGradient>
    <filter id="glow" x="-10%" y="-60%" width="120%" height="220%">
      <feGaussianBlur stdDeviation="6" result="blur"/>
      <feMerge><feMergeNode in="blur"/><feMergeNode in="SourceGraphic"/></feMerge>
    </filter>
  </defs>
  <style>
    text {{ font-family: {FONT}; }}
    .brand {{ font-weight: 700; font-size: {s["brand"]}px; letter-spacing: 2px; fill: #36d7c9; }}
    .title {{ font-weight: 800; font-size: {s["title"]}px; fill: #f4fbfb; }}
    .sub {{ font-weight: 500; font-size: {s["sub"]}px; fill: #b7c9cf; }}
    .groupOn {{ font-weight: 700; font-size: 12px; letter-spacing: 1.5px; fill: #36d7c9; }}
    .groupOff {{ font-weight: 700; font-size: 12px; letter-spacing: 1.5px; fill: #6d8189; }}
    .toolOn {{ font-weight: 800; font-size: 17px; fill: #f4fbfb; }}
    .tool {{ font-weight: 600; font-size: 16px; fill: #d9e7ea; }}
    .toolOff {{ font-weight: 600; font-size: 16px; fill: #7d9098; }}
    .valueOn {{ font-weight: 800; font-size: 16px; fill: #9ef8f1; }}
    .value {{ font-weight: 700; font-size: 15px; fill: #d9e7ea; }}
    .valueOff {{ font-weight: 600; font-size: 15px; fill: #7d9098; }}
    .track {{ fill: rgba(255,255,255,0.05); }}
    .tile {{ fill: rgba(255,255,255,0.045); stroke: rgba(158,248,241,0.14); }}
    .bigOn {{ font-weight: 800; font-size: {s["big"]}px; fill: #9ef8f1; }}
    .big {{ font-weight: 800; font-size: {s["big"]}px; fill: #f4fbfb; }}
    .tileText {{ font-weight: 600; font-size: 15px; fill: #d9e7ea; }}
    .tileFine {{ font-weight: 500; font-size: 13px; fill: #8ba4ad; }}
    .foot {{ font-weight: 500; font-size: {s["foot"]}px; fill: #6d8189; }}
    .cmd {{ font-family: ui-monospace, "SF Mono", Menlo, Consolas, "DejaVu Sans Mono", monospace; font-weight: 600; font-size: {s["cmd"]}px; fill: #f4fbfb; }}
    .prompt {{ fill: #36d7c9; }}
  </style>
  <rect width="{width}" height="{height}" fill="url(#bg)"/>
{body}
</svg>
"""


def build_card(content: dict[str, Any] | None = None) -> str:
    c = content or card_content()
    bars, _ = render_bars(c["primary"], c["names"], 56, 182, label_w=168, plot_w=404)
    tiles = "".join(
        render_tile(800 + (i % 2) * 220, 140 + (i // 2) * 128, tile, width=204) for i, tile in enumerate(c["tiles"])
    )
    progress, _ = render_progress(c["progress"], 800, 400, width=424, spark_w=88)
    body = (
        '  <text x="56" y="52" class="brand">TURBO PICARD</text>\n'
        f'  <text x="56" y="98" class="title">{html.escape(c["title"])}</text>\n'
        f'  <text x="56" y="128" class="sub">{html.escape(c["subtitle"])}</text>\n'
        f"  {bars}\n  {tiles}\n  {progress}\n  {install_pill(56, 490, 372)}\n"
        f'  <text x="56" y="{HEIGHT - 26}" class="foot">{html.escape(c["footer"])}</text>'
    )
    return svg_document(
        c, WIDTH, HEIGHT, body, {"brand": 16, "title": 40, "sub": 19, "big": 38, "foot": 12.5, "cmd": 19}
    )


def build_mobile_card(content: dict[str, Any] | None = None) -> str:
    c = content or card_content()
    x, inner = 24, MOBILE_WIDTH - 48
    parts = ['<text x="24" y="44" class="brand">TURBO PICARD</text>']
    title = wrap(c["title"], 21)
    parts.append(text_lines(title, x, 86, 36, "title"))
    cursor = 86 + 36 * (len(title) - 1) + 30
    sub = wrap(c["subtitle"], 40)
    parts.append(text_lines(sub, x, cursor, 21, "sub"))
    cursor += 21 * (len(sub) - 1) + 44
    bars, cursor = render_bars(c["primary"], c["names"], x, cursor, label_w=0, plot_w=inner - 64, stacked=True)
    parts.append(bars)
    cursor += 4
    for tile in c["tiles"]:
        parts.append(render_tile(x, cursor, tile, width=inner, wide=True))
        cursor += 80
    progress, cursor = render_progress(c["progress"], x, cursor + 4, width=inner, spark_w=56)
    parts.append(progress)
    cursor += 16
    parts.append(install_pill(x, cursor, inner))
    cursor += 48 + 32
    foot = wrap(c["footer"], 62)
    parts.append(text_lines(foot, x, cursor, 16, "foot"))
    height = cursor + 16 * (len(foot) - 1) + 28
    body = "  " + "\n  ".join(parts)
    return svg_document(
        c, MOBILE_WIDTH, height, body, {"brand": 13, "title": 31, "sub": 15.5, "big": 30, "foot": 10.5, "cmd": 17}
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
        (build_card(content), SVG_PATH, PNG_PATH, 1),
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
    "Render the MarkDuplicates competitor card from checked-in evidence.\n"
    "\n"
    "The card is the first graphic a visitor sees (README, PyPI, site home, link\n"
    "previews). Every number on it comes from three files:\n"
    "\n"
    "- the newest ``benchmarks/markduplicates-competitors/landscape-*.json``\n"
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
