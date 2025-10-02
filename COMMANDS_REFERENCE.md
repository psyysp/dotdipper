# Dotdipper Commands Reference Card

Quick reference for all dotdipper commands.

## 🔧 Core Commands

```bash
dotdipper init [--force]                # Initialize dotdipper
dotdipper discover [--write] [--all]    # Find dotfiles
dotdipper snapshot [-f] [-m "msg"]      # Create snapshot
dotdipper status [--detailed]           # Show changes
dotdipper config --show | --edit        # View/edit config
dotdipper doctor [--fix]                # Health check
```

## 🔐 Secrets (Milestone 1) ✅

```bash
dotdipper secrets init                  # Setup age encryption
dotdipper secrets encrypt <file>        # Encrypt file → file.age
dotdipper secrets decrypt <file>        # Decrypt file.age
dotdipper secrets edit <file>.age       # Edit encrypted file

# Options
-o, --output <path>                     # Custom output path
```

**Examples:**

```bash
dotdipper secrets encrypt ~/.aws/credentials
dotdipper secrets edit ~/.ssh/config.age
```

## 🎯 Diff & Apply (Milestone 2) ✅

```bash
dotdipper diff [--detailed]             # Show differences
dotdipper apply [options]               # Apply dotfiles

# Apply options
-i, --interactive                       # Choose files (TUI)
--only <paths>                          # Filter paths (comma-separated)
-f, --force                             # No confirmations
--unsafe-allow-outside-home             # Allow operations outside $HOME
```

**Examples:**

```bash
dotdipper diff --detailed
dotdipper apply --interactive
dotdipper apply --only "~/.zshrc,~/.config/nvim"
dotdipper apply --only "~/.config/kitty"
dotdipper apply --force
```

## 🔄 GitHub Sync

```bash
dotdipper push [-m "msg"] [-f]          # Push to GitHub
dotdipper pull [--apply] [-f]           # Pull from GitHub

# Options
-m, --message <msg>                     # Commit message
-f, --force                             # Force operation
--apply                                 # Apply after pull
--unsafe-allow-outside-home             # Allow outside $HOME
```

**Typical workflow:**

```bash
dotdipper pull
dotdipper diff --detailed
dotdipper apply --interactive
```

## 📦 Installation

```bash
dotdipper install [options]             # Install packages

# Options
--dry-run                               # Generate scripts only
--target-os <os>                        # Target OS (auto-detected)
--unsafe-allow-outside-home             # Allow outside $HOME
```

## 🚧 Snapshots (Milestone 3 - Stub)

```bash
dotdipper snapshot-cmd create [-m "msg"]  # Create snapshot
dotdipper snapshot-cmd list               # List snapshots
dotdipper snapshot-cmd rollback <id>      # Rollback to snapshot
dotdipper snapshot-cmd delete <id>        # Delete snapshot
```

**Status:** Not yet implemented (returns helpful error)

## 🚧 Profiles (Milestone 4 - Stub)

```bash
dotdipper profile list                  # List profiles
dotdipper profile create <name>         # Create profile
dotdipper profile switch <name>         # Switch to profile
dotdipper profile remove <name>         # Remove profile
```

**Status:** Not yet implemented (returns helpful error)

## 🚧 Remote Backups (Milestone 5 - Stub)

```bash
dotdipper remote set <kind>             # Configure remote (s3/gcs/webdav)
dotdipper remote show                   # Show config
dotdipper remote push [--dry-run]       # Push to remote
dotdipper remote pull                   # Pull from remote
```

**Status:** Not yet implemented (returns helpful error)

## 🚧 Daemon (Milestone 6 - Stub)

```bash
dotdipper daemon start                  # Start auto-sync daemon
dotdipper daemon stop                   # Stop daemon
dotdipper daemon status                 # Check daemon status
```

**Status:** Not yet implemented (returns helpful error)

## 🌍 Global Flags

```bash
--verbose, -v                           # Verbose output
--config <path>                         # Custom config path
--help, -h                              # Show help
```

## 🎨 Common Workflows

### First-Time Setup

```bash
dotdipper init
dotdipper secrets init                  # If using encryption
dotdipper discover --write
dotdipper snapshot -m "Initial"
dotdipper push -m "Initial commit"
```

### Daily Updates

