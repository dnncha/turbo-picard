use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn unsupported_command_fails_clearly() {
    let mut cmd = Command::cargo_bin("jeanluc").expect("binary exists");
    cmd.arg("SortSam").assert().failure().stderr(
        predicate::str::contains("unsupported Picard command: SortSam"),
    );
}
