use anyhow::{Context, Result};
use std::path::PathBuf;

/// Returns the dotdipper base directory.
///
/// Resolution order:
/// 1. `DOTDIPPER_HOME` environment variable (if set)
/// 2. `$XDG_CONFIG_HOME/dotdipper` (if `XDG_CONFIG_HOME` is set)
/// 3. `~/.config/dotdipper`
pub fn base_dir() -> Result<PathBuf> {
    if let Ok(custom) = std::env::var("DOTDIPPER_HOME") {
        let p = PathBuf::from(custom);
        if p.is_absolute() {
            return Ok(p);
        }
        let home = dirs::home_dir().context("Failed to find home directory")?;
        return Ok(home.join(p));
    }

    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        return Ok(PathBuf::from(xdg).join("dotdipper"));
    }

    let home = dirs::home_dir().context("Failed to find home directory")?;
    Ok(home.join(".config").join("dotdipper"))
}

pub fn config_file() -> Result<PathBuf> {
    Ok(base_dir()?.join("config.toml"))
}

pub fn compiled_dir() -> Result<PathBuf> {
    Ok(base_dir()?.join("compiled"))
}

pub fn manifest_file() -> Result<PathBuf> {
    Ok(base_dir()?.join("manifest.lock"))
}

pub fn snapshots_dir() -> Result<PathBuf> {
    Ok(base_dir()?.join("snapshots"))
}

pub fn cache_dir() -> Result<PathBuf> {
    Ok(base_dir()?.join("cache"))
}

pub fn install_dir() -> Result<PathBuf> {
    Ok(base_dir()?.join("install"))
}

pub fn profiles_dir() -> Result<PathBuf> {
    Ok(base_dir()?.join("profiles"))
}
