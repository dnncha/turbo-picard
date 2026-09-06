#!/usr/bin/env python3
"""Independent public-API probes. Synthetic data; no patient or biological claims."""
from __future__ import annotations
import argparse, hashlib, importlib, importlib.metadata, inspect, itertools, json, os, platform, shutil, subprocess, sys, traceback
from io import StringIO
from pathlib import Path

OUT = Path(os.environ.get('AUDIT_OUTPUT', 'round2-evidence')).resolve()
OUT.mkdir(parents=True, exist_ok=True)
REPORT = {'python': sys.version, 'platform': platform.platform(), 'run_id': os.environ.get('GITHUB_RUN_ID'),
          'harness_sha': os.environ.get('GITHUB_SHA'), 'data': 'deterministic synthetic fixtures; no patient data', 'cases': {}}


def save_json():
    (OUT / 'results.json').write_text(json.dumps(REPORT, indent=2, default=str) + '\n')


def run_case(name, fn):
    try:
        REPORT['cases'][name] = {'execution': 'completed', **fn()}
    except Exception as exc:
        REPORT['cases'][name] = {'execution': 'error', 'error': repr(exc), 'traceback': traceback.format_exc()}
    print(name, json.dumps(REPORT['cases'][name], indent=2, default=str), flush=True)
    save_json()


def snapshot(module_name):
    module = importlib.import_module(module_name)
    path = Path(inspect.getfile(module))
    data = path.read_bytes()
    target = OUT / 'installed-sources' / (module_name.replace('.', '/') + '.py')
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_bytes(data)
    return {'module': module_name, 'source_sha256': hashlib.sha256(data).hexdigest()}


def consensus():
    import Bio
    from Bio import Phylo
    from Bio.Phylo import Consensus
    strings = ['((A:1,B:1):1,(C:1,D:1):1);', '((A:1,C:1):1,(B:1,D:1):1);', '((A:1,D:1):1,(B:1,C:1):1);']
    def trees(ss): return [Phylo.read(StringIO(s), 'newick') for s in ss]
    def nontrivial(t):
        return [{'taxa': sorted(x.name for x in c.get_terminals()), 'confidence': c.confidence}
                for c in t.get_nonterminals() if 1 < len(c.get_terminals()) < 4]
    inputs = trees(strings)
    occurrence = {}
    for t in inputs:
        for c in nontrivial(t):
            key = ','.join(c['taxa']); occurrence[key] = occurrence.get(key, 0) + 1
    majority = Consensus.majority_consensus(trees(strings), cutoff=1.0)
    strict = Consensus.strict_consensus(trees(strings))
    permutation_results = []
    for ss in itertools.permutations(strings):
        permutation_results.append({'first_tree': ss[0], 'clades': nontrivial(Consensus.strict_consensus(trees(ss)))})
    Phylo.write(majority, str(OUT / 'biopython-majority-observed.nwk'), 'newick')
    Phylo.write(strict, str(OUT / 'biopython-strict-observed.nwk'), 'newick')
    return {'version': Bio.__version__, 'source': snapshot('Bio.Phylo.Consensus'), 'input_newick': strings,
            'actual_input_clade_occurrences': occurrence, 'trees': 3,
            'expected_at_unanimous_cutoff': [], 'majority_cutoff_1_observed': nontrivial(majority),
            'strict_observed': nontrivial(strict), 'input_permutation_results': permutation_results,
            'reproduced': bool(nontrivial(majority) and nontrivial(strict))}


