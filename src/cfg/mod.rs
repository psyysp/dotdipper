use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use walkdir::WalkDir;

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    vec![]
}

pub const DEFAULT_IGNORE_CONTENTS: &str = "\
# .dotdipperignore — gitignore-style patterns for dotdipper discover
# Lines starting with # are comments.  Blank lines are ignored.
# Patterns prefixed with ~/ are anchored to $HOME.

# --- Dotdipper internal (generated / runtime) ---
~/.config/dotdipper/compiled/**
~/.config/dotdipper/cache/**
~/.config/dotdipper/install/**
~/.config/dotdipper/manifest.lock
~/.config/dotdipper/snapshots/**
~/.config/dotdipper/profiles/*/compiled/**
~/.config/dotdipper/profiles/*/manifest.lock
~/.config/dotdipper/bundle*.tar.zst
~/.config/dotdipper/daemon.pid

# --- Cryptographic keys & secrets ---
~/.ssh/**
~/.gnupg/**
~/.config/age/keys.txt
**/*.key
**/*.pem
**/*.pfx
**/*.p12
**/*.keystore

# --- Credentials & tokens ---
**/credentials.db
**/access_tokens.db
**/tokens.json
**/legacy_credentials/**
~/.config/gh/hosts.yml
~/.config/gcloud/**

# --- Environment & secret files ---
**/.env
**/.env.local
**/.env.production
**/.env.*.local
**/secrets/**
**/.secret*
**/*.secret

# --- Build & dependency artifacts ---
**/node_modules/**
**/.git/**
**/target/**
**/dist/**
**/build/**
**/__pycache__/**
**/.venv/**

# --- OS & editor junk ---
**/.DS_Store
**/Thumbs.db
**/*.swp
**/*.swo
**/*~

# --- Caches, logs & temp ---
**/cache/**
**/Cache/**
**/tmp/**
**/temp/**
**/logs/**
**/*.log

# --- Backup files (auto-generated) ---
**/*.bak
**/*.bak.*
**/*.backup
**/backup-*
**/old-*
**/temp-*
**/automatic_backups/**

# --- Application state (machine-specific) ---
~/.config/configstore/**
**/sockets/**
**/*.db
**/*.sqlite
**/*.sqlite3

