"""Generate reproducible SYNTHETIC workloads, never used as production proof."""
import argparse
import datetime
import random
from pathlib import Path
import pyhmmer as ph

p = argparse.ArgumentParser()
p.add_argument("output", type=Path)
p.add_argument("--models", type=int, default=64)
p.add_argument("--proteins", type=int, default=2000)
args = p.parse_args()
if min(args.models, args.proteins) < 1:
    p.error("counts must be positive")
args.output.mkdir(parents=True, exist_ok=False)
rng = random.Random(71839)
amino = ph.easel.Alphabet.amino()
background = ph.plan7.Background(amino)
builder = ph.plan7.Builder(amino, seed=42)
alphabet = "ACDEFGHIKLMNPQRSTVWY"
seeds = []
with open(args.output / "models.hmm", "wb") as handle:
    for i in range(args.models):
        sequence = "".join(rng.choices(alphabet, k=rng.randint(80, 400)))
        seeds.append(sequence)
        model, _, _ = builder.build(ph.easel.TextSequence(name=f"synthetic_{i}", sequence=sequence).digitize(amino), background)
        model.creation_time = datetime.datetime(2026, 1, 1)
        model.command_line = "hmmforge synthetic fixture; sequence seed=71839; builder seed=42"
        model.write(handle)
with open(args.output / "proteins.fa", "w") as handle:
    for i in range(args.proteins):
        if i % 3 == 0:
            seq = "".join(rng.choices(alphabet, k=rng.randint(80,600)))
        else:
            seed = rng.choice(seeds)
            seq = "".join(c if rng.random() > .2 else rng.choice(alphabet) for c in seed)
            if i % 7 == 0:
                seq += "GGGGSGGGGS" + rng.choice(seeds)
        handle.write(f">synthetic_{i}\n{seq}\n")