def skipped_distance():
    import Bio
    from Bio.Align import MultipleSeqAlignment
    from Bio.Seq import Seq
    from Bio.SeqRecord import SeqRecord
    from Bio.Phylo.TreeConstruction import DistanceCalculator, DistanceTreeConstructor, DistanceMatrix
    seqs = ['ACGT', 'ACGTNNNN', 'ACGT--------']
    calc = DistanceCalculator('identity', skip_letters=('N', '-'))
    examples = []
    for seq in seqs:
        aln = MultipleSeqAlignment([SeqRecord(Seq(seq), id='a'), SeqRecord(Seq(seq), id='b')])
        examples.append({'sequence_pair': [seq, seq], 'observed': calc.get_distance(aln)['a', 'b'], 'expected': 0.0})
    # Independent column-deletion oracle over an exhaustive short-alignment domain.
    failures, total = 0, 0
    words = list(map(''.join, itertools.product('ACN', repeat=3)))
    for a, b in itertools.product(words, repeat=2):
        usable = [(x,y) for x,y in zip(a,b) if x != 'N' and y != 'N']
        if not usable: continue  # all-missing policy is deliberately not judged
        expected = sum(x != y for x,y in usable) / len(usable)
        observed = calc._pairwise(a,b)
        failures += abs(observed - expected) > 1e-12
        total += 1
    return {'version': Bio.__version__, 'source': snapshot('Bio.Phylo.TreeConstruction'), 'examples': examples,
            'exhaustive_nonempty_column_pairs': total, 'oracle_disagreements': failures,
            'reproduced': examples[1]['observed'] != 0}


def gseapy_fdr():
    import gseapy as gp
    import numpy as np
    from scipy.stats import hypergeom
    from statsmodels.stats.multitest import multipletests
    universe = [f'G{i:04d}' for i in range(1000)]
    library = {f'P{i:03d}': universe[10*i:10*(i+1)] for i in range(100)}
    def enrich(query, lib=library):
        obj = gp.enrichr(gene_list=query, gene_sets=lib, background=universe, outdir=None, no_plot=True, verbose=False)
        return obj.results
    query = [universe[i] for i in [0, 10, 20, 30, 40]]
    observed = enrich(query)
    ps = [float(hypergeom.sf(len(set(query)&set(gs))-1, 1000, len(gs), len(query))) for gs in library.values()]
    expected = multipletests(ps, method='fdr_bh')[1]
    expected_map = dict(zip(library, expected.tolist()))
    records = observed[['Term', 'P-value', 'Adjusted P-value', 'Overlap']].to_dict(orient='records')
    for r in records: r['expected_full_family_adjusted_p'] = expected_map[r['Term']]
    # Repeated complete-null experiment: fixed disjoint sets, uniformly sampled query.
    rng = np.random.default_rng(20260906)
    native_rejections = oracle_rejections = 0
    trials = 100
    for _ in range(trials):
        q = rng.choice(universe, 5, replace=False).tolist()
        frame = enrich(q)
        native_rejections += bool((frame['Adjusted P-value'] < 0.05).any())
        p = [hypergeom.sf(len(set(q)&set(gs))-1, 1000, 10, 5) for gs in library.values()]
        oracle_rejections += bool(multipletests(p, method='fdr_bh')[0].any())
    small_lib = {'TARGET': universe[:2] + universe[10:18]}
    small_lib.update({f'NULL{i:03d}': universe[10+10*i:20+10*i] for i in range(99)})
    second = enrich(universe[:5], small_lib)
    return {'version': importlib.metadata.version('gseapy'), 'source': [snapshot('gseapy.stats'), snapshot('gseapy.enrichr')],
            'family_size': 100, 'returned_terms': len(observed), 'query': query, 'rows': records,
            'at_least_one_hit_p': float(hypergeom.sf(0,1000,10,5)),
            'complete_null_experiment': {'seed': 20260906, 'trials': trials, 'genes':1000, 'fixed_disjoint_sets':100,
                'genes_per_set':10, 'uniform_query_size':5, 'alpha':0.05,
                'native_runs_with_any_discovery':native_rejections, 'full_family_runs_with_any_discovery':oracle_rejections},
            'single_hit_term_example': second[['Term','P-value','Adjusted P-value','Overlap']].to_dict(orient='records'),
            'reproduced': all(abs(r['Adjusted P-value']-r['expected_full_family_adjusted_p']) > 1e-6 for r in records)}


