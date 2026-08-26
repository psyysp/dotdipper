/// Tests for milestone features that are now fully implemented
/// These tests verify that the commands work correctly (not stubs anymore)
use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

fn isolated_bin(temp_dir: &TempDir, config_path: &Path) -> Command {
    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.env("HOME", temp_dir.path())
        .env("DOTDIPPER_HOME", temp_dir.path())
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("DOTDIPPER_PROFILE")
        .arg("--config")
        .arg(config_path);
    cmd
}

fn write_minimal_config(temp_dir: &TempDir) -> std::path::PathBuf {
    let config_path = temp_dir.path().join("config.toml");
    fs::write(
        &config_path,
        r#"
[general]
tracked_files = []
"#,
    )
    .unwrap();
    config_path
}

#[test]
fn test_snapshot_list() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = write_minimal_config(&temp_dir);

    isolated_bin(&temp_dir, &config_path)
        .arg("snapshot")
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("snapshots"));
}

#[test]
fn test_profile_list() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = write_minimal_config(&temp_dir);

    isolated_bin(&temp_dir, &config_path)
        .arg("profile")
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("profiles"));
}

#[test]
fn test_remote_show() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = write_minimal_config(&temp_dir);

    isolated_bin(&temp_dir, &config_path)
        .arg("remote")
        .arg("show")
        .assert()
        .success()
        .stdout(predicate::str::contains("remote"));
}

#[test]
fn test_daemon_status() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = write_minimal_config(&temp_dir);

    isolated_bin(&temp_dir, &config_path)
        .arg("daemon")
        .arg("status")
        .assert()
        .success()
        .stdout(predicate::str::contains("Daemon"));
}

#[test]
fn test_all_milestone_commands_exist() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = write_minimal_config(&temp_dir);

    isolated_bin(&temp_dir, &config_path)
        .arg("snapshot")
        .arg("list")
        .assert()
        .success();

    isolated_bin(&temp_dir, &config_path)
        .arg("profile")
        .arg("list")
        .assert()
        .success();

    isolated_bin(&temp_dir, &config_path)
        .arg("remote")
        .arg("show")
        .assert()
        .success();

    isolated_bin(&temp_dir, &config_path)
        .arg("daemon")
        .arg("status")
        .assert()
        .success();
}

#[test]
fn test_remote_set_localfs_requires_endpoint() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = write_minimal_config(&temp_dir);

    isolated_bin(&temp_dir, &config_path)
        .arg("remote")
        .arg("set")
        .arg("localfs")
        .assert()
        .failure()
        .stderr(predicate::str::contains("--endpoint"));
}

#[test]
fn test_remote_set_localfs_with_endpoint() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = write_minimal_config(&temp_dir);
    let backup_dir = temp_dir.path().join("backup");
    fs::create_dir_all(&backup_dir).unwrap();

    isolated_bin(&temp_dir, &config_path)
        .arg("remote")
        .arg("set")
        .arg("localfs")
        .arg("--endpoint")
        .arg(backup_dir.to_str().unwrap())
        .assert()
        .success()
        .stdout(predicate::str::contains("Remote configured"));
}

#[test]
fn test_remote_set_s3_requires_bucket() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = write_minimal_config(&temp_dir);

    isolated_bin(&temp_dir, &config_path)
        .arg("remote")
        .arg("set")
        .arg("s3")
        .assert()
        .failure()
        .stderr(predicate::str::contains("--bucket"));
}

#[test]
fn test_remote_set_s3_with_options() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = write_minimal_config(&temp_dir);

    isolated_bin(&temp_dir, &config_path)
        .arg("remote")
        .arg("set")
        .arg("s3")
        .arg("--bucket")
        .arg("my-dotfiles")
        .arg("--region")
        .arg("us-west-2")
        .assert()
        .success()
        .stdout(predicate::str::contains("Remote configured"));
}
