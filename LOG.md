# Turbo Picard work log

## 2026-08-14 — published container tag is runtime-smoke-tested

- Extended the guarded container workflow to pull the exact version tag after
  GHCR push, then run `--version`, `doctor`, the read-only `trial` contract, and
  a real checked-in `MarkDuplicates` fixture with read-only source mounting.
- Added ordering and command-content checks to the publish-workflow verifier and
  its tests. This remains workflow proof until a tagged run executes; no image,
  tag, package, or external state was changed in this task.

## 2026-08-14 — PyPI workflow smoke-tests native macOS wheels

- Added `tools/verify_pypi_wheel_install_smoke.sh`, a reusable offline wheel
  check covering isolated installation, `pip check`, version, doctor, trial,
  compatibility shim, real MarkDuplicates output, and mate-specific barcode
  histogram behavior.
- Wired the smoke into the native macOS arm64 and Intel build jobs before wheel
  upload. Linux arm64 remains explicitly cross-built and artifact-validated;
  the workflow does not claim to execute an arm64 binary on an x86 runner.
- Ran the reusable smoke against the current arm64 `0.1.12` candidate wheel;
  it passed. The workflow change is local and awaits the next owner-controlled
  tagged release run for native CI evidence; no publication or external state
  changed.

## 2026-08-14 — distribution channel audit added

- Extended the read-only public adoption report to schema version 4. It now
  records published GitHub releases, anonymous GHCR image tags, Bioconda main
  and shim package availability, and the existing Bioconda PR state alongside
  PyPI and GitHub interest signals. No credentials or mutation paths were added.
- The live snapshot at `2026-08-14T06:03:24Z` found GitHub release `v0.1.11`
  and GHCR tag `0.1.11` as the latest public distribution versions. Neither
  Bioconda package is indexed, and PR #65922 remains open for `0.1.10`; the
  workspace `0.1.12` version is not present across those channels.
- Added parser, validator, and offline collection tests. The report remains
  explicit that channel state is observational and that no publication or
  repair was attempted.

## 2026-08-14 — public adoption audit records author provenance

- Extended the read-only public adoption report to schema version 3. It now
  separates maintainer, external, and unknown authors for open issues and the
  public trial-report thread's comments, without retaining public usernames;
  the validator and focused tests enforce the new shape and safety boundary.
- Refreshed the report at `2026-08-14T05:52:19Z`: all five sampled open issues
  and the one trial-thread comment were maintainer-authored, with zero
  externally authored issues or comments observed. `workflow_owner_trial_reports_verified`
  remains false because this is public maintainer activity, not a workflow-owner
  trial report.
- PyPI remains `0.1.11` with 605 without-mirrors downloads in the latest 30
  days and 40 in the latest 7 days; `0.1.12` remains untagged locally and on
  `origin`. This was read-only; no issue, comment, outreach, publication, or
  external-service mutation occurred.

## 2026-08-14 — outreach bundle corrected for Bioconda state

- A read-only check of Bioconda PR #65922 found it open for older `0.1.10`
  metadata, not the current `0.1.12` candidate. Updated the prepared Biostars,
  Hacker News, nf-core, Reddit, Seqera, and outreach README drafts so they do
  not imply that current Bioconda packages are available or imminent.
- The drafts now direct current users to the published PyPI `0.1.11` package or
  container and keep `0.1.12` tagging, archive, review, and publication as
  separate gates. Release-text and link checks pass; no post, issue, comment,
  outreach message, or external setting was changed.

## 2026-08-14 — representative CRAM and barcode profiles pass

- Extended the repeated competitor evidence path to the public reference-backed
  CRAM fixture with profile `cram_reference`: five repeats, 14,917 records,
  required Turbo-Picard/Picard tools, exact parity, and a validated
  `release_candidate` manifest at
  `/private/tmp/turbo-picard-profile-cram-c51faef/evidence/`.
- Ran the bounded primary barcode fixture with profile `umi_panel`,
  `BARCODE_TAG=RX`, and paired `DS`/`DI` tagging: five repeats, exact parity,
  and a validated manifest at
  `/private/tmp/turbo-picard-profile-umi-c51faef/evidence/`.
- Ran the mate-specific barcode fixture with `READ_ONE_BARCODE_TAG=BX` and
  `READ_TWO_BARCODE_TAG=BY`: five repeats, exact parity, and a validated
  manifest at `/private/tmp/turbo-picard-profile-umi-mate-c51faef/evidence/`.
  These are bounded release-candidate profile checks, not production-scale
  WGS/WES evidence or advanced UMI-normalization approval. No external
  mutation occurred.

## 2026-08-14 — production-evidence path smoke-tested end to end

- Ran the pinned competitor benchmark and manifest adapter on the public Picard
  SNVQ BAM using the current `f790814` release binary and Picard `3.4.0`.
  Five repeats completed with the required-tool gate passing and exact ordered
  duplicate/metrics parity across 26,577 records.
