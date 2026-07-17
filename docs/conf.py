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
html_title = "turbo-picard"
html_baseurl = "https://turbo-picard.readthedocs.io/en/latest/"
html_logo = None
html_favicon = None

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

copybutton_prompt_text = r"^\$ "
copybutton_prompt_is_regexp = True

nitpicky = True
