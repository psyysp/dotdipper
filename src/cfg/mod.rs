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

    // macOS application capture (Homebrew / MAS / /Applications)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub apps: Option<AppsConfig>,

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
    /// Optional git branch override. When unset, `default` uses `main` and
    /// other profiles use `dotdipper/<name>`. Independent of `repo_name`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
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

    /// Extra age recipients for SOPS encrypt (multi-machine).
    /// When set (or when `SOPS_AGE_RECIPIENTS` / `.sops.yaml` applies), encrypt does not
    /// force a single local `--age` key.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recipients: Vec<String>,
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
pub struct AppsConfig {
    /// Capture Homebrew / MAS / Applications during `dotdipper push`.
    #[serde(default = "default_true")]
    pub capture_on_push: bool,

    /// Scan `/Applications` and `~/Applications` for unmanaged apps.
    #[serde(default = "default_true")]
    pub scan_applications: bool,
}

impl Default for AppsConfig {
    fn default() -> Self {
        AppsConfig {
            capture_on_push: true,
            scan_applications: true,
        }
    }
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
            apps: None,
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
            active_profile: Some("default".to_string()),
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
            branch: None,
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
~/.config/dotdipper/profiles/**
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

fn default_true() -> bool {
    true
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

    // Create default config (active_profile = default)
    let config = Config::default();

    // Write config to file
    let toml_string = toml::to_string_pretty(&config).context("Failed to serialize config")?;
    fs::write(&config_path, toml_string).context("Failed to write config file")?;

    // Create required directories + default profile store
    let base_dir = crate::paths::base_dir()?;

    fs::create_dir_all(base_dir.join("install")).context("Failed to create install directory")?;
    fs::create_dir_all(base_dir.join("cache")).context("Failed to create cache directory")?;
    crate::profiles::ensure_exists("default").context("Failed to create default profile store")?;
    crate::profiles::refresh_compat_links("default")
        .context("Failed to create profile compatibility links")?;

    // Write default .dotdipperignore
    let ignore_path = crate::paths::ignore_file()?;
    if !ignore_path.exists() || force {
        fs::write(&ignore_path, DEFAULT_IGNORE_CONTENTS)
            .context("Failed to write .dotdipperignore")?;
    }

    Ok(())
}

/// Parse a single config file with no profile overlay.
/// Use this when reading/writing the global `config.toml` so overlay keys
/// are never flattened back into the base file.
pub fn load_file(config_path: &Path) -> Result<Config> {
    if !config_path.exists() {
        anyhow::bail!(
            "Config not found at {}. Run 'dotdipper init' first.",
            config_path.display()
        );
    }

    let contents = fs::read_to_string(config_path).context("Failed to read config file")?;
    let config: Config = toml::from_str(&contents).context("Failed to parse config file")?;
    Ok(normalize_config(config))
}

/// Load global config and overlay `profiles/<active>/config.toml`.
/// Overlay keys win. `general.active_profile` is never taken from an overlay.
pub fn load(config_path: &Path) -> Result<Config> {
    let base = load_file(config_path)?;
    apply_profile_overlay(config_path, base)
}

/// Comment-only starter overlay so new profiles inherit the global config.
pub const SPARSE_OVERLAY_CONTENTS: &str = r#"# Per-profile overlay.
# Keys here override the global config.toml for this profile only.
# Leave this file comments-only to inherit everything from the global config.
#
# [github]
# repo_name = "dotfiles-work"   # optional dedicated repository
# branch = "main"               # optional; default is "main" (default profile) or "dotdipper/<name>"
"#;

pub fn overlay_path_for(profile: &str) -> Result<PathBuf> {
    Ok(crate::paths::base_dir()?
        .join("profiles")
        .join(profile)
        .join("config.toml"))
}

pub fn write_sparse_overlay_if_missing(path: &Path) -> Result<()> {
    if path.exists() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context("Failed to create profile overlay directory")?;
    }
    atomic_write(path, SPARSE_OVERLAY_CONTENTS).context("Failed to write profile overlay")?;
    Ok(())
}

fn apply_profile_overlay(config_path: &Path, base: Config) -> Result<Config> {
    let profile =
        crate::profiles::resolve_active_profile_name().unwrap_or_else(|_| "default".into());
    let overlay_path = overlay_path_for(&profile)?;
    if overlay_path == config_path {
        return Ok(base);
    }
    let Some(overlay) = parse_overlay_file(&overlay_path)? else {
        return Ok(base);
    };
    let base_value = toml::Value::try_from(&base)
        .context("Failed to serialize base config for overlay merge")?;
    let merged = merge_toml_values(base_value, overlay);
    let config: Config = merged
        .try_into()
        .context("Failed to parse merged profile overlay")?;
    Ok(normalize_config(config))
}

fn parse_overlay_file(path: &Path) -> Result<Option<toml::Value>> {
    if !path.exists() {
        return Ok(None);
    }
    let contents = fs::read_to_string(path)
        .with_context(|| format!("Failed to read profile overlay {}", path.display()))?;
    if overlay_is_blank(&contents) {
        return Ok(None);
    }
    let mut value: toml::Value = toml::from_str(&contents)
        .with_context(|| format!("Failed to parse profile overlay {}", path.display()))?;
    sanitize_overlay(&mut value);
    if value.as_table().map(|t| t.is_empty()).unwrap_or(true) {
        return Ok(None);
    }
    Ok(Some(value))
}

fn overlay_is_blank(contents: &str) -> bool {
    contents.lines().all(|line| {
        let trimmed = line.trim();
        trimmed.is_empty() || trimmed.starts_with('#')
    })
}

/// Overlay keys win. Tables merge recursively; arrays and scalars replace.
pub fn merge_toml_values(base: toml::Value, overlay: toml::Value) -> toml::Value {
    match (base, overlay) {
        (toml::Value::Table(mut base_map), toml::Value::Table(overlay_map)) => {
            for (key, overlay_val) in overlay_map {
                match base_map.remove(&key) {
                    Some(base_val) => {
                        base_map.insert(key, merge_toml_values(base_val, overlay_val));
                    }
                    None => {
                        base_map.insert(key, overlay_val);
                    }
                }
            }
            toml::Value::Table(base_map)
        }
        (_base, overlay) => overlay,
    }
}

fn sanitize_overlay(value: &mut toml::Value) {
    let Some(table) = value.as_table_mut() else {
        return;
    };

    if let Some(general) = table.get_mut("general").and_then(|v| v.as_table_mut()) {
        general.remove("active_profile");
        if matches!(general.get("tracked_files"), Some(toml::Value::Array(a)) if a.is_empty()) {
            general.remove("tracked_files");
        }
        if general.is_empty() {
            table.remove("general");
        }
    }

    if let Some(packages) = table.get_mut("packages").and_then(|v| v.as_table_mut()) {
        for key in ["common", "macos", "linux", "ubuntu", "arch"] {
            if matches!(packages.get(key), Some(toml::Value::Array(a)) if a.is_empty()) {
                packages.remove(key);
            }
        }
        if packages.is_empty() {
            table.remove("packages");
        }
    }

    if let Some(github) = table.get_mut("github").and_then(|v| v.as_table_mut()) {
        for key in ["username", "repo_name", "branch"] {
            if matches!(github.get(key), Some(toml::Value::String(s)) if s.trim().is_empty()) {
                github.remove(key);
            }
        }
        if github.is_empty() {
            table.remove("github");
        }
    }
}

fn normalize_config(mut config: Config) -> Config {
    if let Some(dotfiles) = &config.dotfiles {
        if config.general.tracked_files.is_empty() {
            config.general.tracked_files = dotfiles.tracked_files.clone();
        }
    }

    config.general.tracked_files = config
        .general
        .tracked_files
        .into_iter()
        .map(expand_user_path)
        .collect();

    if let Some(secrets) = config.secrets.as_mut() {
        if let Some(key_path) = secrets.key_path.as_mut() {
            *key_path = expand_user_path(PathBuf::from(key_path.clone()))
                .to_string_lossy()
                .to_string();
        }
    }

    config
}

fn expand_user_path(path: PathBuf) -> PathBuf {
    let as_str = path.to_string_lossy();
    PathBuf::from(shellexpand::tilde(&as_str).as_ref())
}

fn atomic_write(path: &Path, contents: impl AsRef<[u8]>) -> Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .with_context(|| format!("Failed to create directory for {}", path.display()))?;
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("config.toml");
    let tmp = parent.join(format!(".{file_name}.{}.tmp", std::process::id()));
    fs::write(&tmp, contents.as_ref())
        .with_context(|| format!("Failed to write temporary file {}", tmp.display()))?;
    if let Err(err) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(err).with_context(|| format!("Failed to write {}", path.display()));
    }
    Ok(())
}

pub fn save(config_path: &Path, config: &Config) -> Result<()> {
    let toml_string = toml::to_string_pretty(config).context("Failed to serialize config")?;
    atomic_write(config_path, toml_string).context("Failed to write config file")?;
    Ok(())
}

fn read_overlay_table(path: &Path) -> Result<toml::map::Map<String, toml::Value>> {
    if !path.exists() {
        return Ok(toml::map::Map::new());
    }
    let contents = fs::read_to_string(path)
        .with_context(|| format!("Failed to read profile overlay {}", path.display()))?;
    if overlay_is_blank(&contents) {
        return Ok(toml::map::Map::new());
    }
    let value: toml::Value = toml::from_str(&contents)
        .with_context(|| format!("Failed to parse profile overlay {}", path.display()))?;
    Ok(value.as_table().cloned().unwrap_or_default())
}

fn write_overlay_table(path: &Path, table: toml::map::Map<String, toml::Value>) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let serialized = toml::to_string_pretty(&toml::Value::Table(table))
        .context("Failed to serialize profile overlay")?;
    atomic_write(path, serialized)
}

fn overlay_general_table(
    table: &mut toml::map::Map<String, toml::Value>,
) -> &mut toml::map::Map<String, toml::Value> {
    let general = table
        .entry("general".to_string())
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
    if !general.is_table() {
        *general = toml::Value::Table(toml::map::Map::new());
    }
    general.as_table_mut().expect("general table")
}

/// Write discovered tracked files to the active profile overlay when a
/// profile store exists; otherwise write them to the global config file.
pub fn update_discovered(config_path: &Path, files: &[PathBuf]) -> Result<()> {
    let mut tracked_files = files.to_vec();
    tracked_files.sort();
    tracked_files.dedup();

    if let Ok(profile) = crate::profiles::resolve_active_profile_name() {
        if let Ok(overlay_path) = overlay_path_for(&profile) {
            if overlay_path.parent().map(|p| p.exists()).unwrap_or(false) {
                let mut table = read_overlay_table(&overlay_path)?;
                let general = overlay_general_table(&mut table);
                general.remove("active_profile");
                general.insert(
                    "tracked_files".to_string(),
                    toml::Value::Array(
                        tracked_files
                            .iter()
                            .map(|p| toml::Value::String(p.to_string_lossy().into_owned()))
                            .collect(),
                    ),
                );
                return write_overlay_table(&overlay_path, table);
            }
        }
    }

    let mut config = load_file(config_path)?;
    config.general.tracked_files = tracked_files;
    save(config_path, &config)?;
    Ok(())
}

/// Persist discovered package names on the active profile overlay when possible.
pub fn update_packages_common(config_path: &Path, packages: Vec<String>) -> Result<()> {
    if let Ok(profile) = crate::profiles::resolve_active_profile_name() {
        if let Ok(overlay_path) = overlay_path_for(&profile) {
            if overlay_path.parent().map(|p| p.exists()).unwrap_or(false) {
                let mut table = read_overlay_table(&overlay_path)?;
                let packages_table = table
                    .entry("packages".to_string())
                    .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
                if !packages_table.is_table() {
                    *packages_table = toml::Value::Table(toml::map::Map::new());
                }
                if let Some(pkg) = packages_table.as_table_mut() {
                    pkg.insert(
                        "common".to_string(),
                        toml::Value::Array(packages.into_iter().map(toml::Value::String).collect()),
                    );
                }
                return write_overlay_table(&overlay_path, table);
            }
        }
    }

    let mut config = load_file(config_path)?;
    config.packages.common = packages;
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
    let mut config = load_file(config_path)?;
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
    let mut config = load_file(config_path)?;
    let pattern = pattern.trim();

    if pattern.is_empty() {
        anyhow::bail!("Ignore pattern cannot be empty");
    }

    config.push_ignore.retain(|existing| existing != pattern);
    save(config_path, &config)?;
    Ok(())
}

pub fn set_config_value(config_path: &Path, key: &str, value: &str) -> Result<()> {
    let mut config = load_file(config_path)?;

    match key {
        "github.username" => config.github.username = Some(value.to_string()),
        "github.repo_name" => config.github.repo_name = Some(value.to_string()),
        "github.branch" => {
            let trimmed = value.trim();
            config.github.branch = if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            };
        }
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
             github.username, github.repo_name, github.branch, github.private,\n  \
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
    use serial_test::serial;
    use tempfile::TempDir;

    fn merge_from_toml(base: &str, overlay: &str) -> Config {
        let mut overlay_value: toml::Value = toml::from_str(overlay).unwrap();
        sanitize_overlay(&mut overlay_value);
        let base_value: toml::Value = toml::from_str(base).unwrap();
        let merged = merge_toml_values(base_value, overlay_value);
        let config: Config = merged.try_into().unwrap();
        normalize_config(config)
    }

    #[test]
    fn overlay_keys_win_and_active_profile_is_stripped() {
        let config = merge_from_toml(
            r#"
[general]
active_profile = "default"
backup = true
tracked_files = ["/tmp/a"]

[github]
username = "alice"
repo_name = "dotfiles"
"#,
            r#"
[general]
active_profile = "should-not-apply"
tracked_files = ["/tmp/b"]

[github]
repo_name = "dotfiles-work"
branch = "dotdipper/work"
"#,
        );

        assert_eq!(config.general.active_profile.as_deref(), Some("default"));
        assert_eq!(config.general.tracked_files, vec![PathBuf::from("/tmp/b")]);
        assert_eq!(config.github.username.as_deref(), Some("alice"));
        assert_eq!(config.github.repo_name.as_deref(), Some("dotfiles-work"));
        assert_eq!(config.github.branch.as_deref(), Some("dotdipper/work"));
        assert!(config.general.backup);
    }

    #[test]
    fn empty_tracked_files_and_packages_inherit() {
        let config = merge_from_toml(
            r#"
[general]
tracked_files = ["/tmp/keep"]

[packages]
common = ["git"]
macos = ["fzf"]
"#,
            r#"
[general]
tracked_files = []

[packages]
common = []
"#,
        );

        assert_eq!(
            config.general.tracked_files,
            vec![PathBuf::from("/tmp/keep")]
        );
        assert_eq!(config.packages.common, vec!["git".to_string()]);
        assert_eq!(config.packages.macos, vec!["fzf".to_string()]);
    }

    #[test]
    fn empty_include_patterns_replace() {
        let config = merge_from_toml(
            r#"
include_patterns = ["~/.zshrc"]
exclude_patterns = ["**/*.key"]
"#,
            r#"
include_patterns = []
"#,
        );

        assert!(config.include_patterns.is_empty());
        assert_eq!(config.exclude_patterns, vec!["**/*.key".to_string()]);
    }

    #[test]
    #[serial]
    fn switch_does_not_flatten_overlay_into_global() {
        let temp = TempDir::new().unwrap();
        let base = temp.path().join("dotdipper");
        fs::create_dir_all(base.join("profiles").join("work")).unwrap();
        let global = base.join("config.toml");
        fs::write(
            &global,
            r#"
[general]
active_profile = "default"
backup = true
tracked_files = ["/tmp/global"]

[github]
username = "alice"
repo_name = "dotfiles"
"#,
        )
        .unwrap();
        fs::write(
            base.join("profiles").join("work").join("config.toml"),
            r#"
[github]
repo_name = "dotfiles-work"
"#,
        )
        .unwrap();

        std::env::set_var("DOTDIPPER_HOME", &base);
        std::env::set_var("DOTDIPPER_PROFILE", "work");
        std::env::remove_var("XDG_CONFIG_HOME");

        let loaded = load(&global).unwrap();
        assert_eq!(loaded.github.repo_name.as_deref(), Some("dotfiles-work"));

        let mut raw = load_file(&global).unwrap();
        raw.general.active_profile = Some("work".to_string());
        save(&global, &raw).unwrap();

        let global_after = fs::read_to_string(&global).unwrap();
        assert!(global_after.contains("repo_name = \"dotfiles\""));
        assert!(!global_after.contains("dotfiles-work"));

        std::env::remove_var("DOTDIPPER_PROFILE");
        std::env::remove_var("DOTDIPPER_HOME");
    }
}
