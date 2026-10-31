#!/usr/bin/env python3
from __future__ import annotations

import argparse
import importlib
import json
import sys
from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import unquote, urlparse
from xml.parsers import expat

import yaml

MODULE_DESCRIPTION = "Check every marketing page, discovery route and Sphinx delivery contract."

_build_marketing_site = importlib.import_module(
    ".build_marketing_site" if __package__ else "build_marketing_site", __package__ or None
)
BASE = _build_marketing_site.BASE
PAGES = _build_marketing_site.PAGES
ROOT = _build_marketing_site.ROOT
build = _build_marketing_site.build


class Page(HTMLParser):
    def __init__(self, source: str) -> None:
        super().__init__(convert_charrefs=True)
        self.ids: list[str] = []
        self.h1 = 0
        self.title = ""
        self.in_title = False
        self.meta: dict[str | None, str | None] = {}
        self.canonicals: list[str | None] = []
        self.refs: list[str] = []
        self.json: list[object] = []
        self.in_json = False
        self.payload = ""
        self.forms = 0
        self.buttons: list[dict[str, str | None]] = []
        self.commands: dict[str, str | None] = {}
        self.feed(source)

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        a = dict(attrs)
        element_id = a.get("id")
        if element_id:
            self.ids.append(element_id)
        if tag == "h1":
            self.h1 += 1
        if tag == "title":
            self.in_title = True
        if tag == "meta":
            self.meta[a.get("name") or a.get("property")] = a.get("content", "")
        if tag == "link" and a.get("rel") == "canonical":
            self.canonicals.append(a.get("href"))
        href = a.get("href")
        src = a.get("src")
        if tag == "a" and href:
            self.refs.append(href)
        if tag in ("script", "img") and src:
            self.refs.append(src)
        if tag == "link" and a.get("rel") in ("stylesheet", "icon") and href is not None:
            self.refs.append(href)
        if tag == "script" and a.get("type") == "application/ld+json":
            self.in_json = True
            self.payload = ""
        if tag == "form":
            self.forms += 1
        if tag == "button":
            self.buttons.append(a)
        command = a.get("data-command")
        if command:
            self.commands[command] = a["data-status"]

    def handle_endtag(self, tag: str) -> None:
        if tag == "title":
            self.in_title = False
        if tag == "script" and self.in_json:
            self.json.append(json.loads(self.payload))
            self.in_json = False

    def handle_data(self, data: str) -> None:
        if self.in_title:
            self.title += data
        if self.in_json:
            self.payload += data


def reject_doctype(*_declaration: object) -> None:
    raise ValueError("DTD declarations are not allowed in a sitemap")


def sitemap_locations(path: Path) -> set[str]:
    locations: set[str] = set()
    open_elements: list[tuple[str, list[str]]] = []

    def start(name: str, _attributes: dict[str, str]) -> None:
        open_elements.append((name, []))

    def text(data: str) -> None:
        if open_elements:
            open_elements[-1][1].append(data)

    def end(_name: str) -> None:
        name, chunks = open_elements.pop()
        if name.endswith("loc"):
            locations.add("".join(chunks))

    parser = expat.ParserCreate()
    parser.StartDoctypeDeclHandler = reject_doctype
    parser.StartElementHandler = start
    parser.CharacterDataHandler = text
    parser.EndElementHandler = end
    parser.Parse(path.read_bytes(), True)
    return locations


