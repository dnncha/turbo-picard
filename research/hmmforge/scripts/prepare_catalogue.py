"""Explicitly acquire a versioned Pfam catalogue and emit a reproducibility lock.

No network requests occur in normal annotation. First acquisition records an
observed SHA256 over HTTPS; it is not a publisher-signed checksum. Later runs
should pass --expected-sha256. No fallback to current_release or another version.
"""
from __future__ import annotations

import argparse
import gzip
import hashlib
import json
import os
import re
import tempfile
import urllib.request
from pathlib import Path


def validate_release(release):
    if not re.fullmatch(r"\d{2}\.\d+", release):
        raise ValueError("release must be a fixed numeric version such as 38.0")
    return release


def acquire(destination, release="38.0", expected=None, max_download_bytes=800_000_000,
            max_uncompressed_bytes=4_000_000_000):
    validate_release(release)
    if expected is not None and not re.fullmatch(r"[0-9a-f]{64}", expected):
        raise ValueError("expected-sha256 must be 64 lowercase hexadecimal characters")
    if destination.exists():
        raise FileExistsError(f"destination already exists: {destination}")
    destination.parent.mkdir(parents=True, exist_ok=True)
    url = f"https://ftp.ebi.ac.uk/pub/databases/Pfam/releases/Pfam{release}/Pfam-A.hmm.gz"
    with tempfile.TemporaryDirectory(prefix=".pfam-acquire-", dir=destination.parent) as temporary:
        root = Path(temporary)
        compressed = root/"Pfam-A.hmm.gz"
        digest, size = hashlib.sha256(), 0
        request = urllib.request.Request(url, headers={"User-Agent": "HMMForge-research/0.1"})
        with urllib.request.urlopen(request, timeout=90) as response, compressed.open("wb") as out:
            if not response.geturl().startswith("https://ftp.ebi.ac.uk/"):
                raise ValueError("unexpected download redirect")
            while chunk := response.read(1024*1024):
                size += len(chunk)
                if size > max_download_bytes:
                    raise ValueError("compressed download exceeds the configured size limit")
                digest.update(chunk)
                out.write(chunk)
        observed = digest.hexdigest()
        if expected is not None and observed != expected:
            raise ValueError("catalogue SHA256 differs from the supplied lock")
        uncompressed_hash, total = hashlib.sha256(), 0
        names, headers, ends = set(), 0, 0
        with gzip.open(compressed, "rb") as source, (root/"models.hmm").open("wb") as out:
            for line in source:
                total += len(line)
                if total > max_uncompressed_bytes:
                    raise ValueError("uncompressed catalogue exceeds the configured size limit")
                if line.startswith(b"HMMER3/"):
                    headers += 1
                elif line.startswith(b"NAME  "):
                    name = line.split()[1]
                    if name in names:
                        raise ValueError("duplicate model name in catalogue")
                    names.add(name)
                elif line.strip() == b"//":
                    ends += 1
                uncompressed_hash.update(line)
                out.write(line)
        if not headers or headers != len(names) or headers != ends:
            raise ValueError("incomplete or structurally inconsistent HMM catalogue")
        manifest = dict(schema="hmmforge.catalogue-lock.v1", source_url=url,
                        release=release, compressed_sha256=observed,
                        models_sha256=uncompressed_hash.hexdigest(), models=headers,
                        compressed_bytes=size, uncompressed_bytes=total,
                        expected_sha256_verified=expected is not None,
                        acquisition="HTTPS; first observation is not publisher authentication",
                        scope="complete downloaded model catalogue; no model selection")
        compressed.unlink()  # Avoid keeping an unnecessary second large file.
        destination.mkdir(exist_ok=False)
        try:
            os.replace(root/"models.hmm", destination/"models.hmm")
            (destination/"catalogue-lock.json").write_text(json.dumps(manifest, indent=2)+"\n")
        except BaseException:
            # An interrupted acquisition is never represented by a completed lock.
            (destination/"catalogue-lock.json").unlink(missing_ok=True)
            raise
        return manifest


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("destination", type=Path)
    p.add_argument("--release", default="38.0")
    p.add_argument("--expected-sha256")
    args = p.parse_args()
    print(json.dumps(acquire(args.destination, args.release, args.expected_sha256), sort_keys=True))
