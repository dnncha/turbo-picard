"""Export a SMALL biological test workload bundled with pinned PyHMMER.

Not a full Pfam catalogue or metagenomic production benchmark. No network is
used. Third-party data retain their upstream provenance and licensing.
"""
import argparse
import json
from pathlib import Path
import pyhmmer as ph
from hmmforge.io import sha256

p = argparse.ArgumentParser()
p.add_argument("output", type=Path)
args = p.parse_args()
if ph.__version__ != "0.12.3":
    p.error("this fixture requires pyhmmer==0.12.3")
root = Path(ph.__file__).parent / "tests" / "data"
files = [root / "hmms/txt" / name for name in
         ("RREFam.hmm", "PF02826.hmm", "LuxC.hmm", "KR.hmm", "Thioesterase.hmm")]
seq = root / "seqs/938293.PRJEB85.HG003687.faa"
for path in files + [seq]:
    if not path.is_file():
        p.error(f"installed backend does not bundle required fixture: {path}")
args.output.mkdir(parents=True, exist_ok=False)
with open(args.output / "models.hmm", "wb") as out:
    for path in files:
        with ph.plan7.HMMFile(path) as handle:
            for model in handle:
                model.write(out)
# Explicit fixture preparation, NOT silent behavior in the annotation CLI.
# This source includes terminal translation-stop markers. Both compared engines
# receive exactly the same stop-free amino-acid sequences.
stripped = 0
with open(args.output / "proteins.fa", "w") as out:
    for record in seq.read_text().split(">")[1:]:
        lines = record.splitlines()
        sequence = "".join(lines[1:])
        if "*" in sequence[:-1]:
            raise ValueError("fixture contains internal stops; refusing to repair")
        if sequence.endswith("*"):
            sequence = sequence[:-1]
            stripped += 1
        out.write(f">{lines[0]}\n{sequence}\n")
manifest = dict(kind="small-biological-test", backend="pyhmmer==0.12.3",
                description="14 protein profiles against the bundled Anaerococcus proteome; not production scale",
                upstream_commit="956c542559a077d4ecfe8904c887331c621f988c",
                sources={str(path.relative_to(root)):sha256(path) for path in files+[seq]},
                preprocessing={"terminal_stop_markers_removed": stripped,
                               "internal_stops_removed": 0},
                models_sha256=sha256(args.output/"models.hmm"),
                proteins_sha256=sha256(args.output/"proteins.fa"))
(args.output/"provenance.json").write_text(json.dumps(manifest, indent=2)+"\n")
