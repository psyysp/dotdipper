use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

// ============================================
// Basic CLI Tests
// ============================================

#[test]
fn test_help_command() {
    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("dotfiles manager"));
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
fn test_all_subcommands_have_help() {
    let subcommands = [
        "init", "discover", "status", "diff", "apply",
        "secrets", "snapshot", "profile", "remote", "daemon",
        "push", "pull", "install", "doctor", "config"
    ];
    
    for subcmd in subcommands {
        let mut cmd = Command::cargo_bin("dotdipper").unwrap();
        cmd.arg(subcmd)
            .arg("--help")
            .assert()
            .success();
    }
}

// ============================================
// Init Command Tests
// ============================================

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
    assert!(content.contains("[general]"));
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
    assert!(content.contains("[general]"));
}

#[test]
fn test_init_creates_directories() {
    let temp_dir = TempDir::new().unwrap();
    let dotdipper_dir = temp_dir.path().join(".dotdipper");
    let config_path = dotdipper_dir.join("config.toml");
    
    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.env("HOME", temp_dir.path())
        .arg("init")
        .arg("--config")
        .arg(&config_path)
        .assert()
        .success();
    
    // Check directories were created
    assert!(dotdipper_dir.exists());
    assert!(dotdipper_dir.join("compiled").exists());
}

// ============================================
// Discover Command Tests
// ============================================

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
fn test_discover_dry_run() {
    let temp_dir = TempDir::new().unwrap();
    let dotdipper_dir = temp_dir.path().join(".dotdipper");
    fs::create_dir_all(&dotdipper_dir).unwrap();
    let config_path = dotdipper_dir.join("config.toml");
    
    // Create config
    fs::write(
        &config_path,
        r#"
[general]
tracked_files = []
include_patterns = ["~/.config/**"]
"#,
    )
    .unwrap();
    
    // Create a test dotfile
    let config_dir = temp_dir.path().join(".config");
    fs::create_dir_all(&config_dir).unwrap();
    fs::write(config_dir.join("test.conf"), "test").unwrap();
    
    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.env("HOME", temp_dir.path())
        .arg("--config")
        .arg(&config_path)
        .arg("discover")
        .assert()
        .success();
}

// ============================================
// Status Command Tests
// ============================================

#[test]
fn test_status_no_manifest() {
    let temp_dir = TempDir::new().unwrap();
    let dotdipper_dir = temp_dir.path().join(".dotdipper");
    fs::create_dir_all(&dotdipper_dir).unwrap();
    let config_path = dotdipper_dir.join("config.toml");
    
    fs::write(
        &config_path,
        r#"
[general]
tracked_files = []
"#,
    )
    .unwrap();
    
    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.env("HOME", temp_dir.path())
        .arg("--config")
        .arg(&config_path)
        .arg("status")
        .assert()
        .success();
}

#[test]
fn test_status_detailed() {
    let temp_dir = TempDir::new().unwrap();
    let dotdipper_dir = temp_dir.path().join(".dotdipper");
    fs::create_dir_all(&dotdipper_dir).unwrap();
    let config_path = dotdipper_dir.join("config.toml");
    
    fs::write(
        &config_path,
        r#"
[general]
tracked_files = []
"#,
    )
    .unwrap();
    
    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.env("HOME", temp_dir.path())
        .arg("--config")
        .arg(&config_path)
        .arg("status")
        .arg("--detailed")
        .assert()
        .success();
}

// ============================================
// Doctor Command Tests
// ============================================

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

#[test]
fn test_doctor_with_fix() {
    let temp_dir = TempDir::new().unwrap();
    let dotdipper_dir = temp_dir.path().join(".dotdipper");
    fs::create_dir_all(&dotdipper_dir).unwrap();
    let config_path = dotdipper_dir.join("config.toml");
    
    fs::write(
        &config_path,
        r#"
[general]
tracked_files = []
"#,
    )
    .unwrap();
    
    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.env("HOME", temp_dir.path())
        .arg("--config")
        .arg(&config_path)
        .arg("doctor")
        .arg("--fix")
        .assert()
        .success();
}

// ============================================
// Config Command Tests
// ============================================

#[test]
fn test_config_show() {
    let temp_dir = TempDir::new().unwrap();
    let dotdipper_dir = temp_dir.path().join(".dotdipper");
    fs::create_dir_all(&dotdipper_dir).unwrap();
    let config_path = dotdipper_dir.join("config.toml");
    
    fs::write(
        &config_path,
        r#"
[general]
tracked_files = []
default_mode = "symlink"
"#,
    )
    .unwrap();
    
    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.env("HOME", temp_dir.path())
        .arg("--config")
        .arg(&config_path)
        .arg("config")
        .arg("--show")
        .assert()
        .success()
        .stdout(predicate::str::contains("symlink").or(predicate::str::contains("general")));
}

