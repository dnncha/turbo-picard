# Turbo Picard success work

The objective remains active: make Turbo Picard a massive industry success
story through practical adoption, distribution, performance, release readiness,
and evidence-backed growth.

## Next gates

1. Run the pinned production-scale MarkDuplicates protocol on a permissioned
   30x WGS input, plus representative WES, UMI/barcode, optical-heavy,
   multi-library, and reference-backed CRAM profiles.
2. Reproduce the production-scale result on a second machine or by an
   independent reviewer, retaining the exact manifest and raw resource logs.
3. Reconcile the dirty release checkout with the intended mainline commit,
   verify the exact `v0.1.12` release tag, and only then publish the prepared
   PyPI and Bioconda distribution updates.
4. Collect real one-command trial reports from workflow owners, including
   successful matches, mismatches, and installation or integration friction.
5. Expand the bounded MarkDuplicates path to advanced UMI normalization only
   with matching external-state and parity proof.

## Current verified direction

- The current `0.1.12` candidate is committed locally on the existing `codex/`
  branch; the worktree is clean and the branch is ahead of `origin/main`. The
  exact current source SHA is retained in the release
  handoff manifest. The candidate remains a release candidate because neither
  local nor origin has the matching `v0.1.12` tag.
- Fresh current-HEAD release-candidate evidence passed the five-repeat
  `MarkDuplicates` protocol on the pinned public SNVQ BAM: 26,577 records,
  exact duplicate/tag and normalized-metrics parity, `0.311250` seconds Turbo
  median versus `1.551860` seconds Picard, and median peak RSS of `37,765,120`
  versus `1,018,560,512` bytes. The retained manifest leaves independent
  reproduction `not_run` and does not claim production-scale readiness.
- The copy-paste trial contract passes on current HEAD, including the native
  `MarkDuplicates` trial shape and fallback-only behavior. The production-
  readiness golden surface now keeps `CollectHsMetrics` explicitly delegated
  until native bait/target accounting and parity evidence exist.
- The production-scale evidence dispatch path now shares a tested
  `tools/validate_production_dispatch.py` contract between local validation and
  GitHub Actions. It fails before input download/build for missing Picard/Turbo
  comparison tools, invalid CRAM/reference or UMI/barcode settings, malformed
  hashes, or fewer than five measured repeats.
- The benchmarked candidate runtime at source SHA
  `3c43bca0cc8624008cef6979e0c5b5450a965124` passes all 32 three-repeat
  benchmark parity checks against Picard 3.4.0. The refreshed checked-in assets
  report an `87.47x` geometric mean, `100.27x` median, `22.17x` floor on
  `SetNmMdAndUqTags`, and `261.75x` maximum on `NormalizeFasta`. A focused
  `FastqToSam` five-repeat baseline passes parity at `22.31x` median on 100,000
  paired reads; no speculative source change was justified.
- An isolated exact-commit Bioconda archive rehearsal passes: the local archive
  was accepted by `prepare_bioconda_release.py`, both recipes passed the
  release-ready verifier, and source/version/link checks passed. The live
  recipe placeholders remain intentional until the actual GitHub `v0.1.12`
  archive exists; the local archive hash is not release evidence.
- The measured candidate optimization reduces reference-cache overhead in
  `SetNmMdAndUqTags`: five focused repeats moved from 18.84x to 20.94x with
  exact parity. In-window segments use bounded borrowed slices; oversized and
  window-crossing segments retain the prior fallback. This remains local
  release-candidate evidence only.
- Measured the next MarkDuplicates adoption optimization on the candidate
  source: single BAM/CRAM inputs up to 100,000 records now use a bounded
  compact plan after an exact count preflight, while larger and multi-input
  shapes retain the external or existing bounded path. The refreshed suite
  still passes 32/32 parity, with `MarkDuplicates` at `27.82x` on its saved
  fixture. This is local release-candidate evidence only; production-scale and
  independent reproduction gates remain open.
- The post-`0.1.11` corrections are now a locally consistent `0.1.12` release
  candidate: Cargo, PyPI metadata, citation, bio.tools, both Bioconda recipes,
  release archive instructions, and the source-release marker align. The
  published PyPI/container references remain explicitly labeled `0.1.11` until
  the candidate is tagged and published.
- The candidate arm64 wheel and source distribution passed artifact validation;
  a clean offline virtual environment passed `pip check`, `--version`,
  `doctor`, the read-only `trial` contract, the `picard` shim, the real install
  smoke, and the mate-specific barcode install smoke.
- Rebuilt those artifacts from the clean committed candidate and rechecked the
  exact wheel in an isolated environment. The fresh entrypoints match the
  release target binaries byte-for-byte, and the fresh wheel passes five BAM
  and six reference-backed CRAM parity comparisons through the no-`samtools`
  helper path.
