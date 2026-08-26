use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

/// Helper to create a minimal config and dotdipper directory structure
fn setup_test_env(temp_dir: &TempDir) -> (std::path::PathBuf, std::path::PathBuf) {
    let dotdipper_dir = temp_dir.path().join(".config").join("dotdipper");
    fs::create_dir_all(&dotdipper_dir).unwrap();
    fs::create_dir_all(dotdipper_dir.join("compiled")).unwrap();
    fs::create_dir_all(dotdipper_dir.join("snapshots")).unwrap();

    let config_path = dotdipper_dir.join("config.toml");
    fs::write(
        &config_path,
        r#"
[general]
tracked_files = []
"#,
    )
    .unwrap();

    // Create a minimal manifest
    let manifest = r#"{
        "version": "1.0.0",
        "created": "2025-01-01T00:00:00Z",
        "files": {}
    }"#;
    fs::write(dotdipper_dir.join("manifest.lock"), manifest).unwrap();

    (config_path, dotdipper_dir)
}

/// Helper to create a fake snapshot
/// Note: The actual snapshots module uses snapshot.json (not meta.json)
/// and ID format is YYYYMMDD_HHMMSS (not ISO format with dashes)
fn create_fake_snapshot(
    snapshots_dir: &std::path::Path,
    id: &str,
    size_bytes: u64,
    file_count: u64,
    created_at: &str,
) {
    let snapshot_dir = snapshots_dir.join(id);
    fs::create_dir_all(&snapshot_dir).unwrap();

    // Create some dummy files to match the size
    for i in 0..file_count {
        let file_path = snapshot_dir.join(format!("file{}.txt", i));
        let content = vec![b'x'; (size_bytes / file_count.max(1)) as usize];
        fs::write(file_path, content).unwrap();
    }

    // Create snapshot.json (the actual file name used by the code)
    let meta = format!(
        r#"{{
        "id": "{}",
        "created_at": "{}",
        "message": "Test snapshot",
        "size_bytes": {},
        "file_count": {}
    }}"#,
        id, created_at, size_bytes, file_count
    );
    fs::write(snapshot_dir.join("snapshot.json"), meta).unwrap();
}

#[test]
fn test_snapshot_list_empty() {
    let temp_dir = TempDir::new().unwrap();
    let (config_path, _) = setup_test_env(&temp_dir);

    // Set HOME to temp dir so dotdipper looks in the right place
    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.env("HOME", temp_dir.path())
        .arg("--config")
        .arg(&config_path)
        .arg("snapshot")
        .arg("list");

    cmd.assert().success().stdout(
        predicate::str::contains("No snapshots found")
            .or(predicate::str::contains("Found 0 snapshots")),
    );
}

#[test]
fn test_snapshot_list_with_snapshots() {
    let temp_dir = TempDir::new().unwrap();
    let (config_path, dotdipper_dir) = setup_test_env(&temp_dir);
    let snapshots_dir = dotdipper_dir.join("snapshots");

    // Create some fake snapshots (using actual ID format: YYYYMMDD_HHMMSS)
    create_fake_snapshot(
        &snapshots_dir,
        "20250115_100000",
        1024,
        5,
        "2025-01-15T10:00:00Z",
    );
    create_fake_snapshot(
        &snapshots_dir,
        "20250116_100000",
        2048,
        10,
        "2025-01-16T10:00:00Z",
    );

    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.env("HOME", temp_dir.path())
        .arg("--config")
        .arg(&config_path)
        .arg("snapshot")
        .arg("list");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("20250115_100000"))
        .stdout(predicate::str::contains("20250116_100000"));
}

