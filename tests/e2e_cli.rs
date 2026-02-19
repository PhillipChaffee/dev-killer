use assert_cmd::cargo_bin_cmd;
use predicates::prelude::*;

#[test]
fn test_help_shows_usage() {
    cargo_bin_cmd!("dev-killer")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("autonomous coding agent"));
}

#[test]
fn test_version_shows_version() {
    cargo_bin_cmd!("dev-killer")
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("dev-killer"));
}

#[test]
fn test_sessions_command_works_with_no_sessions() {
    cargo_bin_cmd!("dev-killer")
        .arg("sessions")
        .assert()
        .success();
}

#[test]
fn test_unknown_provider_fails_gracefully() {
    cargo_bin_cmd!("dev-killer")
        .args(["--provider", "nonexistent", "run", "--simple", "hello"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown provider"));
}

#[test]
fn test_run_without_api_key_fails_gracefully() {
    cargo_bin_cmd!("dev-killer")
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("OPENAI_API_KEY")
        .args(["run", "--simple", "hello"])
        .assert()
        .failure();
}
