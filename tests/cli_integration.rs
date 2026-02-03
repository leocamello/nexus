//! CLI Integration Tests for F04
//!
//! End-to-end tests for CLI commands using assert_cmd.

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

/// Get the nexus binary for testing
fn nexus_cmd() -> Command {
    Command::cargo_bin("nexus").unwrap()
}

#[test]
fn test_version_output() {
    nexus_cmd()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("nexus"));
}

#[test]
fn test_help_shows_all_commands() {
    nexus_cmd()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("serve"))
        .stdout(predicate::str::contains("backends"))
        .stdout(predicate::str::contains("models"))
        .stdout(predicate::str::contains("health"))
        .stdout(predicate::str::contains("config"))
        .stdout(predicate::str::contains("completions"));
}

#[test]
fn test_serve_help() {
    nexus_cmd()
        .args(["serve", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--config"))
        .stdout(predicate::str::contains("--port"))
        .stdout(predicate::str::contains("--host"));
}

#[test]
fn test_backends_help() {
    nexus_cmd()
        .args(["backends", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("list"))
        .stdout(predicate::str::contains("add"))
        .stdout(predicate::str::contains("remove"));
}

#[test]
fn test_config_init_creates_file() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("nexus.toml");

    nexus_cmd()
        .args(["config", "init", "-o", config_path.to_str().unwrap()])
        .assert()
        .success();

    assert!(config_path.exists());
    let content = std::fs::read_to_string(&config_path).unwrap();
    assert!(content.contains("[server]"));
}

#[test]
fn test_config_init_no_overwrite() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("nexus.toml");

    // Create file first
    std::fs::write(&config_path, "existing content").unwrap();

    // Try to overwrite without --force
    nexus_cmd()
        .args(["config", "init", "-o", config_path.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("exists"));
}

#[test]
fn test_config_init_force_overwrites() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("nexus.toml");

    // Create file first
    std::fs::write(&config_path, "existing content").unwrap();

    // Force overwrite
    nexus_cmd()
        .args([
            "config",
            "init",
            "-o",
            config_path.to_str().unwrap(),
            "--force",
        ])
        .assert()
        .success();

    let content = std::fs::read_to_string(&config_path).unwrap();
    assert!(content.contains("[server]"));
}

#[test]
fn test_invalid_command() {
    nexus_cmd()
        .arg("invalid-command")
        .assert()
        .failure()
        .stderr(predicate::str::contains("error"));
}

#[test]
fn test_completions_bash() {
    nexus_cmd()
        .args(["completions", "bash"])
        .assert()
        .success()
        .stdout(predicate::str::contains("complete"));
}

#[test]
fn test_completions_zsh() {
    nexus_cmd()
        .args(["completions", "zsh"])
        .assert()
        .success()
        .stdout(predicate::str::contains("compdef"));
}