- Fresh wheel-binary MarkDuplicates guardrails passed exact parity against
  Picard 3.4.0: the 1M synthetic fixture measured 0.531 versus 1.861 seconds
  and 237,420,544 versus 1,160,527,872 bytes median RSS; the reference-backed
  CRAM fixture measured 0.233 versus 0.826 seconds and 39,452,672 versus
  859,127,808 bytes. These remain fixture guardrails, not production-scale or
  independent proof.
- The post-bump live adoption audit confirms PyPI is still `0.1.11`, with
  605 without-mirrors downloads in the latest 30 days and 40 in the latest 7;
  `v0.1.12` is not locally or remotely tagged. This is an explicit publication
  boundary, not a claim of adoption or production readiness.
- The read-only adoption audit now records public issue and trial-comment
  author provenance without retaining usernames. The 2026-08-14 refresh found
  all five open issues and the one public trial-thread comment maintainer-
  authored, with zero externally authored issues or comments in the sampled
  GitHub API responses. `workflow_owner_trial_reports_verified=false` remains
  correct: public maintainer activity is not workflow-owner adoption evidence,
  and an empty external count does not rule out private or unreported trials.
- Extended the same read-only audit to distribution channels. The 2026-08-14
  snapshot found GitHub release `v0.1.11` and GHCR tag `0.1.11` as the latest
  published channels; neither Bioconda package is indexed, and PR #65922 still
  targets older `0.1.10` metadata. The audit records these as channel-state
  gaps without attempting publication or repair.
- Closed a PyPI release-workflow coverage gap: macOS arm64 and Intel wheel jobs
  now run a reusable isolated install, `pip check`, version/doctor/trial/shim,
  real-data, and mate-barcode smoke before artifact upload. Linux arm64 remains
  explicitly cross-built and artifact-validated, not falsely runtime-tested on
  an x86 runner. The current arm64 wheel passed the reusable smoke locally.
- Closed the corresponding container-release gap: after GHCR push, the workflow
  now pulls the exact version tag and runs version, doctor, trial, and a real
  MarkDuplicates fixture with checked-in inputs. The verifier enforces that this
  smoke happens after the push and before the job can finish.
- Re-ran the full current `0.1.12` 32-command suite: all parity checks passed;
  one-repeat geometric mean was 83.27x, the floor was 8.41x, the maximum was
  278.74x, and MarkDuplicates measured 15.99x. This is local regression
  evidence only, not a universal performance claim.
- Refreshed the checked-in public Picard SNVQ evidence against the current
  `0.1.12` release binary and Picard `3.4.0`: `ViewSam`, `CleanSam`, both
  alignment-metrics commands, and `MarkDuplicates` all pass exact comparison.
  The observed run measured 4.10x, 4.89x, 17.51x, 18.02x, and 6.24x
  respectively; these remain command-level release-candidate evidence, not
  cohort or production approval. The separate redacted trial report remains
  retained outside the repository for owner review.
- Removed the reviewable comparison helper's undocumented host `samtools`
  dependency: output SAM materialization now uses the configured
  Picard-compatible `ViewSam` entrypoints and passes explicit CRAM references.
  A no-`samtools` run passed all five SNVQ commands plus six reference-backed
  CRAM commands, with focused regression coverage for BAM and CRAM command
  construction.
- The reviewable comparator now validates repository-ready manifest requests
  before running expensive real-data commands. Invalid output layout, missing
  pinned source citation, duplicate command selection, release-candidate
  command coverage, and undersized inputs fail immediately; the one-command
  trial guide documents the required manifest shape.
- The manual production-evidence workflow's validation bootstrap now uses the
  current checkout and Python setup actions used by the rest of the release
  path, removing its stale action-version mismatch before the next
  owner-controlled production-scale run.
- Exercised the production-evidence runner, report adapter, and manifest
  validator end to end on the public SNVQ fixture with five repeats. The
  required-tool gate and exact duplicate/metrics parity passed for 26,577
  records; the resulting `release_candidate` manifest retains resource and
  provenance fields while leaving independent reproduction unrun.
- Extended the same release-candidate path across the public reference-backed
  CRAM fixture and bounded primary and mate-specific barcode fixtures. All
  three five-repeat profiles passed exact parity with the required tools and
  validated manifests; they strengthen profile coverage without satisfying
  production-scale, independent, or advanced UMI-normalization gates.
- Corrected the prepared outreach drafts after a read-only Bioconda check:
  PR #65922 is for older `0.1.10` metadata, not the current candidate. Drafts
  now point users to published PyPI/container installs and describe `0.1.12`
  as unpublished until its tag, archive, review, and publication gates pass.
- The trial caught and fixed a real no-reference metrics regression: Picard
  leaves mismatch-rate fields at zero without `REFERENCE_SEQUENCE`, even when
  NM/MD tags are present. Turbo Picard now matches that boundary and has a
  regression test; reference-backed mismatch calculation remains available.
