//! Snapshot management for dotfiles.
//!
//! This module provides functionality to create, list, rollback, and delete
//! versioned snapshots of dotfiles.

use anyhow::Result;
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

fn get_snapshots_dir() -> Result<PathBuf> {
    crate::paths::snapshots_dir()
}

/// Create a new snapshot
pub fn create(config: &Config, message: Option<String>) -> Result<Snapshot> {
    create_inner(config, message, true)
}

fn create_inner(
    config: &Config,
    message: Option<String>,
    run_auto_prune: bool,
) -> Result<Snapshot> {
    let snapshots_dir = get_snapshots_dir()?;
    fs::create_dir_all(&snapshots_dir)?;

    // Generate unique ID based on timestamp (include millis to avoid same-second collisions)
    let now = Utc::now();
    let mut id = format!(
        "{}_{:03}",
        now.format("%Y%m%d_%H%M%S"),
        now.timestamp_subsec_millis()
    );
    // Extremely unlikely, but guarantee uniqueness if the directory already exists
    let mut suffix = 1u32;
    while snapshots_dir.join(&id).exists() {
        id = format!(
            "{}_{:03}_{}",
            now.format("%Y%m%d_%H%M%S"),
            now.timestamp_subsec_millis(),
            suffix
        );
        suffix += 1;
    }

    // Create snapshot directory
    let snapshot_dir = snapshots_dir.join(&id);
    fs::create_dir_all(&snapshot_dir)?;

    let compiled_dir = crate::paths::compiled_dir()?;

    let mut file_count = 0;
    let mut size_bytes = 0u64;

    if compiled_dir.exists() {
        for entry in walkdir::WalkDir::new(&compiled_dir)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.file_type().is_file() {
                let rel_path = entry.path().strip_prefix(&compiled_dir)?;
                // Skip git objects — they are huge and restored separately by preserving .git
                if is_git_path(rel_path) {
                    continue;
                }
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

    // Auto-prune if configured (skipped for safety snapshots taken during rollback)
    if run_auto_prune {
        if let Some(opts) = build_prune_opts_from_config(config) {
            ui::info("Auto-pruning old snapshots...");
            if let Err(e) = prune(config, &opts) {
                ui::warn(&format!("Auto-pruning failed: {}", e));
                // Don't fail snapshot creation if pruning fails
            }
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
    snapshots.sort_by_key(|b| std::cmp::Reverse(b.created_at));

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
                snap.id, msg, snap.file_count, size
            );
        }
    }

    Ok(snapshots)
}

/// Rollback to a specific snapshot
pub fn rollback(config: &Config, id: &str, force: bool) -> Result<()> {
    let snapshots_dir = get_snapshots_dir()?;
    let snapshot_dir = snapshots_dir.join(id);

    if !snapshot_dir.exists() {
        anyhow::bail!("Snapshot not found: {}", id);
    }

    // Confirm with user unless force is set
    if !force {
        let confirm = ui::prompt_confirm(
            &format!(
                "Rollback to snapshot {}? This will overwrite current compiled files (your git history in compiled/.git is preserved).",
                id
            ),
            false,
        );
        if !confirm {
            ui::info("Rollback cancelled");
            return Ok(());
        }
    }

    // Always create a safety snapshot of the current store before destructive rollback
    // (skip auto-prune so we cannot accidentally delete the rollback target)
    ui::info("Creating safety snapshot of current state before rollback...");
    match create_inner(
        config,
        Some(format!("pre-rollback safety snapshot (before {})", id)),
        false,
    ) {
        Ok(safety) => ui::success(&format!("Safety snapshot created: {}", safety.id)),
        Err(e) => {
            ui::warn(&format!("Could not create safety snapshot: {}", e));
            if !force && !ui::prompt_confirm("Continue rollback without a safety snapshot?", false)
            {
                ui::info("Rollback cancelled");
                return Ok(());
            }
        }
    }

    // Ensure the target still exists after the safety snapshot
    if !snapshot_dir.exists() {
        anyhow::bail!(
            "Snapshot {} disappeared unexpectedly; aborting rollback",
            id
        );
    }

    let compiled_dir = crate::paths::compiled_dir()?;

    // Clear compiled contents but preserve .git so push/pull history survives
    clear_compiled_preserving_git(&compiled_dir)?;
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
            if is_git_path(rel_path) {
                continue;
            }
            let target_path = compiled_dir.join(rel_path);

            if let Some(parent) = target_path.parent() {
                fs::create_dir_all(parent)?;
            }

            fs::copy(entry.path(), &target_path)?;
            file_count += 1;
        }
    }

    // Keep base manifest.lock aligned with whatever the snapshot restored
    if let Err(e) = crate::repo::sync_manifest_from_compiled() {
        ui::warn(&format!(
            "Restored files but could not sync manifest.lock: {}",
            e
        ));
    }

    ui::success(&format!(
        "Rolled back to snapshot {} ({} files restored)",
        id, file_count
    ));
    ui::hint("Run 'dotdipper apply' to apply the restored files to your system");
    ui::hint("Your previous compiled state was saved as a safety snapshot (see 'dotdipper snapshot list')");

    Ok(())
}

