"""Command line: no telemetry, no automatic downloads, no implicit overwrite."""
from __future__ import annotations

import argparse
import itertools
import json
import platform
import resource
import statistics
import subprocess
import sys
import tempfile
import time
from dataclasses import asdict
from pathlib import Path

from . import __version__
from .core import Options, annotate_batch, backend, differences, load_models
from .io import atomic_output, batches, dump_json, read_fasta, sha256


def parser():
    root = argparse.ArgumentParser(prog="hmmforge", description="Experimental HMMER-backed domain annotation")
    root.add_argument("--version", action="version", version=__version__)
    subs = root.add_subparsers(dest="command", required=True)
    subs.add_parser("capabilities", help="emit machine-readable scope and limitations")
    for command in ("annotate", "verify", "benchmark"):
        p = subs.add_parser(command)
        p.add_argument("models", type=Path)
        p.add_argument("proteins", type=Path)
        p.add_argument("--cpus", type=int, default=1)
        p.add_argument("--seed", type=int, default=42)
        p.add_argument("--batch-residues", type=int, default=1_000_000)
        p.add_argument("--batch-count", type=int, default=4096)
        p.add_argument("--max-length", type=int, default=100_000)
        p.add_argument("--E", type=float, default=10.0)
        p.add_argument("--domE", type=float, default=10.0)
        p.add_argument("--incE", type=float, default=0.01)
        p.add_argument("--incdomE", type=float, default=0.01)
        p.add_argument("--cutoffs", choices=("gathering", "trusted", "noise"))
        if command == "annotate":
            p.add_argument("--engine", choices=("model-major", "scan"), default="model-major")
            p.add_argument("--output", type=Path, required=True)
        if command == "benchmark":
            p.add_argument("--repeats", type=int, default=3)
            p.add_argument("--dataset-kind", choices=("synthetic", "biological"), required=True)
    return root


def options(args):
    if min(args.batch_count, args.batch_residues, args.max_length) < 1:
        raise ValueError("batch limits and max-length must be positive")
    return Options(cpus=args.cpus, seed=args.seed, E=args.E, domE=args.domE,
                   incE=args.incE, incdomE=args.incdomE, bit_cutoffs=args.cutoffs)


def usage():
    value = resource.getrusage(resource.RUSAGE_SELF)
    return dict(cpu_seconds=value.ru_utime + value.ru_stime,
                peak_rss_bytes=int(value.ru_maxrss * (1 if sys.platform == "darwin" else 1024)))


def provenance(args, opts):
    return dict(hmmforge=__version__, pyhmmer=backend().__version__, python=platform.python_version(),
                platform=platform.platform(), machine=platform.machine(), options=asdict(opts),
                models_sha256=sha256(args.models), proteins_sha256=sha256(args.proteins),
                batch_count=args.batch_count, batch_residues=args.batch_residues,
                max_length=args.max_length, coordinates="1-based-inclusive")


def file_identity(path):
    stat = path.stat()
    return stat.st_dev, stat.st_ino, stat.st_size, stat.st_mtime_ns, stat.st_ctime_ns


def perform(args):
    started = time.perf_counter()
    opts = options(args)
    identities = [file_identity(p) for p in (args.models, args.proteins)]
    info = provenance(args, opts)
    models = load_models(args.models, opts)
    chunks = batches(read_fasta(args.proteins, args.max_length), args.batch_residues, args.batch_count)
    report = dict(schema="hmmforge.run.v1", command=args.command, provenance=info,
                  proteins=0, residues=0, reported_models=0, reported_domains=0, batches=0)
    def search():
        for batch in chunks:
            report["batches"] += 1
            report["proteins"] += len(batch)
            report["residues"] += sum(len(p.sequence) for p in batch)
            yield batch
    def unchanged():
        if identities != [file_identity(p) for p in (args.models, args.proteins)]:
            raise RuntimeError("input changed during execution; refusing to publish results")
    if args.command == "annotate":
        report["engine"] = args.engine
        with atomic_output(args.output) as handle:
            for batch in search():
                for row in annotate_batch(models, batch, opts, args.engine):
                    report["reported_models"] += len(row["hits"])
                    report["reported_domains"] += sum(len(h["domains"]) for h in row["hits"])
                    dump_json(row, handle)
            unchanged()
        report["output_sha256"] = sha256(args.output)
    else:
        report["parity"] = True
        report["mismatch_examples"] = []
        report["mismatched_batches"] = 0
        for batch in search():
            candidate = annotate_batch(models, batch, opts, "model-major")
            reference = annotate_batch(models, batch, opts, "scan")
            issues = differences(reference, candidate, f"batch{report['batches']}")
            if issues:
                report["parity"] = False
                report["mismatched_batches"] += 1
                report["mismatch_examples"] = (report["mismatch_examples"] + issues)[:20]
        unchanged()
    report.update(usage())
    report["wall_seconds"] = time.perf_counter() - started
    return report


