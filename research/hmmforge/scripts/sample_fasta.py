"""Deterministic hash-priority subsample, returned in original order.

This is a computational test subset, not a claim of biological representativeness.
FASTA records are selected independently of their annotation results.
"""
import argparse
import hashlib
import heapq
import json
from pathlib import Path
from hmmforge.io import atomic_output, read_fasta, sha256


def sample(source, output, count, seed=731):
    if count < 1:
        raise ValueError("count must be positive")
    def priority(protein):
        return hashlib.sha256(f"{seed}:{protein.index}:{protein.name}:{protein.sequence}".encode()).digest()
    selected = heapq.nsmallest(count, read_fasta(source), key=priority)
    selected.sort(key=lambda p: p.index)
    with atomic_output(output) as handle:
        for p in selected:
            handle.write(f">{p.name} {p.description}\n{p.sequence}\n")
    return dict(schema="hmmforge.sample.v1", source_sha256=sha256(source),
                output_sha256=sha256(output), requested=count, selected=len(selected),
                residues=sum(len(p.sequence) for p in selected), seed=seed,
                source_ordinals=[p.index for p in selected],
                strategy="smallest SHA256 priorities, then source order",
                representative_metagenomic_corpus=False)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--count", type=int, required=True)
    args = parser.parse_args()
    print(json.dumps(sample(args.source, args.output, args.count), sort_keys=True))