- Built and validated a `release_candidate` production-evidence manifest with
  profile `cohort_batch`, read count, source citation, resource measurements,
  and `independent_reproduction.status=not_run`. The bundle is retained at
  `/private/tmp/turbo-picard-production-smoke-f790814/evidence/`.
- This proves the evidence plumbing and bounded public-fixture protocol, not
  production-scale WGS/WES readiness or independent reproduction. No workflow
  dispatch, tag, push, publication, outreach, or external-service mutation
  occurred.

## 2026-08-14 — production-evidence bootstrap uses current actions

- Updated the manual production-evidence workflow's validation job from
  `actions/checkout@v4` and `actions/setup-python@v5` to the current repository
  action generations, matching the build and publication workflows. This
  removes a known stale bootstrap path before the next owner-controlled
  production-scale run.
- The workflow remains manual, pinned-input, and fail-closed; no production
  evidence was fabricated and no workflow dispatch or external mutation
  occurred.

## 2026-08-14 — comparator manifest requests fail fast

- The reviewable real-data comparator now validates manifest output layout,
  pinned source citation, duplicate command arguments, release-candidate
  command coverage, and minimum input size before starting an expensive
  comparison. This prevents a malformed trial request from consuming a full
  BAM or CRAM run before failing at manifest creation.
- Added focused coverage for invalid output and citation requests plus a valid
  release-candidate shape. The complete Python tool suite passes 413 tests with
  one skip, and release, evidence, workflow-starter, publication, text-quality,
  and adoption-report validators pass.
- Updated the one-command trial guide with the repository-ready manifest
  requirements. No tag, push, publication, issue, comment, outreach, or
  external-service mutation occurred.

## 2026-08-14 — exact committed-source package verification

- Rebuilt the `0.1.12` arm64 wheel and source distribution from the clean
  candidate source, then passed `tools/verify_release_artifacts.py` against
  those exact files. The fresh wheel's `turbo-picard` and `picard` entrypoint
  hashes match the release target binaries byte-for-byte.
- Installed the fresh wheel in an isolated virtual environment. `pip check`,
  both version entrypoints, the text `doctor` contract, JSON `trial` contract,
  the real install smoke, and the mate-specific barcode smoke all passed.
- Ran the fresh wheel through the no-`samtools` real-data comparator: five
  public SNVQ BAM commands and six reference-backed CRAM commands all passed
  exact Picard parity. The exact-source artifact manifest records the hashes,
  clean source state, benchmark summary, and remaining tag blockers.
- No tag, push, package publication, container publication, Bioconda update,
  issue, comment, outreach, or external service mutation occurred.

## 2026-08-14 — clean local release-candidate handoff

- Committed the complete current candidate changeset as
  `5d968a3ead1ad8350f60e9ff9ab937f5f65353a2`
  (`Prepare Turbo Picard 0.1.12 release candidate`) on the existing
  `codex/turbo-picard-bioconda-0-1-11` branch.
- The worktree is clean and the branch contains the local candidate commits
  ahead of `origin/main`. The release handoff manifest at
  `/private/tmp/turbo-picard-release-manifest-0.1.12-commit-5d968a3.json`
  records the candidate artifacts, 32/32 parity benchmark, and only the
  missing local and origin `v0.1.12` tags as source blockers.
- The post-commit read-only adoption audit still observes PyPI `0.1.11`,
  605 downloads in the latest 30 days and 40 in the latest 7 days, and no
  verified workflow-owner trial reports. These remain distribution signals,
  not adoption or production proof.
- No tag, push, package publication, container publication, Bioconda update,
  issue, comment, outreach, or external service mutation occurred.

## 2026-08-14 — comparison helper no longer requires host samtools

- The real-data comparison helper failed on a clean host when `samtools` was
  not on `PATH`, even though the selected Turbo Picard and Picard entrypoints
  were available. This was an adoption blocker for the documented reviewable
  trial path.
- Changed output materialization to invoke each configured Picard-compatible
  `ViewSam` entrypoint, preserving explicit reference arguments for CRAM and
  keeping the existing digest comparators unchanged. The one-command trial
  guide now documents that `samtools quickcheck` is optional manual hygiene.
- With `samtools` deliberately absent from `PATH`, the updated helper passed
  exact parity on five BAM commands and six reference-backed CRAM commands.
  Focused tests cover BAM and CRAM command construction; the full tool suite
  passes 410 tests with one skip.
- Regenerated the checked-in SNVQ candidate evidence through this path. It
  remains path-neutral and records Turbo Picard `0.1.12` versus Picard `3.4.0`,
  with all five commands passing and observed speedups of 4.10x, 4.89x,
  17.51x, 18.02x, and 6.24x respectively.
- No package publication, tag, issue, comment, outreach, or external service
  mutation occurred.

## 2026-08-14 — refreshed checked-in public SNVQ candidate evidence

- Replaced the stale checked-in SNVQ comparison, which still identified the
  Turbo Picard side as `picard 0.1.0`, with a run from the current `0.1.12`
  release binary against Picard `3.4.0`.
