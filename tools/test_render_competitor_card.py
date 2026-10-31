#!/usr/bin/env python3

from __future__ import annotations

import importlib.util
import re
import struct
import sys
import unittest
import zlib
from pathlib import Path
from unittest.mock import patch

import pytest

MODULE_PATH = Path(__file__).with_name("render_competitor_card.py")
SPEC = importlib.util.spec_from_file_location("render_competitor_card", MODULE_PATH)
assert SPEC is not None
card = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules["render_competitor_card"] = card
SPEC.loader.exec_module(card)


def result(seconds: float, parity: bool) -> dict[str, object]:
    return {"successful_runs": 3, "median_wall_seconds": seconds, "full_parity": parity}


def tiny_png() -> bytes:
    def chunk(kind: bytes, data: bytes) -> bytes:
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))

    ihdr = struct.pack(">IIBBBBB", 1, 1, 8, 0, 0, 0, 0)
    return (
        b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr) + chunk(b"IDAT", zlib.compress(b"\x00\x00")) + chunk(b"IEND", b"")
    )


class HeadlineTests(unittest.TestCase):
    def test_comparison_tiles_follow_direction(self) -> None:
        for metric, labels in [
            ("memory", ["more memory than Picard", "same memory as Picard", "less memory than Picard"]),
            ("speed", ["slower than Picard", "same speed as Picard", "faster than Picard"]),
        ]:
            for ratio, label in zip([0.5, 1, 2], labels):
                value, actual, _, _ = card.comparison_tile(ratio, metric, "fixture")
                assert actual == label
                assert value == ("1x" if ratio == 1 else card.fmt_ratio(2))

    def test_card_content_routes_all_comparison_tiles_through_direction(self) -> None:
        for ratio, speed_label, memory_label in [
            (0.5, "slower than Picard", "more memory than Picard"),
            (1, "same speed as Picard", "same memory as Picard"),
            (2, "faster than Picard", "less memory than Picard"),
        ]:
            results = {
                card.TURBO: {**result(2, True), "median_peak_rss_bytes": 2},
                card.REFERENCE: {**result(2 * ratio, True), "median_peak_rss_bytes": 2 * ratio},
            }
            with (
                patch.object(card, "case_results", return_value=results),
                patch.object(card, "headline", return_value=("Fixture", "Fixture")),
            ):
                tiles = card.card_content()["tiles"]
            assert [tile[1] for tile in tiles[:3]] == [speed_label, memory_label, speed_label]

    def test_claims_overall_fastest_only_when_every_tool_is_slower(self) -> None:
        title, qualifier = card.headline(
            {
                "candidate": result(1.0, True),
                "picard": result(5.0, True),
                "fastdup": result(1.2, False),
            }
        )
        assert title == "The fastest MarkDuplicates we've measured."
        assert "only one with Picard-identical output" in qualifier

    def test_overall_fastest_does_not_claim_sole_parity_when_a_rival_matches(self) -> None:
        _, qualifier = card.headline(
            {
                "candidate": result(1.0, True),
                "picard": result(5.0, True),
                "rival": result(1.2, True),
            }
        )
        assert "only one" not in qualifier

    def test_faster_tool_with_different_output_limits_claim_to_picard_identical(self) -> None:
        title, qualifier = card.headline(
            {
                "candidate": result(2.5, True),
                "picard": result(5.0, True),
                "fastdup": result(1.2, False),
            }
        )
        assert title == "The fastest Picard-identical MarkDuplicates."
        assert "change the output" in qualifier

    def test_faster_exact_competitor_removes_fastest_claim(self) -> None:
        title, _ = card.headline(
            {
                "candidate": result(2.5, True),
                "picard": result(5.0, True),
                "rival": result(1.2, True),
            }
        )
        assert "fastest" not in title

    def test_refuses_when_turbo_fails_parity_or_loses_to_picard(self) -> None:
        with pytest.raises(ValueError, match="full Picard parity"):
            card.headline({"candidate": result(1.0, False), "picard": result(5.0, True)})
        with pytest.raises(ValueError, match="not faster than Picard"):
            card.headline({"candidate": result(6.0, True), "picard": result(5.0, True)})


class CardFileTests(unittest.TestCase):
    def test_committed_card_matches_current_evidence(self) -> None:
        assert card.check_card() == []

    def test_png_hash_round_trips(self) -> None:
        tagged = card.png_with_text(tiny_png(), card.PNG_HASH_KEY, "abc123")
        assert card.png_text(tagged, card.PNG_HASH_KEY) == "abc123"
        assert card.png_text(tiny_png(), card.PNG_HASH_KEY) is None

    def test_progress_chains_same_run_ratios_not_absolute_times(self) -> None:
        ratios, pct = card.series_change(
            {
                "steps": [
                    {"before": 4.0, "after": 3.0},
                    {"before": 10.0, "after": 5.0},
                ]
            }
        )
        assert ratios == [1.0, 0.75, 0.375]
        assert pct == pytest.approx(62.5)
        assert card.series_change({"steps": []}) is None

    def test_newest_landscape_is_chosen_by_date_across_json_and_gz(self) -> None:
        path = card.latest_landscape()
        assert card.landscape_date(path) == max(
            card.landscape_date(p) for p in card.LANDSCAPE_DIR.glob("landscape-*.json*")
        )
        assert "summaries" in card.load_landscape(path)

    def test_source_label_marks_measured_patches(self) -> None:
        assert card.source_label("a4aeaa83659c+patch-4447") == "a4aeaa8+patch"
        assert card.source_label("d14b74494dde") == "d14b744"

    def test_both_layouts_name_every_measured_tool(self) -> None:
        content = card.card_content()
        for svg in (card.build_card(content), card.build_mobile_card(content)):
            for tool in content["primary"]:
                assert content["names"][tool] in svg
            assert "output differs from Picard" in svg

    def test_mobile_card_is_portrait_and_keeps_every_claim(self) -> None:
        content = card.card_content()
        svg = card.build_mobile_card(content)
        viewbox = re.search(r'viewBox="0 0 (\d+) (\d+)"', svg)
        assert viewbox is not None
        width, height = (int(v) for v in viewbox.groups())
        assert width == card.MOBILE_WIDTH
        assert height > width
        for big, line1, _, _ in content["tiles"]:
            assert f">{big}<" in svg
            assert line1 in svg
        assert "pip install turbo-picard" in svg

    def test_wrap_keeps_words_whole_within_limit(self) -> None:
        lines = card.wrap("The fastest Picard-identical MarkDuplicates.", 21)
        assert lines == ["The fastest", "Picard-identical", "MarkDuplicates."]


if __name__ == "__main__":
    unittest.main()