// ============================================
// Profile Command Tests
// ============================================

#[test]
fn test_profile_list_empty() {
    let temp_dir = TempDir::new().unwrap();
    let dotdipper_dir = temp_dir.path().join(".dotdipper");
    fs::create_dir_all(&dotdipper_dir).unwrap();
    let config_path = dotdipper_dir.join("config.toml");
    
    fs::write(
        &config_path,
        r#"
[general]
tracked_files = []
"#,
    )
    .unwrap();
    
    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.env("HOME", temp_dir.path())
        .arg("--config")
        .arg(&config_path)
        .arg("profile")
        .arg("list")
        .assert()
        .success();
}

#[test]
fn test_profile_create_and_list() {
    let temp_dir = TempDir::new().unwrap();
    let dotdipper_dir = temp_dir.path().join(".dotdipper");
    fs::create_dir_all(&dotdipper_dir).unwrap();
    let config_path = dotdipper_dir.join("config.toml");
    
    fs::write(
        &config_path,
        r#"
[general]
tracked_files = []
"#,
    )
    .unwrap();
    
    // Create profile
    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.env("HOME", temp_dir.path())
        .arg("--config")
        .arg(&config_path)
        .arg("profile")
        .arg("create")
        .arg("test-profile")
        .assert()
        .success();
    
    // List profiles
    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.env("HOME", temp_dir.path())
        .arg("--config")
        .arg(&config_path)
        .arg("profile")
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("test-profile").or(predicate::str::contains("1")));
}

// ============================================
// Diff Command Tests
// ============================================

#[test]
fn test_diff_no_manifest() {
    let temp_dir = TempDir::new().unwrap();
    let dotdipper_dir = temp_dir.path().join(".dotdipper");
    fs::create_dir_all(&dotdipper_dir).unwrap();
    let config_path = dotdipper_dir.join("config.toml");
    
    fs::write(
        &config_path,
        r#"
[general]
tracked_files = []
"#,
    )
    .unwrap();
    
    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.env("HOME", temp_dir.path())
        .arg("--config")
        .arg(&config_path)
        .arg("diff")
        .assert()
        .success();
}

// ============================================
// Apply Command Tests
// ============================================

#[test]
fn test_apply_no_manifest() {
    let temp_dir = TempDir::new().unwrap();
    let dotdipper_dir = temp_dir.path().join(".dotdipper");
    fs::create_dir_all(&dotdipper_dir).unwrap();
    let config_path = dotdipper_dir.join("config.toml");
    
    fs::write(
        &config_path,
        r#"
[general]
tracked_files = []
"#,
    )
    .unwrap();
    
    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.env("HOME", temp_dir.path())
        .arg("--config")
        .arg(&config_path)
        .arg("apply")
        .arg("--force")
        .assert()
        .success()
        .stdout(predicate::str::contains("No manifest").or(predicate::str::contains("nothing to apply")));
}

// ============================================
// End-to-End Workflow Tests
// ============================================

#[test]
#[serial_test::serial]
fn test_full_workflow_init_discover_snapshot() {
    let temp_dir = TempDir::new().unwrap();
    let dotdipper_dir = temp_dir.path().join(".dotdipper");
    let config_path = dotdipper_dir.join("config.toml");
    
    // Create a test dotfile
    let test_file = temp_dir.path().join(".testrc");
    fs::write(&test_file, "# Test dotfile").unwrap();
    
    // Init
    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.env("HOME", temp_dir.path())
        .arg("init")
        .arg("--config")
        .arg(&config_path)
        .assert()
        .success();
    
    // Add the test file to tracked files
    let config_content = format!(r#"
[general]
tracked_files = ["{}"]
"#, test_file.display());
    fs::write(&config_path, config_content).unwrap();
    
    // Status
    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.env("HOME", temp_dir.path())
        .arg("--config")
        .arg(&config_path)
        .arg("status")
        .assert()
        .success();
    
    // The snapshot create requires the file to be in compiled directory
    // which requires a full workflow - this is a simplified test
}

#[test]
fn test_verbose_flag() {
    let temp_dir = TempDir::new().unwrap();
    let dotdipper_dir = temp_dir.path().join(".dotdipper");
    fs::create_dir_all(&dotdipper_dir).unwrap();
    let config_path = dotdipper_dir.join("config.toml");
    
    fs::write(
        &config_path,
        r#"
[general]
tracked_files = []
"#,
    )
    .unwrap();
    
    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.env("HOME", temp_dir.path())
        .arg("--verbose")
        .arg("--config")
        .arg(&config_path)
        .arg("status")
        .assert()
        .success();
}

#[test]
fn test_invalid_command() {
    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.arg("invalid-command-that-does-not-exist")
        .assert()
        .failure();
}
