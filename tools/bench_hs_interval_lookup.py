#!/usr/bin/env python3
"""Compile the unchanged production interval-index section in a synthetic harness.

This measures interval lookup only, not CollectHsMetrics or WES throughput.
"""
from __future__ import annotations

import argparse
import csv
import hashlib
import platform
import subprocess
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SOURCE = "crates/turbo-picard-cli/src/hs_metrics.rs"
HARNESS = r'''
use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::BTreeMap;
use std::hint::black_box;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;
struct CountingAllocator;
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn main() {
    let queries: usize = std::env::args().nth(1).unwrap().parse().unwrap();
    let spans: Vec<Span> = (0..4096).map(|i| Span {
        contig: "chr1".into(), start: i * 200, end: i * 200 + 150, name: i.to_string(),
    }).collect();
    let index = IntervalIndex::from_spans(&spans);
    let mut checksum = 0usize;
    ALLOCATIONS.store(0, Ordering::Relaxed);
    let started = Instant::now();
    for i in 0..queries {
        checksum = checksum.wrapping_add(black_box(&index).index_at(
            black_box("chr1"), black_box((i as u64 * 31) % 819200),
        ).map(|position| position + 1).unwrap_or(0));
    }
    let seconds = started.elapsed().as_secs_f64();
    let allocations = ALLOCATIONS.load(Ordering::Relaxed);
    println!("{seconds},{allocations},{checksum}");
}
'''


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline-ref", required=True)
    parser.add_argument("--rustc", default="rustc")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--queries", type=int, default=1000000)
    parser.add_argument("--repetitions", type=int, default=5)
    args = parser.parse_args()
    if args.queries <= 0 or args.repetitions <= 0:
        parser.error("queries and repetitions must be positive")
    sources = {
        "baseline": subprocess.check_output(["git", "show", f"{args.baseline_ref}:{SOURCE}"], cwd=ROOT),
        "candidate": (ROOT / SOURCE).read_bytes(),
    }
    rustc_version = subprocess.check_output([args.rustc, "--version"], text=True).strip()
    rows = []
    with tempfile.TemporaryDirectory() as directory:
        binaries = {}
        for variant, source in sources.items():
            section = source.decode().split("#[derive(Debug, Clone)]\nstruct Span {", 1)[1]
            section = "#[derive(Debug, Clone)]\nstruct Span {" + section.split("#[derive(Debug)]\nstruct TargetCoverage", 1)[0]
            harness = Path(directory) / f"{variant}.rs"
            harness.write_text(HARNESS + section)
            binaries[variant] = Path(directory) / variant
            subprocess.run([args.rustc, "--edition=2024", "-O", "-A", "dead_code", str(harness), "-o", str(binaries[variant])], check=True)
        expected_checksum = None
        for repetition in range(args.repetitions):
            for variant in list(sources)[::1 if repetition % 2 == 0 else -1]:
                seconds, allocations, checksum = subprocess.check_output([str(binaries[variant]), str(args.queries)], text=True).strip().split(",")
                if expected_checksum is None:
                    expected_checksum = checksum
                assert checksum == expected_checksum, "lookup results changed"
                rows.append({
                    "variant": variant, "repetition": repetition + 1, "queries": args.queries,
                    "wall_seconds": seconds, "allocations": allocations, "checksum": checksum,
                    "source_sha256": hashlib.sha256(sources[variant]).hexdigest(),
                    "harness_sha256": hashlib.sha256(HARNESS.encode()).hexdigest(),
                    "rustc": rustc_version, "platform": platform.platform(), "optimization": "-O",
                })
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("w", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=list(rows[0]))
        writer.writeheader()
        writer.writerows(rows)


if __name__ == "__main__":
    main()
