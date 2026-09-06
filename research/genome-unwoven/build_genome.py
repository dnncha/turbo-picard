#!/usr/bin/env python3
"""Build Genome Unwoven from UCSC tables. Standard library only; bounded by one chromosome.
Coordinates: 0-based, half-open. Coverage is disjoint; family unions are non-additive.
"""
from __future__ import annotations
import argparse, array, collections, datetime, gzip, hashlib, json, os, pathlib
import resource, shutil, sys, tempfile, time, urllib.request

CHROMS = [f'chr{i}' for i in range(1, 23)] + ['chrX', 'chrY']
FINE = 100_000
COARSE = 1_000_000
SOURCES = {
    'rmsk': 'https://hgdownload.soe.ucsc.edu/goldenPath/hg38/database/rmsk.txt.gz',
    'gap': 'https://hgdownload.soe.ucsc.edu/goldenPath/hg38/database/gap.txt.gz',
    'sizes': 'https://hgdownload.soe.ucsc.edu/goldenPath/hg38/bigZips/hg38.chrom.sizes',
}
# Source hashes are frozen after the first full-corpus validation run.
PINNED_SHA256: dict[str, str] = {}
CATEGORIES = [
    ('LINE', 'LINE', '#e6ae68', 'Long interspersed elements. Many derive from sequences that copied through RNA.'),
    ('SINE', 'SINE', '#71c9ba', 'Short interspersed elements, including Alu. They depend on machinery encoded elsewhere.'),
    ('LTR', 'LTR', '#b69bd8', 'Long-terminal-repeat elements. This broad class includes endogenous retroviral sequences; it is not synonymous with viruses.'),
    ('DNA', 'DNA transposons', '#ea9489', 'Sequences annotated as DNA transposons. Annotation alone does not establish present-day activity.'),
    ('satellite', 'Satellites', '#b7c77b', 'Repeat arrays, often associated with specialised chromosome regions.'),
    ('simple', 'Simple / low complexity', '#c7b591', 'Short tandem repeats and low-complexity sequence, not all mobile-element derived.'),
    ('other', 'Other repeats', '#8ea7bd', 'RNA, rolling-circle, retroposon, unknown and other repeat classes, kept separate from the four main classes.'),
    ('overlap', 'Multiple classes', '#dedbd1', 'Bases covered by annotations in more than one displayed repeat category. Counted once here.'),
    ('unannotated', 'No repeat annotation', '#36464b', 'Sequence without a RepeatMasker annotation in this snapshot. This does not mean no biological function.'),
    ('gap', 'Assembly gaps', '#16262b', 'Regions in the UCSC gap track. They are not treated as ordinary unannotated sequence.'),
]

def category(raw: str) -> int:
    return {'LINE': 0, 'SINE': 1, 'LTR': 2, 'DNA': 3, 'Satellite': 4,
            'Simple_repeat': 5, 'Low_complexity': 5}.get(raw, 6)

def dump(path: pathlib.Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, separators=(',', ':'), ensure_ascii=False) + '\n')

def sha256(path: pathlib.Path) -> str:
    h = hashlib.sha256()
    with path.open('rb') as f:
        for chunk in iter(lambda: f.read(1 << 20), b''):
            h.update(chunk)
    return h.hexdigest()

def download(name: str, cache: pathlib.Path) -> tuple[pathlib.Path, dict]:
    dest = cache / SOURCES[name].rsplit('/', 1)[1]
    if not dest.exists():
        for attempt in range(3):
            partial = dest.with_suffix(dest.suffix + '.partial')
            try:
                req = urllib.request.Request(SOURCES[name], headers={'User-Agent': 'GenomeUnwoven/1.0 (cheerfulduck.com; reproducible research visualisation)'})
                with urllib.request.urlopen(req, timeout=120) as src, partial.open('wb') as out:
                    shutil.copyfileobj(src, out, 1 << 20)
                partial.replace(dest)
                break
            except Exception:
                partial.unlink(missing_ok=True)
                if attempt == 2:
                    raise
                time.sleep(2 ** attempt)
    checksum = sha256(dest)
    if name in PINNED_SHA256 and checksum != PINNED_SHA256[name]:
        raise ValueError(f'Source changed: {name}; refusing an unreviewed snapshot')
    return dest, {'id': name, 'url': SOURCES[name], 'sha256': checksum, 'bytes': dest.stat().st_size}

