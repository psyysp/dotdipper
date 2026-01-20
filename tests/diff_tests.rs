//! Integration tests for the diff module

use dotdipper::diff::{DiffEntry, DiffStatus, filter_by_paths};
use std::path::PathBuf;

#[test]
fn test_diff_status_symbol() {
    // Just verify symbols are created without panicking
    let _ = DiffStatus::Modified.symbol();
    let _ = DiffStatus::New.symbol();
    let _ = DiffStatus::Missing.symbol();
    let _ = DiffStatus::Identical.symbol();
}

#[test]
fn test_diff_status_equality() {
    assert_eq!(DiffStatus::Modified, DiffStatus::Modified);
    assert_eq!(DiffStatus::New, DiffStatus::New);
    assert_eq!(DiffStatus::Missing, DiffStatus::Missing);
    assert_eq!(DiffStatus::Identical, DiffStatus::Identical);
    
    assert_ne!(DiffStatus::Modified, DiffStatus::New);
    assert_ne!(DiffStatus::Missing, DiffStatus::Identical);
}

#[test]
fn test_diff_entry_struct() {
    let entry = DiffEntry {
        rel_path: PathBuf::from(".zshrc"),
        source_path: PathBuf::from("/home/user/.dotdipper/compiled/.zshrc"),
        target_path: PathBuf::from("/home/user/.zshrc"),
        status: DiffStatus::Modified,
    };
    
    assert_eq!(entry.rel_path, PathBuf::from(".zshrc"));
    assert_eq!(entry.status, DiffStatus::Modified);
}

#[test]
fn test_filter_by_paths_empty_filter() {
    let entries = vec![
        DiffEntry {
            rel_path: PathBuf::from(".zshrc"),
            source_path: PathBuf::from("/source/.zshrc"),
            target_path: PathBuf::from("/target/.zshrc"),
            status: DiffStatus::Modified,
        },
        DiffEntry {
            rel_path: PathBuf::from(".vimrc"),
            source_path: PathBuf::from("/source/.vimrc"),
            target_path: PathBuf::from("/target/.vimrc"),
            status: DiffStatus::New,
        },
    ];
    
    // Empty filter should return all entries
    let filtered = filter_by_paths(entries.clone(), &[]).unwrap();
    assert_eq!(filtered.len(), 2);
}

#[test]
fn test_filter_by_paths_specific() {
    let entries = vec![
        DiffEntry {
            rel_path: PathBuf::from(".zshrc"),
            source_path: PathBuf::from("/source/.zshrc"),
            target_path: PathBuf::from("/target/.zshrc"),
            status: DiffStatus::Modified,
        },
        DiffEntry {
            rel_path: PathBuf::from(".vimrc"),
            source_path: PathBuf::from("/source/.vimrc"),
            target_path: PathBuf::from("/target/.vimrc"),
            status: DiffStatus::New,
        },
        DiffEntry {
            rel_path: PathBuf::from(".bashrc"),
            source_path: PathBuf::from("/source/.bashrc"),
            target_path: PathBuf::from("/target/.bashrc"),
            status: DiffStatus::Missing,
        },
    ];
    
    let filter_paths = vec![".zshrc".to_string()];
    let filtered = filter_by_paths(entries, &filter_paths).unwrap();
    
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].rel_path, PathBuf::from(".zshrc"));
}

#[test]
fn test_filter_by_paths_directory() {
    let entries = vec![
        DiffEntry {
            rel_path: PathBuf::from(".config/nvim/init.lua"),
            source_path: PathBuf::from("/source/.config/nvim/init.lua"),
            target_path: PathBuf::from("/target/.config/nvim/init.lua"),
            status: DiffStatus::Modified,
        },
        DiffEntry {
            rel_path: PathBuf::from(".config/nvim/lua/plugins.lua"),
            source_path: PathBuf::from("/source/.config/nvim/lua/plugins.lua"),
            target_path: PathBuf::from("/target/.config/nvim/lua/plugins.lua"),
            status: DiffStatus::New,
        },
        DiffEntry {
            rel_path: PathBuf::from(".zshrc"),
            source_path: PathBuf::from("/source/.zshrc"),
            target_path: PathBuf::from("/target/.zshrc"),
            status: DiffStatus::Modified,
        },
    ];
    
    let filter_paths = vec![".config/nvim".to_string()];
    let filtered = filter_by_paths(entries, &filter_paths).unwrap();
    
    assert_eq!(filtered.len(), 2);
}

#[test]
fn test_diff_entry_clone() {
    let entry = DiffEntry {
        rel_path: PathBuf::from(".tmux.conf"),
        source_path: PathBuf::from("/source/.tmux.conf"),
        target_path: PathBuf::from("/target/.tmux.conf"),
        status: DiffStatus::Identical,
    };
    
    let cloned = entry.clone();
    assert_eq!(entry.rel_path, cloned.rel_path);
    assert_eq!(entry.status, cloned.status);
}

#[test]
fn test_diff_status_copy() {
    let status = DiffStatus::Modified;
    let copied = status;
    assert_eq!(status, copied);
}

#[test]
fn test_filter_by_paths_multiple_filters() {
    let entries = vec![
        DiffEntry {
            rel_path: PathBuf::from(".zshrc"),
            source_path: PathBuf::from("/source/.zshrc"),
            target_path: PathBuf::from("/target/.zshrc"),
            status: DiffStatus::Modified,
        },
        DiffEntry {
            rel_path: PathBuf::from(".vimrc"),
            source_path: PathBuf::from("/source/.vimrc"),
            target_path: PathBuf::from("/target/.vimrc"),
            status: DiffStatus::New,
        },
        DiffEntry {
            rel_path: PathBuf::from(".bashrc"),
            source_path: PathBuf::from("/source/.bashrc"),
            target_path: PathBuf::from("/target/.bashrc"),
            status: DiffStatus::Missing,
        },
    ];
    
    let filter_paths = vec![".zshrc".to_string(), ".bashrc".to_string()];
    let filtered = filter_by_paths(entries, &filter_paths).unwrap();
    
    assert_eq!(filtered.len(), 2);
}

#[test]
fn test_filter_by_paths_no_match() {
    let entries = vec![
        DiffEntry {
            rel_path: PathBuf::from(".zshrc"),
            source_path: PathBuf::from("/source/.zshrc"),
            target_path: PathBuf::from("/target/.zshrc"),
            status: DiffStatus::Modified,
        },
    ];
    
    let filter_paths = vec!["nonexistent".to_string()];
    let filtered = filter_by_paths(entries, &filter_paths).unwrap();
    
    assert!(filtered.is_empty());
}

#[test]
fn test_diff_status_variants_complete() {
    // Ensure all variants are accounted for
    let variants = [
        DiffStatus::Modified,
        DiffStatus::New,
        DiffStatus::Missing,
        DiffStatus::Identical,
    ];
    
    for variant in variants {
        // Each variant should produce a symbol without panicking
        let _ = variant.symbol();
    }
}