#[test]
fn test_snapshot_delete_nonexistent() {
    let temp_dir = TempDir::new().unwrap();
    let (config_path, _) = setup_test_env(&temp_dir);

    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.env("HOME", temp_dir.path())
        .arg("--config")
        .arg(&config_path)
        .arg("snapshot")
        .arg("delete")
        .arg("nonexistent-snapshot-id")
        .arg("--force");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

#[test]
fn test_snapshot_delete_existing() {
    let temp_dir = TempDir::new().unwrap();
    let (config_path, dotdipper_dir) = setup_test_env(&temp_dir);
    let snapshots_dir = dotdipper_dir.join("snapshots");

    // Create a fake snapshot (using actual ID format)
    create_fake_snapshot(
        &snapshots_dir,
        "20250115_100000",
        1024,
        5,
        "2025-01-15T10:00:00Z",
    );

    // Verify it exists
    assert!(snapshots_dir.join("20250115_100000").exists());

    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.env("HOME", temp_dir.path())
        .arg("--config")
        .arg(&config_path)
        .arg("snapshot")
        .arg("delete")
        .arg("20250115_100000")
        .arg("--force");

    cmd.assert().success();

    // Verify it's deleted
    assert!(!snapshots_dir.join("20250115_100000").exists());
}

#[test]
fn test_snapshot_rollback_nonexistent() {
    let temp_dir = TempDir::new().unwrap();
    let (config_path, _) = setup_test_env(&temp_dir);

    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.env("HOME", temp_dir.path())
        .arg("--config")
        .arg(&config_path)
        .arg("snapshot")
        .arg("rollback")
        .arg("nonexistent-snapshot-id")
        .arg("--force");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

#[test]
fn test_snapshot_prune_no_criteria() {
    let temp_dir = TempDir::new().unwrap();
    let (config_path, dotdipper_dir) = setup_test_env(&temp_dir);
    let snapshots_dir = dotdipper_dir.join("snapshots");

    // Create some snapshots (using actual ID format)
    create_fake_snapshot(
        &snapshots_dir,
        "20250115_100000",
        1024,
        5,
        "2025-01-15T10:00:00Z",
    );

    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.env("HOME", temp_dir.path())
        .arg("--config")
        .arg(&config_path)
        .arg("snapshot")
        .arg("prune");

    // When no criteria is specified and snapshots exist, all are kept
    cmd.assert().success().stdout(predicate::str::contains(
        "No prune criteria specified; keeping all snapshots",
    ));
}

#[test]
fn test_snapshot_prune_keep_count_dry_run() {
    let temp_dir = TempDir::new().unwrap();
    let (config_path, dotdipper_dir) = setup_test_env(&temp_dir);
    let snapshots_dir = dotdipper_dir.join("snapshots");

    // Create multiple snapshots (using actual ID format: YYYYMMDD_HHMMSS)
    create_fake_snapshot(
        &snapshots_dir,
        "20250113_100000",
        1024,
        5,
        "2025-01-13T10:00:00Z",
    );
    create_fake_snapshot(
        &snapshots_dir,
        "20250114_100000",
        1024,
        5,
        "2025-01-14T10:00:00Z",
    );
    create_fake_snapshot(
        &snapshots_dir,
        "20250115_100000",
        1024,
        5,
        "2025-01-15T10:00:00Z",
    );
    create_fake_snapshot(
        &snapshots_dir,
        "20250116_100000",
        1024,
        5,
        "2025-01-16T10:00:00Z",
    );

    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.env("HOME", temp_dir.path())
        .arg("--config")
        .arg(&config_path)
        .arg("snapshot")
        .arg("prune")
        .arg("--keep-count")
        .arg("2")
        .arg("--dry-run");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("dry run").or(predicate::str::contains("Would delete")));

    // All snapshots should still exist (dry run)
    assert!(snapshots_dir.join("20250113_100000").exists());
    assert!(snapshots_dir.join("20250114_100000").exists());
    assert!(snapshots_dir.join("20250115_100000").exists());
    assert!(snapshots_dir.join("20250116_100000").exists());
}

#[test]
fn test_snapshot_prune_keep_count() {
    let temp_dir = TempDir::new().unwrap();
    let (config_path, dotdipper_dir) = setup_test_env(&temp_dir);
    let snapshots_dir = dotdipper_dir.join("snapshots");

    // Create multiple snapshots (using actual ID format: YYYYMMDD_HHMMSS)
    create_fake_snapshot(
        &snapshots_dir,
        "20250113_100000",
        1024,
        5,
        "2025-01-13T10:00:00Z",
    );
    create_fake_snapshot(
        &snapshots_dir,
        "20250114_100000",
        1024,
        5,
        "2025-01-14T10:00:00Z",
    );
    create_fake_snapshot(
        &snapshots_dir,
        "20250115_100000",
        1024,
        5,
        "2025-01-15T10:00:00Z",
    );
    create_fake_snapshot(
        &snapshots_dir,
        "20250116_100000",
        1024,
        5,
        "2025-01-16T10:00:00Z",
    );

    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.env("HOME", temp_dir.path())
        .arg("--config")
        .arg(&config_path)
        .arg("snapshot")
        .arg("prune")
        .arg("--keep-count")
        .arg("2");

    cmd.assert().success();

    // Oldest snapshots should be deleted, newest 2 should remain
    assert!(!snapshots_dir.join("20250113_100000").exists());
    assert!(!snapshots_dir.join("20250114_100000").exists());
    assert!(snapshots_dir.join("20250115_100000").exists());
    assert!(snapshots_dir.join("20250116_100000").exists());
}

#[test]
fn test_snapshot_prune_no_snapshots() {
    let temp_dir = TempDir::new().unwrap();
    let (config_path, _) = setup_test_env(&temp_dir);

    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.env("HOME", temp_dir.path())
        .arg("--config")
        .arg(&config_path)
        .arg("snapshot")
        .arg("prune")
        .arg("--keep-count")
        .arg("5");

    cmd.assert().success().stdout(
        predicate::str::contains("No snapshots")
            .or(predicate::str::contains("No snapshots to prune")),
    );
}

#[cfg(test)]
mod unit_tests {
    // Unit tests for internal functions are in src/snapshots/mod.rs
    // These integration tests cover the CLI interface
}
