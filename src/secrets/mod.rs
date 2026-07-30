use anyhow::{bail, Context, Result};
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
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "age" => Some(SecretsProvider::Age),
            "sops" => Some(SecretsProvider::Sops),
            _ => None,
        }
    }
}

fn provider_from_config(config: &Config) -> Result<SecretsProvider> {
    let provider = config
        .secrets
        .as_ref()
        .and_then(|s| s.provider.as_deref())
        .unwrap_or("age");
    SecretsProvider::parse(provider)
        .ok_or_else(|| anyhow::anyhow!("Unknown secrets provider: {}", provider))
}

fn age_key_path(config: &Config) -> PathBuf {
    config
        .secrets
        .as_ref()
        .and_then(|s| s.key_path.as_ref())
        .map(|p| PathBuf::from(shellexpand::tilde(p).to_string()))
        .unwrap_or_else(|| {
            dirs::home_dir()
                .expect("Could not find home directory")
                .join(".config/age/keys.txt")
        })
}

fn age_public_key(key_path: &Path) -> Result<String> {
    let key_content = fs::read_to_string(key_path).context("Failed to read age key file")?;
    key_content
        .lines()
        .find(|l| l.starts_with("# public key: "))
        .and_then(|l| l.strip_prefix("# public key: "))
        .map(|s| s.trim().to_string())
        .context("Could not find public key in age key file")
}

/// Initialize secrets management - generate or import age keys
pub fn init(config: &Config) -> Result<()> {
    match provider_from_config(config)? {
        SecretsProvider::Age => init_age(config),
        SecretsProvider::Sops => init_sops(config),
    }
}

