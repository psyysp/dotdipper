use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_hooks_configuration_parsing() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");
    
    // Create config with hooks
    fs::write(
        &config_path,
        r#"
[general]
tracked_files = []

[hooks]
pre_apply = ["echo 'before'"]
post_apply = ["echo 'after'"]
pre_snapshot = []
post_snapshot = []
"#,
    )
    .unwrap();
    
    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.arg("--config")
        .arg(&config_path)
        .arg("config")
        .arg("--show");
    
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("pre_apply"))
        .stdout(predicate::str::contains("post_apply"));
}

#[test]
fn test_snapshot_with_hooks() {
    let temp_dir = TempDir::new().unwrap();
    let dotdipper_dir = temp_dir.path().join(".dotdipper");
    fs::create_dir_all(&dotdipper_dir).unwrap();
    
    let config_path = dotdipper_dir.join("config.toml");
    
    // Create config with snapshot hooks
    fs::write(
        &config_path,
        r#"
[general]
tracked_files = []

[hooks]
pre_snapshot = ["echo 'Creating snapshot'"]
post_snapshot = ["echo 'Snapshot complete'"]
"#,
    )
    .unwrap();
    
    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.env("HOME", temp_dir.path())
        .arg("--config")
        .arg(&config_path)
        .arg("snapshot");
    
    // Should execute hooks
    cmd.assert().success();
}

#[test]
fn test_failing_hook_stops_execution() {
    let temp_dir = TempDir::new().unwrap();
    let dotdipper_dir = temp_dir.path().join(".dotdipper");
    fs::create_dir_all(&dotdipper_dir).unwrap();
    
    let config_path = dotdipper_dir.join("config.toml");
    
    // Create config with failing hook
    fs::write(
        &config_path,
        r#"
[general]
tracked_files = []

[hooks]
pre_snapshot = ["exit 1"]
"#,
    )
    .unwrap();
    
    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.env("HOME", temp_dir.path())
        .arg("--config")
        .arg(&config_path)
        .arg("snapshot");
    
    // Should fail due to hook failure
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Hook failed"));
}

