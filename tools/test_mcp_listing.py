#!/usr/bin/env python3
"""Check the MCP Registry listing against the package that serves it."""

from __future__ import annotations

import json
import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SERVER_JSON = ROOT / "packaging" / "mcp" / "server.json"


class McpListingTests(unittest.TestCase):
    def test_listing_points_at_the_pypi_package_and_mcp_subcommand(self) -> None:
        listing = json.loads(SERVER_JSON.read_text(encoding="utf-8"))
        assert listing["name"] == "io.github.dnncha/turbo-picard"
        assert len(listing["description"]) <= 100
        assert listing["repository"]["url"] == "https://github.com/dnncha/turbo-picard"
        (package,) = listing["packages"]
        assert package["registryType"] == "pypi"
        assert package["identifier"] == "turbo-picard"
        assert package["version"] == listing["version"]
        assert package["transport"] == {"type": "stdio"}
        assert package["packageArguments"] == [{"type": "positional", "value": "mcp"}]

    def test_listing_version_is_not_older_than_the_workspace(self) -> None:
        listing = json.loads(SERVER_JSON.read_text(encoding="utf-8"))
        cargo = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
        match = re.search(r'^version = "([^"]+)"', cargo, re.MULTILINE)
        assert match is not None
        workspace = tuple(int(part) for part in match.group(1).split("."))
        listed = tuple(int(part) for part in listing["version"].split("."))
        assert listed >= workspace

    def test_pypi_readme_carries_the_registry_ownership_marker(self) -> None:
        readme = (ROOT / "README.md").read_text(encoding="utf-8")
        assert "mcp-name: io.github.dnncha/turbo-picard" in readme


if __name__ == "__main__":
    unittest.main()
