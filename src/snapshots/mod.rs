//! Snapshot management for dotfiles.
//!
//! This module provides functionality to create, list, rollback, and delete
//! versioned snapshots of dotfiles.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use crate::cfg::Config;
use crate::ui;

/// Represents a snapshot of dotfiles
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    /// Unique identifier for the snapshot
    pub id: String,
    /// Optional description/message for the snapshot
    pub message: Option<String>,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Number of files in the snapshot
    pub file_count: usize,
    /// Total size in bytes
    pub size_bytes: u64,
}

/// Options for pruning old snapshots
#[derive(Debug, Clone)]
pub struct PruneOpts {
    /// Keep N most recent snapshots
    pub keep_count: Option<usize>,
    /// Keep snapshots newer than this duration string (e.g., "30d", "7d")
    pub keep_age: Option<String>,
    /// Keep snapshots until total size is under this limit
    pub keep_size: Option<String>,
    /// If true, just show what would be deleted without actually deleting
    pub dry_run: bool,
}

/// Get the snapshots directory path
fn get_snapshots_dir() -> Result<PathBuf> {
    let base_dir = dirs::home_dir()
        .context("Failed to find home directory")?
        .join(".dotdipper")
        .join("snapshots");
    
    Ok(base_dir)
}

/// Create a new snapshot
pub fn create(_config: &Config, message: Option<String>) -> Result<Snapshot> {
    let snapshots_dir = get_snapshots_dir()?;
    fs::create_dir_all(&snapshots_dir)?;
    
    // Generate unique ID based on timestamp
    let now = Utc::now();
    let id = now.format("%Y%m%d_%H%M%S").to_string();
    
    // Create snapshot directory
    let snapshot_dir = snapshots_dir.join(&id);
    fs::create_dir_all(&snapshot_dir)?;
    
    // Copy compiled files to snapshot
    let compiled_dir = dirs::home_dir()
        .context("Failed to find home directory")?
        .join(".dotdipper")
        .join("compiled");
    
    let mut file_count = 0;
    let mut size_bytes = 0u64;
    
    if compiled_dir.exists() {
        for entry in walkdir::WalkDir::new(&compiled_dir)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.file_type().is_file() {
                let rel_path = entry.path().strip_prefix(&compiled_dir)?;
                let target_path = snapshot_dir.join(rel_path);
                
                if let Some(parent) = target_path.parent() {
                    fs::create_dir_all(parent)?;
                }
                
                fs::copy(entry.path(), &target_path)?;
                file_count += 1;
                size_bytes += entry.metadata()?.len();
            }
        }
    }
    
    let snapshot = Snapshot {
        id: id.clone(),
        message,
        created_at: now,
        file_count,
        size_bytes,
    };
    
    // Save snapshot metadata
    let metadata_path = snapshot_dir.join("snapshot.json");
    let metadata_json = serde_json::to_string_pretty(&snapshot)?;
    fs::write(metadata_path, metadata_json)?;
    
    ui::success(&format!("Created snapshot: {} ({} files)", id, file_count));
    
    // Auto-prune if configured
    if let Some(opts) = build_prune_opts_from_config(_config) {
        ui::info("Auto-pruning old snapshots...");
        if let Err(e) = prune(_config, &opts) {
            ui::warn(&format!("Auto-pruning failed: {}", e));
            // Don't fail snapshot creation if pruning fails
        }
    }
    
    Ok(snapshot)
}

/// List all snapshots
pub fn list(config: &Config) -> Result<Vec<Snapshot>> {
    let _ = config; // Config might be used for filtering in the future
    let snapshots_dir = get_snapshots_dir()?;
    
    if !snapshots_dir.exists() {
        return Ok(Vec::new());
    }
    
    let mut snapshots = Vec::new();
    
    for entry in fs::read_dir(snapshots_dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            let metadata_path = entry.path().join("snapshot.json");
            if metadata_path.exists() {
                let content = fs::read_to_string(&metadata_path)?;
                if let Ok(snapshot) = serde_json::from_str::<Snapshot>(&content) {
                    snapshots.push(snapshot);
                }
            }
        }
    }
    
    // Sort by creation time, newest first
    snapshots.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    
    // Display snapshots
    if snapshots.is_empty() {
        ui::info("No snapshots found");
    } else {
        ui::section("Snapshots:");
        for snap in &snapshots {
            let msg = snap.message.as_deref().unwrap_or("(no message)");
            let size = humansize::format_size(snap.size_bytes, humansize::BINARY);
            println!(
                "  {} - {} ({} files, {})",
                snap.id,
                msg,
                snap.file_count,
                size
            );
        }
    }
    
    Ok(snapshots)
}

