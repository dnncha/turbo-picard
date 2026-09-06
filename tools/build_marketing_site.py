#!/usr/bin/env python3
"""Build a static, evidence-backed site; no network, credentials or JS rendering.

Edit website/pages and website/assets. Generated docs/site output is checked in
and verified in CI. Sphinx overlays it after a successful HTML build so the
landing page actually reaches the existing Read the Docs hostname.
"""
from __future__ import annotations

import argparse
import html
import json
from pathlib import Path
import posixpath
import shutil
import sys
import tomllib
from xml.etree import ElementTree as ET

import yaml

ROOT = Path(__file__).resolve().parents[1]
BASE = 'https://turbo-picard.readthedocs.io/en/latest/'
REPO = 'https://github.com/dnncha/turbo-picard'
PAGES = [
    ('', 'Turbo Picard — native speed for Picard workflows', 'Run selected Picard commands in Rust. Diagnose slow duplicate marking, BAM sorting and FASTQ export. Check support and compare safely; fallback handles unsupported commands.', 'Picard workflows. Native speed.', 'home'),
    ('solutions/markduplicates-memory/', 'Picard MarkDuplicates slow or out of memory? | Turbo Picard', 'Diagnose heap, total RAM and scratch-space failures in MarkDuplicates. Learn when Turbo Picard’s bounded native path fits, and compare without changing optical-duplicate semantics.', 'MarkDuplicates: slow runs and memory failures', 'markduplicates'),
    ('solutions/bam-to-fastq/', 'Paired BAM to FASTQ without unbounded mate buffering | Turbo Picard', 'Export paired, unpaired or gzipped FASTQ from BAM or CRAM. Understand queryname staging, read orientation and scratch storage before using native SamToFastq.', 'BAM to FASTQ, without the guesswork', 'fastq'),
    ('solutions/sequencing-qc/', 'Faster Picard sequencing QC with CollectMultipleMetrics | Turbo Picard', 'Reduce repeated sequencing-QC work with native CollectMultipleMetrics. Check supported programs, filters and output contracts before replacing a Picard task.', 'Sequencing QC without the repeated work', 'qc'),
    ('solutions/bam-sorting/', 'Picard SortSam: coordinate sorting, memory and temporary files | Turbo Picard', 'Understand BAM sorting, memory budgets and scratch I/O. Evaluate native SortSam with stable external merging and keep required index and checksum outputs.', 'BAM sorting that fits your workflow', 'sorting'),
    ('commands/', 'Turbo Picard command support — native, scoped and fallback', 'Search the source-derived Turbo Picard command catalogue. See exact native scope, fallback boundaries and parity tests before changing your pipeline.', 'Find your command. Know the boundary.', 'commands'),
    ('evaluate/', 'Compare Turbo Picard with Picard on your own BAM or CRAM', 'Build a local comparison script with separate outputs, a pinned release and native-only execution. Inspect parity and timing before switching a workflow step.', 'Prove the switch on your data.', 'evaluate'),
    ('install/', 'Install Turbo Picard on Linux or macOS — PyPI and Docker', 'Install Turbo Picard in an isolated environment or use the versioned container. Avoid shadowing upstream Picard and verify the executable you actually run.', 'One package. A familiar command.', 'install'),
    ('compare/', 'Turbo Picard vs Picard, samtools and Riker: choose by task', 'Choose a Picard alternative by command semantics, workflow changes and evidence—not a universal speed claim. Compare Turbo Picard, Picard, samtools and Riker.', 'Choose by workflow. Not by the biggest number.', 'compare'),
    ('evidence/', 'Turbo Picard benchmarks, parity and release evidence', 'Inspect absolute benchmark timings, input sizes, provenance and compatibility limits. Separate small-fixture evidence from production-scale performance claims.', 'Evidence you can inspect.', 'evidence'),
]


def esc(value: object) -> str:
    return html.escape(str(value), quote=True)


def prefix(route: str) -> str:
    return '../' * len(route.strip('/').split('/')) if route else './'


def code(text: str, label: str = 'Shell') -> str:
    return f'<div class="codebox"><div class="codebar"><span>{esc(label)}</span><button type="button" data-copy aria-label="Copy {esc(label)} command">Copy</button></div><pre tabindex="0"><code>{esc(text)}</code></pre><span class="copy-status" role="status"></span></div>'


