#!/usr/bin/env python3
"""Keep the published agent skill, rules and plugin metadata consistent."""

from __future__ import annotations

import json
import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
KIT = ROOT / "packaging" / "agents"
SKILL = KIT / "claude-plugin" / "skills" / "turbo-picard" / "SKILL.md"
SNIPPETS = [SKILL, KIT / "codex" / "AGENTS.md", KIT / "cursor" / "turbo-picard.mdc"]
OVERCLAIMS = ("drop-in replacement for picard", "replaces picard", "full replacement for picard.")


def frontmatter(text: str) -> dict[str, str]:
    match = re.match(r"---\n(.*?)\n---\n", text, re.DOTALL)
    if match is None:
        return {}
    fields = {}
    for line in match.group(1).splitlines():
        key, _, value = line.partition(":")
        fields[key.strip()] = value.strip()
    return fields


class AgentKitTests(unittest.TestCase):
    def test_skill_frontmatter_names_the_skill_and_its_trigger(self) -> None:
        fields = frontmatter(SKILL.read_text(encoding="utf-8"))
        self.assertEqual(fields.get("name"), "turbo-picard")
        description = fields.get("description", "")
        self.assertLessEqual(len(description), 1024)
        for needle in ("Picard", "MarkDuplicates", "explain --json"):
            self.assertIn(needle, description)

    def test_every_snippet_carries_the_same_decision_rule(self) -> None:
        for path in SNIPPETS:
            text = path.read_text(encoding="utf-8")
            with self.subTest(path=path.name):
                for needle in (
                    "turbo-picard explain --json",
                    "native_scope",
                    "fallback_ready",
                    "fallback_install_hint",
                    "python3 -m pip install turbo-picard",
                    "-Xmx",
                ):
                    self.assertIn(needle, text)
                lowered = text.lower()
                for phrase in OVERCLAIMS:
                    self.assertNotIn(phrase, lowered.replace("not a full replacement for picard", ""))

    def test_plugin_metadata_names_the_plugin_and_ships_the_mcp_server(self) -> None:
        plugin = json.loads((KIT / "claude-plugin" / ".claude-plugin" / "plugin.json").read_text(encoding="utf-8"))
        self.assertEqual(plugin["name"], "turbo-picard")
        self.assertNotIn("version", plugin)
        mcp = json.loads((KIT / "claude-plugin" / ".mcp.json").read_text(encoding="utf-8"))
        self.assertEqual(mcp["mcpServers"]["turbo-picard"], {"command": "turbo-picard", "args": ["mcp"]})
        marketplace = json.loads((ROOT / ".claude-plugin" / "marketplace.json").read_text(encoding="utf-8"))
        source = marketplace["plugins"][0]["source"]
        self.assertTrue((ROOT / source / ".claude-plugin" / "plugin.json").is_file())

    def test_cursor_rule_has_valid_frontmatter(self) -> None:
        fields = frontmatter((KIT / "cursor" / "turbo-picard.mdc").read_text(encoding="utf-8"))
        self.assertIn("description", fields)
        self.assertEqual(fields.get("alwaysApply"), "false")


if __name__ == "__main__":
    unittest.main()
