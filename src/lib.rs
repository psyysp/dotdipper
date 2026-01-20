//! Dotdipper - A smart dotfiles manager with GitHub sync and machine bootstrapping.
//!
//! This library provides the core functionality for dotdipper, including:
//! - Configuration management
//! - Dotfile discovery and scanning
//! - Package discovery from dotfiles
//! - Installation script generation
//! - Version control integration
//! - Secrets management

pub mod cfg;
pub mod daemon;
pub mod diff;
pub mod hash;
pub mod install;
pub mod profiles;
pub mod remote;
pub mod repo;
pub mod scan;
pub mod secrets;
pub mod snapshots;
pub mod ui;
pub mod vcs;
