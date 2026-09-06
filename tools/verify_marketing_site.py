#!/usr/bin/env python3
"""Check every marketing page, discovery route and Sphinx delivery contract."""
from __future__ import annotations
import argparse
from html.parser import HTMLParser
import json
from pathlib import Path
import re
import sys
from urllib.parse import unquote, urlparse
from xml.etree import ElementTree as ET
import yaml
try:
    from .build_marketing_site import BASE, PAGES, ROOT, build
except ImportError:
    from build_marketing_site import BASE, PAGES, ROOT, build


class Page(HTMLParser):
    def __init__(self, source: str):
        super().__init__(convert_charrefs=True)
        self.ids=[];self.h1=0;self.title='';self.in_title=False;self.meta={};self.canonicals=[];self.refs=[];self.json=[];self.in_json=False;self.payload='';self.forms=0;self.buttons=[];self.commands={}
        self.feed(source)
    def handle_starttag(self,tag,attrs):
        a=dict(attrs)
        if a.get('id'):self.ids.append(a['id'])
        if tag=='h1':self.h1+=1
        if tag=='title':self.in_title=True
        if tag=='meta':self.meta[a.get('name') or a.get('property')]=a.get('content','')
        if tag=='link' and a.get('rel')=='canonical':self.canonicals.append(a.get('href'))
        if tag=='a' and a.get('href'):self.refs.append(a['href'])
        if tag in ('script','img') and a.get('src'):self.refs.append(a['src'])
        if tag=='link' and a.get('rel') in ('stylesheet','icon'):self.refs.append(a['href'])
        if tag=='script' and a.get('type')=='application/ld+json':self.in_json=True;self.payload=''
        if tag=='form':self.forms+=1
        if tag=='button':self.buttons.append(a)
        if a.get('data-command'):self.commands[a['data-command']]=a['data-status']
    def handle_endtag(self,tag):
        if tag=='title':self.in_title=False
        if tag=='script' and self.in_json:
            self.json.append(json.loads(self.payload));self.in_json=False
    def handle_data(self,data):
        if self.in_title:self.title+=data
        if self.in_json:self.payload+=data


def validate(root: Path = ROOT, built_dir: Path | None = None) -> list[str]:
    site=built_dir or root/'docs/site';errors=[];titles=set();descriptions=set()
    for route,*_ in PAGES:
        path=site/route/'index.html'
        if not path.is_file():errors.append(f'missing page: {route}');continue
        source=path.read_text();page=Page(source)
        if page.h1 != 1:errors.append(f'{route}: expected one h1')
        if not page.title or page.title in titles:errors.append(f'{route}: missing/duplicate title')
        titles.add(page.title)
        description=page.meta.get('description','')
        if not description or description in descriptions:errors.append(f'{route}: missing/duplicate description')
        descriptions.add(description)
        if page.canonicals != [BASE+route]:errors.append(f'{route}: wrong canonical')
        if page.meta.get('og:url') != BASE+route:errors.append(f'{route}: wrong Open Graph URL')
        if page.meta.get('robots') != 'index,follow':errors.append(f'{route}: indexing policy changed')
        if len(page.ids)!=len(set(page.ids)):errors.append(f'{route}: duplicate element IDs')
        if not page.json:errors.append(f'{route}: missing structured data')
        if page.forms:errors.append(f'{route}: local planner must not submit a form')
        if '{{' in source:errors.append(f'{route}: unresolved template placeholder')
        if any(a.get('type')!='button' for a in page.buttons):errors.append(f'{route}: implicit submit button')
        for ref in page.refs:
            u=urlparse(ref)
            if u.scheme or u.netloc:
                if u.scheme not in ('https','mailto'):errors.append(f'{route}: unsafe link scheme {ref}')
                # On the built artifact, also validate reader-facing same-host docs.
                if built_dir and ref.startswith(BASE):
                    target=built_dir/unquote(ref[len(BASE):].split('#')[0])
                else:continue
            else:target=(path.parent/unquote(u.path)).resolve() if u.path else path
            if target.is_dir():target=target/'index.html'
            if not target.is_file():errors.append(f'{route}: missing linked file {ref}');continue
            if u.fragment and target.suffix=='.html' and unquote(u.fragment) not in Page(target.read_text()).ids:
                errors.append(f'{route}: missing linked anchor {ref}')
    matrix=yaml.safe_load((root/'docs/command-matrix.yml').read_text())
    catalogue=site/'commands/index.html'
    if catalogue.exists():
        expected={c['name']:c['status'] for c in matrix['commands']}
        if Page(catalogue.read_text()).commands!=expected:errors.append('catalogue differs from authoritative matrix')
    locations={el.text for el in ET.parse(site/'sitemap.xml').iter() if el.tag.endswith('loc')}
    for route,*_ in PAGES:
        if BASE+route not in locations:errors.append(f'sitemap missing {route}')
    for doc in (root/'docs').glob('*.rst'):
        if doc.stem!='index' and BASE+doc.stem+'.html' not in locations:errors.append(f'sitemap missing {doc.name}')
    if any(not s.startswith(BASE) for s in locations):errors.append('sitemap has foreign canonical host')
    config=yaml.safe_load((root/'.readthedocs.yaml').read_text())
    if 'rust' in config['build']['tools']:errors.append('docs must not request an unnecessary Rust toolchain')
    if not config['sphinx'].get('fail_on_warning'):errors.append('strict docs gate disabled')
    if 'deploy_to_sphinx' not in (root/'docs/conf.py').read_text():errors.append('marketing output is not connected to Sphinx')
    js=(site/'assets/site.js').read_text()
    for needle in ['fetch(', 'XMLHttpRequest','localStorage','sessionStorage','.innerHTML','eval(']:
        if needle in js:errors.append(f'planner must not use {needle}')
    home=(site/'index.html').read_text()
    for term in ['not a full Picard suite','Small-fixture results','evidence/','commands/','evaluate/']:
        if term not in home:errors.append(f'home omits required visible scope/navigation: {term}')
    if build(root,check=True):errors.append('generated site is stale')
    return errors


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--built-dir',type=Path)
    args=parser.parse_args()
    errors=validate(built_dir=args.built_dir)
    if errors:
        print('\n'.join(errors),file=sys.stderr);return 1
    print('Marketing site: all pages, catalogue, sitemap, privacy and delivery checks passed');return 0


if __name__=='__main__':raise SystemExit(main())
