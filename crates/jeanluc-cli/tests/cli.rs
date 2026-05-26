use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;

#[test]
fn unsupported_command_fails_clearly() {
    let mut cmd = Command::cargo_bin("jeanluc").expect("binary exists");
    cmd.arg("SortSam")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "unsupported Picard command: SortSam",
        ));
}

#[test]
fn markduplicates_requires_metrics_file() {
    let mut cmd = Command::cargo_bin("jeanluc").expect("binary exists");
    cmd.args(["MarkDuplicates", "I=in.bam", "O=out.bam"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "missing required MarkDuplicates argument: METRICS_FILE",
        ));
}

#[test]
fn markduplicates_rejects_unsupported_option() {
    let mut cmd = Command::cargo_bin("jeanluc").expect("binary exists");
    cmd.args([
        "MarkDuplicates",
        "I=in.bam",
        "O=out.bam",
        "M=metrics.txt",
        "TAGGING_POLICY=All",
    ])
    .assert()
    .failure()
    .stderr(predicate::str::contains(
        "unsupported MarkDuplicates argument: TAGGING_POLICY=All",
    ));
}

#[test]
fn markduplicates_marks_duplicate_sam_records() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("output.sam");
    let metrics = tempdir.path().join("metrics.txt");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "read-a\t0\tchr1\t10\t60\t10M\t*\t0\t0\tAAAAAAAAAA\tFFFFFFFFFF\n",
            "read-b\t0\tchr1\t10\t60\t10M\t*\t0\t0\tAAAAAAAAAA\tFFFFFFFFFF\n",
            "read-c\t0\tchr1\t50\t60\t10M\t*\t0\t0\tCCCCCCCCCC\tFFFFFFFFFF\n",
        ),
    )
    .expect("input fixture is written");

    let mut cmd = Command::cargo_bin("jeanluc").expect("binary exists");
    cmd.args([
        "MarkDuplicates",
        &format!("I={}", input.display()),
        &format!("O={}", output.display()),
        &format!("M={}", metrics.display()),
    ])
    .assert()
    .success();

    let output_sam = fs::read_to_string(&output).expect("output SAM exists");
    assert!(output_sam.contains("read-a\t0\tchr1\t10"));
    assert!(output_sam.contains("read-b\t1024\tchr1\t10"));
    assert!(output_sam.contains("read-c\t0\tchr1\t50"));

    let metrics_text = fs::read_to_string(&metrics).expect("metrics file exists");
    assert!(metrics_text.contains("UNPAIRED_READ_DUPLICATES"));
    assert!(metrics_text.contains("Unknown Library\t3\t0\t0\t0\t1\t0\t0\t0.333333\t\n"));
}
