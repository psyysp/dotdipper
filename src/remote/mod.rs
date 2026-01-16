/// Cloud Backups & Remotes (Milestone 5)
/// 
/// This module handles:
/// - Pluggable remote backends (GitHub, S3, GCS, WebDAV, LocalFS)
/// - Push/pull to cloud storage
/// - Bundle creation and extraction (tar.zst)
/// - Credentials management

mod bundle;

#[cfg(feature = "s3")]
mod s3_backend;

#[cfg(feature = "webdav")]
mod webdav_backend;

mod local_fs;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::cfg::Config;
use crate::ui;

/// Remote backend trait
pub trait Remote: Send + Sync {
    fn name(&self) -> &str;
    fn push_bundle(&self, bundle_path: &Path) -> Result<RemoteObject>;
    fn pull_latest(&self, dest_bundle: &Path) -> Result<RemoteObject>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteObject {
    pub etag_or_rev: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RemoteKind {
    GitHub,
    S3,
    GCS,
    WebDAV,
    LocalFS,
}

impl RemoteKind {
    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "github" => Ok(RemoteKind::GitHub),
            "s3" => Ok(RemoteKind::S3),
            "gcs" => Ok(RemoteKind::GCS),
            "webdav" => Ok(RemoteKind::WebDAV),
            "localfs" | "local" => Ok(RemoteKind::LocalFS),
            _ => bail!("Unknown remote kind: {}", s),
        }
    }
}

/// Configure a remote
pub fn set(_config: &Config, kind_str: &str, options: Vec<(String, String)>) -> Result<()> {
    let kind = RemoteKind::from_str(kind_str)?;
    
    ui::info(&format!("Configuring remote: {:?}", kind));
    
    // Parse options into a hashmap for easier lookup
    let opts: std::collections::HashMap<String, String> = options.into_iter().collect();
    
    // Get endpoint value, expanding ~ to home directory if present
    let endpoint = opts.get("endpoint").map(|e| {
        if e.starts_with("~/") {
            if let Some(home) = dirs::home_dir() {
                home.join(&e[2..]).to_string_lossy().to_string()
            } else {
                e.clone()
            }
        } else {
            e.clone()
        }
    });
    
    // Validate required options based on remote kind
    match kind {
        RemoteKind::LocalFS => {
            if endpoint.is_none() {
                bail!("LocalFS remote requires --endpoint (directory path).\n\
                       Example: dotdipper remote set localfs --endpoint ~/dotfiles-backup");
            }
        }
        RemoteKind::S3 => {
            if opts.get("bucket").is_none() {
                bail!("S3 remote requires --bucket.\n\
                       Example: dotdipper remote set s3 --bucket my-dotfiles --region us-east-1");
            }
        }
        RemoteKind::WebDAV => {
            if endpoint.is_none() {
                bail!("WebDAV remote requires --endpoint (URL).\n\
                       Example: dotdipper remote set webdav --endpoint https://dav.example.com/dotfiles");
            }
        }
        RemoteKind::GitHub | RemoteKind::GCS => {
            // GitHub uses vcs module, GCS may have different requirements
        }
    }
    
    // Update config with remote settings
    let dotdipper_dir = get_dotdipper_dir()?;
    let config_path = dotdipper_dir.join("config.toml");
    let mut cfg = if config_path.exists() {
        crate::cfg::load(&config_path)?
    } else {
        Config::default()
    };
    
    let remote_config = crate::cfg::RemoteConfig {
        kind: kind_str.to_lowercase(),
        bucket: opts.get("bucket").cloned(),
        prefix: opts.get("prefix").cloned(),
        region: opts.get("region").cloned(),
        endpoint,
    };
    
    cfg.remote = Some(remote_config);
    crate::cfg::save(&config_path, &cfg)?;
    
    ui::success(&format!("Remote configured: {}", kind_str));
    
    // Show configured values
    if let Some(ref remote) = cfg.remote {
        if let Some(ref e) = remote.endpoint {
            ui::info(&format!("  Endpoint: {}", e));
        }
        if let Some(ref b) = remote.bucket {
            ui::info(&format!("  Bucket: {}", b));
        }
        if let Some(ref r) = remote.region {
            ui::info(&format!("  Region: {}", r));
        }
        if let Some(ref p) = remote.prefix {
            ui::info(&format!("  Prefix: {}", p));
        }
    }
    
    if matches!(kind, RemoteKind::S3) {
        ui::hint("Set credentials via environment variables (AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY)");
    }
    
    Ok(())
}

/// Show current remote configuration
pub fn show(config: &Config) -> Result<()> {
    if let Some(remote_cfg) = &config.remote {
        ui::section("Remote Configuration:");
        println!("  Kind: {}", remote_cfg.kind);
        
        if let Some(bucket) = &remote_cfg.bucket {
            println!("  Bucket: {}", bucket);
        }
        if let Some(prefix) = &remote_cfg.prefix {
            println!("  Prefix: {}", prefix);
        }
        if let Some(region) = &remote_cfg.region {
            println!("  Region: {}", region);
        }
        if let Some(endpoint) = &remote_cfg.endpoint {
            println!("  Endpoint: {}", endpoint);
        }
    } else {
        ui::warn("No remote configured");
        ui::hint("Configure with: dotdipper remote set <kind>");
    }
    
    Ok(())
}

