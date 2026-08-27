pub mod apply;

use anyhow::{Context, Result};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::cfg::Config;
use crate::hash::{hash_file, Manifest};
use crate::ui;

/// Files that live in `compiled/` but are not user dotfiles.
pub(crate) fn is_store_metadata(rel_path: &Path) -> bool {
    let s = rel_path.to_string_lossy();
    s == "manifest.lock"
        || s == ".gitignore"
        || s == "Brewfile"
        || s == "apps_manifest.toml"
        || s.starts_with(".git/")
        || s == ".git"
        || s.starts_with(".dotdipper/")
}

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
    let home = dirs::home_dir().context("Failed to find home directory")?;
    let repo_path = get_compiled_path()?;
    fs::create_dir_all(&repo_path)?;

    // Hash tracked home files. Encrypted store names that are missing from $HOME
    // (common after pull→apply decrypt) are skipped here and preserved from compiled/.
    let mut hashes = Vec::new();
    let pb_hash = ui::progress_bar(tracked_files.len() as u64, "Hashing files");
    for path in tracked_files {
        let rel = path.strip_prefix(&home).unwrap_or(path);
        if path.exists() {
            // Skip non-regular files (sockets, fifos, devices) that can appear
            // in tracked lists written by older discover runs.
            if !path.is_file() {
                ui::warn(&format!(
                    "Skipping non-regular file {} (socket/fifo/device)",
                    path.display()
                ));
                pb_hash.inc(1);
                continue;
            }
            // Never snapshot decrypted plaintext over an encrypted store entry
            if compiled_has_encrypted_for_plain(&repo_path, rel) {
                ui::warn(&format!(
                    "Skipping plaintext {} — encrypted copy already in compiled store",
                    path.display()
                ));
                pb_hash.inc(1);
                continue;
            }
            let hashed = crate::hash::hash_file(path).with_context(|| {
                format!(
                    "Failed to hash tracked file {}. \
                     Check that the path exists and is readable (expand ~ if needed).",
                    path.display()
                )
            })?;
            if hashed.size == 0 {
                ui::warn(&format!(
                    "Tracked file {} is empty (0 bytes)",
                    path.display()
                ));
            }
            hashes.push(hashed);
        } else if crate::secrets::is_encrypted_secret_path(rel) {
            // Legacy/bad sync put encrypted names into tracked_files; keep store copy
            ui::hint(&format!(
                "Tracked encrypted path {} missing from $HOME; preserving compiled copy",
                rel.display()
            ));
        } else {
            anyhow::bail!(
                "Failed to hash tracked file {}: file not found. \
                 Check that the path exists and is readable (expand ~ if needed).",
                path.display()
            );
        }
        pb_hash.inc(1);
    }
    pb_hash.finish_with_message("Hashing complete");

    // Copy files to repo and add to manifest
    let pb = ui::progress_bar(hashes.len() as u64, "Creating snapshot");

    for file_hash in hashes {
        let rel_path = file_hash
            .path
            .strip_prefix(&home)
            .unwrap_or(&file_hash.path);

        let dest_path = repo_path.join(rel_path);
        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent)?;
        }

        copy_file_with_permissions(&file_hash.path, &dest_path)?;

        // Hash the compiled dest, not the home source. If we skipped an empty
        // home file over a non-empty store copy, the dest still has real bytes.
        let mut stored = hash_file(&dest_path)
            .with_context(|| format!("Failed to hash compiled copy {}", dest_path.display()))?;
        stored.path = rel_path.to_path_buf();
        manifest.add_file(stored);

        pb.inc(1);
    }

    pb.finish_with_message("Snapshot created");

    // Keep encrypted blobs already in the store so consumer machines can push
    // without re-hashing missing ~/file.age paths after decrypt-on-apply.
    preserve_encrypted_store_entries(&mut manifest, &repo_path)?;

    // Save manifest outside and inside the git store so pull/apply work on new machines
    manifest.save(&manifest_path)?;
    save_manifest_into_compiled(&manifest)?;

    write_push_gitignore(&repo_path, config)?;

    Ok(Snapshot {
        file_count: manifest.files.len(),
    })
}

