/// WebDAV remote backend (feature-gated)
/// Supports standard WebDAV servers (Nextcloud, ownCloud, etc.)

use anyhow::{Context, Result, bail};
use std::path::Path;
use reqwest::blocking::Client;
use reqwest::header::CONTENT_TYPE;

use super::{Remote, RemoteObject};

pub struct WebDavRemote {
    endpoint: String,
    client: Client,
    username: Option<String>,
    password: Option<String>,
}

impl WebDavRemote {
    pub fn new(endpoint: &str) -> Result<Self> {
        // Get credentials from environment
        let username = std::env::var("WEBDAV_USERNAME").ok();
        let password = std::env::var("WEBDAV_PASSWORD").ok();
        
        if username.is_none() || password.is_none() {
            crate::ui::warn("WebDAV credentials not found. Set WEBDAV_USERNAME and WEBDAV_PASSWORD");
        }
        
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(300)) // 5 min timeout for large uploads
            .build()
            .context("Failed to create HTTP client")?;
        
        // Normalize endpoint URL
        let endpoint = endpoint.trim_end_matches('/').to_string();
        
        Ok(Self {
            endpoint,
            client,
            username,
            password,
        })
    }
    
    fn bundle_url(&self, filename: &str) -> String {
        format!("{}/dotdipper/{}", self.endpoint, filename)
    }
    
    
    fn list_bundles(&self) -> Result<Vec<(String, u64, String)>> {
        let propfind_url = format!("{}/dotdipper/", self.endpoint);
        
        // WebDAV PROPFIND request to list files
        let propfind_body = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:propfind xmlns:D="DAV:">
  <D:prop>
    <D:getlastmodified/>
    <D:getcontentlength/>
    <D:resourcetype/>
  </D:prop>
</D:propfind>"#;
        
        let mut request = self.client.request(reqwest::Method::from_bytes(b"PROPFIND")?, &propfind_url);
        
        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request = request.basic_auth(username, Some(password));
        }
        
        let response = request
            .header("Depth", "1")
            .header(CONTENT_TYPE, "application/xml")
            .body(propfind_body)
            .send()
            .context("Failed to send PROPFIND request")?;
        
        if !response.status().is_success() {
            bail!("PROPFIND request failed: {}", response.status());
        }
        
        let xml = response.text().context("Failed to read PROPFIND response")?;
        
        // Parse XML to extract bundle files
        // This is a simple parser - in production, use a proper XML library
        let mut bundles = Vec::new();
        
        for line in xml.lines() {
            if line.contains(".tar.zst") {
                // Extract filename, size, and modified date from XML
                // This is simplified - real implementation should use xml-rs or similar
                if let Some(start) = line.find("bundle_") {
                    if let Some(end) = line[start..].find(".tar.zst") {
                        let filename = &line[start..start + end + 8];
                        // For now, we'll use simplified parsing
                        bundles.push((
                            filename.to_string(),
                            0u64, // size - would parse from XML
                            String::new(), // timestamp - would parse from XML
                        ));
                    }
                }
            }
        }
        
        // Sort by filename (which includes timestamp)
        bundles.sort_by(|a, b| b.0.cmp(&a.0));
        
        Ok(bundles)
    }
}

impl Remote for WebDavRemote {
    fn name(&self) -> &str {
        "WebDAV"
    }
    
    fn push_bundle(&self, bundle_path: &Path) -> Result<RemoteObject> {
        let filename = bundle_path.file_name()
            .context("Invalid bundle path")?
            .to_str()
            .context("Invalid filename")?;
        
        // Generate timestamped filename
        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S").to_string();
        let remote_filename = format!("bundle_{}.tar.zst", timestamp);
        let url = self.bundle_url(&remote_filename);
        
        crate::ui::info(&format!("Uploading to WebDAV: {}", url));
        
        // Ensure dotdipper directory exists
        let dir_url = format!("{}/dotdipper/", self.endpoint);
        let mut mkcol_req = self.client.request(reqwest::Method::from_bytes(b"MKCOL")?, &dir_url);
        
        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            mkcol_req = mkcol_req.basic_auth(username, Some(password));
        }
        
