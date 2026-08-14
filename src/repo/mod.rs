pub mod apply;

use anyhow::{Context, Result};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::cfg::Config;
use crate::hash::{hash_files, Manifest};
use crate::ui;

pub struct Snapshot {
    pub file_count: usize,
}

pub struct Status {
    pub modified: Vec<PathBuf>,
    pub added: Vec<PathBuf>,
    pub deleted: Vec<PathBuf>,
}

impl Status {
    pub fn is_clean(&self) -> bool {
        self.modified.is_empty() && self.added.is_empty() && self.deleted.is_empty()
    }

    pub fn print_detailed(&self) {
        if !self.modified.is_empty() {
            ui::section("Modified files:");
            for file in &self.modified {
                println!("  M {}", file.display());
            }
        }

        if !self.added.is_empty() {
            ui::section("Added files:");
            for file in &self.added {
                println!("  A {}", file.display());
            }
        }

        if !self.deleted.is_empty() {
            ui::section("Deleted files:");
            for file in &self.deleted {
                println!("  D {}", file.display());
            }
        }
    }
}

pub fn snapshot(config: &Config, force: bool) -> Result<Snapshot> {
    let manifest_path = get_manifest_path()?;

    // Check if we need to create a snapshot
    if !force && manifest_path.exists() {
        let current_manifest = Manifest::load(&manifest_path)?;
        let home = dirs::home_dir().context("Failed to find home directory")?;
        let tracked_files = crate::cfg::existing_tracked_files(config)?;

        // Quick check if any files have changed (manifest keys are home-relative)
        let mut has_changes = tracked_files.len() != current_manifest.files.len();
        for file in &tracked_files {
            let rel = file.strip_prefix(&home).unwrap_or(file);
            if !file.exists() {
                has_changes = true;
                break;
            }

            if let Some(stored_hash) = current_manifest.get_file(rel) {
                if let Ok(current_hash) = crate::hash::hash_file(file) {
                    if stored_hash.hash != current_hash.hash {
                        has_changes = true;
                        break;
                    }
                }
            } else {
                has_changes = true;
                break;
            }
        }

        if !has_changes {
            ui::info("No file content changes; refreshing bootstrap artifacts");
            refresh_shipped_artifacts(config, &current_manifest)?;
            return Ok(Snapshot {
                file_count: current_manifest.files.len(),
            });
        }
    }

    // Create new manifest
    let mut manifest = Manifest::new();
    // Hash all tracked files (directories are expanded)
    let hashes = hash_files(&crate::cfg::existing_tracked_files(config)?, true)?;
    if hashes.is_empty() && manifest_path.exists() {
        let current = Manifest::load(&manifest_path)?;
        if !current.files.is_empty() {
            ui::warn(
                "No tracked files present on this machine; keeping the existing snapshot instead of wiping it",
            );
            refresh_shipped_artifacts(config, &current)?;
            return Ok(Snapshot {
                file_count: current.files.len(),
            });
        }
    }

    // Copy files to repo and add to manifest
    let repo_path = get_compiled_path()?;
    fs::create_dir_all(&repo_path)?;

    let pb = ui::progress_bar(hashes.len() as u64, "Creating snapshot");

    for file_hash in hashes {
        // Calculate relative path from home
        let home = dirs::home_dir().context("Failed to find home directory")?;
        let rel_path = file_hash
            .path
            .strip_prefix(&home)
            .unwrap_or(&file_hash.path);

        // Copy file to repo
        let dest_path = repo_path.join(rel_path);
        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent)?;
        }

        copy_file_with_permissions(&file_hash.path, &dest_path)?;

        // Add to manifest with relative path
        let mut relative_hash = file_hash.clone();
        relative_hash.path = rel_path.to_path_buf();
        manifest.add_file(relative_hash);

        pb.inc(1);
    }

    pb.finish_with_message("Snapshot created");

    carry_forward_missing_entries(&mut manifest, config, &repo_path)?;

    refresh_shipped_artifacts(config, &manifest)?;

    Ok(Snapshot {
        file_count: manifest.files.len(),
    })
}

fn refresh_shipped_artifacts(config: &Config, manifest: &Manifest) -> Result<()> {
    let repo_path = get_compiled_path()?;
    fs::create_dir_all(&repo_path)?;
    let manifest_path = get_manifest_path()?;
    manifest.save(&manifest_path)?;
    let bundled_dir = repo_path.join(".dotdipper");
    fs::create_dir_all(&bundled_dir)?;
    manifest.save(&bundled_dir.join("manifest.lock"))?;
    write_bootstrap_toml(&bundled_dir, config)?;

    write_push_gitignore(&repo_path, config)?;

    crate::install::generate_scripts(config, &crate::install::detect_os())?;
    Ok(())
}

fn load_previous_manifest() -> Option<Manifest> {
    let candidates = [
        get_manifest_path().ok(),
        crate::paths::compiled_bundled_manifest().ok(),
    ];
    for path in candidates.into_iter().flatten() {
        if path.exists() {
            if let Ok(manifest) = Manifest::load(&path) {
                if !manifest.files.is_empty() {
                    return Some(manifest);
                }
            }
        }
    }
    None
}

fn is_tracked_rel(rel: &Path, config: &Config, home: &Path) -> bool {
    for tracked in &config.general.tracked_files {
        let abs = crate::cfg::expand_tracked_path(tracked, home);
        if let Some(tracked_rel) = crate::cfg::home_relative_path(&abs, home) {
            if rel == tracked_rel.as_path() || rel.starts_with(&tracked_rel) {
                return true;
            }
        }
        let fallback = abs.strip_prefix(home).unwrap_or(abs.as_path());
        if rel == fallback || rel.starts_with(fallback) {
            return true;
        }
    }
    false
}

