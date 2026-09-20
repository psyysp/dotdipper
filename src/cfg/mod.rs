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

    // Sanitized public mirror (`dotdipper publish`)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public: Option<PublicConfig>,

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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
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
    /// Separate repository for the sanitized public copy. Visibility is a
    /// per-repository property, so the public copy cannot be a branch of the
    /// private repo — it needs its own repo with its own history.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_repo_name: Option<String>,
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

    /// Promote scanned apps that map to Homebrew casks or known MAS ids into the Brewfile.
    #[serde(default = "default_true")]
    pub promote_unmanaged: bool,
}

impl Default for AppsConfig {
    fn default() -> Self {
        AppsConfig {
            capture_on_push: true,
            scan_applications: true,
            promote_unmanaged: true,
        }
    }
}

/// Settings for the sanitized public mirror produced by `dotdipper publish`.
///
/// The private store stays the source of truth. Publish derives a separate
/// tree from it: files matching `exclude` are withheld entirely, the rest are
/// rewritten by the built-in redactors plus any `redact` rules. A secret and
/// PII scan then runs over the result and aborts the publish on any hit, so
/// a gap in the rules fails closed instead of leaking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicConfig {
    /// Glob patterns, relative to the store root, never published.
    #[serde(default = "default_public_exclude")]
    pub exclude: Vec<String>,

    /// Apply the built-in redactors (identity, hostnames, home paths, tailnet
    /// names, email addresses). Turning this off is rarely right.
    #[serde(default = "default_true")]
    pub builtin_redactors: bool,

    /// Extra project-specific redaction rules, applied after the built-ins.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub redact: Vec<RedactRule>,

    /// Scanner finding ids that have been reviewed and accepted. Each entry
    /// suppresses exactly one finding; anything else still aborts the publish.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allow: Vec<String>,

    /// Synthesise an install script from the captured `Brewfile` and
    /// `apps_manifest.toml` and publish that instead of the files
    /// themselves. The script installs the same tools without reproducing
    /// the machine name, capture time, installed versions, or applications
    /// that were installed by hand and cannot be installed from a script.
    #[serde(default = "default_true")]
    pub apps_script: bool,

    /// Where the generated script lands in the public tree.
    #[serde(default = "default_apps_script_path")]
    pub apps_script_path: String,

    /// Allowlist file, relative to the dotdipper base dir. It records every
    /// path approved for publication and the redactions applied to each, and
    /// `dotdipper publish --review` generates it. A path missing from it is
    /// withheld, so a newly captured file cannot publish itself unnoticed.
    #[serde(default = "default_allowlist_path")]
    pub allowlist: String,
}

fn default_allowlist_path() -> String {
    "public-allowlist.toml".to_string()
}

fn default_apps_script_path() -> String {
    "install-apps.sh".to_string()
}

impl Default for PublicConfig {
    fn default() -> Self {
        PublicConfig {
            exclude: default_public_exclude(),
            builtin_redactors: true,
            redact: Vec::new(),
            allow: Vec::new(),
            apps_script: true,
            apps_script_path: default_apps_script_path(),
            allowlist: default_allowlist_path(),
        }
    }
}

/// A user-supplied redaction: within files matching `path`, every match of
/// `pattern` becomes `replacement`. Capture groups are available as `${1}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedactRule {
    /// Short identifier, shown in publish output.
    pub name: String,
    /// Glob matched against the store-relative path. Omit for every file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Regular expression to replace.
    pub pattern: String,
    /// Replacement text.
    pub replacement: String,
}

