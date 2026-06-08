#!/usr/bin/env python3
"""Tests for workflow starter verification."""

from __future__ import annotations

import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("verify_workflow_starters.py")
SPEC = importlib.util.spec_from_file_location("verify_workflow_starters", MODULE_PATH)
assert SPEC and SPEC.loader
verify_workflow_starters = importlib.util.module_from_spec(SPEC)
sys.modules["verify_workflow_starters"] = verify_workflow_starters
SPEC.loader.exec_module(verify_workflow_starters)


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
                    ]
                ),
                encoding="utf-8",
            )
            (workflows / "one-command-trial.md").write_text(
                "trial-samtofastq.nf\ntrial-samtofastq.wdl\ntrial-fastqtosam.wdl\ntrial-fixmateinformation.nf\n",
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
                "OUTPUT_PER_RG=\nRG_TAG=\nOUTPUT_DIR=\nif output_per_rg then \"OUTPUT_PER_RG=true\"\nFile? fastq\nBoolean output_per_rg = false\n",
                encoding="utf-8",
            )
            (workflows / "fastqtosam.nf").write_text("USE_SEQUENTIAL_FASTQS=\n", encoding="utf-8")
            (workflows / "fastqtosam.wdl").write_text("USE_SEQUENTIAL_FASTQS=\n", encoding="utf-8")
            (workflows / "fixmateinformation.nf").write_text("CREATE_INDEX=true\n", encoding="utf-8")
            (workflows / "fixmateinformation.wdl").write_text("CREATE_INDEX=true\n", encoding="utf-8")
            (workflows / "trial-samtofastq.nf").write_text("OUTPUT_PER_RG=\n", encoding="utf-8")
            (workflows / "trial-samtofastq.wdl").write_text(
                "OUTPUT_PER_RG=\nFile? fastq\n", encoding="utf-8"
            )
            (workflows / "trial-fastqtosam.nf").write_text("USE_SEQUENTIAL_FASTQS=\n", encoding="utf-8")
            (workflows / "trial-fastqtosam.wdl").write_text("USE_SEQUENTIAL_FASTQS=\n", encoding="utf-8")
            (workflows / "trial-fixmateinformation.wdl").write_text(
                "CREATE_INDEX=true\n", encoding="utf-8"
            )
            (workflows / "trial-fixmateinformation.nf").write_text(
                "CREATE_INDEX=true\n", encoding="utf-8"
            )
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

            old_root = verify_workflow_starters.ROOT
            old_workflows = verify_workflow_starters.WORKFLOWS
            old_readme = verify_workflow_starters.README
            old_use_cases = verify_workflow_starters.USE_CASES
            old_evaluation_playbook = verify_workflow_starters.EVALUATION_PLAYBOOK
            old_faq = verify_workflow_starters.FAQ
            try:
                verify_workflow_starters.ROOT = root
                verify_workflow_starters.WORKFLOWS = workflows
                verify_workflow_starters.README = root / "README.md"
                verify_workflow_starters.USE_CASES = docs / "use-cases.rst"
                verify_workflow_starters.EVALUATION_PLAYBOOK = docs / "evaluation-playbook.rst"
                verify_workflow_starters.FAQ = docs / "faq.rst"
                self.assertEqual(verify_workflow_starters.validate_workflow_starters(), [])
            finally:
                verify_workflow_starters.ROOT = old_root
                verify_workflow_starters.WORKFLOWS = old_workflows
                verify_workflow_starters.README = old_readme
                verify_workflow_starters.USE_CASES = old_use_cases
                verify_workflow_starters.EVALUATION_PLAYBOOK = old_evaluation_playbook
                verify_workflow_starters.FAQ = old_faq

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
            (workflows / "fixmateinformation.nf").write_text(
                "SORT_ORDER=coordinate\n", encoding="utf-8"
            )
            (workflows / "fixmateinformation.wdl").write_text(
                "SORT_ORDER=coordinate\n", encoding="utf-8"
            )
            (workflows / "trial-fixmateinformation.wdl").write_text(
                "SORT_ORDER=coordinate\n", encoding="utf-8"
            )
            (workflows / "trial-fixmateinformation.nf").write_text(
                "CREATE_INDEX=true\n", encoding="utf-8"
            )
            (workflows / "Snakefile").write_text("SORT_ORDER=coordinate\n", encoding="utf-8")

            old_root = verify_workflow_starters.ROOT
            old_workflows = verify_workflow_starters.WORKFLOWS
            old_readme = verify_workflow_starters.README
            old_use_cases = verify_workflow_starters.USE_CASES
            old_evaluation_playbook = verify_workflow_starters.EVALUATION_PLAYBOOK
            old_faq = verify_workflow_starters.FAQ
            try:
                verify_workflow_starters.ROOT = root
                verify_workflow_starters.WORKFLOWS = workflows
                verify_workflow_starters.README = root / "README.md"
                verify_workflow_starters.USE_CASES = docs / "use-cases.rst"
                verify_workflow_starters.EVALUATION_PLAYBOOK = docs / "evaluation-playbook.rst"
                verify_workflow_starters.FAQ = docs / "faq.rst"
                errors = verify_workflow_starters.validate_workflow_starters()
            finally:
                verify_workflow_starters.ROOT = old_root
                verify_workflow_starters.WORKFLOWS = old_workflows
                verify_workflow_starters.README = old_readme
                verify_workflow_starters.USE_CASES = old_use_cases
                verify_workflow_starters.EVALUATION_PLAYBOOK = old_evaluation_playbook
                verify_workflow_starters.FAQ = old_faq

            self.assertIn(
                "packaging/workflows/samtofastq.nf missing Nextflow SamToFastq per-read-group toggle",
                errors,
            )
            self.assertIn(
                "packaging/workflows/fastqtosam.wdl missing WDL FastqToSam sequential FASTQ support",
                errors,
            )
            self.assertIn(
                "packaging/workflows/fixmateinformation.nf still hard-codes FixMateInformation SORT_ORDER=coordinate",
                errors,
            )
            self.assertIn(
                "README.md missing per-read-group SamToFastq migration wording",
                errors,
            )
            self.assertIn(
                "packaging/workflows/README.md missing WDL FastqToSam starter listing",
                errors,
            )
            self.assertIn(
                "packaging/workflows/README.md missing Nextflow SamToFastq trial listing",
                errors,
            )
            self.assertIn(
                "packaging/workflows/trial-samtofastq.nf missing tiny SamToFastq trial",
                errors,
            )
            self.assertIn(
                "packaging/workflows/one-command-trial.md missing SamToFastq trial mention in one-command-trial",
                errors,
            )
            self.assertIn(
                "docs/evaluation-playbook.rst missing Nextflow SamToFastq trial mention in evaluation playbook",
                errors,
            )
            self.assertIn(
                "docs/use-cases.rst missing WDL SamToFastq starter mention in use-cases",
                errors,
            )
            self.assertIn(
                "docs/evaluation-playbook.rst missing WDL SamToFastq starter mention in evaluation playbook",
                errors,
            )
            self.assertIn(
                "docs/faq.rst missing FastqToSam mention in FAQ shortlist",
                errors,
            )
            self.assertIn(
                "packaging/workflows/trial-config.yaml missing SamToFastq trial toggle in trial-config",
                errors,
            )
            self.assertIn(
                "packaging/workflows/trial-samtofastq.wdl missing optional single-FASTQ output",
                errors,
            )
            self.assertIn(
                "packaging/workflows/trial-fixmateinformation.wdl still hard-codes FixMateInformation SORT_ORDER=coordinate",
                errors,
            )


if __name__ == "__main__":
    unittest.main()