- Added `CHANGELOG.md` as a sober release handoff covering candidate scope,
  compatibility boundaries, and the evidence still required before publication.
- Added `tools/build_release_manifest.py`; the real candidate handoff records
  the rebuilt wheel/sdist hashes, source/tag blockers, and the current 32-command
  benchmark summary without exposing local filesystem paths. The PyPI workflow
  now retains this manifest with its validated artifacts.
- A local arm64 PyPI wheel and source distribution can be built and installed
  from the current checkout; artifact metadata, README content, entrypoint
  architecture, and the real install smoke all pass.
- The live PyPI `0.1.11` macOS arm64 wheel was installed in a clean temporary
  environment and passed `--version`, `doctor`, the read-only `trial` contract,
  the `picard` shim check, and the real install smoke. Its published long
  description still reflects an older README, so the current checkout's newer
  adoption guidance is not yet a live-package claim.
- The live PyPI wheel also passed a real `MarkDuplicates` trial through
  `tools/compare_real_data.py` on the pinned public Picard SNVQ BAM: exact
  duplicate-semantic and stable-metrics parity, 0.491 versus 1.838 seconds
  (3.74x). This is command-level adoption evidence only, not production-scale
  or independent-reproduction proof.
- The same live wheel passed the checked-in barcode-tag fixture with
  `BARCODE_TAG=RX` and `READ_NAME_REGEX=null`: exact parity, 0.200 versus
  0.642 seconds (3.20x). This supports a public bounded barcode-grouping trial,
  not advanced UMI normalization or production-scale readiness.
- The live wheel failed the mate-specific barcode fixture's histogram digest
  with `READ_ONE_BARCODE_TAG=BX`, `READ_TWO_BARCODE_TAG=BY`, and
  `READ_NAME_REGEX=null`, while the current checkout binary passed it. Keep
  mate-specific barcode parity as an unreleased gate until the corrected
  checkout is tagged, published, and rechecked against Picard.
- Single BAM, explicit-reference CRAM, and already globally coordinate-ordered
  multiple alignment inputs can use the bounded two-pass plan. `BARCODE_TAG`,
  mate-specific barcode grouping, default or validated three-capture-group
  optical-family discovery, and `REMOVE_SEQUENCING_DUPLICATES` are included;
  explicit `READ_NAME_REGEX=null` selects the no-optical variant. DS/DI
  duplicate-set tags are also carried through bounded replay.
- The bounded optical fixture matches Picard 3.4.0's duplicate semantics and
  sparse four-column optical histogram, including custom read-name regex
  parsing and optical-only removal/tagging.
- The production competitor runner can explicitly request paired DS/DI tags
  with `--tag-duplicate-set-members`, can pass primary or mate-specific barcode
  tags for UMI-panel evidence, and records those choices plus a profile label in
  its evidence protocol. The `umi_panel` and `cram_reference` profiles fail
  closed when their required inputs are missing.
- The real-data trial/audit wrapper can pass the same bounded
  `BARCODE_TAG`/mate-specific and DS/DI options through repeated
  `--markduplicates-arg` flags, preserving the exact options in private
  evidence while keeping shareable reports redacted.
- Out-of-order multi-input streams deliberately retain the existing compact
  sorted compatibility path.
- The real-data comparator can emit a reviewable shareable trial report that
  omits local paths, input hashes, command arguments, generated artifacts, and
  raw data before a workflow owner posts public evidence.
- Release-facing claims remain evidence-only until production-scale and
  independent-reproduction gates pass.
- Committed mainline CI is green for `6c998857...931e9fd`, and the tagged
  `v0.1.11` production-evidence workflow is green for `f09e1e4...`; neither
  remote run proves the uncommitted newer checkout, and the tagged workflow
  reports a Node.js 20 deprecation warning.
- The PyPI publish validation now includes a mate-specific barcode histogram
  install smoke. It passes on the current checkout and fails on the public
  0.1.11 wheel, turning the discovered release-sync mismatch into a pre-publish
  artifact gate.
- A fresh five-repeat current-source competitor run against the pinned public
  GATK NA12878 mitochondrial BAM passed exact duplicate-semantic and normalized
  metrics parity: median 0.190 seconds for Turbo-Picard versus 0.567 seconds
  for Picard 3.4.0, with peak RSS of 14,991,360 versus 803,487,744 bytes. The
  same protocol passed parity on the Picard SNVQ fixture but measured 1.800
  versus 1.588 seconds, so these remain scoped release-candidate evidence and
  do not support a universal speed claim.
- The current-source evidence run used the pinned conda Picard wrapper and
  samtools because the host has no installed Java runtime for direct JAR
  execution. The raw bundle is retained outside the repository; production
  and independent-reproduction gates remain open.
