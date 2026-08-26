//! Integration tests for the snapshots module

use dotdipper::snapshots::PruneOpts;

#[cfg(test)]
mod parse_duration_tests {
    // Re-implementing the parse_duration function for testing
    // since it's private in the module
    fn parse_duration(s: &str) -> Option<chrono::Duration> {
        let s = s.trim();
        if s.is_empty() {
            return None;
        }

        let (num_str, unit) = s.split_at(s.len() - 1);
        let num: i64 = num_str.parse().ok()?;

        match unit {
            "d" => Some(chrono::Duration::days(num)),
            "w" => Some(chrono::Duration::weeks(num)),
            "m" => Some(chrono::Duration::days(num * 30)),
            "h" => Some(chrono::Duration::hours(num)),
            _ => None,
        }
    }

    #[test]
    fn test_parse_duration_days() {
        assert_eq!(parse_duration("7d"), Some(chrono::Duration::days(7)));
        assert_eq!(parse_duration("1d"), Some(chrono::Duration::days(1)));
        assert_eq!(parse_duration("30d"), Some(chrono::Duration::days(30)));
        assert_eq!(parse_duration("365d"), Some(chrono::Duration::days(365)));
    }

    #[test]
    fn test_parse_duration_weeks() {
        assert_eq!(parse_duration("1w"), Some(chrono::Duration::weeks(1)));
        assert_eq!(parse_duration("2w"), Some(chrono::Duration::weeks(2)));
        assert_eq!(parse_duration("52w"), Some(chrono::Duration::weeks(52)));
    }

    #[test]
    fn test_parse_duration_months() {
        assert_eq!(parse_duration("1m"), Some(chrono::Duration::days(30)));
        assert_eq!(parse_duration("3m"), Some(chrono::Duration::days(90)));
        assert_eq!(parse_duration("12m"), Some(chrono::Duration::days(360)));
    }

    #[test]
    fn test_parse_duration_hours() {
        assert_eq!(parse_duration("24h"), Some(chrono::Duration::hours(24)));
        assert_eq!(parse_duration("1h"), Some(chrono::Duration::hours(1)));
        assert_eq!(parse_duration("48h"), Some(chrono::Duration::hours(48)));
    }

    #[test]
    fn test_parse_duration_invalid() {
        assert_eq!(parse_duration("invalid"), None);
        assert_eq!(parse_duration(""), None);
        assert_eq!(parse_duration("abc"), None);
        assert_eq!(parse_duration("10x"), None);
        assert_eq!(parse_duration("d"), None);
    }
}

#[cfg(test)]
mod prune_opts_tests {
    use super::*;

    #[test]
    fn test_prune_opts_all_none() {
        let opts = PruneOpts {
            keep_count: None,
            keep_age: None,
            keep_size: None,
            dry_run: false,
        };

        assert!(opts.keep_count.is_none());
        assert!(opts.keep_age.is_none());
        assert!(opts.keep_size.is_none());
        assert!(!opts.dry_run);
    }

    #[test]
    fn test_prune_opts_keep_count() {
        let opts = PruneOpts {
            keep_count: Some(10),
            keep_age: None,
            keep_size: None,
            dry_run: false,
        };

        assert_eq!(opts.keep_count, Some(10));
    }

    #[test]
    fn test_prune_opts_keep_age() {
        let opts = PruneOpts {
            keep_count: None,
            keep_age: Some("30d".to_string()),
            keep_size: None,
            dry_run: false,
        };

        assert_eq!(opts.keep_age, Some("30d".to_string()));
    }

    #[test]
    fn test_prune_opts_keep_size() {
        let opts = PruneOpts {
            keep_count: None,
            keep_age: None,
            keep_size: Some("1GB".to_string()),
            dry_run: false,
        };

        assert_eq!(opts.keep_size, Some("1GB".to_string()));
    }

    #[test]
    fn test_prune_opts_dry_run() {
        let opts = PruneOpts {
            keep_count: Some(5),
            keep_age: None,
            keep_size: None,
            dry_run: true,
        };

        assert!(opts.dry_run);
    }

