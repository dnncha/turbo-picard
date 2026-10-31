from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

from tools import verify_real_data_ci_coverage as coverage


class DiscoveryContractTests(unittest.TestCase):
    def test_bulk_commands_cover_tests_and_compilation_not_release_gates(self) -> None:
        commands = "python3 -m unittest discover -s tools\npython3 -m compileall -q tools\n"
        required = [
            s
            for s in coverage.REQUIRED_SNIPPETS
            if not s.startswith("python3 -m unittest ") and not s.startswith("tools/")
        ]
        ci = commands + "\n".join(required)
        assert coverage.validate_ci_coverage(ci) == []
        ci = ci.replace("python3 tools/verify_real_data_evidence.py --release-ready", "")
        assert (
            "CI missing release-critical helper coverage: python3 tools/verify_real_data_evidence.py --release-ready"
            in coverage.validate_ci_coverage(ci)
        )

    def test_commented_commands_do_not_cover_execution_or_compilation(self) -> None:
        ci = "# python3 -m unittest discover -s tools\n# python3 -m compileall -q tools\n"
        assert not coverage.has_discovery(ci)
        assert not coverage.has_compileall(ci)
        assert coverage.validate_ci_coverage(ci)

    def test_compileall_covers_future_tool_files(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tools = Path(tmp)
            (tools / "future.py").write_text("pass\n")
            assert coverage.validate_python_tool_compile_coverage("  python3 -m compileall -q tools\n", tools) == []
            assert coverage.validate_python_tool_compile_coverage("# python3 -m compileall -q tools\n", tools)


if __name__ == "__main__":
    unittest.main()
