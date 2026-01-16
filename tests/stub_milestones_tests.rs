/// Tests for milestone features that are now fully implemented
/// These tests verify that the commands work correctly (not stubs anymore)

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_snapshot_list() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");
    
    fs::write(
        &config_path,
        r#"
[general]
tracked_files = []
"#,
    )
    .unwrap();
    
    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.arg("--config")
        .arg(&config_path)
        .arg("snapshot")
        .arg("list");
    
    // Now fully implemented - should succeed
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("snapshots"));
}

#[test]
fn test_profile_list() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");
    
    fs::write(
        &config_path,
        r#"
[general]
tracked_files = []
"#,
    )
    .unwrap();
    
    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.arg("--config")
        .arg(&config_path)
        .arg("profile")
        .arg("list");
    
    // Now fully implemented - should succeed and show profiles
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("profiles"));
}

#[test]
fn test_remote_show() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");
    
    fs::write(
        &config_path,
        r#"
[general]
tracked_files = []
"#,
    )
    .unwrap();
    
    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.arg("--config")
        .arg(&config_path)
        .arg("remote")
        .arg("show");
    
    // Now fully implemented - should succeed (shows "No remote configured")
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("remote"));
}

#[test]
fn test_daemon_status() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");
    
    fs::write(
        &config_path,
        r#"
[general]
tracked_files = []
"#,
    )
    .unwrap();
    
    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.arg("--config")
        .arg(&config_path)
        .arg("daemon")
        .arg("status");
    
    // Now fully implemented - should succeed (shows daemon status)
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Daemon"));
}

#[test]
fn test_all_milestone_commands_exist() {
    // Verify all milestone commands are accessible and working
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");
    
    fs::write(
        &config_path,
        r#"
[general]
tracked_files = []
"#,
    )
    .unwrap();
    
    // Test snapshot list
    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.arg("--config")
        .arg(&config_path)
        .arg("snapshot")
        .arg("list");
    cmd.assert().success();
    
    // Test profile list
    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.arg("--config")
        .arg(&config_path)
        .arg("profile")
        .arg("list");
    cmd.assert().success();
    
    // Test remote show
    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.arg("--config")
        .arg(&config_path)
        .arg("remote")
        .arg("show");
    cmd.assert().success();
    
    // Test daemon status
    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.arg("--config")
        .arg(&config_path)
        .arg("daemon")
        .arg("status");
    cmd.assert().success();
}

#[test]
fn test_remote_set_localfs_requires_endpoint() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");
    
    fs::write(
        &config_path,
        r#"
[general]
tracked_files = []
"#,
    )
    .unwrap();
    
    // Test that localfs without --endpoint fails with helpful message
    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.arg("--config")
        .arg(&config_path)
        .arg("remote")
        .arg("set")
        .arg("localfs");
    
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("--endpoint"));
}

#[test]
fn test_remote_set_localfs_with_endpoint() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");
    let backup_dir = temp_dir.path().join("backup");
    fs::create_dir_all(&backup_dir).unwrap();
    
    fs::write(
        &config_path,
        r#"
[general]
tracked_files = []
"#,
    )
    .unwrap();
    
    // Test that localfs with --endpoint succeeds
    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.arg("--config")
        .arg(&config_path)
        .arg("remote")
        .arg("set")
        .arg("localfs")
        .arg("--endpoint")
        .arg(backup_dir.to_str().unwrap());
    
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Remote configured"));
}

#[test]
fn test_remote_set_s3_requires_bucket() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");
    
    fs::write(
        &config_path,
        r#"
[general]
tracked_files = []
"#,
    )
    .unwrap();
    
    // Test that s3 without --bucket fails with helpful message
    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.arg("--config")
        .arg(&config_path)
        .arg("remote")
        .arg("set")
        .arg("s3");
    
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("--bucket"));
}

#[test]
fn test_remote_set_s3_with_options() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");
    
    fs::write(
        &config_path,
        r#"
[general]
tracked_files = []
"#,
    )
    .unwrap();
    
    // Test that s3 with --bucket succeeds
    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.arg("--config")
        .arg(&config_path)
        .arg("remote")
        .arg("set")
        .arg("s3")
        .arg("--bucket")
        .arg("my-dotfiles")
        .arg("--region")
        .arg("us-west-2");
    
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Remote configured"));
}
