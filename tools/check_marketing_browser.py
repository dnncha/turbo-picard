#!/usr/bin/env python3
"""Browser checks for the generated website. Requires Playwright + Chromium.

--offline-render is for restricted preview environments; it injects local CSS/JS
without network navigation. Normal mode serves and navigates the actual output.
Neither mode submits form values or executes generated genomics commands.
"""
from __future__ import annotations
import argparse
import contextlib
import functools
import http.server
import json
from pathlib import Path
import re
import threading
from playwright.sync_api import sync_playwright

ROOT=Path(__file__).resolve().parents[1]


def main():
    ap=argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--site',type=Path,default=ROOT/'docs/site')
    ap.add_argument('--output',type=Path,required=True)
    ap.add_argument('--offline-render',action='store_true')
    ap.add_argument('--chromium',default=None)
    args=ap.parse_args();args.output.mkdir(parents=True,exist_ok=True)
    class Quiet(http.server.SimpleHTTPRequestHandler):
        def log_message(self,*a):pass
    server=None
    if not args.offline_render:
        server=http.server.ThreadingHTTPServer(('127.0.0.1',0),functools.partial(Quiet,directory=str(args.site.resolve())))
        threading.Thread(target=server.serve_forever,daemon=True).start()
    report={'mode':'offline local render' if args.offline_render else 'real local HTTP navigation','layouts':[],'checks':[],'errors':[]}
    with sync_playwright() as p:
        kwargs={'headless':True,'args':['--no-sandbox']}
        if args.chromium:kwargs['executable_path']=args.chromium
        browser=p.chromium.launch(**kwargs)
        page=browser.new_page(viewport={'width':1440,'height':1000})
        page.on('pageerror',lambda e:report['errors'].append(str(e)))
        def visit(route,js=True):
            if args.offline_render:
                html=(args.site/route/'index.html').read_text()
                html=re.sub(r'<link rel="stylesheet"[^>]+>',lambda m:'<style>'+(args.site/'assets/site.css').read_text()+'</style>',html)
                html=re.sub(r'<script src="[^"]+" defer></script>','',html)
                page.set_content(html,wait_until='domcontentloaded')
                if js:page.add_script_tag(content=(args.site/'assets/site.js').read_text())
            else:page.goto(f'http://127.0.0.1:{server.server_port}/'+route,wait_until='networkidle')
        for width in (1440,820,390,320):
            page.set_viewport_size({'width':width,'height':1000 if width>700 else 844})
            for route in ('','commands/','evaluate/','solutions/markduplicates-memory/','solutions/bam-to-fastq/','solutions/sequencing-qc/','solutions/bam-sorting/','evidence/','install/','compare/'):
                visit(route)
                overflow=page.evaluate('document.documentElement.scrollWidth > window.innerWidth')
                assert not overflow,(width,route,'horizontal page overflow')
                assert page.locator('h1').count()==1
                report['layouts'].append({'width':width,'page':route or '/','overflow':False})
            visit('')
            if width in (1440,390):page.screenshot(path=str(args.output/f'home-{width}.png'),full_page=True)
        page.set_viewport_size({'width':1280,'height':1000})
        visit('commands/');page.locator('#command-search').fill('CollectRnaSeqMetrics')
        assert page.locator('[data-command]:visible').count()==1
        assert 'Upstream only' in page.locator('[data-command]:visible').inner_text()
        page.locator('#command-filter').select_option('accelerated');assert page.locator('#no-commands').is_visible()
        page.locator('#command-search').fill('');assert page.locator('[data-command]:visible').count()==38
        report['checks'].append('catalogue: exact search, fallback boundary, empty state and native filter')
        page.screenshot(path=str(args.output/'commands-desktop.png'),full_page=False)
        visit('evaluate/');requests=[];page.on('request',lambda r:requests.append(r.url))
        page.locator('#trial-input').fill('/data/sample.cram');page.locator('#generate-trial').click()
        assert 'Reference' in page.locator('#trial-error').inner_text()
        assert page.locator('#trial-script').locator('..').locator('..').locator('[data-copy]').is_disabled()
        page.locator('#trial-reference').fill('/reference/a b.fa')
        page.locator('#trial-jar').fill("/opt/O'Brien/$(touch should-not-run).jar")
        page.locator('#generate-trial').click()
        text=page.locator('#trial-script').inner_text();assert '--reference-fasta' in text and "O'" in text
        assert page.locator('#trial-error').inner_text()==''
        before=page.url;page.locator('#trial-input').press('Enter');assert page.url==before
        assert not requests,requests
        report['checks'].append('planner: CRAM reference gate, quoted paths, no form navigation or network submission')
        copy=page.locator('#trial-script').locator('..').locator('..').locator('[data-copy]')
        page.evaluate("Object.defineProperty(window,'isSecureContext',{value:true,configurable:true}); Object.defineProperty(navigator,'clipboard',{value:{writeText:async text=>{window.__copied=text}},configurable:true})")
        copy.click();assert page.evaluate('window.__copied')==page.locator('#trial-script').inner_text()
        report['checks'].append('clipboard: success receives the exact generated script')
        page.evaluate("Object.defineProperty(navigator,'clipboard',{value:{writeText:async()=>{throw new Error('denied')}},configurable:true})")
        copy.click();assert 'Command selected' in page.locator('.copy-status').inner_text()
        report['checks'].append('clipboard: denied access selects text and gives manual instructions')
        page.locator('#trial-input').fill('/data/changed.bam');assert copy.is_disabled()
        report['checks'].append('planner: changed inputs cannot copy a stale script')
        page.locator('#generate-trial').click();page.screenshot(path=str(args.output/'evaluate-desktop.png'),full_page=True)
        page.emulate_media(reduced_motion='reduce');visit('')
        assert page.evaluate("getComputedStyle(document.documentElement).scrollBehavior")=='auto'
        report['checks'].append('reduced motion: smooth scrolling disabled')
        if args.offline_render:
            visit('commands/',js=False);assert page.locator('[data-command]').count()==126
        else:
            context=browser.new_context(java_script_enabled=False,viewport={'width':390,'height':844})
            nojs=context.new_page();nojs.goto(f'http://127.0.0.1:{server.server_port}/commands/');assert nojs.locator('[data-command]').count()==126;context.close()
        report['checks'].append('progressive enhancement: complete catalogue remains without JavaScript')
        assert not report['errors'],report['errors']
        browser.close()
    if server:server.shutdown()
    report['status']='PASS';(args.output/'browser-report.json').write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps({'layouts':len(report['layouts']),'behaviour_checks':len(report['checks']),'status':'PASS','mode':report['mode']}))


if __name__=='__main__':main()
