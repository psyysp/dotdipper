use anyhow::{Context, Result};
use chrono::Utc;
use colored::*;
use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs as unix_fs;
use std::path::{Component, Path, PathBuf};

use crate::cfg::{Config, RestoreMode};
use crate::hash::Manifest;
use crate::ui;

#[derive(Debug, Clone)]
pub struct ApplyOpts {
    pub force: bool,
    pub allow_outside_home: bool,
}

#[derive(Debug, Clone)]
pub struct AppliedAction {
    pub mode: AppliedMode,
    pub target: PathBuf,
    pub source: PathBuf,
    pub backup_created: bool,
    pub skipped_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AppliedMode {
    Symlinked,
    Copied,
    Skipped,
}

impl AppliedMode {
    pub fn color_str(&self) -> ColoredString {
        match self {
            AppliedMode::Symlinked => "Symlinked".green(),
            AppliedMode::Copied => "Copied".blue(),
            AppliedMode::Skipped => "Skipped".dimmed(),
        }
    }
}

pub fn apply(
    compiled_root: &Path,
    manifest: &Manifest,
    cfg: &Config,
    opts: &ApplyOpts,
) -> Result<Vec<AppliedAction>> {
    let home_dir = dirs::home_dir().context("Failed to find home directory")?;
    let mut actions = Vec::new();

    let pb = ui::progress_bar(manifest.files.len() as u64, "Applying dotfiles");

    for rel_path in manifest.files.keys() {
        // Reject path traversal / absolute / weird components before touching the filesystem
        let target_path = match resolve_home_target(&home_dir, rel_path) {
            Ok(p) => p,
            Err(reason) => {
                pb.inc(1);
                actions.push(AppliedAction {
                    mode: AppliedMode::Skipped,
                    target: home_dir.join(rel_path),
                    source: compiled_root.join(rel_path),
                    backup_created: false,
                    skipped_reason: Some(reason),
                });
                continue;
            }
        };

        let mut source_path = compiled_root.join(rel_path);
        let mut target_path = target_path;

        // Check if this is an encrypted file (.age / .sops.*)
        let is_encrypted = crate::secrets::is_encrypted_secret_path(&source_path);

        // For encrypted files, we need to decrypt before applying
        let temp_decrypted = if is_encrypted {
            ui::info(&format!("Decrypting {}", rel_path.display()));

            match crate::secrets::decrypt_to_memory(cfg, &source_path) {
                Ok(decrypted_content) => {
                    // Create temp file with decrypted content (0600)
                    let mut temp = tempfile::NamedTempFile::new()
                        .context("Failed to create temporary file for decrypted content")?;
                    use std::io::Write;
                    temp.write_all(&decrypted_content)?;
                    temp.flush()?;
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        let mut perms = temp.as_file().metadata()?.permissions();
                        perms.set_mode(0o600);
                        temp.as_file().set_permissions(perms)?;
                    }

                    // Update source path to temp file
                    let (file, temp_path) = temp
                        .keep()
                        .context("Failed to persist temporary decrypted file")?;
                    drop(file);

                    // Map encrypted name to plaintext home target
                    let plain = crate::secrets::plain_path_from_encrypted(&target_path);
                    target_path = plain;

                    source_path = temp_path.clone();
                    Some(temp_path)
                }
                Err(e) => {
                    ui::warn(&format!("Failed to decrypt {}: {}", rel_path.display(), e));
                    ui::hint("Skipping encrypted file. Run 'dotdipper secrets init' if needed.");
                    pb.inc(1);
                    actions.push(AppliedAction {
                        mode: AppliedMode::Skipped,
                        target: target_path.clone(),
                        source: compiled_root.join(rel_path),
                        backup_created: false,
                        skipped_reason: Some("Decryption failed".to_string()),
                    });
                    continue;
                }
            }
        } else {
            None
        };

        // Safety check: refuse to operate outside $HOME (defense in depth after resolve)
        if !opts.allow_outside_home && !is_path_within_home(&target_path, &home_dir) {
            pb.inc(1);
            actions.push(AppliedAction {
                mode: AppliedMode::Skipped,
                target: target_path.clone(),
                source: source_path.clone(),
                backup_created: false,
                skipped_reason: Some("Outside $HOME".to_string()),
            });
            if let Some(temp_path) = temp_decrypted {
                let _ = fs::remove_file(temp_path);
            }
            continue;
        }

        // Check for file-specific overrides
        let path_str = format!("~/{}", rel_path.display());
        let file_override = cfg.files.get(&path_str);

        // Check if excluded
        if file_override.is_some_and(|o| o.exclude) {
            pb.inc(1);
            actions.push(AppliedAction {
                mode: AppliedMode::Skipped,
                target: target_path.clone(),
                source: source_path.clone(),
                backup_created: false,
                skipped_reason: Some("Excluded".to_string()),
            });
            if let Some(temp_path) = temp_decrypted {
                let _ = fs::remove_file(temp_path);
            }
            continue;
        }

        // Determine mode (override or default). Encrypted files MUST be copied —
        // never symlink to a temp path that we delete after apply.
        let mode = if is_encrypted {
            RestoreMode::Copy
        } else {
            file_override
                .and_then(|o| o.mode)
                .unwrap_or(cfg.general.default_mode)
        };

        // Apply the file
        let action = apply_file(
            &source_path,
            &target_path,
            mode,
            cfg.general.backup,
            opts.force,
        )?;

        actions.push(action);

        // Clean up temporary decrypted file if it exists (safe: we forced Copy mode)
        if let Some(temp_path) = temp_decrypted {
            let _ = fs::remove_file(temp_path);
        }

        pb.inc(1);
    }

