from __future__ import annotations

import base64
import importlib.util
import tempfile
import unittest
from pathlib import Path

import pytest

SPEC = importlib.util.spec_from_file_location(
    "check_marketing_browser", Path(__file__).with_name("check_marketing_browser.py")
)
assert SPEC is not None
assert SPEC.loader is not None
browser = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(browser)


class OfflineCssTests(unittest.TestCase):
    def test_css_asset_is_embedded_with_exact_svg_bytes(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            assets = Path(directory)
            image = b'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1 1"/>'
            (assets / "pika-mark.svg").write_bytes(image)
            (assets / "site.css").write_text(".brand-mark{background:url(pika-mark.svg)}")
            assert (
                browser.offline_css(assets)
                == '.brand-mark{background:url("data:image/svg+xml;base64,' + base64.b64encode(image).decode() + '")}'
            )

    def test_missing_local_asset_fails_the_offline_render(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            assets = Path(directory)
            (assets / "site.css").write_text(".brand-mark{background:url(missing.svg)}")
            with pytest.raises(FileNotFoundError):
                browser.offline_css(assets)


class OfflineHtmlTests(unittest.TestCase):
    def setUp(self) -> None:
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.site = Path(self.directory.name) / "site"
        assets = self.site / "assets"
        assets.mkdir(parents=True)
        (assets / "site.css").write_text("body{color:black}")
        self.images = {
            "desktop.svg": b"<svg>desktop</svg>",
            "mobile.svg": b"<svg>mobile</svg>",
            "large.svg": b"<svg>large</svg>",
        }
        for name, payload in self.images.items():
            (assets / name).write_bytes(payload)

    def uri(self, name: str) -> str:
        return "data:image/svg+xml;base64," + base64.b64encode(self.images[name]).decode()

    def render(self, route: str, body: str) -> str:
        document = self.site / route / "index.html"
        document.parent.mkdir(parents=True, exist_ok=True)
        document.write_text('<link rel="stylesheet" href="./assets/site.css">' + body)
        rendered: str = browser.offline_html(self.site, route)
        return rendered

    def test_home_picture_embeds_mobile_source_and_desktop_image_exactly(self) -> None:
        rendered = self.render(
            "",
            '<picture><source media="(max-width: 700px)" '
            'srcset="./assets/mobile.svg"><img src="./assets/desktop.svg" '
            'alt="Timing chart"></picture>',
        )
        assert (
            rendered == "<style>body{color:black}</style><picture>"
            '<source media="(max-width: 700px)" srcset="'
            + self.uri("mobile.svg")
            + '"><img src="'
            + self.uri("desktop.svg")
            + '" alt="Timing chart"></picture>'
        )

    def test_nested_route_resolves_source_and_image_srcsets_preserving_descriptors(self) -> None:
        rendered = self.render(
            "solutions/example/",
            '<source srcset="../../assets/mobile.svg 390w, ../../assets/large.svg 1280w">'
            "<img src='/assets/desktop.svg' srcset='../../assets/desktop.svg 1x, ../../assets/large.svg 2x'>",
        )
        assert 'srcset="' + self.uri("mobile.svg") + " 390w, " + self.uri("large.svg") + ' 1280w"' in rendered
        assert "src='" + self.uri("desktop.svg") + "'" in rendered
        assert "srcset='" + self.uri("desktop.svg") + " 1x, " + self.uri("large.svg") + " 2x'" in rendered

    def test_missing_source_or_image_fails(self) -> None:
        for body in ('<img src="./assets/missing.svg">', '<source srcset="./assets/missing.svg 2x">'):
            with self.subTest(body=body), pytest.raises(FileNotFoundError):
                self.render("", body)

    def test_path_escape_is_rejected_before_reading(self) -> None:
        for url in ("../outside.svg", "%2e%2e/outside.svg"):
            with self.subTest(url=url), pytest.raises(ValueError, match="subpath"):
                self.render("", '<img src="' + url + '">')

    def test_symlink_escape_is_rejected(self) -> None:
        outside = Path(self.directory.name) / "outside.svg"
        outside.write_bytes(b"outside")
        (self.site / "assets/escape.svg").symlink_to(outside)
        with pytest.raises(ValueError, match="subpath"):
            self.render("", '<source srcset="./assets/escape.svg">')

    def test_external_images_are_rejected_without_fetching(self) -> None:
        for url in ("https://example.invalid/image.svg", "//example.invalid/image.svg"):
            with self.subTest(url=url), pytest.raises(ValueError, match="must be local"):
                self.render("", '<img src="' + url + '">')
