#!/usr/bin/env python3
"""Browser checks for the generated website. Requires Playwright + Chromium.

--offline-render is for restricted preview environments; it injects local CSS/JS
without network navigation. Normal mode serves and navigates the actual output.
Neither mode submits form values or executes generated genomics commands.
"""
from __future__ import annotations
import argparse
import base64
import mimetypes
import contextlib
import functools
import http.server
import html as html_module
import json
from pathlib import Path
import re
import threading
from urllib.parse import unquote, urlsplit

ROOT=Path(__file__).resolve().parents[1]


def offline_css(asset_dir):
    def embed(match):
        url = match.group(1).strip('\"\'')
        if url.startswith(('data:', 'http:', 'https:', '#')):
            return match.group(0)
        asset = (asset_dir / url).resolve()
        asset.relative_to(asset_dir.resolve())
        mime = mimetypes.guess_type(asset.name)[0] or 'application/octet-stream'
        payload = base64.b64encode(asset.read_bytes()).decode('ascii')
        return f'url("data:{mime};base64,{payload}")'
    return re.sub(r'url\(([^)]+)\)', embed, (asset_dir / 'site.css').read_text())


def offline_html(site, route):
    site = site.resolve()
    document = site / route / 'index.html'

    def embed_url(url):
        url = html_module.unescape(url)
        if url.startswith('data:'):
            return url
        parsed = urlsplit(url)
        if parsed.scheme or parsed.netloc:
            raise ValueError(f'Offline image must be local: {url}')
        asset = ((site if parsed.path.startswith('/') else document.parent)
                 / unquote(parsed.path).lstrip('/')).resolve()
        asset.relative_to(site)
        mime = mimetypes.guess_type(asset.name)[0] or 'application/octet-stream'
        payload = base64.b64encode(asset.read_bytes()).decode('ascii')
        return f'data:{mime};base64,{payload}'

    def embed_tag(match):
        def embed_attribute(attribute):
            name, quote, value = attribute.groups()
            if name.lower() == 'srcset':
                # Generated srcsets contain local URLs and optional w/x descriptors.
                value = re.sub(r'(^|,\s*)([^,\s]+)',
                               lambda candidate: candidate[1] + embed_url(candidate[2]), value)
            else:
                value = embed_url(value)
            return name + '=' + quote + value + quote
        return re.sub(
            r"(?<![\w:-])(srcset|src)=([\"'])(.*?)\2", embed_attribute, match[0], flags=re.I)

    html = re.sub(r'<(?:img|source)\b[^>]*>', embed_tag, document.read_text(), flags=re.I)
    html = re.sub(r'<link rel="stylesheet"[^>]+>',
                  lambda m: '<style>' + offline_css(site / 'assets') + '</style>', html)
    return re.sub(r'<script src="[^\"]+" defer></script>', '', html)