# --- Trash ---
~/.local/share/Trash/**
~/.Trash/**
";

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

    // Write default .dotdipperignore
    let ignore_path = crate::paths::ignore_file()?;
    if !ignore_path.exists() || force {
        fs::write(&ignore_path, DEFAULT_IGNORE_CONTENTS)
            .context("Failed to write .dotdipperignore")?;
    }

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

    if let Some(home) = dirs::home_dir() {
        config.general.tracked_files = config
            .general
            .tracked_files
            .into_iter()
            .map(|p| expand_tracked_path(&p, &home))
            .collect();
    }

    Ok(config)
}

pub fn save(config_path: &Path, config: &Config) -> Result<()> {
    let to_write = portable_config(config);
    let toml_string = toml::to_string_pretty(&to_write).context("Failed to serialize config")?;
    fs::write(config_path, toml_string).context("Failed to write config file")?;
    Ok(())
}

/// Clone of `config` with tracked paths stored as portable `~/…` entries.
pub fn portable_config(config: &Config) -> Config {
    let mut cloned = config.clone();
    if let Some(home) = dirs::home_dir() {
        cloned.general.tracked_files = cloned
            .general
            .tracked_files
            .iter()
            .map(|p| to_portable_tracked_path(p, &home))
            .collect();
    }
    cloned
}

pub fn update_discovered(config_path: &Path, files: &[PathBuf]) -> Result<()> {
    let mut config = load(config_path)?;
    let mut tracked_files = files.to_vec();
    tracked_files.sort();
    tracked_files.dedup();
    config.general.tracked_files = tracked_files;

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

/// Convert a tracked path (`~/…`, absolute under `$HOME`, or already relative) into a
/// home-relative path. Returns `None` when the path is outside `$HOME`.
pub fn home_relative_path(path: &Path, home: &Path) -> Option<PathBuf> {
    let raw = path.to_string_lossy();
    if raw == "~" {
        return Some(PathBuf::new());
    }
    if let Some(rest) = raw.strip_prefix("~/") {
        return Some(PathBuf::from(rest));
    }
    if let Ok(stripped) = path.strip_prefix(home) {
        return Some(stripped.to_path_buf());
    }
    if path.is_relative() && !raw.starts_with('/') {
        return Some(path.to_path_buf());
    }
    None
}

/// Look up a `[files]` override: exact `~/rel` or `rel` key, then longest directory prefix.
pub fn file_override_for<'a>(config: &'a Config, rel: &Path) -> Option<&'a FileOverride> {
    let exact_tilde = format!("~/{}", rel.display());
    if let Some(ov) = config.files.get(&exact_tilde) {
        return Some(ov);
    }
    let exact = rel.display().to_string();
    if let Some(ov) = config.files.get(&exact) {
        return Some(ov);
    }

    let mut best: Option<(&FileOverride, usize)> = None;
    for (key, ov) in &config.files {
        let key_rel = key.strip_prefix("~/").unwrap_or(key.as_str());
        let key_path = Path::new(key_rel);
        if key_path.as_os_str().is_empty() {
            continue;
        }
        if rel.starts_with(key_path) {
            let len = key_rel.len();
            if best.map(|(_, l)| len > l).unwrap_or(true) {
                best = Some((ov, len));
            }
        }
    }
    best.map(|(ov, _)| ov)
}

/// Expand `~/…` or home-relative paths to an absolute path under `$HOME`.
pub fn expand_tracked_path(path: &Path, home: &Path) -> PathBuf {
    match home_relative_path(path, home) {
        Some(rel) if rel.as_os_str().is_empty() => home.to_path_buf(),
        Some(rel) => home.join(rel),
        None => path.to_path_buf(),
    }
}

/// Store a path as `~/rel` when it lives under `$HOME`, otherwise leave it unchanged.
pub fn to_portable_tracked_path(path: &Path, home: &Path) -> PathBuf {
    match home_relative_path(path, home) {
        Some(rel) if rel.as_os_str().is_empty() => PathBuf::from("~"),
        Some(rel) => PathBuf::from(format!("~/{}", rel.display())),
        None => path.to_path_buf(),
    }
}

/// Existing files from `tracked_files`, with directories expanded to their files.
pub fn existing_tracked_files(config: &Config) -> Result<Vec<PathBuf>> {
    let home = dirs::home_dir().context("Failed to find home directory")?;
    let mut out = Vec::new();

    for tracked in &config.general.tracked_files {
        let abs = expand_tracked_path(tracked, &home);
        if abs.is_dir() {
            for entry in WalkDir::new(&abs)
                .follow_links(false)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                let p = entry.path();
                if p.is_file() {
                    out.push(p.to_path_buf());
                }
            }
        } else if abs.is_file() {
            out.push(abs);
        }
    }

    out.sort();
    out.dedup();
    Ok(out)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portable_roundtrip_under_home() {
        let home = PathBuf::from("/home/alice");
        let abs = home.join(".zshrc");
        assert_eq!(
            home_relative_path(&abs, &home).unwrap(),
            PathBuf::from(".zshrc")
        );
        assert_eq!(
            to_portable_tracked_path(&abs, &home),
            PathBuf::from("~/.zshrc")
        );
        assert_eq!(expand_tracked_path(&PathBuf::from("~/.zshrc"), &home), abs);
        assert_eq!(
            expand_tracked_path(&PathBuf::from(".vimrc"), &home),
            home.join(".vimrc")
        );
    }

    #[test]
    fn portable_leaves_outside_home_unchanged() {
        let home = PathBuf::from("/home/alice");
        let abs = PathBuf::from("/etc/hosts");
        assert!(home_relative_path(&abs, &home).is_none());
        assert_eq!(to_portable_tracked_path(&abs, &home), abs);
    }
}
