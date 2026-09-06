"""Direct, fully resident PyHMMER baseline, not an external expert endorsement.

Uses upstream model-major execution without HMMForge's batch executor or result
extraction helpers. Model preparation and strict input parsing are shared. All
proteins and compact results are resident; this is explicitly not memory-bounded.
Both engines use unchanged HMMER kernels. Credit: PyHMMER performance recipes.
"""
from __future__ import annotations


def direct_search(database, proteins, options):
    import pyhmmer as ph

    alphabet = ph.easel.Alphabet.amino()
    targets = ph.easel.DigitalSequenceBlock(alphabet, [
        ph.easel.TextSequence(name=p.key, sequence=p.sequence).digitize(alphabet)
        for p in proteins
    ])
    rows = [dict(schema="hmmforge.annotations.v1", index=p.index, name=p.name,
                 description=p.description, length=len(p.sequence), hits=[])
            for p in proteins]
    positions = {p.key: i for i, p in enumerate(proteins)}
    nmodels = len(database)
    results = ph.hmmer.hmmsearch(
        database.profiles, targets, cpus=options.cpus, parallel="queries",
        Z=nmodels, domZ=1, seed=options.seed, E=options.E, incE=options.incE,
        domE=1.0, incdomE=1.0, bit_cutoffs=options.bit_cutoffs,
    )
    for model, result in zip(database.models, results, strict=True):
        for hit in result.reported:
            domains = []
            for d in hit.domains:
                if not d.reported:
                    continue  # Retain upstream duplicate-domain suppression.
                a = d.alignment
                domains.append(dict(
                    score=d.score, bias=d.bias, pvalue=d.pvalue,
                    i_evalue=d.pvalue*nmodels, c_evalue=d.pvalue,
                    included=d.included, hmm_from=a.hmm_from, hmm_to=a.hmm_to,
                    ali_from=a.target_from, ali_to=a.target_to,
                    env_from=d.env_from, env_to=d.env_to,
                ))
            rows[positions[hit.name]]["hits"].append(dict(
                model=model.name, accession=model.accession, model_length=model.M,
                score=hit.score, bias=hit.bias, pvalue=hit.pvalue,
                evalue=hit.pvalue*nmodels, included=hit.included, domains=domains,
            ))
    # In scan orientation, conditional search space is per protein, not model.
    for row in rows:
        reported_models = len(row["hits"])
        for hit in row["hits"]:
            retained = []
            for d in hit["domains"]:
                d["c_evalue"] = d["pvalue"] * reported_models
                if options.bit_cutoffs is None:
                    if d["c_evalue"] > options.domE:
                        continue
                    d["included"] = hit["included"] and d["c_evalue"] <= options.incdomE
                retained.append(d)
            hit["domains"] = sorted(retained, key=lambda d: (
                d["ali_from"], d["ali_to"], d["hmm_from"], d["hmm_to"]))
        row["hits"].sort(key=lambda h: h["model"])
    return rows
