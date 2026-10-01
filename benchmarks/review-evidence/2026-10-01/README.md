# Scientific duplicate-planning evidence — 1 October 2026

Source: `d14b74494dde26fe280f7bfac3531661e017cdce`, tree
`649af81b448c9a9efc66f736204b104a669dc417`. Baseline for the local
before/after comparison: `f04ba1810055bbbf78db6995f4bf6f7cfebfccc6`,
the preceding PR revision, rather than a released binary.

The source joins nearby mates in a bounded cache, preserves the exact stable
QNAME fallback, avoids repeated singleton allocations, lowers the paired sort
window to 96 MiB and avoids fragment sorting when no unpaired candidate exists.
It fixes missing single-library coverage histograms, inappropriate multi-library
ROI pooling and nonnumeric/superfluous family-size bins.

## Sequential before/after measurements

One warm-up plus five alternating measured repeats; one pinned CPU; compression
level 1; native execution required; HTS workers disabled; Picard 3.4.0 uses a
2 GiB heap and one active processor. All 36 outputs pass the complete contract.

| Synthetic paired workload | Baseline seconds | Candidate seconds | Runtime reduction | Baseline RSS MB | Candidate RSS MB | RSS reduction |
|---|---:|---:|---:|---:|---:|---:|
| 600k reads | 2.8294 | 2.6282 | 7.1% | 285.0 | 250.1 | 12.3% |
| 1.2m reads | 5.6174 | 4.7395 | 15.6% | 285.1 | 262.9 | 7.8% |

RSS uses decimal MB; the JSON retains exact byte values and every repetition.
These are highly compressible synthetic 150-base paired reads, not WGS.

## Current competitors from identical coordinate BAMs

One warm-up plus three alternating repeats; one pinned CPU; compression level
5. Required collation, fixmate and final coordinate sorting are timed for
samtools and dupblaster. Their Python pipeline-wrapper startup is also included,
which can matter on the small public fixture. Versions are Picard 3.4.0,
FastDup 1.0.0, samtools 1.24 and dupblaster 0.3.0; source and binary hashes are
recorded in the reports and provenance.

| Tool | Paired-only 600k seconds | Full Picard contract | Public NA12878 mitochondrial seconds | Full Picard contract |
|---|---:|---|---:|---|
| Turbo Picard | 2.5743 | Pass | 0.2474 | Pass |
| Picard | 5.4795 | Oracle | 2.5250 | Oracle |
| FastDup | 1.2256 | Different tags and metrics | 0.2008 | Different tags and metrics |
| samtools pipeline | 2.0752 | Different tags and metrics | 0.2597 | Different alignment fields and metrics |
| dupblaster pipeline | 1.6450 | Different output order/metrics | No successful run | Strict mode rejects missing mates |

FastDup, samtools and dupblaster match the unordered mandatory-field/RG diagnostic
on the clean paired fixture. This is a Picard compatibility contract, not a claim
that tools using different output schemas are scientifically inaccurate. The
faster alternative timings remain visible; Turbo Picard does not win every
runtime comparison. dupblaster is run in its default strict missing-mate mode;
relaxing that policy was not tested here. The public fixture contains 14,917
records and does not establish production throughput or cohort accuracy.

## Comparison and validation

The named **bounded-v4** contract checks ordered mandatory SAM fields, all typed
auxiliary tags except PG provenance, SQ/RG headers, the DuplicationMetrics table
and every numeric histogram column/bin. RG dates with explicit zones compare as
exact instants because Picard reformats them in the JVM timezone. Different
instants, all other header fields and alignment DT tags remain distinct;
malformed dates and excess fractional precision remain unnormalized. Independent
tests guard this rule. Histogram normalization changes numeric spelling only.
The SAM specification defines RG DT as an ISO8601 run date/date-time:
https://samtools.github.io/hts-specs/SAMv1.pdf.

Local validation passes 431 Rust tests (one existing ignored), 506 Python tests
(one environment-dependent skip), formatting, required verifiers and the pinned
strict Sphinx build. CI on the exact source head is tracked in
https://github.com/dnncha/turbo-picard/actions/runs/36926340424 and the eleven-case
bounded comparison in https://github.com/dnncha/turbo-picard/actions/runs/36926340419.

The six-case core-sort CI comparison against the actual PR base preserves all
96 exact hashes. Its final medians range from a 0.37% loss to a 1.9% gain; they
do not establish a material overall sorter speedup. An earlier run recorded a
5.8% ordered-case loss, retained in the PR history.

## Files and limits

`bounded-summary.json` and `landscape-summary.json` retain every command,
measurement and outcome. `raw-evidence.tar.gz` contains unmodified complete
reports, numeric metrics, resources, stdout/stderr and local check logs.
`provenance.json` records sources, host details, the original public input
manifest and the archive hash. The archive excludes BAM inputs/outputs, scratch
files and generated plots. Failed outputs remain in the benchmark work folders;
their summaries, command lines and failure logs are archived.

The paired-only competitor input is produced by `samtools view -bh -f 1` from
the deterministic `paired-600k` generator, excluding its independent secondary,
supplementary and unmapped test records. Original inputs are immutable.

Overlapping exploratory jobs shared the pinned CPU; their times are excluded
from these claims and identified in provenance. Final measurements above run
sequentially, use warm page caches, omit fsync and do not control other tenants.
Pipeline RSS is GNU time's maximum child RSS rather than concurrent aggregate
memory. Confidence intervals, WGS/WES/UMI/cohort reliability, variant-calling
impact and universal SOTA have not been established.
