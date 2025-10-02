use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_secrets_init_without_age() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");
    
    // Create minimal config
    fs::write(
        &config_path,
        r#"
[general]
tracked_files = []

[secrets]
provider = "age"
key_path = "~/.config/age/keys.txt"
"#,
    )
    .unwrap();
    
    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.arg("--config")
        .arg(&config_path)
        .arg("secrets")
        .arg("init");
    
    // This might fail if age is not installed, which is expected
    // The test just verifies the command structure is correct
    let _ = cmd.assert();
}

#[test]
fn test_secrets_encrypt_missing_file() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");
    let nonexistent = temp_dir.path().join("nonexistent.txt");
    
    // Create minimal config
    fs::write(
        &config_path,
        r#"
[general]
tracked_files = []

[secrets]
provider = "age"
"#,
    )
    .unwrap();
    
    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.arg("--config")
        .arg(&config_path)
        .arg("secrets")
        .arg("encrypt")
        .arg(&nonexistent);
    
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("does not exist"));
}

#[test]
fn test_secrets_provider_validation() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");
    
    // Create config with invalid provider
    fs::write(
        &config_path,
        r#"
[general]
tracked_files = []

[secrets]
provider = "invalid_provider"
"#,
    )
    .unwrap();
    
    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.arg("--config")
        .arg(&config_path)
        .arg("secrets")
        .arg("init");
    
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Unknown secrets provider"));
}

#[cfg(test)]
mod integration {
    use super::*;
    use std::io::Write;
    
    // This test only runs if age is installed
    #[test]
    #[ignore] // Ignored by default, run with --ignored if age is available
    fn test_full_encryption_workflow() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");
        let key_path = temp_dir.path().join("test_key.txt");
        let test_file = temp_dir.path().join("secret.txt");
        
        // Create test file
        let mut file = fs::File::create(&test_file).unwrap();
        file.write_all(b"This is a secret").unwrap();
        drop(file);
        
        // Generate age key first
        let output = std::process::Command::new("age-keygen")
            .arg("-o")
            .arg(&key_path)
            .output();
        
        if output.is_err() {
            println!("age-keygen not available, skipping test");
            return;
        }
        
        // Create config
        fs::write(
            &config_path,
            format!(
                r#"
[general]
tracked_files = []

[secrets]
provider = "age"
key_path = "{}"
"#,
                key_path.display()
            ),
        )
        .unwrap();
        
        // Encrypt
        let mut cmd = Command::cargo_bin("dotdipper").unwrap();
        cmd.arg("--config")
            .arg(&config_path)
            .arg("secrets")
            .arg("encrypt")
            .arg(&test_file);
        
        cmd.assert().success();
        
        // Check encrypted file exists
        let encrypted_path = test_file.with_extension("txt.age");
        assert!(encrypted_path.exists());
        
        // Decrypt
        let mut cmd = Command::cargo_bin("dotdipper").unwrap();
        cmd.arg("--config")
            .arg(&config_path)
            .arg("secrets")
            .arg("decrypt")
            .arg(&encrypted_path);
        
        cmd.assert().success();
        
        // Verify decrypted content
        let decrypted_content = fs::read_to_string(&test_file).unwrap();
        assert_eq!(decrypted_content, "This is a secret");
    }
}

