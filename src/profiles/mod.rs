//! Multiple profiles: work / personal / server, etc.
//!
//! The active profile selects the live store used by snapshot, apply, push, pull,
//! status, diff, install, and remote bundle ops:
//!
//! ```text
//! ~/.config/dotdipper/profiles/<name>/compiled/
//! ~/.config/dotdipper/profiles/<name>/manifest.lock
//! ~/.config/dotdipper/profiles/<name>/snapshots/
//! ~/.config/dotdipper/profiles/<name>/config.toml   # sparse overlay
//! ```
//!
//! Global `config.toml` is the base. The profile overlay is merged on load
//! (overlay keys win). GitHub push defaults to branch `main` for `default`
//! and `dotdipper/<name>` for other profiles; set `[github].repo_name` in the
//! overlay for a dedicated repository.
//!
//! Legacy `~/.config/dotdipper/compiled/` is migrated into `profiles/default/` on first use.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::cfg::Config;
use crate::ui;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    pub config_path: PathBuf,
    pub manifest_path: PathBuf,
    pub compiled_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct ProfilePaths {
    pub compiled: PathBuf,
    pub manifest: PathBuf,
    pub snapshots: PathBuf,
    pub root: PathBuf,
}

/// Ensure the profile store is ready (migrate legacy layout, ensure active profile dirs).
pub fn ensure_store_ready() -> Result<()> {
    migrate_legacy_if_needed()?;
    let name = resolve_active_profile_name()?;
    ensure_exists(&name)?;
    // Keep top-level compiled/manifest/snapshots as symlinks into the active profile
    // so older scripts and tests that use ~/.config/dotdipper/compiled still work.
    refresh_compat_links(&name)?;
    Ok(())
}

/// Paths for the currently active profile store.
pub fn active_store_paths() -> Result<ProfilePaths> {
    ensure_store_ready()?;
    let name = resolve_active_profile_name()?;
    profile_paths(&name)
}

/// Resolve active profile: `DOTDIPPER_PROFILE` env overrides config.
pub fn resolve_active_profile_name() -> Result<String> {
    if let Ok(name) = std::env::var("DOTDIPPER_PROFILE") {
        let name = name.trim();
        if !name.is_empty() {
            validate_profile_name(name)?;
            return Ok(name.to_string());
        }
    }
    let name = active_profile_name()?;
    validate_profile_name(&name)?;
    Ok(name)
}

/// List all profiles
pub fn list(_config: &Config) -> Result<Vec<Profile>> {
    ensure_store_ready()?;
    let profiles_dir = get_dotdipper_dir()?.join("profiles");
    fs::create_dir_all(&profiles_dir)?;

    let mut profiles = Vec::new();

    for entry in fs::read_dir(&profiles_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string();

            let profile = Profile {
                name: name.clone(),
                config_path: path.join("config.toml"),
                manifest_path: path.join("manifest.lock"),
                compiled_path: path.join("compiled"),
            };

            profiles.push(profile);
        }
    }

    profiles.sort_by(|a, b| a.name.cmp(&b.name));

    let active = resolve_active_profile_name()?;
    ui::section(&format!("Found {} profiles:", profiles.len()));
    for prof in &profiles {
        let marker = if prof.name == active { " (active)" } else { "" };
        println!("  {}{}", prof.name, marker);
    }

    Ok(profiles)
}

/// Validate a profile name (reject path traversal / empty).
pub fn validate_profile_name(name: &str) -> Result<()> {
    let name = name.trim();
    if name.is_empty()
        || name.contains('/')
        || name.contains('\\')
        || name.contains('\0')
        || name == "."
        || name == ".."
    {
        bail!("Invalid profile name: {:?}", name);
    }
    if name.len() > 64 {
        bail!("Profile name too long (max 64 characters)");
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        bail!(
            "Invalid profile name '{}': use only letters, numbers, '.', '-', '_'",
            name
        );
    }
    if name.starts_with('.') || name.ends_with('.') {
        bail!(
            "Invalid profile name '{}': cannot start or end with '.'",
            name
        );
    }
    Ok(())
}