def scanpy_logreg():
    import numpy as np
    import pandas as pd
    import scanpy as sc
    from anndata import AnnData
    from scipy.sparse import csr_matrix
    from sklearn.linear_model import LogisticRegression
    rng = np.random.default_rng(19)
    raw = np.vstack([np.tile([10.,0.,1.],(12,1)), np.tile([0.,10.,1.],(12,1))])
    raw[:,2] += rng.uniform(0,0.1,24)
    x = np.log1p(raw)
    results = []
    for sparse in [False, True]:
      for categories in [['A','B'], ['B','A']]:
       for extra in [{}, {'groups':['A'], 'reference':'B'}, {'groups':['B'], 'reference':'A'}]:
        adata = AnnData(csr_matrix(x) if sparse else x.copy(),
            obs=pd.DataFrame({'group':pd.Categorical(['A']*12+['B']*12,categories=categories)},index=[f'c{i}' for i in range(24)]),
            var=pd.DataFrame(index=['marker_A','marker_B','neutral']))
        sc.tl.rank_genes_groups(adata, groupby='group', method='logreg', use_raw=False, n_genes=3, **extra)
        rg = adata.uns['rank_genes_groups']
        rows = {}
        for name in rg['names'].dtype.names:
            rows[name] = {'genes':rg['names'][name].tolist(), 'scores':rg['scores'][name].tolist(),
                          'expected_top_marker':'marker_'+name}
        results.append({'sparse':sparse,'category_order':categories,'arguments':extra,'returned_groups':rows})
    bad = [(r['category_order'],name) for r in results for name,val in r['returned_groups'].items() if val['genes'][0] != val['expected_top_marker']]
    return {'version':importlib.metadata.version('scanpy'),'source':snapshot('scanpy.tools._rank_genes_groups'),
            'cells_per_group':12, 'linear_expression_A':[10,0,1], 'linear_expression_B':[0,10,1],
            'observations':results,'wrong_top_marker_results':len(bad),'reproduced':bool(bad)}


def bedtools_e():
    bt = os.environ.get('BEDTOOLS', 'bedtools')
    a = OUT / 'either-A.bed'; b = OUT / 'either-B.bed'
    a.write_text('chr1\t0\t100\tquery\t0\t+\t0\t100\t0\t1\t100\t0\n')
    b.write_text('chr1\t40\t60\thit\n')
    base = [bt, 'intersect', '-a',str(a),'-b',str(b),'-f','0.9','-F','0.9','-e','-wa','-wb']
    outputs = []
    for options in [[], ['-split'], ['-sorted'], ['-split','-sorted']]:
        p = subprocess.run(base+options,text=True,capture_output=True,check=True)
        outputs.append({'extra_options':options,'stdout':p.stdout,'lines':len(p.stdout.splitlines())})
    return {'version':subprocess.check_output([bt,'--version'],text=True).strip(),
            'a':'one block [0,100)','b':'[40,60)','fraction_a':0.2,'fraction_b':1.0,
            'expected_lines':1,'outputs':outputs,'reproduced':outputs[0]['lines']==1 and outputs[1]['lines']==0}


if __name__ == '__main__':
    parser=argparse.ArgumentParser(); parser.add_argument('mode',choices=['python','bedtools','bio'])
    mode=parser.parse_args().mode
    if mode in ['python','bio']:
        run_case('biopython_consensus',consensus)
        run_case('biopython_skip_distance',skipped_distance)
    if mode=='python':
        run_case('gseapy_fdr',gseapy_fdr)
        run_case('scanpy_logreg',scanpy_logreg)
        (OUT/'pip-freeze.txt').write_text(subprocess.check_output([sys.executable,'-m','pip','freeze'],text=True))
    if mode=='bedtools': run_case('bedtools_split_e',bedtools_e)
    save_json()
    # A hypothesis can be false; dependency/runtime errors are not evidence.
    sys.exit(any(v['execution']=='error' for v in REPORT['cases'].values()))
