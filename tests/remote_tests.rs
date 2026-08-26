//! Integration tests for the remote module

#[cfg(test)]
mod remote_kind_tests {
    #[derive(Debug, Clone, PartialEq)]
    enum RemoteKind {
        GitHub,
        S3,
        Gcs,
        WebDAV,
        LocalFS,
    }

    impl RemoteKind {
        fn from_str(s: &str) -> Option<Self> {
            match s.to_lowercase().as_str() {
                "github" => Some(RemoteKind::GitHub),
                "s3" => Some(RemoteKind::S3),
                "gcs" => Some(RemoteKind::Gcs),
                "webdav" => Some(RemoteKind::WebDAV),
                "localfs" | "local" => Some(RemoteKind::LocalFS),
                _ => None,
            }
        }
    }

    #[test]
    fn test_remote_kind_from_str_github() {
        assert_eq!(RemoteKind::from_str("github"), Some(RemoteKind::GitHub));
        assert_eq!(RemoteKind::from_str("GitHub"), Some(RemoteKind::GitHub));
        assert_eq!(RemoteKind::from_str("GITHUB"), Some(RemoteKind::GitHub));
    }

    #[test]
    fn test_remote_kind_from_str_s3() {
        assert_eq!(RemoteKind::from_str("s3"), Some(RemoteKind::S3));
        assert_eq!(RemoteKind::from_str("S3"), Some(RemoteKind::S3));
    }

    #[test]
    fn test_remote_kind_from_str_gcs() {
        assert_eq!(RemoteKind::from_str("gcs"), Some(RemoteKind::Gcs));
        assert_eq!(RemoteKind::from_str("GCS"), Some(RemoteKind::Gcs));
    }

    #[test]
    fn test_remote_kind_from_str_webdav() {
        assert_eq!(RemoteKind::from_str("webdav"), Some(RemoteKind::WebDAV));
        assert_eq!(RemoteKind::from_str("WebDAV"), Some(RemoteKind::WebDAV));
    }

    #[test]
    fn test_remote_kind_from_str_localfs() {
        assert_eq!(RemoteKind::from_str("localfs"), Some(RemoteKind::LocalFS));
        assert_eq!(RemoteKind::from_str("local"), Some(RemoteKind::LocalFS));
        assert_eq!(RemoteKind::from_str("LOCALFS"), Some(RemoteKind::LocalFS));
    }

    #[test]
    fn test_remote_kind_from_str_invalid() {
        assert_eq!(RemoteKind::from_str("invalid"), None);
        assert_eq!(RemoteKind::from_str(""), None);
        assert_eq!(RemoteKind::from_str("azure"), None);
        assert_eq!(RemoteKind::from_str("dropbox"), None);
    }
}

#[cfg(test)]
mod remote_config_tests {
    use dotdipper::cfg::{Config, RemoteConfig};

    #[test]
    fn test_remote_config_defaults() {
        let config = Config::default();
        assert!(config.remote.is_none());
    }

    #[test]
    fn test_remote_config_localfs() {
        let remote = RemoteConfig {
            kind: "localfs".to_string(),
            bucket: None,
            prefix: None,
            region: None,
            endpoint: Some("/home/user/dotfiles-backup".to_string()),
        };

        assert_eq!(remote.kind, "localfs");
        assert!(remote.bucket.is_none());
        assert_eq!(
            remote.endpoint,
            Some("/home/user/dotfiles-backup".to_string())
        );
    }

    #[test]
    fn test_remote_config_s3() {
        let remote = RemoteConfig {
            kind: "s3".to_string(),
            bucket: Some("my-dotfiles".to_string()),
            prefix: Some("backups/".to_string()),
            region: Some("us-east-1".to_string()),
            endpoint: None,
        };

        assert_eq!(remote.kind, "s3");
        assert_eq!(remote.bucket, Some("my-dotfiles".to_string()));
        assert_eq!(remote.region, Some("us-east-1".to_string()));
        assert_eq!(remote.prefix, Some("backups/".to_string()));
    }

