//! Integration tests for the bundle module

use serde::{Deserialize, Serialize};
use std::fs;
use tempfile::TempDir;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BundleMeta {
    profile_name: String,
    timestamp: String,
    hostname: String,
    dotdipper_version: String,
    file_count: usize,
    size_bytes: u64,
}

#[cfg(test)]
mod bundle_meta_tests {
    use super::*;

    #[test]
    fn test_bundle_meta_creation() {
        let meta = BundleMeta {
            profile_name: "default".to_string(),
            timestamp: "2025-01-20T12:00:00Z".to_string(),
            hostname: "testhost".to_string(),
            dotdipper_version: "0.3.0".to_string(),
            file_count: 42,
            size_bytes: 102400,
        };

        assert_eq!(meta.profile_name, "default");
        assert_eq!(meta.file_count, 42);
        assert_eq!(meta.size_bytes, 102400);
    }

    #[test]
    fn test_bundle_meta_serialization() {
        let meta = BundleMeta {
            profile_name: "work".to_string(),
            timestamp: "2025-01-20T14:30:00Z".to_string(),
            hostname: "workstation".to_string(),
            dotdipper_version: "0.3.0".to_string(),
            file_count: 100,
            size_bytes: 2048000,
        };

        let json = serde_json::to_string_pretty(&meta).unwrap();

        assert!(json.contains("work"));
        assert!(json.contains("workstation"));
        assert!(json.contains("100"));

        let deserialized: BundleMeta = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.profile_name, meta.profile_name);
        assert_eq!(deserialized.file_count, meta.file_count);
    }

    #[test]
    fn test_bundle_meta_json_file() {
        let temp_dir = TempDir::new().unwrap();
        let meta_path = temp_dir.path().join("meta.json");

        let meta = BundleMeta {
            profile_name: "personal".to_string(),
            timestamp: "2025-01-20T10:00:00Z".to_string(),
            hostname: "laptop".to_string(),
            dotdipper_version: "0.3.0".to_string(),
            file_count: 25,
            size_bytes: 51200,
        };

        let json = serde_json::to_string_pretty(&meta).unwrap();
        fs::write(&meta_path, &json).unwrap();

        assert!(meta_path.exists());

        let content = fs::read_to_string(&meta_path).unwrap();
        let loaded: BundleMeta = serde_json::from_str(&content).unwrap();

        assert_eq!(loaded.profile_name, "personal");
        assert_eq!(loaded.file_count, 25);
    }
}

#[cfg(test)]
mod bundle_structure_tests {
    use super::*;

    #[test]
    fn test_bundle_directory_structure() {
        let temp_dir = TempDir::new().unwrap();
        let bundle_root = temp_dir.path().join("dotdipper_bundle");

        fs::create_dir_all(&bundle_root).unwrap();
        fs::create_dir_all(bundle_root.join("compiled")).unwrap();

        // Create meta.json
        let meta = BundleMeta {
            profile_name: "default".to_string(),
            timestamp: "2025-01-20T12:00:00Z".to_string(),
            hostname: "test".to_string(),
            dotdipper_version: "0.3.0".to_string(),
            file_count: 1,
            size_bytes: 100,
        };

        let meta_json = serde_json::to_string_pretty(&meta).unwrap();
        fs::write(bundle_root.join("meta.json"), meta_json).unwrap();

        // Create manifest.lock
        fs::write(bundle_root.join("manifest.lock"), "{}").unwrap();

        // Create a sample compiled file
        fs::write(bundle_root.join("compiled/.zshrc"), "# zshrc content").unwrap();

        // Verify structure
        assert!(bundle_root.join("meta.json").exists());
        assert!(bundle_root.join("manifest.lock").exists());
        assert!(bundle_root.join("compiled").exists());
        assert!(bundle_root.join("compiled/.zshrc").exists());
    }
}

#[cfg(test)]
mod file_count_tests {
    use super::*;
    use walkdir::WalkDir;