/// Load the active manifest, preferring the synced copy inside `compiled/` when present.
pub fn load_manifest() -> Result<Manifest> {
    let base = get_manifest_path()?;
    let compiled_manifest = get_compiled_path()?.join("manifest.lock");

    if compiled_manifest.exists() {
        let manifest = Manifest::load(&compiled_manifest)?;
        // Keep the base path in sync for tools that still read it directly
        if let Some(parent) = base.parent() {
            fs::create_dir_all(parent)?;
        }
        manifest.save(&base)?;
        return Ok(manifest);
    }

    if base.exists() {
        let manifest = Manifest::load(&base)?;
        // Backfill into compiled/ so future pushes include it
        let _ = save_manifest_into_compiled(&manifest);
        return Ok(manifest);
    }

    anyhow::bail!("Manifest not found. Run 'dotdipper pull' or 'dotdipper snapshot' first.")
}

/// After a git pull/clone, ensure `manifest.lock` is available for apply/install.
/// Rebuilds from compiled files when the remote predates manifest syncing.
pub fn sync_manifest_from_compiled() -> Result<Manifest> {
    let compiled = get_compiled_path()?;
    let compiled_manifest = compiled.join("manifest.lock");
    let base = get_manifest_path()?;

    if compiled_manifest.exists() {
        let manifest = Manifest::load(&compiled_manifest)?;
        if let Some(parent) = base.parent() {
            fs::create_dir_all(parent)?;
        }
        manifest.save(&base)?;
        ui::info("Synced manifest.lock from pulled repository");
        return Ok(manifest);
    }

    if !compiled.exists() {
        anyhow::bail!("Compiled directory not found at {}", compiled.display());
    }

    ui::info("No manifest.lock in repository; rebuilding from compiled files...");
    let manifest = rebuild_manifest_from_compiled()?;
    ui::success(&format!(
        "Rebuilt manifest with {} files",
        manifest.files.len()
    ));
    Ok(manifest)
}

/// Hash every real dotfile under `compiled/` (skipping git/metadata) and write the manifest.
pub fn rebuild_manifest_from_compiled() -> Result<Manifest> {
    let compiled = get_compiled_path()?;
    let mut manifest = Manifest::new();

    if compiled.exists() {
        for entry in walkdir::WalkDir::new(&compiled)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if !entry.file_type().is_file() {
                continue;
            }
            let Ok(rel_path) = entry.path().strip_prefix(&compiled) else {
                continue;
            };
            if is_store_metadata(rel_path) {
                continue;
            }

            let mut file_hash = hash_file(entry.path())?;
            file_hash.path = rel_path.to_path_buf();
            manifest.add_file(file_hash);
        }
    }

    let base = get_manifest_path()?;
    if let Some(parent) = base.parent() {
        fs::create_dir_all(parent)?;
    }
    manifest.save(&base)?;
    save_manifest_into_compiled(&manifest)?;
    Ok(manifest)
}

fn save_manifest_into_compiled(manifest: &Manifest) -> Result<()> {
    let compiled = get_compiled_path()?;
    fs::create_dir_all(&compiled)?;
    manifest.save(&compiled.join("manifest.lock"))
}

/// Update config `tracked_files` from a pulled/rebuilt manifest so install/discover work.
/// Encrypted store names (`.age` / `.sops.*`) are skipped — after apply they decrypt to a
/// plaintext home path, and tracking the encrypted name breaks consumer `snapshot`/`push`.
pub fn sync_tracked_files_from_manifest(
    config_path: &Path,
    manifest: &Manifest,
) -> Result<Vec<PathBuf>> {
    let home = dirs::home_dir().context("Failed to find home directory")?;
    let files: Vec<PathBuf> = manifest
        .files
        .keys()
        .filter(|rel| !crate::secrets::is_encrypted_secret_path(rel))
        .map(|rel| home.join(rel))
        .collect();

    if !files.is_empty() {
        crate::cfg::update_discovered(config_path, &files)?;
        ui::info(&format!(
            "Updated tracked_files from manifest ({} paths)",
            files.len()
        ));
    }

    Ok(files)
}

