"""Compare reported targets/domains with an independently installed hmmscan.

This is NOT a native speed benchmark. The comparison respects native text
precision (two significant digits for E-values, one decimal for bit scores).
Inclusion flags and alignment strings are outside this script's coverage.
Inputs are copied to a temporary directory before hmmpress; source databases
are never modified. Requires hmmpress and hmmscan on PATH.
"""
import argparse
import json
import math
import shutil
import subprocess
import tempfile
from pathlib import Path

from hmmforge.core import Options, annotate_batch, load_models
from hmmforge.io import batches, read_fasta, sha256


def run(models_path: Path, proteins_path: Path, cpus=1, cutoffs=None):
    for command in ("hmmpress", "hmmscan"):
        if shutil.which(command) is None:
            raise RuntimeError(f"{command} is not installed")
    opts = Options(cpus=cpus, bit_cutoffs=cutoffs)
    models = load_models(models_path, opts)
    # This verification script is intentionally for small fixtures, not huge
    # production inputs. The annotation CLI itself streams its inputs.
    proteins = list(read_fasta(proteins_path))
    ours = []
    for batch in batches(proteins):
        ours.extend(annotate_batch(models, batch, opts))
    errors = []
    version = subprocess.run(["hmmscan", "-h"], capture_output=True, text=True, check=True).stdout.splitlines()[:3]
    with tempfile.TemporaryDirectory(prefix="hmmforge-native-") as folder:
        root = Path(folder)
        database, fasta = root/"models.hmm", root/"proteins.fa"
        shutil.copyfile(models_path, database)
        fasta.write_text("".join(f">{p.key}\n{p.sequence}\n" for p in proteins))
        subprocess.run(["hmmpress", str(database)], capture_output=True, text=True, check=True)
        subprocess.run(["hmmscan", "--cpu", str(cpus), "--seed", "42", "--noali",
                        "--tblout", str(root/"hits.tbl"), "--domtblout", str(root/"domains.tbl"),
                        *([{"gathering": "--cut_ga", "trusted": "--cut_tc", "noise": "--cut_nc"}[cutoffs]] if cutoffs else []),
                        str(database), str(fasta)], stdout=subprocess.DEVNULL,
                       stderr=subprocess.PIPE, text=True, check=True)
        def parse(path):
            with open(path) as handle:
                return [line.split() for line in handle if line.strip() and not line.startswith("#")]
        native_hits = {(r[2], r[0]):r for r in parse(root/"hits.tbl")}
        native_domains = {}
        for r in parse(root/"domains.tbl"):
            key = (r[3], r[0], *(int(r[i]) for i in (15,16,17,18,19,20)))
            if key in native_domains:
                errors.append(f"duplicate native domain key {key}")
            native_domains[key] = r
        our_hits, our_domains = {}, {}
        for row in ours:
            query = f"q{row['index']:012d}"
            for hit in row["hits"]:
                our_hits[(query, hit["model"])] = hit
                for dom in hit["domains"]:
                    key = (query, hit["model"], *(dom[k] for k in
                           ("hmm_from","hmm_to","ali_from","ali_to","env_from","env_to")))
                    if key in our_domains:
                        errors.append(f"duplicate candidate domain key {key}")
                    our_domains[key] = dom
        for label, a, b in (("targets", our_hits, native_hits), ("domains", our_domains, native_domains)):
            if a.keys() != b.keys():
                errors.append(f"{label}: {len(a.keys()-b.keys())} candidate-only, {len(b.keys()-a.keys())} native-only")
        def equal(value, rendered, precision, field):
            expected = float(format(value, precision))
            actual = float(rendered)
            if not math.isclose(expected, actual, rel_tol=1e-12, abs_tol=0):
                errors.append(f"{field}: candidate {expected} != native {actual}")
        for key in our_hits.keys() & native_hits.keys():
            a, b = our_hits[key], native_hits[key]
            for field, pos, precision in (("evalue",4,".2g"),("score",5,".1f"),("bias",6,".1f")):
                equal(a[field], b[pos], precision, f"{key}.{field}")
        for key in our_domains.keys() & native_domains.keys():
            a, b = our_domains[key], native_domains[key]
            for field, pos, precision in (("c_evalue",11,".2g"),("i_evalue",12,".2g"),("score",13,".1f"),("bias",14,".1f")):
                equal(a[field], b[pos], precision, f"{key}.{field}")
    return dict(schema="hmmforge.native-parity.v1", parity=not errors,
                native_version=version, cutoffs=cutoffs, models_sha256=sha256(models_path),
                proteins_sha256=sha256(proteins_path), proteins=len(proteins),
                reported_models=len(our_hits), reported_domains=len(our_domains),
                mismatches=len(errors), examples=errors[:20],
                excludes=["inclusion flags", "alignment strings", "full CLI compatibility"],
                numeric_precision="native text precision", production_validated=False)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("models", type=Path)
    parser.add_argument("proteins", type=Path)
    parser.add_argument("--cpus", type=int, default=1)
    parser.add_argument("--cutoffs", choices=("gathering", "trusted", "noise"))
    args = parser.parse_args()
    report = run(args.models, args.proteins, args.cpus, args.cutoffs)
    print(json.dumps(report, sort_keys=True))
    raise SystemExit(0 if report["parity"] else 3)
