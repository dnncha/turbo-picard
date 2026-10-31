use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn example_inspect_capabilities_before_choosing_a_command() {
    Command::cargo_bin("turbo-picard")
        .expect("binary exists")
        .args(["capabilities", "--json", "--command", "MarkDuplicates"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"name\": \"MarkDuplicates\""))
        .stdout(predicate::str::contains("\"status\": \"partial-native\""))
        .stdout(predicate::str::contains(
            "\"trial_fit\": \"recommended-first-trial\"",
        ))
        .stdout(predicate::str::contains("\"benchmark_evidence\": null"));
}

#[test]
fn example_print_a_trial_before_changing_a_workflow() {
    Command::cargo_bin("turbo-picard")
        .expect("binary exists")
        .env("TURBO_PICARD_REQUIRE_NATIVE", "1")
        .args([
            "trial",
            "MarkDuplicates",
            "I=input.bam",
            "O=marked.bam",
            "M=metrics.txt",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("inspection_only=true"))
        .stdout(predicate::str::contains("command=MarkDuplicates"))
        .stdout(predicate::str::contains(
            "trial_fit=recommended-first-trial",
        ))
        .stdout(predicate::str::contains("picard MarkDuplicates"))
        .stdout(predicate::str::contains("turbo-picard MarkDuplicates"))
        .stdout(predicate::str::contains("I=input.bam"))
        .stdout(predicate::str::contains("O=marked.bam"));
}

#[test]
fn example_shim_reports_the_same_version() {
    let explicit = Command::cargo_bin("turbo-picard")
        .expect("binary exists")
        .arg("--version")
        .output()
        .expect("version runs");
    let shim = Command::cargo_bin("picard")
        .expect("shim exists")
        .arg("--version")
        .output()
        .expect("shim version runs");

    assert!(explicit.status.success());
    assert!(shim.status.success());

    let explicit = String::from_utf8_lossy(&explicit.stdout);
    let shim = String::from_utf8_lossy(&shim.stdout);
    assert!(explicit.starts_with("turbo-picard "));
    assert!(shim.starts_with("picard "));
    assert_eq!(
        explicit.trim().split_once(' ').map(|(_, version)| version),
        shim.trim().split_once(' ').map(|(_, version)| version)
    );
}
