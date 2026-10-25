#!/usr/bin/env python3
"""Scoped warm-cache BAM rewrites; decoded equality against original checkout."""
from pathlib import Path
import hashlib,json,os,platform,statistics,subprocess,sys
sys.path.insert(0,str(Path('/workspace/turbo-picard/tools')))
from bench_bounded_markduplicates import measure,summarize_bam,digest_file
repo=Path('/workspace/turbo-picard')
root=repo/'benchmarks/runs/2026-10-25-final-bam-io'
root.mkdir(parents=True,exist_ok=False)
source=repo/'benchmarks/runs/2026-10-25-final-bounded/paired-600k/input.bam'
samtools='/workspace/scratch/rival-tools/installed/usr/bin/samtools'
binaries={'baseline':'/workspace/scratch/rival-tools/turbo-baseline','candidate':str(repo/'target/release/turbo-picard')}
env={k:v for k,v in os.environ.items() if not k.startswith('TURBO_PICARD_')}
env.update(TURBO_PICARD_THREADS='0',TURBO_PICARD_REQUIRE_NATIVE='1')
affinity=['taskset','-c',str(min(os.sched_getaffinity(0)))]
report={'scope':__doc__,'host':platform.platform(),'input':{'path':str(source),'sha256':digest_file(source),'bytes':source.stat().st_size},'harness_sha256':digest_file(Path(__file__)),'executables':{name:{'path':binary,'sha256':digest_file(Path(binary))} for name,binary in binaries.items()},'affinity':affinity,'compression_level':5,'hts_workers':0,'repeats':3,'warmups':1,'cache':'warm cache; no fsync or cache dropping','contract':'ordered full SAM fields and typed tags except PG; SQ/RG headers; SortSam uses already-coordinate-sorted input','runs':[],'summaries':[],'pass':True}
def save():
 (root/'report.json').write_text(json.dumps(report,indent=2)+'\n')
for command,options in [('CleanSam',[]),('SortSam',['SORT_ORDER=coordinate'])]:
 reference=None
 for repeat in range(4):
  for name in (list(binaries) if repeat%2==0 else list(reversed(binaries))):
   destination=root/f'{command}-{name}-{repeat}'
   output=destination/'output.bam'
   argv=[*affinity,binaries[name],command,f'I={source}',f'O={output}',f'TMP_DIR={destination / "scratch"}','COMPRESSION_LEVEL=5','VALIDATION_STRINGENCY=SILENT','QUIET=true',*options]
   row=measure(argv,destination,env)
   row.update(command=command,version=name,repeat=repeat,warmup=repeat==0,parity_pass=False)
   if row['exit_code']==0:
    summary=summarize_bam(output,samtools,destination)
    if reference is None:
     reference=summary
    row.update(parity_pass=summary==reference,output_summary=summary,output_bytes=output.stat().st_size)
    if row['parity_pass']:
     output.unlink()
   report['pass'] &= row['parity_pass']
   report['runs'].append(row)
   save()
 summary={'command':command}
 for name in binaries:
  rows=[r for r in report['runs'] if r['command']==command and r['version']==name and not r['warmup']]
  summary[name]={'parity_pass':all(r['parity_pass'] for r in rows),'median_wall_seconds':statistics.median(r['wall_seconds'] for r in rows),'median_peak_rss_bytes':statistics.median(r.get('peak_rss_bytes',0) for r in rows)}
 report['summaries'].append(summary)
 save()
 print(json.dumps(summary),flush=True)
assert digest_file(source)==report['input']['sha256']
raise SystemExit(0 if report['pass'] else 1)
