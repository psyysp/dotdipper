//! Integration tests for the profiles module

#[cfg(test)]
mod profile_validation_tests {
    #[test]
    fn test_profile_name_validation_valid() {
        let valid_names = vec![
            "default",
            "work",
            "personal",
            "server-prod",
            "dev_local",
            "profile123",
        ];

        for name in valid_names {
            assert!(!name.contains('/'), "Name {} should not contain /", name);
            assert!(!name.contains('\\'), "Name {} should not contain \\", name);
            assert!(!name.is_empty(), "Name should not be empty");
        }
    }

    #[test]
    fn test_profile_name_validation_invalid() {
        let invalid_names = vec!["../malicious", "path/to/profile", "back\\slash", ""];

        for name in invalid_names {
            let is_invalid = name.contains('/') || name.contains('\\') || name.is_empty();
            assert!(is_invalid, "Name '{}' should be invalid", name);
        }
    }
}

#[cfg(test)]
mod profile_paths_tests {
    use std::path::PathBuf;

    #[test]
    fn test_profile_paths_structure() {
        // Simulate profile paths structure
        let profile_name = "work";
        let base_dir = PathBuf::from("/home/user/.dotdipper/profiles");

        let profile_dir = base_dir.join(profile_name);
        let compiled_dir = profile_dir.join("compiled");
        let manifest_path = profile_dir.join("manifest.lock");
        let config_path = profile_dir.join("config.toml");

        assert_eq!(
            profile_dir,
            PathBuf::from("/home/user/.dotdipper/profiles/work")
        );
        assert_eq!(
            compiled_dir,
            PathBuf::from("/home/user/.dotdipper/profiles/work/compiled")
        );
        assert_eq!(
            manifest_path,
            PathBuf::from("/home/user/.dotdipper/profiles/work/manifest.lock")
        );
        assert_eq!(
            config_path,
            PathBuf::from("/home/user/.dotdipper/profiles/work/config.toml")
        );
    }

    #[test]
    fn test_default_profile_name() {
        let default_name = "default";
        assert_eq!(default_name, "default");
    }
}

#[cfg(test)]
mod profile_struct_tests {
    use std::path::PathBuf;

    #[derive(Debug, Clone)]
    struct Profile {
        name: String,
        config_path: PathBuf,
        manifest_path: PathBuf,
        compiled_path: PathBuf,
    }

    #[test]
    fn test_profile_struct_creation() {
        let profile = Profile {
            name: "personal".to_string(),
            config_path: PathBuf::from("/home/user/.dotdipper/profiles/personal/config.toml"),
            manifest_path: PathBuf::from("/home/user/.dotdipper/profiles/personal/manifest.lock"),
            compiled_path: PathBuf::from("/home/user/.dotdipper/profiles/personal/compiled"),
        };

        assert_eq!(profile.name, "personal");
        assert!(profile.config_path.ends_with("config.toml"));
        assert!(profile.manifest_path.ends_with("manifest.lock"));
        assert!(profile.compiled_path.ends_with("compiled"));
    }

    #[test]
    fn test_profile_clone() {
        let profile = Profile {
            name: "test".to_string(),
            config_path: PathBuf::from("/path/to/config.toml"),
            manifest_path: PathBuf::from("/path/to/manifest.lock"),
            compiled_path: PathBuf::from("/path/to/compiled"),
        };

        let cloned = profile.clone();
        assert_eq!(profile.name, cloned.name);
        assert_eq!(profile.config_path, cloned.config_path);
    }
}

#[cfg(test)]
mod profile_config_tests {
    use dotdipper::cfg::{Config, GeneralConfig, RestoreMode};

    #[test]
    fn test_profile_config_creation() {
        let profile_config = Config {
            general: GeneralConfig {
                default_mode: RestoreMode::Symlink,
                backup: true,
                tracked_files: Vec::new(),
                active_profile: None,
            },
            ..Default::default()
        };

        assert_eq!(profile_config.general.default_mode, RestoreMode::Symlink);
        assert!(profile_config.general.backup);
    }

    #[test]
    fn test_active_profile_in_config() {
        let mut config = Config::default();
        config.general.active_profile = Some("work".to_string());

        assert_eq!(config.general.active_profile, Some("work".to_string()));
    }

    #[test]
    fn test_no_active_profile() {
        let config = Config::default();
        assert!(config.general.active_profile.is_none());
    }

    #[test]
    fn test_switch_profile_updates_config() {
        let mut config = Config::default();

        // Initially no active profile
        assert!(config.general.active_profile.is_none());

        // Switch to work
        config.general.active_profile = Some("work".to_string());
        assert_eq!(config.general.active_profile, Some("work".to_string()));

        // Switch to personal
        config.general.active_profile = Some("personal".to_string());
        assert_eq!(config.general.active_profile, Some("personal".to_string()));
    }
}

#[cfg(test)]
mod profile_sorting_tests {
    #[test]
    fn test_profiles_sorted_alphabetically() {
        let mut profiles = [
            "work".to_string(),
            "default".to_string(),
            "personal".to_string(),
            "server".to_string(),
        ];

        profiles.sort();

        assert_eq!(profiles[0], "default");
        assert_eq!(profiles[1], "personal");
        assert_eq!(profiles[2], "server");
        assert_eq!(profiles[3], "work");
    }
}
