use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub general: GeneralConfig,

    #[serde(default)]
    pub github: GitHubConfig,

    #[serde(default)]
    pub packages: PackagesConfig,

    #[serde(default)]
    pub exclude_patterns: Vec<String>,

    #[serde(default)]
    pub include_patterns: Vec<String>,

    #[serde(default)]
    pub files: BTreeMap<String, FileOverride>,

    #[serde(default)]
    pub push_ignore: Vec<String>,

    // Secrets configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secrets: Option<SecretsConfig>,

    // Hooks configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hooks: Option<HooksConfig>,

    // Daemon configuration (future milestone)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub daemon: Option<DaemonConfig>,

    // Auto-pruning configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_prune: Option<AutoPruneConfig>,

    // Remote configuration (future milestone)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote: Option<RemoteConfig>,

    // Legacy field for compatibility
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dotfiles: Option<DotfilesConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    #[serde(default = "default_mode")]
    pub default_mode: RestoreMode,

    #[serde(default = "default_backup")]
    pub backup: bool,

    #[serde(default)]
    pub tracked_files: Vec<PathBuf>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_profile: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum RestoreMode {
    Symlink,
    Copy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileOverride {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<RestoreMode>,

    #[serde(default)]
    pub exclude: bool,

    #[serde(default)]
    pub local_only: bool,
}

// Legacy config for migration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DotfilesConfig {
    #[serde(default = "default_repo_path")]
    pub repo_path: PathBuf,

    #[serde(default = "default_symlink")]
    pub use_symlinks: bool,

    #[serde(default)]
    pub tracked_files: Vec<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubConfig {
    pub username: Option<String>,
    pub repo_name: Option<String>,
    #[serde(default = "default_private")]
    pub private: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackagesConfig {
    #[serde(default)]
    pub common: Vec<String>,

    #[serde(default)]
    pub macos: Vec<String>,

    #[serde(default)]
    pub linux: Vec<String>,

    #[serde(default)]
    pub ubuntu: Vec<String>,

    #[serde(default)]
    pub arch: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretsConfig {
    /// Provider: "age" or "sops"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,

    /// Path to key file (e.g., "~/.config/age/keys.txt")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HooksConfig {
    #[serde(default)]
    pub pre_apply: Vec<String>,

    #[serde(default)]
    pub post_apply: Vec<String>,

    #[serde(default)]
    pub pre_snapshot: Vec<String>,

    #[serde(default)]
    pub post_snapshot: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    #[serde(default)]
    pub enabled: bool,

    /// Mode: "ask" or "auto"
    #[serde(default = "default_daemon_mode")]
    pub mode: String,

    #[serde(default = "default_debounce_ms")]
    pub debounce_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoPruneConfig {
    #[serde(default)]
    pub enabled: bool,

    /// Keep N most recent snapshots
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_count: Option<usize>,

    /// Keep snapshots newer than this duration (e.g., "30d", "7d", "2w")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_age: Option<String>,

    /// Keep snapshots until total size is under this limit (e.g., "1GB", "500MB")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_size: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteConfig {
    /// Kind: "github", "s3", "gcs", "webdav"
    pub kind: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub bucket: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            general: GeneralConfig::default(),
            github: GitHubConfig::default(),
            packages: PackagesConfig::default(),
            exclude_patterns: default_exclude_patterns(),
            include_patterns: default_include_patterns(),
            files: BTreeMap::new(),
            push_ignore: Vec::new(),
            secrets: None,
            hooks: None,
            daemon: None,
            auto_prune: None,
            remote: None,
            dotfiles: None,
        }
    }
}

impl Default for GeneralConfig {
    fn default() -> Self {
        GeneralConfig {
            default_mode: default_mode(),
            backup: default_backup(),
            tracked_files: Vec::new(),
            active_profile: None,
        }
    }
}

impl Default for DotfilesConfig {
    fn default() -> Self {
        DotfilesConfig {
            repo_path: default_repo_path(),
            use_symlinks: default_symlink(),
            tracked_files: Vec::new(),
        }
    }
}

impl Default for GitHubConfig {
    fn default() -> Self {
        GitHubConfig {
            username: None,
            repo_name: None,
            private: default_private(),
        }
    }
}

impl Default for PackagesConfig {
    fn default() -> Self {
        PackagesConfig {
            common: vec![
                "git".to_string(),
                "vim".to_string(),
                "tmux".to_string(),
                "curl".to_string(),
                "wget".to_string(),
            ],
            macos: vec![],
            linux: vec![],
            ubuntu: vec![],
            arch: vec![],
        }
    }
}

fn default_repo_path() -> PathBuf {
    crate::paths::compiled_dir().expect("Could not determine dotdipper compiled directory")
}

fn default_symlink() -> bool {
    true
}

fn default_private() -> bool {
    true
}

fn default_mode() -> RestoreMode {
    RestoreMode::Symlink
}

fn default_backup() -> bool {
    true
}

fn default_exclude_patterns() -> Vec<String> {
    vec![
        // --- Dotdipper internal (generated / runtime) ---
        "~/.config/dotdipper/compiled/**".to_string(),
        "~/.config/dotdipper/cache/**".to_string(),
        "~/.config/dotdipper/install/**".to_string(),
        "~/.config/dotdipper/manifest.lock".to_string(),
        "~/.config/dotdipper/profiles/*/compiled/**".to_string(),
        "~/.config/dotdipper/profiles/*/manifest.lock".to_string(),
        "~/.config/dotdipper/bundle*.tar.zst".to_string(),
        "~/.config/dotdipper/daemon.pid".to_string(),
        "~/.config/dotdipper/snapshots/**".to_string(),
        // --- Cryptographic keys & secrets ---
        "~/.ssh/**".to_string(),
        "~/.gnupg/**".to_string(),
        "~/.config/age/keys.txt".to_string(),
        "**/*.key".to_string(),
        "**/*.pem".to_string(),
        "**/*.pfx".to_string(),
        "**/*.p12".to_string(),
        "**/*.keystore".to_string(),
        // --- Credentials & tokens ---
        "**/credentials.db".to_string(),
        "**/access_tokens.db".to_string(),
        "**/tokens.json".to_string(),
        "**/legacy_credentials/**".to_string(),
        "~/.config/gh/hosts.yml".to_string(),
        "~/.config/gcloud/**".to_string(),
        // --- Environment & secret files ---
        "**/.env".to_string(),
        "**/.env.local".to_string(),
        "**/.env.production".to_string(),
        "**/.env.*.local".to_string(),
        "**/secrets/**".to_string(),
        "**/.secret*".to_string(),
        "**/*.secret".to_string(),
        // --- Build & dependency artifacts ---
        "**/node_modules/**".to_string(),
        "**/.git/**".to_string(),
        "**/target/**".to_string(),
        "**/dist/**".to_string(),
        "**/build/**".to_string(),
        "**/__pycache__/**".to_string(),
        "**/.venv/**".to_string(),
        // --- OS & editor junk ---
        "**/.DS_Store".to_string(),
        "**/Thumbs.db".to_string(),
        "**/*.swp".to_string(),
        "**/*.swo".to_string(),
        "**/*~".to_string(),
        // --- Caches, logs & temp ---
        "**/cache/**".to_string(),
        "**/Cache/**".to_string(),
        "**/tmp/**".to_string(),
        "**/temp/**".to_string(),
        "**/logs/**".to_string(),
        "**/*.log".to_string(),
        // --- Backup files (auto-generated) ---
        "**/*.bak".to_string(),
        "**/*.backup".to_string(),
        "**/automatic_backups/**".to_string(),
        // --- Application state (machine-specific) ---
        "~/.config/configstore/**".to_string(),
        "**/sockets/**".to_string(),
        "**/*.db".to_string(),
        "**/*.sqlite".to_string(),
        "**/*.sqlite3".to_string(),
        // --- Trash ---
        "~/.local/share/Trash/**".to_string(),
        "~/.Trash/**".to_string(),
    ]
}

fn default_include_patterns() -> Vec<String> {
    vec![
        "~/.config/**".to_string(),
        "~/.zshrc".to_string(),
        "~/.bashrc".to_string(),
        "~/.profile".to_string(),
        "~/.gitconfig".to_string(),
        "~/.gitignore_global".to_string(),
        "~/.vimrc".to_string(),
        "~/.tmux.conf".to_string(),
        "~/.ssh/config".to_string(), // Only SSH config, not keys
    ]
}

fn default_daemon_mode() -> String {
    "ask".to_string()
}

fn default_debounce_ms() -> u64 {
    1500
}

pub fn init(config_path: PathBuf, force: bool) -> Result<()> {
    if config_path.exists() && !force {
        anyhow::bail!(
            "Config already exists at {}. Use --force to overwrite.",
            config_path.display()
        );
    }

    // Create directory if it doesn't exist
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).context("Failed to create config directory")?;
    }

    // Create default config
    let config = Config::default();

    // Write config to file
    let toml_string = toml::to_string_pretty(&config).context("Failed to serialize config")?;
    fs::write(&config_path, toml_string).context("Failed to write config file")?;

    // Create required directories
    let base_dir = crate::paths::base_dir()?;

    fs::create_dir_all(base_dir.join("compiled")).context("Failed to create compiled directory")?;
    fs::create_dir_all(base_dir.join("install")).context("Failed to create install directory")?;
    fs::create_dir_all(base_dir.join("cache")).context("Failed to create cache directory")?;

    // Create manifest directory
    let manifest_dir = config_path
        .parent()
        .expect("Config path should have parent");
    fs::create_dir_all(manifest_dir).context("Failed to create manifest directory")?;

    Ok(())
}