- GitHub currently restricts new issue creation on the public repository. The
  README, support page, adoption guide, and one-command trial guide now provide
  the existing public trial thread as a fallback reporting path, without
  changing repository settings or posting on the user's behalf.
- The container release path now builds with Cargo `--locked` and validates
  release metadata before GHCR login; tag-triggered runs fail if the tag does
  not match the workspace version. Docker was unavailable locally, so the
  Ubuntu CI container job remains the build-level proof.
- Refreshed the checked-in synthetic 1M and reference-backed CRAM guardrails
  against the current binary SHA-256. Both exact parity gates pass, and the
  JSON plus benchmark README now agree with the raw runs; these remain
  evidence-only guardrails rather than production-scale or independent proof.
- Added a fail-closed guardrail verifier with negative tests for stale binary
  hashes, failed parity, inconsistent resource ratios, and README drift; CI,
  package-install, PyPI, and container validation now execute it.
- Strengthened the production manifest's independent-reproduction contract:
  `status=pass` now requires retained evidence, an independent host profile,
  and matching commit, input, and command-protocol hashes. The builder, example,
  documentation, and negative tests now preserve that boundary.
- Extended the manual production-evidence workflow to expose the existing
  profile, optical-regex, DS/DI, and primary or mate-specific barcode controls;
  dispatch validation now fails closed for invalid profile/input combinations
  and missing UMI-panel tags.
- Ran the current-source competitor runner through both UMI-panel profiles on
  the checked-in barcode fixture: primary `BARCODE_TAG=RX` and mate-specific
  `READ_ONE_BARCODE_TAG=BX`/`READ_TWO_BARCODE_TAG=BY`. Five repeats passed exact
  parity and the required-tool gate; manifests validate as release-candidate
  evidence, not production-scale evidence. Primary medians were 0.0291 versus
  0.4198 seconds, and mate-specific medians were 0.0291 versus 0.4195 seconds,
  with Turbo-Picard peak RSS near 8.4 MB versus Picard near 755 MB.
- The focused Rust MarkDuplicates suite now passes 57 tests across library,
  BAM, CRAM, and SAM-validation targets with `cargo test -p
  turbo-picard-markdup --tests --locked`; `cargo fmt --all -- --check` also
  passes.
- A read-only public adoption baseline was captured on 2026-08-14: PyPIStats
  reports 1,707 downloads without mirrors from 2026-06-05 through 2026-08-13;
  GitHub reports 0 stars, 0 forks, 0 subscribers, and 5 open issues. These are
  distribution and interest signals only, not evidence of sustained external
  usage or customer adoption. Re-measure after each verified release and after
  owner-approved trial outreach.
- Added `tools/audit_public_adoption.py`, a read-only repeatable audit that
  records live PyPI version/README freshness, PyPIStats download windows,
  GitHub interest signals, open issue counts, source URLs, and explicit
  unverified-adoption boundaries. It is documented in the adoption guide and
  covered by unit tests and CI syntax coverage.
- The first live audit found 1,707 without-mirrors downloads overall, 605 in
  the latest 30 calendar days, and 40 in the latest 7; PyPI `0.1.11` still
  matches the workspace version but its published README is stale. This is a
  release-freshness finding, not evidence of sustained external usage.
- The repeatable adoption report now also records the workspace version,
  current HEAD, dirty-worktree state, local and `origin` release-tag commits,
  and explicit release-source blockers. The latest read-only report confirms
  the current source is dirty and ahead of the matching `v0.1.11` tag while
  the public package remains `0.1.11` with stale long-description content.
- Added `tools/verify_public_adoption_report.py` with negative tests and a
  weekly-workflow gate so the report cannot silently lose its release-state
  fields or turn download counts into production/adoption claims.
- Added a quiet weekly/manual `.github/workflows/public-adoption-audit.yml`
  job that stores the read-only JSON report as a 90-day GitHub Actions
  artifact. It uses repository read permission only and performs no publishing,
  issue creation, or outreach.
- Hardened container publication so both tag-triggered and manually dispatched
  runs require the exact `v<workspace-version>` tag before GHCR login. Added a
  fail-closed verifier and negative tests, wired into CI.
- Tightened production evidence promotion: `production_scale` and
  `independent_reproduction` manifests now require a workflow profile and
  positive measured input bytes/read count; the builder, validator, and
  negative tests enforce the boundary.
- Refreshed the current checkout's 32-command benchmark suite in a temporary
  profile bundle: all 32 parity checks passed, with one-repeat local geometric
  mean 82.98x, floor 15.77x, top 258.64x, and MarkDuplicates 22.86x. This is
  regression evidence for the current host, not a replacement for the saved
  multi-repeat public benchmark or production-scale evidence.