    pb.finish_with_message("Application complete");

    // Print summary
    print_summary(&actions);

    Ok(actions)
}

fn apply_file(
    source: &Path,
    target: &Path,
    mode: RestoreMode,
    backup_enabled: bool,
    force: bool,
) -> Result<AppliedAction> {
    // Check if source exists
    if !source.exists() {
        return Ok(AppliedAction {
            mode: AppliedMode::Skipped,
            target: target.to_path_buf(),
            source: source.to_path_buf(),
            backup_created: false,
            skipped_reason: Some("Source not found".to_string()),
        });
    }

    // Check if we need to do anything (idempotency)
    if is_already_applied(source, target, mode)? {
        return Ok(AppliedAction {
            mode: match mode {
                RestoreMode::Symlink => AppliedMode::Symlinked,
                RestoreMode::Copy => AppliedMode::Copied,
            },
            target: target.to_path_buf(),
            source: source.to_path_buf(),
            backup_created: false,
            skipped_reason: Some("Already applied".to_string()),
        });
    }

    // Handle existing target
    let mut backup_created = false;
    if target.exists() || target.is_symlink() {
        if !force {
            // Prompt user
            if !ui::prompt_confirm(&format!("Overwrite {}?", target.display()), false) {
                return Ok(AppliedAction {
                    mode: AppliedMode::Skipped,
                    target: target.to_path_buf(),
                    source: source.to_path_buf(),
                    backup_created: false,
                    skipped_reason: Some("User declined".to_string()),
                });
            }
        }

        // Create backup if enabled
        if backup_enabled && !target.is_symlink() {
            create_backup(target)?;
            backup_created = true;
        }

        // Remove existing target
        if target.is_dir() && !target.is_symlink() {
            fs::remove_dir_all(target)?;
        } else {
            fs::remove_file(target)?;
        }
    }

    // Ensure parent directory exists
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }

    // Apply based on mode
    let applied_mode = match mode {
        RestoreMode::Symlink => {
            unix_fs::symlink(source, target).with_context(|| {
                format!(
                    "Failed to symlink {} -> {}",
                    source.display(),
                    target.display()
                )
            })?;
            AppliedMode::Symlinked
        }
        RestoreMode::Copy => {
            if source.is_dir() {
                copy_dir_recursive(source, target)?;
            } else {
                copy_file_with_metadata(source, target)?;
            }
            AppliedMode::Copied
        }
    };

    Ok(AppliedAction {
        mode: applied_mode,
        target: target.to_path_buf(),
        source: source.to_path_buf(),
        backup_created,
        skipped_reason: None,
    })
}

