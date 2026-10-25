"""Generate the Turbo the Pika kit SVGs from one geometry definition."""
import sys
from pathlib import Path
from fontTools.ttLib import TTFont
from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.transformPen import TransformPen

OUT = Path(sys.argv[1])
FONTS = Path(sys.argv[2])
OUT.mkdir(parents=True, exist_ok=True)

FOREST = "#142b25"
LIME = "#d9ef9f"
PAPER = "#f7f8f2"

# Full mascot, 256 grid. Facing forward, moving right (speed lines trail left).
SPEED = [(10, 102, 36), (22, 130, 24), (10, 158, 36)]
EARS = [(110, 64, 22), (194, 64, 22)]
BODY = "M152 52c52 0 94 34 94 84c0 50-42 74-94 74s-94-24-94-74c0-50 42-84 94-84z"
FEET = [(102, 202), (174, 202)]
STRAP = (58, 118, 46, 14)
LENSES = [(130, 125), (194, 125)]
FRAME_R, RING_R, GLASS_R, PUPIL_R, GAZE = 27, 19, 13, 7, 6
NOSE = "M146 162h12l-6 9z"
MOUTH = "M136 176a16 12 0 0 0 32 0h-6a10 7 0 0 1-20 0z"


def circ(cx, cy, r):
    return f"M{cx - r} {cy}a{r} {r} 0 1 0 {2 * r} 0a{r} {r} 0 1 0 {-2 * r} 0z"


def rect(x, y, w, h):
    return f"M{x} {y}h{w}v{h}h{-w}z"


def pill(x, y, w, h):
    r = h / 2
    return f"M{x + r} {y}h{w - h}a{r} {r} 0 0 1 0 {h}h{-(w - h)}a{r} {r} 0 0 1 0 {-h}z"


def svg(body, title, vb="0 0 256 256"):
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="{vb}" role="img">'
            f"<title>{title}</title>{body}</svg>\n")


def mascot_layers(body_c, frame_c, ring_c, glass_c, pupil_c, face_c, speed_c, speed=True):
    """Full mascot as stacked colour layers (for colour versions)."""
    s = ""
    if speed:
        s += f'<path fill="{speed_c}" d="{"".join(pill(x, y, w, 12) for x, y, w in SPEED)}"/>'
    s += f'<path fill="{body_c}" d="{"".join(circ(*e) for e in EARS)}"/>'
    s += f'<path fill="{body_c}" d="{BODY}{"".join(pill(x, y, 28, 14) for x, y in FEET)}"/>'
    s += f'<path fill="{frame_c}" d="{rect(*STRAP)}{"".join(circ(x, y, FRAME_R) for x, y in LENSES)}{NOSE}{MOUTH}"/>'
    s += f'<path fill="{ring_c}" d="{"".join(circ(x, y, RING_R) for x, y in LENSES)}"/>'
    s += f'<path fill="{glass_c}" d="{"".join(circ(x, y, GLASS_R) for x, y in LENSES)}"/>'
    s += f'<path fill="{pupil_c}" d="{"".join(circ(x + GAZE, y, PUPIL_R) for x, y in LENSES)}"/>'
    return s


def mascot_mono(color="#000"):
    """One-colour mascot with real holes: frame, glass and face are cut out."""
    outer = (f'{"".join(circ(*e) for e in EARS)}'
             f'{"".join(pill(x, y, 28, 14) for x, y in FEET)}'
             f'{"".join(pill(x, y, w, 12) for x, y, w in SPEED)}')
    holes = rect(*STRAP) + "".join(circ(x, y, FRAME_R) for x, y in LENSES) + NOSE + MOUTH
    rings = "".join(circ(x, y, RING_R) + circ(x, y, GLASS_R) for x, y in LENSES)
    pupils = "".join(circ(x + GAZE, y, PUPIL_R) for x, y in LENSES)
    return (f'<g fill="{color}"><path d="{outer}"/>'
            f'<path fill-rule="evenodd" d="{BODY}{holes}"/>'
            f'<path fill-rule="evenodd" d="{rings}"/><path d="{pupils}"/></g>')


# Small-size head cut: no strap, feet, mouth or speed lines; bigger lenses and pupils.
H_EARS = [(72, 64, 30), (184, 64, 30)]
H_BODY = "M128 50c64 0 116 40 116 100c0 58-52 92-116 92S12 208 12 150C12 90 64 50 128 50z"
H_LENSES = [(88, 142), (168, 142)]
H_FRAME, H_PUPIL, H_GAZE = 34, 15, 9


def head_layers(body_c, frame_c, pupil_c):
    return (f'<path fill="{body_c}" d="{"".join(circ(*e) for e in H_EARS)}"/><path fill="{body_c}" d="{H_BODY}"/>'
            f'<path fill="{frame_c}" d="{"".join(circ(x, y, H_FRAME) for x, y in H_LENSES)}"/>'
            f'<path fill="{pupil_c}" d="{"".join(circ(x + H_GAZE, y, H_PUPIL) for x, y in H_LENSES)}"/>')


