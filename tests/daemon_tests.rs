//! Integration tests for the daemon module

#[cfg(test)]
mod process_tests {
    use sysinfo::{Pid, System};

    #[test]
    fn test_current_process_running() {
        let current_pid = std::process::id() as i32;
        let mut sys = System::new_all();
        sys.refresh_all();

        // Current process should be running
        assert!(sys.process(Pid::from(current_pid as usize)).is_some());
    }

    #[test]
    fn test_invalid_process_not_running() {
        let invalid_pid = 999999i32;
        let mut sys = System::new_all();
        sys.refresh_all();

        // Invalid PID should not be running
        assert!(sys.process(Pid::from(invalid_pid as usize)).is_none());
    }
}

#[cfg(test)]
mod daemon_config_tests {
    use dotdipper::cfg::{Config, DaemonConfig};

    #[test]
    fn test_daemon_config_defaults() {
        let config = Config::default();
        assert!(config.daemon.is_none());
    }

    #[test]
    fn test_daemon_config_enabled() {
        let daemon_config = DaemonConfig {
            enabled: true,
            mode: "auto".to_string(),
            debounce_ms: 1500,
        };

        assert!(daemon_config.enabled);
        assert_eq!(daemon_config.mode, "auto");
        assert_eq!(daemon_config.debounce_ms, 1500);
    }

    #[test]
    fn test_daemon_config_disabled() {
        let daemon_config = DaemonConfig {
            enabled: false,
            mode: "ask".to_string(),
            debounce_ms: 2000,
        };

        assert!(!daemon_config.enabled);
    }

    #[test]
    fn test_daemon_config_modes() {
        let auto_mode = DaemonConfig {
            enabled: true,
            mode: "auto".to_string(),
            debounce_ms: 1500,
        };

        let ask_mode = DaemonConfig {
            enabled: true,
            mode: "ask".to_string(),
            debounce_ms: 1500,
        };

        assert_eq!(auto_mode.mode, "auto");
        assert_eq!(ask_mode.mode, "ask");
    }

    #[test]
    fn test_daemon_config_serialization() {
        let daemon_config = DaemonConfig {
            enabled: true,
            mode: "auto".to_string(),
            debounce_ms: 3000,
        };

        let toml = toml::to_string(&daemon_config).unwrap();
        assert!(toml.contains("enabled = true"));
        assert!(toml.contains("mode = \"auto\""));
        assert!(toml.contains("debounce_ms = 3000"));

        let deserialized: DaemonConfig = toml::from_str(&toml).unwrap();
        assert_eq!(deserialized.enabled, daemon_config.enabled);
        assert_eq!(deserialized.mode, daemon_config.mode);
        assert_eq!(deserialized.debounce_ms, daemon_config.debounce_ms);
    }
}

#[cfg(test)]
mod pid_file_tests {
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_pid_file_create_and_read() {
        let temp_dir = TempDir::new().unwrap();
        let pid_file = temp_dir.path().join("daemon.pid");

        let test_pid = 12345u32;
        fs::write(&pid_file, test_pid.to_string()).unwrap();

        assert!(pid_file.exists());

        let content = fs::read_to_string(&pid_file).unwrap();
        let parsed_pid: u32 = content.trim().parse().unwrap();

        assert_eq!(parsed_pid, test_pid);
    }

    #[test]
    fn test_pid_file_cleanup() {
        let temp_dir = TempDir::new().unwrap();
        let pid_file = temp_dir.path().join("daemon.pid");

        fs::write(&pid_file, "12345").unwrap();
        assert!(pid_file.exists());

        fs::remove_file(&pid_file).unwrap();
        assert!(!pid_file.exists());
    }

    #[test]
    fn test_pid_file_overwrite() {
        let temp_dir = TempDir::new().unwrap();
        let pid_file = temp_dir.path().join("daemon.pid");

        fs::write(&pid_file, "11111").unwrap();
        let first_content = fs::read_to_string(&pid_file).unwrap();
        assert_eq!(first_content.trim(), "11111");

        fs::write(&pid_file, "22222").unwrap();
        let second_content = fs::read_to_string(&pid_file).unwrap();
        assert_eq!(second_content.trim(), "22222");
    }
}

#[cfg(test)]
mod debounce_tests {
    use std::time::{Duration, Instant};

    #[test]
    fn test_debounce_duration_creation() {
        let debounce_ms = 1500u64;
        let debounce_duration = Duration::from_millis(debounce_ms);

        assert_eq!(debounce_duration.as_millis(), 1500);
    }

    #[test]
    fn test_instant_elapsed() {
        let start = Instant::now();
        std::thread::sleep(Duration::from_millis(10));
        let elapsed = start.elapsed();

        assert!(elapsed >= Duration::from_millis(10));
    }

    #[test]
    fn test_debounce_threshold() {
        let debounce_ms = 100u64;
        let debounce_duration = Duration::from_millis(debounce_ms);

        let start = Instant::now();
        std::thread::sleep(Duration::from_millis(50));

        // Should not have reached debounce threshold
        assert!(start.elapsed() < debounce_duration);

        std::thread::sleep(Duration::from_millis(60));

        // Should have reached debounce threshold
        assert!(start.elapsed() >= debounce_duration);
    }
}

#[cfg(test)]
mod file_watching_tests {
    use std::collections::HashSet;
    use std::path::PathBuf;

    #[test]
    fn test_tracked_files_set() {
        let mut tracked: HashSet<PathBuf> = HashSet::new();

        tracked.insert(PathBuf::from("/home/user/.zshrc"));
        tracked.insert(PathBuf::from("/home/user/.vimrc"));
        tracked.insert(PathBuf::from("/home/user/.zshrc")); // Duplicate

        assert_eq!(tracked.len(), 2);
        assert!(tracked.contains(&PathBuf::from("/home/user/.zshrc")));
        assert!(tracked.contains(&PathBuf::from("/home/user/.vimrc")));
    }

    #[test]
    fn test_pending_changes_clear() {
        let mut pending: HashSet<PathBuf> = HashSet::new();

        pending.insert(PathBuf::from("/home/user/.zshrc"));
        pending.insert(PathBuf::from("/home/user/.vimrc"));

        assert_eq!(pending.len(), 2);

        pending.clear();

        assert!(pending.is_empty());
    }

    #[test]
    fn test_parent_directory_extraction() {
        let file_path = PathBuf::from("/home/user/.config/nvim/init.lua");
        let parent = file_path.parent().unwrap();

        assert_eq!(parent, PathBuf::from("/home/user/.config/nvim"));
    }

    #[test]
    fn test_watched_directories_unique() {
        let files = vec![
            PathBuf::from("/home/user/.config/nvim/init.lua"),
            PathBuf::from("/home/user/.config/nvim/lua/plugins.lua"),
            PathBuf::from("/home/user/.zshrc"),
        ];

        let mut watched_dirs: HashSet<PathBuf> = HashSet::new();

        for file in &files {
            if let Some(parent) = file.parent() {
                watched_dirs.insert(parent.to_path_buf());
            }
        }

        // Should have 3 unique parent directories
        // /home/user/.config/nvim, /home/user/.config/nvim/lua, /home/user
        assert_eq!(watched_dirs.len(), 3);
    }
}
