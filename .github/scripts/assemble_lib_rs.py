#!/usr/bin/env python3
import urllib.request, pathlib
chunks = []
for i in range(37):
    name = f'c{i:02d}'
    url = f'https://raw.githubusercontent.com/dnncha/turbo-picard/cursor/markdup-compact-ordinal-replay-1c0b/.cursor-agent/chunks/{name}'
    chunks.append(urllib.request.urlopen(url).read())
pathlib.Path('crates/turbo-picard-markdup/src/lib.rs').write_bytes(b''.join(chunks))
print(len(b''.join(chunks)))