/// Create a new profile
pub fn create(_config: &Config, name: &str) -> Result<Profile> {
    validate_profile_name(name)?;

    let profile_dir = get_dotdipper_dir()?.join("profiles").join(name);

    if profile_dir.exists() {
        bail!("Profile '{}' already exists", name);
    }

    ui::info(&format!("Creating profile: {}", name));

    fs::create_dir_all(profile_dir.join("compiled"))?;
    fs::create_dir_all(profile_dir.join("snapshots"))?;

    let config_path = profile_dir.join("config.toml");
    crate::cfg::write_sparse_overlay_if_missing(&config_path)?;

    ui::success(&format!("Profile '{}' created", name));
    ui::hint(&format!(
        "Switch to it with: dotdipper profile switch {}",
        name
    ));
    ui::hint(&format!(
        "Optional overlay: profiles/{}/config.toml (github.repo_name / branch / tracked_files)",
        name
    ));

    Ok(Profile {
        name: name.to_string(),
        config_path,
        manifest_path: profile_dir.join("manifest.lock"),
        compiled_path: profile_dir.join("compiled"),
    })
}

/// Switch to a different profile
pub fn switch(_config: &Config, name: &str) -> Result<()> {
    validate_profile_name(name)?;
    ensure_store_ready()?;
    let profile_dir = get_dotdipper_dir()?.join("profiles").join(name);

    if !profile_dir.exists() {
        bail!(
            "Profile '{}' does not exist. Create it first with 'dotdipper profile create {}'",
            name,
            name
        );
    }

    let main_config_path = get_dotdipper_dir()?.join("config.toml");
    let mut config = if main_config_path.exists() {
        crate::cfg::load_file(&main_config_path)?
    } else {
        Config::default()
    };

    config.general.active_profile = Some(name.to_string());
    crate::cfg::save(&main_config_path, &config)?;
    refresh_compat_links(name)?;

    ui::success(&format!("Switched to profile: {}", name));
    ui::hint("snapshot / apply / push / pull now use this profile's compiled store");
    ui::hint(&format!(
        "compat links: compiled/ → profiles/{}/compiled",
        name
    ));
    if name != "default" {
        ui::hint(&format!(
            "GitHub push defaults to branch dotdipper/{}",
            name
        ));
    }

    Ok(())
}

/// Remove a profile
pub fn remove(_config: &Config, name: &str, force: bool) -> Result<()> {
    if name == "default" {
        bail!("Cannot remove the default profile");
    }

    let profile_dir = get_dotdipper_dir()?.join("profiles").join(name);

    if !profile_dir.exists() {
        bail!("Profile '{}' does not exist", name);
    }

    if !force {
        let proceed = dialoguer::Confirm::new()
            .with_prompt(format!(
                "Delete profile '{}'? This will remove all profile data",
                name
            ))
            .default(false)
            .interact()?;

        if !proceed {
            ui::info("Deletion cancelled");
            return Ok(());
        }
    }

    let active = resolve_active_profile_name()?;
    if active == name {
        ui::warn("Cannot delete active profile. Switch to another profile first.");
        bail!("Active profile cannot be deleted");
    }

    fs::remove_dir_all(&profile_dir)?;
    ui::success(&format!("Profile '{}' removed", name));

    Ok(())
}

/// Get the currently active profile name from config (no env override).
pub fn active_profile_name() -> Result<String> {
    let main_config_path = get_dotdipper_dir()?.join("config.toml");

    if main_config_path.exists() {
        let config = crate::cfg::load_file(&main_config_path)?;
        if let Some(profile) = config.general.active_profile {
            if !profile.trim().is_empty() {
                return Ok(profile);
            }
        }
    }

    Ok("default".to_string())
}