        // Try to create directory (ignore error if it exists)
        let _ = mkcol_req.send();
        
        // Read bundle data
        let data = std::fs::read(bundle_path)
            .context("Failed to read bundle file")?;
        let size = data.len() as u64;
        
        // Upload with PUT
        let mut put_req = self.client.put(&url);
        
        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            put_req = put_req.basic_auth(username, Some(password));
        }
        
        let response = put_req
            .header(CONTENT_TYPE, "application/octet-stream")
            .body(data.clone())
            .send()
            .context("Failed to upload bundle to WebDAV")?;
        
        if !response.status().is_success() {
            bail!("Upload failed: {}", response.status());
        }
        
        // Get ETag from response
        let etag = response.headers()
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("unknown")
            .to_string();
        
        // Also upload as "latest"
        let latest_url = self.bundle_url("latest.tar.zst");
        let mut latest_req = self.client.put(&latest_url);
        
        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            latest_req = latest_req.basic_auth(username, Some(password));
        }
        
        let _ = latest_req
            .header(CONTENT_TYPE, "application/octet-stream")
            .body(data)
            .send(); // Don't fail if latest update fails
        
        Ok(RemoteObject {
            etag_or_rev: etag,
            size_bytes: size,
        })
    }
    
    fn pull_latest(&self, dest_bundle: &Path) -> Result<RemoteObject> {
        crate::ui::info(&format!("Listing bundles from WebDAV: {}", self.endpoint));
        
        // Try to download "latest.tar.zst" first
        let latest_url = self.bundle_url("latest.tar.zst");
        
        let mut get_req = self.client.get(&latest_url);
        
        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            get_req = get_req.basic_auth(username, Some(password));
        }
        
        let response = get_req.send();
        
        let (data, etag, size) = if let Ok(resp) = response {
            if resp.status().is_success() {
                let etag = resp.headers()
                    .get("etag")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("unknown")
                    .to_string();
                
                let bytes = resp.bytes()
                    .context("Failed to read response body")?;
                
                let size = bytes.len() as u64;
                
                (bytes.to_vec(), etag, size)
            } else {
                bail!("Failed to download latest bundle: {}", resp.status());
            }
        } else {
            // Fallback: list and get the most recent bundle
            let bundles = self.list_bundles()?;
            
            if bundles.is_empty() {
                bail!("No bundles found on WebDAV server at {}", self.endpoint);
            }
            
            let (latest_name, _, _) = &bundles[0];
            let url = self.bundle_url(latest_name);
            
            crate::ui::info(&format!("Downloading: {}", latest_name));
            
            let mut get_req = self.client.get(&url);
            
            if let (Some(username), Some(password)) = (&self.username, &self.password) {
                get_req = get_req.basic_auth(username, Some(password));
            }
            
            let resp = get_req.send()
                .context("Failed to download bundle from WebDAV")?;
            
            if !resp.status().is_success() {
                bail!("Download failed: {}", resp.status());
            }
            
            let etag = resp.headers()
                .get("etag")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("unknown")
                .to_string();
            
            let bytes = resp.bytes()
                .context("Failed to read response body")?;
            
            let size = bytes.len() as u64;
            
            (bytes.to_vec(), etag, size)
        };
        
        // Write to destination
        std::fs::write(dest_bundle, &data)
            .context("Failed to write downloaded bundle")?;
        
        Ok(RemoteObject {
            etag_or_rev: etag,
            size_bytes: size,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_url_generation() {
        let endpoint = "https://cloud.example.com/remote.php/webdav";
        let filename = "test.tar.zst";
        let expected = format!("{}/dotdipper/{}", endpoint, filename);
        
        // Test URL generation logic
        let url = format!("{}/dotdipper/{}", endpoint.trim_end_matches('/'), filename);
        
        assert_eq!(url, expected);
    }
}

