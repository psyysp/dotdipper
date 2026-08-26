//! Integration tests for the scan module

use dotdipper::cfg::Config;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

#[cfg(test)]
mod tilde_expansion_tests {
    fn expand_tilde(path: &str, home: &std::path::Path) -> String {
        if let Some(stripped) = path.strip_prefix("~/") {
            home.join(stripped).to_string_lossy().to_string()
        } else {
            path.to_string()
        }
    }

    #[test]
    fn test_expand_tilde_with_home() {
        let home = std::path::PathBuf::from("/home/testuser");
        let expanded = expand_tilde("~/.zshrc", &home);

        assert_eq!(expanded, "/home/testuser/.zshrc");
    }

    #[test]
    fn test_expand_tilde_no_tilde() {
        let home = std::path::PathBuf::from("/home/testuser");
        let expanded = expand_tilde("/absolute/path", &home);

        assert_eq!(expanded, "/absolute/path");
    }

    #[test]
    fn test_expand_tilde_just_tilde() {
        let home = std::path::PathBuf::from("/home/testuser");
        let expanded = expand_tilde("~", &home);

        // Should not expand (no slash after tilde)
        assert_eq!(expanded, "~");
    }
}

#[cfg(test)]
mod pattern_tests {
    use glob::Pattern;

    #[test]
    fn test_glob_pattern_match() {
        let pattern = Pattern::new("**/*.txt").unwrap();

        assert!(pattern.matches_path(std::path::Path::new("/path/to/file.txt")));
        assert!(!pattern.matches_path(std::path::Path::new("/path/to/file.rs")));
    }

    #[test]
    fn test_glob_pattern_config_files() {
        let pattern = Pattern::new("*.config").unwrap();

        assert!(pattern.matches_path(std::path::Path::new("app.config")));
        assert!(!pattern.matches_path(std::path::Path::new("app.txt")));
    }

    #[test]
    fn test_glob_pattern_hidden_files() {
        let pattern = Pattern::new(".*").unwrap();

        assert!(pattern.matches_path(std::path::Path::new(".zshrc")));
        assert!(pattern.matches_path(std::path::Path::new(".vimrc")));
        assert!(!pattern.matches_path(std::path::Path::new("zshrc")));
    }
}

#[cfg(test)]
mod base_dir_tests {
    use std::path::PathBuf;

    fn get_base_dir_from_pattern(pattern: &str, home: &std::path::Path) -> PathBuf {
        let expanded = if let Some(stripped) = pattern.strip_prefix("~/") {
            home.join(stripped).to_string_lossy().to_string()
        } else {
            pattern.to_string()
        };

        let parts: Vec<&str> = expanded.split('/').collect();

        let mut base_parts = Vec::new();
        for part in parts {
            if part.contains('*') || part.contains('?') || part.contains('[') {
                break;
            }
            base_parts.push(part);
        }

        if base_parts.is_empty() {
            home.to_path_buf()
        } else {
            PathBuf::from(base_parts.join("/"))
        }
    }

    #[test]
    fn test_get_base_dir_simple() {
        let home = PathBuf::from("/home/user");
        let base = get_base_dir_from_pattern("/home/user/.config/*", &home);

        assert_eq!(base, PathBuf::from("/home/user/.config"));
    }

    #[test]
    fn test_get_base_dir_with_tilde() {
        let home = PathBuf::from("/home/user");
        let base = get_base_dir_from_pattern("~/.config/**", &home);

        assert_eq!(base, PathBuf::from("/home/user/.config"));
    }

    #[test]
    fn test_get_base_dir_all_glob() {
        let home = PathBuf::from("/home/user");
        let base = get_base_dir_from_pattern("**/*.txt", &home);

        assert_eq!(base, home);
    }
}

#[cfg(test)]
mod discovery_tests {
    use super::*;
    use serial_test::serial;

    /// Point DOTDIPPER_HOME at an empty temp dir for the duration of a test.
    ///
    /// scan::discover consults <DOTDIPPER_HOME>/.dotdipperignore; on a shared
    /// machine/CI runner that file may exist (created by other test binaries
    /// running `init`) and its default `**/tmp/**` pattern would exclude
    /// TempDir fixtures on Linux, where temp dirs live under /tmp.
    struct IsolatedHome {
        _dir: TempDir,
        prev: Option<std::ffi::OsString>,
    }

