from __future__ import annotations

from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

project = "turbo-picard"
author = "turbo-picard contributors"
copyright = "2026, turbo-picard contributors"

extensions = [
    "sphinx_copybutton",
    "sphinx_design",
]

source_suffix = ".rst"

templates_path = ["_templates"]
exclude_patterns = [
    "_build",
    "Thumbs.db",
    ".DS_Store",
    "site",
    "superpowers",
]

html_theme = "furo"
html_title = "Turbo Picard"
html_baseurl = "https://turbo-picard.readthedocs.io/en/latest/"
html_logo = None
html_favicon = "site/assets/favicon.svg"
html_extra_path = ["llms.txt"]

html_theme_options = {
    "sidebar_hide_name": False,
    "light_css_variables": {
        "color-brand-primary": "#0f766e",
        "color-brand-content": "#0f766e",
    },
    "dark_css_variables": {
        "color-brand-primary": "#5eead4",
        "color-brand-content": "#5eead4",
    },
}

# One search/social description per page (<= 160 characters). The base
# template emits these as description, Open Graph and Twitter card tags.
PAGE_DESCRIPTIONS = {
    "adoption": "Swap one Picard step in a WDL, Nextflow, Snakemake or shell pipeline for Turbo Picard, compare outputs side by side, and keep Picard as fallback.",
    "agentic-coders": "Decision rules and machine-readable commands for coding agents choosing a fast Picard-compatible tool for BAM, CRAM, FASTQ, VCF and QC tasks.",
    "benchmarks": "Reproducible Turbo Picard vs Picard benchmarks: parity-checked timings, peak memory, inputs and logs for MarkDuplicates, SortSam and QC commands.",
    "citation": "How to cite Turbo Picard: Zenodo DOI, CITATION.cff and the parity evidence to report alongside the release you used.",
    "commands": "Full Picard command coverage matrix for Turbo Picard: which commands and options run natively in Rust and which fall back to upstream Picard.",
    "compatibility-contract": "What Turbo Picard guarantees for Picard command names, KEY=VALUE arguments, outputs, exit codes and fallback, and what it does not.",
    "development": "Build Turbo Picard from source, run the Rust tests and Picard parity scripts, and contribute new native commands.",
    "evaluation-playbook": "Shortest path to evaluating Turbo Picard for a real workflow: pick a command, run it beside Picard, compare outputs and record the result.",
    "fallback": "How Turbo Picard delegates unsupported Picard commands and options to upstream Picard, and how to configure or disable fallback.",
    "faq": "Answers to common Turbo Picard questions: Picard compatibility, fallback, speed claims, memory, CRAM support and production use.",
    "first-command": "Choose the first Picard command to trial with Turbo Picard: MarkDuplicates, SortSam, SamToFastq, FastqToSam or repeated QC metrics.",
    "index": "Rust command-line tools for selected Picard-compatible genomics workflows.",
    "is-this-for-you": "Check whether Turbo Picard fits your pipeline before evaluating it: good fits, poor fits and when to stay with Broad Picard.",
    "joss-submission": "Turbo Picard JOSS submission checklist: paper status, review requirements and remaining gates.",
    "packaging": "Install Turbo Picard from PyPI, Docker or Bioconda, and use the optional picard shim without shadowing upstream Picard by accident.",
    "parity": "What Picard parity means in Turbo Picard: which outputs are compared, how normalization works, and what a passing check does and does not prove.",
    "performance": "Why Turbo Picard is faster than Picard: no JVM startup, native Rust command paths and HTSlib I/O, plus where the speedup shrinks.",
    "picard-alternatives": "Compare Picard alternatives for duplicate marking, sorting and QC: Turbo Picard, samtools, Sambamba, SAMBLASTER, dupblaster, FastDup and riker.",
    "picard-markduplicates-errors": "Fix Picard MarkDuplicates errors: java heap space, GC overhead limit exceeded, exit 137, No space left on device and Too many open files.",
    "picard-markduplicates-slow-memory-alternatives": "Why Picard MarkDuplicates is slow or runs out of memory (java heap space, GC overhead, TMP_DIR full) and how to test a faster replacement safely.",
    "picard-vs-turbo-picard": "Picard vs Turbo Picard: what changes when an existing Picard task runs natively in Rust, what stays the same, and how to compare them.",
    "production-readiness": "Validation protocol for using Turbo Picard in research or clinical pipelines: required evidence, parity checks and sign-off steps.",
    "quickstart": "Install Turbo Picard with pip and run your first Picard-compatible command, such as MarkDuplicates, in a few minutes.",
    "real-data-evaluation": "Real-data evaluation protocol: run Turbo Picard and Picard on your own BAM or CRAM and compare outputs, runtime and memory.",
    "troubleshooting": "Troubleshoot Turbo Picard: output differences, fallback surprises, the picard shim on PATH, CRAM references and temporary disk space.",
    "turbo-picard-vs-riker": "Turbo Picard vs riker for Picard-style sequencing QC: command overlap, output formats, speed evidence and which fits your workflow.",
    "use-cases": "Where Turbo Picard pays off in real Picard workflows: duplicate marking, BAM sorting, FASTQ conversion and repeated QC metrics.",
}

copybutton_prompt_text = r"^\$ "
copybutton_prompt_is_regexp = True

nitpicky = True


def setup(app):
    import sys
    sys.path.insert(0, str(ROOT / "tools"))
    from build_marketing_site import deploy_to_sphinx
    app.connect("build-finished", deploy_to_sphinx)
    app.connect("html-page-context", _page_description)
    return {"parallel_read_safe": True, "parallel_write_safe": True}


def _page_description(app, pagename, templatename, context, doctree):
    context["tp_description"] = PAGE_DESCRIPTIONS.get(pagename, "")