def aggregate(size: int, records, gaps, family_count: int, bin_size: int = FINE) -> dict:
    """Sweep grouped endpoints, tracking multiplicity. Never allocate per-base arrays.
    records yields (start, end, category, family_id, milliDiv).
    """
    events = []
    cats, fams, divs = array.array('B'), array.array('i'), array.array('H')
    fragments = [0] * 10
    family_fragments = [0] * family_count
    for start, end, cat, fam, div in records:
        if not (0 <= start < end <= size and 0 <= cat < 7 and 0 <= fam < family_count and 0 <= div <= 1000):
            raise ValueError(f'Invalid record: {start}, {end}, {cat}, {fam}, {div}; size={size}')
        i = len(cats)
        cats.append(cat); fams.append(fam); divs.append(div)
        events.extend(((start << 32) | (i << 1), (end << 32) | (i << 1) | 1))
        fragments[cat] += 1; family_fragments[fam] += 1
    for start, end in gaps:
        if not 0 <= start < end <= size:
            raise ValueError('Invalid gap coordinates')
        i = len(cats)
        cats.append(9); fams.append(-1); divs.append(0)
        events.extend(((start << 32) | (i << 1), (end << 32) | (i << 1) | 1))
    if len(cats) >= 2 ** 31:
        raise ValueError('Endpoint encoding capacity exceeded')
    events.sort()
    bins = [[0] * 10 for _ in range((size + bin_size - 1) // bin_size)]
    totals = [0] * 10
    histogram = [[0] * 101 for _ in range(7)]
    hist_excluded = 0
    family_bp = [0] * family_count
    active = [0] * 10
    active_families: dict[int, int] = {}
    active_records: set[int] = set()
    repeat_in_gap = 0
    previous = 0

    def span(start: int, end: int) -> None:
        nonlocal hist_excluded, repeat_in_gap
        length = end - start
        if not length:
            return
        present = [c for c in range(7) if active[c]]
        c = 9 if active[9] else (7 if len(present) > 1 else (present[0] if present else 8))
        totals[c] += length
        p = start
        while p < end:
            b = p // bin_size
            q = min(end, (b + 1) * bin_size)
            bins[b][c] += q - p
            p = q
        if c == 9:
            if present:
                repeat_in_gap += length
            return
        for f in active_families:
            family_bp[f] += length
        if present:
            if len(active_records) == 1:
                i = next(iter(active_records))
                histogram[cats[i]][min(100, divs[i] // 10)] += length
            else:
                hist_excluded += length

    for event in events:
        pos = event >> 32
        if pos != previous:
            span(previous, pos)
            previous = pos
        rid = (event & 0xffffffff) >> 1
        delta = -1 if event & 1 else 1
        c = cats[rid]
        active[c] += delta
        if active[c] < 0:
            raise AssertionError('Negative active interval count')
        if c != 9:
            f = fams[rid]
            active_families[f] = active_families.get(f, 0) + delta
            if not active_families[f]:
                del active_families[f]
            if delta == 1:
                active_records.add(rid)
            else:
                active_records.remove(rid)
    span(previous, size)
    assert not any(active) and not active_records
    assert sum(totals) == size
    assert all(sum(b) == min(bin_size, size - i * bin_size) for i, b in enumerate(bins))
    assert sum(map(sum, histogram)) + hist_excluded == sum(totals[:8])
    return {'bins': bins, 'totals': totals, 'fragments': fragments, 'histogram': histogram,
            'histExcluded': hist_excluded, 'familyBp': family_bp, 'familyFragments': family_fragments,
            'repeatInGap': repeat_in_gap}

def coarsen(bins, factor=10):
    return [[sum(row[c] for row in bins[i:i + factor]) for c in range(10)]
            for i in range(0, len(bins), factor)]

def build(cache: pathlib.Path, output: pathlib.Path) -> None:
    started = time.monotonic()
    cache.mkdir(parents=True, exist_ok=True)
    files, sources = {}, []
    for name in SOURCES:
        files[name], info = download(name, cache)
        sources.append(info)
        print(f'{name}: {info["bytes"]:,} bytes sha256={info["sha256"]}', flush=True)
    sizes = {}
    for line in files['sizes'].read_text().splitlines():
        chrom, n = line.split()
        if chrom in CHROMS:
            sizes[chrom] = int(n)
    assert set(sizes) == set(CHROMS)
    gaps = collections.defaultdict(list)
    with gzip.open(files['gap'], 'rt') as f:
        for line in f:
            cols = line.rstrip('\n').split('\t')
            if cols[1] in sizes:
                gaps[cols[1]].append((int(cols[2]), int(cols[3])))
    with tempfile.TemporaryDirectory(dir=cache, prefix='partition-') as tmp:
        root = pathlib.Path(tmp)
        handles = {c: (root / c).open('w') for c in CHROMS}
        families, family_index = [], {}
        raw_classes = collections.Counter()
        excluded_rows = 0
        try:
            with gzip.open(files['rmsk'], 'rt') as f:
                for line in f:
                    cols = line.rstrip('\n').split('\t')
                    if len(cols) != 17:
                        raise ValueError('Unexpected UCSC rmsk schema')
                    chrom = cols[5]
                    if chrom not in sizes:
                        excluded_rows += 1
                        continue
                    raw, family = cols[11], cols[12]
                    key = (raw, family)
                    if key not in family_index:
                        family_index[key] = len(families)
                        families.append({'id': len(families), 'name': family, 'originalClass': raw, 'category': category(raw)})
                    fid = family_index[key]
                    handles[chrom].write(f'{cols[6]}\t{cols[7]}\t{category(raw)}\t{fid}\t{cols[2]}\n')
                    raw_classes[raw] += 1
        finally:
            for f in handles.values():
                f.close()
        output.parent.mkdir(parents=True, exist_ok=True)
        stage = pathlib.Path(tempfile.mkdtemp(dir=output.parent, prefix='.genome-stage-'))
        total = [0] * 10
        fragments = [0] * 10
        hist = [[0] * 101 for _ in range(7)]
        fam_bp, fam_counts = [0] * len(families), [0] * len(families)
        chromosomes, overview = [], []
        excluded, repeat_gap = 0, 0
        try:
            for c in CHROMS:
                with (root / c).open() as f:
                    result = aggregate(sizes[c], (tuple(map(int, l.split())) for l in f), gaps[c], len(families))
                fine = {'schemaVersion': 1, 'chromosome': c, 'size': sizes[c], 'binSize': FINE, 'bins': result['bins']}
                dump(stage / f'{c}.json', fine)
                coarse = coarsen(result['bins'], COARSE // FINE)
                assert [sum(row[k] for row in coarse) for k in range(10)] == result['totals']
                overview.append({'id': c, 'size': sizes[c], 'bins': coarse})
                chromosomes.append({'id': c, 'size': sizes[c], 'totals': result['totals'], 'fragments': sum(result['fragments'])})
                for k in range(10):
                    total[k] += result['totals'][k]
                    fragments[k] += result['fragments'][k]
                for k in range(7):
                    hist[k] = [a + b for a, b in zip(hist[k], result['histogram'][k])]
                for k in range(len(families)):
                    fam_bp[k] += result['familyBp'][k]
                    fam_counts[k] += result['familyFragments'][k]
                excluded += result['histExcluded']; repeat_gap += result['repeatInGap']
                print(f'{c}: {sizes[c]:,} bp, {sum(result["fragments"]):,} fragments, exact reconciliation passed', flush=True)
            for fam in families:
                k = fam['id']; fam['bp'] = fam_bp[k]; fam['fragments'] = fam_counts[k]
            cats = [{'id': i, 'key': key, 'label': label, 'color': color, 'description': desc,
                     'bp': total[i], 'fragments': fragments[i], 'histogram': hist[i] if i < 7 else None}
                    for i, (key, label, color, desc) in enumerate(CATEGORIES)]
            manifest = {'schemaVersion': 1, 'datasetVersion': 'hg38-ucsc-2026-09-v1',
                        'assembly': 'GRCh38 / hg38', 'coordinateSystem': '0-based, half-open',
                        'referenceSpan': sum(sizes.values()), 'nonGapSpan': sum(sizes.values()) - total[9],
                        'repeatBp': sum(total[:8]), 'annotationFragments': sum(fragments),
                        'categories': cats, 'chromosomes': chromosomes,
                        'families': sorted(families, key=lambda v: (-v['bp'], v['id'])),
                        'histogramExcludedBp': excluded, 'repeatAnnotationInsideGapBp': repeat_gap,
                        'excludedNonPrimaryRows': excluded_rows, 'rawClasses': dict(raw_classes),
                        'sources': sources, 'builtAt': datetime.datetime.now(datetime.timezone.utc).isoformat(),
                        'validation': {'allChromosomesReconcile': True, 'fineCoarseAgree': True,
                                       'histogramReconciles': True, 'peakRssMiB': round(resource.getrusage(resource.RUSAGE_SELF).ru_maxrss / (1024 if sys.platform != 'darwin' else 1024**2), 1),
                                       'wallSeconds': round(time.monotonic() - started, 2)}}
            dump(stage / 'manifest.json', manifest)
            dump(stage / 'overview.json', {'schemaVersion': 1, 'binSize': COARSE, 'chromosomes': overview})
            with (stage / 'coverage.csv').open('w') as f:
                f.write('category,covered_base_pairs,reference_span_base_pairs,annotation_fragments\n')
                for c in cats:
                    f.write(f'{c["label"]},{c["bp"]},{manifest["referenceSpan"]},{c["fragments"]}\n')
            if output.exists():
                shutil.rmtree(output)
            stage.replace(output)
            print(json.dumps({'totals': total, 'manifest': {k: manifest[k] for k in ('referenceSpan','repeatBp','annotationFragments','validation')}}), flush=True)
        except BaseException:
            shutil.rmtree(stage, ignore_errors=True)
            raise

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cache', type=pathlib.Path, default=pathlib.Path('.cache/genome-unwoven'))
    parser.add_argument('--output', type=pathlib.Path, default=pathlib.Path('public/genome-unwoven/data/v1'))
    args = parser.parse_args()
    build(args.cache, args.output)
