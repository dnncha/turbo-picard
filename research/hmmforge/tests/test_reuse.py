import pytest
from hmmforge.io import read_fasta


def test_non_ascii_is_not_casefolded_into_amino_acids(tmp_path):
    path = tmp_path / "unicode.fa"
    path.write_text(">a\nACßD\n")
    with pytest.raises(ValueError):
        list(read_fasta(path))


def test_prepared_database_reused_across_orientations():
    ph = pytest.importorskip("pyhmmer")
    from hmmforge import ModelDatabase, Options, annotate_batch
    from hmmforge.core import differences
    from hmmforge.io import Protein
    amino = ph.easel.Alphabet.amino()
    sequence = "ACDEFGHIKLMNPQRSTVWY" * 5
    model, _, _ = ph.plan7.Builder(amino, seed=42).build(
        ph.easel.TextSequence(name="test", sequence=sequence).digitize(amino),
        ph.plan7.Background(amino))
    proteins = [Protein(0, "test", "synthetic", sequence)]
    db = ModelDatabase([model], Options())
    expected = annotate_batch([model], proteins, Options(), "scan")
    for engine in ("model-major", "scan", "model-major"):
        actual = annotate_batch(db, proteins, Options(cpus=2), engine)
        assert not differences(expected, actual), differences(expected, actual)
