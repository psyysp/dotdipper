use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_help_command() {
    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("A smart dotfiles manager"));
}

#[test]
fn test_version_command() {
    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("dotdipper"));
}

#[test]
fn test_init_command() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");
    
    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.arg("init")
        .arg("--config")
        .arg(&config_path)
        .assert()
        .success();
    
    // Check that config file was created
    assert!(config_path.exists());
    
    // Check config content
    let content = fs::read_to_string(&config_path).unwrap();
    assert!(content.contains("[dotfiles]"));
    assert!(content.contains("[github]"));
    assert!(content.contains("[packages]"));
}

#[test]
fn test_init_fails_when_config_exists() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");
    
    // Create config file
    fs::write(&config_path, "test").unwrap();
    
    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.arg("init")
        .arg("--config")
        .arg(&config_path)
        .assert()
        .failure()
        .stderr(predicate::str::contains("already exists"));
}

#[test]
fn test_init_force_overwrites() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");
    
    // Create config file with test content
    fs::write(&config_path, "test content").unwrap();
    
    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.arg("init")
        .arg("--config")
        .arg(&config_path)
        .arg("--force")
        .assert()
        .success();
    
    // Check that config was overwritten
    let content = fs::read_to_string(&config_path).unwrap();
    assert!(!content.contains("test content"));
    assert!(content.contains("[dotfiles]"));
}

#[test]
fn test_discover_without_init() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");
    
    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.arg("discover")
        .arg("--config")
        .arg(&config_path)
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

#[test]
#[serial_test::serial]
fn test_doctor_command() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");
    
    // Initialize first
    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.arg("init")
        .arg("--config")
        .arg(&config_path)
        .assert()
        .success();
    
    // Run doctor
    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.arg("doctor")
        .arg("--config")
        .arg(&config_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("Config file exists"));
}