```bash
# After editing dotfiles
dotdipper snapshot -m "Updated configs"
dotdipper push -m "Update configurations"
```

### New Machine

```bash
dotdipper init
dotdipper pull
dotdipper diff --detailed               # Review changes
dotdipper apply --interactive           # Choose what to apply
dotdipper install                       # Install packages
```

### Managing Encrypted Files

```bash
# Add new secret
dotdipper secrets encrypt ~/.secret
# Track in config: tracked_files = ["~/.secret.age"]
dotdipper snapshot && dotdipper push

# Edit secret
dotdipper secrets edit ~/.secret.age

# On other machine
dotdipper pull && dotdipper apply       # Auto-decrypts
```

### Selective Updates

```bash
dotdipper pull                          # Get latest
dotdipper diff --detailed               # Review all
dotdipper apply --only "~/.zshrc"       # Apply one file
# or
dotdipper apply --interactive           # Pick from list
```

## 📋 Configuration Quick Reference

### Minimal Config

```toml
[general]
tracked_files = ["~/.zshrc", "~/.bashrc"]
```

### With Secrets

```toml
[general]
tracked_files = ["~/.zshrc"]

[secrets]
provider = "age"
key_path = "~/.config/age/keys.txt"
```

### With Hooks

```toml
[hooks]
post_apply = ["tmux source-file ~/.tmux.conf || true"]
```

### Per-File Overrides

```toml
[general]
default_mode = "symlink"

[files."~/.config/nvim"]
mode = "copy"                           # Copy instead of symlink

[files."~/.ssh/id_rsa"]
exclude = true                          # Never track
```

## 🔍 Troubleshooting Commands

```bash
# Check health
dotdipper doctor

# View current config
dotdipper config --show

# Check what's tracked
cat ~/.dotdipper/config.toml

# View manifest
cat ~/.dotdipper/manifest.lock | jq

# Check age setup
which age && which age-keygen

# Verify age key
cat ~/.config/age/keys.txt
```

## 💡 Tips & Tricks

### Tip 1: Always Review Before Apply

```bash
dotdipper pull && dotdipper diff --detailed && dotdipper apply --interactive
```

### Tip 2: Use Hooks for Reloading

```toml
[hooks]
post_apply = [
    "tmux source-file ~/.tmux.conf || true",
    "source ~/.zshrc || true"
]
```

### Tip 3: Encrypt First, Track Second

```bash
dotdipper secrets encrypt ~/.secret
rm ~/.secret
# Then add ~/.secret.age to tracked_files in config
```

### Tip 4: Copy Configs That Change

```toml
# Files you modify per-machine should be copied
[files."~/.gitconfig"]
mode = "copy"  # Allow local user.email
```

### Tip 5: Filter Updates by Directory

```bash
# Only update editor config
dotdipper apply --only "~/.config/nvim"

# Only update shell
dotdipper apply --only "~/.zshrc,~/.bashrc,~/.config/fish"
```

## 🎓 Exit Codes

- `0` - Success
- `1` - Error (check stderr for details)

## 📞 Getting Help

```bash
# General help
dotdipper --help

# Command help
dotdipper secrets --help
dotdipper apply --help
dotdipper diff --help

# Subcommand help
dotdipper secrets encrypt --help
```

## 🔗 Related Files

- **Config:** `~/.dotdipper/config.toml`
- **Manifest:** `~/.dotdipper/manifest.lock`
- **Compiled:** `~/.dotdipper/compiled/`
- **Age key:** `~/.config/age/keys.txt`
- **Backups:** `<file>.bak.<timestamp>`

## ⚡ Quick Fixes

### Age not found

```bash
brew install age  # macOS
apt install age   # Ubuntu
```

### Permission denied

```bash
chmod 600 ~/.config/age/keys.txt
dotdipper apply --force
```

### Diff fails

```bash
# Ensure git is installed
which git

# Or use summary mode
dotdipper diff  # Without --detailed
```

### Hook fails

```bash
# Make non-fatal
post_apply = ["command || true"]

# Test manually
sh -c "your-hook-command"
```

---

**Version:** 0.2.0 (Milestones 1-2)  
**Last Updated:** October 2, 2025  
**More Info:** See QUICK_START.md, README_FEATURES.md
