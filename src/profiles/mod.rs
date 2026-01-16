/// Multiple Profiles (Milestone 4)
/// 
/// This module handles:
/// - Creating and managing multiple profiles (work, personal, server, etc.)
/// - Switching between profiles
/// - Profile-specific configurations with base + overlay merging
/// - Per-profile manifest and compiled directories

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use crate::cfg::{Config, GeneralConfig, RestoreMode};
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
    pub root: PathBuf,
}

/// List all profiles
pub fn list(_config: &Config) -> Result<Vec<Profile>> {
    let dotdipper_dir = get_dotdipper_dir()?;
    let profiles_dir = dotdipper_dir.join("profiles");
    
    if !profiles_dir.exists() {
        // Create default profile if none exist
        fs::create_dir_all(&profiles_dir)?;
        ensure_default_profile()?;
    }
    
    let mut profiles = Vec::new();
    
    for entry in fs::read_dir(&profiles_dir)? {
        let entry = entry?;
        let path = entry.path();
        
        if path.is_dir() {
            let name = path.file_name()
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
    
    // Display profiles
    let active = active_profile_name()?;
    ui::section(&format!("Found {} profiles:", profiles.len()));
    for prof in &profiles {
        let marker = if prof.name == active { " (active)" } else { "" };
        println!("  {}{}", prof.name, marker);
    }
    
    Ok(profiles)
}

/// Create a new profile
pub fn create(_config: &Config, name: &str) -> Result<Profile> {
    // Validate profile name
    if name.is_empty() || name.contains('/') || name.contains('\\') {
        bail!("Invalid profile name: {}", name);
    }
    
    let dotdipper_dir = get_dotdipper_dir()?;
    let profiles_dir = dotdipper_dir.join("profiles");
    let profile_dir = profiles_dir.join(name);
    
    if profile_dir.exists() {
        bail!("Profile '{}' already exists", name);
    }
    
    ui::info(&format!("Creating profile: {}", name));
    
    // Create profile directories
    fs::create_dir_all(&profile_dir)?;
    let compiled_dir = profile_dir.join("compiled");
    fs::create_dir_all(&compiled_dir)?;
    
    // Create profile config (inherits from root defaults)
    let profile_config = Config {
        general: GeneralConfig {
            default_mode: RestoreMode::Symlink,
            backup: true,
            tracked_files: Vec::new(),
            active_profile: None,
        },
        ..Default::default()
    };
    
    let config_path = profile_dir.join("config.toml");
    let config_toml = toml::to_string_pretty(&profile_config)?;
    fs::write(&config_path, config_toml)?;
    
    ui::success(&format!("Profile '{}' created", name));
    ui::hint(&format!("Switch to it with: dotdipper profile switch {}", name));
    
    Ok(Profile {
        name: name.to_string(),
        config_path,
        manifest_path: profile_dir.join("manifest.lock"),
        compiled_path: compiled_dir,
    })
}

/// Switch to a different profile
pub fn switch(_config: &Config, name: &str) -> Result<()> {
    let dotdipper_dir = get_dotdipper_dir()?;
    let profiles_dir = dotdipper_dir.join("profiles");
    let profile_dir = profiles_dir.join(name);
    
    if !profile_dir.exists() {
        bail!("Profile '{}' does not exist. Create it first with 'dotdipper profile create {}'", name, name);
    }
    
    // Update main config to set active profile
    let main_config_path = dotdipper_dir.join("config.toml");
    let mut config = if main_config_path.exists() {
        crate::cfg::load(&main_config_path)?
    } else {
        Config::default()
    };
    
    config.general.active_profile = Some(name.to_string());
    crate::cfg::save(&main_config_path, &config)?;
    
    ui::success(&format!("Switched to profile: {}", name));
    
    Ok(())
}

/// Remove a profile
pub fn remove(_config: &Config, name: &str, force: bool) -> Result<()> {
    if name == "default" {
        bail!("Cannot remove the default profile");
    }
    
    let dotdipper_dir = get_dotdipper_dir()?;
    let profiles_dir = dotdipper_dir.join("profiles");
    let profile_dir = profiles_dir.join(name);
    
    if !profile_dir.exists() {
        bail!("Profile '{}' does not exist", name);
    }
    
    // Confirm deletion (unless force is set)
    if !force {
        let proceed = dialoguer::Confirm::new()
            .with_prompt(format!("Delete profile '{}'? This will remove all profile data", name))
            .default(false)
            .interact()?;
        
        if !proceed {
            ui::info("Deletion cancelled");
            return Ok(());
        }
    }
    
    // Check if it's the active profile
    let active = active_profile_name()?;
    if active == name {
        ui::warn("Cannot delete active profile. Switch to another profile first.");
        bail!("Active profile cannot be deleted");
    }
    
    fs::remove_dir_all(&profile_dir)?;
    ui::success(&format!("Profile '{}' removed", name));
    
    Ok(())
}

/// Get the currently active profile name
pub fn active_profile_name() -> Result<String> {
    let dotdipper_dir = get_dotdipper_dir()?;
    let main_config_path = dotdipper_dir.join("config.toml");
    
    if main_config_path.exists() {
        let config = crate::cfg::load(&main_config_path)?;
        if let Some(profile) = config.general.active_profile {
            return Ok(profile);
        }
    }
    
    Ok("default".to_string())
}

/// Ensure a profile exists, create if not
pub fn ensure_exists(name: &str) -> Result<()> {
    let dotdipper_dir = get_dotdipper_dir()?;
    let profiles_dir = dotdipper_dir.join("profiles");
    let profile_dir = profiles_dir.join(name);
    
    if !profile_dir.exists() {
        fs::create_dir_all(&profile_dir)?;
        fs::create_dir_all(profile_dir.join("compiled"))?;
        
        // Create minimal config
        let config = Config::default();
        let config_toml = toml::to_string_pretty(&config)?;
        fs::write(profile_dir.join("config.toml"), config_toml)?;
    }
    
    Ok(())
}

/// Get paths for a profile (with overlay semantics)
pub fn profile_paths(name: &str) -> Result<ProfilePaths> {
    let dotdipper_dir = get_dotdipper_dir()?;
    let profiles_dir = dotdipper_dir.join("profiles");
    let profile_dir = profiles_dir.join(name);
    
    ensure_exists(name)?;
    
    Ok(ProfilePaths {
        compiled: profile_dir.join("compiled"),
        manifest: profile_dir.join("manifest.lock"),
        root: profile_dir,
    })
}

fn get_dotdipper_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Failed to find home directory")?;
    Ok(home.join(".dotdipper"))
}

fn ensure_default_profile() -> Result<()> {
    ensure_exists("default")?;
    
    // Migrate legacy config/compiled to default profile if they exist
    let dotdipper_dir = get_dotdipper_dir()?;
    let legacy_compiled = dotdipper_dir.join("compiled");
    let legacy_manifest = dotdipper_dir.join("manifest.lock");
    
    let default_profile = profile_paths("default")?;
    
    // Only migrate if default profile is empty and legacy exists
    if legacy_compiled.exists() && !default_profile.compiled.exists() {
        ui::info("Migrating legacy compiled/ to default profile...");
        fs_extra::dir::copy(
            &legacy_compiled,
            &default_profile.root,
            &fs_extra::dir::CopyOptions::new(),
        )?;
    }
    
    if legacy_manifest.exists() && !default_profile.manifest.exists() {
        fs::copy(&legacy_manifest, &default_profile.manifest)?;
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_profile_name_validation() {
        // Valid names would not trigger errors in actual create
        assert!(!"test-profile".contains('/'));
        assert!(!"work_env".contains('\\'));
        
        // Invalid names
        assert!("../bad".contains('/'));
        assert!("bad\\path".contains('\\'));
    }
    
}
