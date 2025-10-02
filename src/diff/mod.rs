use anyhow::{Context, Result};
use colored::*;
use dialoguer::{MultiSelect, theme::ColorfulTheme};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::cfg::Config;
use crate::hash::Manifest;
use crate::ui;

#[derive(Debug, Clone)]
pub struct DiffEntry {
    pub rel_path: PathBuf,
    pub source_path: PathBuf,
    pub target_path: PathBuf,
    pub status: DiffStatus,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DiffStatus {
    Modified,
    New,
    Missing,
    Identical,
}

impl DiffStatus {
    pub fn symbol(&self) -> ColoredString {
        match self {
            DiffStatus::Modified => "M".yellow(),
            DiffStatus::New => "A".green(),
            DiffStatus::Missing => "D".red(),
            DiffStatus::Identical => "=".dimmed(),
        }
    }
    
    pub fn description(&self) -> &str {
        match self {
            DiffStatus::Modified => "modified",
            DiffStatus::New => "new",
            DiffStatus::Missing => "missing from system",
            DiffStatus::Identical => "identical",
        }
    }
}

/// Generate diff between compiled files and system files
pub fn diff(
    compiled_root: &Path,
    manifest: &Manifest,
    _config: &Config,
    detailed: bool,
) -> Result<Vec<DiffEntry>> {
    let home_dir = dirs::home_dir().context("Failed to find home directory")?;
    let mut entries = Vec::new();
    
    ui::info("Computing differences...");
    
    // Sort manifest keys for deterministic output
    let mut manifest_files: Vec<_> = manifest.files.iter().collect();
    manifest_files.sort_by_key(|(path, _)| path.as_path());
    
    for (rel_path, file_hash) in manifest_files {
        let source_path = compiled_root.join(rel_path);
        let target_path = home_dir.join(rel_path);
        
        let status = if !target_path.exists() {
            DiffStatus::Missing
        } else if target_path.is_symlink() {
            // Check if symlink points to source
            match fs::read_link(&target_path) {
                Ok(link) if link == source_path => DiffStatus::Identical,
                _ => DiffStatus::Modified,
            }
        } else {
            // Compare hashes
            match crate::hash::hash_file(&target_path) {
                Ok(target_hash) => {
                    if target_hash.hash == file_hash.hash {
                        DiffStatus::Identical
                    } else {
                        DiffStatus::Modified
                    }
                }
                Err(_) => DiffStatus::Missing,
            }
        };
        
        entries.push(DiffEntry {
            rel_path: rel_path.clone(),
            source_path: source_path.clone(),
            target_path: target_path.clone(),
            status,
        });
    }
    
    // Print summary
    print_diff_summary(&entries, detailed)?;
    
    Ok(entries)
}

/// Print a summary of the diff
pub fn print_diff_summary(entries: &[DiffEntry], detailed: bool) -> Result<()> {
    let modified: Vec<_> = entries.iter().filter(|e| e.status == DiffStatus::Modified).collect();
    let new: Vec<_> = entries.iter().filter(|e| e.status == DiffStatus::New).collect();
    let missing: Vec<_> = entries.iter().filter(|e| e.status == DiffStatus::Missing).collect();
    let identical: Vec<_> = entries.iter().filter(|e| e.status == DiffStatus::Identical).collect();
    
    ui::section("Diff Summary");
    println!("  {} modified", modified.len().to_string().yellow());
    println!("  {} new (not yet on system)", new.len().to_string().green());
    println!("  {} missing from system", missing.len().to_string().red());
    println!("  {} identical", identical.len().to_string().dimmed());
    println!();
    
    // Show detailed listing
    if !modified.is_empty() {
        println!("{}", "Modified files:".yellow().bold());
        for entry in &modified {
            println!("  {} ~/{}", entry.status.symbol(), entry.rel_path.display());
            
            if detailed {
                show_file_diff(&entry.target_path, &entry.source_path)?;
            }
        }
        println!();
    }
    
    if !missing.is_empty() {
        println!("{}", "Missing from system:".red().bold());
        for entry in &missing {
            println!("  {} ~/{}", entry.status.symbol(), entry.rel_path.display());
        }
        println!();
    }
    
    if !new.is_empty() {
        println!("{}", "New files (not yet applied):".green().bold());
        for entry in &new {
            println!("  {} ~/{}", entry.status.symbol(), entry.rel_path.display());
        }
        println!();
    }
    
    Ok(())
}

/// Show detailed diff for a specific file
pub fn show_file_diff(target: &Path, source: &Path) -> Result<()> {
    // Check if files are binary
    if is_binary(source)? || (target.exists() && is_binary(target)?) {
        println!("    {}", "(binary file)".dimmed());
        if target.exists() {
            let source_size = fs::metadata(source)?.len();
            let target_size = fs::metadata(target)?.len();
            println!("    Source: {} bytes, Target: {} bytes", source_size, target_size);
        }
        return Ok(());
    }
    
    // Use git diff for text files
    if target.exists() {
        let output = Command::new("git")
            .arg("diff")
            .arg("--no-index")
            .arg("--color=always")
            .arg("--")
            .arg(target)
            .arg(source)
            .output();
        
        match output {
            Ok(out) if out.status.code() == Some(1) || out.status.success() => {
                // Exit code 1 means differences found, which is expected
                let diff_output = String::from_utf8_lossy(&out.stdout);
                // Print diff with indentation
                for line in diff_output.lines().skip(4) {  // Skip header lines
                    println!("    {}", line);
                }
            }
            _ => {
                // Fallback to simple comparison
                println!("    {}", "Differs from source".yellow());
            }
        }
    } else {
        println!("    {}", "File missing from system".red());
    }
    
    Ok(())
}

/// Check if a file is binary
fn is_binary(path: &Path) -> Result<bool> {
    if !path.exists() || !path.is_file() {
        return Ok(false);
    }
    
    let mut file = fs::File::open(path)?;
    let mut buffer = [0u8; 8192];
    use std::io::Read;
    let n = file.read(&mut buffer)?;
    
    // Check for null bytes (simple binary detection)
    Ok(buffer[..n].contains(&0))
}

/// Interactive file selection for apply
pub fn interactive_select(entries: &[DiffEntry]) -> Result<Vec<PathBuf>> {
    // Filter to only files that can be applied (not identical)
    let applicable: Vec<_> = entries.iter()
        .filter(|e| e.status != DiffStatus::Identical)
        .collect();
    
    if applicable.is_empty() {
        ui::info("All files are already up to date");
        return Ok(vec![]);
    }
    
    let items: Vec<String> = applicable.iter()
        .map(|e| format!("{} ~/{}", e.status.symbol(), e.rel_path.display()))
        .collect();
    
    ui::section("Select files to apply");
    
    let selections = MultiSelect::with_theme(&ColorfulTheme::default())
        .items(&items)
        .interact()?;
    
    let selected_paths: Vec<PathBuf> = selections
        .into_iter()
        .map(|i| applicable[i].rel_path.clone())
        .collect();
    
    Ok(selected_paths)
}

/// Filter entries by path patterns
pub fn filter_by_paths(entries: Vec<DiffEntry>, filter_paths: &[String]) -> Result<Vec<DiffEntry>> {
    if filter_paths.is_empty() {
        return Ok(entries);
    }
    
    let home_dir = dirs::home_dir().context("Failed to find home directory")?;
    
    // Expand and normalize filter paths
    let normalized_filters: Vec<PathBuf> = filter_paths.iter()
        .map(|p| {
            let expanded = shellexpand::tilde(p).to_string();
            let path = PathBuf::from(expanded);
            if path.is_absolute() {
                path.strip_prefix(&home_dir).unwrap_or(&path).to_path_buf()
            } else {
                path.strip_prefix("~/").unwrap_or(&path).to_path_buf()
            }
        })
        .collect();
    
    // Filter entries
    let filtered = entries.into_iter()
        .filter(|entry| {
            normalized_filters.iter().any(|filter| {
                entry.rel_path.starts_with(filter) || entry.rel_path == *filter
            })
        })
        .collect();
    
    Ok(filtered)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_diff_status_symbol() {
        assert_eq!(DiffStatus::Modified.description(), "modified");
        assert_eq!(DiffStatus::New.description(), "new");
        assert_eq!(DiffStatus::Missing.description(), "missing from system");
        assert_eq!(DiffStatus::Identical.description(), "identical");
    }
}

