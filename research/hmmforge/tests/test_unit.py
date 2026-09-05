import gzip
import json
import math

import pytest

from hmmforge.core import Options, conditional_domain, differences
from hmmforge.io import Protein, atomic_output, batches, dump_json, read_fasta
from hmmforge.__main__ import main


@pytest.mark.parametrize("key,value", [("cpus",0), ("seed",0), ("seed",2**32), ("E",float("nan")), ("domE",float("inf")), ("incE",-1), ("incE",20), ("incdomE",20), ("bit_cutoffs","unknown")])
def test_invalid_options(key, value):
    with pytest.raises(ValueError):
        Options(**{key:value})


def test_conditional_scan_space():
    opts = Options(domE=.05, incdomE=.01)
    assert conditional_domain(.02, 1, opts, True) == (True, False)
    assert conditional_domain(.02, 3, opts, True) == (False, False)
    assert conditional_domain(.001, 3, opts, False) == (True, False)
    assert conditional_domain(.001, 3, opts, True) == (True, True)


def test_equality_never_masks_tiny_evalue_mismatch():
    assert differences({"evalue":1e-80}, {"evalue":1e-50})
    assert differences({"score":math.nan}, {"score":math.nan})
    assert differences({"included":True}, {"included":1})
    assert not differences({"score":2.0}, {"score":2.000001})


def test_fasta_duplicates_gzip_and_batches(tmp_path):
    path = tmp_path / "test.fa.gz"
    with gzip.open(path, "wt") as out:
        out.write(">same first\nacde\n>same second\nGG H\n>third\nK\n")
    records = list(read_fasta(path))
    assert [r.name for r in records] == ["same", "same", "third"]
    assert [r.sequence for r in records] == ["ACDE", "GGH", "K"]
    assert len({r.key for r in records}) == 3
    assert [len(b) for b in batches(records, 5, 99)] == [1, 2]
    assert [len(b) for b in batches(records, 1, 99)] == [1, 1, 1]


@pytest.mark.parametrize("text", ["", "ACDE\n", ">\nACD", ">a\n>b\nACD", ">a\nAC*D", ">a\nAC-D", ">a\nAC12", ">a\n"])
def test_bad_fasta(text, tmp_path):
    path = tmp_path / "bad.fa"
    path.write_text(text)
    with pytest.raises(ValueError):
        list(read_fasta(path))


def test_length_limit(tmp_path):
    path = tmp_path / "a.fa"
    path.write_text(">a\nACDE\n")
    with pytest.raises(ValueError):
        list(read_fasta(path, max_length=3))


def test_atomic_no_clobber_and_failure(tmp_path):
    out = tmp_path / "out.json"
    with pytest.raises(RuntimeError):
        with atomic_output(out) as handle:
            handle.write("partial")
            raise RuntimeError("failed")
    assert not out.exists()
    with atomic_output(out) as handle:
        dump_json({"complete":True}, handle)
    with pytest.raises(FileExistsError):
        with atomic_output(out):
            pass
    assert json.loads(out.read_text()) == {"complete":True}
    assert len(list(tmp_path.iterdir())) == 1


def test_atomic_concurrent_creator(tmp_path):
    out = tmp_path / "raced.json"
    with pytest.raises(FileExistsError):
        with atomic_output(out) as handle:
            handle.write("mine")
            out.write_text("other")
    assert out.read_text() == "other"


def test_capabilities(capsys):
    assert main(["capabilities"]) == 0
    report = json.loads(capsys.readouterr().out)
    assert report["production_validated"] is False
