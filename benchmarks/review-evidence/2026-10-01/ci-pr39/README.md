# PR #39 bounded comparison in CI

Original run: https://github.com/dnncha/turbo-picard/actions/runs/36926340419

Candidate source: `d14b74494dde26fe280f7bfac3531661e017cdce`. Baseline source: `9ad15c17bb6179fca51520a7118e306e1157c6bd`.
The source was versioned 0.1.15 during testing; v0.1.16 packages the merged implementation.

All eleven candidate cases and all 44 candidate outputs pass the named bounded-v4 contract.
The baseline's nine histogram failures are retained; they are not parity-qualified speed claims.
Three alternating measured repeats after one warm-up, one pinned CPU, compression level 1,
HTS workers disabled, warm page cache, no fsync; synthetic fixtures, not WGS/cohort evidence.
RSS byte counts are exact in JSON. MB in the release notes denotes 1,000,000 bytes.

`summary.json` retains all provenance, input hashes and case summaries.
`ci-pr39-evidence.tar.gz` retains the unmodified full report, metrics and logs from the CI
artifact, excluding regenerable BAM/SAM files. No measurements are discarded.

Archive SHA-256: `3d73f7d350c022efa989b55f72ec5c6331152e85dab683396ca1925fbba63d37`.
Original artifact ID: 11194203723, ZIP SHA-256:
`f090de64298911a9634e042f5aa7498adf8f902eb77ab651815471bd4aad0dd2`.
