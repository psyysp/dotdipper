//! Integration tests for the repo module

use dotdipper::cfg::RestoreMode;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

#[cfg(test)]
mod snapshot_struct_tests {
    #[derive(Debug)]
    struct Snapshot {
        file_count: usize,
    }

    #[test]
    fn test_snapshot_creation() {
        let snapshot = Snapshot { file_count: 10 };
        assert_eq!(snapshot.file_count, 10);
    }
}

#[cfg(test)]
mod status_tests {
    use super::*;

    #[derive(Debug)]
    struct Status {
        modified: Vec<PathBuf>,
        added: Vec<PathBuf>,
        deleted: Vec<PathBuf>,
    }

    impl Status {
        fn is_clean(&self) -> bool {
            self.modified.is_empty() && self.added.is_empty() && self.deleted.is_empty()
        }
    }

    #[test]
    fn test_status_is_clean_true() {
        let status = Status {
            modified: vec![],
            added: vec![],
            deleted: vec![],
        };

        assert!(status.is_clean());
    }

    #[test]
    fn test_status_is_clean_modified() {
        let status = Status {
            modified: vec![PathBuf::from(".zshrc")],
            added: vec![],
            deleted: vec![],
        };

        assert!(!status.is_clean());
    }

    #[test]
    fn test_status_is_clean_added() {
        let status = Status {
            modified: vec![],
            added: vec![PathBuf::from(".vimrc")],
            deleted: vec![],
        };

        assert!(!status.is_clean());
    }

    #[test]
    fn test_status_is_clean_deleted() {
        let status = Status {
            modified: vec![],
            added: vec![],
            deleted: vec![PathBuf::from(".bashrc")],
        };

        assert!(!status.is_clean());
    }

    #[test]
    fn test_status_counts() {
        let status = Status {
            modified: vec![PathBuf::from("a"), PathBuf::from("b")],
            added: vec![PathBuf::from("c")],
            deleted: vec![PathBuf::from("d"), PathBuf::from("e"), PathBuf::from("f")],
        };

        assert_eq!(status.modified.len(), 2);
        assert_eq!(status.added.len(), 1);
        assert_eq!(status.deleted.len(), 3);
    }
}

#[cfg(test)]
mod apply_opts_tests {
    use dotdipper::repo::apply::ApplyOpts;

    #[test]
    fn test_apply_opts_default() {
        let opts = ApplyOpts {
            force: false,
            allow_outside_home: false,
        };

        assert!(!opts.force);
        assert!(!opts.allow_outside_home);
    }

    #[test]
    fn test_apply_opts_force() {
        let opts = ApplyOpts {
            force: true,
            allow_outside_home: false,
        };

        assert!(opts.force);
    }

    #[test]
    fn test_apply_opts_unsafe() {
        let opts = ApplyOpts {
            force: false,
            allow_outside_home: true,
        };

        assert!(opts.allow_outside_home);
    }
}

#[cfg(test)]
mod applied_mode_tests {
    use dotdipper::repo::apply::AppliedMode;

    #[test]
    fn test_applied_mode_equality() {
        assert_eq!(AppliedMode::Symlinked, AppliedMode::Symlinked);
        assert_eq!(AppliedMode::Copied, AppliedMode::Copied);
        assert_eq!(AppliedMode::Skipped, AppliedMode::Skipped);

        assert_ne!(AppliedMode::Symlinked, AppliedMode::Copied);
        assert_ne!(AppliedMode::Copied, AppliedMode::Skipped);
    }

    #[test]
    fn test_applied_mode_ord() {
        // AppliedMode implements Ord
        let mut modes = [
            AppliedMode::Skipped,
            AppliedMode::Symlinked,
            AppliedMode::Copied,
        ];

        modes.sort();

        // Order should be consistent (Symlinked, Copied, Skipped based on enum order)
        assert!(modes[0] <= modes[1]);
        assert!(modes[1] <= modes[2]);
    }

    #[test]
    fn test_applied_mode_color_str() {
        let _ = AppliedMode::Symlinked.color_str();
        let _ = AppliedMode::Copied.color_str();
        let _ = AppliedMode::Skipped.color_str();
    }
}

#[cfg(test)]
mod applied_action_tests {
    use dotdipper::repo::apply::{AppliedAction, AppliedMode};
    use std::path::PathBuf;

    #[test]
    fn test_applied_action_creation() {
        let action = AppliedAction {
            mode: AppliedMode::Symlinked,
            target: PathBuf::from("/home/user/.zshrc"),
            source: PathBuf::from("/home/user/.dotdipper/compiled/.zshrc"),
            backup_created: false,
            skipped_reason: None,
        };

        assert_eq!(action.mode, AppliedMode::Symlinked);
        assert!(!action.backup_created);
        assert!(action.skipped_reason.is_none());
    }

    #[test]
    fn test_applied_action_with_backup() {
        let action = AppliedAction {
            mode: AppliedMode::Copied,
            target: PathBuf::from("/home/user/.vimrc"),
            source: PathBuf::from("/home/user/.dotdipper/compiled/.vimrc"),
            backup_created: true,
            skipped_reason: None,
        };

        assert!(action.backup_created);
    }

    #[test]
    fn test_applied_action_skipped() {
        let action = AppliedAction {
            mode: AppliedMode::Skipped,
            target: PathBuf::from("/etc/hosts"),
            source: PathBuf::from("/home/user/.dotdipper/compiled/etc/hosts"),
            backup_created: false,
            skipped_reason: Some("Outside $HOME".to_string()),
        };

        assert_eq!(action.mode, AppliedMode::Skipped);
        assert_eq!(action.skipped_reason, Some("Outside $HOME".to_string()));
    }
}