/// Withheld by default: the SSH config maps private network topology, and the
/// manifest is an index of the private file set, including the names of files
/// that were deliberately excluded.
fn default_public_exclude() -> Vec<String> {
    vec![
        ".ssh/**".to_string(),
        "manifest.lock".to_string(),
        ".gitignore".to_string(),
        // The allowlist's [[withheld]] section is a complete index of every
        // private path and why each was held back. `manifest.lock` is
        // excluded for exactly that reason; this file says the same thing
        // more legibly. Both spellings, since the store layout depends on
        // DOTDIPPER_HOME / XDG_CONFIG_HOME.
        "**/public-allowlist.toml".to_string(),
        "public-allowlist.toml".to_string(),
    ]
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
            public: None,
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
            public_repo_name: None,
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

/// Overlay path for the profile that is active right now.
pub fn active_overlay_path() -> Result<PathBuf> {
    let profile =
        crate::profiles::resolve_active_profile_name().unwrap_or_else(|_| "default".into());
    overlay_path_for(&profile)
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

/// One base-config key that the active profile overlay overrides with a
/// different value. Editing the base file for such a key has no effect —
/// the overlay wins on load — so every write and edit path reports these.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShadowedKey {
    /// Dotted path, e.g. `general.tracked_files`.
    pub key: String,
    /// Human-readable base value ("49 entries" for arrays).
    pub base: String,
    /// Human-readable overlay value.
    pub overlay: String,
}

/// Keys the active profile overlay overrides with a *different* value.
///
/// A key the overlay defines identically, or one the base never defines, is
/// not shadowed: editing the base there is either a no-op by agreement or
/// simply unrelated. Only a genuine disagreement misleads the editor.
pub fn shadowed_keys(config_path: &Path) -> Result<Vec<ShadowedKey>> {
    let profile =
        crate::profiles::resolve_active_profile_name().unwrap_or_else(|_| "default".into());
    let overlay_path = overlay_path_for(&profile)?;
    if overlay_path == config_path {
        return Ok(Vec::new());
    }
    let Some(overlay) = parse_overlay_file(&overlay_path)? else {
        return Ok(Vec::new());
    };
    let base_contents = match fs::read_to_string(config_path) {
        Ok(contents) => contents,
        Err(_) => return Ok(Vec::new()),
    };
    let base: toml::Value = match toml::from_str(&base_contents) {
        Ok(value) => value,
        Err(_) => return Ok(Vec::new()),
    };

    let mut found = Vec::new();
    collect_shadowed(&base, &overlay, &mut Vec::new(), &mut found);
    found.sort_by(|a, b| a.key.cmp(&b.key));
    Ok(found)
}

fn collect_shadowed(
    base: &toml::Value,
    overlay: &toml::Value,
    prefix: &mut Vec<String>,
    out: &mut Vec<ShadowedKey>,
) {
    let (Some(base_table), Some(overlay_table)) = (base.as_table(), overlay.as_table()) else {
        return;
    };
    for (key, overlay_val) in overlay_table {
        let Some(base_val) = base_table.get(key) else {
            continue;
        };
        prefix.push(key.clone());
        if base_val.is_table() && overlay_val.is_table() {
            collect_shadowed(base_val, overlay_val, prefix, out);
        } else if base_val != overlay_val {
            out.push(ShadowedKey {
                key: prefix.join("."),
                base: describe_value(base_val),
                overlay: describe_value(overlay_val),
            });
        }
        prefix.pop();
    }
}

fn describe_value(value: &toml::Value) -> String {
    match value {
        toml::Value::Array(items) => format!("{} entries", items.len()),
        toml::Value::String(s) => format!("\"{s}\""),
        other => other.to_string().trim().to_string(),
    }
}

/// Which file a `config --set` write must land in for `key` to take effect:
/// the overlay when it already defines that key, otherwise the base config.
pub fn write_target_for(config_path: &Path, key: &str) -> Result<PathBuf> {
    let profile =
        crate::profiles::resolve_active_profile_name().unwrap_or_else(|_| "default".into());
    let overlay_path = overlay_path_for(&profile)?;
    if overlay_path == config_path {
        return Ok(config_path.to_path_buf());
    }
    let Some(overlay) = parse_overlay_file(&overlay_path)? else {
        return Ok(config_path.to_path_buf());
    };
    if overlay_defines(&overlay, key) {
        Ok(overlay_path)
    } else {
        Ok(config_path.to_path_buf())
    }
}

fn overlay_defines(overlay: &toml::Value, key: &str) -> bool {
    let mut cursor = overlay;
    for segment in key.split('.') {
        let Some(table) = cursor.as_table() else {
            return false;
        };
        let Some(next) = table.get(segment) else {
            return false;
        };
        cursor = next;
    }
    true
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

/// True when a profile other than the active one has no overlay value for `path`
/// and would therefore fall back to the base config's copy.
fn base_key_is_inherited_elsewhere(config_path: &Path, path: &[&str]) -> Result<bool> {
    let active = crate::profiles::resolve_active_profile_name().unwrap_or_else(|_| "default".into());
    let profiles_dir = match config_path.parent() {
        Some(parent) => parent.join("profiles"),
        None => return Ok(false),
    };
    let entries = match fs::read_dir(&profiles_dir) {
        Ok(entries) => entries,
        Err(_) => return Ok(false),
    };

    let dotted = path.join(".");
    for entry in entries.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == active {
            continue;
        }
        let overlay_path = entry.path().join("config.toml");
        match parse_overlay_file(&overlay_path)? {
            Some(overlay) if overlay_defines(&overlay, &dotted) => continue,
            // No overlay, or an overlay silent on this key: it inherits the base.
            _ => return Ok(true),
        }
    }
    Ok(false)
}

/// Remove a key from the base config once the overlay has taken ownership of it.
///
/// Leaving a stale copy behind is the shadowing trap: the base file still reads
/// as authoritative, but the overlay wins on load, so hand-edits there vanish.
/// One writable owner per key is the invariant.
///
/// The base copy is kept when another profile would still inherit it — removing
/// it there would not heal a shadowed key, it would delete a live value out from
/// under a profile that never asked.
fn retire_base_key(config_path: &Path, path: &[&str]) -> Result<()> {
    if !config_path.exists() {
        return Ok(());
    }
    if base_key_is_inherited_elsewhere(config_path, path)? {
        return Ok(());
    }
    let contents = fs::read_to_string(config_path)
        .with_context(|| format!("Failed to read {}", config_path.display()))?;
    let Ok(parsed) = toml::from_str::<toml::Value>(&contents) else {
        return Ok(());
    };
    let Some(mut root) = parsed.as_table().cloned() else {
        return Ok(());
    };

    let Some((leaf, tables)) = path.split_last() else {
        return Ok(());
    };
    let mut cursor = &mut root;
    for segment in tables {
        match cursor.get_mut(*segment).and_then(|v| v.as_table_mut()) {
            Some(next) => cursor = next,
            None => return Ok(()),
        }
    }
    if cursor.remove(*leaf).is_none() {
        return Ok(());
    }

    let serialized =
        toml::to_string_pretty(&toml::Value::Table(root)).context("Failed to serialize config")?;
    atomic_write(config_path, serialized)
        .with_context(|| format!("Failed to write {}", config_path.display()))
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
                write_overlay_table(&overlay_path, table)?;
                return retire_base_key(config_path, &["general", "tracked_files"]);
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
                write_overlay_table(&overlay_path, table)?;
                return retire_base_key(config_path, &["packages", "common"]);
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

/// Strips a `~/` prefix so a pattern anchors to the compiled store root, which
/// mirrors $HOME. Patterns without the prefix are passed through untouched.
fn push_ignore_pattern(pattern: &str) -> String {
    pattern
        .strip_prefix("~/")
        .map(|rest| rest.to_string())
        .unwrap_or_else(|| pattern.to_string())
}

/// Reads `.dotdipperignore` and returns its patterns in push-ignore form.
///
/// `.dotdipperignore` used to gate discovery only, so a pattern written there
/// never stopped an already-tracked file from being pushed. Feeding it here
/// makes one ignore list govern both discovery and push.
fn ignore_file_patterns() -> Result<Vec<String>> {
    let path = crate::paths::ignore_file()?;
    if !path.exists() {
        return Ok(Vec::new());
    }

    let contents = std::fs::read_to_string(&path)
        .context("Failed to read .dotdipperignore for push-ignore")?;

    Ok(contents
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        // Negations are dropped on purpose. A `!pattern` only means anything
        // relative to the positive pattern it re-includes, and the caller
        // merges three sources then sorts, so that order cannot survive.
        // Emitting them anyway would silently re-include ignored files.
        .filter(|line| !line.starts_with('!'))
        .map(push_ignore_pattern)
        .collect())
}

/// Returns relative paths (relative to $HOME) that should be excluded from git push.
/// Combines `.dotdipperignore`, top-level `push_ignore` patterns, and per-file
/// `local_only` entries.
pub fn resolve_push_ignored_paths(config: &Config) -> Result<Vec<String>> {
    let home = dirs::home_dir().context("Failed to find home directory")?;
    let mut ignored = ignore_file_patterns().unwrap_or_default();

    for pattern in &config.push_ignore {
        ignored.push(push_ignore_pattern(pattern));
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

/// Set one config key, writing to whichever file actually governs it.
///
/// A key the active profile overlay already defines is written to the overlay:
/// writing it to the base config would be silently overridden on the next
/// load. Returns the file written so callers can report it.
pub fn set_config_value(config_path: &Path, key: &str, value: &str) -> Result<PathBuf> {
    let parsed = parse_config_value(key, value)?;
    let target = write_target_for(config_path, key)?;
    set_in_file(&target, key, parsed)?;
    Ok(target)
}

/// Validate a `key=value` pair and render the value as TOML.
fn parse_config_value(key: &str, value: &str) -> Result<toml::Value> {
    let parsed = match key {
        "github.username" | "github.repo_name" => toml::Value::String(value.to_string()),
        "github.branch" => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                anyhow::bail!("github.branch cannot be empty");
            }
            toml::Value::String(trimmed.to_string())
        }
        "github.private" => toml::Value::Boolean(
            value
                .parse()
                .context("Invalid boolean value. Use 'true' or 'false'")?,
        ),
        "general.default_mode" => match value {
            "symlink" | "copy" => toml::Value::String(value.to_string()),
            _ => anyhow::bail!("Invalid mode '{}'. Use 'symlink' or 'copy'", value),
        },
        "general.backup" => toml::Value::Boolean(
            value
                .parse()
                .context("Invalid boolean value. Use 'true' or 'false'")?,
        ),
        _ => anyhow::bail!(
            "Unknown config key '{}'. Supported keys:\n  \
             github.username, github.repo_name, github.branch, github.private,\n  \
             general.default_mode, general.backup",
            key
        ),
    };
    Ok(parsed)
}

/// Insert a dotted key into a TOML file, creating intermediate tables and
/// leaving every other key — including ones this binary does not model — intact.
fn set_in_file(path: &Path, key: &str, value: toml::Value) -> Result<()> {
    let mut root = if path.exists() {
        let contents = fs::read_to_string(path)
            .with_context(|| format!("Failed to read {}", path.display()))?;
        if overlay_is_blank(&contents) {
            toml::map::Map::new()
        } else {
            toml::from_str::<toml::Value>(&contents)
                .with_context(|| format!("Failed to parse {}", path.display()))?
                .as_table()
                .cloned()
                .unwrap_or_default()
        }
    } else {
        toml::map::Map::new()
    };

    let segments: Vec<&str> = key.split('.').collect();
    let (leaf, tables) = segments.split_last().context("Empty config key")?;
    let mut cursor = &mut root;
    for segment in tables {
        let entry = cursor
            .entry(segment.to_string())
            .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
        if !entry.is_table() {
            *entry = toml::Value::Table(toml::map::Map::new());
        }
        cursor = entry.as_table_mut().expect("table");
    }
    cursor.insert(leaf.to_string(), value);

    let serialized =
        toml::to_string_pretty(&toml::Value::Table(root)).context("Failed to serialize config")?;
    atomic_write(path, serialized).with_context(|| format!("Failed to write {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use tempfile::TempDir;

    /// Sets up HOME/DOTDIPPER_HOME with a base config and a profile overlay.
    fn overlay_fixture(base_body: &str, overlay_body: &str) -> (TempDir, PathBuf) {
        let temp = TempDir::new().unwrap();
        let home = temp.path();
        let base = home.join(".config").join("dotdipper");
        std::fs::create_dir_all(base.join("profiles").join("default")).unwrap();

        std::env::set_var("HOME", home);
        std::env::set_var("DOTDIPPER_HOME", &base);
        std::env::remove_var("DOTDIPPER_PROFILE");
        std::env::remove_var("XDG_CONFIG_HOME");

        let config_path = base.join("config.toml");
        std::fs::write(&config_path, base_body).unwrap();
        std::fs::write(
            base.join("profiles").join("default").join("config.toml"),
            overlay_body,
        )
        .unwrap();
        (temp, config_path)
    }

    #[test]
    #[serial]
    fn update_discovered_retires_the_stale_base_copy() {
        let (_temp, config_path) = overlay_fixture(
            "[general]\nactive_profile = \"default\"\nbackup = true\ntracked_files = [\"/stale\"]\n",
            "# comments only\n",
        );

        update_discovered(&config_path, &[PathBuf::from("/a"), PathBuf::from("/b")]).unwrap();

        let base_text = std::fs::read_to_string(&config_path).unwrap();
        assert!(!base_text.contains("/stale"), "{base_text}");
        assert!(!base_text.contains("tracked_files"), "{base_text}");
        // Unrelated base keys survive, and nothing shadows anything any more.
        assert!(base_text.contains("backup = true"), "{base_text}");
        assert!(shadowed_keys(&config_path).unwrap().is_empty());
        assert_eq!(load(&config_path).unwrap().general.tracked_files.len(), 2);
    }

    #[test]
    #[serial]
    fn update_discovered_keeps_the_base_copy_another_profile_inherits() {
        let (temp, config_path) = overlay_fixture(
            "[general]\nactive_profile = \"default\"\ntracked_files = [\"/shared\"]\n",
            "# comments only\n",
        );
        // A second profile with no overlay value of its own still inherits the base.
        std::fs::create_dir_all(
            temp.path()
                .join(".config")
                .join("dotdipper")
                .join("profiles")
                .join("work"),
        )
        .unwrap();

        update_discovered(&config_path, &[PathBuf::from("/a")]).unwrap();

        let base_text = std::fs::read_to_string(&config_path).unwrap();
        assert!(
            base_text.contains("/shared"),
            "base value another profile inherits must survive: {base_text}"
        );
        // The active profile still gets its own value.
        assert_eq!(load(&config_path).unwrap().general.tracked_files.len(), 1);
    }

    #[test]
    #[serial]
    fn shadowed_keys_reports_a_disagreeing_overlay() {
        let (_temp, config_path) = overlay_fixture(
            "[general]\nactive_profile = \"default\"\ntracked_files = [\"/a\", \"/b\"]\n",
            "[general]\ntracked_files = [\"/a\", \"/b\", \"/c\"]\n",
        );

        let shadowed = shadowed_keys(&config_path).unwrap();

        assert_eq!(shadowed.len(), 1, "{shadowed:?}");
        assert_eq!(shadowed[0].key, "general.tracked_files");
        assert_eq!(shadowed[0].base, "2 entries");
        assert_eq!(shadowed[0].overlay, "3 entries");
    }

    #[test]
    #[serial]
    fn shadowed_keys_ignores_agreement_and_base_only_keys() {
        let (_temp, config_path) = overlay_fixture(
            "[general]\nactive_profile = \"default\"\nbackup = true\n\n[github]\nrepo_name = \"dots\"\n",
            "[general]\nbackup = true\n",
        );

        assert!(shadowed_keys(&config_path).unwrap().is_empty());
    }

    #[test]
    #[serial]
    fn set_config_value_writes_the_overlay_when_the_overlay_owns_the_key() {
        let (_temp, config_path) = overlay_fixture(
            "[general]\nactive_profile = \"default\"\n\n[github]\nrepo_name = \"base-repo\"\n",
            "[github]\nrepo_name = \"overlay-repo\"\n",
        );

        let written = set_config_value(&config_path, "github.repo_name", "changed").unwrap();

        assert_eq!(written, active_overlay_path().unwrap());
        // The base file is untouched, and the merged view reflects the write.
        let base_text = std::fs::read_to_string(&config_path).unwrap();
        assert!(base_text.contains("base-repo"), "{base_text}");
        assert_eq!(
            load(&config_path).unwrap().github.repo_name.as_deref(),
            Some("changed")
        );
    }

    #[test]
    #[serial]
    fn set_config_value_writes_the_base_when_no_overlay_owns_the_key() {
        let (_temp, config_path) = overlay_fixture(
            "[general]\nactive_profile = \"default\"\n\n[github]\nrepo_name = \"base-repo\"\n",
            "[general]\ntracked_files = [\"/a\"]\n",
        );

        let written = set_config_value(&config_path, "github.repo_name", "changed").unwrap();

        assert_eq!(written, config_path);
        assert_eq!(
            load(&config_path).unwrap().github.repo_name.as_deref(),
            Some("changed")
        );
    }

    #[test]
    #[serial]
    fn set_config_value_preserves_keys_it_does_not_model() {
        let (_temp, config_path) = overlay_fixture(
            "[general]\nactive_profile = \"default\"\n\n[hooks]\npost_apply = [\"echo hi\"]\n",
            "# comments only\n",
        );

        set_config_value(&config_path, "github.private", "false").unwrap();

        let text = std::fs::read_to_string(&config_path).unwrap();
        assert!(text.contains("echo hi"), "{text}");
        assert!(text.contains("active_profile"), "{text}");
        assert!(!load(&config_path).unwrap().github.private);
    }

    #[test]
    #[serial]
    fn set_config_value_rejects_an_unknown_key_without_writing() {
        let (_temp, config_path) = overlay_fixture(
            "[general]\nactive_profile = \"default\"\n",
            "# comments only\n",
        );
        let before = std::fs::read_to_string(&config_path).unwrap();

        assert!(set_config_value(&config_path, "github.nope", "x").is_err());
        assert_eq!(std::fs::read_to_string(&config_path).unwrap(), before);
    }

    #[test]
    #[serial]
    fn push_ignore_includes_dotdipperignore_patterns() {
        let temp = TempDir::new().unwrap();
        let home = temp.path();
        let base = home.join(".config").join("dotdipper");
        std::fs::create_dir_all(&base).unwrap();
        std::fs::write(
            base.join(".dotdipperignore"),
            "# comment\n\n~/.config/gcloud/**\n**/backup-*\n!**/backup-keep\n",
        )
        .unwrap();

        std::env::set_var("HOME", home);
        std::env::set_var("DOTDIPPER_HOME", &base);
        std::env::remove_var("DOTDIPPER_PROFILE");
        std::env::remove_var("XDG_CONFIG_HOME");

        let config = Config {
            push_ignore: vec!["~/.config/stripe/**".to_string()],
            ..Default::default()
        };

        let resolved = resolve_push_ignored_paths(&config).unwrap();

        // `.dotdipperignore` used to gate discovery only; it must now also
        // reach the generated .gitignore, alongside explicit push_ignore.
        assert!(resolved.contains(&".config/gcloud/**".to_string()));
        assert!(resolved.contains(&"**/backup-*".to_string()));
        assert!(resolved.contains(&".config/stripe/**".to_string()));
        // Comments and blank lines are dropped.
        assert!(!resolved.iter().any(|p| p.starts_with('#') || p.is_empty()));
        // Negations are dropped: the merged list is sorted, so a `!` pattern
        // would land before the rule it means to undo and silently re-include
        // an ignored file.
        assert!(!resolved.iter().any(|p| p.starts_with('!')));
    }

    #[test]
    #[serial]
    fn push_ignore_survives_missing_dotdipperignore() {
        let temp = TempDir::new().unwrap();
        let home = temp.path();
        let base = home.join(".config").join("dotdipper");
        std::fs::create_dir_all(&base).unwrap();

        std::env::set_var("HOME", home);
        std::env::set_var("DOTDIPPER_HOME", &base);
        std::env::remove_var("DOTDIPPER_PROFILE");
        std::env::remove_var("XDG_CONFIG_HOME");

        let config = Config {
            push_ignore: vec!["~/.aws/**".to_string()],
            ..Default::default()
        };

        let resolved = resolve_push_ignored_paths(&config).unwrap();
        assert_eq!(resolved, vec![".aws/**".to_string()]);
    }

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
