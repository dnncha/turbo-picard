"""Numerical kernels belong to HMMER/PyHMMER; this module plans execution.

The experimental model-major path transposes hmmsearch output, keeping Z equal
 to the number of MODELS and restoring conditional domain E-values separately
for each protein. It must be validated on the user's workload before adoption.
"""
from __future__ import annotations

import math
from dataclasses import dataclass

from .io import Protein


@dataclass(frozen=True)
class Options:
    cpus: int = 1
    seed: int = 42
    E: float = 10.0
    domE: float = 10.0
    incE: float = 0.01
    incdomE: float = 0.01
    bit_cutoffs: str | None = None

    def __post_init__(self):
        if not isinstance(self.cpus, int) or self.cpus < 1:
            raise ValueError("cpus must be a positive integer")
        if not isinstance(self.seed, int) or not 0 < self.seed < 2**32:
            raise ValueError("seed must be in 1..2^32-1; random seed 0 is unsupported")
        for key in ("E", "domE", "incE", "incdomE"):
            value = getattr(self, key)
            if not math.isfinite(value) or value <= 0:
                raise ValueError(f"{key} must be positive and finite")
        if self.incE > self.E or self.incdomE > self.domE:
            raise ValueError("inclusion thresholds must not exceed reporting thresholds")
        if self.bit_cutoffs not in (None, "gathering", "trusted", "noise"):
            raise ValueError("unknown model-specific threshold")

    def pipeline(self, nmodels: int) -> dict:
        return dict(Z=nmodels, seed=self.seed, E=self.E, domE=self.domE,
                    incE=self.incE, incdomE=self.incdomE, bit_cutoffs=self.bit_cutoffs)


def backend():
    try:
        import pyhmmer
    except ImportError as exc:
        raise RuntimeError("install HMMForge with its pinned pyhmmer==0.12.3 dependency") from exc
    if pyhmmer.__version__ != "0.12.3":
        raise RuntimeError(f"validated backend pin is 0.12.3, found {pyhmmer.__version__}")
    return pyhmmer


def validate_models(models, options: Options):
    if not models:
        raise ValueError("HMM database contains no models")
    amino = backend().easel.Alphabet.amino()
    names = set()
    for model in models:
        if model.alphabet != amino:
            raise ValueError("only amino-acid HMMs are supported")
        if not model.name or model.name in names:
            raise ValueError(f"missing or duplicate model name: {model.name!r}")
        names.add(model.name)
        if options.bit_cutoffs and getattr(model.cutoffs, options.bit_cutoffs) is None:
            raise ValueError(f"model {model.name!r} lacks {options.bit_cutoffs} cutoffs")


def load_models(path, options: Options):
    with backend().plan7.HMMFile(path) as handle:
        models = list(handle)
    validate_models(models, options)
    return models


def _domain(domain, nmodels: int, domz: int, included: bool | None = None):
    alignment = domain.alignment
    return dict(score=domain.score, bias=domain.bias, pvalue=domain.pvalue,
                i_evalue=domain.pvalue * nmodels, c_evalue=domain.pvalue * domz,
                included=domain.included if included is None else included,
                hmm_from=alignment.hmm_from, hmm_to=alignment.hmm_to,
                ali_from=alignment.target_from, ali_to=alignment.target_to,
                env_from=domain.env_from, env_to=domain.env_to)


def _hit(hit, model, domains, evalue=None):
    return dict(model=model.name, accession=model.accession, model_length=model.M,
                score=hit.score, bias=hit.bias, pvalue=hit.pvalue,
                evalue=hit.evalue if evalue is None else evalue,
                included=hit.included, domains=domains)


def conditional_domain(pvalue: float, reported_models: int, options: Options,
                       hit_included: bool) -> tuple[bool, bool]:
    """E-value thresholding in SCAN orientation, not per-model SEARCH space."""
    if not math.isfinite(pvalue) or not 0 <= pvalue <= 1 or reported_models < 1:
        raise ValueError("invalid probability or reported-model count")
    ce = pvalue * reported_models
    return ce <= options.domE, hit_included and ce <= options.incdomE


