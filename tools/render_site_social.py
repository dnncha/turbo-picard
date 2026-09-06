#!/usr/bin/env python3
"""Render the authored 1200 x 630 social card; only needed when its design changes."""
from pathlib import Path
import argparse
from playwright.sync_api import sync_playwright
ROOT=Path(__file__).resolve().parents[1]
def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--chromium')
    args=parser.parse_args()
    with sync_playwright() as p:
        kw={'headless':True,'args':['--no-sandbox']}
        if args.chromium:kw['executable_path']=args.chromium
        browser=p.chromium.launch(**kw)
        page=browser.new_page(viewport={'width':1200,'height':630},device_scale_factor=1)
        page.set_content((ROOT/'website/social-card.html').read_text())
        page.screenshot(path=str(ROOT/'website/assets/social-card.png'))
        browser.close()
if __name__=='__main__':main()
