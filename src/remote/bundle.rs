/// Bundle creation and extraction for remote storage
/// 
/// Creates .tar.zst archives containing:
/// - compiled/ directory
/// - manifest.lock
/// - meta.json (profile name, timestamp, host, version)

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use chrono::Utc;
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleMeta {
    pub profile_name: String,
    pub timestamp: String,
    pub hostname: String,
    pub dotdipper_version: String,
    pub file_count: usize,
    pub size_bytes: u64,
}

/// Pack compiled/ and manifest into a bundle
pub fn pack(
    compiled_root: &Path,
    manifest_path: &Path,
    output_bundle: &Path,
    profile_name: &str,
) -> Result<BundleMeta> {
    if !compiled_root.exists() {
        anyhow::bail!("Compiled directory does not exist: {}", compiled_root.display());
    }
    
    if !manifest_path.exists() {
        anyhow::bail!("Manifest does not exist: {}", manifest_path.display());
    }
    
    // Create bundle metadata
    let hostname = hostname::get()
        .ok()
        .and_then(|h| h.into_string().ok())
        .unwrap_or_else(|| "unknown".to_string());
    
    let (file_count, size_bytes) = count_files_and_size(compiled_root)?;
    
    let meta = BundleMeta {
        profile_name: profile_name.to_string(),
        timestamp: Utc::now().to_rfc3339(),
        hostname,
        dotdipper_version: env!("CARGO_PKG_VERSION").to_string(),
        file_count,
        size_bytes,
    };
    
    // Create temp directory for bundle contents
    let temp_dir = tempfile::tempdir()?;
    let bundle_root = temp_dir.path().join("dotdipper_bundle");
    fs::create_dir_all(&bundle_root)?;
    
    // Copy compiled/ to bundle
    let bundle_compiled = bundle_root.join("compiled");
    copy_dir_recursive(compiled_root, &bundle_compiled)?;
    
    // Copy manifest
    fs::copy(manifest_path, bundle_root.join("manifest.lock"))?;
    
    // Write meta.json
    let meta_json = serde_json::to_string_pretty(&meta)?;
    fs::write(bundle_root.join("meta.json"), meta_json)?;
    
    // Create tar.zst archive
    let tar_gz = File::create(output_bundle)?;
    let encoder = zstd::Encoder::new(tar_gz, 3)?; // Compression level 3
    let mut tar = tar::Builder::new(encoder);
    
    // Add bundle contents to tar
    tar.append_dir_all("", &bundle_root)?;
    
    let encoder = tar.into_inner()?;
    encoder.finish()?;
    
    Ok(meta)
}

/// Unpack a bundle to destination
pub fn unpack(bundle_path: &Path, _dest_dir: &Path) -> Result<BundleMeta> {
    if !bundle_path.exists() {
        anyhow::bail!("Bundle does not exist: {}", bundle_path.display());
    }
    
    // Create temp extraction directory
    let temp_dir = tempfile::tempdir()?;
    let extract_root = temp_dir.path();
    
    // Extract tar.zst
    let tar_file = File::open(bundle_path)?;
    let decoder = zstd::Decoder::new(tar_file)?;
    let mut archive = tar::Archive::new(decoder);
    archive.unpack(extract_root)?;
    
    // Find bundle root (may be nested)
    let bundle_root = find_bundle_root(extract_root)?;
    
    // Read meta.json
    let meta_path = bundle_root.join("meta.json");
    if !meta_path.exists() {
        anyhow::bail!("Bundle is missing meta.json");
    }
    
    let meta_content = fs::read_to_string(&meta_path)?;
    let meta: BundleMeta = serde_json::from_str(&meta_content)?;
    
    // Get profile paths
    let profile_paths = crate::profiles::profile_paths(&meta.profile_name)?;
    
    // Copy compiled/ to profile
    let src_compiled = bundle_root.join("compiled");
    if src_compiled.exists() {
        if profile_paths.compiled.exists() {
            // Backup existing
            let backup = profile_paths.compiled.with_extension("compiled.backup");
            fs::rename(&profile_paths.compiled, &backup)?;
        }
        
        copy_dir_recursive(&src_compiled, &profile_paths.compiled)?;
    }
    
    // Copy manifest
    let src_manifest = bundle_root.join("manifest.lock");
    if src_manifest.exists() {
        fs::copy(&src_manifest, &profile_paths.manifest)?;
    }
    
    Ok(meta)
}

fn find_bundle_root(extract_root: &Path) -> Result<PathBuf> {
    // Check if extract_root itself is the bundle root
    if extract_root.join("meta.json").exists() {
        return Ok(extract_root.to_path_buf());
    }
    
    // Search one level deep
    for entry in fs::read_dir(extract_root)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() && path.join("meta.json").exists() {
            return Ok(path);
        }
    }
    
    anyhow::bail!("Could not find bundle root in extracted archive");
}

fn copy_dir_recursive(src: &Path, dest: &Path) -> Result<()> {
    fs::create_dir_all(dest)?;
    
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let dest_path = dest.join(entry.file_name());
        
        if path.is_dir() {
            copy_dir_recursive(&path, &dest_path)?;
        } else if path.is_file() {
            fs::copy(&path, &dest_path)?;
            
            // Preserve mtime
            let metadata = entry.metadata()?;
            let mtime = filetime::FileTime::from_last_modification_time(&metadata);
            filetime::set_file_mtime(&dest_path, mtime)?;
        } else if path.is_symlink() {
            // Preserve symlinks
            let target = fs::read_link(&path)?;
            #[cfg(unix)]
            std::os::unix::fs::symlink(&target, &dest_path)?;
        }
    }
    
    Ok(())
}

fn count_files_and_size(dir: &Path) -> Result<(usize, u64)> {
    let mut count = 0;
    let mut size = 0u64;
    
    for entry in WalkDir::new(dir) {
        let entry = entry?;
        if entry.file_type().is_file() {
            count += 1;
            size += entry.metadata()?.len();
        }
    }
    
    Ok((count, size))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    
    #[test]
    fn test_bundle_meta_serialization() {
        let meta = BundleMeta {
            profile_name: "default".to_string(),
            timestamp: "2025-10-02T14:33:12Z".to_string(),
            hostname: "testhost".to_string(),
            dotdipper_version: "0.1.0".to_string(),
            file_count: 10,
            size_bytes: 1024,
        };
        
        let json = serde_json::to_string(&meta).unwrap();
        let parsed: BundleMeta = serde_json::from_str(&json).unwrap();
        
        assert_eq!(parsed.profile_name, "default");
        assert_eq!(parsed.file_count, 10);
    }
}