- All five commands passed the existing command-specific parity checks:
  `ViewSam`, `CleanSam`, `CollectQualityYieldMetrics`,
  `CollectAlignmentSummaryMetrics`, and `MarkDuplicates`. The observed
  speedups were 4.01x, 4.56x, 21.09x, 18.42x, and 5.14x respectively.
- The checked-in JSON, Markdown, and manifest remain path-neutral and retain
  the pinned public input source and raw digests. This is command-level
  release-candidate evidence only; it is not production-scale,
  workflow-owner, or independent-reproduction proof.
- The current wheel passed `pip check`, the real install smoke, the
  mate-specific barcode install smoke, and the JSON `trial` contract for the
  recommended `MarkDuplicates` shape. The repository tool suite passed 408
  tests with one skip; release-ready real-data, guardrail, adoption-report,
  release-artifact, version, text-quality, command-matrix, and whitespace
  checks also passed.
- No package publication, tag, issue, comment, outreach, or external service
  mutation occurred.

## 2026-08-14 — adoption audit separates activity from verified trials

- The public repository currently has five open issues and one comment on the
  public trial-report thread. Extended the read-only adoption report to retain
  that thread comment count as a community signal without treating it as a
  workflow-owner trial or customer-adoption claim.
- The refreshed report is retained at
  `/private/tmp/turbo-picard-public-adoption-trial-activity.json` and passes
  the public-adoption safety validator.
- Added parser, safety-boundary, and negative tests; no issue, comment,
  outreach, or repository setting was changed.

## 2026-08-14 — candidate parity regression caught and fixed

- A fresh current-source trial on the pinned public Picard SNVQ BAM initially
  failed only `CollectAlignmentSummaryMetrics`: Turbo Picard used NM tags for
  mismatch rates while Picard correctly leaves those fields at zero when no
  `REFERENCE_SEQUENCE` is supplied. The other four commands passed exact
  comparison.
- Changed the metrics path so mismatch rates require an actual reference,
  removed the obsolete no-reference NM fallback, and added a regression test
  proving that an NM tag cannot reintroduce the fields. Reference-backed
  mismatch calculation remains available.
- Rebuilt both release entrypoints. The pre-package target binary SHA-256 was
  `c1970475cafdbd863e01f6a302d3d1dcff175d52520586e5cec582b58e9fab7b`; the
  final maturin-built wheel entrypoint SHA-256 is
  `a527a8a967ac17785ed46a7e5110373ec1bd7676897c99d637e1901acffe7a69`.
  The checked-in synthetic and reference-backed CRAM guardrails were rerun
  against the final wheel binary and both passed exact parity.
- The final public SNVQ trial at
  `/private/tmp/turbo-picard-adoption-trial-0.1.12-final.boYMGc/` passed all
  five commands with Picard `3.4.0`: ViewSam 13.19x, CleanSam 9.24x,
  CollectQualityYieldMetrics 59.49x, CollectAlignmentSummaryMetrics 66.53x,
  and MarkDuplicates 9.26x. The shareable report is redacted and remains
  command-level release-candidate evidence only.
- Improved the trial report version capture so Picard's version is retained
  even when its legacy `--version` probe exits nonzero after printing it.
- No commit, tag, package publication, container publication, Bioconda update,
  issue, outreach, or external service mutation occurred.
- The final wheel trial used the clean installed `0.1.12` artifact and passed
  all five commands against Picard `3.4.0`: ViewSam 4.14x, CleanSam 5.15x,
  CollectQualityYieldMetrics 15.35x, CollectAlignmentSummaryMetrics 18.67x,
  and MarkDuplicates 5.01x. The redacted report is retained at
  `/private/tmp/turbo-picard-wheel-trial-final-0.1.12.TJDW2B/`.
- The final rebuilt artifacts are the arm64 wheel SHA-256
  `a529f3a16585cbe4662546ee849ec0149acc5f4440a41f448572e249d038990a` and
  source distribution SHA-256
  `5cf8e98f2cb8cc9310d39fafcb73e70faa1c2824171bb4fd2197a2ba7ad8bf7f`.
- The final one-repeat 32-command suite passed all parity checks with an
  83.27x geometric mean, 8.41x floor, 278.74x maximum, and 15.99x
  MarkDuplicates speedup. The profile is retained at
  `/private/tmp/turbo-picard-0.1.12-bench-profile-final.json`.
- The final handoff manifest is retained at
  `/private/tmp/turbo-picard-release-manifest-0.1.12-final.json`; it records
  the final artifact hashes, 32/32 parity, and `release_candidate` status with
  publication, production-scale, and independent-reproduction flags false.
- The final repository sweep passed 408 Python tests with one skip, all local
  release/evidence/site/package validators, the final artifact verifier, the
  refreshed guardrail verifier, public-audit schema validation, and
  `git diff --check`. Rust tests, strict Clippy, formatting, and Sphinx with
  warnings as errors had already passed after the parity fix.
