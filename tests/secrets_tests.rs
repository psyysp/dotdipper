//! Integration tests for the secrets module

use dotdipper::secrets::SecretsProvider;

#[test]
fn test_secrets_provider_from_str_age() {
    assert_eq!(SecretsProvider::from_str("age"), Some(SecretsProvider::Age));
    assert_eq!(SecretsProvider::from_str("Age"), Some(SecretsProvider::Age));
    assert_eq!(SecretsProvider::from_str("AGE"), Some(SecretsProvider::Age));
}

#[test]
fn test_secrets_provider_from_str_sops() {
    assert_eq!(SecretsProvider::from_str("sops"), Some(SecretsProvider::Sops));
    assert_eq!(SecretsProvider::from_str("Sops"), Some(SecretsProvider::Sops));
    assert_eq!(SecretsProvider::from_str("SOPS"), Some(SecretsProvider::Sops));
}

#[test]
fn test_secrets_provider_from_str_invalid() {
    assert_eq!(SecretsProvider::from_str("invalid"), None);
    assert_eq!(SecretsProvider::from_str(""), None);
    assert_eq!(SecretsProvider::from_str("gpg"), None);
    assert_eq!(SecretsProvider::from_str("vault"), None);
}

#[test]
fn test_secrets_provider_equality() {
    assert_eq!(SecretsProvider::Age, SecretsProvider::Age);
    assert_eq!(SecretsProvider::Sops, SecretsProvider::Sops);
    assert_ne!(SecretsProvider::Age, SecretsProvider::Sops);
}

#[test]
fn test_secrets_provider_copy() {
    let provider = SecretsProvider::Age;
    let copied = provider;
    assert_eq!(provider, copied);
}

#[cfg(test)]
mod age_encryption_tests {
    use dotdipper::cfg::Config;
    use tempfile::TempDir;

    // Note: These tests require 'age' to be installed on the system
    // They will be skipped if age is not available

    fn age_available() -> bool {
        std::process::Command::new("age")
            .arg("--version")
            .output()
            .is_ok()
    }

    #[test]
    fn test_check_age_installed() {
        if !age_available() {
            println!("Skipping test: age not installed");
            return;
        }
        
        let result = dotdipper::secrets::check_age();
        assert!(result.is_ok());
    }

    #[test]
    fn test_encrypt_nonexistent_file() {
        if !age_available() {
            println!("Skipping test: age not installed");
            return;
        }
        
        let temp_dir = TempDir::new().unwrap();
        let nonexistent = temp_dir.path().join("nonexistent.txt");
        
        let config = Config::default();
        let result = dotdipper::secrets::encrypt(&config, &nonexistent, None);
        
        assert!(result.is_err());
    }

    #[test]
    fn test_decrypt_nonexistent_file() {
        if !age_available() {
            println!("Skipping test: age not installed");
            return;
        }
        
        let temp_dir = TempDir::new().unwrap();
        let nonexistent = temp_dir.path().join("nonexistent.age");
        
        let config = Config::default();
        let result = dotdipper::secrets::decrypt(&config, &nonexistent, None);
        
        assert!(result.is_err());
    }

    #[test]
    fn test_edit_nonexistent_file() {
        if !age_available() {
            println!("Skipping test: age not installed");
            return;
        }
        
        let temp_dir = TempDir::new().unwrap();
        let nonexistent = temp_dir.path().join("nonexistent.age");
        
        let config = Config::default();
        let result = dotdipper::secrets::edit(&config, &nonexistent);
        
        assert!(result.is_err());
    }
}

#[cfg(test)]
mod secrets_config_tests {
    use dotdipper::cfg::{Config, SecretsConfig};

    #[test]
    fn test_secrets_config_optional() {
        let config = Config::default();
        assert!(config.secrets.is_none());
    }

    #[test]
    fn test_secrets_config_with_values() {
        let mut config = Config::default();
        config.secrets = Some(SecretsConfig {
            provider: Some("age".to_string()),
            key_path: Some("~/.config/age/keys.txt".to_string()),
        });
        
        let secrets = config.secrets.as_ref().unwrap();
        assert_eq!(secrets.provider, Some("age".to_string()));
        assert!(secrets.key_path.is_some());
    }

    #[test]
    fn test_secrets_config_serialization() {
        let secrets = SecretsConfig {
            provider: Some("age".to_string()),
            key_path: Some("/path/to/keys.txt".to_string()),
        };
        
        let toml = toml::to_string(&secrets).unwrap();
        assert!(toml.contains("age"));
        assert!(toml.contains("/path/to/keys.txt"));
        
        let deserialized: SecretsConfig = toml::from_str(&toml).unwrap();
        assert_eq!(deserialized.provider, secrets.provider);
    }
}
