//! Expect tests for the command-line contract.
//!
//! The whole `trial` report is asserted so a changed default, reason, output
//! declaration, or ordering shows up in the diff. Run `UPDATE_EXPECT=1 cargo
//! test -p turbo-picard-cli --test expectations` to accept an intentional
//! change.

use assert_cmd::Command;
use expect_test::expect;

#[test]
fn trial_text_contract_is_visible() {
    let mut command = Command::cargo_bin("turbo-picard").expect("binary exists");
    command
        .env("TURBO_PICARD_REQUIRE_NATIVE", "1")
        .env_remove("TURBO_PICARD_FALLBACK_COMMAND")
        .env_remove("PICARD_JAR")
        .args([
            "trial",
            "MarkDuplicates",
            "I=input.bam",
            "O=marked.bam",
            "M=metrics.txt",
        ]);

    let output = command.output().expect("trial runs");
    assert!(
        output.status.success(),
        "trial failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "unexpected stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    expect![[r#"
        inspection_only=true option_support=not-checked output_manifest=declared-arguments-only require_native=true
        command=MarkDuplicates
        status=partial-native
        trial_fit=recommended-first-trial
        why=Duplicate marking is a common runtime and heap-pressure point; this keeps Picard-style arguments and writes BAM plus metrics outputs.
        picard_command=picard MarkDuplicates I=input.bam O=marked.bam M=metrics.txt
        turbo_command=turbo-picard MarkDuplicates I=input.bam O=marked.bam M=metrics.txt
        compare=compare O=marked.bam,M=metrics.txt; also record sidecars, exit code, runtime, and peak RSS
        evidence=run both commands on the same representative input; keep input SHA-256, command versions, trial command lines, and output comparison notes
        fallback_command=not-found
        declared_outputs=O=marked.bam,M=metrics.txt
    "#]].assert_eq(&String::from_utf8_lossy(&output.stdout));
}

#[test]
fn explain_text_contract_is_visible() {
    let mut command = Command::cargo_bin("turbo-picard").expect("binary exists");
    command
        .env("TURBO_PICARD_REQUIRE_NATIVE", "1")
        .env_remove("TURBO_PICARD_FALLBACK_COMMAND")
        .args(["explain", "CollectHsMetrics"]);

    let output = command.output().expect("explain runs");
    assert!(output.status.success());
    assert!(output.stderr.is_empty());

    expect![[r#"
        inspection_only=true option_support=not-checked output_manifest=declared-arguments-only require_native=true
        command=CollectHsMetrics
        status=partial-native
        native_scope=Core ALL_READS hybrid-capture metrics, Picard-shaped histogram, PER_TARGET_COVERAGE, and PER_BASE_COVERAGE for reference-backed coordinate-sorted SAM/BAM/CRAM inputs, including unique bait/target territory, on/near/off-bait accounting, duplicate/adapter/mapQ/baseQ/overlap filter fractions, target coverage thresholds, GC dropout, and bounded theoretical het-sensitivity sampling.
        fallback_scope=Non-ALL_READS accumulation levels and unsupported advanced options should use upstream Picard.
        execution_path=native-when-inside-documented-scope-otherwise-fallback
        fallback_command=not-found
        declared_outputs=none
    "#]].assert_eq(&String::from_utf8_lossy(&output.stdout));
}

#[test]
fn documented_command_metadata_is_visible() {
    let mut command = Command::cargo_bin("turbo-picard").expect("binary exists");
    command.args(["capabilities", "--json", "--command", "MarkDuplicates"]);

    let output = command.output().expect("capabilities runs");
    assert!(
        output.status.success(),
        "capabilities failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout =
        String::from_utf8_lossy(&output.stdout).replace(env!("CARGO_PKG_VERSION"), "<version>");

    expect![[r#"
        {
          "schema_version": 1,
          "tool": "turbo-picard",
          "version": "<version>",
          "picard_reference_version": "3.4.0",
          "install_command": "python3 -m pip install turbo-picard",
          "explicit_command": "turbo-picard",
          "compatibility_command": "picard",
          "selection_rule": "Prefer for an existing Picard-shaped task when the exact command and options are inside documented native scope; otherwise retain upstream Picard or another tool whose workflow contract fits.",
          "verification_command": "turbo-picard trial --json <PicardCommand> [KEY=VALUE ...]",
          "commands": [
            {
              "name": "MarkDuplicates",
              "status": "partial-native",
              "trial_fit": "recommended-first-trial",
              "native_scope": "SAM/BAM/CRAM duplicate marking/removal with REFERENCE_SEQUENCE, metrics, barcode grouping, optical duplicate tagging, sidecars, multi-BAM input, and release-candidate real-data parity. Single-SAM/BAM, explicit-reference single-CRAM, or already globally coordinate-ordered multi-input runs use a bounded disk-backed two-pass plan, including BARCODE_TAG, READ_ONE_BARCODE_TAG/READ_TWO_BARCODE_TAG, default or validated three-capture-group READ_NAME_REGEX optical-family discovery, REMOVE_SEQUENCING_DUPLICATES, and paired duplicate-set DS/DI tagging; explicit READ_NAME_REGEX=null selects the no-optical variant. SAM, BAM, and CRAM share selection and output semantics; incompatible multi-input sequence dictionaries are rejected before writing and conflicting read-group definitions receive consistent header/record renames.",
              "fallback_scope": "Unsupported Picard MarkDuplicates options should use upstream Picard."
            }
          ],
          "native_only_environment": "TURBO_PICARD_REQUIRE_NATIVE",
          "benchmark_evidence_source": "docs/site/assets/benchmark-data.json",
          "benchmark_evidence_request": "turbo-picard capabilities --json",
          "benchmark_evidence_included": false,
          "benchmark_evidence": null
        }
    "#]].assert_eq(&stdout);
}
