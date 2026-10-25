#!/usr/bin/env python3
"""Tests for the evidence-derived competitor card."""

from __future__ import annotations

import importlib.util
import re
import struct
import sys
import unittest
import zlib
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("render_competitor_card.py")
SPEC = importlib.util.spec_from_file_location("render_competitor_card", MODULE_PATH)
assert SPEC is not None
card = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules["render_competitor_card"] = card
SPEC.loader.exec_module(card)


def result(seconds: float, parity: bool) -> dict:
    return {"successful_runs": 3, "median_wall_seconds": seconds, "full_parity": parity}


def tiny_png() -> bytes:
    def chunk(kind: bytes, data: bytes) -> bytes:
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))
    ihdr = struct.pack(">IIBBBBB", 1, 1, 8, 0, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr) + chunk(b"IDAT", zlib.compress(b"\x00\x00")) + chunk(b"IEND", b"")


class HeadlineTests(unittest.TestCase):
    def test_claims_overall_fastest_only_when_every_tool_is_slower(self) -> None:
        title, _ = card.headline({
            "candidate": result(1.0, True), "picard": result(5.0, True), "fastdup": result(1.2, False),
        })
        self.assertEqual(title, "The fastest MarkDuplicates.")

    def test_faster_tool_with_different_output_limits_claim_to_picard_identical(self) -> None:
        title, qualifier = card.headline({
            "candidate": result(2.5, True), "picard": result(5.0, True), "fastdup": result(1.2, False),
        })
        self.assertEqual(title, "The fastest Picard-identical MarkDuplicates.")
        self.assertIn("change the output", qualifier)

    def test_faster_exact_competitor_removes_fastest_claim(self) -> None:
        title, _ = card.headline({
            "candidate": result(2.5, True), "picard": result(5.0, True), "rival": result(1.2, True),
        })
        self.assertNotIn("fastest", title)

    def test_refuses_when_turbo_fails_parity_or_loses_to_picard(self) -> None:
        with self.assertRaises(ValueError):
            card.headline({"candidate": result(1.0, False), "picard": result(5.0, True)})
        with self.assertRaises(ValueError):
            card.headline({"candidate": result(6.0, True), "picard": result(5.0, True)})


class CardFileTests(unittest.TestCase):
    def test_committed_card_matches_current_evidence(self) -> None:
        self.assertEqual(card.check_card(), [])

    def test_png_hash_round_trips(self) -> None:
        tagged = card.png_with_text(tiny_png(), card.PNG_HASH_KEY, "abc123")
        self.assertEqual(card.png_text(tagged, card.PNG_HASH_KEY), "abc123")
        self.assertIsNone(card.png_text(tiny_png(), card.PNG_HASH_KEY))

    def test_progress_series_reports_percentage_reduction(self) -> None:
        first, last, pct = card.series_change({"points": [{"value": 4.0}, {"value": 3.0}]})
        self.assertEqual((first, last), (4.0, 3.0))
        self.assertAlmostEqual(pct, 25.0)
        self.assertIsNone(card.series_change({"points": [{"value": 4.0}]}))

    def test_both_layouts_name_every_measured_tool(self) -> None:
        content = card.card_content()
        for svg in (card.build_card(content), card.build_mobile_card(content)):
            for name in ("Turbo Picard", "Picard 3.4.0", "FastDup", "samtools markdup", "dupblaster"):
                self.assertIn(name, svg)
            self.assertIn("output differs from Picard", svg)

    def test_mobile_card_is_portrait_and_keeps_every_claim(self) -> None:
        content = card.card_content()
        svg = card.build_mobile_card(content)
        width, height = (int(v) for v in re.search(r'viewBox="0 0 (\d+) (\d+)"', svg).groups())
        self.assertEqual(width, card.MOBILE_WIDTH)
        self.assertGreater(height, width)
        for big, line1, _, _ in content["tiles"]:
            self.assertIn(f">{big}<", svg)
            self.assertIn(line1, svg)
        self.assertIn("pip install turbo-picard", svg)

    def test_wrap_keeps_words_whole_within_limit(self) -> None:
        lines = card.wrap("The fastest Picard-identical MarkDuplicates.", 21)
        self.assertEqual(lines, ["The fastest", "Picard-identical", "MarkDuplicates."])


if __name__ == "__main__":
    unittest.main()