def compare_files(a, b):
    problems, count = [], 0
    with open(a) as first, open(b) as second:
        for x, y in itertools.zip_longest(first, second):
            if x is None or y is None:
                problems.append("different number of output proteins")
                break
            count += 1
            problems.extend(differences(json.loads(x), json.loads(y), f"protein{count}", limit=3))
            if len(problems) >= 20:
                break
    return problems[:20]


def benchmark(args):
    """Fresh processes: package import, hashing, model loading and output included.

    OS page caches are NOT flushed. This is process-cold, not storage-cold.
    Baseline is optimised in-memory PyHMMER hmmscan, not the slow disk variant.
    """
    options(args)
    if not 1 <= args.repeats <= 100:
        raise ValueError("repeats must be between 1 and 100")
    runs, problems = [], []
    expected = None
    with tempfile.TemporaryDirectory(prefix="hmmforge-bench-") as temporary:
        for repeat in range(args.repeats):
            engines = ("scan", "model-major") if repeat % 2 == 0 else ("model-major", "scan")
            outputs = {}
            for engine in engines:
                output = Path(temporary) / f"{repeat}-{engine}.jsonl"
                command = [sys.executable, "-m", "hmmforge", "annotate", str(args.models), str(args.proteins),
                           "--engine", engine, "--output", str(output)]
                for key in ("cpus", "seed", "batch_residues", "batch_count", "max_length", "E", "domE", "incE", "incdomE"):
                    command += ["--" + key.replace("_", "-"), str(getattr(args, key))]
                if args.cutoffs:
                    command += ["--cutoffs", args.cutoffs]
                started = time.perf_counter()
                result = subprocess.run(command, text=True, capture_output=True, check=False)
                elapsed = time.perf_counter() - started
                if result.returncode:
                    raise RuntimeError(f"benchmark subprocess failed: {result.stderr[-4000:]}")
                run = json.loads(result.stdout)
                identity = run["provenance"]
                if expected is not None and expected != identity:
                    raise RuntimeError("benchmark input or configuration changed between runs")
                expected = identity
                run.update(repeat=repeat, end_to_end_seconds=elapsed)
                runs.append(run)
                outputs[engine] = output
            problems.extend(compare_files(outputs["scan"], outputs["model-major"]))
    medians = {engine: statistics.median(r["end_to_end_seconds"] for r in runs if r["engine"] == engine)
               for engine in ("scan", "model-major")}
    return dict(schema="hmmforge.benchmark.v1", dataset_kind=args.dataset_kind, parity=not problems,
                mismatch_examples=problems[:20], cache_state="process-cold; OS page cache uncontrolled",
                baseline="PyHMMER 0.12.3 in-memory optimized-profile hmmscan", runs=runs,
                medians_seconds=medians,
                speedup_scan_over_model_major=medians["scan"] / medians["model-major"] if not problems else None,
                production_claim_permitted=False)


def main(argv=None):
    args = parser().parse_args(argv)
    try:
        if args.command == "capabilities":
            report = dict(schema="hmmforge.capabilities.v1", version=__version__, status="experimental",
                          backend="pyhmmer==0.12.3", new_scoring_kernel=False,
                          engines=["model-major", "scan"], inputs=["amino HMMER3", "protein FASTA", "FASTA.gz"],
                          outputs=["versioned JSONL"], unsupported=["full hmmscan CLI", "domtblout", "GPU", "InterProScan replacement", "DNA/RNA", "clan overlap resolution"],
                          telemetry=False, auto_downloads=False, production_validated=False,
                          exit_codes={"0": "success/parity", "2": "invalid input or execution failure", "3": "parity mismatch"})
        else:
            report = benchmark(args) if args.command == "benchmark" else perform(args)
        dump_json(report, sys.stdout)
        return 0 if report.get("parity", True) else 3
    except (ValueError, OSError, RuntimeError) as exc:
        dump_json(dict(schema="hmmforge.error.v1", error=type(exc).__name__, message=str(exc)), sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