def main():
    from playwright.sync_api import expect, sync_playwright
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
        offline_requests = []
        if args.offline_render:
            def block_request(request_route):
                offline_requests.append(request_route.request.url)
                request_route.abort()
            page.route('**/*', block_request)
        def visit(route,js=True):
            if args.offline_render:
                html=offline_html(args.site, route)
                page.set_content(html,wait_until='domcontentloaded')
                if js:page.add_script_tag(content=(args.site/'assets/site.js').read_text())
                page.evaluate("""async () => {
                    for (const image of document.images) {
                        await image.decode();
                        if (!image.naturalWidth) throw new Error('Document image did not load: ' + image.currentSrc);
                    }
                    for (const mark of document.querySelectorAll('.brand-mark')) {
                        const url = getComputedStyle(mark).backgroundImage.slice(5, -2);
                        const image = new Image();
                        image.src = url;
                        await image.decode();
                        if (!image.naturalWidth) throw new Error('Mascot image did not load');
                    }
                }""")
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
        assert 'Upstream Picard required' in page.locator('[data-command]:visible').inner_text()
        page.locator('#command-filter').select_option('accelerated');assert page.locator('#no-commands').is_visible()
        page.locator('#command-search').fill('');assert page.locator('[data-command]:visible').count()==38
        report['checks'].append('catalogue: exact search, fallback boundary, empty state and native filter')
        page.screenshot(path=str(args.output/'commands-desktop.png'),full_page=False)
        visit('evaluate/');requests=[]
        def record_request(request):requests.append(request.url)
        page.on('request',record_request)
        copy=page.locator('#trial-script').locator('..').locator('..').locator('[data-copy]')
        status=page.locator('.copy-status')
        def clipboard(body):
            page.evaluate("Object.defineProperty(window,'isSecureContext',{value:true,configurable:true}); Object.defineProperty(navigator,'clipboard',{value:{writeText:"+body+"},configurable:true})")
        clipboard('async text=>{window.__copied=text}')
        copy.click();expect(status).to_contain_text('Example script copied. Replace the example paths')
        assert page.evaluate('window.__copied')==page.locator('#trial-script').inner_text()
        original=page.locator('#trial-script').inner_text()
        page.locator('#trial-input').fill('/data/sample.cram');page.locator('#generate-trial').click()
        reference=page.locator('#trial-reference')
        expect(reference).to_be_focused();expect(reference).to_have_attribute('aria-invalid','true')
        expect(reference).to_have_accessible_name('Reference FASTA path')
        expect(page.locator('#trial-input')).to_have_accessible_name('Input BAM or CRAM path (required)')
        expect(page.locator('#trial-jar')).to_have_accessible_name('Upstream Picard JAR path (required)')
        assert reference.evaluate('(e)=>e.required')
        expect(reference).to_have_attribute('aria-describedby','trial-reference-help trial-reference-error')
        expect(page.locator('#trial-reference-error')).to_contain_text('Reference FASTA is required for CRAM')
        assert page.locator('#trial-script').inner_text()==original
        assert page.locator('#trial-input').input_value()=='/data/sample.cram'
        assert copy.is_disabled()
        page.locator('#trial-reference').fill('/reference/a b.fa')
        page.locator('#trial-jar').fill("/opt/O'Brien/$(touch should-not-run).jar")
        page.locator('#generate-trial').click()
        text=page.locator('#trial-script').inner_text();assert '--reference-fasta' in text and "O'" in text
        assert page.locator('#trial-error').inner_text()==''
        expect(reference).not_to_have_attribute('aria-invalid','true')
        expect(reference).to_have_attribute('aria-describedby','trial-reference-help')
        expect(page.locator('#trial-script-label')).to_have_text('Comparison script')
        before=page.url;page.locator('#trial-input').press('Enter');assert page.url==before
        assert not requests,requests
        report['checks'].append('planner: CRAM reference gate, quoted paths, no form navigation or network submission')
        clipboard('async text=>{window.__copied=text}')
        copy.click();assert page.evaluate('window.__copied')==page.locator('#trial-script').inner_text()
        expect(status).to_have_text('Script copied. Review it before running.')
        report['checks'].append('clipboard: success receives the exact generated script')
        clipboard("async()=>{throw new Error('denied')}")
        copy.click();expect(status).to_contain_text('Script selected')
        assert page.evaluate('window.getSelection().toString()')==page.locator('#trial-script').inner_text()
        report['checks'].append('clipboard: denied access selects text and gives manual instructions')
        page.evaluate('window.__getSelection=window.getSelection; window.getSelection=()=>null')
        copy.click();expect(status).to_contain_text('Select the script text')
        expect(page.locator('.codebox pre')).to_be_focused()
        page.evaluate('window.getSelection=window.__getSelection')
        report['checks'].append('clipboard: failed selection offers manual copying and focuses the script')
        page.locator('#trial-input').fill('/data/changed.bam');assert copy.is_disabled()
        expect(page.locator('#trial-script-label')).to_have_text('Previous script')
        assert not reference.evaluate('(e)=>e.required')
        report['checks'].append('planner: changed inputs cannot copy a stale script')
        page.locator('#generate-trial').click()
        # A pending clipboard operation may finish after settings change. Its result
        # must not claim that the current, newly generated text was copied.
        clipboard('text=>new Promise(resolve=>{window.__copied=text;window.__resolveCopy=resolve})')
        copy.click();expect(copy).to_be_disabled();expect(copy).to_have_attribute('aria-busy','true')
        copied=page.evaluate('window.__copied')
        page.locator('#trial-input').fill('/data/newest.bam');page.locator('#generate-trial').click()
        assert copied!=page.locator('#trial-script').inner_text()
        page.evaluate('window.__resolveCopy()');expect(status).to_contain_text('An earlier script was copied')
        expect(copy).to_be_enabled()
        report['checks'].append('clipboard: completion after regeneration identifies the earlier script')
        clipboard('()=>new Promise(()=>{})')
        page.clock.install();copy.click();page.clock.fast_forward(8100)
        expect(copy).to_be_enabled();expect(status).to_contain_text('Script selected')
        report['checks'].append('clipboard: stalled access recovers without an indefinitely disabled button')
        # Ordered recovery keeps the previous artifact and moves focus to the
        # relevant control, clearing its error once settings change.
        for field,value,message in [('trial-input','','Enter an input'),
                                    ('trial-input','relative.bam','full path'),
                                    ('trial-input','/data/wrong.sam','ending in .bam'),
                                    ('trial-jar','/bin/turbo-picard','ending in .jar')]:
            page.locator('#trial-input').fill('/data/研究 🧬.bam')
            page.locator('#trial-jar').fill('/opt/picard.jar')
            old=page.locator('#trial-script').inner_text()
            control=page.locator('#'+field);control.fill(value);page.locator('#generate-trial').click()
            expect(control).to_be_focused();expect(control).to_have_attribute('aria-invalid','true')
            expect(page.locator('#'+field+'-error')).to_contain_text(message)
            assert page.locator('#trial-script').inner_text()==old and copy.is_disabled()
        report['checks'].append('planner: field-linked errors preserve paths and previous script')
        page.locator('#trial-jar').fill('/opt/picard.jar');page.locator('#generate-trial').click()
        assert '/data/研究 🧬.bam' in page.locator('#trial-script').inner_text()
        report['checks'].append('planner: Unicode paths generate without truncation')
        assert not requests,requests
        page.remove_listener('request',record_request)
        page.screenshot(path=str(args.output/'evaluate-desktop.png'),full_page=True)
        page.emulate_media(reduced_motion='reduce');visit('')
        assert page.evaluate("getComputedStyle(document.documentElement).scrollBehavior")=='auto'
        report['checks'].append('reduced motion: smooth scrolling disabled')
        if args.offline_render:
            visit('commands/',js=False);assert page.locator('[data-command]').count()==126
        else:
            context=browser.new_context(java_script_enabled=False,viewport={'width':390,'height':844})
            nojs=context.new_page();nojs.goto(f'http://127.0.0.1:{server.server_port}/commands/');assert nojs.locator('[data-command]').count()==126
            nojs.goto(f'http://127.0.0.1:{server.server_port}/evaluate/')
            assert nojs.locator('#generate-trial').is_disabled() and nojs.locator('[data-copy]').is_disabled()
            assert nojs.locator('#trial-script').inner_text().startswith('#!/usr/bin/env bash')
            assert nojs.locator('noscript').is_visible();context.close()
        report['checks'].append('progressive enhancement: complete catalogue remains without JavaScript')
        assert not offline_requests,offline_requests
        assert not report['errors'],report['errors']
        browser.close()
    if server:server.shutdown()
    report['status']='PASS';(args.output/'browser-report.json').write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps({'layouts':len(report['layouts']),'behaviour_checks':len(report['checks']),'status':'PASS','mode':report['mode']}))


if __name__=='__main__':main()