- The read-only adoption audit at
  `/private/tmp/turbo-picard-public-adoption-final-2.json` observed PyPI
  `0.1.11`, 605 downloads without mirrors in the latest 30 days and 40 in the
  latest 7, plus zero GitHub stars and forks. Bioconda preflight remains
  waiting on a clean intended checkout, the exact `v0.1.12` tag, and immutable
  source-archive metadata; no external mutation was performed.

## 2026-08-14 — prepared local `0.1.12` release candidate

- Bumped the workspace, Cargo.lock, PyPI, citation, bio.tools, crate
  dependency, Bioconda recipe, and archive-helper metadata from `0.1.11` to
  `0.1.12`. The recipes intentionally retain the v0.1.12 source-archive SHA
  placeholder until the exact release tag exists and its downloaded archive is
  verified.
- Updated release validators so the source-release marker is checked
  independently from already-published older container tags. The current
  README/docs state that `0.1.11` is the latest published package/container
  while `0.1.12` is a local release candidate.
- Rebuilt the arm64 candidate package in an isolated temporary environment:
  `turbo_picard-0.1.12-py3-none-macosx_11_0_arm64.whl` and
  `turbo_picard-0.1.12.tar.gz` passed release-artifact validation. Clean install
  and install-path smokes passed, including mate-specific barcode parity.
- Rebuilt the release binary and re-ran the checked-in synthetic and
  reference-backed CRAM guardrails with binary SHA-256
  `72f3e7863b9444f695cd4f29384ead4684119ceffef350c996ac2302f6ceacce`.
  Both required-tool gates and exact parity passed; the refreshed JSON and
  README remain evidence-only and explicitly exclude production-scale and
  independent-reproduction claims.
- The live read-only adoption audit at
  `/private/tmp/turbo-picard-public-adoption-20260814-0.1.12-candidate.json`
  reports PyPI `0.1.11`, no local/origin `v0.1.12` tag, 605 downloads in the
  latest 30 days and 40 in the latest 7. The public/source mismatch is now
  explicit and fail-closed.
- Final candidate verification passed 402 Python tests with one skip,
  `cargo test --workspace --locked`, `cargo fmt --all -- --check`,
  `cargo clippy --workspace --all-targets --locked -- -D warnings`, Sphinx with
  warnings as errors, all local release/evidence/site verifiers, and workflow
  YAML parsing.
- Re-ran `python3 tools/bench_suite.py --repeats 1 --skip-build` against the
  rebuilt `0.1.12` binary. All 32 commands passed parity; the geometric mean
  speedup was 80.25x, the floor was 7.92x, the maximum was 242.94x, and
  MarkDuplicates measured 19.96x. The profile is retained at
  `/private/tmp/turbo-picard-0.1.12-bench-profile.json` as local regression
  evidence only.
- Added `CHANGELOG.md` and linked it from the README and packaging docs so a
  workflow owner can see the candidate scope and evidence boundaries before
  trying the package.
- Re-ran the full 402-test suite, Sphinx, release/evidence/site verifiers,
  README-link checks, and workflow YAML parsing after the release-note change;
  all passed.

## 2026-08-14 — release handoff manifest catches artifact drift

- Added `tools/build_release_manifest.py` with focused tests. It validates the
  distribution directory through the release-artifact verifier, records only
  artifact filenames/sizes/SHA-256 values plus source/tag state, and can attach
  a parity benchmark summary without local paths.
- The first real run caught that the wheel and sdist had been built before the
  final README-to-CHANGELOG link. Rebuilding fixed the mismatch; the final
  candidate artifacts are:
  - wheel SHA-256 `e6b1a6ccae2fcd32cd8ec7022baec01704313a52095a2fabc7ea32d9234dd9ba`
  - sdist SHA-256 `170d473d958060c064c9a0c2e944a1de27ac625442f5db133d31d94a7ea3af79`
- The manifest is retained at
  `/private/tmp/turbo-picard-release-manifest-0.1.12.json`; it reports the
  candidate status, no local/origin `v0.1.12` tag, 32/32 parity, and false
  production-scale/independent/publication flags. The PyPI workflow now uploads
  the same handoff artifact after validation.
- No commit, tag, package publication, container publication, Bioconda update,
  issue, outreach, or external service mutation occurred.
- Final post-manifest verification passed 405 Python tests with one skip, the
  workspace Cargo tests, strict Clippy, formatting, Sphinx with warnings as
  errors, the release/evidence/adoption validators, install smokes, workflow
  YAML parsing, Python compilation, and `git diff --check`.

## 2026-08-14 — public adoption audit now includes release-state proof

- Extended `tools/audit_public_adoption.py` to capture source version, current
  HEAD, branch, worktree cleanliness, local and `origin` release-tag commits,
  and explicit release-source blockers without recording local filesystem paths.
- The report schema is now version 2. Focused parser tests cover a dirty,
  mismatched tag and a clean annotated tag; the complete live audit remains
  read-only and keeps public download and repository counts as signals only.
- The latest live report at
  `/private/tmp/turbo-picard-public-adoption-20260814-release-state.json`
  confirms workspace `0.1.11` at `6c998857...`, public and origin tag `v0.1.11`
  at `f09e1e4...`, an uncommitted worktree, and stale live PyPI README content.