fn is_already_applied(source: &Path, target: &Path, mode: RestoreMode) -> Result<bool> {
    if !target.exists() && !target.is_symlink() {
        return Ok(false);
    }

    match mode {
        RestoreMode::Symlink => {
            // Check if target is a symlink pointing to source (canonicalize for
            // profile compat links: compiled/ may be a symlink into profiles/).
            if target.is_symlink() {
                let link_target = fs::read_link(target)?;
                if link_target == source {
                    return Ok(true);
                }
                match (link_target.canonicalize(), source.canonicalize()) {
                    (Ok(a), Ok(b)) => Ok(a == b),
                    _ => Ok(false),
                }
            } else {
                Ok(false)
            }
        }
        RestoreMode::Copy => {
            // For copy mode, check hash to determine if content is the same
            if source.is_file() && target.is_file() {
                let source_hash = crate::hash::hash_file(source)?;
                let target_hash = crate::hash::hash_file(target)?;
                Ok(source_hash.hash == target_hash.hash)
            } else {
                Ok(false)
            }
        }
    }
}

fn create_backup(path: &Path) -> Result<()> {
    let timestamp = Utc::now().format("%Y%m%d-%H%M%S-%3f");
    let mut backup_path = PathBuf::from(format!("{}.bak.{}", path.display(), timestamp));
    let mut suffix = 1u32;
    while backup_path.exists() {
        backup_path = PathBuf::from(format!("{}.bak.{}.{}", path.display(), timestamp, suffix));
        suffix += 1;
    }

    if path.is_dir() {
        // Use fs_extra for directory copying with better control
        let options = fs_extra::dir::CopyOptions::new();
        fs_extra::dir::copy(path, &backup_path, &options)
            .with_context(|| format!("Failed to backup directory {}", path.display()))?;
    } else {
        fs::copy(path, &backup_path)
            .with_context(|| format!("Failed to backup file {}", path.display()))?;
    }

    ui::info(&format!("Backed up to {}", backup_path.display()));
    Ok(())
}

/// Resolve a manifest-relative path under $HOME, rejecting traversal and absolute paths.
fn resolve_home_target(home: &Path, rel: &Path) -> std::result::Result<PathBuf, String> {
    if rel.as_os_str().is_empty() {
        return Err("Empty path".to_string());
    }
    if rel.is_absolute() {
        return Err("Absolute path in manifest".to_string());
    }
    for component in rel.components() {
        match component {
            Component::Normal(_) | Component::CurDir => {}
            Component::ParentDir => {
                return Err("Path traversal (..) rejected".to_string());
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err("Unsafe path component".to_string());
            }
        }
    }
    let target = home.join(rel);
    if !is_path_within_home(&target, home) {
        return Err("Outside $HOME".to_string());
    }
    Ok(target)
}

fn is_path_within_home(target: &Path, home: &Path) -> bool {
    let mut normalized = PathBuf::new();
    for component in target.components() {
        match component {
            Component::RootDir | Component::Prefix(_) => normalized.push(component),
            Component::CurDir => {}
            Component::Normal(s) => normalized.push(s),
            Component::ParentDir => {
                if !normalized.pop() {
                    return false;
                }
            }
        }
    }
    normalized.starts_with(home)
}

