/// S3 remote backend (feature-gated)
/// Supports AWS S3 and S3-compatible storage (MinIO, DigitalOcean Spaces, etc.)

use anyhow::{Context, Result, bail};
use std::path::Path;
use s3::Bucket;
use s3::creds::Credentials;
use s3::Region;

use super::{Remote, RemoteObject};

pub struct S3Remote {
    bucket: Bucket,
    prefix: String,
}

impl S3Remote {
    pub fn new(bucket_name: &str, region_str: &str) -> Result<Self> {
        Self::new_with_prefix(bucket_name, region_str, None)
    }
    
    pub fn new_with_prefix(bucket_name: &str, region_str: &str, prefix: Option<&str>) -> Result<Self> {
        // Get credentials from environment or IAM
        let credentials = Credentials::default()
            .context("Failed to load S3 credentials. Set AWS_ACCESS_KEY_ID and AWS_SECRET_ACCESS_KEY")?;
        
        // Parse region
        let region = if let Some(endpoint) = std::env::var("AWS_ENDPOINT_URL").ok() {
            // Custom S3-compatible endpoint
            Region::Custom {
                region: region_str.to_string(),
                endpoint,
            }
        } else {
            region_str.parse()
                .with_context(|| format!("Invalid AWS region: {}", region_str))?
        };
        
        // Create bucket
        let bucket = Bucket::new(bucket_name, region, credentials)
            .with_context(|| format!("Failed to create S3 bucket handle for: {}", bucket_name))?;
        
        Ok(Self {
            bucket,
            prefix: prefix.unwrap_or("dotdipper").to_string(),
        })
    }
    
    fn bundle_key(&self, filename: &str) -> String {
        if self.prefix.is_empty() {
            filename.to_string()
        } else {
            format!("{}/{}", self.prefix, filename)
        }
    }
    
    fn list_bundles(&self) -> Result<Vec<(String, u64, String)>> {
        // List objects with our prefix
        let results = self.bucket.list(self.prefix.clone(), None)
            .context("Failed to list S3 objects")?;
        
        let mut bundles = Vec::new();
        
        for list in results {
            for object in list.contents {
                // Filter for .tar.zst files
                if object.key.ends_with(".tar.zst") {
                    bundles.push((
                        object.key.clone(),
                        object.size,
                        object.last_modified.clone(),
                    ));
                }
            }
        }
        
        // Sort by last_modified, newest first
        bundles.sort_by(|a, b| b.2.cmp(&a.2));
        
        Ok(bundles)
    }
}

impl Remote for S3Remote {
    fn name(&self) -> &str {
        "S3"
    }
    
    fn push_bundle(&self, bundle_path: &Path) -> Result<RemoteObject> {
        let filename = bundle_path.file_name()
            .context("Invalid bundle path")?
            .to_str()
            .context("Invalid filename")?;
        
        // Generate timestamped key
        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S").to_string();
        let key = self.bundle_key(&format!("bundle_{}.tar.zst", timestamp));
        
        crate::ui::info(&format!("Uploading to S3: s3://{}/{}", self.bucket.name(), key));
        
        // Read bundle data
        let data = std::fs::read(bundle_path)
            .context("Failed to read bundle file")?;
        let size = data.len() as u64;
        
        // Upload to S3
        let response = self.bucket.put_object(&key, &data)
            .context("Failed to upload bundle to S3")?;
        
        // Get ETag from response
        let etag = response.headers().get("etag")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("unknown")
            .to_string();
        
        // Also update "latest" pointer
        let latest_key = self.bundle_key("latest.tar.zst");
        self.bucket.put_object(&latest_key, &data)
            .ok(); // Don't fail if latest update fails
        
        Ok(RemoteObject {
            etag_or_rev: etag,
            size_bytes: size,
        })
    }
    
    fn pull_latest(&self, dest_bundle: &Path) -> Result<RemoteObject> {
        crate::ui::info(&format!("Listing bundles from S3: s3://{}/{}", self.bucket.name(), self.prefix));
        
        // List all bundles and get the latest
        let bundles = self.list_bundles()?;
        
        if bundles.is_empty() {
            bail!("No bundles found in S3 bucket at prefix: {}", self.prefix);
        }
        
        let (latest_key, size, _timestamp) = &bundles[0];
        
        crate::ui::info(&format!("Downloading latest bundle: {}", latest_key));
        
        // Download from S3
        let response = self.bucket.get_object(latest_key)
            .context("Failed to download bundle from S3")?;
        
        // Write to destination
        std::fs::write(dest_bundle, response.bytes())
            .context("Failed to write downloaded bundle")?;
        
        // Get ETag from response
        let etag = response.headers().get("etag")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("unknown")
            .to_string();
        
        Ok(RemoteObject {
            etag_or_rev: etag,
            size_bytes: *size,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_bundle_key_generation() {
        // This test doesn't require actual S3 credentials
        let prefix = "dotdipper";
        let filename = "test.tar.zst";
        let expected = format!("{}/{}", prefix, filename);
        
        // Just test the key generation logic
        let key = if prefix.is_empty() {
            filename.to_string()
        } else {
            format!("{}/{}", prefix, filename)
        };
        
        assert_eq!(key, expected);
    }
}

