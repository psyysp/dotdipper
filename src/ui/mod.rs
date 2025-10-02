use colored::*;
use console::style;
use indicatif::{ProgressBar, ProgressStyle};
use std::time::Duration;

pub fn init() {
    // Enable colored output on Windows
    #[cfg(windows)]
    colored::control::set_virtual_terminal(true).ok();
}

pub fn info(message: &str) {
    println!("{} {}", style("ℹ").blue(), message);
}

pub fn success(message: &str) {
    println!("{} {}", style("✓").green(), message.green());
}

pub fn error(message: &str) {
    eprintln!("{} {}", style("✗").red(), message.red());
}

pub fn warn(message: &str) {
    println!("{} {}", style("⚠").yellow(), message.yellow());
}

pub fn hint(message: &str) {
    println!("{} {}", style("💡").cyan(), message.dimmed());
}

pub fn section(title: &str) {
    println!("\n{}", title.bold().underline());
}

pub fn progress_bar(total: u64, message: &str) -> ProgressBar {
    let pb = ProgressBar::new(total);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} {msg}")
            .expect("Invalid progress bar template")
            .progress_chars("#>-"),
    );
    pb.set_message(message.to_string());
    pb.enable_steady_tick(Duration::from_millis(100));
    pb
}

pub fn spinner(message: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .expect("Invalid spinner template"),
    );
    pb.set_message(message.to_string());
    pb.enable_steady_tick(Duration::from_millis(100));
    pb
}

pub fn prompt_confirm(message: &str, default: bool) -> bool {
    dialoguer::Confirm::new()
        .with_prompt(message)
        .default(default)
        .interact()
        .unwrap_or(default)
}

pub fn prompt_text(message: &str, default: Option<&str>) -> String {
    let mut prompt = dialoguer::Input::new();
    prompt = prompt.with_prompt(message);
    
    if let Some(default_value) = default {
        prompt = prompt.default(default_value.to_string());
    }
    
    prompt.interact_text().unwrap_or_default()
}

pub fn prompt_select<T: ToString>(message: &str, items: &[T], default: usize) -> usize {
    dialoguer::Select::new()
        .with_prompt(message)
        .items(items)
        .default(default)
        .interact()
        .unwrap_or(default)
}

pub fn print_table(headers: &[&str], rows: Vec<Vec<String>>) {
    // Calculate column widths
    let mut widths = headers.iter().map(|h| h.len()).collect::<Vec<_>>();
    for row in &rows {
        for (i, cell) in row.iter().enumerate() {
            if i < widths.len() {
                widths[i] = widths[i].max(cell.len());
            }
        }
    }
    
    // Print headers
    for (i, header) in headers.iter().enumerate() {
        print!("{:width$} ", header.bold(), width = widths[i]);
    }
    println!();
    
    // Print separator
    for width in &widths {
        print!("{} ", "-".repeat(*width).dimmed());
    }
    println!();
    
    // Print rows
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            if i < widths.len() {
                print!("{:width$} ", cell, width = widths[i]);
            }
        }
        println!();
    }
}

pub fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB"];
    let mut size = bytes as f64;
    let mut unit_index = 0;
    
    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }
    
    format!("{:.1} {}", size, UNITS[unit_index])
}
