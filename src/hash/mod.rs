use anyhow::{Context, Result};
use blake3::Hasher;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileHash {
    pub path: PathBuf,
    pub hash: String,
    pub size: u64,
    pub mode: u32,
    pub modified: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub version: String,
    pub created: DateTime<Utc>,
    pub files: HashMap<PathBuf, FileHash>,
}

impl Manifest {
    pub fn new() -> Self {
        Manifest {
            version: "1.0.0".to_string(),
            created: Utc::now(),
            files: HashMap::new(),
        }
    }

    pub fn load(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read manifest from {}", path.display()))?;
        let manifest: Manifest = serde_json::from_str(&content)
            .context("Failed to parse manifest JSON")?;
        Ok(manifest)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let content = serde_json::to_string_pretty(self)
            .context("Failed to serialize manifest")?;
        fs::write(path, content)
            .with_context(|| format!("Failed to write manifest to {}", path.display()))?;
        Ok(())
    }

    pub fn add_file(&mut self, file_hash: FileHash) {
        self.files.insert(file_hash.path.clone(), file_hash);
    }

    pub fn get_file(&self, path: &Path) -> Option<&FileHash> {
        self.files.get(path)
    }

    pub fn has_file(&self, path: &Path) -> bool {
        self.files.contains_key(path)
    }

    pub fn remove_file(&mut self, path: &Path) -> Option<FileHash> {
        self.files.remove(path)
    }

    pub fn diff(&self, other: &Manifest) -> ManifestDiff {
        let mut diff = ManifestDiff::default();

        // Check for modified and deleted files
        for (path, hash) in &self.files {
            match other.files.get(path) {
                Some(other_hash) => {
                    if hash.hash != other_hash.hash {
                        diff.modified.push(path.clone());
                    }
                }
                None => diff.deleted.push(path.clone()),
            }
        }

        // Check for added files
        for path in other.files.keys() {
            if !self.files.contains_key(path) {
                diff.added.push(path.clone());
            }
        }

        diff
    }
}

#[derive(Debug, Default)]
pub struct ManifestDiff {
    pub added: Vec<PathBuf>,
    pub modified: Vec<PathBuf>,
    pub deleted: Vec<PathBuf>,
}

impl ManifestDiff {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.modified.is_empty() && self.deleted.is_empty()
    }

    pub fn print_summary(&self) {
        if !self.added.is_empty() {
            println!("Added files: {}", self.added.len());
        }
        if !self.modified.is_empty() {
            println!("Modified files: {}", self.modified.len());
        }
        if !self.deleted.is_empty() {
            println!("Deleted files: {}", self.deleted.len());
        }
    }
}

pub fn hash_file(path: &Path) -> Result<FileHash> {
    let file = File::open(path)
        .with_context(|| format!("Failed to open file: {}", path.display()))?;
    
    let metadata = file.metadata()
        .with_context(|| format!("Failed to get metadata for: {}", path.display()))?;
    
    let mut reader = BufReader::new(file);
    let mut hasher = Hasher::new();
    let mut buffer = [0; 8192];
    
    loop {
        let bytes_read = reader.read(&mut buffer)
            .with_context(|| format!("Failed to read file: {}", path.display()))?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }
    
    let hash = hasher.finalize();
    let modified = metadata.modified()
        .context("Failed to get modification time")?;
    
    Ok(FileHash {
        path: path.to_path_buf(),
        hash: hash.to_hex().to_string(),
        size: metadata.len(),
        mode: get_file_mode(&metadata),
        modified: DateTime::from(modified),
    })
}

pub fn hash_files(paths: &[PathBuf], progress: bool) -> Result<Vec<FileHash>> {
    let mut hashes = Vec::new();
    
    let pb = if progress {
        Some(crate::ui::progress_bar(paths.len() as u64, "Hashing files"))
    } else {
        None
    };
    
    for path in paths {
        if let Ok(hash) = hash_file(path) {
            hashes.push(hash);
        }
        if let Some(ref pb) = pb {
            pb.inc(1);
        }
    }
    
    if let Some(pb) = pb {
        pb.finish_with_message("Hashing complete");
    }
    
    Ok(hashes)
}

pub fn verify_file(file_hash: &FileHash) -> Result<bool> {
    if !file_hash.path.exists() {
        return Ok(false);
    }
    
    let current_hash = hash_file(&file_hash.path)?;
    Ok(current_hash.hash == file_hash.hash)
}

pub fn verify_manifest(manifest: &Manifest) -> Result<Vec<PathBuf>> {
    let mut invalid_files = Vec::new();
    
    for (path, file_hash) in &manifest.files {
        if !verify_file(file_hash)? {
            invalid_files.push(path.clone());
        }
    }
    
    Ok(invalid_files)
}

#[cfg(unix)]
fn get_file_mode(metadata: &std::fs::Metadata) -> u32 {
    use std::os::unix::fs::MetadataExt;
    metadata.mode()
}

#[cfg(not(unix))]
fn get_file_mode(_metadata: &std::fs::Metadata) -> u32 {
    0o644  // Default permissions for non-Unix systems
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_hash_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        
        let mut file = File::create(&file_path).unwrap();
        file.write_all(b"Hello, world!").unwrap();
        drop(file);
        
        let hash = hash_file(&file_path).unwrap();
        assert_eq!(hash.size, 13);
        assert!(!hash.hash.is_empty());
    }

    #[test]
    fn test_manifest_diff() {
        let mut manifest1 = Manifest::new();
        let mut manifest2 = Manifest::new();
        
        let hash1 = FileHash {
            path: PathBuf::from("/test/file1.txt"),
            hash: "hash1".to_string(),
            size: 100,
            mode: 0o644,
            modified: Utc::now(),
        };
        
        let hash2 = FileHash {
            path: PathBuf::from("/test/file2.txt"),
            hash: "hash2".to_string(),
            size: 200,
            mode: 0o644,
            modified: Utc::now(),
        };
        
        let hash1_modified = FileHash {
            path: PathBuf::from("/test/file1.txt"),
            hash: "hash1_modified".to_string(),
            size: 150,
            mode: 0o644,
            modified: Utc::now(),
        };
        
        manifest1.add_file(hash1.clone());
        manifest2.add_file(hash1_modified);
        manifest2.add_file(hash2);
        
        let diff = manifest1.diff(&manifest2);
        
        assert_eq!(diff.modified.len(), 1);
        assert_eq!(diff.added.len(), 1);
        assert_eq!(diff.deleted.len(), 0);
    }
}
