//! Integration tests for the hash module

use dotdipper::hash::{self, FileHash, Manifest};
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use tempfile::TempDir;

#[test]
fn test_manifest_new() {
    let manifest = Manifest::new();
    assert_eq!(manifest.version, "1.0.0");
    assert!(manifest.files.is_empty());
}

#[test]
fn test_manifest_add_file() {
    let mut manifest = Manifest::new();

    let file_hash = FileHash {
        path: PathBuf::from(".zshrc"),
        hash: "abc123".to_string(),
        size: 1024,
        mode: 0o644,
        modified: chrono::Utc::now(),
    };

    manifest.add_file(file_hash.clone());

    assert!(manifest.has_file(&PathBuf::from(".zshrc")));
    assert_eq!(
        manifest.get_file(&PathBuf::from(".zshrc")).unwrap().hash,
        "abc123"
    );
}

#[test]
fn test_manifest_has_file() {
    let mut manifest = Manifest::new();

    assert!(!manifest.has_file(&PathBuf::from(".zshrc")));

    let file_hash = FileHash {
        path: PathBuf::from(".zshrc"),
        hash: "abc123".to_string(),
        size: 1024,
        mode: 0o644,
        modified: chrono::Utc::now(),
    };

    manifest.add_file(file_hash);

    assert!(manifest.has_file(&PathBuf::from(".zshrc")));
    assert!(!manifest.has_file(&PathBuf::from(".bashrc")));
}

#[test]
fn test_manifest_save_and_load() {
    let temp_dir = TempDir::new().unwrap();
    let manifest_path = temp_dir.path().join("manifest.lock");

    let mut manifest = Manifest::new();

    let file_hash = FileHash {
        path: PathBuf::from(".config/nvim/init.lua"),
        hash: "def456".to_string(),
        size: 2048,
        mode: 0o755,
        modified: chrono::Utc::now(),
    };

    manifest.add_file(file_hash);
    manifest.save(&manifest_path).unwrap();

    assert!(manifest_path.exists());

    let loaded = Manifest::load(&manifest_path).unwrap();
    assert_eq!(loaded.version, "1.0.0");
    assert!(loaded.has_file(&PathBuf::from(".config/nvim/init.lua")));
}

#[test]
fn test_hash_file() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.txt");

    let mut file = File::create(&file_path).unwrap();
    file.write_all(b"Hello, world!").unwrap();
    drop(file);

    let hash_result = hash::hash_file(&file_path).unwrap();

    assert_eq!(hash_result.size, 13);
    assert!(!hash_result.hash.is_empty());
    assert!(hash_result.hash.len() == 64); // BLAKE3 produces 64-char hex
}

#[test]
fn test_hash_file_consistent() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.txt");

    let mut file = File::create(&file_path).unwrap();
    file.write_all(b"Consistent content").unwrap();
    drop(file);

    let hash1 = hash::hash_file(&file_path).unwrap();
    let hash2 = hash::hash_file(&file_path).unwrap();

    assert_eq!(hash1.hash, hash2.hash);
}

#[test]
fn test_hash_file_different_content() {
    let temp_dir = TempDir::new().unwrap();
    let file1 = temp_dir.path().join("file1.txt");
    let file2 = temp_dir.path().join("file2.txt");

    fs::write(&file1, "content 1").unwrap();
    fs::write(&file2, "content 2").unwrap();

    let hash1 = hash::hash_file(&file1).unwrap();
    let hash2 = hash::hash_file(&file2).unwrap();

    assert_ne!(hash1.hash, hash2.hash);
}

#[test]
fn test_hash_files_multiple() {
    let temp_dir = TempDir::new().unwrap();
    let file1 = temp_dir.path().join("file1.txt");
    let file2 = temp_dir.path().join("file2.txt");
    let file3 = temp_dir.path().join("file3.txt");

    fs::write(&file1, "content 1").unwrap();
    fs::write(&file2, "content 2").unwrap();
    fs::write(&file3, "content 3").unwrap();

    let paths = vec![file1, file2, file3];
    let hashes = hash::hash_files(&paths, false).unwrap();

    assert_eq!(hashes.len(), 3);
}

#[test]
fn test_hash_file_nonexistent() {
    let result = hash::hash_file(&PathBuf::from("/nonexistent/file.txt"));
    assert!(result.is_err());
}