pub fn load(config_path: &Path) -> Result<Config> {
    if !config_path.exists() {
        anyhow::bail!(
            "Config not found at {}. Run 'dotdipper init' first.",
            config_path.display()
        );
    }

    let contents = fs::read_to_string(config_path).context("Failed to read config file")?;
    let mut config: Config = toml::from_str(&contents).context("Failed to parse config file")?;

    // Migrate from legacy dotfiles config if present
    if let Some(dotfiles) = &config.dotfiles {
        config.general.tracked_files = dotfiles.tracked_files.clone();
        // Note: we keep the dotfiles section for backward compatibility but use general.tracked_files
    }

    Ok(config)
}

pub fn save(config_path: &Path, config: &Config) -> Result<()> {
    let toml_string = toml::to_string_pretty(config).context("Failed to serialize config")?;
    fs::write(config_path, toml_string).context("Failed to write config file")?;
    Ok(())
}

pub fn update_discovered(config_path: &Path, files: &[PathBuf]) -> Result<()> {
    let mut config = load(config_path)?;

    // Add new files that aren't already tracked
    for file in files {
        if !config.general.tracked_files.contains(file) {
            config.general.tracked_files.push(file.clone());
        }
    }

    // Sort for deterministic output
    config.general.tracked_files.sort();

    save(config_path, &config)?;
    Ok(())
}

