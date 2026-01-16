use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_diff_without_manifest() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");
    
    // Create minimal config
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
        .arg("diff");
    
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("No manifest found"));
}

#[test]
fn test_apply_without_manifest() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");
    
    // Create minimal config
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
        .arg("apply");
    
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("No manifest found"));
}

#[test]
fn test_apply_with_only_filter() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");
    
    // Create minimal config
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
        .arg("apply")
        .arg("--only")
        .arg("~/.zshrc,~/.bashrc");
    
    // Should still warn about no manifest
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("No manifest found"));
}

#[test]
fn test_apply_force_flag() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");
    
    // Create minimal config
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
        .arg("apply")
        .arg("--force");
    
    // Force flag should be accepted
    cmd.assert().success();
}

#[cfg(test)]
mod integration {
    use super::*;
    
    #[test]
    fn test_diff_shows_changes() {
        let temp_dir = TempDir::new().unwrap();
        let dotdipper_dir = temp_dir.path().join(".dotdipper");
        let compiled_dir = dotdipper_dir.join("compiled");
        let manifest_path = dotdipper_dir.join("manifest.lock");
        let config_path = dotdipper_dir.join("config.toml");
        
        // Create directory structure
        fs::create_dir_all(&compiled_dir).unwrap();
        
        // Create a test file in compiled
        let test_file = compiled_dir.join("test.txt");
        fs::write(&test_file, "test content").unwrap();
        
        // Create manifest
        let manifest = serde_json::json!({
            "version": "1.0.0",
            "created": "2025-10-02T00:00:00Z",
            "files": {
                "test.txt": {
                    "path": "test.txt",
                    "hash": "dummy_hash",
                    "size": 12,
                    "mode": 0o644,
                    "modified": "2025-10-02T00:00:00Z"
                }
            }
        });
        
        fs::write(&manifest_path, serde_json::to_string_pretty(&manifest).unwrap()).unwrap();
        
        // Create config
        fs::write(
            &config_path,
            r#"
[general]
tracked_files = []
"#,
        )
        .unwrap();
        
        // Set HOME to temp dir for test
        let mut cmd = Command::cargo_bin("dotdipper").unwrap();
        cmd.env("HOME", temp_dir.path())
            .arg("--config")
            .arg(&config_path)
            .arg("diff");
        
        cmd.assert()
            .success()
            .stdout(predicate::str::contains("Diff Summary"));
    }
}

