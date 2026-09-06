#!/usr/bin/env python3
"""Independent sort/merge coverage check; not the endpoint sweep used by the builder."""
import argparse,collections,gzip,hashlib,json,pathlib,tempfile

def union(items):
    result=[]
    for a,b in sorted(items):
        if result and a<=result[-1][1]:result[-1]=(result[-1][0],max(b,result[-1][1]))
        else:result.append((a,b))
    return result

def length(items):return sum(b-a for a,b in items)
def overlap(a,b):
    i=j=total=0
    while i<len(a) and j<len(b):
        total+=max(0,min(a[i][1],b[j][1])-max(a[i][0],b[j][0]))
        if a[i][1]<b[j][1]:i+=1
        else:j+=1
    return total

def cat(raw):return {'LINE':0,'SINE':1,'LTR':2,'DNA':3,'Satellite':4,'Simple_repeat':5,'Low_complexity':5}.get(raw,6)
def run(cache,data,report):
    m=json.loads((data/'manifest.json').read_text());chroms={c['id']:c for c in m['chromosomes']}
    for source in m['sources']:
        p=cache/source['url'].split('/')[-1];h=hashlib.sha256()
        with p.open('rb') as f:
            for chunk in iter(lambda:f.read(1<<20),b''):h.update(chunk)
        assert h.hexdigest()==source['sha256'],source['id']
    gaps=collections.defaultdict(list)
    with gzip.open(cache/'gap.txt.gz','rt') as f:
        for l in f:
            x=l.rstrip('\n').split('\t')
            if x[1] in chroms:gaps[x[1]].append((int(x[2]),int(x[3])))
    family_totals=collections.Counter();counts=collections.Counter();results=[]
    with tempfile.TemporaryDirectory() as td:
        root=pathlib.Path(td);files={c:(root/c).open('w') for c in chroms}
        try:
            with gzip.open(cache/'rmsk.txt.gz','rt') as f:
                for l in f:
                    x=l.rstrip('\n').split('\t')
                    if x[5] in files:files[x[5]].write('\t'.join([x[6],x[7],x[11],x[12]])+'\n')
        finally:
            for f in files.values():f.close()
        for c,meta in chroms.items():
            classes=[[] for _ in range(7)];families=collections.defaultdict(list)
            with (root/c).open() as f:
                for l in f:
                    a,b,cl,fam=l.rstrip('\n').split('\t');seg=(int(a),int(b));classes[cat(cl)].append(seg);families[(cl,fam)].append(seg);counts[(cl,fam)]+=1
            merged=[union(x) for x in classes];gap=union(gaps[c]);all_repeat=union(seg for group in merged for seg in group)
            repeat_bp=length(all_repeat)-overlap(all_repeat,gap);exclusive=[]
            for i in range(7):
                excluded=union(seg for group in merged[:i]+merged[i+1:]+[gap] for seg in group)
                exclusive.append(length(merged[i])-overlap(merged[i],excluded))
            totals=exclusive+[repeat_bp-sum(exclusive),meta['size']-length(gap)-repeat_bp,length(gap)]
            assert totals==meta['totals'],(c,totals,meta['totals'])
            for key,segments in families.items():
                u=union(segments);family_totals[key]+=length(u)-overlap(u,gap)
            results.append({'chromosome':c,'disjointCategoriesAgree':True,'repeatUnionOutsideGaps':repeat_bp})
    for family in m['families']:
        key=(family['originalClass'],family['name']);assert family_totals[key]==family['bp'],key;assert counts[key]==family['fragments'],key
    result={'method':'Independent sorted interval unions and two-pointer intersections; no builder imports','all24ChromosomesAnd10CategoriesAgree':True,'allFamilyUnionsAndFragmentCountsAgree':True,'familiesChecked':len(m['families']),'chromosomes':results}
    report.parent.mkdir(parents=True,exist_ok=True);report.write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result))

if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--cache',type=pathlib.Path,default=pathlib.Path('.cache/genome-unwoven'));p.add_argument('--data',type=pathlib.Path,required=True);p.add_argument('--report',type=pathlib.Path,required=True);a=p.parse_args();run(a.cache,a.data,a.report)