    fn count_files(dir: &std::path::Path) -> usize {
        WalkDir::new(dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .count()
    }

    #[test]
    fn test_count_files_empty() {
        let temp_dir = TempDir::new().unwrap();
        let count = count_files(temp_dir.path());
        assert_eq!(count, 0);
    }

    #[test]
    fn test_count_files_with_files() {
        let temp_dir = TempDir::new().unwrap();

        fs::write(temp_dir.path().join("file1.txt"), "content").unwrap();
        fs::write(temp_dir.path().join("file2.txt"), "content").unwrap();
        fs::write(temp_dir.path().join("file3.txt"), "content").unwrap();

        let count = count_files(temp_dir.path());
        assert_eq!(count, 3);
    }

    #[test]
    fn test_count_files_with_subdirectories() {
        let temp_dir = TempDir::new().unwrap();

        fs::write(temp_dir.path().join("root.txt"), "content").unwrap();

        fs::create_dir_all(temp_dir.path().join("subdir")).unwrap();
        fs::write(temp_dir.path().join("subdir/nested.txt"), "content").unwrap();

        fs::create_dir_all(temp_dir.path().join("subdir/deep")).unwrap();
        fs::write(temp_dir.path().join("subdir/deep/deep.txt"), "content").unwrap();

        let count = count_files(temp_dir.path());
        assert_eq!(count, 3);
    }
}

#[cfg(test)]
mod size_calculation_tests {
    use super::*;
    use walkdir::WalkDir;

    fn calculate_size(dir: &std::path::Path) -> u64 {
        WalkDir::new(dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .filter_map(|e| e.metadata().ok())
            .map(|m| m.len())
            .sum()
    }

    #[test]
    fn test_calculate_size_empty() {
        let temp_dir = TempDir::new().unwrap();
        let size = calculate_size(temp_dir.path());
        assert_eq!(size, 0);
    }

    #[test]
    fn test_calculate_size_with_content() {
        let temp_dir = TempDir::new().unwrap();

        // Write files with known sizes
        let content = b"Hello, World!"; // 13 bytes
        fs::write(temp_dir.path().join("file1.txt"), content).unwrap();
        fs::write(temp_dir.path().join("file2.txt"), content).unwrap();

        let size = calculate_size(temp_dir.path());
        assert_eq!(size, 26); // 13 * 2
    }
}

#[cfg(test)]
mod copy_recursive_tests {
    use super::*;

    fn copy_dir_recursive(src: &std::path::Path, dest: &std::path::Path) -> std::io::Result<()> {
        fs::create_dir_all(dest)?;

        for entry in fs::read_dir(src)? {
            let entry = entry?;
            let path = entry.path();
            let dest_path = dest.join(entry.file_name());

            if path.is_dir() {
                copy_dir_recursive(&path, &dest_path)?;
            } else {
                fs::copy(&path, &dest_path)?;
            }
        }

        Ok(())
    }

    #[test]
    fn test_copy_dir_recursive_simple() {
        let src_dir = TempDir::new().unwrap();
        let dest_dir = TempDir::new().unwrap();

        // Create source structure
        fs::write(src_dir.path().join("file.txt"), "content").unwrap();

        // Copy
        copy_dir_recursive(src_dir.path(), dest_dir.path()).unwrap();

        // Verify
        assert!(dest_dir.path().join("file.txt").exists());
    }

    #[test]
    fn test_copy_dir_recursive_nested() {
        let src_dir = TempDir::new().unwrap();
        let dest_dir = TempDir::new().unwrap();

        // Create nested structure
        fs::create_dir_all(src_dir.path().join("a/b/c")).unwrap();
        fs::write(src_dir.path().join("a/b/c/deep.txt"), "deep content").unwrap();
        fs::write(src_dir.path().join("root.txt"), "root content").unwrap();

        // Copy
        copy_dir_recursive(src_dir.path(), dest_dir.path()).unwrap();

        // Verify
        assert!(dest_dir.path().join("a/b/c/deep.txt").exists());
        assert!(dest_dir.path().join("root.txt").exists());

        let content = fs::read_to_string(dest_dir.path().join("a/b/c/deep.txt")).unwrap();
        assert_eq!(content, "deep content");
    }
}

#[cfg(test)]
mod hostname_tests {
    #[test]
    fn test_hostname_retrieval() {
        let hostname = hostname::get()
            .ok()
            .and_then(|h| h.into_string().ok())
            .unwrap_or_else(|| "unknown".to_string());

        assert!(!hostname.is_empty());
        assert_ne!(hostname, "");
    }
}

#[cfg(test)]
mod timestamp_tests {
    use chrono::Utc;

    #[test]
    fn test_timestamp_rfc3339() {
        let timestamp = Utc::now().to_rfc3339();

        // RFC 3339 format: YYYY-MM-DDTHH:MM:SS+00:00 or similar
        assert!(timestamp.contains('T'));
        assert!(timestamp.len() > 20);
    }

    #[test]
    fn test_timestamp_parsing() {
        let original = Utc::now();
        let timestamp_str = original.to_rfc3339();

        // Parse back
        let parsed = chrono::DateTime::parse_from_rfc3339(&timestamp_str).unwrap();

        // Should be very close (within a second)
        assert!((parsed.timestamp() - original.timestamp()).abs() < 1);
    }
}
