# Production-scale benchmark evidence

This directory is for workflow-sized evidence. It is separate from the fast
synthetic benchmark suite.

A release-evidence submission needs:

- manifest-entry.json following manifest-entry.example.json;
- raw command logs;
- Picard and Turbo-Picard versions and commits;
- immutable input source and local SHA-256;
- host, CPU, RAM, storage, container/conda image and thread settings;
- at least five timed repeats with median and p95;
- peak RSS and temporary-disk measurements;
- parity result and comparator method;
- a scope caveat describing what the dataset does and does not represent.

Recommended profiles:

- wgs_30x: coordinate-sorted 30x human WGS;
- wes_capture: bait/target metrics and duplicate marking;
- rna_seq: spliced alignments and alignment metrics;
- umi_panel: barcode/UMI duplicate-marking behaviour;
- cram_reference: reference-backed CRAM;
- multi_library: multiple lanes, read groups and libraries;
- cohort_batch: at least 25 samples.

Run quick regression evidence with:

    python3 tools/bench_suite.py --repeats 5 --skip-build \
      --profile-output benchmarks/production/synthetic-profile.json

For a production audit, use tools/audit_real_data.py, retain the generated
bundle beside the manifest, then run:

    python3 tools/verify_real_data_evidence.py --release-ready
    python3 tools/validate_production_manifest.py manifest-entry.json

If Turbo-Picard loses a profile, publish the loss and the next bottleneck.
Negative results are useful compatibility and product evidence.

For production-scale MarkDuplicates comparisons against Picard, samtools and
FastDup, use the auditable runner documented in
`benchmarks/markduplicates-competitors/README.md`. It preserves failed and
unavailable competitors, raw resource logs, executable hashes and streaming
parity evidence instead of emitting an unsupported headline speedup.
