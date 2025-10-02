use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_snapshot_cmd_stub() {
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
        .arg("snapshot-cmd")
        .arg("list");
    
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("not yet implemented"));
}

#[test]
fn test_profile_stub() {
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
    
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("not yet implemented"));
}

#[test]
fn test_remote_stub() {
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
    
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("not yet implemented"));
}

#[test]
fn test_daemon_stub() {
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
    
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("not yet implemented"));
}

#[test]
fn test_all_stub_commands_exist() {
    // Verify all stub commands are accessible
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
    
    // Test snapshot-cmd subcommands
    for subcmd in &["create", "list", "rollback", "delete"] {
        let mut cmd = Command::cargo_bin("dotdipper").unwrap();
        cmd.arg("--config")
            .arg(&config_path)
            .arg("snapshot-cmd")
            .arg(subcmd);
        
        if *subcmd == "rollback" || *subcmd == "delete" {
            cmd.arg("test-id");
        }
        
        // All should fail with "not implemented" message
        let output = cmd.output().unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("not yet implemented") || stderr.contains("Not implemented"));
    }
    
    // Test profile subcommands
    for subcmd in &["list", "create", "switch", "remove"] {
        let mut cmd = Command::cargo_bin("dotdipper").unwrap();
        cmd.arg("--config")
            .arg(&config_path)
            .arg("profile")
            .arg(subcmd);
        
        if *subcmd != "list" {
            cmd.arg("test-profile");
        }
        
        let output = cmd.output().unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("not yet implemented") || stderr.contains("Not implemented"));
    }
}

