use anyhow::{Context, Result, bail};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::NamedTempFile;

use crate::cfg::Config;
use crate::ui;

/// Provider for secrets encryption
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SecretsProvider {
    Age,
    Sops,
}

impl SecretsProvider {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "age" => Some(SecretsProvider::Age),
            "sops" => Some(SecretsProvider::Sops),
            _ => None,
        }
    }
}

/// Initialize secrets management - generate or import age keys
pub fn init(config: &Config) -> Result<()> {
    let provider = config.secrets.as_ref()
        .and_then(|s| s.provider.as_deref())
        .unwrap_or("age");
    
    match SecretsProvider::from_str(provider) {
        Some(SecretsProvider::Age) => init_age(config),
        Some(SecretsProvider::Sops) => init_sops(config),
        None => bail!("Unknown secrets provider: {}", provider),
    }
}

fn init_age(config: &Config) -> Result<()> {
    let key_path = config.secrets.as_ref()
        .and_then(|s| s.key_path.as_ref())
        .map(|p| PathBuf::from(shellexpand::tilde(p).to_string()))
        .unwrap_or_else(|| {
            dirs::home_dir()
                .expect("Could not find home directory")
                .join(".config/age/keys.txt")
        });
    
    if key_path.exists() {
        ui::info(&format!("Age key already exists at {}", key_path.display()));
        
        // Verify it's valid
        let content = fs::read_to_string(&key_path)?;
        if !content.contains("AGE-SECRET-KEY-") {
            bail!("Invalid age key file at {}", key_path.display());
        }
        
        ui::success("Age key is valid");
        return Ok(());
    }
    
    ui::info("Generating new age key...");
    
    // Create parent directory
    if let Some(parent) = key_path.parent() {
        fs::create_dir_all(parent)?;
    }
    
    // Generate key using age-keygen
    let output = Command::new("age-keygen")
        .arg("-o")
        .arg(&key_path)
        .output()
        .context("Failed to run age-keygen. Is age installed?")?;
    
    if !output.status.success() {
        bail!("Failed to generate age key: {}", String::from_utf8_lossy(&output.stderr));
    }
    
    // Set restrictive permissions (0600)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let permissions = std::fs::Permissions::from_mode(0o600);
        fs::set_permissions(&key_path, permissions)?;
    }
    
    ui::success(&format!("Age key generated at {}", key_path.display()));
    ui::hint("Back up this key file securely - you'll need it to decrypt your secrets");
    
    // Extract and display public key
    let key_content = fs::read_to_string(&key_path)?;
    if let Some(public_key_line) = key_content.lines().find(|l| l.starts_with("# public key: ")) {
        ui::info(&format!("Public key: {}", public_key_line.trim_start_matches("# public key: ")));
    }
    
    Ok(())
}

fn init_sops(_config: &Config) -> Result<()> {
    ui::warn("SOPS support is not yet implemented");
    ui::hint("Use 'age' provider for now");
    bail!("SOPS provider not implemented");
}

/// Encrypt a file using the configured provider
pub fn encrypt(config: &Config, input_path: &Path, output_path: Option<&Path>) -> Result<PathBuf> {
    let provider = config.secrets.as_ref()
        .and_then(|s| s.provider.as_deref())
        .unwrap_or("age");
    
    match SecretsProvider::from_str(provider) {
        Some(SecretsProvider::Age) => encrypt_age(config, input_path, output_path),
        Some(SecretsProvider::Sops) => encrypt_sops(config, input_path, output_path),
        None => bail!("Unknown secrets provider: {}", provider),
    }
}