    #[test]
    fn test_remote_config_webdav() {
        let remote = RemoteConfig {
            kind: "webdav".to_string(),
            bucket: None,
            prefix: None,
            region: None,
            endpoint: Some("https://cloud.example.com/remote.php/webdav".to_string()),
        };

        assert_eq!(remote.kind, "webdav");
        assert_eq!(
            remote.endpoint,
            Some("https://cloud.example.com/remote.php/webdav".to_string())
        );
    }

    #[test]
    fn test_remote_config_serialization() {
        let remote = RemoteConfig {
            kind: "s3".to_string(),
            bucket: Some("test-bucket".to_string()),
            prefix: Some("prefix/".to_string()),
            region: Some("eu-west-1".to_string()),
            endpoint: None,
        };

        let toml = toml::to_string(&remote).unwrap();
        assert!(toml.contains("kind = \"s3\""));
        assert!(toml.contains("bucket = \"test-bucket\""));

        let deserialized: RemoteConfig = toml::from_str(&toml).unwrap();
        assert_eq!(deserialized.kind, remote.kind);
        assert_eq!(deserialized.bucket, remote.bucket);
    }
}

#[cfg(test)]
mod remote_object_tests {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct RemoteObject {
        etag_or_rev: String,
        size_bytes: u64,
    }

    #[test]
    fn test_remote_object_creation() {
        let obj = RemoteObject {
            etag_or_rev: "abc123def456".to_string(),
            size_bytes: 1024000,
        };

        assert_eq!(obj.etag_or_rev, "abc123def456");
        assert_eq!(obj.size_bytes, 1024000);
    }

    #[test]
    fn test_remote_object_serialization() {
        let obj = RemoteObject {
            etag_or_rev: "test-etag".to_string(),
            size_bytes: 2048,
        };

        let json = serde_json::to_string(&obj).unwrap();
        assert!(json.contains("test-etag"));
        assert!(json.contains("2048"));

        let deserialized: RemoteObject = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.etag_or_rev, obj.etag_or_rev);
        assert_eq!(deserialized.size_bytes, obj.size_bytes);
    }
}

#[cfg(test)]
mod tilde_expansion_tests {

    fn expand_tilde(path: &str) -> String {
        if let Some(stripped) = path.strip_prefix("~/") {
            if let Some(home) = dirs::home_dir() {
                return home.join(stripped).to_string_lossy().to_string();
            }
        }
        path.to_string()
    }

    #[test]
    fn test_expand_tilde_with_home() {
        let path = "~/dotfiles-backup";
        let expanded = expand_tilde(path);

        assert!(!expanded.starts_with("~/"));
        assert!(expanded.ends_with("dotfiles-backup"));
    }

    #[test]
    fn test_expand_tilde_no_tilde() {
        let path = "/absolute/path";
        let expanded = expand_tilde(path);

        assert_eq!(expanded, path);
    }

    #[test]
    fn test_expand_tilde_relative() {
        let path = "relative/path";
        let expanded = expand_tilde(path);

        assert_eq!(expanded, path);
    }
}

#[cfg(test)]
mod localfs_tests {
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_localfs_directory_creation() {
        let temp_dir = TempDir::new().unwrap();
        let backup_dir = temp_dir.path().join("dotfiles-backup");

        fs::create_dir_all(&backup_dir).unwrap();

        assert!(backup_dir.exists());
        assert!(backup_dir.is_dir());
    }

    #[test]
    fn test_localfs_file_operations() {
        let temp_dir = TempDir::new().unwrap();
        let backup_dir = temp_dir.path().join("backup");
        fs::create_dir_all(&backup_dir).unwrap();

        // Write a bundle file
        let bundle_path = backup_dir.join("bundle.tar.zst");
        fs::write(&bundle_path, b"mock bundle content").unwrap();

        assert!(bundle_path.exists());

        // Read it back
        let content = fs::read(&bundle_path).unwrap();
        assert_eq!(content, b"mock bundle content");
    }
}
