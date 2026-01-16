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
        let tracked_files = &config.general.tracked_files;
        
        // Quick check if any files have changed
        let mut has_changes = false;
        for file in tracked_files {
            if !file.exists() {
                has_changes = true;
                break;
            }
            
            if let Some(stored_hash) = current_manifest.get_file(file) {
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
            ui::info("No changes detected, skipping snapshot");
            return Ok(Snapshot {
                file_count: current_manifest.files.len(),
            });
        }
    }
    
    // Create new manifest
    let mut manifest = Manifest::new();
    let tracked_files = &config.general.tracked_files;
    
    // Hash all tracked files
    let hashes = hash_files(tracked_files, true)?;
    
    // Copy files to repo and add to manifest
    let repo_path = get_compiled_path()?;
    fs::create_dir_all(&repo_path)?;
    
    let pb = ui::progress_bar(hashes.len() as u64, "Creating snapshot");
    
    for file_hash in hashes {
        // Calculate relative path from home
        let home = dirs::home_dir().context("Failed to find home directory")?;
        let rel_path = file_hash.path
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
    
    // Save manifest
    manifest.save(&manifest_path)?;
    
    Ok(Snapshot {
        file_count: manifest.files.len(),
    })
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
    
    // Check tracked files
    for file_path in &config.general.tracked_files {
        let rel_path = file_path
            .strip_prefix(&home)
            .unwrap_or(file_path);
        
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
    for (rel_path, _) in &manifest.files {
        let full_path = home.join(rel_path);
        if !config.general.tracked_files.contains(&full_path) {
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
        anyhow::bail!("Manifest verification failed for {} files", invalid_files.len());
    }
    
    Ok(())
}

fn get_manifest_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Failed to find home directory")?;
    Ok(home.join(".dotdipper").join("manifest.lock"))
}

fn get_compiled_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Failed to find home directory")?;
    Ok(home.join(".dotdipper").join("compiled"))
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
