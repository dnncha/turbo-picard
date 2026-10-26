# Turbo the Pika: mascot and logo guidelines

## 1. The mascot

**Idea:** Picard sounds like pika, so the Turbo Picard mascot is a round pika in
racing goggles, already mid-sprint. The goggles and speed lines say "turbo"
without a rocket, lightning bolt, DNA helix, Star Trek reference or Rust's
Ferris.

| Version | Use it for | Files |
|---|---|---|
| Full pika | README headers, docs landing page, stickers, slides | `export/mascot/pika-color.svg`, `pika-reversed.svg` |
| Head cut | Favicon, app icon, avatars, anything under 64 px | `export/mascot/pika-head-*.svg`, `export/icon/` |
| Horizontal lockup | Site header, README title, social cards | `export/lockups/lockup-horizontal-*.svg` |
| Stacked lockup | Square spaces, posters, sticker sheets | `export/lockups/lockup-stacked-*.svg` |
| Wordmark | Where the mascot already appears nearby | `export/lockups/wordmark-*.svg` |
| One colour | Single-ink print, embroidery, watermarks | `export/mascot/turbo-pika-{black,white,mono-*}.svg` |

Colour artwork has `-color` (for light backgrounds) and `-reversed` (for forest)
versions. Mascot PNGs are 512 and 1024 px wide; lockup PNGs are 512 px high.
Icon variants are 64, 256 and 512 px wide.

## 2. Clear space

Keep a clear zone equal to **one ear width** around the mascot and lockups on
all sides. One ear is 44 units on the 256-unit mascot grid, so the clear space
is about a quarter of the mascot's height. In the wordmark, use the cap height of
"P".

## 3. Minimum size

| Version | Screen | Print |
|---|---|---|
| Full pika (with speed lines) | 64 px tall | 15 mm |
| Head cut | 16 px (use `export/icon/favicon.svg`) | 5 mm |
| Horizontal lockup | 120 px wide | 30 mm |

Below 64 px, switch to the head cut, because the speed lines and mouth stop
reading at that size.

## 4. Colour

These come from the existing `website/assets/site.css` tokens.

| Name | Token | HEX | RGB |
|---|---|---|---|
| Forest (primary) | `--ink` | `#142b25` | 20 43 37 |
| Lime (accent) | `--accent` | `#d9ef9f` | 217 239 159 |
| Green (wordmark i-dot on light) | `--green` | `#216546` | 33 101 70 |
| Paper | `--paper` | `#f7f8f2` | 247 248 242 |

Measured contrast: forest on paper 14.0:1, lime on forest 12.0:1, green on
paper 6.5:1. Lime on paper is only 1.2:1, so lime is never the outline or text
colour on light backgrounds. It only appears inside a forest shape (goggles) or
on forest.

**Approved pairs:** colour version on paper or white · reversed version on
forest · black or forest one-colour on white · white one-colour on photos or
dark surfaces other than forest.

CMYK and Pantone values haven't been matched yet. Get a printer's proof before
any print run.

## 5. Typography

The wordmark uses Inter Display Medium for "Turbo" and Inter Display Bold for
"Picard", with −2% tracking, outlined to paths. Its one custom detail is a
round dot over the "i" that echoes the pika's ears. Inter is under the SIL Open
Font License, which allows logo use. The site's body text stays in Inter. Never
type the wordmark; use the SVG.

## 6. Don'ts

- Don't stretch, squash or rotate the pika.
- Don't recolour it outside the palette, or put lime goggles on a lime body.
- Don't add outlines, shadows, gradients or glows.
- Don't remove the goggles. They are what makes it Turbo and not a hamster.
- Don't use the full pika below 64 px. Use the head cut.
- Don't flip it to face left. It always runs right, with the speed lines
  trailing.
- Don't re-space or resize the parts of a lockup.

## 7. Source and regeneration

All art is generated from one geometry file, `tools/build_pika.py`. Edit the
coordinates there and rerun it instead of editing exported SVGs by hand.

Run from the repository root with Inter 4.1's `extras/ttf` directory. Pin
fonttools 4.55.3 in the environment and retain `rsvg-convert --version` with the
regeneration evidence. Rendering uses librsvg, so use the same recorded version
when checking PNG drift.

```sh
python3 -m venv .venv-pika
.venv-pika/bin/pip install fonttools==4.55.3
rsvg-convert --version
/home/donncha/.codex/bin/resource-run .venv-pika/bin/python branding/mascot/kit/tools/build_pika.py branding/mascot/kit/src /path/to/Inter-4.1/extras/ttf --export
git diff -- branding/mascot/kit/src branding/mascot/kit/export website/assets docs/site/assets
```

`--export` is the authoritative full pipeline. It regenerates source SVGs,
all colour and mono exports, square variants, lockups and their PNGs, the
16/32/48 px favicons and ICO, 192/512 px web icons, the 512 px maskable icon,
the 180 px Apple touch icon, and both deployed asset directories. SVG widths control PNG widths while
preserving aspect ratio. Review and commit every changed artifact together.
Run the same command again with unchanged inputs; `git diff` must show no new
changes. Without `--export`, the command only writes source SVGs and does not
refresh the deployed kit.


The site uses `export/icon/favicon.svg` as both `website/assets/favicon.svg` and
`website/assets/pika-mark.svg` (the header and footer mark), plus
`export/icon/apple-touch-icon.png`.

**Not done:** trademark search, print colour matching, and replacing the `tp`
mark on the social card and README hero image.
