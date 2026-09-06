import gzip
import importlib.util
import io
import json
from pathlib import Path

import pytest

from hmmforge import ModelDatabase, Options, annotate_batch
from hmmforge.baseline import direct_search
from hmmforge.core import differences
from hmmforge.study import ENGINES, main, order, summarize
from test_integration import fixture  # shared synthetic scientific fixture


def script(name):
    path = Path(__file__).resolve().parents[1]/"scripts"/f"{name}.py"
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


@pytest.mark.parametrize("cpus", [1, 2])
@pytest.mark.parametrize("cutoffs", [None, "gathering", "trusted", "noise"])
def test_direct_independent_extraction_matches_scan(fixture, cpus, cutoffs):
    models, proteins = fixture
    opts = Options(cpus=cpus, bit_cutoffs=cutoffs)
    db = ModelDatabase(models, opts)
    reference = annotate_batch(db, proteins, opts, "scan")
    actual = direct_search(db, proteins, opts)
    assert not differences(reference, actual), differences(reference, actual)


@pytest.mark.parametrize("domE", [0.0001, 0.01, 0.1])
def test_direct_conditional_thresholds(fixture, domE):
    models, proteins = fixture
    opts = Options(E=1., incE=0.001, domE=domE, incdomE=domE/10)
    db = ModelDatabase(models, opts)
    assert not differences(annotate_batch(db, proteins, opts, "scan"), direct_search(db, proteins, opts))


def test_balanced_run_order():
    for group in (0, 3):
        for position in range(3):
            assert {order(i)[position] for i in range(group, group+3)} == set(ENGINES)


def test_incomplete_study_cannot_advertise_speedup():
    assert summarize([], False) == ({}, None)


def test_study_subprocess_evidence(fixture, tmp_path, capsys):
    models, proteins = fixture
    hmm, fasta = tmp_path/"models.hmm", tmp_path/"proteins.fa"
    with open(hmm, "wb") as handle:
        for model in models[:2]:
            model.write(handle)
    fasta.write_text("".join(f">{p.name}\n{p.sequence}\n" for p in proteins[:7]))
    dest = tmp_path/"study"
    args = ["run", str(hmm), str(fasta), "--output-dir", str(dest),
            "--repeats", "1", "--dataset-kind", "synthetic", "--batch-count", "3"]
    assert main(args) == 0
    result = json.loads(capsys.readouterr().out)
    assert result["complete"] and result["parity"]
    assert len(result["runs"]) == 3
    assert result["production_claim_permitted"] is False
    assert result["runs"][1]["memory_strategy"] == "fully-resident"
    assert result["runs"][1]["batches"] == 1
    assert result["runs"][2]["batches"] == 3
    assert all(r["provenance"]["package_source_sha256"] for r in result["runs"])
    assert all(r["phases_seconds"]["model_load_and_prepare"] > 0 for r in result["runs"])
    assert (dest/"study.json").is_file()
    assert main(args) == 2  # Refuse to clobber prior evidence.


def test_failed_worker_retains_report(tmp_path, capsys):
    result = main(["run", str(tmp_path/"missing.hmm"), str(tmp_path/"missing.fa"),
                   "--output-dir", str(tmp_path/"study"), "--dataset-kind", "synthetic", "--repeats", "1"])
    assert result == 2
    data = json.loads(capsys.readouterr().out)
    assert data["errors"] and not data["complete"]
    assert data["ratios"] is None
    assert (tmp_path/"study/study.json").exists()


@pytest.mark.parametrize("release", ["current_release", "../38.0", "latest", "38", "38.0/extra"])
def test_unpinned_catalogues_rejected(release):
    with pytest.raises(ValueError):
        script("prepare_catalogue").validate_release(release)


def test_catalogue_hash_mismatch_and_success(tmp_path, monkeypatch):
    module = script("prepare_catalogue")
    raw = b"HMMER3/f\nNAME  Example\n//\n"
    payload = gzip.compress(raw)
    class Response(io.BytesIO):
        def geturl(self):
            return "https://ftp.ebi.ac.uk/pub/databases/Pfam/releases/Pfam38.0/Pfam-A.hmm.gz"
    monkeypatch.setattr(module.urllib.request, "urlopen", lambda *a, **k: Response(payload))
    with pytest.raises(ValueError, match="SHA256"):
        module.acquire(tmp_path/"wrong", expected="0"*64)
    assert not (tmp_path/"wrong").exists()
    data = module.acquire(tmp_path/"right")
    assert data["models"] == 1 and not data["expected_sha256_verified"]
    assert (tmp_path/"right/models.hmm").read_bytes() == raw
    with pytest.raises(FileExistsError):
        module.acquire(tmp_path/"right")


def test_sample_is_repeatable_and_preserves_order(tmp_path):
    module = script("sample_fasta")
    source = tmp_path/"source.fa"
    source.write_text("".join(f">record{i}\nACDEFG\n" for i in range(25)))
    a = module.sample(source, tmp_path/"a.fa", 7)
    b = module.sample(source, tmp_path/"b.fa", 7)
    assert a == b
    assert a["source_ordinals"] == sorted(a["source_ordinals"])
    assert a["selected"] == 7


def test_native_profile_unavailable_is_not_success(tmp_path):
    data = script("native_profile").capture(tmp_path/"profile", ["true"], perf="/no-such-perf")
    assert not data["valid_native_profile"]
    assert data["status"] == "failed_or_denied"
