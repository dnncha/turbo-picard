"""Synthetic algorithmic regressions, not biological production validation."""
import json
import random

import pytest

ph = pytest.importorskip("pyhmmer")
from hmmforge.core import Options, annotate_batch, differences, validate_models
from hmmforge.io import Protein
from hmmforge.__main__ import main


@pytest.fixture(scope="module")
def fixture():
    rng = random.Random(731)
    amino = ph.easel.Alphabet.amino()
    background = ph.plan7.Background(amino)
    builder = ph.plan7.Builder(amino, seed=42)
    models, proteins = [], []
    for i, length in enumerate((70, 100, 160, 240, 310, 400)):
        sequence = "".join(rng.choices("ACDEFGHIKLMNPQRSTVWY", k=length))
        hmm, _, _ = builder.build(ph.easel.TextSequence(name=f"family{i}", sequence=sequence).digitize(amino), background)
        hmm.cutoffs.gathering = (20., 15.)
        hmm.cutoffs.trusted = (25., 20.)
        hmm.cutoffs.noise = (10., 10.)
        models.append(hmm)
        variants = [sequence, sequence + "GGGGSGGGGS" + sequence,
                    "".join(c if rng.random() > .3 else rng.choice("ACDEFGHIKLMNPQRSTVWY") for c in sequence),
                    sequence[:length//2], sequence[::-1]]
        for variant in variants:
            proteins.append(Protein(len(proteins), "duplicate-name", "synthetic", variant))
    # Similar profiles create >1 reported model per protein: catches wrong domZ.
    clone = models[0].copy()
    clone.name = "family0-related"
    models.append(clone)
    proteins.extend([Protein(len(proteins), "nohit", "synthetic", "A"*180),
                     Protein(len(proteins)+1, "ambiguous", "synthetic", "XXBXZXJXUOX"*10)])
    return models, proteins


@pytest.mark.parametrize("cpus", [1,2])
@pytest.mark.parametrize("cutoffs", [None, "gathering", "trusted", "noise"])
def test_scan_parity(fixture, cpus, cutoffs):
    models, proteins = fixture
    opts = Options(cpus=cpus, bit_cutoffs=cutoffs)
    reference = annotate_batch(models, proteins, opts, "scan")
    candidate = annotate_batch(models, proteins, opts, "model-major")
    assert not differences(reference, candidate), differences(reference, candidate)
    assert any(len(r["hits"]) > 1 for r in candidate)
    assert any(len(h["domains"]) > 1 for r in candidate for h in r["hits"])
    assert any(not r["hits"] for r in candidate)


@pytest.mark.parametrize("batch_size", [1, 7, 32])
def test_batch_invariance(fixture, batch_size):
    models, proteins = fixture
    reference = annotate_batch(models, proteins, Options(), "scan")
    candidate = []
    for start in range(0, len(proteins), batch_size):
        candidate.extend(annotate_batch(models, proteins[start:start+batch_size], Options(cpus=2)))
    assert not differences(reference, candidate), differences(reference, candidate)


@pytest.mark.parametrize("domE", [.0001, .01, .1])
def test_domain_thresholds(fixture, domE):
    models, proteins = fixture
    opts = Options(E=1, incE=.001, domE=domE, incdomE=domE/10)
    a = annotate_batch(models, proteins, opts, "scan")
    b = annotate_batch(models, proteins, opts, "model-major")
    assert not differences(a,b), differences(a,b)


def test_cli_verify_and_no_clobber(fixture, tmp_path, capsys):
    models, proteins = fixture
    hmm = tmp_path / "models.hmm"
    fasta = tmp_path / "proteins.fa"
    out = tmp_path / "annotations.jsonl"
    with open(hmm,"wb") as handle:
        for model in models:
            model.write(handle)
    fasta.write_text("".join(f">{p.name} {p.description}\n{p.sequence}\n" for p in proteins))
    assert main(["verify", str(hmm), str(fasta), "--batch-count", "7"]) == 0
    assert json.loads(capsys.readouterr().out)["parity"]
    args = ["annotate",str(hmm),str(fasta),"--output",str(out)]
    assert main(args) == 0
    capsys.readouterr()
    assert len(out.read_text().splitlines()) == len(proteins)
    assert main(args) == 2
    assert "already exists" in capsys.readouterr().err


def test_models_rejected(fixture):
    models, _ = fixture
    with pytest.raises(ValueError):
        validate_models([], Options())
    with pytest.raises(ValueError):
        validate_models([models[0], models[0]], Options())
    model = models[0].copy()
    model.cutoffs.gathering = None
    with pytest.raises(ValueError):
        validate_models([model], Options(bit_cutoffs="gathering"))