fn is_git_path(rel_path: &std::path::Path) -> bool {
    rel_path
        .components()
        .next()
        .map(|c| c.as_os_str() == ".git")
        .unwrap_or(false)
}

fn clear_compiled_preserving_git(compiled_dir: &std::path::Path) -> Result<()> {
    if !compiled_dir.exists() {
        return Ok(());
    }

    for entry in fs::read_dir(compiled_dir)? {
        let entry = entry?;
        if entry.file_name() == ".git" {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            fs::remove_dir_all(&path)?;
        } else {
            fs::remove_file(&path)?;
        }
    }

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

/// Prune old snapshots based on criteria.
/// A snapshot is kept if ANY configured keep criterion says to keep it.
pub fn prune(config: &Config, opts: &PruneOpts) -> Result<()> {
    let snapshots = list(config)?;

    if snapshots.is_empty() {
        ui::info("No snapshots to prune");
        return Ok(());
    }

    if opts.keep_count.is_none() && opts.keep_age.is_none() && opts.keep_size.is_none() {
        ui::info("No prune criteria specified; keeping all snapshots");
        return Ok(());
    }

    let mut protected_ids = std::collections::HashSet::new();

    // Keep N most recent
    if let Some(keep_count) = opts.keep_count {
        for snap in snapshots.iter().take(keep_count) {
            protected_ids.insert(snap.id.clone());
        }
    }

    // Keep anything newer than the age cutoff
    if let Some(age_str) = &opts.keep_age {
        if let Some(duration) = parse_duration(age_str) {
            let cutoff = Utc::now() - duration;
            for snap in &snapshots {
                if snap.created_at >= cutoff {
                    protected_ids.insert(snap.id.clone());
                }
            }
        } else {
            ui::warn(&format!("Invalid keep_age value: {}", age_str));
        }
    }

    // Apply keep_size filter (simplified - would need proper implementation)
    if let Some(_size_str) = &opts.keep_size {
        // TODO: Implement size-based pruning
        ui::warn("Size-based pruning not yet implemented");
    }

    let to_delete: Vec<&Snapshot> = snapshots
        .iter()
        .filter(|snap| !protected_ids.contains(&snap.id))
        .collect();

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
        ui::info(&format!(
            "Would delete {} snapshots (dry run)",
            to_delete.len()
        ));
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
        && auto_prune.keep_size.is_none()
    {
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
    use std::path::Path;

    #[test]
    fn test_parse_duration() {
        assert_eq!(parse_duration("7d"), Some(chrono::Duration::days(7)));
        assert_eq!(parse_duration("2w"), Some(chrono::Duration::weeks(2)));
        assert_eq!(parse_duration("30d"), Some(chrono::Duration::days(30)));
        assert_eq!(parse_duration("1m"), Some(chrono::Duration::days(30)));
        assert_eq!(parse_duration("invalid"), None);
    }

    #[test]
    fn clear_compiled_preserves_git_directory() {
        let temp = tempfile::TempDir::new().unwrap();
        let compiled = temp.path().join("compiled");
        fs::create_dir_all(compiled.join(".git").join("objects")).unwrap();
        fs::write(compiled.join(".git").join("HEAD"), "ref: refs/heads/main\n").unwrap();
        fs::write(compiled.join(".zshrc"), "old\n").unwrap();
        fs::create_dir_all(compiled.join("nested")).unwrap();
        fs::write(compiled.join("nested").join("a"), "x\n").unwrap();

        clear_compiled_preserving_git(&compiled).unwrap();

        assert!(compiled.join(".git").join("HEAD").exists());
        assert!(!compiled.join(".zshrc").exists());
        assert!(!compiled.join("nested").exists());
    }

    #[test]
    fn is_git_path_detects_git_prefix() {
        assert!(is_git_path(Path::new(".git/config")));
        assert!(is_git_path(Path::new(".git")));
        assert!(!is_git_path(Path::new(".gitignore")));
        assert!(!is_git_path(Path::new("foo/.git/config")));
    }

    #[test]
    fn prune_keep_age_alone_marks_old_snapshots() {
        let old = Snapshot {
            id: "old".into(),
            message: None,
            created_at: Utc::now() - chrono::Duration::days(40),
            file_count: 1,
            size_bytes: 10,
        };
        let recent = Snapshot {
            id: "recent".into(),
            message: None,
            created_at: Utc::now() - chrono::Duration::days(2),
            file_count: 1,
            size_bytes: 10,
        };
        let snapshots = vec![recent.clone(), old.clone()]; // newest first

        let mut protected_ids = std::collections::HashSet::new();
        let duration = parse_duration("30d").unwrap();
        let cutoff = Utc::now() - duration;
        for snap in &snapshots {
            if snap.created_at >= cutoff {
                protected_ids.insert(snap.id.clone());
            }
        }

        let to_delete: Vec<_> = snapshots
            .iter()
            .filter(|s| !protected_ids.contains(&s.id))
            .map(|s| s.id.as_str())
            .collect();

        assert_eq!(to_delete, vec!["old"]);
        assert!(protected_ids.contains("recent"));
    }
}