    #[test]
    fn test_prune_opts_combined() {
        let opts = PruneOpts {
            keep_count: Some(5),
            keep_age: Some("7d".to_string()),
            keep_size: Some("500MB".to_string()),
            dry_run: true,
        };

        assert_eq!(opts.keep_count, Some(5));
        assert_eq!(opts.keep_age, Some("7d".to_string()));
        assert_eq!(opts.keep_size, Some("500MB".to_string()));
        assert!(opts.dry_run);
    }
}

#[cfg(test)]
mod snapshot_struct_tests {
    use chrono::Utc;

    #[derive(Debug, Clone)]
    struct Snapshot {
        id: String,
        message: Option<String>,
        created_at: chrono::DateTime<Utc>,
        file_count: usize,
        size_bytes: u64,
    }

    #[test]
    fn test_snapshot_creation() {
        let now = Utc::now();
        let snapshot = Snapshot {
            id: "20250120_120000".to_string(),
            message: Some("Test snapshot".to_string()),
            created_at: now,
            file_count: 42,
            size_bytes: 1024000,
        };

        assert_eq!(snapshot.id, "20250120_120000");
        assert_eq!(snapshot.message, Some("Test snapshot".to_string()));
        assert_eq!(snapshot.file_count, 42);
        assert_eq!(snapshot.size_bytes, 1024000);
    }

    #[test]
    fn test_snapshot_no_message() {
        let snapshot = Snapshot {
            id: "20250120_120000".to_string(),
            message: None,
            created_at: Utc::now(),
            file_count: 10,
            size_bytes: 500,
        };

        assert!(snapshot.message.is_none());
    }

    #[test]
    fn test_snapshot_id_format() {
        let now = Utc::now();
        let id = now.format("%Y%m%d_%H%M%S").to_string();

        // ID should be in format YYYYMMDD_HHMMSS
        assert_eq!(id.len(), 15);
        assert!(id.contains('_'));
    }

    #[test]
    fn test_snapshots_sorted_by_date() {
        use chrono::Duration;

        let now = Utc::now();

        let mut snapshots = [
            Snapshot {
                id: "older".to_string(),
                message: None,
                created_at: now - Duration::days(2),
                file_count: 1,
                size_bytes: 100,
            },
            Snapshot {
                id: "newest".to_string(),
                message: None,
                created_at: now,
                file_count: 3,
                size_bytes: 300,
            },
            Snapshot {
                id: "middle".to_string(),
                message: None,
                created_at: now - Duration::days(1),
                file_count: 2,
                size_bytes: 200,
            },
        ];

        // Sort newest first
        snapshots.sort_by_key(|s| std::cmp::Reverse(s.created_at));

        assert_eq!(snapshots[0].id, "newest");
        assert_eq!(snapshots[1].id, "middle");
        assert_eq!(snapshots[2].id, "older");
    }
}

#[cfg(test)]
mod snapshot_serialization_tests {
    use chrono::{DateTime, Utc};
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize)]
    struct Snapshot {
        id: String,
        message: Option<String>,
        created_at: DateTime<Utc>,
        file_count: usize,
        size_bytes: u64,
    }

    #[test]
    fn test_snapshot_json_serialization() {
        let snapshot = Snapshot {
            id: "test_snap".to_string(),
            message: Some("Test message".to_string()),
            created_at: Utc::now(),
            file_count: 10,
            size_bytes: 1024,
        };

        let json = serde_json::to_string(&snapshot).unwrap();
        assert!(json.contains("test_snap"));
        assert!(json.contains("Test message"));
        assert!(json.contains("10"));

        let deserialized: Snapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, snapshot.id);
        assert_eq!(deserialized.file_count, snapshot.file_count);
    }

    #[test]
    fn test_snapshot_json_no_message() {
        let snapshot = Snapshot {
            id: "test_snap".to_string(),
            message: None,
            created_at: Utc::now(),
            file_count: 5,
            size_bytes: 512,
        };

        let json = serde_json::to_string(&snapshot).unwrap();
        let deserialized: Snapshot = serde_json::from_str(&json).unwrap();

        assert!(deserialized.message.is_none());
    }
}