fn encrypt_age(config: &Config, input_path: &Path, output_path: Option<&Path>) -> Result<PathBuf> {
    if !input_path.exists() {
        bail!("Input file does not exist: {}", input_path.display());
    }
    
    let key_path = config.secrets.as_ref()
        .and_then(|s| s.key_path.as_ref())
        .map(|p| PathBuf::from(shellexpand::tilde(p).to_string()))
        .unwrap_or_else(|| {
            dirs::home_dir()
                .expect("Could not find home directory")
                .join(".config/age/keys.txt")
        });
    
    if !key_path.exists() {
        bail!("Age key not found at {}. Run 'dotdipper secrets init' first", key_path.display());
    }
    
    // Read public key from key file
    let key_content = fs::read_to_string(&key_path)
        .context("Failed to read age key file")?;
    
    let public_key = key_content
        .lines()
        .find(|l| l.starts_with("# public key: "))
        .and_then(|l| l.strip_prefix("# public key: "))
        .context("Could not find public key in age key file")?
        .trim();
    
    // Determine output path
    let out_path = output_path
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| {
            let mut path = input_path.to_path_buf();
            let new_name = format!("{}.age", input_path.file_name().unwrap().to_string_lossy());
            path.set_file_name(new_name);
            path
        });
    
    ui::info(&format!("Encrypting {} → {}", input_path.display(), out_path.display()));
    
    // Encrypt using age
    let output = Command::new("age")
        .arg("--encrypt")
        .arg("--recipient")
        .arg(public_key)
        .arg("--output")
        .arg(&out_path)
        .arg(input_path)
        .output()
        .context("Failed to run age. Is age installed?")?;
    
    if !output.status.success() {
        bail!("Failed to encrypt file: {}", String::from_utf8_lossy(&output.stderr));
    }
    
    ui::success(&format!("Encrypted to {}", out_path.display()));
    Ok(out_path)
}

fn encrypt_sops(_config: &Config, _input_path: &Path, _output_path: Option<&Path>) -> Result<PathBuf> {
    bail!("SOPS provider not implemented");
}

/// Decrypt a file using the configured provider
pub fn decrypt(config: &Config, input_path: &Path, output_path: Option<&Path>) -> Result<PathBuf> {
    let provider = config.secrets.as_ref()
        .and_then(|s| s.provider.as_deref())
        .unwrap_or("age");
    
    match SecretsProvider::from_str(provider) {
        Some(SecretsProvider::Age) => decrypt_age(config, input_path, output_path),
        Some(SecretsProvider::Sops) => decrypt_sops(config, input_path, output_path),
        None => bail!("Unknown secrets provider: {}", provider),
    }
}

fn decrypt_age(config: &Config, input_path: &Path, output_path: Option<&Path>) -> Result<PathBuf> {
    if !input_path.exists() {
        bail!("Input file does not exist: {}", input_path.display());
    }
    
    let key_path = config.secrets.as_ref()
        .and_then(|s| s.key_path.as_ref())
        .map(|p| PathBuf::from(shellexpand::tilde(p).to_string()))
        .unwrap_or_else(|| {
            dirs::home_dir()
                .expect("Could not find home directory")
                .join(".config/age/keys.txt")
        });
    
    if !key_path.exists() {
        bail!("Age key not found at {}. Run 'dotdipper secrets init' first", key_path.display());
    }
    
    // Determine output path
    let out_path = if let Some(p) = output_path {
        p.to_path_buf()
    } else {
        // Remove .age suffix if present
        let name = input_path.file_name().unwrap().to_string_lossy();
        if let Some(stripped) = name.strip_suffix(".age") {
            let mut path = input_path.to_path_buf();
            path.set_file_name(stripped);
            path
        } else {
            let mut path = input_path.to_path_buf();
            path.set_file_name(format!("{}.decrypted", name));
            path
        }
    };
    
    ui::info(&format!("Decrypting {} → {}", input_path.display(), out_path.display()));
    
    // Decrypt using age
    let output = Command::new("age")
        .arg("--decrypt")
        .arg("--identity")
        .arg(&key_path)
        .arg("--output")
        .arg(&out_path)
        .arg(input_path)
        .output()
        .context("Failed to run age. Is age installed?")?;
    
    if !output.status.success() {
        bail!("Failed to decrypt file: {}", String::from_utf8_lossy(&output.stderr));
    }
    
    ui::success(&format!("Decrypted to {}", out_path.display()));
    Ok(out_path)
}

fn decrypt_sops(_config: &Config, _input_path: &Path, _output_path: Option<&Path>) -> Result<PathBuf> {
    bail!("SOPS provider not implemented");
}