def head_mono(color="#000"):
    holes = "".join(circ(x, y, H_FRAME) for x, y in H_LENSES)
    pupils = "".join(circ(x + H_GAZE, y, H_PUPIL) for x, y in H_LENSES)
    return (f'<g fill="{color}"><path d="{"".join(circ(*e) for e in H_EARS)}"/>'
            f'<path fill-rule="evenodd" d="{H_BODY}{holes}"/><path d="{pupils}"/></g>')


# Wordmark: Inter Display Medium "Turbo" + Bold "Picard", outlined; the i-dot becomes a round pika ear.
def glyph_run(font_file, text, x0, baseline, size, tracking):
    font = TTFont(font_file)
    gs, cmap, hmtx = font.getGlyphSet(), font.getBestCmap(), font["hmtx"]
    scale = size / font["head"].unitsPerEm
    x, paths, idot = x0, [], None
    for ch in text:
        name = cmap[ord(ch)]
        if ch == "i":
            name = cmap[ord("ı")]  # dotless i; we draw our own dot
            idot = x
        pen = SVGPathPen(gs)
        gs[name].draw(TransformPen(pen, (scale, 0, 0, -scale, x, baseline)))
        paths.append(pen.getCommands())
        x += hmtx[name][0] * scale + tracking
    return "".join(paths), x - tracking, idot, scale, font


def wordmark(text_c, dot_c, x0=0, baseline=0, size=100):
    track = -0.02 * size
    turbo, x1, _, _, _ = glyph_run(FONTS / "InterDisplay-Medium.ttf", "Turbo", x0, baseline, size, track)
    picard, x2, idot, scale, font = glyph_run(FONTS / "InterDisplay-Bold.ttf", "Picard", x1 + 0.26 * size, baseline,
                                              size, track)
    gs, cmap, hmtx = font.getGlyphSet(), font.getBestCmap(), font["hmtx"]
    i_adv = hmtx[cmap[ord("ı")]][0] * scale
    dot_r = 0.08 * size
    dot = circ(round(idot + i_adv / 2, 2), round(baseline - 0.74 * size, 2), round(dot_r, 2))
    return (f'<path fill="{text_c}" d="{turbo}{picard}"/><path fill="{dot_c}" d="{dot}"/>', x2,
            baseline - 0.74 * size - dot_r)


def write(name, content):
    (OUT / name).write_text(content)


TITLE = "Turbo Picard mascot, Turbo the Pika"
write("pika-mono.svg", svg(mascot_mono(), TITLE))
write("pika-color.svg", svg(mascot_layers(FOREST, LIME, FOREST, PAPER, FOREST, LIME, FOREST), TITLE))
write("pika-reversed.svg", svg(mascot_layers(LIME, FOREST, LIME, PAPER, FOREST, FOREST, LIME), TITLE))
write("pika-head-mono.svg", svg(head_mono(), "Turbo Picard icon"))
write("pika-head-color.svg", svg(head_layers(FOREST, LIME, FOREST), "Turbo Picard icon"))
write("pika-head-reversed.svg", svg(head_layers(LIME, FOREST, LIME), "Turbo Picard icon"))

# Lockups. Horizontal: head cut + wordmark, height 256. Stacked: full mascot over wordmark.
for variant, (sym, text_c, dot_c) in {
    "color": (head_layers(FOREST, LIME, FOREST), FOREST, "#216546"),
    "reversed": (head_layers(LIME, FOREST, LIME), PAPER, LIME),
    "mono": (head_mono(), "#000", "#000"),
}.items():
    size = 104
    wm, w_end, _ = wordmark(text_c, dot_c, x0=300, baseline=128 + 0.36 * size, size=size)
    width = round(w_end + 8)
    write(f"lockup-horizontal-{variant}.svg", svg(f"{sym}{wm}", "Turbo Picard", f"0 0 {width} 256"))

for variant, (sym, text_c, dot_c) in {
    "color": (mascot_layers(FOREST, LIME, FOREST, PAPER, FOREST, LIME, FOREST), FOREST, "#216546"),
    "reversed": (mascot_layers(LIME, FOREST, LIME, PAPER, FOREST, FOREST, LIME), PAPER, LIME),
    "mono": (mascot_mono(), "#000", "#000"),
}.items():
    size = 72
    probe, w_end, _ = wordmark(text_c, dot_c, 0, 0, size)
    x0 = (512 - w_end) / 2
    wm, _, _ = wordmark(text_c, dot_c, x0=x0, baseline=330, size=size)
    write(f"lockup-stacked-{variant}.svg",
          svg(f'<g transform="translate(128 20)">{sym}</g>{wm}', "Turbo Picard", "0 0 512 360"))

for variant, (text_c, dot_c) in {"color": (FOREST, "#216546"), "reversed": (PAPER, LIME), "mono": ("#000", "#000")}.items():
    size = 100
    wm, w_end, _ = wordmark(text_c, dot_c, x0=0, baseline=100, size=size)
    write(f"wordmark-{variant}.svg", svg(wm, "Turbo Picard", f"0 0 {round(w_end + 4)} 124"))
print("ok", sorted(p.name for p in OUT.glob("*.svg")))