def validate(root: Path = ROOT, built_dir: Path | None = None) -> list[str]:
    site = built_dir or root / "docs/site"
    errors: list[str] = []
    titles: set[str] = set()
    descriptions: set[str | None] = set()
    for route, *_ in PAGES:
        path = site / route / "index.html"
        if not path.is_file():
            errors.append(f"missing page: {route}")
            continue
        source = path.read_text()
        page = Page(source)
        if page.h1 != 1:
            errors.append(f"{route}: expected one h1")
        if not page.title or page.title in titles:
            errors.append(f"{route}: missing/duplicate title")
        titles.add(page.title)
        description = page.meta.get("description", "")
        if not description or description in descriptions:
            errors.append(f"{route}: missing/duplicate description")
        descriptions.add(description)
        if page.canonicals != [BASE + route]:
            errors.append(f"{route}: wrong canonical")
        if page.meta.get("og:url") != BASE + route:
            errors.append(f"{route}: wrong Open Graph URL")
        if page.meta.get("robots") != "index,follow":
            errors.append(f"{route}: indexing policy changed")
        if len(page.ids) != len(set(page.ids)):
            errors.append(f"{route}: duplicate element IDs")
        if not page.json:
            errors.append(f"{route}: missing structured data")
        if page.forms:
            errors.append(f"{route}: local planner must not submit a form")
        if "{{" in source:
            errors.append(f"{route}: unresolved template placeholder")
        if any(a.get("type") != "button" for a in page.buttons):
            errors.append(f"{route}: implicit submit button")
        for ref in page.refs:
            u = urlparse(ref)
            if u.scheme or u.netloc:
                if u.scheme not in ("https", "mailto"):
                    errors.append(f"{route}: unsafe link scheme {ref}")
                if built_dir and ref.startswith(BASE):
                    target = built_dir / unquote(ref[len(BASE) :].split("#")[0])
                else:
                    continue
            else:
                target = (path.parent / unquote(u.path)).resolve() if u.path else path
            if target.is_dir():
                target = target / "index.html"
            if not target.is_file():
                errors.append(f"{route}: missing linked file {ref}")
                continue
            if u.fragment and target.suffix == ".html" and unquote(u.fragment) not in Page(target.read_text()).ids:
                errors.append(f"{route}: missing linked anchor {ref}")
    matrix = yaml.safe_load((root / "docs/command-matrix.yml").read_text())
    catalogue = site / "commands/index.html"
    if catalogue.exists():
        expected = {c["name"]: c["status"] for c in matrix["commands"]}
        if Page(catalogue.read_text()).commands != expected:
            errors.append("catalogue differs from authoritative matrix")
    locations = sitemap_locations(site / "sitemap.xml")
    for route, *_ in PAGES:
        if BASE + route not in locations:
            errors.append(f"sitemap missing {route}")
    for doc in (root / "docs").glob("*.rst"):
        if doc.stem != "index" and BASE + doc.stem + ".html" not in locations:
            errors.append(f"sitemap missing {doc.name}")
    if any(not s.startswith(BASE) for s in locations):
        errors.append("sitemap has foreign canonical host")
    config = yaml.safe_load((root / ".readthedocs.yaml").read_text())
    if "rust" in config["build"]["tools"]:
        errors.append("docs must not request an unnecessary Rust toolchain")
    if not config["sphinx"].get("fail_on_warning"):
        errors.append("strict docs gate disabled")
    if "deploy_to_sphinx" not in (root / "docs/conf.py").read_text():
        errors.append("marketing output is not connected to Sphinx")
    js = (site / "assets/site.js").read_text()
    for needle in ["fetch(", "XMLHttpRequest", "localStorage", "sessionStorage", ".innerHTML", "eval("]:
        if needle in js:
            errors.append(f"planner must not use {needle}")
    home = (site / "index.html").read_text()
    for term in ["not a full Picard suite", "Small-fixture results", "evidence/", "commands/", "evaluate/"]:
        if term not in home:
            errors.append(f"home omits required visible scope/navigation: {term}")
    if build(root, check=True):
        errors.append("generated site is stale")
    return errors


def main() -> int:
    parser = argparse.ArgumentParser(description=MODULE_DESCRIPTION)
    parser.add_argument("--built-dir", type=Path)
    args = parser.parse_args()
    errors = validate(built_dir=args.built_dir)
    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1
    print("Marketing site: all pages, catalogue, sitemap, privacy and delivery checks passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