/// Push to remote
pub fn push(config: &Config, dry_run: bool) -> Result<()> {
    let remote_cfg = config.remote.as_ref()
        .context("No remote configured. Run 'dotdipper remote set <kind>' first")?;
    
    let remote = create_remote(remote_cfg)?;
    
    ui::info(&format!("Pushing to remote: {}", remote.name()));
    
    // Get active profile
    let profile_name = crate::profiles::active_profile_name()?;
    let profile_paths = crate::profiles::profile_paths(&profile_name)?;
    
    if !profile_paths.compiled.exists() {
        bail!("No compiled directory found. Run 'dotdipper snapshot' first");
    }
    
    // Create bundle
    let dotdipper_dir = get_dotdipper_dir()?;
    let bundle_path = dotdipper_dir.join("bundle.tar.zst");
    
    ui::info("Creating bundle...");
    let meta = bundle::pack(
        &profile_paths.compiled,
        &profile_paths.manifest,
        &bundle_path,
        &profile_name,
    )?;
    
    let size_str = humansize::format_size(meta.size_bytes, humansize::DECIMAL);
    ui::success(&format!("Bundle created: {} ({} files, {})", 
        bundle_path.display(), meta.file_count, size_str));
    
    if dry_run {
        ui::info("Dry run - skipping actual push");
        return Ok(());
    }
    
    // Push bundle
    ui::info("Uploading bundle...");
    let obj = remote.push_bundle(&bundle_path)?;
    
    let uploaded_size = humansize::format_size(obj.size_bytes, humansize::DECIMAL);
    ui::success(&format!("Pushed to remote: {} ({})", obj.etag_or_rev, uploaded_size));
    
    // Clean up bundle
    std::fs::remove_file(&bundle_path)?;
    
    Ok(())
}

/// Pull from remote
pub fn pull(config: &Config) -> Result<()> {
    let remote_cfg = config.remote.as_ref()
        .context("No remote configured")?;
    
    let remote = create_remote(remote_cfg)?;
    
    ui::info(&format!("Pulling from remote: {}", remote.name()));
    
    // Download bundle
    let dotdipper_dir = get_dotdipper_dir()?;
    let bundle_path = dotdipper_dir.join("bundle_download.tar.zst");
    
    ui::info("Downloading bundle...");
    let obj = remote.pull_latest(&bundle_path)?;
    
    let size_str = humansize::format_size(obj.size_bytes, humansize::DECIMAL);
    ui::success(&format!("Downloaded: {} ({})", obj.etag_or_rev, size_str));
    
    // Extract bundle
    ui::info("Extracting bundle...");
    let extracted_meta = bundle::unpack(&bundle_path, &dotdipper_dir)?;
    
    ui::success(&format!("Extracted {} files to profile: {}", 
        extracted_meta.file_count, extracted_meta.profile_name));
    
    // Clean up bundle
    std::fs::remove_file(&bundle_path)?;
    
    ui::hint("Apply changes with: dotdipper apply");
    
    Ok(())
}

fn create_remote(remote_cfg: &crate::cfg::RemoteConfig) -> Result<Box<dyn Remote>> {
    match remote_cfg.kind.as_str() {
        "localfs" | "local" => {
            let path = remote_cfg.endpoint.as_ref()
                .context("LocalFS remote requires 'endpoint' (directory path)")?;
            Ok(Box::new(local_fs::LocalFsRemote::new(path)?))
        },
        #[cfg(feature = "s3")]
        "s3" => {
            let bucket = remote_cfg.bucket.as_ref()
                .context("S3 remote requires 'bucket'")?;
            let region = remote_cfg.region.as_deref().unwrap_or("us-east-1");
            let prefix = remote_cfg.prefix.as_deref();
            Ok(Box::new(s3_backend::S3Remote::new_with_prefix(bucket, region, prefix)?))
        },
        #[cfg(feature = "webdav")]
        "webdav" => {
            let endpoint = remote_cfg.endpoint.as_ref()
                .context("WebDAV remote requires 'endpoint' URL")?;
            Ok(Box::new(webdav_backend::WebDavRemote::new(endpoint)?))
        },
        _ => {
            bail!("Remote kind '{}' not supported or feature not enabled", remote_cfg.kind);
        }
    }
}

fn get_dotdipper_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Failed to find home directory")?;
    Ok(home.join(".dotdipper"))
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_remote_kind_parse() {
        assert!(matches!(RemoteKind::from_str("s3").unwrap(), RemoteKind::S3));
        assert!(matches!(RemoteKind::from_str("localfs").unwrap(), RemoteKind::LocalFS));
        assert!(RemoteKind::from_str("invalid").is_err());
    }
}