/// Ensure a profile exists, create if not.
/// Only auto-creates `default`; other names must be created explicitly
/// (avoids `DOTDIPPER_PROFILE` typos silently creating empty stores).
pub fn ensure_exists(name: &str) -> Result<()> {
    validate_profile_name(name)?;
    let profile_dir = get_dotdipper_dir()?.join("profiles").join(name);

    if !profile_dir.exists() {
        if name != "default" {
            bail!(
                "Profile '{}' does not exist. Create it first with 'dotdipper profile create {}'",
                name,
                name
            );
        }
        fs::create_dir_all(profile_dir.join("compiled"))?;
        fs::create_dir_all(profile_dir.join("snapshots"))?;
        crate::cfg::write_sparse_overlay_if_missing(&profile_dir.join("config.toml"))?;
    } else {
        fs::create_dir_all(profile_dir.join("compiled"))?;
        fs::create_dir_all(profile_dir.join("snapshots"))?;
    }

    Ok(())
}

/// Get paths for a profile
pub fn profile_paths(name: &str) -> Result<ProfilePaths> {
    validate_profile_name(name)?;
    ensure_exists(name)?;
    let profile_dir = get_dotdipper_dir()?.join("profiles").join(name);

    Ok(ProfilePaths {
        compiled: profile_dir.join("compiled"),
        manifest: profile_dir.join("manifest.lock"),
        snapshots: profile_dir.join("snapshots"),
        root: profile_dir,
    })
}

fn get_dotdipper_dir() -> Result<PathBuf> {
    crate::paths::base_dir()
}

/// Migrate legacy `{base}/compiled` + `manifest.lock` (+ snapshots) into `profiles/default/`.
pub fn migrate_legacy_if_needed() -> Result<()> {
    let base = get_dotdipper_dir()?;
    let legacy_compiled = base.join("compiled");
    let legacy_manifest = base.join("manifest.lock");
    let legacy_snapshots = base.join("snapshots");
    let default_root = base.join("profiles").join("default");
    let default_compiled = default_root.join("compiled");
    let default_manifest = default_root.join("manifest.lock");
    let default_snapshots = default_root.join("snapshots");

    let legacy_has_data = dir_has_store_data(&legacy_compiled);
    let default_has_data = dir_has_store_data(&default_compiled);

    if legacy_has_data && !default_has_data {
        ui::info("Migrating legacy compiled/ into profiles/default/...");
        fs::create_dir_all(&default_root)?;

        if default_compiled.exists() {
            // Remove empty scaffold so rename/copy can proceed
            let _ = fs::remove_dir_all(&default_compiled);
        }

        if let Err(e) = fs::rename(&legacy_compiled, &default_compiled) {
            // Cross-device or busy: copy then remove
            ui::warn(&format!(
                "Could not rename legacy compiled/ ({e}); copying instead..."
            ));
            copy_dir_recursive(&legacy_compiled, &default_compiled)?;
            fs::remove_dir_all(&legacy_compiled)?;
        }

        ui::success("Migrated compiled store to profiles/default/compiled");
    }

    if is_plain_file(&legacy_manifest) && !default_manifest.exists() {
        fs::create_dir_all(&default_root)?;
        if fs::rename(&legacy_manifest, &default_manifest).is_err() {
            fs::copy(&legacy_manifest, &default_manifest)?;
            let _ = fs::remove_file(&legacy_manifest);
        }
    }

    if dir_has_store_data(&legacy_snapshots) && !dir_has_store_data(&default_snapshots) {
        fs::create_dir_all(&default_root)?;
        if default_snapshots.exists() {
            let _ = fs::remove_dir_all(&default_snapshots);
        }
        if fs::rename(&legacy_snapshots, &default_snapshots).is_err() {
            copy_dir_recursive(&legacy_snapshots, &default_snapshots)?;
            let _ = fs::remove_dir_all(&legacy_snapshots);
        }
    }

    // Ensure default profile scaffold exists even on fresh installs
    ensure_exists("default")?;

    // Persist active_profile = default when unset
    let main_config_path = base.join("config.toml");
    if main_config_path.exists() {
        let mut config = crate::cfg::load_file(&main_config_path)?;
        if config.general.active_profile.is_none() {
            config.general.active_profile = Some("default".to_string());
            let _ = crate::cfg::save(&main_config_path, &config);
        }
    }

    Ok(())
}