def command_catalogue(matrix: dict) -> str:
    utility = {'AccelerationStatus', 'capabilities', 'doctor', 'explain', 'trial'}
    cards = []
    for c in matrix['commands']:
        name, status = c['name'], c['status']
        badge = 'Utility' if name in utility else {'native': 'Native', 'partial-native': 'Scoped native', 'fallback-only': 'Upstream only'}[status]
        scope = c['native_scope'] if status != 'fallback-only' else 'No native implementation in this release. Keep upstream Picard for this task; delegation is not a speedup.'
        parity = f'<a href="{REPO}/blob/v{{{{VERSION}}}}/{esc(c["parity_script"])}">Read the parity test ↗</a>' if c.get('parity_script') else ''
        cards.append(f'<article class="command-row" id="{esc(name)}" data-command="{esc(name)}" data-status="{esc(status)}"><div><h2>{esc(name)}</h2><span class="badge {esc(status)}">{badge}</span></div><p>{esc(scope)}</p><details><summary>Scope and verification</summary><p><strong>Boundary:</strong> {esc(c["fallback_scope"])}</p>{parity}</details></article>')
    return '\n'.join(cards)


def evidence_table(data: dict) -> str:
    rows=[]
    for b in sorted(data['benchmarks'], key=lambda x: x['command'].lower()):
        workload = b['workload_parameter']
        rows.append(f'<tr><th scope="row">{esc(b["command"])}</th><td>{b["median_turbo_seconds"]:.6f}</td><td>{b["median_picard_seconds"]:.6f}</td><td>{b["speedup"]:.2f}x</td><td>{b["runs"]}</td><td>{esc(workload["name"])}={workload["value"]:,}</td><td>{esc(b["parity"])}</td></tr>')
    return '<div class="table-scroll" role="region" aria-label="All saved benchmark results" tabindex="0"><table><caption>32 commands · saved small-fixture suite · '+esc(data['date'])+'</caption><thead><tr><th scope="col">Command</th><th scope="col">Turbo (s)</th><th scope="col">Picard (s)</th><th scope="col">Saved ratio</th><th scope="col">Runs</th><th scope="col">Generator input</th><th scope="col">Parity</th></tr></thead><tbody>'+''.join(rows)+'</tbody></table></div>'


def sample_results(data: dict) -> str:
    chosen = ['MarkDuplicates','SamToFastq','CollectMultipleMetrics']
    result=[]
    for name in chosen:
        b=next(x for x in data['benchmarks'] if x['command']==name)
        result.append(f'<article class="result-card"><p class="eyebrow">{name}</p><p class="timing">{b["median_turbo_seconds"]:.3f}<span>s</span></p><p>Turbo Picard median<br><strong>{b["median_picard_seconds"]:.3f}s</strong> upstream Picard median</p><p class="fine">3 runs · generator reads={b["workload_parameter"]["value"]:,}<br>Saved fixture parity: PASS</p></article>')
    return ''.join(result)


def real_data_provenance(manifest: dict) -> str:
    """Render the pinned dataset inventory without hand-maintained proof claims."""
    sections=[]
    for dataset in manifest['datasets']:
        evidence=dataset['evidence_markdown']
        minimum=dataset.get('minimum_input_bytes')
        threshold=(f'<p>Release input-size guard: <code>{minimum}</code> bytes minimum. '
                   'This guard does not establish production-scale representativeness.</p>') if minimum else ''
        commands=', '.join(esc(name) for name in dataset['expected_commands'])
        sections.append(f'<details><summary>{esc(dataset["id"])} · {esc(dataset["release_tier"])}</summary>'
                        f'<p>{esc(dataset["description"])}</p><p><strong>Scope:</strong> {esc(dataset["scope_caveat"])}</p>'
                        f'<p><strong>Commands in the saved evidence:</strong> {commands}</p>'
                        f'<p><a href="{esc(dataset["source_url"])}">Pinned input source ↗</a></p>'
                        f'<p>Source commit: <code>{esc(dataset["source_commit"])}</code><br>'
                        f'Input SHA-256: <code>{esc(dataset["sha256"])}</code></p>{threshold}'
                        f'<p><a href="{REPO}/blob/v{{{{VERSION}}}}/{esc(evidence)}">Read {esc(evidence)} ↗</a></p></details>')
    return '\n'.join(sections)


