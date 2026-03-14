use anyhow::{Context, Result};
use glob::Pattern;
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use crate::cfg::Config;

pub fn discover(config: &Config, show_all: bool) -> Result<Vec<PathBuf>> {
    let home = dirs::home_dir().context("Failed to find home directory")?;
    let mut discovered = Vec::new();

    let ignore_file = crate::paths::ignore_file()?;
    let excluder = build_excluder(&config.exclude_patterns, &home, &ignore_file)?;

    for pattern in &config.include_patterns {
        let expanded = expand_tilde(pattern, &home);
        let is_glob = pattern.contains('*');
        discover_pattern(&expanded, &excluder, &mut discovered, show_all, is_glob)?;
    }

    // Re-add already tracked files (they were explicitly chosen)
    for file in &config.general.tracked_files {
        if file.exists()
            && !discovered.contains(file)
            && should_readd_tracked_file(file, &config.include_patterns, &excluder, &home, show_all)
        {
            discovered.push(file.clone());
        }
    }

    discovered.sort();
    discovered.dedup();

    Ok(discovered)
}

fn should_readd_tracked_file(
    path: &Path,
    include_patterns: &[String],
    excluder: &Gitignore,
    home: &Path,
    show_all: bool,
) -> bool {
    if show_all || is_explicit_file_include(path, include_patterns, home) {
        return true;
    }

    !excluder.matched(path, false).is_ignore()
}

fn is_explicit_file_include(path: &Path, include_patterns: &[String], home: &Path) -> bool {
    include_patterns.iter().any(|pattern| {
        !contains_glob_chars(pattern) && PathBuf::from(expand_tilde(pattern, home)) == path
    })
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

fn build_excluder(patterns: &[String], home: &Path, ignore_file: &Path) -> Result<Gitignore> {
    let mut builder = GitignoreBuilder::new(home);

    if ignore_file.exists() {
        let contents =
            std::fs::read_to_string(ignore_file).context("Failed to read .dotdipperignore")?;
        for line in contents.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let gitignore_pat = if let Some(stripped) = trimmed.strip_prefix("~/") {
                format!("/{}", stripped)
            } else {
                trimmed.to_string()
            };
            builder
                .add_line(None, &gitignore_pat)
                .with_context(|| format!("Invalid pattern in .dotdipperignore: {}", trimmed))?;
        }
    }

    for pattern in patterns {
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

fn contains_glob_chars(path: &str) -> bool {
    path.contains('*') || path.contains('?') || path.contains('[')
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn explicit_file_include_overrides_broad_ignore() {
        let temp_dir = TempDir::new().unwrap();
        let home = temp_dir.path();
        let ignore_file = temp_dir.path().join(".dotdipperignore");
        fs::write(&ignore_file, "~/.ssh/**\n").unwrap();

        let excluder = build_excluder(&[], home, &ignore_file).unwrap();
        let ssh_config = home.join(".ssh/config");

        assert!(should_readd_tracked_file(
            &ssh_config,
            &["~/.ssh/config".to_string()],
            &excluder,
            home,
            false,
        ));
    }

    #[test]
    fn ignored_tracked_file_is_not_readded() {
        let temp_dir = TempDir::new().unwrap();
        let home = temp_dir.path();
        let ignore_file = temp_dir.path().join(".dotdipperignore");
        fs::write(&ignore_file, "~/.config/gcloud/**\n").unwrap();

        let excluder = build_excluder(&[], home, &ignore_file).unwrap();
        let gcloud_file = home.join(".config/gcloud/credentials.db");

        assert!(!should_readd_tracked_file(
            &gcloud_file,
            &["~/.config/**".to_string()],
            &excluder,
            home,
            false,
        ));
    }
}