    impl IsolatedHome {
        fn new() -> Self {
            let dir = TempDir::new().unwrap();
            let prev = std::env::var_os("DOTDIPPER_HOME");
            std::env::set_var("DOTDIPPER_HOME", dir.path());
            Self { _dir: dir, prev }
        }
    }

    impl Drop for IsolatedHome {
        fn drop(&mut self) {
            match self.prev.take() {
                Some(v) => std::env::set_var("DOTDIPPER_HOME", v),
                None => std::env::remove_var("DOTDIPPER_HOME"),
            }
        }
    }

    #[test]
    #[serial]
    fn test_discover_empty_config() {
        let _home = IsolatedHome::new();
        let config = Config::default();

        // With empty include patterns and no tracked files
        let result = dotdipper::scan::discover(&config, false);

        // Should not error
        assert!(result.is_ok());
    }

    #[test]
    #[serial]
    fn test_discover_with_tracked_files() {
        let _home = IsolatedHome::new();
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.txt");
        fs::write(&test_file, "content").unwrap();

        let mut config = Config::default();
        config.general.tracked_files = vec![test_file.clone()];
        config.include_patterns = vec![]; // Clear default include patterns
        config.exclude_patterns = vec![];

        let result = dotdipper::scan::discover(&config, false).unwrap();

        // Should include the tracked file
        assert!(result.contains(&test_file));
    }

    #[test]
    #[serial]
    fn test_discover_sorts_results() {
        let _home = IsolatedHome::new();
        let temp_dir = TempDir::new().unwrap();
        let file_c = temp_dir.path().join("c.txt");
        let file_a = temp_dir.path().join("a.txt");
        let file_b = temp_dir.path().join("b.txt");

        fs::write(&file_c, "c").unwrap();
        fs::write(&file_a, "a").unwrap();
        fs::write(&file_b, "b").unwrap();

        let mut config = Config::default();
        config.general.tracked_files = vec![file_c.clone(), file_a.clone(), file_b.clone()];
        config.include_patterns = vec![];
        config.exclude_patterns = vec![];

        let result = dotdipper::scan::discover(&config, false).unwrap();

        // Results should be sorted
        let sorted: Vec<PathBuf> = {
            let mut v = vec![file_a, file_b, file_c];
            v.sort();
            v
        };

        assert_eq!(result, sorted);
    }

    #[test]
    #[serial]
    fn test_discover_removes_duplicates() {
        let _home = IsolatedHome::new();
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.txt");
        fs::write(&test_file, "content").unwrap();

        let mut config = Config::default();
        config.general.tracked_files = vec![test_file.clone(), test_file.clone()];
        config.include_patterns = vec![];
        config.exclude_patterns = vec![];

        let result = dotdipper::scan::discover(&config, false).unwrap();

        // Should have only one entry
        let count = result.iter().filter(|p| **p == test_file).count();
        assert_eq!(count, 1);
    }
}

#[cfg(test)]
mod exclude_pattern_tests {
    use ignore::gitignore::{Gitignore, GitignoreBuilder};
    use std::path::Path;

    fn build_excluder(patterns: &[String], home: &Path) -> Gitignore {
        let mut builder = GitignoreBuilder::new(home);

        for pattern in patterns {
            let _ = builder.add_line(None, pattern);
        }

        builder.build().unwrap()
    }

    #[test]
    fn test_excluder_matches_pattern() {
        let home = Path::new("/home/user");
        let patterns = vec!["**/*.bak".to_string()];
        let excluder = build_excluder(&patterns, home);

        let test_path = Path::new("/home/user/config.bak");
        assert!(excluder.matched(test_path, false).is_ignore());
    }

    #[test]
    fn test_excluder_no_match() {
        let home = Path::new("/home/user");
        let patterns = vec!["**/*.bak".to_string()];
        let excluder = build_excluder(&patterns, home);

        let test_path = Path::new("/home/user/config.txt");
        assert!(!excluder.matched(test_path, false).is_ignore());
    }

    #[test]
    fn test_excluder_multiple_patterns() {
        let home = Path::new("/home/user");
        let patterns = vec![
            "**/*.bak".to_string(),
            "**/node_modules/**".to_string(),
            "**/.git/**".to_string(),
        ];
        let excluder = build_excluder(&patterns, home);

        assert!(excluder
            .matched(Path::new("/home/user/file.bak"), false)
            .is_ignore());
        assert!(excluder
            .matched(Path::new("/home/user/project/node_modules/pkg"), true)
            .is_ignore());
        assert!(excluder
            .matched(Path::new("/home/user/repo/.git/config"), false)
            .is_ignore());
    }
}