fn compiled_has_encrypted_for_plain(compiled: &Path, plain_rel: &Path) -> bool {
    let Some(name) = plain_rel.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let parent = plain_rel.parent().unwrap_or_else(|| Path::new(""));
    let candidates = [
        format!("{name}.age"),
        format!("{name}.sops"),
        // file.yaml → file.sops.yaml
        {
            if let Some(dot) = name.rfind('.') {
                let (stem, ext) = name.split_at(dot);
                format!("{stem}.sops{ext}")
            } else {
                String::new()
            }
        },
    ];
    candidates.iter().any(|c| {
        if c.is_empty() {
            return false;
        }
        compiled.join(parent).join(c).is_file()
    })
}

fn preserve_encrypted_store_entries(manifest: &mut Manifest, compiled: &Path) -> Result<()> {
    if !compiled.exists() {
        return Ok(());
    }

    for entry in walkdir::WalkDir::new(compiled)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let Ok(rel) = entry.path().strip_prefix(compiled) else {
            continue;
        };
        if is_store_metadata(rel) {
            continue;
        }
        if !crate::secrets::is_encrypted_secret_path(rel) {
            continue;
        }
        if manifest.has_file(rel) {
            continue;
        }
        let mut fh = hash_file(entry.path())?;
        fh.path = rel.to_path_buf();
        manifest.add_file(fh);
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

    // Check tracked files
    for file_path in &config.general.tracked_files {
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
    let tracked: HashSet<&PathBuf> = config.general.tracked_files.iter().collect();
    for rel_path in manifest.files.keys() {
        let full_path = home.join(rel_path);
        if !tracked.contains(&full_path) {
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
        let preview: Vec<String> = invalid_files
            .iter()
            .take(5)
            .map(|p| p.display().to_string())
            .collect();
        anyhow::bail!(
            "Manifest verification failed for {} files (e.g. {}). \
             Entries are relative to $HOME; re-run 'dotdipper push' if this machine drifted.",
            invalid_files.len(),
            preview.join(", ")
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

/// True when `a` and `b` resolve to the same inode (symlinks followed).
pub(crate) fn paths_resolve_to_same_file(a: &Path, b: &Path) -> Result<bool> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;

        let (a_meta, b_meta) = match (a.metadata(), b.metadata()) {
            (Ok(am), Ok(bm)) => (am, bm),
            _ => return Ok(false),
        };
        Ok(a_meta.dev() == b_meta.dev() && a_meta.ino() == b_meta.ino())
    }

    #[cfg(not(unix))]
    {
        match (a.canonicalize(), b.canonicalize()) {
            (Ok(ac), Ok(bc)) => Ok(ac == bc),
            _ => Ok(false),
        }
    }
}

/// Why a copy should be skipped to avoid emptying the destination.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CopySkip {
    SameFile,
    EmptyOverNonEmpty,
}

/// Skip `fs::copy` when it would truncate (same inode) or replace real content with empty.
pub(crate) fn copy_skip_reason(source: &Path, dest: &Path) -> Result<Option<CopySkip>> {
    if !dest.exists() {
        return Ok(None);
    }
    if paths_resolve_to_same_file(source, dest)? {
        return Ok(Some(CopySkip::SameFile));
    }
    let src_len = fs::metadata(source).map(|m| m.len()).unwrap_or(0);
    let dst_len = fs::metadata(dest).map(|m| m.len()).unwrap_or(0);
    if src_len == 0 && dst_len > 0 {
        return Ok(Some(CopySkip::EmptyOverNonEmpty));
    }
    Ok(None)
}

fn copy_file_with_permissions(source: &Path, dest: &Path) -> Result<()> {
    match copy_skip_reason(source, dest)? {
        Some(CopySkip::SameFile) => {
            ui::hint(&format!(
                "Skipping copy of {} — already the compiled file (symlink restore)",
                source.display()
            ));
            return Ok(());
        }
        Some(CopySkip::EmptyOverNonEmpty) => {
            ui::warn(&format!(
                "Refusing to overwrite non-empty {} with empty {}",
                dest.display(),
                source.display()
            ));
            return Ok(());
        }
        None => {}
    }

    fs::copy(source, dest)
        .with_context(|| format!("Failed to copy {} -> {}", source.display(), dest.display()))?;

    #[cfg(unix)]
    {
        let metadata = source.metadata()?;
        let permissions = metadata.permissions();
        fs::set_permissions(dest, permissions)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use tempfile::TempDir;

    #[test]
    #[serial]
    fn rebuild_manifest_skips_git_and_metadata() {
        let temp = TempDir::new().unwrap();
        let home = temp.path();
        let base = home.join(".config").join("dotdipper");
        let compiled = base.join("compiled");
        fs::create_dir_all(compiled.join(".git").join("objects")).unwrap();
        fs::write(compiled.join(".git").join("HEAD"), "ref: refs/heads/main\n").unwrap();
        fs::write(compiled.join(".gitignore"), "*.tmp\n").unwrap();
        fs::write(compiled.join("Brewfile"), "brew \"git\"\n").unwrap();
        fs::write(
            compiled.join("apps_manifest.toml"),
            "[meta]\nos = \"macos\"\n",
        )
        .unwrap();
        fs::write(compiled.join(".zshrc"), "export Z=1\n").unwrap();
        fs::create_dir_all(compiled.join(".config")).unwrap();
        fs::write(compiled.join(".config").join("app.conf"), "a=1\n").unwrap();

        std::env::set_var("HOME", home);
        std::env::set_var("DOTDIPPER_HOME", &base);
        std::env::remove_var("DOTDIPPER_PROFILE");
        std::env::remove_var("XDG_CONFIG_HOME");

        let manifest = rebuild_manifest_from_compiled().unwrap();
        assert!(manifest.has_file(Path::new(".zshrc")));
        assert!(manifest.has_file(Path::new(".config/app.conf")));
        assert!(!manifest.has_file(Path::new("manifest.lock")));
        assert!(!manifest.has_file(Path::new(".gitignore")));
        assert!(!manifest.has_file(Path::new("Brewfile")));
        assert!(!manifest.has_file(Path::new("apps_manifest.toml")));
        assert!(!manifest.files.keys().any(|p| {
            p.components()
                .next()
                .map(|c| c.as_os_str() == ".git")
                .unwrap_or(false)
        }));
        assert!(base.join("manifest.lock").exists());
        assert!(compiled.join("manifest.lock").exists());
    }

    #[cfg(unix)]
    #[test]
    #[serial]
    fn copy_file_with_permissions_skips_same_inode_via_symlink() {
        use std::os::unix::fs as unix_fs;

        let temp = TempDir::new().unwrap();
        let home = temp.path();
        let base = home.join(".config").join("dotdipper");
        let compiled = base.join("compiled");
        fs::create_dir_all(&compiled).unwrap();

        let content = "export ZSH_THEME=robbyrussell\n";
        let compiled_file = compiled.join(".zshrc");
        fs::write(&compiled_file, content).unwrap();

        let home_file = home.join(".zshrc");
        unix_fs::symlink(&compiled_file, &home_file).unwrap();

        std::env::set_var("HOME", home);
        std::env::set_var("DOTDIPPER_HOME", &base);
        std::env::remove_var("DOTDIPPER_PROFILE");
        std::env::remove_var("XDG_CONFIG_HOME");

        copy_file_with_permissions(&home_file, &compiled_file).unwrap();

        assert_eq!(
            fs::read_to_string(&compiled_file).unwrap(),
            content,
            "must not truncate compiled file when home path is a symlink to it"
        );
    }

    #[test]
    #[serial]
    fn copy_file_with_permissions_copies_distinct_files() {
        let temp = TempDir::new().unwrap();
        let source = temp.path().join("source.txt");
        let dest = temp.path().join("dest.txt");

        fs::write(&source, "distinct content\n").unwrap();
        copy_file_with_permissions(&source, &dest).unwrap();

        assert_eq!(fs::read_to_string(&dest).unwrap(), "distinct content\n");
    }

    #[test]
    #[serial]
    fn copy_file_with_permissions_refuses_empty_over_nonempty() {
        let temp = TempDir::new().unwrap();
        let source = temp.path().join("empty.txt");
        let dest = temp.path().join("kept.txt");

        fs::write(&source, "").unwrap();
        fs::write(&dest, "keep me\n").unwrap();
        copy_file_with_permissions(&source, &dest).unwrap();

        assert_eq!(
            fs::read_to_string(&dest).unwrap(),
            "keep me\n",
            "must not replace a non-empty dest with an empty source"
        );
    }

    #[test]
    #[serial]
    fn snapshot_manifest_uses_compiled_hash_when_home_is_empty() {
        let temp = TempDir::new().unwrap();
        let home = temp.path();
        let base = home.join(".config").join("dotdipper");
        let compiled = base.join("compiled");
        fs::create_dir_all(&compiled).unwrap();

        let kept = "export KEEP=1\n";
        fs::write(compiled.join(".zshrc"), kept).unwrap();
        fs::write(home.join(".zshrc"), "").unwrap();

        std::env::set_var("HOME", home);
        std::env::set_var("DOTDIPPER_HOME", &base);
        std::env::remove_var("DOTDIPPER_PROFILE");
        std::env::remove_var("XDG_CONFIG_HOME");

        let mut config = Config::default();
        config.general.tracked_files = vec![home.join(".zshrc")];
        snapshot(&config, true).unwrap();

        assert_eq!(fs::read_to_string(compiled.join(".zshrc")).unwrap(), kept);
        let manifest = Manifest::load(&compiled.join("manifest.lock")).unwrap();
        let stored = manifest.get_file(Path::new(".zshrc")).unwrap();
        let dest_hash = hash_file(&compiled.join(".zshrc")).unwrap();
        assert_eq!(
            stored.hash, dest_hash.hash,
            "manifest must record compiled bytes, not the empty home file"
        );
        assert_eq!(stored.size, kept.len() as u64);
    }

    #[test]
    #[serial]
    fn sync_manifest_prefers_compiled_copy() {
        let temp = TempDir::new().unwrap();
        let home = temp.path();
        let base = home.join(".config").join("dotdipper");
        let compiled = base.join("compiled");
        fs::create_dir_all(&compiled).unwrap();
        fs::write(compiled.join(".a"), "a\n").unwrap();

        std::env::set_var("HOME", home);
        std::env::set_var("DOTDIPPER_HOME", &base);
        std::env::remove_var("DOTDIPPER_PROFILE");
        std::env::remove_var("XDG_CONFIG_HOME");

        let mut manifest = Manifest::new();
        let mut fh = hash_file(&compiled.join(".a")).unwrap();
        fh.path = PathBuf::from(".a");
        manifest.add_file(fh);
        manifest.save(&compiled.join("manifest.lock")).unwrap();

        // Stale/missing base path should be repaired from compiled/
        let loaded = sync_manifest_from_compiled().unwrap();
        assert!(loaded.has_file(Path::new(".a")));
        assert!(base.join("manifest.lock").exists());
    }
}