/// Keep compiled copies of still-tracked files that are missing from $HOME
/// (fresh machine before install, or `.age` after decrypt-to-plaintext).
fn carry_forward_missing_entries(
    manifest: &mut Manifest,
    config: &Config,
    compiled: &Path,
) -> Result<()> {
    let Some(old) = load_previous_manifest() else {
        return Ok(());
    };
    let home = dirs::home_dir().context("Failed to find home directory")?;
    let mut kept = 0usize;
    for (rel, file_hash) in &old.files {
        if manifest.has_file(rel) {
            continue;
        }
        if !is_tracked_rel(rel, config, &home) {
            continue;
        }
        if compiled.join(rel).is_file() {
            ui::warn(&format!(
                "Keeping compiled copy of {} (not present in $HOME)",
                rel.display()
            ));
            manifest.add_file(file_hash.clone());
            kept += 1;
        }
    }
    if kept > 0 {
        ui::info(&format!(
            "Preserved {} tracked file(s) from the existing snapshot",
            kept
        ));
    }
    Ok(())
}

pub fn status(config: &Config) -> Result<Status> {
    let manifest_path = get_manifest_path()?;

    if !manifest_path.exists() {
        // No snapshot yet, all files are "added"
        return Ok(Status {
            modified: vec![],
            added: config.general.tracked_files.clone(),
            deleted: vec![],
        });
    }

    let manifest = Manifest::load(&manifest_path)?;
    let mut status = Status {
        modified: vec![],
        added: vec![],
        deleted: vec![],
    };

    let home = dirs::home_dir().context("Failed to find home directory")?;
    let tracked_files = crate::cfg::existing_tracked_files(config)?;

    // Check tracked files
    for file_path in &tracked_files {
        let rel_path = file_path.strip_prefix(&home).unwrap_or(file_path);

        if !file_path.exists() {
            // File was deleted
            if manifest.has_file(rel_path) {
                status.deleted.push(file_path.clone());
            }
        } else if let Some(stored_hash) = manifest.get_file(rel_path) {
            // Check if modified
            if let Ok(current_hash) = crate::hash::hash_file(file_path) {
                if stored_hash.hash != current_hash.hash {
                    status.modified.push(file_path.clone());
                }
            }
        } else {
            // New file
            status.added.push(file_path.clone());
        }
    }

    // Check for files in manifest that are no longer tracked
    for rel_path in manifest.files.keys() {
        let full_path = home.join(rel_path);
        if !tracked_files.contains(&full_path) {
            status.deleted.push(full_path);
        }
    }

    Ok(status)
}

pub fn check_manifest(config_path: &Path) -> Result<()> {
    let manifest_path = config_path
        .parent()
        .context("Invalid config path")?
        .join("manifest.lock");

    if !manifest_path.exists() {
        anyhow::bail!("Manifest not found");
    }

    let manifest = Manifest::load(&manifest_path)?;
    let invalid_files = crate::hash::verify_manifest(&manifest)?;

    if !invalid_files.is_empty() {
        anyhow::bail!(
            "Manifest verification failed for {} files",
            invalid_files.len()
        );
    }

    Ok(())
}

fn get_manifest_path() -> Result<PathBuf> {
    crate::paths::manifest_file()
}

fn get_compiled_path() -> Result<PathBuf> {
    crate::paths::compiled_dir()
}

fn write_push_gitignore(repo_path: &Path, config: &Config) -> Result<()> {
    let push_ignored = crate::cfg::resolve_push_ignored_paths(config)?;

    let mut lines = vec![
        "# Auto-generated by dotdipper - do not edit manually",
        "*.tmp",
        "*.swp",
        "*.swo",
        "*~",
        ".DS_Store",
        "Thumbs.db",
        "*.bak",
        "*.backup",
    ];

    let ignored_lines: Vec<String>;
    if !push_ignored.is_empty() {
        ignored_lines = push_ignored;
        lines.push("");
        lines.push("# Local-only files (excluded from git push)");
    } else {
        ignored_lines = Vec::new();
    }

    let mut content: String = lines.join("\n");
    for line in &ignored_lines {
        content.push('\n');
        content.push_str(line);
    }
    content.push('\n');

    fs::write(repo_path.join(".gitignore"), content)?;
    Ok(())
}

fn write_bootstrap_toml(bundled_dir: &Path, config: &Config) -> Result<()> {
    let mut boot = crate::cfg::portable_config(config);
    boot.general.active_profile = None;
    boot.hooks = None;
    boot.daemon = None;
    boot.remote = None;
    boot.auto_prune = None;
    boot.dotfiles = None;
    boot.push_ignore.clear();
    boot.exclude_patterns.clear();
    boot.include_patterns.clear();
    let toml_string = toml::to_string_pretty(&boot)?;
    fs::write(bundled_dir.join("bootstrap.toml"), toml_string)?;
    Ok(())
}

fn copy_file_with_permissions(source: &Path, dest: &Path) -> Result<()> {
    // Read source file
    let mut source_file = File::open(source)
        .with_context(|| format!("Failed to open source file: {}", source.display()))?;
    let mut contents = Vec::new();
    source_file.read_to_end(&mut contents)?;

    // Write to destination
    let mut dest_file = File::create(dest)
        .with_context(|| format!("Failed to create destination file: {}", dest.display()))?;
    dest_file.write_all(&contents)?;

    // Copy permissions on Unix
    #[cfg(unix)]
    {
        let metadata = source.metadata()?;
        let permissions = metadata.permissions();
        fs::set_permissions(dest, permissions)?;
    }

    Ok(())
}
