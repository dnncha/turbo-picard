"""Acquire only public upstream packages, data, and published notebook sources."""
import hashlib, json, platform, subprocess, sys, traceback, urllib.request
from pathlib import Path
import squidpy as sq
import scanpy as sc
import anndata as ad
import numpy as np
import pandas as pd
import scipy.sparse as sp
ROOT=Path('bundle'); (ROOT/'data').mkdir(exist_ok=True); (ROOT/'notebooks').mkdir(exist_ok=True)
COMMIT='407b151c1b7d657dd46a99cd9d924b13ec2d9afa'
def dump(path,obj):path.write_text(json.dumps(obj,indent=2,default=str)+'\n')
def sha(path):
 h=hashlib.sha256()
 with open(path,'rb') as f:
  for b in iter(lambda:f.read(1<<20),b''):h.update(b)
 return h.hexdigest()
def get_json(url):
 r=urllib.request.Request(url,headers={'User-Agent':'public-reference-bundle'})
 with urllib.request.urlopen(r,timeout=180) as s:return json.load(s)
errors=[]
for name in ['visium_hne_adata','slideseqv2']:
 try:
  path=ROOT/'data'/f'{name}.h5ad';a=getattr(sq.datasets,name)(path=path)
  meta={'dataset':name,'original_sha256':sha(path),'bytes':path.stat().st_size,'shape':list(a.shape),'X_dtype':str(a.X.dtype),'X_type':str(type(a.X)),'obs_columns':list(a.obs.columns),'var_columns':list(a.var.columns),'obsp':list(a.obsp.keys()),'obsm':list(a.obsm.keys()),'uns':list(a.uns.keys()),'raw_shape':None if a.raw is None else list(a.raw.shape)}
  dump(ROOT/'metadata'/f'{name}.json',meta);print('DATA',json.dumps(meta),flush=True)
 except Exception as e:traceback.print_exc();errors.append({'dataset':name,'error':repr(e)})
for name in ['visium','slideseqv2']:
 try:
  path=f'notebooks/graph_figures/{name}.ipynb';url=f'https://raw.githubusercontent.com/theislab/squidpy_reproducibility/{COMMIT}/{path}';nb=get_json(url);cells=[]
  for i,c in enumerate(nb.get('cells',[])):
   text=[]
   for v in c.get('outputs',[]):
    if 'text' in v:text.append(''.join(v['text']))
    if 'text/plain' in v.get('data',{}):text.append(''.join(v['data']['text/plain']))
   cells.append({'cell':i,'type':c['cell_type'],'source':''.join(c.get('source',[])),'text_outputs':text})
  dump(ROOT/'notebooks'/f'{name}_source.json',{'source_url':url,'commit':COMMIT,'cells':cells})
 except Exception as e:traceback.print_exc();errors.append({'notebook':name,'error':repr(e)})
dump(ROOT/'metadata'/'environment.json',{'python':sys.version,'platform':platform.platform(),'squidpy':sq.__version__,'scanpy':sc.__version__,'anndata':ad.__version__,'numpy':np.__version__})
(ROOT/'metadata'/'requirements.txt').write_text(subprocess.check_output([sys.executable,'-m','pip','freeze'],text=True))
dump(ROOT/'metadata'/'failures.json',errors)
manifest={str(p.relative_to(ROOT)):sha(p) for p in ROOT.rglob('*') if p.is_file() and p.suffix in ('.whl','.h5ad','.json')}
dump(ROOT/'metadata'/'sha256.json',manifest)
print('FAILURES',json.dumps(errors),flush=True)
if errors:sys.exit(1)