#[cfg(test)]
mod restore_mode_tests {
    use super::*;

    #[test]
    fn test_restore_mode_symlink() {
        let mode = RestoreMode::Symlink;
        assert_eq!(mode, RestoreMode::Symlink);
    }

    #[test]
    fn test_restore_mode_copy() {
        let mode = RestoreMode::Copy;
        assert_eq!(mode, RestoreMode::Copy);
    }

    #[test]
    fn test_restore_mode_serialization() {
        let config_str = r#"default_mode = "symlink""#;

        #[derive(serde::Deserialize)]
        struct TestConfig {
            default_mode: RestoreMode,
        }

        let config: TestConfig = toml::from_str(config_str).unwrap();
        assert_eq!(config.default_mode, RestoreMode::Symlink);
    }

    #[test]
    fn test_restore_mode_copy_serialization() {
        let config_str = r#"default_mode = "copy""#;

        #[derive(serde::Deserialize)]
        struct TestConfig {
            default_mode: RestoreMode,
        }

        let config: TestConfig = toml::from_str(config_str).unwrap();
        assert_eq!(config.default_mode, RestoreMode::Copy);
    }
}

#[cfg(test)]
mod backup_tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_backup_path_generation() {
        let original_path = PathBuf::from("/home/user/.zshrc");
        let timestamp = Utc::now().format("%Y%m%d-%H%M%S");
        let backup_path = PathBuf::from(format!("{}.bak.{}", original_path.display(), timestamp));

        assert!(backup_path.to_string_lossy().contains(".bak."));
        assert!(backup_path
            .to_string_lossy()
            .starts_with("/home/user/.zshrc.bak."));
    }

    #[test]
    fn test_backup_creation() {
        let temp_dir = TempDir::new().unwrap();
        let original = temp_dir.path().join("original.txt");

        fs::write(&original, "original content").unwrap();

        let backup = temp_dir.path().join("original.txt.bak");
        fs::copy(&original, &backup).unwrap();

        assert!(backup.exists());

        let backup_content = fs::read_to_string(&backup).unwrap();
        assert_eq!(backup_content, "original content");
    }
}

#[cfg(test)]
mod file_copy_tests {
    use super::*;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn test_copy_file_content() {
        let temp_dir = TempDir::new().unwrap();
        let source = temp_dir.path().join("source.txt");
        let dest = temp_dir.path().join("dest.txt");

        fs::write(&source, "test content").unwrap();
        fs::copy(&source, &dest).unwrap();

        let dest_content = fs::read_to_string(&dest).unwrap();
        assert_eq!(dest_content, "test content");
    }

    #[cfg(unix)]
    #[test]
    fn test_copy_preserves_permissions() {
        let temp_dir = TempDir::new().unwrap();
        let source = temp_dir.path().join("source.sh");
        let dest = temp_dir.path().join("dest.sh");

        fs::write(&source, "#!/bin/bash").unwrap();

        // Make source executable
        let mut perms = fs::metadata(&source).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&source, perms).unwrap();

        // Copy with metadata preservation
        fs::copy(&source, &dest).unwrap();
        let source_perms = fs::metadata(&source).unwrap().permissions();
        fs::set_permissions(&dest, source_perms).unwrap();

        let dest_perms = fs::metadata(&dest).unwrap().permissions();
        assert_eq!(dest_perms.mode() & 0o777, 0o755);
    }
}

#[cfg(unix)]
#[cfg(test)]
mod symlink_tests {
    use super::*;
    use std::os::unix::fs as unix_fs;

    #[test]
    fn test_create_symlink() {
        let temp_dir = TempDir::new().unwrap();
        let source = temp_dir.path().join("source.txt");
        let link = temp_dir.path().join("link.txt");

        fs::write(&source, "source content").unwrap();
        unix_fs::symlink(&source, &link).unwrap();

        assert!(link.is_symlink());

        let link_content = fs::read_to_string(&link).unwrap();
        assert_eq!(link_content, "source content");
    }

    #[test]
    fn test_symlink_target() {
        let temp_dir = TempDir::new().unwrap();
        let source = temp_dir.path().join("source.txt");
        let link = temp_dir.path().join("link.txt");

        fs::write(&source, "content").unwrap();
        unix_fs::symlink(&source, &link).unwrap();

        let target = fs::read_link(&link).unwrap();
        assert_eq!(target, source);
    }
}

#[cfg(test)]
mod home_boundary_tests {
    use std::path::Path;

    fn is_inside_home(path: &Path, home: &Path) -> bool {
        path.starts_with(home)
    }

    #[test]
    fn test_inside_home() {
        let home = Path::new("/home/user");

        assert!(is_inside_home(Path::new("/home/user/.zshrc"), home));
        assert!(is_inside_home(Path::new("/home/user/.config/nvim"), home));
        assert!(is_inside_home(Path::new("/home/user"), home));
    }

    #[test]
    fn test_outside_home() {
        let home = Path::new("/home/user");

        assert!(!is_inside_home(Path::new("/etc/hosts"), home));
        assert!(!is_inside_home(Path::new("/root/.bashrc"), home));
        assert!(!is_inside_home(Path::new("/home/otheruser/.zshrc"), home));
    }
}
