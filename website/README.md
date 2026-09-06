# Turbo Picard website

The marketing site is part of the product, not a disconnected mockup. It is
published through the existing Read the Docs project at
`https://turbo-picard.readthedocs.io/en/latest/`.

## Build and delivery

Author content in `pages/`, shared markup in `layout.html`, styling and behaviour
in `assets/`. `tools/build_marketing_site.py` generates the checked-in `docs/site`
output. It reads the package version, command matrix and historical benchmark
JSON; it makes no network requests. Do not hand-edit generated pages.

```sh
python3 -m pip install -r docs/requirements.txt
python3 tools/build_marketing_site.py
python3 tools/build_marketing_site.py --check
python3 tools/verify_marketing_site.py
python3 -m sphinx -W -b html docs docs/_build/html
python3 tools/verify_marketing_site.py --built-dir docs/_build/html
```

A `build-finished` Sphinx hook overlays the marketing pages after a successful
HTML build. Existing `.html` documentation routes remain available, including
`commands.html`; the interactive catalogue has its own `commands/` route.
The hook runs on clean and incremental builds and refuses stale generated output.
Read the Docs does not need Rust to build these pages. Its accepted tool versions
are a separate list from the package's minimum supported Rust version.

The custom `sitemap.xml` includes every marketing page and current top-level
Sphinx guide, rather than just version roots. It contains no invented last-modified
or publication dates. Read the Docs serves a custom sitemap from its default
version at the domain root. Confirm the actual default-version deployment when
checking the root sitemap; the explicit `/en/latest/sitemap.xml` is also linked
from the generated robots file. Canonical URLs intentionally consolidate the
marketing content on the existing hostname and `/en/latest/` prefix.

## Search and conversion design

| User problem / search intent | Entry page | Useful next action |
| --- | --- | --- |
| Picard MarkDuplicates slow, memory footprint, scheduler OOM | `solutions/markduplicates-memory/` | Diagnose the limit, verify bounded-path eligibility, compare |
| Paired BAM to FASTQ, coordinate-sorted BAM mate buffering | `solutions/bam-to-fastq/` | Explicit bounded staging, filters, pairing validation |
| Repeated sequencing QC, CollectMultipleMetrics | `solutions/sequencing-qc/` | Choose supported programs and preserve filters |
| SortSam slow, temporary space, queryname vs coordinate | `solutions/bam-sorting/` | Match required order and resource/storage constraints |
| Picard alternatives, samtools markdup vs Picard, Riker | `compare/` | Choose by the complete task, not unequal benchmark scopes |
| Does Turbo Picard support this command? | `commands/` | Source-derived native/fallback scope, then inspect the binary |

These are researched intent hypotheses, **not measured keyword volumes or a
claim of rankings**. The connected Search Console account did not contain this
hostname on 6 September 2026. A verified URL-prefix property is needed before
measuring its query impressions, landing pages, CTR and indexing status.

Primary sources reviewed: Picard's command documentation and issue #1773 for
documented heap/process-memory confusion; official samtools markdup, sort and
FASTQ documentation; Riker's stated non-drop-in approach; Google Search Central's
helpful-content and AI-features guidance; Read the Docs configuration and sitemap
references. Each guide links directly to its technical sources. Do not turn
historical support issues into claims about present unresolved bugs or demand.

Avoid publishing multiple near-identical pages for keyword variants. Existing
detailed Sphinx guides remain reference material and link into the relevant entry
pages. Add a page only when it answers a distinct problem with concrete guidance.

## Evidence and copy rules

Lead with workflow fit, not giant startup-dominated fixture ratios. Historical
August 14 data is explicitly dated, with absolute medians, input parameters,
repetitions, raw logs and the entire comparison table. Keep the source evidence
unchanged unless an actual new benchmark is run. The 0.1.13 release checks and
platform notes describe that release only; do not silently relabel them as tests
of a new package version. No fake testimonials, affiliations, review ratings,
production claims or upstream workflow adoption badges.

The homepage keeps the native-scope boundary visible. Exhaustive methodology,
benchmark exclusions, distribution checks and citation notes live on `evidence/`.
Existing claim verifiers continue checking them there. The `llms.txt` file is a
reading convenience, not a Google ranking mechanism or a substitute for indexed
HTML. No rank or rich-result outcome is guaranteed.

## Local-only planner

`assets/site.js` exports a pure `buildTrial` function for tests. It generates a
fixed, pinned runner configuration with absolute paths, fresh output directories,
and an explicit upstream jar. It quotes both the outer shell and the runner's
inner shlex-parsed command prefix. It never executes commands, uploads paths,
sets browser storage or embeds them in query strings. Editing inputs disables
copying a stale script. Do not replace this with an LLM-generated shell command.

`default-trial.sh` is the static no-JavaScript example, checked against the same
planner in `test-planner.cjs`. The evaluator's own native-only policy provides the
execution boundary. Plan generation is not file inspection or full option validation.

## Browser and social-card checks

```sh
python3 -m pip install playwright==1.55.0
python3 -m playwright install --with-deps chromium
python3 tools/check_marketing_browser.py --site docs/_build/html --output /tmp/turbo-site-qa
```

Checks cover all ten pages at four viewport widths, actual HTTP navigation,
script generation, CRAM reference errors, clipboard success/failure, stale plans,
no submission requests, no-JavaScript content and reduced motion. Screenshots and
results are CI artifacts. `--offline-render` supports local visual inspection in
restricted environments but is not a substitute for the normal HTTP test.

The social card has an authored HTML source. After changing that design, run
`python3 tools/render_site_social.py`, regenerate the site and review its PNG.
No third-party web fonts or tracking scripts are required by the site. The Read
the Docs hosting service may inject its own addons; the local planner's privacy
claim applies to the planner, not to all hosting infrastructure.
