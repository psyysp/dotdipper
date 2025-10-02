/// Auto-Sync Daemon (Milestone 6)
/// 
/// This module handles:
/// - Watching filesystem for changes to tracked dotfiles
/// - Debouncing file events to avoid excessive snapshots
/// - Auto-snapshotting or prompting on drift detection
/// - Graceful start/stop/status with PID file management

use anyhow::{Context, Result, bail};
use notify::{Watcher, RecursiveMode, Event as NotifyEvent};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};
use sysinfo::{System, Pid};

use crate::cfg::Config;
use crate::ui;

const DAEMON_PID_FILE: &str = "daemon.pid";
const DEFAULT_DEBOUNCE_MS: u64 = 1500;

/// Start the daemon
pub fn start(config: &Config) -> Result<()> {
    let dotdipper_dir = get_dotdipper_dir()?;
    let pid_file = dotdipper_dir.join(DAEMON_PID_FILE);
    
    // Check if already running
    if pid_file.exists() {
        let pid_str = fs::read_to_string(&pid_file)?;
        if let Ok(pid) = pid_str.trim().parse::<i32>() {
            if is_process_running(pid) {
                bail!("Daemon is already running (PID: {})", pid);
            } else {
                ui::warn("Stale PID file found, removing...");
                fs::remove_file(&pid_file)?;
            }
        }
    }
    
    let daemon_config = config.daemon.as_ref()
        .context("Daemon configuration not found in config")?;
    
    if !daemon_config.enabled {
        bail!("Daemon is disabled in configuration. Enable it first.");
    }
    
    let mode = daemon_config.mode.as_str();
    let debounce_ms = daemon_config.debounce_ms;
    
    ui::info(&format!("Starting daemon in '{}' mode (debounce: {}ms)...", mode, debounce_ms));
    
    // Get tracked files
    let tracked_files: Vec<PathBuf> = config.general.tracked_files.clone();
    
    if tracked_files.is_empty() {
        bail!("No tracked files configured. Add files with 'dotdipper discover --write'");
    }
    
    ui::info(&format!("Watching {} files", tracked_files.len()));
    
    // Write PID file
    let current_pid = std::process::id();
    fs::write(&pid_file, current_pid.to_string())?;
    
    ui::success(&format!("Daemon started (PID: {})", current_pid));
    ui::hint("Stop with: dotdipper daemon stop");
    
    // Run daemon loop
    match run_daemon_loop(tracked_files, debounce_ms, mode) {
        Ok(_) => {
            ui::info("Daemon stopped gracefully");
        }
        Err(e) => {
            ui::error(&format!("Daemon error: {}", e));
            // Clean up PID file on error
            let _ = fs::remove_file(&pid_file);
            return Err(e);
        }
    }
    
    // Clean up PID file
    let _ = fs::remove_file(&pid_file);
    
    Ok(())
}

/// Stop the daemon
pub fn stop(_config: &Config) -> Result<()> {
    let dotdipper_dir = get_dotdipper_dir()?;
    let pid_file = dotdipper_dir.join(DAEMON_PID_FILE);
    
    if !pid_file.exists() {
        bail!("Daemon is not running (no PID file found)");
    }
    
    let pid_str = fs::read_to_string(&pid_file)?;
    let pid = pid_str.trim().parse::<i32>()
        .context("Invalid PID in PID file")?;
    
    if !is_process_running(pid) {
        ui::warn("Daemon is not running (stale PID file)");
        fs::remove_file(&pid_file)?;
        return Ok(());
    }
    
    ui::info(&format!("Stopping daemon (PID: {})...", pid));
    
    // Send SIGTERM to process
    #[cfg(unix)]
    {
        use std::process::Command;
        Command::new("kill")
            .arg(pid.to_string())
            .status()?;
    }
    
    #[cfg(not(unix))]
    {
        bail!("Daemon stop not supported on non-Unix systems");
    }
    
    // Wait for process to stop
    for _ in 0..10 {
        std::thread::sleep(Duration::from_millis(500));
        if !is_process_running(pid) {
            break;
        }
    }
    
    if is_process_running(pid) {
        ui::warn("Daemon did not stop gracefully, forcing...");
        #[cfg(unix)]
        {
            use std::process::Command;
            Command::new("kill")
                .arg("-9")
                .arg(pid.to_string())
                .status()?;
        }
    }
    
    fs::remove_file(&pid_file)?;
    ui::success("Daemon stopped");
    
    Ok(())
}

/// Check daemon status
pub fn status(_config: &Config) -> Result<()> {
    let dotdipper_dir = get_dotdipper_dir()?;
    let pid_file = dotdipper_dir.join(DAEMON_PID_FILE);
    
    if !pid_file.exists() {
        ui::info("Daemon is not running");
        return Ok(());
    }
    
    let pid_str = fs::read_to_string(&pid_file)?;
    let pid = pid_str.trim().parse::<i32>()
        .context("Invalid PID in PID file")?;
    
    if is_process_running(pid) {
        ui::success(&format!("Daemon is running (PID: {})", pid));
    } else {
        ui::warn("Daemon is not running (stale PID file)");
        ui::hint("Clean up with: dotdipper daemon stop");
    }
    
    Ok(())
}

