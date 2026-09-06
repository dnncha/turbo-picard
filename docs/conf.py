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
html_title = "Turbo Picard documentation"
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

copybutton_prompt_text = r"^\$ "
copybutton_prompt_is_regexp = True

nitpicky = True


def setup(app):
    import sys
    sys.path.insert(0, str(ROOT / "tools"))
    from build_marketing_site import deploy_to_sphinx
    app.connect("build-finished", deploy_to_sphinx)
    return {"parallel_read_safe": True, "parallel_write_safe": True}
