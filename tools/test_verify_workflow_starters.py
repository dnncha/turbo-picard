#!/usr/bin/env python3

from __future__ import annotations

import contextlib
import importlib.util
import sys
import tempfile
import unittest
from collections.abc import Iterator
from pathlib import Path
from unittest import mock

MODULE_PATH = Path(__file__).with_name("verify_workflow_starters.py")
SPEC = importlib.util.spec_from_file_location("verify_workflow_starters", MODULE_PATH)
assert SPEC
assert SPEC.loader
verify_workflow_starters = importlib.util.module_from_spec(SPEC)
sys.modules["verify_workflow_starters"] = verify_workflow_starters
SPEC.loader.exec_module(verify_workflow_starters)


@contextlib.contextmanager
def patched_paths(root: Path, workflows: Path, docs: Path) -> Iterator[None]:
    with mock.patch.multiple(
        verify_workflow_starters,
        ROOT=root,
        WORKFLOWS=workflows,
        README=root / "README.md",
        USE_CASES=docs / "use-cases.rst",
        EVALUATION_PLAYBOOK=docs / "evaluation-playbook.rst",
        FAQ=docs / "faq.rst",
    ):
        yield


class WorkflowStarterVerifierTests(unittest.TestCase):
    def test_validate_workflow_starters_accepts_complete_bundle(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            root = Path(tmpdir)
            workflows = root / "packaging" / "workflows"
            workflows.mkdir(parents=True)
            (root / "README.md").write_text(
                "\n".join(
                    [
                        "per-read-group `SamToFastq`",
                        "sequential-shard `FastqToSam`",
                        "mate-repair boundaries around `FixMateInformation`",
                    ]
                ),
                encoding="utf-8",
            )
            docs = root / "docs"
            docs.mkdir()
            (docs / "use-cases.rst").write_text(
                "\n".join(
                    [
                        "packaging/workflows/samtofastq.wdl",
                        "packaging/workflows/fastqtosam.wdl",
                        "packaging/workflows/fixmateinformation.wdl",
                        "packaging/workflows/samtofastq.nf",
                        "packaging/workflows/fastqtosam.nf",
                        "packaging/workflows/fixmateinformation.nf",
                        "SamToFastq",
                        "FastqToSam",
                        "FixMateInformation",
                        "turbo-picard explain --json",
                    ]
                ),
                encoding="utf-8",
            )
            (docs / "evaluation-playbook.rst").write_text(
                "\n".join(
                    [
                        "samtofastq.wdl",
                        "fastqtosam.wdl",
                        "fixmateinformation.wdl",
                        "fastqtosam.nf",
                        "fixmateinformation.nf",
                        "trial-samtofastq.nf",
                        "trial-samtofastq.wdl",
                        "trial-fastqtosam.wdl",
                        "trial-fixmateinformation.nf",
                    ]
                ),
                encoding="utf-8",
            )
            (docs / "faq.rst").write_text(
                "FastqToSam\nFixMateInformation\n",
                encoding="utf-8",
            )
            (workflows / "README.md").write_text(
                "\n".join(
                    [
                        "fastqtosam.wdl",
                        "per-read-group export",
                        "sequential-ingest toggles",
                        "trial-samtofastq.nf",
                        "trial-samtofastq.wdl",
                        "trial-fastqtosam.wdl",
                        "trial-fixmateinformation.nf",
                        "turbo-picard explain --json",
                    ]
                ),
                encoding="utf-8",
            )
            (workflows / "one-command-trial.md").write_text(
                "\n".join(
                    [
                        "python3 -m pip install turbo-picard",
                        "turbo-picard doctor",
                        "--commands MarkDuplicates",
                        "trial-samtofastq.nf",
                        "trial-samtofastq.wdl",
                        "trial-fastqtosam.wdl",
                        "trial-fixmateinformation.nf",
                    ]
                )
                + "\n",
                encoding="utf-8",
            )
            (workflows / "trial-config.yaml").write_text(
                "output_per_rg: false\nrg_tag: PU\nuse_sequential_fastqs: false\n",
                encoding="utf-8",
            )
            (workflows / "samtofastq.nf").write_text(
                "OUTPUT_PER_RG=\nRG_TAG=\nOUTPUT_DIR=\ndef outputArgs = outputPerRg\n",
                encoding="utf-8",
            )
            (workflows / "samtofastq.wdl").write_text(
                'OUTPUT_PER_RG=\nRG_TAG=\nOUTPUT_DIR=\nif output_per_rg then "OUTPUT_PER_RG=true"\nFile? fastq\nBoolean output_per_rg = false\n',
                encoding="utf-8",
            )
            (workflows / "fastqtosam.nf").write_text("USE_SEQUENTIAL_FASTQS=\n", encoding="utf-8")
            (workflows / "fastqtosam.wdl").write_text("USE_SEQUENTIAL_FASTQS=\n", encoding="utf-8")
            (workflows / "fixmateinformation.nf").write_text("CREATE_INDEX=true\n", encoding="utf-8")
            (workflows / "fixmateinformation.wdl").write_text("CREATE_INDEX=true\n", encoding="utf-8")
            (workflows / "trial-samtofastq.nf").write_text("OUTPUT_PER_RG=\n", encoding="utf-8")
            (workflows / "trial-samtofastq.wdl").write_text("OUTPUT_PER_RG=\nFile? fastq\n", encoding="utf-8")
            (workflows / "trial-fastqtosam.nf").write_text("USE_SEQUENTIAL_FASTQS=\n", encoding="utf-8")
            (workflows / "trial-fastqtosam.wdl").write_text("USE_SEQUENTIAL_FASTQS=\n", encoding="utf-8")
            (workflows / "trial-fixmateinformation.wdl").write_text("CREATE_INDEX=true\n", encoding="utf-8")
            (workflows / "trial-fixmateinformation.nf").write_text("CREATE_INDEX=true\n", encoding="utf-8")
            (workflows / "Snakefile").write_text(
                "\n".join(
                    [
                        "rule sam_to_fastq:",
                        "    shell:",
                        '        "OUTPUT_PER_RG=true"',
                        "",
                        "rule sam_to_fastq_per_rg:",
                        "    shell:",
                        '        "OUTPUT_PER_RG=true"',
                        "",
                        "rule fastq_to_sam:",
                        "    shell:",
                        '        "USE_SEQUENTIAL_FASTQS=false"',
                        "",
                        "rule fix_mate_information:",
                        "    shell:",
                        '        "CREATE_INDEX=true"',
                        "",
                    ]
                ),
                encoding="utf-8",
            )

            with patched_paths(root, workflows, docs):
                assert verify_workflow_starters.validate_workflow_starters() == []

    def test_validate_workflow_starters_reports_missing_patterns(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            root = Path(tmpdir)
            workflows = root / "packaging" / "workflows"
            workflows.mkdir(parents=True)
            (root / "README.md").write_text("workflow docs\n", encoding="utf-8")
            docs = root / "docs"
            docs.mkdir()
            (docs / "use-cases.rst").write_text("MarkDuplicates\n", encoding="utf-8")
            (docs / "evaluation-playbook.rst").write_text("markduplicates.wdl\n", encoding="utf-8")
            (docs / "faq.rst").write_text("MarkDuplicates\n", encoding="utf-8")
            (workflows / "README.md").write_text("workflow starters\n", encoding="utf-8")
            (workflows / "one-command-trial.md").write_text("trial.wdl\n", encoding="utf-8")
            (workflows / "trial-config.yaml").write_text("sample_id: trial\n", encoding="utf-8")
            (workflows / "samtofastq.nf").write_text("FASTQ=\n", encoding="utf-8")
            (workflows / "samtofastq.wdl").write_text("FASTQ=\n", encoding="utf-8")
            (workflows / "fastqtosam.nf").write_text("FASTQ=\n", encoding="utf-8")
            (workflows / "fastqtosam.wdl").write_text("FASTQ=\n", encoding="utf-8")
            (workflows / "trial-samtofastq.wdl").write_text("FASTQ=\n", encoding="utf-8")
            (workflows / "trial-fastqtosam.nf").write_text("FASTQ=\n", encoding="utf-8")
            (workflows / "trial-fastqtosam.wdl").write_text("FASTQ=\n", encoding="utf-8")
            (workflows / "fixmateinformation.nf").write_text("SORT_ORDER=coordinate\n", encoding="utf-8")
            (workflows / "fixmateinformation.wdl").write_text("SORT_ORDER=coordinate\n", encoding="utf-8")
            (workflows / "trial-fixmateinformation.wdl").write_text("SORT_ORDER=coordinate\n", encoding="utf-8")
            (workflows / "trial-fixmateinformation.nf").write_text("CREATE_INDEX=true\n", encoding="utf-8")
            (workflows / "Snakefile").write_text("SORT_ORDER=coordinate\n", encoding="utf-8")

            with patched_paths(root, workflows, docs):
                errors = verify_workflow_starters.validate_workflow_starters()

            assert "packaging/workflows/samtofastq.nf missing Nextflow SamToFastq per-read-group toggle" in errors
            assert "packaging/workflows/fastqtosam.wdl missing WDL FastqToSam sequential FASTQ support" in errors
            assert (
                "packaging/workflows/fixmateinformation.nf still hard-codes FixMateInformation SORT_ORDER=coordinate"
                in errors
            )
            assert "README.md missing per-read-group SamToFastq migration wording" in errors
            assert "packaging/workflows/README.md missing WDL FastqToSam starter listing" in errors
            assert "packaging/workflows/README.md missing Nextflow SamToFastq trial listing" in errors
            assert "packaging/workflows/trial-samtofastq.nf missing tiny SamToFastq trial" in errors
            assert (
                "packaging/workflows/one-command-trial.md missing SamToFastq trial mention in one-command-trial"
                in errors
            )
            assert (
                "packaging/workflows/one-command-trial.md missing PyPI install command in one-command-trial" in errors
            )
            assert (
                "docs/evaluation-playbook.rst missing Nextflow SamToFastq trial mention in evaluation playbook"
                in errors
            )
            assert "docs/use-cases.rst missing WDL SamToFastq starter mention in use-cases" in errors
            assert (
                "docs/evaluation-playbook.rst missing WDL SamToFastq starter mention in evaluation playbook" in errors
            )
            assert "docs/faq.rst missing FastqToSam mention in FAQ shortlist" in errors
            assert "packaging/workflows/trial-config.yaml missing SamToFastq trial toggle in trial-config" in errors
            assert "packaging/workflows/trial-samtofastq.wdl missing optional single-FASTQ output" in errors
            assert (
                "packaging/workflows/trial-fixmateinformation.wdl still hard-codes FixMateInformation SORT_ORDER=coordinate"
                in errors
            )


if __name__ == "__main__":
    unittest.main()
