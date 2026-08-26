//! Integration tests for the configuration module

use dotdipper::cfg::{self, Config, GeneralConfig, PackagesConfig, RestoreMode};
use std::fs;
use tempfile::TempDir;

#[test]
fn test_config_default() {
    let config = Config::default();
    assert_eq!(config.general.default_mode, RestoreMode::Symlink);
    assert!(config.general.backup);
    assert!(config.general.tracked_files.is_empty());
}

#[test]
fn test_general_config_default() {
    let general = GeneralConfig::default();
    assert_eq!(general.default_mode, RestoreMode::Symlink);
    assert!(general.backup);
    assert!(general.tracked_files.is_empty());
    assert!(general.active_profile.is_none());
}

#[test]
fn test_packages_config_default() {
    let packages = PackagesConfig::default();
    assert!(packages.common.contains(&"git".to_string()));
    assert!(packages.common.contains(&"vim".to_string()));
    assert!(packages.common.contains(&"tmux".to_string()));
}

#[test]
fn test_restore_mode_variants() {
    assert_eq!(RestoreMode::Symlink, RestoreMode::Symlink);
    assert_eq!(RestoreMode::Copy, RestoreMode::Copy);
    assert_ne!(RestoreMode::Symlink, RestoreMode::Copy);
}

#[test]
fn test_config_init_creates_directories() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");

    cfg::init(config_path.clone(), false).unwrap();

    assert!(config_path.exists());

    // Read and verify config content
    let content = fs::read_to_string(&config_path).unwrap();
    assert!(content.contains("[general]"));
}

#[test]
fn test_config_init_force_overwrites() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");

    // Create first
    cfg::init(config_path.clone(), false).unwrap();

    // Try to create again without force - should fail
    let result = cfg::init(config_path.clone(), false);
    assert!(result.is_err());

    // With force - should succeed
    cfg::init(config_path.clone(), true).unwrap();
}

#[test]
fn test_config_load_nonexistent() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("nonexistent.toml");

    let result = cfg::load(&config_path);
    assert!(result.is_err());
}

#[test]
fn test_config_save_and_load() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");

    let mut config = Config::default();
    config.general.default_mode = RestoreMode::Copy;
    config.general.backup = false;
    config.packages.common = vec!["neovim".to_string(), "ripgrep".to_string()];

    cfg::save(&config_path, &config).unwrap();

    let loaded = cfg::load(&config_path).unwrap();
    assert_eq!(loaded.general.default_mode, RestoreMode::Copy);
    assert!(!loaded.general.backup);
    assert!(loaded.packages.common.contains(&"neovim".to_string()));
}

#[test]
fn test_config_update_discovered() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");

    // Create initial config
    cfg::init(config_path.clone(), false).unwrap();

    // Update with discovered files
    let discovered_files = vec![
        std::path::PathBuf::from("/home/test/.zshrc"),
        std::path::PathBuf::from("/home/test/.vimrc"),
    ];

    cfg::update_discovered(&config_path, &discovered_files).unwrap();

    let loaded = cfg::load(&config_path).unwrap();
    assert!(loaded
        .general
        .tracked_files
        .contains(&std::path::PathBuf::from("/home/test/.zshrc")));
    assert!(loaded
        .general
        .tracked_files
        .contains(&std::path::PathBuf::from("/home/test/.vimrc")));
}

#[test]
fn test_config_update_discovered_replaces_stale_entries() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");

    cfg::init(config_path.clone(), false).unwrap();

    let mut config = cfg::load(&config_path).unwrap();
    config.general.tracked_files = vec![
        std::path::PathBuf::from("/home/test/.zshrc"),
        std::path::PathBuf::from("/home/test/.config/gcloud/credentials.db"),
    ];
    cfg::save(&config_path, &config).unwrap();

    let discovered_files = vec![std::path::PathBuf::from("/home/test/.zshrc")];
    cfg::update_discovered(&config_path, &discovered_files).unwrap();

    let loaded = cfg::load(&config_path).unwrap();
    assert_eq!(
        loaded.general.tracked_files,
        vec![std::path::PathBuf::from("/home/test/.zshrc")]
    );
}

#[test]
fn test_config_check_exists() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");

    // Should fail when file doesn't exist
    assert!(cfg::check_exists(&config_path).is_err());

    // Create the file
    cfg::init(config_path.clone(), false).unwrap();

    // Now should succeed
    assert!(cfg::check_exists(&config_path).is_ok());
}

#[test]
fn test_config_serialization() {
    let config = Config::default();
    let toml_string = toml::to_string_pretty(&config).unwrap();

    assert!(toml_string.contains("default_mode"));
    assert!(toml_string.contains("backup"));

    // Parse back
    let parsed: Config = toml::from_str(&toml_string).unwrap();
    assert_eq!(parsed.general.default_mode, config.general.default_mode);
}

#[test]
fn test_hooks_config() {
    let config_str = r#"
[general]
default_mode = "symlink"
backup = true

[hooks]
pre_apply = ["echo 'pre'"]
post_apply = ["echo 'post'"]
pre_snapshot = []
post_snapshot = ["git commit -m 'snapshot'"]
"#;

    let config: Config = toml::from_str(config_str).unwrap();
    let hooks = config.hooks.unwrap();

    assert_eq!(hooks.pre_apply.len(), 1);
    assert_eq!(hooks.post_apply.len(), 1);
    assert!(hooks.pre_snapshot.is_empty());
    assert_eq!(hooks.post_snapshot.len(), 1);
}

#[test]
fn test_secrets_config() {
    let config_str = r#"
[general]
default_mode = "symlink"

[secrets]
provider = "age"
key_path = "~/.config/age/keys.txt"
"#;

    let config: Config = toml::from_str(config_str).unwrap();
    let secrets = config.secrets.unwrap();

    assert_eq!(secrets.provider, Some("age".to_string()));
    assert_eq!(secrets.key_path, Some("~/.config/age/keys.txt".to_string()));
}

#[test]
fn test_daemon_config() {
    let config_str = r#"
[general]
default_mode = "symlink"

[daemon]
enabled = true
mode = "auto"
debounce_ms = 2000
"#;

    let config: Config = toml::from_str(config_str).unwrap();
    let daemon = config.daemon.unwrap();

    assert!(daemon.enabled);
    assert_eq!(daemon.mode, "auto");
    assert_eq!(daemon.debounce_ms, 2000);
}

#[test]
fn test_remote_config() {
    let config_str = r#"
[general]
default_mode = "symlink"

[remote]
kind = "s3"
bucket = "my-dotfiles"
region = "us-east-1"
prefix = "backups/"
"#;

    let config: Config = toml::from_str(config_str).unwrap();
    let remote = config.remote.unwrap();

    assert_eq!(remote.kind, "s3");
    assert_eq!(remote.bucket, Some("my-dotfiles".to_string()));
    assert_eq!(remote.region, Some("us-east-1".to_string()));
    assert_eq!(remote.prefix, Some("backups/".to_string()));
}

#[test]
fn test_file_overrides() {
    let config_str = r#"
[general]
default_mode = "symlink"

[files."~/.ssh/config"]
mode = "copy"
exclude = false

[files."~/.config/secrets"]
exclude = true
"#;

    let config: Config = toml::from_str(config_str).unwrap();

    let ssh_override = config.files.get("~/.ssh/config").unwrap();
    assert_eq!(ssh_override.mode, Some(RestoreMode::Copy));
    assert!(!ssh_override.exclude);

    let secrets_override = config.files.get("~/.config/secrets").unwrap();
    assert!(secrets_override.exclude);
}