pub fn edit(config_path: &Path) -> Result<()> {
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());

    Command::new(editor)
        .arg(config_path)
        .status()
        .context("Failed to open editor")?;

    Ok(())
}

pub fn check_exists(config_path: &Path) -> Result<()> {
    if config_path.exists() {
        Ok(())
    } else {
        anyhow::bail!("Config file not found")
    }
}

/// Returns relative paths (relative to $HOME) that should be excluded from git push.
/// Combines top-level `push_ignore` patterns and per-file `local_only` entries.
pub fn resolve_push_ignored_paths(config: &Config) -> Result<Vec<String>> {
    let home = dirs::home_dir().context("Failed to find home directory")?;
    let mut ignored = Vec::new();

    for pattern in &config.push_ignore {
        let expanded = if let Some(rest) = pattern.strip_prefix("~/") {
            rest.to_string()
        } else {
            pattern.clone()
        };
        ignored.push(expanded);
    }

    for (file_path, file_override) in &config.files {
        if file_override.local_only {
            let expanded = if let Some(rest) = file_path.strip_prefix("~/") {
                rest.to_string()
            } else if let Ok(stripped) = PathBuf::from(file_path).strip_prefix(&home) {
                stripped.to_string_lossy().to_string()
            } else {
                file_path.clone()
            };
            ignored.push(expanded);
        }
    }

    ignored.sort();
    ignored.dedup();
    Ok(ignored)
}

pub fn add_push_ignore(config_path: &Path, pattern: &str) -> Result<()> {
    let mut config = load(config_path)?;
    let pattern = pattern.trim();

    if pattern.is_empty() {
        anyhow::bail!("Ignore pattern cannot be empty");
    }

    if !config
        .push_ignore
        .iter()
        .any(|existing| existing == pattern)
    {
        config.push_ignore.push(pattern.to_string());
        config.push_ignore.sort();
    }

    save(config_path, &config)?;
    Ok(())
}

pub fn remove_push_ignore(config_path: &Path, pattern: &str) -> Result<()> {
    let mut config = load(config_path)?;
    let pattern = pattern.trim();

    if pattern.is_empty() {
        anyhow::bail!("Ignore pattern cannot be empty");
    }

    config.push_ignore.retain(|existing| existing != pattern);
    save(config_path, &config)?;
    Ok(())
}

pub fn set_config_value(config_path: &Path, key: &str, value: &str) -> Result<()> {
    let mut config = load(config_path)?;

    match key {
        "github.username" => config.github.username = Some(value.to_string()),
        "github.repo_name" => config.github.repo_name = Some(value.to_string()),
        "github.private" => {
            config.github.private = value
                .parse()
                .context("Invalid boolean value. Use 'true' or 'false'")?
        }
        "general.default_mode" => {
            config.general.default_mode = match value {
                "symlink" => RestoreMode::Symlink,
                "copy" => RestoreMode::Copy,
                _ => anyhow::bail!("Invalid mode '{}'. Use 'symlink' or 'copy'", value),
            }
        }
        "general.backup" => {
            config.general.backup = value
                .parse()
                .context("Invalid boolean value. Use 'true' or 'false'")?
        }
        _ => anyhow::bail!(
            "Unknown config key '{}'. Supported keys:\n  \
             github.username, github.repo_name, github.private,\n  \
             general.default_mode, general.backup",
            key
        ),
    }

    save(config_path, &config)?;
    Ok(())
}