- Added `tools/verify_public_adoption_report.py` and negative tests; the
  scheduled/manual adoption workflow now validates schema version 2, release
  state, and the explicit false adoption/production boundaries before storing
  its artifact.
- No package, tag, publication, issue, outreach, or external service changed.

## 2026-08-14 — repeatable public adoption audit

- Added `tools/audit_public_adoption.py` as a read-only measurement loop for
  release-driven growth. It combines live PyPI version and README freshness,
  PyPIStats downloads without mirrors, GitHub repository interest signals, and
  open issue counts into a timestamped JSON report with source URLs.
- The report explicitly records that sustained external usage, customer demand,
  and production readiness remain unverified. It does not send data, create
  issues, infer users from downloads, or mutate a provider account.
- Added deterministic parser and aggregation tests, wired the test and tool
  into CI coverage, and documented the command in `docs/adoption.rst`.
- A live run at `2026-08-14T03:26:03Z` returned 1,707 downloads over 58
  without-mirrors days, including 605 in the last 30 calendar days and 40 in
  the last 7; the latest returned day had 3 downloads. PyPI is currently
  version `0.1.11`, matching the workspace version, but its long description
  does not match the current checkout README.
- The same run found 0 GitHub stars, forks, watchers, or subscribers and 5
  open issues (#4, #5, #9, #10, and #11). The report keeps these as public
  interest signals and leaves sustained external usage unverified.
- Added a quiet weekly/manual GitHub Actions workflow that stores each complete
  report for 90 days. It uses read-only repository permissions, validates the
  JSON artifact, and performs no package publication, issue creation, or
  outreach.
- Closed a container-release gap: manual branch dispatch previously could pass
  the conditional tag check and reach GHCR login. The workflow now requires
  `GITHUB_REF_TYPE=tag` and the exact workspace version tag on every route,
  with `tools/verify_publish_workflows.py` and negative tests covering the guard.
- Tightened production evidence promotion so `production_scale` and
  `independent_reproduction` manifests cannot omit the workflow profile or use
  zero-byte/zero-read inputs. The builder, validator, docs, and negative tests
  now fail closed on those incomplete records.
- Ran the current-source `python3 tools/bench_suite.py --repeats 1 --skip-build`
  with a temporary profile bundle at
  `/private/tmp/turbo-picard-current-bench-profile.json`. All 32 commands
  passed parity; the local one-repeat geometric mean was 82.98x, the floor was
  15.77x, the top was 258.64x, and MarkDuplicates measured 22.86x. This is
  scoped regression evidence, not a public or production-scale performance
  claim.
- No package, tag, publication, GitHub issue, or external service changed.

## 2026-08-14 — public adoption baseline (read-only)

- Captured a starting public distribution and interest baseline without
  changing any external service. PyPIStats reports 1,707 downloads in the
  `without_mirrors` category from 2026-06-05 through 2026-08-13; the latest
  returned day was 2026-08-13 with 3 downloads. The recent-window endpoint
  returned HTTP 429 during this read, so no daily or weekly window estimate is
  recorded.
- GitHub repository metadata reports 0 stars, 0 forks, 0 subscribers, and 5
  open issues. The five open issues are #4 (trial reports), #5 (roadmap), #9
  (bounded duplicate marking), #10 (metrics), and #11 (benchmarks).
- These are distribution and interest signals only. They do not establish
  sustained external usage, workflow-owner adoption, customer demand, or
  production readiness. Re-measure after each verified release and after any
  owner-approved trial outreach.
- Sources: [PyPIStats overall API](https://pypistats.org/api/packages/turbo-picard/overall?mirrors=false),
  [GitHub repository metadata](https://api.github.com/repos/dnncha/turbo-picard),
  and [open GitHub issues](https://api.github.com/repos/dnncha/turbo-picard/issues?state=open&per_page=100).

## 2026-08-14 — bounded UMI profile fixture evidence

- Ran the current-source auditable competitor runner with five measured
  repeats, one warm-up, one thread, `READ_NAME_REGEX=null`, and the pinned
  Picard 3.4.0 conda wrapper on the checked-in barcode fixture.
- The primary `BARCODE_TAG=RX` profile passed exact parity and the required-tool
  gate: Turbo-Picard median wall time 0.0291 seconds and peak RSS 8,339,456
  bytes versus Picard 0.4198 seconds and 754,434,048 bytes.
- The mate-specific `READ_ONE_BARCODE_TAG=BX` and `READ_TWO_BARCODE_TAG=BY`
  profile also passed exact parity and the required-tool gate: Turbo-Picard
  median 0.0291 seconds and peak RSS 8,404,992 bytes versus Picard 0.4195
  seconds and 754,876,416 bytes.
- Raw bundles and generated release-candidate manifests are retained at
  `/private/tmp/turbo-picard-umi-profile-primary/` and
  `/private/tmp/turbo-picard-umi-profile-mate/`. These are fixture-level
  bounded barcode evidence, not advanced UMI normalization, production-scale,
  or independent-reproduction proof.
- The focused Rust MarkDuplicates suite passed 57 tests across library, BAM,
  CRAM, and SAM-validation targets with `cargo test -p
  turbo-picard-markdup --tests --locked`; Rust formatting also passed.

## 2026-08-14 — production-evidence workflow profile controls

- Added workflow-dispatch inputs for WGS, WES, RNA-seq, UMI/barcode,
  reference-backed CRAM, multi-library, and cohort profiles, plus optical-regex,
  DS/DI, and primary or mate-specific barcode settings already supported by the
  auditable competitor runner.
- Dispatch validation rejects invalid SAM tags, missing UMI-panel barcodes,
  and a `cram_reference` profile without CRAM input. The chosen controls now
  flow into the raw protocol and generated manifest.
- YAML parsing and repository verification remain required; no production run,
  package, tag, publication, or external service changed.

## 2026-08-14 — fail-closed independent-reproduction attestation

- Tightened `tools/validate_production_manifest.py` so an independent
  reproduction cannot be marked `pass` from a reviewer name alone. A pass or
  fail record now needs retained evidence, a second host profile, and matching
  Turbo-Picard commit, input, and command-protocol SHA-256 values.
- Extended `tools/build_production_manifest.py`, the production manifest
  example, runbook, and focused tests to carry and validate those fields.
- This improves release evidence integrity without claiming production-scale or
  independent results; the owner-controlled runs and retained raw bundles are
  still pending. No package, tag, publication, or external service changed.

## 2026-08-14 — refreshed checked-in MarkDuplicates guardrails

- Re-ran the checked-in 1,000,000-record synthetic guardrail with five measured
  repeats and the reference-backed CRAM guardrail with three measured repeats
  using the current release binary SHA-256
  `d78b53dbe8a1980b8d2e5f3ccd6619dfec67501f42eae3d79537d9e6c4071f52`.
- Both required-tool gates passed exact alignment and normalized metrics parity.
  The synthetic run measured 0.544 seconds versus 1.757 seconds median and
  237,436,928 versus 1,433,518,080 bytes peak RSS. The CRAM run measured
  0.234 versus 0.748 seconds and 39,403,520 versus 850,362,368 bytes.
- Updated the checked-in JSON provenance and benchmark README to match the raw
  bundles. These remain evidence-only guardrails, not production-scale or
  independent-reproduction proof.
- Added `tools/verify_markduplicates_guardrails.py` with negative tests for
  parity, ratio, stale-hash, disclosure, and cross-guardrail consistency. CI,
  package-install verification, PyPI validation, and container validation now
  run the guardrail verifier.

## 2026-08-14 — adoption feedback path and container release guard

- The public repository currently reports that new issue creation is restricted,
  so the documented trial form is not a complete user path. Added a fallback in
  the README, support page, adoption guide, and one-command trial guide that
  directs users to comment on the existing public trial thread instead. No
  GitHub setting or issue content was changed.
- Hardened the container release path: the builder now uses
  `cargo build --release --locked`, and the publish workflow validates release
  version/source markers before logging in to GHCR. Tag-triggered runs also
  require the tag to match the workspace version.
- Sphinx with warnings treated as errors, README/site/link/release verifiers,
  YAML parsing, and `git diff --check` passed. Docker is not installed on this
  macOS host, so the actual container build remains covered by the Ubuntu CI
  job rather than being claimed locally.
- No package, image, publication, tag, or external service was changed.

## 2026-08-14 — current-source release-candidate MarkDuplicates evidence

- Re-ran the auditable competitor runner from the current checkout with the
  pinned Picard 3.4.0 and samtools environment, five measured repeats, one
  warm-up, one thread, and `READ_NAME_REGEX=null`.
- On the pinned public GATK NA12878 mitochondrial BAM
  (`70ea2e429805a75ce6007a32ba176ea7c697a398e0c39a9d58aaaa30e1ed86c3`),
  exact duplicate-flag/tag and normalized DuplicationMetrics parity passed
  across all five repeats. Median wall time was 0.190 seconds for
  Turbo-Picard versus 0.567 seconds for Picard 3.4.0; peak RSS was 14,991,360
  versus 803,487,744 bytes. The raw bundle is retained at
  `/private/tmp/turbo-picard-current-release-candidate.z0oouz/`.
- On the pinned Picard SNVQ BAM, the same five-repeat protocol also passed
  exact parity, but Turbo-Picard was slightly slower: 1.800 versus 1.588
  seconds median. This is useful scope evidence and a reminder that the
  current result does not justify a universal performance headline.
- A direct `compare_real_data.py` retry could not launch the retained Picard
  JAR because this macOS host has only the Java launcher stub; the repository's
  pinned conda Picard wrapper plus samtools provided the valid comparator path.
- These are release-candidate public-fixture results only. They are not 30x
  WGS, production-scale, UMI/WES/optical-heavy coverage, or independent
  reproduction evidence. No package, publication, tag, or external service
  was changed.

## 2026-08-14 — executable barcode/UMI production evidence profile

- Extended the competitor evidence runner with explicit `BARCODE_TAG`,
  `READ_ONE_BARCODE_TAG`, and `READ_TWO_BARCODE_TAG` options for the
  Picard-compatible presets.
- Added recorded profile labels for WGS, WES, RNA-seq, UMI panel, CRAM,
  multi-library, and cohort evidence. The UMI-panel profile requires an
  explicit barcode tag; the CRAM-reference profile requires CRAM input and a
  reference FASTA.
- Added focused tests, updated the production/evidence runbooks, and carried
  the profile plus barcode fields into the reviewed manifest with fail-closed
  validation. This makes a real UMI-panel comparison runnable and auditable
  without implying support for advanced UMI normalization.
- Extended the recommended real-data trial/audit comparator with repeatable
  `--markduplicates-arg KEY=VALUE` options, including safe pass-through of
  regex braces, so workflow owners can test the bounded barcode mode through
  the same shareable-report path.
- Re-read the public PyPI `0.1.11` arm64 wheel in a clean temporary
  environment: `--version`, `doctor`, the read-only `trial` contract, the
  compatibility shim, and the real install smoke all passed. The public PyPI
  long description still differs from this newer checkout README, so publishing
  the current documentation remains a future release task rather than a claim
  about the live package.
- Ran that live wheel through `tools/compare_real_data.py` against the pinned
  public Picard SNVQ BAM (`be0daa7c...779c06b3`): `MarkDuplicates` passed the
  duplicate-semantic and stable-metrics comparison, with 0.491 seconds for
  turbo-picard versus 1.838 seconds for Picard 3.4.0 (3.74x). The generated
  shareable report is a command-level trial artifact, not production-scale or
  independent-reproduction evidence.
- Repeated the live-package trial on the checked-in barcode-tag fixture with
  `BARCODE_TAG=RX` and `READ_NAME_REGEX=null`: parity passed, with 0.200
  seconds versus 0.642 seconds for Picard 3.4.0 (3.20x). This confirms the
  public 0.1.11 wheel can exercise the bounded barcode-grouping trial path;
  it remains fixture-level evidence.
- The live 0.1.11 wheel did not pass the mate-specific barcode fixture:
  `READ_ONE_BARCODE_TAG=BX`, `READ_TWO_BARCODE_TAG=BY`, and
  `READ_NAME_REGEX=null` matched duplicate semantics and totals but differed in
  the histogram digest. The current checkout binary passed the same comparison
  (metrics digest `c044e1d3...a04ab7b2`), so this is a concrete release-sync
  gate: do not claim mate-specific barcode parity for the public wheel until
  the corrected checkout is released and rechecked.
- Re-ran `./tools/verify_basic_picard_parity.sh`; the complete local
  MarkDuplicates fixture matrix, including barcode, mate-specific barcode,
  optical, DS/DI, CRAM, multi-input, multi-library, and sequencing-duplicate
  cases, passed semantic equivalence.
- Read-only GitHub state is also healthy for the committed surfaces: mainline
  commit `6c998857...931e9fd` has successful CI run #268, and release tag
  `v0.1.11` commit `f09e1e4...` has successful production-evidence run #32.
  Those remote runs do not validate this dirty, newer checkout; the release
  run carries a Node.js 20 deprecation warning.
- Added `tools/verify_mate_barcode_install_smoke.sh` to the PyPI validation job.
  It asserts the checked-in Picard 3.4.0 mate-specific barcode histogram on the
  built wheel before publication. The current checkout passes; the public
  0.1.11 wheel reproduces the expected negative failure, proving the gate catches
  the release-sync bug.
- No benchmark was run here because no permissioned production UMI input was
  available, and no package, publication, tag, or external service was changed.

## 2026-08-14 — bounded MarkDuplicates optical-family parity

- Extended the bounded external MarkDuplicates plan to carry Picard-compatible
  optical-family decisions through the external sort and replay stream. The
  plan now supports the default read-name parser and validated custom regexes
  with three capture groups, keeps read-group identity in optical clustering,
  preserves optical-only tagging/removal semantics, and carries paired
  duplicate-set `DS`/`DI` metadata through replay.
- Repaired the optical histogram aggregation boundary so per-library optical
  counts reach the sparse four-column Picard metrics format. The no-optical
  `READ_NAME_REGEX=null` behavior remains the existing compact-compatible
  metrics shape; advanced UMI normalization remains compact.
- The optical fixture passed direct Picard 3.4.0 versus Turbo-Picard release
  comparison, including duplicate flags/tags and metrics. The focused suite now
  has 24 unit tests and 28 MarkDuplicates integration tests, including custom
  regex, optical-only removal, and bounded DS/DI regressions. Custom regex
  compilation is done once per bounded run rather than once per duplicate
  family. The duplicate-set-members semantic comparator also passed.
- The complete `tools/verify_basic_picard_parity.sh` MarkDuplicates fixture
  matrix passed, including basic, barcode, optical, DS/DI, CRAM, multi-input,
  and sequencing-duplicate cases.
- Added `--tag-duplicate-set-members` to the production competitor runner so
  workflow-sized DS/DI comparisons are explicit, reproducible, and visible in
  the report protocol without changing the default benchmark scope.
- Final local verification passed: `cargo test --workspace --locked`, strict
  workspace Clippy, 363 Python tool tests with one skipped, `git diff --check`,
  formatting, and Sphinx with warnings treated as errors. The release optical
  comparator report is retained at
  `/private/tmp/turbo-picard-optical-parity-20260814-final/`.
- The read-only release audit still waits on owner-controlled release state:
  the worktree is dirty, `HEAD` is not the `v0.1.11` tag, and live PyPI's
  published long description differs from the checkout. No packaging,
  publication, or tag mutation was attempted.
- This is fixture-level parity evidence, not production-scale performance or
  release approval. No commit, push, package publication, Bioconda update, or
  external service mutation was performed.

## 2026-08-14 — bounded MarkDuplicates barcode parity

- Extended the bounded external MarkDuplicates plan to carry the primary
  `BARCODE_TAG` plus the mate-specific `READ_ONE_BARCODE_TAG` and
  `READ_TWO_BARCODE_TAG` values as separate key dimensions. The relevant mate
  is selected from SAM flags, so a barcode present only on its intended mate is
  not silently combined with the other read.
- Matched Picard 3.4.0's explicit `READ_NAME_REGEX=null` metrics behavior:
  singleton-pair histogram bins remain, while duplicate-pair set-size bins are
  omitted when optical discovery is disabled.
- Rebuilt the arm64 release binary and compared barcode-tag and read-one/read-two
  barcode fixtures against Picard 3.4.0. Record identity/duplicate flags and
  normalized DuplicationMetrics both passed; the focused MarkDuplicates suite
  now has 27 integration tests, including a mate-specific-tag regression.
- Refreshed the checked-in external-plan guardrails with the rebuilt binary:
  the pinned CRAM comparison measured 0.245 seconds versus 0.886 seconds
  median wall time and 40,828,928 versus 857,194,496 bytes peak RSS; the
  1,000,000-record synthetic comparison measured 0.536 versus 1.776 seconds
  and 238,157,824 versus 1,439,760,384 bytes. Both required-tool gates and
  exact parity checks passed.
- No commit, push, package publication, Bioconda update, or external service
  mutation was performed.

## 2026-08-14 — bounded MarkDuplicates multi-input and CRAM evidence

- Extended the fail-closed external MarkDuplicates plan to support BAM and
  explicit-reference CRAM input lists when the streams are already globally
  coordinate-ordered.
- Preserved the compact path for out-of-order multi-input streams so the
  existing output-order contract is not changed.
- Added unit and integration coverage for multiple supported inputs and
  bounded scratch cleanup.
- Re-ran the pinned reference-backed CRAM comparison: exact parity passed;
  Turbo Picard median wall time was 0.233 seconds versus Picard at 0.755
  seconds, with 40,239,104 versus 860,078,080 bytes peak RSS on that fixture.
- Re-ran the 1,000,000-record synthetic guardrail: exact parity passed;
  Turbo Picard median wall time was 0.540 seconds versus Picard at 1.874
  seconds, with 237,305,856 versus 1,223,376,896 bytes peak RSS.
- Passed `cargo test --workspace --locked`, `cargo clippy --workspace
  --all-targets --locked -- -D warnings`, 360 Python tool tests with one
  skipped, Sphinx with warnings treated as errors, release/evidence verifiers,
  and the installed-binary smoke test.
- No commit, push, package publication, Bioconda update, or external service
  mutation was performed. The local release preflight remains a wait state
  while the worktree is dirty and differs from the `v0.1.11` tag.

## 2026-08-14 — local PyPI distribution handoff

- Built `turbo_picard-0.1.11-py3-none-macosx_11_0_arm64.whl` and
  `turbo_picard-0.1.11.tar.gz` from the current checkout in an isolated
  temporary packaging environment.
- `tools/verify_release_artifacts.py` passed for both artifacts, including
  version and README metadata, required entrypoints, safe source paths, and
  wheel binary architecture.
- A clean temporary virtual environment installed the wheel without network
  dependencies; `--version`, `doctor`, the read-only `trial` contract, the
  `picard` shim, and `tools/verify_install_smoke.sh` all passed.

## 2026-08-14 — shareable workflow trial report

- Added `--shareable-report` to `tools/compare_real_data.py` for a separate,
  deliberately lossy Markdown summary suitable for the public trial-report
  issue form.
- The report retains tool versions, input format and approximate size, command
  outcomes, parity method, timings, and speedups while omitting local paths,
  input hashes, command arguments, generated artifact names, and raw data.
- Unit coverage passed, and the mode produced a PASS report from the pinned
  GATK NA12878 CRAM MarkDuplicates comparison with a measured 5.18x speedup.
- `--include-public-source` remains explicit and opt-in; report generation does
  not publish, submit, or mutate external state.
