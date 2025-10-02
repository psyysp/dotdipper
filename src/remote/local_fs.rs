/// LocalFS remote backend - stores bundles in a local directory
/// Useful for testing and local backups

use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

use super::{Remote, RemoteObject};

pub struct LocalFsRemote {
    storage_dir: PathBuf,
}

impl LocalFsRemote {
    pub fn new(storage_dir: &str) -> Result<Self> {
        let expanded = shellexpand::tilde(storage_dir);
        let path = PathBuf::from(expanded.as_ref());
        
        fs::create_dir_all(&path)
            .with_context(|| format!("Failed to create storage directory: {}", path.display()))?;
        
        Ok(Self { storage_dir: path })
    }
}

impl Remote for LocalFsRemote {
    fn name(&self) -> &str {
        "LocalFS"
    }
    
    fn push_bundle(&self, bundle_path: &Path) -> Result<RemoteObject> {
        let filename = bundle_path.file_name()
            .context("Invalid bundle path")?;
        
        let dest_path = self.storage_dir.join(filename);
        
        // Copy bundle to storage
        fs::copy(bundle_path, &dest_path)
            .with_context(|| format!("Failed to copy bundle to {}", dest_path.display()))?;
        
        let metadata = fs::metadata(&dest_path)?;
        
        Ok(RemoteObject {
            etag_or_rev: format!("local:{}", dest_path.display()),
            size_bytes: metadata.len(),
        })
    }
    
    fn pull_latest(&self, dest_bundle: &Path) -> Result<RemoteObject> {
        // Find latest bundle in storage dir
        let mut bundles: Vec<PathBuf> = Vec::new();
        
        for entry in fs::read_dir(&self.storage_dir)? {
            let entry = entry?;
            let path = entry.path();
            
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    if ext == "zst" || ext == "tar" {
                        bundles.push(path);
                    }
                }
            }
        }
        
        if bundles.is_empty() {
            anyhow::bail!("No bundles found in {}", self.storage_dir.display());
        }
        
        // Sort by modification time, newest first
        bundles.sort_by_key(|p| {
            fs::metadata(p)
                .and_then(|m| m.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
        });
        bundles.reverse();
        
        let latest = &bundles[0];
        
        // Copy to destination
        fs::copy(latest, dest_bundle)
            .with_context(|| format!("Failed to copy bundle from {}", latest.display()))?;
        
        let metadata = fs::metadata(dest_bundle)?;
        
        Ok(RemoteObject {
            etag_or_rev: format!("local:{}", latest.display()),
            size_bytes: metadata.len(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    
    #[test]
    fn test_local_fs_push_pull() {
        let temp_storage = tempfile::tempdir().unwrap();
        let temp_bundle = tempfile::tempdir().unwrap();
        
        let remote = LocalFsRemote::new(temp_storage.path().to_str().unwrap()).unwrap();
        
        // Create test bundle
        let bundle_path = temp_bundle.path().join("test.tar.zst");
        let mut file = fs::File::create(&bundle_path).unwrap();
        file.write_all(b"test bundle content").unwrap();
        drop(file);
        
        // Push
        let obj = remote.push_bundle(&bundle_path).unwrap();
        assert!(obj.size_bytes > 0);
        
        // Pull
        let download_path = temp_bundle.path().join("downloaded.tar.zst");
        let obj2 = remote.pull_latest(&download_path).unwrap();
        assert!(obj2.size_bytes > 0);
        assert!(download_path.exists());
    }
}