fn copy_file_with_metadata(source: &Path, target: &Path) -> Result<()> {
    // fs::copy of a file onto itself (including via a home symlink into compiled/)
    // truncates the file to empty. Empty compiled sources must not wipe real files.
    match super::copy_skip_reason(source, target)? {
        Some(super::CopySkip::SameFile) => return Ok(()),
        Some(super::CopySkip::EmptyOverNonEmpty) => {
            ui::warn(&format!(
                "Refusing to overwrite non-empty {} with empty compiled source",
                target.display()
            ));
            return Ok(());
        }
        None => {}
    }

    // Copy file
    fs::copy(source, target).with_context(|| {
        format!(
            "Failed to copy {} to {}",
            source.display(),
            target.display()
        )
    })?;

    // Copy permissions
    let metadata = source.metadata()?;
    let permissions = metadata.permissions();
    fs::set_permissions(target, permissions)?;

    // Try to preserve modification time (best effort)
    if let Ok(mtime) = metadata.modified() {
        filetime::set_file_mtime(target, filetime::FileTime::from_system_time(mtime))?;
    }

    Ok(())
}

fn copy_dir_recursive(source: &Path, target: &Path) -> Result<()> {
    fs::create_dir_all(target)?;

    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let file_name = entry.file_name();
        let target_path = target.join(&file_name);

        if source_path.is_dir() {
            copy_dir_recursive(&source_path, &target_path)?;
        } else {
            copy_file_with_metadata(&source_path, &target_path)?;
        }
    }

    // Copy directory permissions
    let metadata = source.metadata()?;
    let permissions = metadata.permissions();
    fs::set_permissions(target, permissions)?;

    Ok(())
}

fn print_summary(actions: &[AppliedAction]) {
    ui::section("Application Summary");

    let mut table_rows = Vec::new();
    let mut counts = BTreeMap::new();

    for action in actions {
        let mode_str = action.mode.color_str().to_string();
        let arrow = "→".dimmed().to_string();
        let target = action.target.display().to_string();
        let source = action.source.display().to_string();

        let status = if let Some(ref reason) = action.skipped_reason {
            format!("({})", reason).dimmed().to_string()
        } else if action.backup_created {
            "(backed up)".yellow().to_string()
        } else {
            "".to_string()
        };

        table_rows.push(vec![
            mode_str.clone(),
            format!("{} {} {}", target, arrow, source),
            status,
        ]);

        *counts.entry(action.mode).or_insert(0) += 1;
    }

    // Print count summary
    println!();
    for (mode, count) in counts {
        let mode_str = match mode {
            AppliedMode::Symlinked => "Symlinked".green(),
            AppliedMode::Copied => "Copied".blue(),
            AppliedMode::Skipped => "Skipped".dimmed(),
        };
        println!("{}: {}", mode_str, count);
    }

    // Print detailed table if not too long
    if table_rows.len() <= 20 {
        println!();
        ui::print_table(&["Mode", "Path", "Status"], table_rows);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_home_target_rejects_traversal() {
        let home = PathBuf::from("/home/user");
        let err = resolve_home_target(&home, Path::new("../../etc/passwd")).unwrap_err();
        assert!(err.contains("traversal") || err.contains("Outside"));
    }

    #[test]
    fn resolve_home_target_rejects_absolute() {
        let home = PathBuf::from("/home/user");
        let err = resolve_home_target(&home, Path::new("/etc/passwd")).unwrap_err();
        assert!(err.contains("Absolute"));
    }

    #[test]
    fn resolve_home_target_accepts_nested() {
        let home = PathBuf::from("/home/user");
        let target = resolve_home_target(&home, Path::new(".config/app/settings.toml")).unwrap();
        assert_eq!(
            target,
            PathBuf::from("/home/user/.config/app/settings.toml")
        );
    }

    #[test]
    fn is_path_within_home_normalizes_parent_dirs() {
        let home = PathBuf::from("/home/user");
        assert!(!is_path_within_home(
            Path::new("/home/user/../../etc/passwd"),
            &home
        ));
        assert!(is_path_within_home(Path::new("/home/user/.zshrc"), &home));
    }
}