// Private helper functions

fn run_daemon_loop(tracked_files: Vec<PathBuf>, debounce_ms: u64, mode: &str) -> Result<()> {
    // Set up file watcher
    let (tx, rx) = channel();
    
    let mut watcher = notify::recommended_watcher(move |res: Result<NotifyEvent, notify::Error>| {
        if let Ok(event) = res {
            let _ = tx.send(event);
        }
    })?;
    
    // Watch tracked files and their parent directories
    let mut watched_dirs: HashSet<PathBuf> = HashSet::new();
    
    for file in &tracked_files {
        if let Some(parent) = file.parent() {
            if !watched_dirs.contains(parent) {
                if let Err(e) = watcher.watch(parent, RecursiveMode::NonRecursive) {
                    ui::warn(&format!("Failed to watch {}: {}", parent.display(), e));
                } else {
                    watched_dirs.insert(parent.to_path_buf());
                }
            }
        }
    }
    
    ui::info(&format!("Watching {} directories", watched_dirs.len()));
    
    // Debouncing state
    let mut last_event_time: Option<Instant> = None;
    let mut pending_changes: HashSet<PathBuf> = HashSet::new();
    let debounce_duration = Duration::from_millis(debounce_ms);
    
    // Main event loop
    loop {
        // Use timeout to periodically check for debounced events
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(event) => {
                // Process event
                for path in event.paths {
                    if tracked_files.contains(&path) {
                        pending_changes.insert(path.clone());
                        last_event_time = Some(Instant::now());
                        ui::info(&format!("Change detected: {}", path.display()));
                    }
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                // Check if we should process pending changes
                if let Some(last_time) = last_event_time {
                    if last_time.elapsed() >= debounce_duration && !pending_changes.is_empty() {
                        // Process changes
                        ui::info(&format!("Processing {} changed files...", pending_changes.len()));
                        
                        match mode {
                            "auto" => {
                                handle_changes_auto(&pending_changes)?;
                            }
                            "ask" => {
                                handle_changes_ask(&pending_changes)?;
                            }
                            _ => {
                                ui::warn(&format!("Unknown daemon mode: {}", mode));
                            }
                        }
                        
                        // Reset state
                        pending_changes.clear();
                        last_event_time = None;
                    }
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                break;
            }
        }
    }
    
    Ok(())
}

fn handle_changes_auto(changed_files: &HashSet<PathBuf>) -> Result<()> {
    ui::info("Auto-creating snapshot...");
    
    // Load config
    let dotdipper_dir = get_dotdipper_dir()?;
    let config_path = dotdipper_dir.join("config.toml");
    let config = crate::cfg::load(&config_path)?;
    
    // Create snapshot
    let _message = format!("Auto-snapshot: {} files changed", changed_files.len());
    let snapshot = crate::repo::snapshot(&config, false)?;
    
    ui::success(&format!("Snapshot created with {} files", snapshot.file_count));
    
    Ok(())
}

fn handle_changes_ask(changed_files: &HashSet<PathBuf>) -> Result<()> {
    ui::warn(&format!("{} files changed", changed_files.len()));
    
    for file in changed_files.iter().take(5) {
        println!("  {}", file.display());
    }
    
    if changed_files.len() > 5 {
        println!("  ... and {} more", changed_files.len() - 5);
    }
    
    let create_snapshot = dialoguer::Confirm::new()
        .with_prompt("Create snapshot now?")
        .default(true)
        .interact()?;
    
    if create_snapshot {
        let dotdipper_dir = get_dotdipper_dir()?;
        let config_path = dotdipper_dir.join("config.toml");
        let config = crate::cfg::load(&config_path)?;
        
        let _message = format!("Manual snapshot: {} files changed", changed_files.len());
        let snapshot = crate::repo::snapshot(&config, false)?;
        
        ui::success(&format!("Snapshot created with {} files", snapshot.file_count));
    } else {
        ui::info("Skipped snapshot");
    }
    
    Ok(())
}

fn is_process_running(pid: i32) -> bool {
    let mut sys = System::new_all();
    sys.refresh_all();
    
    sys.process(Pid::from(pid as usize)).is_some()
}

fn get_dotdipper_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Failed to find home directory")?;
    Ok(home.join(".dotdipper"))
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_is_process_running() {
        // Test with current process (should always be running)
        let current_pid = std::process::id() as i32;
        assert!(is_process_running(current_pid));
        
        // Test with invalid PID
        assert!(!is_process_running(999999));
    }
}
