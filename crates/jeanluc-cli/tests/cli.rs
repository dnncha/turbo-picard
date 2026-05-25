use assert_cmd::Command;
use predicates::prelude::*;

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
        "OPTICAL_DUPLICATE_PIXEL_DISTANCE=2500",
    ])
    .assert()
    .failure()
    .stderr(predicate::str::contains(
        "unsupported MarkDuplicates argument: OPTICAL_DUPLICATE_PIXEL_DISTANCE",
    ));
}
