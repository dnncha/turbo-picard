"""Three-engine, parity-gated, fresh-process workload study.

python -m hmmforge.study run models.hmm proteins.fa --output-dir study
The _worker subcommand is public for reproducible profiling but not a stable API.
"""
from __future__ import annotations

import argparse
import json
import os
import platform
import statistics
import subprocess
import sys
import time
from pathlib import Path

from .__main__ import compare_files, file_identity, options, provenance, usage
from .core import annotate_batch, load_models
from .io import atomic_output, batches, dump_json, read_fasta, sha256

ENGINES = ("scan", "direct", "model-major")
DESCRIPTIONS = {
    "scan": "PyHMMER optimized-profile hmmscan; bounded sequence batches",
    "direct": "Direct PyHMMER model-major; all sequences resident; independent extraction",
    "model-major": "HMMForge model-major; bounded sequence batches; compact extraction",
}


def parser():
    root = argparse.ArgumentParser(description=__doc__)
    sub = root.add_subparsers(dest="command", required=True)
    for name in ("run", "_worker"):
        p = sub.add_parser(name)
        p.add_argument("models", type=Path)
        p.add_argument("proteins", type=Path)
        for key, default in (("cpus", 1), ("seed", 42), ("batch-count", 4096),
                             ("batch-residues", 1_000_000), ("max-length", 100_000)):
            p.add_argument("--"+key, type=int, default=default)
        for key, default in (("E", 10.0), ("domE", 10.0), ("incE", 0.01), ("incdomE", 0.01)):
            p.add_argument("--"+key, type=float, default=default)
        p.add_argument("--cutoffs", choices=("gathering", "trusted", "noise"))
        if name == "run":
            p.add_argument("--output-dir", type=Path, required=True)
            p.add_argument("--repeats", type=int, default=3)
            p.add_argument("--timeout-seconds", type=int, default=1800)
            p.add_argument("--dataset-kind", choices=("synthetic", "biological"), required=True)
        else:
            p.add_argument("--engine", choices=ENGINES, required=True)
            p.add_argument("--output", type=Path, required=True)
    return root