/// Rollback to a specific snapshot
pub fn rollback(config: &Config, id: &str, force: bool) -> Result<()> {
    let _ = config;
    let snapshots_dir = get_snapshots_dir()?;
    let snapshot_dir = snapshots_dir.join(id);
    
    if !snapshot_dir.exists() {
        anyhow::bail!("Snapshot not found: {}", id);
    }
    
    // Confirm with user unless force is set
    if !force {
        let confirm = ui::prompt_confirm(
            &format!("Rollback to snapshot {}? This will overwrite current compiled files.", id),
            false,
        );
        if !confirm {
            ui::info("Rollback cancelled");
            return Ok(());
        }
    }
    
    // Get compiled directory
    let compiled_dir = dirs::home_dir()
        .context("Failed to find home directory")?
        .join(".dotdipper")
        .join("compiled");
    
    // Clear current compiled directory
    if compiled_dir.exists() {
        fs::remove_dir_all(&compiled_dir)?;
    }
    fs::create_dir_all(&compiled_dir)?;
    
    // Copy snapshot files to compiled directory
    let mut file_count = 0;
    for entry in walkdir::WalkDir::new(&snapshot_dir)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.file_type().is_file() {
            let file_name = entry.file_name().to_string_lossy();
            if file_name == "snapshot.json" {
                continue; // Skip metadata file
            }
            
            let rel_path = entry.path().strip_prefix(&snapshot_dir)?;
            let target_path = compiled_dir.join(rel_path);
            
            if let Some(parent) = target_path.parent() {
                fs::create_dir_all(parent)?;
            }
            
            fs::copy(entry.path(), &target_path)?;
            file_count += 1;
        }
    }
    
    ui::success(&format!("Rolled back to snapshot {} ({} files restored)", id, file_count));
    ui::hint("Run 'dotdipper apply' to apply the restored files to your system");
    
    Ok(())
}

/// Delete a snapshot
pub fn delete(config: &Config, id: &str, force: bool) -> Result<()> {
    let _ = config;
    let snapshots_dir = get_snapshots_dir()?;
    let snapshot_dir = snapshots_dir.join(id);
    
    if !snapshot_dir.exists() {
        anyhow::bail!("Snapshot not found: {}", id);
    }
    
    // Confirm with user unless force is set
    if !force {
        let confirm = ui::prompt_confirm(
            &format!("Delete snapshot {}? This cannot be undone.", id),
            false,
        );
        if !confirm {
            ui::info("Delete cancelled");
            return Ok(());
        }
    }
    
    fs::remove_dir_all(&snapshot_dir)?;
    ui::success(&format!("Deleted snapshot: {}", id));
    
    Ok(())
}

/// Prune old snapshots based on criteria
pub fn prune(config: &Config, opts: &PruneOpts) -> Result<()> {
    let snapshots = list(config)?;
    
    if snapshots.is_empty() {
        ui::info("No snapshots to prune");
        return Ok(());
    }
    
    let mut to_delete: Vec<&Snapshot> = Vec::new();
    let mut to_keep: Vec<&Snapshot> = Vec::new();
    
    // Apply keep_count filter
    if let Some(keep_count) = opts.keep_count {
        for (i, snap) in snapshots.iter().enumerate() {
            if i < keep_count {
                to_keep.push(snap);
            } else {
                to_delete.push(snap);
            }
        }
    } else {
        // If no specific criteria, keep all
        to_keep.extend(snapshots.iter());
    }
    
    // Apply keep_age filter
    if let Some(age_str) = &opts.keep_age {
        if let Some(duration) = parse_duration(age_str) {
            let cutoff = Utc::now() - duration;
            to_delete.retain(|snap| snap.created_at < cutoff);
            to_keep.retain(|snap| snap.created_at >= cutoff);
        }
    }
    
    // Apply keep_size filter (simplified - would need proper implementation)
    if let Some(_size_str) = &opts.keep_size {
        // TODO: Implement size-based pruning
        ui::warn("Size-based pruning not yet implemented");
    }
    
    if to_delete.is_empty() {
        ui::info("No snapshots to prune based on criteria");
        return Ok(());
    }
    
    // Show what will be deleted
    ui::section("Snapshots to delete:");
    for snap in &to_delete {
        let msg = snap.message.as_deref().unwrap_or("(no message)");
        println!("  {} - {}", snap.id, msg);
    }
    
    if opts.dry_run {
        ui::info(&format!("Would delete {} snapshots (dry run)", to_delete.len()));
        return Ok(());
    }
    
    // Actually delete
    for snap in &to_delete {
        delete(config, &snap.id, true)?;
    }
    
    ui::success(&format!("Pruned {} snapshots", to_delete.len()));
    
    Ok(())
}

/// Build PruneOpts from config if auto-pruning is enabled
pub fn build_prune_opts_from_config(config: &Config) -> Option<PruneOpts> {
    let auto_prune = config.auto_prune.as_ref()?;
    if !auto_prune.enabled {
        return None;
    }
    
    // If all options are None, don't prune
    if auto_prune.keep_count.is_none() 
        && auto_prune.keep_age.is_none() 
        && auto_prune.keep_size.is_none() {
        return None;
    }
    
    Some(PruneOpts {
        keep_count: auto_prune.keep_count,
        keep_age: auto_prune.keep_age.clone(),
        keep_size: auto_prune.keep_size.clone(),
        dry_run: false, // Auto-pruning is never dry-run
    })
}

/// Parse a duration string like "30d", "7d", "2w", "1m"
fn parse_duration(s: &str) -> Option<chrono::Duration> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    
    let (num_str, unit) = s.split_at(s.len() - 1);
    let num: i64 = num_str.parse().ok()?;
    
    match unit {
        "d" => Some(chrono::Duration::days(num)),
        "w" => Some(chrono::Duration::weeks(num)),
        "m" => Some(chrono::Duration::days(num * 30)), // Approximate month
        "h" => Some(chrono::Duration::hours(num)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_duration() {
        assert_eq!(parse_duration("7d"), Some(chrono::Duration::days(7)));
        assert_eq!(parse_duration("2w"), Some(chrono::Duration::weeks(2)));
        assert_eq!(parse_duration("30d"), Some(chrono::Duration::days(30)));
        assert_eq!(parse_duration("1m"), Some(chrono::Duration::days(30)));
        assert_eq!(parse_duration("invalid"), None);
    }
}