def annotate_batch(models, proteins: list[Protein], options: Options,
                   engine: str = "model-major") -> list[dict]:
    """Return one row per protein, including no-hit proteins, in input order.

    Coordinates are 1-based and inclusive. A batch retains candidate domain
    objects until all profiles have run. Memory includes models, DP workspaces
    and candidate hits; the residue budget is NOT an RSS guarantee.
    """
    ph = backend()
    validate_models(models, options)
    if engine not in ("model-major", "scan"):
        raise ValueError(f"unsupported engine: {engine}")
    if not proteins:
        return []
    if len({p.index for p in proteins}) != len(proteins):
        raise ValueError("protein ordinals must be unique within a batch")
    amino = ph.easel.Alphabet.amino()
    seqs = ph.easel.DigitalSequenceBlock(amino, [
        ph.easel.TextSequence(name=p.key, sequence=p.sequence).digitize(amino)
        for p in proteins])
    rows = [dict(schema="hmmforge.annotations.v1", index=p.index, name=p.name,
                 description=p.description, length=len(p.sequence), hits=[]) for p in proteins]
    nmodels = len(models)
    kwargs = options.pipeline(nmodels)
    if engine == "scan":
        background = ph.plan7.Background(amino)
        profiles = ph.plan7.OptimizedProfileBlock(amino, [
            model.to_profile(background).to_optimized() for model in models])
        lookup = {model.name: model for model in models}
        for row, hits in zip(rows, ph.hmmer.hmmscan(seqs, profiles, cpus=options.cpus,
                                                   **kwargs), strict=True):
            reported = list(hits.reported)
            for hit in reported:
                domains = []
                for dom in hit.domains:
                    if dom.reported:
                        item = _domain(dom, nmodels, len(reported))
                        # Reference values come directly from the upstream scan.
                        item.update(i_evalue=dom.i_evalue, c_evalue=dom.c_evalue)
                        domains.append(item)
                row["hits"].append(_hit(hit, lookup[hit.name], domains))
    else:
        index = {p.key: i for i, p in enumerate(proteins)}
        candidates = [[] for _ in proteins]
        # Broad domain flags preserve HMMER's duplicate-alignment suppression.
        # Never resurrect a suppressed domain by blindly thresholding pvalues.
        kwargs.update(domZ=1, domE=1.0, incdomE=1.0)
        results = ph.hmmer.hmmsearch(models, seqs, cpus=options.cpus,
                                     parallel="queries", **kwargs)
        for model, hits in zip(models, results, strict=True):
            for hit in hits.reported:
                candidates[index[hit.name]].append((model, hit))
        for row, found in zip(rows, candidates, strict=True):
            domz = len(found)
            for model, hit in found:
                domains = []
                for domain in hit.domains:
                    if not domain.reported:
                        continue
                    reported, included = (True, domain.included) if options.bit_cutoffs else (
                        conditional_domain(domain.pvalue, domz, options, hit.included))
                    if reported:
                        domains.append(_domain(domain, nmodels, domz, included))
                row["hits"].append(_hit(hit, model, domains, hit.pvalue * nmodels))
    for row in rows:
        row["hits"].sort(key=lambda hit: hit["model"])
        for hit in row["hits"]:
            hit["domains"].sort(key=lambda d: (d["ali_from"], d["ali_to"], d["hmm_from"], d["hmm_to"]))
    return rows


def differences(left, right, path="root", limit=20) -> list[str]:
    """Structural parity with strict relative error for even tiny E-values.

    Scores allow 1e-5 absolute / 1e-6 relative error; probabilities and E-values
    allow 1e-6 relative error and ZERO absolute floor. This is not bitwise parity.
    """
    problems = []
    def walk(a, b, key):
        if len(problems) >= limit:
            return
        if isinstance(a, dict) and isinstance(b, dict):
            if a.keys() != b.keys():
                problems.append(f"{key}: different fields")
            for field in sorted(a.keys() & b.keys()):
                walk(a[field], b[field], f"{key}.{field}")
        elif isinstance(a, list) and isinstance(b, list):
            if len(a) != len(b):
                problems.append(f"{key}: lengths {len(a)} != {len(b)}")
            for i, (x, y) in enumerate(zip(a, b)):
                walk(x, y, f"{key}[{i}]")
        elif isinstance(a, float) and isinstance(b, (float, int)) and not isinstance(b, bool):
            absolute = 0.0 if key.endswith(("evalue", "pvalue")) else 1e-5
            if not (math.isfinite(a) and math.isfinite(b) and
                    math.isclose(a, b, rel_tol=1e-6, abs_tol=absolute)):
                problems.append(f"{key}: {a!r} != {b!r}")
        elif type(a) is not type(b) or a != b:
            problems.append(f"{key}: {a!r} != {b!r}")
    walk(left, right, path)
    return problems