fn init_age(config: &Config) -> Result<()> {
    let key_path = age_key_path(config);

    if key_path.exists() {
        ui::info(&format!("Age key already exists at {}", key_path.display()));

        let content = fs::read_to_string(&key_path)?;
        if !content.contains("AGE-SECRET-KEY-") {
            bail!("Invalid age key file at {}", key_path.display());
        }

        ui::success("Age key is valid");
        return Ok(());
    }

    ui::info("Generating new age key...");

    if let Some(parent) = key_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let output = Command::new("age-keygen")
        .arg("-o")
        .arg(&key_path)
        .output()
        .context("Failed to run age-keygen. Is age installed?")?;

    if !output.status.success() {
        bail!(
            "Failed to generate age key: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let permissions = std::fs::Permissions::from_mode(0o600);
        fs::set_permissions(&key_path, permissions)?;
    }

    ui::success(&format!("Age key generated at {}", key_path.display()));
    ui::hint("Back up this key file securely - you'll need it to decrypt your secrets");

    let key_content = fs::read_to_string(&key_path)?;
    if let Some(public_key_line) = key_content
        .lines()
        .find(|l| l.starts_with("# public key: "))
    {
        ui::info(&format!(
            "Public key: {}",
            public_key_line.trim_start_matches("# public key: ")
        ));
    }

    Ok(())
}

fn init_sops(config: &Config) -> Result<()> {
    check_sops()?;

    // SOPS age backend reuses the same age identity file as the age provider.
    init_age(config)?;

    let key_path = age_key_path(config);
    let public_key = age_public_key(&key_path)?;

    ui::success("SOPS is ready (age backend)");
    ui::hint(&format!(
        "Export for shell use:\n  export SOPS_AGE_KEY_FILE={}\n  export SOPS_AGE_RECIPIENTS={}",
        key_path.display(),
        public_key
    ));
    ui::hint(
        "Optional: add a .sops.yaml in your repo to pin creation rules. \
         Dotdipper passes --age <recipient> for encrypt when no creation rules apply.",
    );
    ui::hint("Set [secrets] provider = \"sops\" in config.toml if not already set.");

    Ok(())
}

/// Encrypt a file using the configured provider
pub fn encrypt(config: &Config, input_path: &Path, output_path: Option<&Path>) -> Result<PathBuf> {
    match provider_from_config(config)? {
        SecretsProvider::Age => encrypt_age(config, input_path, output_path),
        SecretsProvider::Sops => encrypt_sops(config, input_path, output_path),
    }
}

fn encrypt_age(config: &Config, input_path: &Path, output_path: Option<&Path>) -> Result<PathBuf> {
    if !input_path.exists() {
        bail!("Input file does not exist: {}", input_path.display());
    }

    let key_path = age_key_path(config);
    if !key_path.exists() {
        bail!(
            "Age key not found at {}. Run 'dotdipper secrets init' first",
            key_path.display()
        );
    }

    let public_key = age_public_key(&key_path)?;

    let out_path = output_path.map(|p| p.to_path_buf()).unwrap_or_else(|| {
        let mut path = input_path.to_path_buf();
        let new_name = format!("{}.age", input_path.file_name().unwrap().to_string_lossy());
        path.set_file_name(new_name);
        path
    });

    ui::info(&format!(
        "Encrypting {} → {}",
        input_path.display(),
        out_path.display()
    ));

    let output = Command::new("age")
        .arg("--encrypt")
        .arg("--recipient")
        .arg(&public_key)
        .arg("--output")
        .arg(&out_path)
        .arg(input_path)
        .output()
        .context("Failed to run age. Is age installed?")?;

    if !output.status.success() {
        bail!(
            "Failed to encrypt file: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    ui::success(&format!("Encrypted to {}", out_path.display()));
    Ok(out_path)
}

fn encrypt_sops(config: &Config, input_path: &Path, output_path: Option<&Path>) -> Result<PathBuf> {
    check_sops()?;
    if !input_path.exists() {
        bail!("Input file does not exist: {}", input_path.display());
    }

    let key_path = age_key_path(config);
    if !key_path.exists() {
        bail!(
            "Age key not found at {}. Run 'dotdipper secrets init' first (SOPS uses age keys)",
            key_path.display()
        );
    }
    let public_key = age_public_key(&key_path)?;

    let out_path = output_path
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| default_sops_output_path(input_path));

    ui::info(&format!(
        "Encrypting with SOPS {} → {}",
        input_path.display(),
        out_path.display()
    ));

    let mut cmd = Command::new("sops");
    cmd.arg("--encrypt")
        .arg("--age")
        .arg(&public_key)
        .arg("--output")
        .arg(&out_path)
        .arg(input_path);
    cmd.env("SOPS_AGE_KEY_FILE", &key_path);

    let output = cmd
        .output()
        .context("Failed to run sops. Is sops installed?")?;

    if !output.status.success() {
        bail!(
            "Failed to encrypt with sops: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    ui::success(&format!("Encrypted to {}", out_path.display()));
    Ok(out_path)
}

fn default_sops_output_path(input: &Path) -> PathBuf {
    let name = input.file_name().unwrap().to_string_lossy();
    // secrets.yaml → secrets.sops.yaml ; plain file → file.sops
    let new_name = if let Some(dot) = name.rfind('.') {
        let (stem, ext) = name.split_at(dot);
        format!("{}.sops{}", stem, ext)
    } else {
        format!("{}.sops", name)
    };
    let mut path = input.to_path_buf();
    path.set_file_name(new_name);
    path
}

/// Decrypt a file using the configured provider
pub fn decrypt(config: &Config, input_path: &Path, output_path: Option<&Path>) -> Result<PathBuf> {
    match provider_from_config(config)? {
        SecretsProvider::Age => decrypt_age(config, input_path, output_path),
        SecretsProvider::Sops => decrypt_sops(config, input_path, output_path),
    }
}

fn decrypt_age(config: &Config, input_path: &Path, output_path: Option<&Path>) -> Result<PathBuf> {
    if !input_path.exists() {
        bail!("Input file does not exist: {}", input_path.display());
    }

    let key_path = age_key_path(config);
    if !key_path.exists() {
        bail!(
            "Age key not found at {}. Run 'dotdipper secrets init' first",
            key_path.display()
        );
    }

    let out_path = if let Some(p) = output_path {
        p.to_path_buf()
    } else {
        plain_path_from_encrypted(input_path)
    };

    ui::info(&format!(
        "Decrypting {} → {}",
        input_path.display(),
        out_path.display()
    ));

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
        bail!(
            "Failed to decrypt file: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    ui::success(&format!("Decrypted to {}", out_path.display()));
    Ok(out_path)
}

fn decrypt_sops(config: &Config, input_path: &Path, output_path: Option<&Path>) -> Result<PathBuf> {
    check_sops()?;
    if !input_path.exists() {
        bail!("Input file does not exist: {}", input_path.display());
    }

    let key_path = age_key_path(config);
    if !key_path.exists() {
        bail!(
            "Age key not found at {}. Run 'dotdipper secrets init' first",
            key_path.display()
        );
    }

    let out_path = if let Some(p) = output_path {
        p.to_path_buf()
    } else {
        plain_path_from_encrypted(input_path)
    };

    ui::info(&format!(
        "Decrypting with SOPS {} → {}",
        input_path.display(),
        out_path.display()
    ));

    let output = Command::new("sops")
        .arg("--decrypt")
        .arg("--output")
        .arg(&out_path)
        .arg(input_path)
        .env("SOPS_AGE_KEY_FILE", &key_path)
        .output()
        .context("Failed to run sops. Is sops installed?")?;

    if !output.status.success() {
        bail!(
            "Failed to decrypt with sops: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    ui::success(&format!("Decrypted to {}", out_path.display()));
    Ok(out_path)
}

/// Edit an encrypted file (decrypt to temp, open in editor, re-encrypt)
pub fn edit(config: &Config, encrypted_path: &Path) -> Result<()> {
    if !encrypted_path.exists() {
        bail!(
            "Encrypted file does not exist: {}",
            encrypted_path.display()
        );
    }

    match provider_from_config(config)? {
        SecretsProvider::Age => edit_age(config, encrypted_path),
        SecretsProvider::Sops => edit_sops(config, encrypted_path),
    }
}

fn edit_age(config: &Config, encrypted_path: &Path) -> Result<()> {
    ui::info(&format!("Editing {}", encrypted_path.display()));

    let temp_file = NamedTempFile::new()?;
    let temp_path = temp_file.path().to_path_buf();

    decrypt_age(config, encrypted_path, Some(&temp_path))?;

    let original_hash = crate::hash::hash_file(&temp_path)?;

    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());

    ui::info(&format!("Opening in {}...", editor));

    let status = Command::new(&editor)
        .arg(&temp_path)
        .status()
        .context("Failed to open editor")?;

    if !status.success() {
        bail!("Editor exited with error");
    }

    let new_hash = crate::hash::hash_file(&temp_path)?;

    if original_hash.hash == new_hash.hash {
        ui::info("No changes made");
        return Ok(());
    }

    ui::info("Saving changes...");
    encrypt_age(config, &temp_path, Some(encrypted_path))?;

    ui::success("Changes saved successfully");

    Ok(())
}

fn edit_sops(config: &Config, encrypted_path: &Path) -> Result<()> {
    check_sops()?;
    let key_path = age_key_path(config);
    if !key_path.exists() {
        bail!(
            "Age key not found at {}. Run 'dotdipper secrets init' first",
            key_path.display()
        );
    }

    ui::info(&format!("Editing with SOPS: {}", encrypted_path.display()));

    // Native sops edit handles decrypt → $EDITOR → re-encrypt
    let status = Command::new("sops")
        .arg(encrypted_path)
        .env("SOPS_AGE_KEY_FILE", &key_path)
        .status()
        .context("Failed to run sops. Is sops installed?")?;

    if !status.success() {
        bail!("sops editor exited with an error");
    }

    ui::success("SOPS edit finished");
    Ok(())
}

/// Decrypt file in-memory and return contents (for apply operation)
pub fn decrypt_to_memory(config: &Config, encrypted_path: &Path) -> Result<Vec<u8>> {
    // Prefer provider from config, but fall back by filename when needed
    let provider = provider_from_config(config).unwrap_or(SecretsProvider::Age);
    let provider = if looks_like_sops_file(encrypted_path) {
        SecretsProvider::Sops
    } else if looks_like_age_file(encrypted_path) {
        SecretsProvider::Age
    } else {
        provider
    };

    match provider {
        SecretsProvider::Age => decrypt_age_to_memory(config, encrypted_path),
        SecretsProvider::Sops => decrypt_sops_to_memory(config, encrypted_path),
    }
}

fn decrypt_age_to_memory(config: &Config, encrypted_path: &Path) -> Result<Vec<u8>> {
    let key_path = age_key_path(config);
    if !key_path.exists() {
        bail!("Age key not found at {}", key_path.display());
    }

    let output = Command::new("age")
        .arg("--decrypt")
        .arg("--identity")
        .arg(&key_path)
        .arg(encrypted_path)
        .output()
        .context("Failed to run age")?;

    if !output.status.success() {
        bail!(
            "Failed to decrypt file: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok(output.stdout)
}

fn decrypt_sops_to_memory(config: &Config, encrypted_path: &Path) -> Result<Vec<u8>> {
    check_sops()?;
    let key_path = age_key_path(config);
    if !key_path.exists() {
        bail!("Age key not found at {}", key_path.display());
    }

    let output = Command::new("sops")
        .arg("--decrypt")
        .arg(encrypted_path)
        .env("SOPS_AGE_KEY_FILE", &key_path)
        .output()
        .context("Failed to run sops")?;

    if !output.status.success() {
        bail!(
            "Failed to decrypt with sops: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok(output.stdout)
}

/// Check if age is installed
pub fn check_age() -> Result<()> {
    which::which("age").context("age not found in PATH")?;
    which::which("age-keygen").context("age-keygen not found in PATH")?;
    Ok(())
}

/// Check if sops is installed
pub fn check_sops() -> Result<()> {
    which::which("sops").context(
        "sops not found in PATH. Install: https://github.com/getsops/sops#install \
         (e.g. brew install sops / apt install sops)",
    )?;
    Ok(())
}

/// True when a compiled path looks like an encrypted secret for apply.
pub fn is_encrypted_secret_path(path: &Path) -> bool {
    looks_like_age_file(path) || looks_like_sops_file(path)
}

pub fn looks_like_age_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("age"))
        .unwrap_or(false)
}

pub fn looks_like_sops_file(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    name.ends_with(".sops")
        || name.contains(".sops.")
        || name.ends_with(".enc.yaml")
        || name.ends_with(".enc.yml")
        || name.ends_with(".enc.json")
        || name.ends_with(".enc.env")
}

/// Map encrypted filename back to the plaintext home target name.
pub fn plain_path_from_encrypted(encrypted: &Path) -> PathBuf {
    let name = encrypted.file_name().unwrap().to_string_lossy();
    let plain = if let Some(stripped) = name.strip_suffix(".age") {
        stripped.to_string()
    } else if let Some(stripped) = name.strip_suffix(".sops") {
        stripped.to_string()
    } else if name.contains(".sops.") {
        name.replacen(".sops.", ".", 1)
    } else if let Some(stripped) = name.strip_suffix(".enc.yaml") {
        format!("{}.yaml", stripped)
    } else if let Some(stripped) = name.strip_suffix(".enc.yml") {
        format!("{}.yml", stripped)
    } else if let Some(stripped) = name.strip_suffix(".enc.json") {
        format!("{}.json", stripped)
    } else if let Some(stripped) = name.strip_suffix(".enc.env") {
        format!("{}.env", stripped)
    } else {
        format!("{}.decrypted", name)
    };

    let mut path = encrypted.to_path_buf();
    path.set_file_name(plain);
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_from_str() {
        assert_eq!(SecretsProvider::parse("age"), Some(SecretsProvider::Age));
        assert_eq!(SecretsProvider::parse("Age"), Some(SecretsProvider::Age));
        assert_eq!(SecretsProvider::parse("AGE"), Some(SecretsProvider::Age));
        assert_eq!(SecretsProvider::parse("sops"), Some(SecretsProvider::Sops));
        assert_eq!(SecretsProvider::parse("invalid"), None);
    }

    #[test]
    fn plain_path_strips_age_and_sops_suffixes() {
        assert_eq!(
            plain_path_from_encrypted(Path::new("/tmp/creds.age"))
                .file_name()
                .unwrap(),
            "creds"
        );
        assert_eq!(
            plain_path_from_encrypted(Path::new("/tmp/secrets.sops.yaml"))
                .file_name()
                .unwrap(),
            "secrets.yaml"
        );
        assert_eq!(
            plain_path_from_encrypted(Path::new("/tmp/app.enc.json"))
                .file_name()
                .unwrap(),
            "app.json"
        );
    }

    #[test]
    fn detects_encrypted_filenames() {
        assert!(looks_like_age_file(Path::new("x.age")));
        assert!(looks_like_sops_file(Path::new("x.sops.yaml")));
        assert!(looks_like_sops_file(Path::new("x.enc.yaml")));
        assert!(!looks_like_sops_file(Path::new("plain.yaml")));
    }
}