/// Edit an encrypted file (decrypt to temp, open in editor, re-encrypt)
pub fn edit(config: &Config, encrypted_path: &Path) -> Result<()> {
    if !encrypted_path.exists() {
        bail!("Encrypted file does not exist: {}", encrypted_path.display());
    }
    
    let provider = config.secrets.as_ref()
        .and_then(|s| s.provider.as_deref())
        .unwrap_or("age");
    
    match SecretsProvider::from_str(provider) {
        Some(SecretsProvider::Age) => edit_age(config, encrypted_path),
        Some(SecretsProvider::Sops) => edit_sops(config, encrypted_path),
        None => bail!("Unknown secrets provider: {}", provider),
    }
}

fn edit_age(config: &Config, encrypted_path: &Path) -> Result<()> {
    ui::info(&format!("Editing {}", encrypted_path.display()));
    
    // Create temporary file
    let temp_file = NamedTempFile::new()?;
    let temp_path = temp_file.path().to_path_buf();
    
    // Decrypt to temp file
    decrypt_age(config, encrypted_path, Some(&temp_path))?;
    
    // Get original hash for comparison
    let original_hash = crate::hash::hash_file(&temp_path)?;
    
    // Open in editor
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
    
    ui::info(&format!("Opening in {}...", editor));
    
    let status = Command::new(&editor)
        .arg(&temp_path)
        .status()
        .context("Failed to open editor")?;
    
    if !status.success() {
        bail!("Editor exited with error");
    }
    
    // Check if file was modified
    let new_hash = crate::hash::hash_file(&temp_path)?;
    
    if original_hash.hash == new_hash.hash {
        ui::info("No changes made");
        return Ok(());
    }
    
    // Re-encrypt
    ui::info("Saving changes...");
    encrypt_age(config, &temp_path, Some(encrypted_path))?;
    
    ui::success("Changes saved successfully");
    
    Ok(())
}

fn edit_sops(_config: &Config, _encrypted_path: &Path) -> Result<()> {
    bail!("SOPS provider not implemented");
}

/// Decrypt file in-memory and return contents (for apply operation)
pub fn decrypt_to_memory(config: &Config, encrypted_path: &Path) -> Result<Vec<u8>> {
    let provider = config.secrets.as_ref()
        .and_then(|s| s.provider.as_deref())
        .unwrap_or("age");
    
    match SecretsProvider::from_str(provider) {
        Some(SecretsProvider::Age) => decrypt_age_to_memory(config, encrypted_path),
        Some(SecretsProvider::Sops) => decrypt_sops_to_memory(config, encrypted_path),
        None => bail!("Unknown secrets provider: {}", provider),
    }
}

fn decrypt_age_to_memory(config: &Config, encrypted_path: &Path) -> Result<Vec<u8>> {
    let key_path = config.secrets.as_ref()
        .and_then(|s| s.key_path.as_ref())
        .map(|p| PathBuf::from(shellexpand::tilde(p).to_string()))
        .unwrap_or_else(|| {
            dirs::home_dir()
                .expect("Could not find home directory")
                .join(".config/age/keys.txt")
        });
    
    if !key_path.exists() {
        bail!("Age key not found at {}", key_path.display());
    }
    
    // Decrypt using age to stdout
    let output = Command::new("age")
        .arg("--decrypt")
        .arg("--identity")
        .arg(&key_path)
        .arg(encrypted_path)
        .output()
        .context("Failed to run age")?;
    
    if !output.status.success() {
        bail!("Failed to decrypt file: {}", String::from_utf8_lossy(&output.stderr));
    }
    
    Ok(output.stdout)
}

fn decrypt_sops_to_memory(_config: &Config, _encrypted_path: &Path) -> Result<Vec<u8>> {
    bail!("SOPS provider not implemented");
}

/// Check if age is installed
pub fn check_age() -> Result<()> {
    which::which("age")
        .context("age not found in PATH")?;
    which::which("age-keygen")
        .context("age-keygen not found in PATH")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_provider_from_str() {
        assert_eq!(SecretsProvider::from_str("age"), Some(SecretsProvider::Age));
        assert_eq!(SecretsProvider::from_str("Age"), Some(SecretsProvider::Age));
        assert_eq!(SecretsProvider::from_str("AGE"), Some(SecretsProvider::Age));
        assert_eq!(SecretsProvider::from_str("sops"), Some(SecretsProvider::Sops));
        assert_eq!(SecretsProvider::from_str("invalid"), None);
    }
}

