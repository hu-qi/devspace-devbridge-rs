use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn prints_version() {
    Command::cargo_bin("devbridge")
        .unwrap()
        .arg("version")
        .assert()
        .success()
        .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn help_lists_core_commands() {
    Command::cargo_bin("devbridge")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("tunnel"))
        .stdout(predicate::str::contains("port"))
        .stdout(predicate::str::contains("auth"));
}
