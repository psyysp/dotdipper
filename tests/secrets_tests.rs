//! Integration tests for the secrets module

use dotdipper::secrets::SecretsProvider;

#[test]
fn test_secrets_provider_from_str_age() {
    assert_eq!(SecretsProvider::parse("age"), Some(SecretsProvider::Age));
    assert_eq!(SecretsProvider::parse("Age"), Some(SecretsProvider::Age));
    assert_eq!(SecretsProvider::parse("AGE"), Some(SecretsProvider::Age));
}

#[test]
fn test_secrets_provider_from_str_sops() {
    assert_eq!(SecretsProvider::parse("sops"), Some(SecretsProvider::Sops));
    assert_eq!(SecretsProvider::parse("Sops"), Some(SecretsProvider::Sops));
    assert_eq!(SecretsProvider::parse("SOPS"), Some(SecretsProvider::Sops));
}

#[test]
fn test_secrets_provider_from_str_invalid() {
    assert_eq!(SecretsProvider::parse("invalid"), None);
    assert_eq!(SecretsProvider::parse(""), None);
    assert_eq!(SecretsProvider::parse("gpg"), None);
    assert_eq!(SecretsProvider::parse("vault"), None);
}

#[test]
fn test_secrets_provider_equality() {
    assert_eq!(SecretsProvider::Age, SecretsProvider::Age);
    assert_eq!(SecretsProvider::Sops, SecretsProvider::Sops);
    assert_ne!(SecretsProvider::Age, SecretsProvider::Sops);
}

#[test]
fn test_encrypted_path_helpers() {
    use std::path::Path;

    assert!(dotdipper::secrets::is_encrypted_secret_path(Path::new(
        "creds.age"
    )));
    assert!(dotdipper::secrets::is_encrypted_secret_path(Path::new(
        "secrets.sops.yaml"
    )));
    assert!(!dotdipper::secrets::is_encrypted_secret_path(Path::new(
        ".zshrc"
    )));

    assert_eq!(
        dotdipper::secrets::plain_path_from_encrypted(Path::new("/tmp/a.sops.yaml"))
            .file_name()
            .unwrap(),
        "a.yaml"
    );
}

#[cfg(test)]
mod age_encryption_tests {
    use dotdipper::cfg::{Config, SecretsConfig};
    use serial_test::serial;
    use std::fs;
    use tempfile::TempDir;

    // Note: These tests require 'age' to be installed on the system
    // They will be skipped if age is not available

    fn age_available() -> bool {
        std::process::Command::new("age")
            .arg("--version")
            .output()
            .is_ok()
    }

    fn sops_available() -> bool {
        std::process::Command::new("sops")
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

    #[test]
    #[serial]
    fn age_encrypt_decrypt_roundtrip() {
        if !age_available() {
            println!("Skipping test: age not installed");
            return;
        }

        let temp = TempDir::new().unwrap();
        let key_path = temp.path().join("keys.txt");
        let plain = temp.path().join("secret.txt");
        let enc = temp.path().join("secret.txt.age");
        let out = temp.path().join("secret.out");

        let gen = std::process::Command::new("age-keygen")
            .arg("-o")
            .arg(&key_path)
            .output()
            .unwrap();
        assert!(gen.status.success());

        fs::write(&plain, b"hello-secret\n").unwrap();

        let config = Config {
            secrets: Some(SecretsConfig {
                provider: Some("age".to_string()),
                key_path: Some(key_path.to_string_lossy().to_string()),
            }),
            ..Config::default()
        };

        let encrypted = dotdipper::secrets::encrypt(&config, &plain, Some(&enc)).unwrap();
        assert!(encrypted.exists());
        assert!(!fs::read(&encrypted)
            .unwrap()
            .windows(5)
            .any(|w| w == b"hello"));

        let decrypted = dotdipper::secrets::decrypt(&config, &encrypted, Some(&out)).unwrap();
        assert_eq!(fs::read_to_string(decrypted).unwrap(), "hello-secret\n");

        let mem = dotdipper::secrets::decrypt_to_memory(&config, &encrypted).unwrap();
        assert_eq!(mem, b"hello-secret\n");
    }

    #[test]
    #[serial]
    fn sops_encrypt_decrypt_roundtrip() {
        if !age_available() || !sops_available() {
            println!("Skipping test: age/sops not installed");
            return;
        }

        let result = dotdipper::secrets::check_sops();
        assert!(result.is_ok());

        let temp = TempDir::new().unwrap();
        let key_path = temp.path().join("keys.txt");
        let plain = temp.path().join("secrets.yaml");
        let enc = temp.path().join("secrets.sops.yaml");
        let out = temp.path().join("secrets.out.yaml");

        let gen = std::process::Command::new("age-keygen")
            .arg("-o")
            .arg(&key_path)
            .output()
            .unwrap();
        assert!(gen.status.success());

        fs::write(&plain, "password: hunter2\n").unwrap();

        let config = Config {
            secrets: Some(SecretsConfig {
                provider: Some("sops".to_string()),
                key_path: Some(key_path.to_string_lossy().to_string()),
            }),
            ..Config::default()
        };

        let encrypted = dotdipper::secrets::encrypt(&config, &plain, Some(&enc)).unwrap();
        assert!(encrypted.exists());
        let enc_text = fs::read_to_string(&encrypted).unwrap();
        assert!(
            enc_text.contains("sops") || enc_text.contains("ENC["),
            "expected sops ciphertext, got: {enc_text}"
        );

        let decrypted = dotdipper::secrets::decrypt(&config, &encrypted, Some(&out)).unwrap();
        assert!(fs::read_to_string(decrypted).unwrap().contains("hunter2"));

        let mem =
            String::from_utf8(dotdipper::secrets::decrypt_to_memory(&config, &encrypted).unwrap())
                .unwrap();
        assert!(mem.contains("hunter2"));
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
        let config = Config {
            secrets: Some(SecretsConfig {
                provider: Some("age".to_string()),
                key_path: Some("~/.config/age/keys.txt".to_string()),
            }),
            ..Config::default()
        };

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