#[test]
fn test_verify_file() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("verify_test.txt");

    fs::write(&file_path, "verification content").unwrap();

    let file_hash = hash::hash_file(&file_path).unwrap();

    // Should verify as correct
    assert!(hash::verify_file(&file_hash).unwrap());

    // Modify the file
    fs::write(&file_path, "modified content").unwrap();

    // Should now fail verification
    assert!(!hash::verify_file(&file_hash).unwrap());
}

#[test]
fn test_verify_file_missing() {
    let file_hash = FileHash {
        path: PathBuf::from("/nonexistent/file.txt"),
        hash: "somehash".to_string(),
        size: 100,
        mode: 0o644,
        modified: chrono::Utc::now(),
    };

    assert!(!hash::verify_file(&file_hash).unwrap());
}

#[test]
fn test_verify_manifest_home_relative_paths() {
    let home = TempDir::new().unwrap();
    let rel = PathBuf::from(".config/kitty/kitty.conf");
    let abs = home.path().join(&rel);
    fs::create_dir_all(abs.parent().unwrap()).unwrap();
    fs::write(&abs, "font_size 13").unwrap();

    let mut file_hash = hash::hash_file(&abs).unwrap();
    file_hash.path = rel.clone();

    let mut manifest = Manifest::new();
    manifest.add_file(file_hash);

    let invalid = hash::verify_manifest_in(&manifest, Some(home.path())).unwrap();
    assert!(
        invalid.is_empty(),
        "home-relative manifest paths should verify against $HOME, got {invalid:?}"
    );

    // Without a home root, a relative path must not be resolved from cwd
    // (the bug that made `dotdipper doctor` fail all 78 files).
    let cwd_miss = hash::resolve_manifest_path_in(&rel, Some(home.path())).unwrap();
    assert_eq!(cwd_miss, abs);
}

#[test]
fn test_verify_manifest() {
    let temp_dir = TempDir::new().unwrap();
    let file1 = temp_dir.path().join("file1.txt");
    let file2 = temp_dir.path().join("file2.txt");

    fs::write(&file1, "content 1").unwrap();
    fs::write(&file2, "content 2").unwrap();

    let mut manifest = Manifest::new();
    manifest.add_file(hash::hash_file(&file1).unwrap());
    manifest.add_file(hash::hash_file(&file2).unwrap());

    // All files should be valid
    let invalid = hash::verify_manifest(&manifest).unwrap();
    assert!(invalid.is_empty());

    // Modify one file
    fs::write(&file1, "modified content").unwrap();

    // Now one file should be invalid
    let invalid = hash::verify_manifest(&manifest).unwrap();
    assert_eq!(invalid.len(), 1);
}

#[test]
fn test_manifest_serialization_json() {
    let mut manifest = Manifest::new();

    let file_hash = FileHash {
        path: PathBuf::from(".bashrc"),
        hash: "serialization_test_hash".to_string(),
        size: 512,
        mode: 0o644,
        modified: chrono::Utc::now(),
    };

    manifest.add_file(file_hash);

    let json = serde_json::to_string(&manifest).unwrap();
    assert!(json.contains("serialization_test_hash"));
    assert!(json.contains(".bashrc"));

    let deserialized: Manifest = serde_json::from_str(&json).unwrap();
    assert!(deserialized.has_file(&PathBuf::from(".bashrc")));
}

#[test]
fn test_file_hash_struct() {
    let now = chrono::Utc::now();
    let file_hash = FileHash {
        path: PathBuf::from("test/path"),
        hash: "testhash".to_string(),
        size: 1234,
        mode: 0o755,
        modified: now,
    };

    assert_eq!(file_hash.path, PathBuf::from("test/path"));
    assert_eq!(file_hash.hash, "testhash");
    assert_eq!(file_hash.size, 1234);
    assert_eq!(file_hash.mode, 0o755);
}

#[test]
fn test_hash_empty_file() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("empty.txt");

    File::create(&file_path).unwrap();

    let hash_result = hash::hash_file(&file_path).unwrap();
    assert_eq!(hash_result.size, 0);
    assert!(!hash_result.hash.is_empty());
}

#[test]
fn test_hash_large_file() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("large.txt");

    // Create a file larger than the buffer size (8192 bytes)
    let content = vec![b'a'; 50000];
    fs::write(&file_path, &content).unwrap();

    let hash_result = hash::hash_file(&file_path).unwrap();
    assert_eq!(hash_result.size, 50000);
}
