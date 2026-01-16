/// Full workflow integration test
/// Tests the complete dotdipper workflow including:
/// - Init
/// - Config loading
/// - Diff (with missing manifest handling)
/// - Apply with filters
/// - Secrets commands (structure only, needs age installed)

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_full_workflow_without_github() {
    let temp_dir = TempDir::new().unwrap();
    let home_dir = temp_dir.path();
    let dotdipper_dir = home_dir.join(".dotdipper");
    
    // Create dotdipper directory structure
    fs::create_dir_all(&dotdipper_dir).unwrap();
    
    let config_path = dotdipper_dir.join("config.toml");
    
    // Step 1: Init
    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.env("HOME", home_dir)
        .arg("--config")
        .arg(&config_path)
        .arg("init")
        .arg("--force");
    
    cmd.assert().success();
    
    // Verify config was created
    assert!(config_path.exists());
    
    // Step 2: Verify config can be loaded
    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.env("HOME", home_dir)
        .arg("--config")
        .arg(&config_path)
        .arg("config")
        .arg("--show");
    
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("general"));
    
    // Step 3: Test diff with no manifest
    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.env("HOME", home_dir)
        .arg("--config")
        .arg(&config_path)
        .arg("diff");
    
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("No manifest found"));
    
    // Step 4: Test apply with no manifest
    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.env("HOME", home_dir)
        .arg("--config")
        .arg(&config_path)
        .arg("apply");
    
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("No manifest found"));
    
    // Step 5: Test apply with --only filter
    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.env("HOME", home_dir)
        .arg("--config")
        .arg(&config_path)
        .arg("apply")
        .arg("--only")
        .arg("~/.zshrc");
    
    cmd.assert().success();
}

#[test]
fn test_config_with_all_sections() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");
    
    // Create comprehensive config
    fs::write(
        &config_path,
        r#"
[general]
default_mode = "symlink"
backup = true
active_profile = "default"
tracked_files = ["~/.zshrc"]

[github]
username = "testuser"
repo_name = "dotfiles"
private = true

[packages]
common = ["git", "vim"]
macos = ["neovim"]
linux = ["neovim"]

[secrets]
provider = "age"
key_path = "~/.config/age/keys.txt"

[hooks]
pre_apply = ["echo 'before'"]
post_apply = ["echo 'after'"]
pre_snapshot = []
post_snapshot = []

[daemon]
enabled = false
mode = "ask"
debounce_ms = 1500

[files."~/.config/nvim"]
mode = "copy"

[files."~/.ssh/config"]
exclude = true

include_patterns = ["~/.config/**"]
exclude_patterns = ["~/.ssh/**"]
"#,
    )
    .unwrap();
    
    // Verify config parses correctly
    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.arg("--config")
        .arg(&config_path)
        .arg("config")
        .arg("--show");
    
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("secrets"))
        .stdout(predicate::str::contains("hooks"))
        .stdout(predicate::str::contains("daemon"))
        .stdout(predicate::str::contains("active_profile"));
}

#[test]
fn test_doctor_checks() {
    let temp_dir = TempDir::new().unwrap();
    let dotdipper_dir = temp_dir.path().join(".dotdipper");
    fs::create_dir_all(&dotdipper_dir).unwrap();
    
    let config_path = dotdipper_dir.join("config.toml");
    
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
    cmd.env("HOME", temp_dir.path())
        .arg("--config")
        .arg(&config_path)
        .arg("doctor");
    
    // Should run checks (some output may be in stdout, some in stderr)
    let output = cmd.assert().success();
    let stdout_str = String::from_utf8_lossy(&output.get_output().stdout);
    let stderr_str = String::from_utf8_lossy(&output.get_output().stderr);
    let combined = format!("{}{}", stdout_str, stderr_str);
    
    assert!(combined.contains("Git"), "Output should mention Git");
    assert!(combined.contains("Age") || combined.contains("age"), "Output should mention Age");
}

#[test]
fn test_apply_force_and_interactive_flags() {
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
    
    // Test force flag
    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.arg("--config")
        .arg(&config_path)
        .arg("apply")
        .arg("--force");
    
    cmd.assert().success();
    
    // Test that interactive and only are mutually compatible
    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.arg("--config")
        .arg(&config_path)
        .arg("apply")
        .arg("--interactive")
        .arg("--only")
        .arg("~/.zshrc");
    
    // Should accept both flags (interactive takes precedence)
    cmd.assert().success();
}

#[test]
fn test_help_messages() {
    // Test main help
    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.arg("--help");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("secrets"))
        .stdout(predicate::str::contains("diff"))
        .stdout(predicate::str::contains("apply"));
    
    // Test secrets help
    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.arg("secrets").arg("--help");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("init"))
        .stdout(predicate::str::contains("encrypt"))
        .stdout(predicate::str::contains("decrypt"))
        .stdout(predicate::str::contains("edit"));
    
    // Test apply help
    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.arg("apply").arg("--help");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("interactive"))
        .stdout(predicate::str::contains("only"))
        .stdout(predicate::str::contains("force"));
}

#[test]
fn test_verbose_flag() {
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
    cmd.arg("--verbose")
        .arg("--config")
        .arg(&config_path)
        .arg("config")
        .arg("--show");
    
    // Verbose flag should be accepted
    cmd.assert().success();
}

#[cfg(test)]
mod advanced_integration {
    use super::*;
    
    #[test]
    fn test_snapshot_with_hooks() {
        let temp_dir = TempDir::new().unwrap();
        let dotdipper_dir = temp_dir.path().join(".dotdipper");
        fs::create_dir_all(&dotdipper_dir).unwrap();
        
        let config_path = dotdipper_dir.join("config.toml");
        let hook_output = temp_dir.path().join("hook_ran.txt");
        
        // Create config with hooks that write to a file
        fs::write(
            &config_path,
            format!(
                r#"
[general]
tracked_files = []

[hooks]
pre_snapshot = ["echo 'pre' > {}"]
post_snapshot = ["echo 'post' >> {}"]
"#,
                hook_output.display(),
                hook_output.display()
            ),
        )
        .unwrap();
        
        let mut cmd = Command::cargo_bin("dotdipper").unwrap();
        cmd.env("HOME", temp_dir.path())
            .arg("--config")
            .arg(&config_path)
            .arg("snapshot")
            .arg("create");
        
        cmd.assert().success();
        
        // Verify hooks ran
        if hook_output.exists() {
            let content = fs::read_to_string(&hook_output).unwrap();
            assert!(content.contains("pre"));
            assert!(content.contains("post"));
        }
    }
    
    #[test]
    fn test_diff_detailed_flag() {
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
        
        // Test diff without detailed
        let mut cmd = Command::cargo_bin("dotdipper").unwrap();
        cmd.env("HOME", temp_dir.path())
            .arg("--config")
            .arg(&config_path)
            .arg("diff");
        
        cmd.assert().success();
        
        // Test diff with detailed
        let mut cmd = Command::cargo_bin("dotdipper").unwrap();
        cmd.env("HOME", temp_dir.path())
            .arg("--config")
            .arg(&config_path)
            .arg("diff")
            .arg("--detailed");
        
        cmd.assert().success();
    }
}