def order(repeat):
    """Cyclic balance, reversed in alternate groups of three repeats."""
    values = ENGINES if (repeat//3) % 2 == 0 else tuple(reversed(ENGINES))
    offset = repeat % 3
    return values[offset:] + values[:offset]


def worker(args):
    opts = options(args)
    started = time.perf_counter()
    identity = [file_identity(p) for p in (args.models, args.proteins)]
    phases = {}
    t = time.perf_counter()
    origin = provenance(args, opts)
    phases["provenance_and_input_hashing"] = time.perf_counter()-t
    t = time.perf_counter()
    database = load_models(args.models, opts)
    phases["model_load_and_prepare"] = time.perf_counter()-t
    phases.update(input_parse=0., search_digitize_and_extract=0., serialize=0.)
    counts = dict(proteins=0, residues=0, reported_models=0, reported_domains=0, batches=0)
    with atomic_output(args.output) as handle:
        if args.engine == "direct":
            from .baseline import direct_search
            t = time.perf_counter()
            proteins = list(read_fasta(args.proteins, args.max_length))
            phases["input_parse"] += time.perf_counter()-t
            t = time.perf_counter()
            result = direct_search(database, proteins, opts)
            phases["search_digitize_and_extract"] += time.perf_counter()-t
            chunks = iter([(proteins, result)])
        else:
            chunks = iter(batches(read_fasta(args.proteins, args.max_length),
                                 args.batch_residues, args.batch_count))
        while True:
            t = time.perf_counter()
            try:
                item = next(chunks)
            except StopIteration:
                break
            if args.engine == "direct":
                proteins, result = item
            else:
                proteins = item
                phases["input_parse"] += time.perf_counter()-t
                t = time.perf_counter()
                result = annotate_batch(database, proteins, opts, args.engine)
                phases["search_digitize_and_extract"] += time.perf_counter()-t
            counts["batches"] += 1
            counts["proteins"] += len(proteins)
            counts["residues"] += sum(len(p.sequence) for p in proteins)
            t = time.perf_counter()
            for row in result:
                counts["reported_models"] += len(row["hits"])
                counts["reported_domains"] += sum(len(h["domains"]) for h in row["hits"])
                dump_json(row, handle)
            phases["serialize"] += time.perf_counter()-t
        if identity != [file_identity(p) for p in (args.models, args.proteins)]:
            raise RuntimeError("input changed during execution")
    t = time.perf_counter()
    output_hash = sha256(args.output)
    phases["output_hashing"] = time.perf_counter()-t
    elapsed = time.perf_counter()-started
    phases["other_including_publish"] = max(0., elapsed-sum(phases.values()))
    return dict(schema="hmmforge.study-worker.v1", engine=args.engine,
                provenance=origin, models=len(database), **counts, **usage(),
                wall_seconds=elapsed, phases_seconds=phases, output_sha256=output_hash,
                phase_caveat="Search includes digitization, native HMMER, scheduling and extraction; not a kernel profile",
                memory_strategy="fully-resident" if args.engine == "direct" else "bounded-sequence-batches")


def command_for(args, engine, output):
    cmd = [sys.executable, "-m", "hmmforge.study", "_worker", str(args.models.resolve()),
           str(args.proteins.resolve()), "--engine", engine, "--output", str(output.resolve())]
    for key in ("cpus", "seed", "batch_count", "batch_residues", "max_length", "E", "domE", "incE", "incdomE"):
        cmd.extend(["--"+key.replace("_", "-"), str(getattr(args, key))])
    if args.cutoffs:
        cmd += ["--cutoffs", args.cutoffs]
    return cmd


def summarize(runs, permitted):
    medians = {}
    for engine in ENGINES:
        subset = [r for r in runs if r["engine"] == engine]
        if not subset:
            continue
        medians[engine] = {
            key: statistics.median(r[key] for r in subset)
            for key in ("end_to_end_seconds", "cpu_seconds", "peak_rss_bytes")
        }
        medians[engine]["phases_seconds"] = {
            phase: statistics.median(r["phases_seconds"][phase] for r in subset)
            for phase in subset[0]["phases_seconds"]
        }
    ratios = None
    if permitted:
        ratios = {base: {
            "wall_speedup_over_hmmforge": medians[base]["end_to_end_seconds"] / medians["model-major"]["end_to_end_seconds"],
            "cpu_time_ratio_over_hmmforge": medians[base]["cpu_seconds"] / medians["model-major"]["cpu_seconds"],
        } for base in ("scan", "direct")}
    return medians, ratios


def environment():
    result = dict(platform=platform.platform(), processor=platform.processor(),
                  logical_cpus=os.cpu_count())
    if hasattr(os, "sched_getaffinity"):
        result["cpu_affinity"] = sorted(os.sched_getaffinity(0))
    for name in ("cpu.max", "memory.max"):
        path = Path("/sys/fs/cgroup")/name
        if path.exists():
            result["cgroup_"+name] = path.read_text().strip()
    return result


def run(args):
    options(args)
    if not 1 <= args.repeats <= 30 or args.timeout_seconds < 1:
        raise ValueError("repeats must be 1..30 and timeout must be positive")
    args.output_dir.mkdir(parents=True, exist_ok=False)
    runs, errors, mismatches = [], [], []
    reference, expected = None, None
    started = time.perf_counter()
    for repeat in range(args.repeats):
        for engine in order(repeat):
            stem = args.output_dir/f"{repeat:02d}-{engine}"
            output = stem.with_suffix(".jsonl")
            cmd = command_for(args, engine, output)
            t = time.perf_counter()
            with open(stem.with_suffix(".stderr.txt"), "w") as err, open(stem.with_suffix(".stdout.json"), "w") as out:
                try:
                    process = subprocess.run(cmd, stdout=out, stderr=err,
                                             timeout=args.timeout_seconds, check=False)
                except subprocess.TimeoutExpired:
                    errors.append(dict(repeat=repeat, engine=engine, reason="timeout", seconds=args.timeout_seconds, command=cmd))
                    break
            elapsed = time.perf_counter()-t
            if process.returncode:
                errors.append(dict(repeat=repeat, engine=engine, reason="nonzero exit", returncode=process.returncode, command=cmd))
                break
            report = json.loads(stem.with_suffix(".stdout.json").read_text())
            if expected is not None and expected != report["provenance"]:
                errors.append(dict(repeat=repeat, engine=engine, reason="input/source/configuration changed"))
                break
            expected = report["provenance"]
            report.update(repeat=repeat, end_to_end_seconds=elapsed, command=cmd)
            runs.append(report)
            if reference is None:
                reference = output  # First engine is always optimized scan.
            else:
                issues = compare_files(reference, output)
                if issues:
                    mismatches.append(dict(repeat=repeat, engine=engine, examples=issues))
        if errors:
            break
    complete = len(runs) == args.repeats*len(ENGINES) and not errors
    parity = complete and not mismatches
    medians, ratios = summarize(runs, parity)
    result = dict(schema="hmmforge.study.v1", dataset_kind=args.dataset_kind,
                  complete=complete, parity=parity, errors=errors, mismatches=mismatches,
                  runs=runs, medians=medians, ratios=ratios, engines=DESCRIPTIONS,
                  repeats=args.repeats, environment=environment(),
                  elapsed_seconds=time.perf_counter()-started,
                  cache_state="fresh processes; OS page cache uncontrolled; no annotation cache",
                  comparison="Direct baseline is authored here, not externally reviewed; shares HMMER kernels, preparation and input parsing, not extraction",
                  production_claim_permitted=False)
    with atomic_output(args.output_dir/"study.json") as handle:
        dump_json(result, handle)
    return result


def main(argv=None):
    args = parser().parse_args(argv)
    try:
        result = worker(args) if args.command == "_worker" else run(args)
        dump_json(result, sys.stdout)
        if args.command == "_worker":
            return 0
        return 0 if result["parity"] else (3 if result["complete"] else 2)
    except (ValueError, OSError, RuntimeError, MemoryError) as exc:
        dump_json(dict(schema="hmmforge.error.v1", error=type(exc).__name__, message=str(exc)), sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
