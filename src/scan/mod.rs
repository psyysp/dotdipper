use anyhow::{Context, Result};
use glob::Pattern;
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use crate::cfg::Config;

pub fn discover(config: &Config, show_all: bool) -> Result<Vec<PathBuf>> {
    let home = dirs::home_dir().context("Failed to find home directory")?;
    let mut discovered = Vec::new();
    
    // Build gitignore-style matcher from exclude patterns
    let excluder = build_excluder(&config.exclude_patterns, &home)?;
    
    // Process include patterns
    for pattern in &config.include_patterns {
        let expanded = expand_tilde(pattern, &home);
        discover_pattern(&expanded, &excluder, &mut discovered, show_all)?;
    }
    
    // Add already tracked files (they should always be included)
    for file in &config.general.tracked_files {
        if file.exists() && !discovered.contains(file) {
            discovered.push(file.clone());
        }
    }
    
    // Sort for deterministic output
    discovered.sort();
    discovered.dedup();
    
    Ok(discovered)
}

fn discover_pattern(
    pattern: &str,
    excluder: &Gitignore,
    discovered: &mut Vec<PathBuf>,
    show_all: bool,
) -> Result<()> {
    let home = dirs::home_dir().context("Failed to find home directory")?;
    
    // Check if it's a glob pattern or a direct path
    if pattern.contains('*') {
        // It's a glob pattern
        let glob_pattern = Pattern::new(pattern)
            .with_context(|| format!("Invalid glob pattern: {}", pattern))?;
        
        // Determine the base directory for walking
        let base_dir = get_base_dir_from_pattern(pattern, &home);
        
        for entry in WalkDir::new(&base_dir)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            
            // Skip if excluded
            if !show_all && excluder.matched(path, path.is_dir()).is_ignore() {
                continue;
            }
            
            // Check if it matches the pattern
            if glob_pattern.matches_path(path) {
                discovered.push(path.to_path_buf());
            }
        }
    } else {
        // It's a direct path
        let path = PathBuf::from(pattern);
        if path.exists() {
            if path.is_dir() {
                // Recursively add all files in directory
                for entry in WalkDir::new(&path)
                    .follow_links(false)
                    .into_iter()
                    .filter_map(|e| e.ok())
                {
                    let entry_path = entry.path();
                    
                    if entry_path.is_file() {
                        // Skip if excluded
                        if !show_all && excluder.matched(entry_path, false).is_ignore() {
                            continue;
                        }
                        discovered.push(entry_path.to_path_buf());
                    }
                }
            } else if path.is_file() {
                // Skip if excluded
                if show_all || !excluder.matched(&path, false).is_ignore() {
                    discovered.push(path);
                }
            }
        }
    }
    
    Ok(())
}

fn build_excluder(patterns: &[String], home: &Path) -> Result<Gitignore> {
    let mut builder = GitignoreBuilder::new(home);
    
    for pattern in patterns {
        let expanded = expand_tilde(pattern, home);
        builder
            .add_line(None, &expanded)
            .with_context(|| format!("Invalid exclude pattern: {}", pattern))?;
    }
    
    Ok(builder.build()?)
}

fn expand_tilde(path: &str, home: &Path) -> String {
    if path.starts_with("~/") {
        home.join(&path[2..]).to_string_lossy().to_string()
    } else {
        path.to_string()
    }
}

fn get_base_dir_from_pattern(pattern: &str, home: &Path) -> PathBuf {
    // Find the first non-glob component and use that as base
    let expanded = expand_tilde(pattern, home);
    let parts: Vec<&str> = expanded.split('/').collect();
    
    let mut base_parts = Vec::new();
    for part in parts {
        if part.contains('*') || part.contains('?') || part.contains('[') {
            break;
        }
        base_parts.push(part);
    }
    
    if base_parts.is_empty() {
        home.to_path_buf()
    } else {
        PathBuf::from(base_parts.join("/"))
    }
}

pub fn scan_for_configs(base_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut configs = Vec::new();
    
    // Common config file patterns
    let patterns = vec![
        ".*rc",       // .bashrc, .zshrc, .vimrc, etc.
        ".*config",   // .gitconfig, etc.
        ".*conf",     // .tmux.conf, etc.
        ".profile",
        ".bash_profile",
        ".zprofile",
    ];
    
    // Scan home directory (non-recursive for top-level files)
    for entry in std::fs::read_dir(base_dir)? {
        let entry = entry?;
        let path = entry.path();
        let name = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        
        if name.starts_with('.') && path.is_file() {
            for pattern in &patterns {
                if Pattern::new(pattern)?.matches(name) {
                    configs.push(path.clone());
                    break;
                }
            }
        }
    }
    
    // Scan .config directory
    let config_dir = base_dir.join(".config");
    if config_dir.exists() {
        for entry in WalkDir::new(&config_dir)
            .max_depth(3)  // Don't go too deep
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.path().is_file() {
                configs.push(entry.path().to_path_buf());
            }
        }
    }
    
    // Scan .local/share for application configs
    let local_share = base_dir.join(".local/share");
    if local_share.exists() {
        for entry in std::fs::read_dir(&local_share)? {
            let entry = entry?;
            let path = entry.path();
            
            // Look for config files in app directories
            if path.is_dir() {
                let config_file = path.join("config");
                if config_file.exists() {
                    configs.push(config_file);
                }
                let config_dir = path.join("config");
                if config_dir.exists() && config_dir.is_dir() {
                    for entry in WalkDir::new(&config_dir)
                        .max_depth(2)
                        .follow_links(false)
                        .into_iter()
                        .filter_map(|e| e.ok())
                    {
                        if entry.path().is_file() {
                            configs.push(entry.path().to_path_buf());
                        }
                    }
                }
            }
        }
    }
    
    configs.sort();
    configs.dedup();
    Ok(configs)
}