def render_all(root: Path = ROOT) -> dict[str, bytes]:
    version=tomllib.loads((root/'Cargo.toml').read_text())['workspace']['package']['version']
    matrix=yaml.safe_load((root/'docs/command-matrix.yml').read_text())
    data=json.loads((root/'docs/site/assets/benchmark-data.json').read_text())
    if data['summary']['command_count'] != len(data['benchmarks']):
        raise ValueError('benchmark count does not match evidence')
    manifest=json.loads((root/'benchmarks/real-data/manifest.json').read_text())
    chrome=(root/'website/layout.html').read_text()
    files={}
    for route,title,description,heading,key in PAGES:
        p=prefix(route)
        schema=[{'@context':'https://schema.org','@type':'WebPage','name':title,'description':description,'url':BASE+route}]
        if not route:
            schema += [{'@context':'https://schema.org','@type':'SoftwareSourceCode','name':'Turbo Picard','codeRepository':REPO,'programmingLanguage':'Rust','license':REPO+'/blob/main/LICENSE','version':version,'description':'Native implementations of selected Picard commands with documented compatibility boundaries.'}]
        else:
            schema += [{'@context':'https://schema.org','@type':'BreadcrumbList','itemListElement':[{'@type':'ListItem','position':1,'name':'Turbo Picard','item':BASE},{'@type':'ListItem','position':2,'name':heading,'item':BASE+route}]}]
        body=(root/f'website/pages/{key}.html').read_text()
        body=body.replace('{{DEFAULT_TRIAL}}',esc((root/'website/default-trial.sh').read_text().strip().replace('0.1.13',version)))
        body=body.replace('{{CATALOGUE}}',command_catalogue(matrix)).replace('{{BENCHMARK_TABLE}}',evidence_table(data)).replace('{{SAMPLE_RESULTS}}',sample_results(data))
        body=body.replace('{{REAL_DATA_PROVENANCE}}',real_data_provenance(manifest))
        content=chrome.replace('{{BODY}}',body)
        replacements={'ROOT':p,'VERSION':version,'PICARD_VERSION':str(matrix['picard_reference']),'TITLE':esc(title),'DESCRIPTION':esc(description),'CANONICAL':BASE+route,'HEADING':esc(heading),'SCHEMA':json.dumps(schema,ensure_ascii=False).replace('<','\\u003c'),'ROUTE':key,'BENCHMARK_DATE':data['date'],'GM':f'{data["summary"]["geometric_mean_speedup"]:.2f}x','FLOOR':f'{data["summary"]["floor_speedup"]:.2f}x','TOP':f'{data["summary"]["top_speedup"]:.2f}x','MEDIAN':f'{data["summary"]["median_speedup"]:.2f}x'}
        for k,v in replacements.items(): content=content.replace('{{'+k+'}}',v)
        if '{{' in content: raise ValueError(f'unresolved template placeholder in {key}')
        files[route+'index.html']=content.encode()
    for path in sorted((root/'website/assets').glob('*')):
        if path.is_file(): files['assets/'+path.name]=path.read_bytes()
    ET.register_namespace('', 'http://www.sitemaps.org/schemas/sitemap/0.9')
    xml=ET.Element('{http://www.sitemaps.org/schemas/sitemap/0.9}urlset')
    paths=[r for r,*_ in PAGES]+[f'{f.stem}.html' for f in sorted((root/'docs').glob('*.rst')) if f.stem!='index']
    for path in paths:
        el=ET.SubElement(xml,'url');ET.SubElement(el,'loc').text=BASE+path
    files['sitemap.xml']=ET.tostring(xml,encoding='utf-8',xml_declaration=True)
    files['robots.txt']=('User-agent: *\nAllow: /\nSitemap: '+BASE+'sitemap.xml\n').encode()
    files['llms.txt']=(f'# Turbo Picard {version}\n\nNative implementations of selected Picard {matrix["picard_reference"]} commands, not the entire upstream suite.\n\n'+''.join(f'- [{h}]({BASE+r})\n' for r,_,_,h,_ in PAGES)+'\n## Machine interface\n\n`turbo-picard capabilities --json --command MarkDuplicates`\n`turbo-picard trial --json MarkDuplicates I=input.bam O=turbo.bam M=turbo.txt`\nInspection is not input validation. Prefer argv arrays and shell=False.\nSet TURBO_PICARD_REQUIRE_NATIVE=1 to prohibit upstream fallback.\nPyPI installs a picard shim as well: isolate the environment.\nBenchmarks describe saved fixtures, not universal production speedups.\n').encode()
    return files


def build(root: Path = ROOT, check: bool = False) -> list[str]:
    mismatches=[]
    for rel,body in render_all(root).items():
        target=root/'docs/site'/rel
        if not target.exists() or target.read_bytes()!=body:
            mismatches.append(rel)
            if not check:
                target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes(body)
    return mismatches


def deploy_to_sphinx(app, exception) -> None:
    """Only successful HTML builds receive the site; failed docs never publish it."""
    if exception is not None or app.builder.name != 'html': return
    root=Path(app.srcdir).resolve().parent
    if build(root,check=True):
        raise RuntimeError('Marketing output is stale; run tools/build_marketing_site.py')
    # Copy the dedicated sub-pages and assets, then deliberately replace index.
    # This event works on incremental and clean Sphinx builds, unlike an implicit
    # copy-order dependency between html_extra_path and document generation.
    shutil.copytree(root/'docs/site', Path(app.outdir), dirs_exist_ok=True)


def main() -> int:
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    args=parser.parse_args()
    changed=build(check=args.check)
    if args.check and changed:
        print('Stale marketing output: '+', '.join(changed),file=sys.stderr);return 1
    print(f'Marketing site: {len(PAGES)} pages; '+('reproducible' if args.check else 'generated'))
    return 0


if __name__=='__main__': raise SystemExit(main())
