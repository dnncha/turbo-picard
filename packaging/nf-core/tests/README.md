# nf-core process test profile

`main.nf.test` exercises the candidate process with the repository’s
redistributable basic BAM fixture and the existing public, reference-backed
CRAM evidence fixture. The tests check the BAM, BAI, metrics, version and
stub output channels rather than treating process completion alone as proof of
parity.

Run after building the CLI and installing Nextflow 26.04.6 and nf-test 0.9.5
(the versions pinned in CI):

```bash
cargo build --release -p turbo-picard-cli --bin turbo-picard
PATH="$PWD/target/release:$PATH" nf-test test packaging/nf-core/tests/main.nf.test
```

The CRAM case is an integration fixture only. It does not establish a
production-scale compatibility or performance claim. Before upstreaming this
candidate to nf-core/modules, add the pinned public container/conda artifact,
run the module lint and Docker/Conda/Singularity profiles, and submit the
result for nf-core review.
