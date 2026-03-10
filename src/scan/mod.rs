use anyhow::{Context, Result};
use glob::Pattern;
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use crate::cfg::Config;

pub fn discover(config: &Config, show_all: bool) -> Result<Vec<PathBuf>> {
    let home = dirs::home_dir().context("Failed to find home directory")?;
    let mut discovered = Vec::new();

    let excluder = build_excluder(&config.exclude_patterns, &home)?;

    for pattern in &config.include_patterns {
        let expanded = expand_tilde(pattern, &home);
        let is_glob = pattern.contains('*');
        discover_pattern(&expanded, &excluder, &mut discovered, show_all, is_glob)?;
    }

    // Re-add already tracked files (they were explicitly chosen)
    for file in &config.general.tracked_files {
        if file.exists() && !discovered.contains(file) {
            discovered.push(file.clone());
        }
    }

    discovered.sort();
    discovered.dedup();

    Ok(discovered)
}

fn discover_pattern(
    pattern: &str,
    excluder: &Gitignore,
    discovered: &mut Vec<PathBuf>,
    show_all: bool,
    is_glob: bool,
) -> Result<()> {
    let home = dirs::home_dir().context("Failed to find home directory")?;

    if pattern.contains('*') {
        let glob_pattern =
            Pattern::new(pattern).with_context(|| format!("Invalid glob pattern: {}", pattern))?;

        let base_dir = get_base_dir_from_pattern(pattern, &home);

        for entry in WalkDir::new(&base_dir)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();

            // Only track files, not bare directories
            if path.is_dir() {
                continue;
            }

            if !show_all && excluder.matched(path, false).is_ignore() {
                continue;
            }

            if glob_pattern.matches_path(path) {
                discovered.push(path.to_path_buf());
            }
        }
    } else {
        let path = PathBuf::from(pattern);
        if path.exists() {
            if path.is_dir() {
                for entry in WalkDir::new(&path)
                    .follow_links(false)
                    .into_iter()
                    .filter_map(|e| e.ok())
                {
                    let entry_path = entry.path();

                    if entry_path.is_file() {
                        if !show_all && excluder.matched(entry_path, false).is_ignore() {
                            continue;
                        }
                        discovered.push(entry_path.to_path_buf());
                    }
                }
            } else if path.is_file() {
                // Direct file include patterns bypass exclusions — the user
                // explicitly asked for this file (e.g. ~/.ssh/config despite
                // ~/.ssh/** being excluded).
                if !is_glob || show_all || !excluder.matched(&path, false).is_ignore() {
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
        // For tilde patterns, make them root-relative for the gitignore builder
        // (whose root is $HOME). E.g. "~/.config/foo" → "/.config/foo".
        // Expanding to an absolute path like "/Users/x/.config/foo" breaks
        // gitignore semantics where a leading '/' means "anchored to root".
        let gitignore_pat = if let Some(stripped) = pattern.strip_prefix("~/") {
            format!("/{}", stripped)
        } else {
            pattern.clone()
        };
        builder
            .add_line(None, &gitignore_pat)
            .with_context(|| format!("Invalid exclude pattern: {}", pattern))?;
    }

    Ok(builder.build()?)
}

fn expand_tilde(path: &str, home: &Path) -> String {
    if let Some(stripped) = path.strip_prefix("~/") {
        home.join(stripped).to_string_lossy().to_string()
    } else {
        path.to_string()
    }
}

fn get_base_dir_from_pattern(pattern: &str, home: &Path) -> PathBuf {
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
