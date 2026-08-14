#!/usr/bin/env python3
"""Production audit wrapper around tools/compare_real_data.py.

Builds a side-by-side evidence bundle suitable for pipeline owners who want
Picard-vs-turbo-picard proof before switching a command in production.
"""

from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
COMPARE = ROOT / "tools" / "compare_real_data.py"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Run a production audit bundle for turbo-picard vs Picard.",
    )
    parser.add_argument("--input-bam", required=True, type=Path, help="BAM or CRAM input alignment.")
    parser.add_argument(
        "--reference-fasta",
        type=Path,
        help="Reference FASTA required when --input-bam is CRAM.",
    )
    parser.add_argument("--output-dir", required=True, type=Path)
    parser.add_argument("--dataset-id", required=True)
    parser.add_argument(
        "--input-source-url",
        required=True,
        help="Immutable public URL or accession for the input BAM.",
    )
    parser.add_argument(
        "--input-source-commit",
        required=True,
        help="40-character git SHA, release id, or accession for the input source.",
    )
    parser.add_argument(
        "--scope-caveat",
        default="production audit bundle for a representative workflow shard",
    )
    parser.add_argument(
        "--commands",
        nargs="+",
        default=[
            "MarkDuplicates",
            "SortSam",
            "SamToFastq",
            "ViewSam",
            "CleanSam",
            "CollectQualityYieldMetrics",
            "ValidateSamFile",
        ],
    )
    parser.add_argument(
        "--markduplicates-arg",
        action="append",
        default=[],
        metavar="KEY=VALUE",
        help="Additional MarkDuplicates KEY=VALUE argument, repeatable",
    )
    parser.add_argument(
        "--picard-command",
        default="picard",
        help="Picard entrypoint, for example: mamba run -p /opt/conda/envs/picard picard",
    )
    parser.add_argument(
        "--turbo-picard-command",
        default=str(ROOT / "target" / "release" / "picard"),
    )
    parser.add_argument("--skip-build", action="store_true")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if not args.input_bam.exists():
        raise SystemExit(f"missing input alignment: {args.input_bam}")
    if args.input_bam.suffix.lower() == ".cram":
        if args.reference_fasta is None:
            raise SystemExit("CRAM input requires --reference-fasta")
        if not args.reference_fasta.exists():
            raise SystemExit(f"missing reference FASTA: {args.reference_fasta}")
    compare_args = [
        sys.executable,
        str(COMPARE),
        "--input-bam",
        str(args.input_bam),
        "--input-source-url",
        args.input_source_url,
        "--input-source-commit",
        args.input_source_commit,
        "--output-dir",
        str(args.output_dir),
        "--dataset-id",
        args.dataset_id,
        "--scope-caveat",
        args.scope_caveat,
        "--release-tier",
        "release_candidate",
        "--picard-command",
        args.picard_command,
        "--turbo-picard-command",
        args.turbo_picard_command,
        "--commands",
        *args.commands,
    ]
    for argument in args.markduplicates_arg:
        compare_args.extend(["--markduplicates-arg", argument])
    if args.skip_build:
        compare_args.append("--skip-build")
    if args.reference_fasta is not None:
        compare_args.extend(["--reference-fasta", str(args.reference_fasta)])
    completed = subprocess.run(compare_args, cwd=ROOT, check=False)
    if completed.returncode != 0:
        return completed.returncode

    manifest = args.output_dir / "manifest-entry.json"
    if manifest.exists():
        update = subprocess.run(
            [
                sys.executable,
                str(ROOT / "tools" / "update_real_data_manifest.py"),
                "--entry",
                str(manifest),
            ],
            cwd=ROOT,
            check=False,
        )
        if update.returncode != 0:
            return update.returncode

    print(f"audit bundle written to {args.output_dir}")
    print("review comparison.md and comparison.json before changing production Picard calls")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