/// Point top-level `compiled` / `manifest.lock` / `snapshots` at a profile store.
pub fn refresh_compat_links(profile_name: &str) -> Result<()> {
    let paths = profile_paths(profile_name)?;
    let base = get_dotdipper_dir()?;

    replace_with_symlink(&base.join("compiled"), &paths.compiled)?;
    replace_with_symlink(&base.join("manifest.lock"), &paths.manifest)?;
    replace_with_symlink(&base.join("snapshots"), &paths.snapshots)?;
    Ok(())
}

fn is_plain_file(path: &Path) -> bool {
    path.symlink_metadata()
        .map(|m| m.is_file() && !m.file_type().is_symlink())
        .unwrap_or(false)
}

fn replace_with_symlink(link: &Path, target: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        if let Ok(meta) = link.symlink_metadata() {
            if meta.file_type().is_symlink() {
                if let Ok(current) = fs::read_link(link) {
                    if current == target {
                        return Ok(());
                    }
                }
                fs::remove_file(link)?;
            } else if meta.is_dir() {
                if dir_has_store_data(link) {
                    // Unmigrated data — leave alone; migrate_legacy_if_needed handles this
                    return Ok(());
                }
                fs::remove_dir_all(link)?;
            } else if meta.is_file() {
                // Plain file (e.g. leftover manifest) — remove so we can symlink
                fs::remove_file(link)?;
            }
        }

        if let Some(parent) = link.parent() {
            fs::create_dir_all(parent)?;
        }
        std::os::unix::fs::symlink(target, link)?;
    }

    #[cfg(not(unix))]
    {
        let _ = (link, target);
    }

    Ok(())
}

fn dir_has_store_data(dir: &Path) -> bool {
    if !dir.exists() {
        return false;
    }
    // Symlink to profile store: treat as already migrated if it points into profiles/
    if dir
        .symlink_metadata()
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
    {
        return false;
    }
    walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .any(|e| {
            if !e.file_type().is_file() {
                return false;
            }
            let Ok(rel) = e.path().strip_prefix(dir) else {
                return false;
            };
            let s = rel.to_string_lossy();
            // Any real content counts (including .git — that's the push history)
            !s.is_empty()
        })
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    for entry in walkdir::WalkDir::new(src)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let rel = entry.path().strip_prefix(src)?;
        let target = dst.join(rel);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&target)?;
        } else if entry.file_type().is_file() {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use tempfile::TempDir;

    #[test]
    fn test_profile_name_validation() {
        assert!(!"test-profile".contains('/'));
        assert!("../bad".contains('/'));
    }

    #[test]
    #[serial]
    fn migrate_moves_legacy_compiled_into_default_profile() {
        let temp = TempDir::new().unwrap();
        let base = temp.path().join("dotdipper");
        fs::create_dir_all(base.join("compiled")).unwrap();
        fs::write(base.join("compiled").join(".zshrc"), "export Z=1\n").unwrap();
        fs::write(
            base.join("manifest.lock"),
            "{\"version\":\"1.0.0\",\"created\":\"2026-01-01T00:00:00Z\",\"files\":{}}",
        )
        .unwrap();
        fs::write(
            base.join("config.toml"),
            "[general]\nbackup = true\ntracked_files = []\n",
        )
        .unwrap();

        std::env::set_var("DOTDIPPER_HOME", &base);
        std::env::remove_var("DOTDIPPER_PROFILE");
        std::env::remove_var("XDG_CONFIG_HOME");

        migrate_legacy_if_needed().unwrap();

        assert!(base
            .join("profiles")
            .join("default")
            .join("compiled")
            .join(".zshrc")
            .exists());
        assert!(base
            .join("profiles")
            .join("default")
            .join("manifest.lock")
            .exists());

        let store = active_store_paths().unwrap();
        assert_eq!(
            store.compiled,
            base.join("profiles").join("default").join("compiled")
        );

        // Compat symlink at top-level compiled/
        let link = base.join("compiled");
        assert!(link.symlink_metadata().unwrap().file_type().is_symlink());
        assert!(base.join("compiled").join(".zshrc").exists());
    }

    #[test]
    #[serial]
    fn switch_updates_compat_symlink_to_active_profile() {
        let temp = TempDir::new().unwrap();
        let base = temp.path().join("dotdipper");
        fs::create_dir_all(&base).unwrap();
        fs::write(
            base.join("config.toml"),
            "[general]\nactive_profile = \"default\"\ntracked_files = []\n",
        )
        .unwrap();

        std::env::set_var("DOTDIPPER_HOME", &base);
        std::env::remove_var("DOTDIPPER_PROFILE");
        std::env::remove_var("XDG_CONFIG_HOME");

        ensure_store_ready().unwrap();
        create(&Config::default(), "work").unwrap();
        fs::write(
            base.join("profiles")
                .join("work")
                .join("compiled")
                .join("workrc"),
            "work\n",
        )
        .unwrap();

        switch(&Config::default(), "work").unwrap();
        assert!(base.join("compiled").join("workrc").exists());

        switch(&Config::default(), "default").unwrap();
        assert!(!base.join("compiled").join("workrc").exists());
    }

    #[test]
    fn rejects_path_traversal_profile_names() {
        assert!(validate_profile_name("../etc").is_err());
        assert!(validate_profile_name("..").is_err());
        assert!(validate_profile_name("a/b").is_err());
        assert!(validate_profile_name("").is_err());
        assert!(validate_profile_name("good-name").is_ok());
        assert!(validate_profile_name("default").is_ok());
    }

    #[test]
    #[serial]
    fn env_override_selects_profile_store() {
        let temp = TempDir::new().unwrap();
        let base = temp.path().join("dotdipper");
        fs::create_dir_all(&base).unwrap();
        fs::write(
            base.join("config.toml"),
            "[general]\nactive_profile = \"default\"\ntracked_files = []\n",
        )
        .unwrap();

        std::env::set_var("DOTDIPPER_HOME", &base);
        std::env::remove_var("XDG_CONFIG_HOME");

        create(&Config::default(), "work").unwrap();
        std::env::set_var("DOTDIPPER_PROFILE", "work");

        let store = active_store_paths().unwrap();
        assert!(store.compiled.ends_with("profiles/work/compiled"));

        std::env::remove_var("DOTDIPPER_PROFILE");
    }

    #[test]
    #[serial]
    fn phantom_env_profile_does_not_auto_create() {
        let temp = TempDir::new().unwrap();
        let base = temp.path().join("dotdipper");
        fs::create_dir_all(&base).unwrap();
        fs::write(
            base.join("config.toml"),
            "[general]\nactive_profile = \"default\"\ntracked_files = []\n",
        )
        .unwrap();

        std::env::set_var("DOTDIPPER_HOME", &base);
        std::env::set_var("DOTDIPPER_PROFILE", "typo-profile");
        std::env::remove_var("XDG_CONFIG_HOME");

        let err = active_store_paths().unwrap_err().to_string();
        assert!(
            err.contains("does not exist") || err.contains("Invalid"),
            "unexpected error: {err}"
        );

        std::env::remove_var("DOTDIPPER_PROFILE");
    }
}
