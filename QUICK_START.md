# Dotdipper Quick Start Guide

> Fast-track guide to get dotdipper up and running. For comprehensive documentation, see [README.md](README.md).

📚 **Quick Navigation:** [Main README](README.md) | [Commands Reference](COMMANDS_REFERENCE.md) | [Feature Overview](README_FEATURES.md) | [Architecture](ARCHITECTURE.md)

---

## Prerequisites

Before installing dotdipper, ensure you have:

- **Rust and Cargo** - Install from [rustup.rs](https://rustup.rs/)
- **Git** - Required for GitHub sync
- **Age** (optional but recommended) - For secrets encryption

```bash
# Install age
# macOS
brew install age

# Ubuntu/Debian
sudo apt install age

# Arch Linux
sudo pacman -S age
```

## Installation

```bash
# Clone the repository
git clone https://github.com/yourusername/dotdipper
cd dotdipper

# Build and install
cargo build --release
cargo install --path .

# Verify installation
dotdipper --version
```

## Initial Setup

```bash
# Initialize dotdipper
dotdipper init

# Discover dotfiles on your system
dotdipper discover --write

# Initialize secrets management (if using encrypted files)
dotdipper secrets init
```

## Milestone 1: Secrets Encryption

### Setup Age Encryption

```bash
# Initialize age keys (auto-generates ~/.config/age/keys.txt)
dotdipper secrets init

# Your public key will be displayed - save it for sharing encrypted files
```

### Encrypt/Decrypt Files

```bash
# Encrypt a file
dotdipper secrets encrypt ~/.ssh/config
# Creates: ~/.ssh/config.age

# Decrypt a file
dotdipper secrets decrypt ~/.ssh/config.age
# Creates: ~/.ssh/config

# Edit an encrypted file (decrypt → edit → re-encrypt)
dotdipper secrets edit ~/.ssh/config.age
```

### Configuration

Add to your `~/.dotdipper/config.toml`:

```toml
[secrets]
provider = "age"
key_path = "~/.config/age/keys.txt"
```

### Usage in Dotfiles Workflow

1. **Encrypt sensitive files before committing:**

   ```bash
   dotdipper secrets encrypt ~/.aws/credentials
   git add ~/.aws/credentials.age
   git commit -m "Add AWS credentials (encrypted)"
   ```

2. **On new machine:**

   ```bash
   # Copy your age key first
   cp /secure/location/keys.txt ~/.config/age/keys.txt
   
   # Pull your dotfiles
   dotdipper pull
   
   # Decrypt secrets
   dotdipper secrets decrypt ~/.aws/credentials.age
   ```

## Milestone 2: Selective Apply & Diff

### Check What Would Change

```bash
# Quick summary
dotdipper diff

# Detailed diff with file contents
dotdipper diff --detailed
```

### Apply Changes

```bash
# Apply all changes (prompts before overwriting)
dotdipper apply

# Apply with force (no prompts)
dotdipper apply --force

# Interactive selection - choose which files to apply
dotdipper apply --interactive

# Apply only specific files
dotdipper apply --only "~/.zshrc,~/.gitconfig"

# Apply only files in a directory
dotdipper apply --only "~/.config/nvim"
```

### Typical Workflow

```bash
# 1. Pull latest dotfiles from GitHub
dotdipper pull

# 2. See what would change
dotdipper diff --detailed

# 3. Apply selectively
dotdipper apply --interactive

# OR apply specific files
dotdipper apply --only "~/.zshrc,~/.tmux.conf"
```

## Hooks System

### Configuration

Add to `~/.dotdipper/config.toml`:

```toml
[hooks]
# Run before applying dotfiles
pre_apply = [
    "echo 'Backing up current config...'",
    "cp ~/.zshrc ~/.zshrc.backup"
]

# Run after applying dotfiles
post_apply = [
    "source ~/.zshrc",
    "tmux source-file ~/.tmux.conf || true",
    "echo 'Dotfiles applied successfully!'"
]

# Run before/after creating snapshots
pre_snapshot = ["echo 'Creating snapshot...'"]
post_snapshot = ["git add -A && git commit -m 'Auto-snapshot'"]
```

### Use Cases

1. **Reload configurations after apply:**

   ```toml
   post_apply = [
       "tmux source-file ~/.tmux.conf || true",
       "killall -USR1 kitty || true"  # Reload kitty config
   ]
   ```

2. **Auto-commit after snapshots:**

   ```toml
   post_snapshot = [
       "cd ~/.dotdipper/compiled",
       "git add -A",
       "git commit -m 'Snapshot $(date)' || true"
   ]
   ```

3. **Validate configs before applying:**

   ```toml
   pre_apply = [
       "zsh -n ~/.zshrc || exit 1",  # Syntax check
       "tmux -f ~/.tmux.conf || exit 1"
   ]
   ```

## Complete Workflow Examples

### Example 1: New Machine Setup

```bash
# 1. Install dotdipper and age
brew install age  # or apt install age
cargo install --path .

# 2. Initialize
dotdipper init

# 3. Configure GitHub
# Edit ~/.dotdipper/config.toml and set your GitHub username/repo

# 4. Pull your dotfiles
dotdipper pull

# 5. Copy your age key (for encrypted files)
# From secure location: ~/.config/age/keys.txt

# 6. Review changes
dotdipper diff --detailed

# 7. Apply (choose which files)
dotdipper apply --interactive

# 8. Install packages
dotdipper install
```

### Example 2: Daily Workflow

```bash
# Make changes to your dotfiles
vim ~/.zshrc

# Create snapshot
dotdipper snapshot -m "Updated zsh aliases"

# Push to GitHub
dotdipper push -m "Update zsh configuration"
```

### Example 3: Managing Secrets

```bash
# Add encrypted AWS credentials
dotdipper secrets encrypt ~/.aws/credentials
mv ~/.aws/credentials.age ~/dotfiles/aws/credentials.age
rm ~/.aws/credentials  # Remove plaintext

# Add to tracked files
# Edit config.toml:
# tracked_files = ["~/dotfiles/aws/credentials.age"]

# On new machine:
dotdipper pull
dotdipper secrets decrypt ~/dotfiles/aws/credentials.age -o ~/.aws/credentials
chmod 600 ~/.aws/credentials
```

### Example 4: Per-File Overrides

```toml
[general]
default_mode = "symlink"  # Default behavior

# Specific overrides
[files."~/.config/nvim"]
mode = "copy"  # Copy nvim config (allow local changes)

[files."~/.ssh/config"]
exclude = true  # Don't manage SSH config

[files."~/.gitconfig"]
mode = "copy"  # Copy (not symlink) to allow local Git settings
```

## Health Check

```bash
# Run diagnostics
dotdipper doctor

# Check for:
# - Git installed
# - GitHub CLI (gh) installed
# - Age encryption tools installed
# - Config file valid
# - Manifest valid
```

## Troubleshooting

### Issue: Age command not found

```bash
# macOS
brew install age

# Ubuntu/Debian
sudo apt install age

# Arch
sudo pacman -S age
```

### Issue: Permission denied on apply

```bash
# Check file ownership
ls -la ~/.dotdipper/compiled/

# Fix with:
dotdipper apply --force
```

### Issue: Hook fails

```bash
# Check hook syntax in config.toml
dotdipper config --show

# Test hook manually:
sh -c "your-hook-command"

# Make hooks non-fatal with || true:
post_apply = ["tmux source-file ~/.tmux.conf || true"]
```

### Issue: Diff shows unexpected changes

```bash
# Check file hashes
dotdipper status --detailed

# Force re-snapshot
dotdipper snapshot --force
```

## Advanced Configuration

### Full config.toml Example

```toml
[general]
default_mode = "symlink"
backup = true
active_profile = "default"
tracked_files = [
    "~/.zshrc",
    "~/.bashrc",
    "~/.config/nvim",
    "~/.tmux.conf",
    "~/.gitconfig"
]

[github]
username = "yourname"
repo_name = "dotfiles"
private = true

[secrets]
provider = "age"
key_path = "~/.config/age/keys.txt"

[hooks]
pre_apply = ["echo 'Applying dotfiles...'"]
post_apply = [
    "tmux source-file ~/.tmux.conf || true",
    "source ~/.zshrc"
]

[files."~/.config/nvim"]
mode = "copy"

[files."~/.ssh/config"]
exclude = true

include_patterns = [
    "~/.config/**",
    "~/.zshrc",
    "~/.bashrc",
    "~/.gitconfig"
]

exclude_patterns = [
    "~/.ssh/**",
    "~/.gnupg/**",
    "**/*.key",
    "**/node_modules/**"
]
```

## Command Reference

### Core Commands

```bash
dotdipper init              # Initialize dotdipper
dotdipper discover          # Find dotfiles
dotdipper snapshot          # Create snapshot
dotdipper status            # Show changes
dotdipper push              # Push to GitHub
dotdipper pull              # Pull from GitHub
dotdipper install           # Install packages
dotdipper doctor            # Health check
```

### New Commands (Milestone 1 & 2)

```bash
# Secrets
dotdipper secrets init
dotdipper secrets encrypt <file> [-o <output>]
dotdipper secrets decrypt <file> [-o <output>]
dotdipper secrets edit <file>

# Diff & Apply
dotdipper diff [--detailed]
dotdipper apply [--interactive] [--only <paths>] [--force]
```

### Future Commands (Stubs)

```bash
# Snapshots (Milestone 3)
dotdipper snapshot-cmd create|list|rollback|delete

# Profiles (Milestone 4)
dotdipper profile list|create|switch|remove

# Remotes (Milestone 5)
dotdipper remote set|show|push|pull

# Daemon (Milestone 6)
dotdipper daemon start|stop|status
```

## Getting Help

```bash
# General help
dotdipper --help

# Command-specific help
dotdipper secrets --help
dotdipper apply --help
dotdipper diff --help
```

## Tips

1. **Always review diffs before applying:**

   ```bash
   dotdipper diff --detailed && dotdipper apply --interactive
   ```

2. **Use hooks for automation:**
   - Reload configs after apply
   - Auto-commit after snapshots
   - Validate syntax before apply

3. **Encrypt sensitive files:**
   - SSH keys
   - AWS credentials
   - API tokens
   - GPG keys

4. **Use per-file overrides:**
   - Copy instead of symlink for configs that change
   - Exclude truly sensitive files
   - Symlink stable configs

5. **Test on a VM first:**

   ```bash
   # In VM
   dotdipper pull
   dotdipper diff --detailed  # Review changes
   dotdipper apply --interactive  # Selective test
   ```

## Next Steps

1. **Customize your config:** Edit `~/.dotdipper/config.toml`
2. **Add hooks:** Automate your workflow
3. **Encrypt secrets:** Use `dotdipper secrets` for sensitive files
4. **Set up GitHub sync:** Configure GitHub username/repo
5. **Test on another machine:** Verify your setup works

## Resources

- **[Main README](README.md)** - Comprehensive project documentation
- **[Commands Reference](COMMANDS_REFERENCE.md)** - All commands and options
- **[Feature Overview](README_FEATURES.md)** - Detailed feature descriptions
- **[Architecture](ARCHITECTURE.md)** - System design and modules
- **[Example Config](example-config.toml)** - Full configuration example
- **[Milestone Status](MILESTONE_STATUS.md)** - Implementation status
- **[Tests](tests/)** - Integration tests and examples
- **[Source Code](src/)** - Project source code

## What's Next?

Now that you've completed the quick start:

1. 📖 Read the [comprehensive README](README.md) for all features
2. 🔧 Customize your [config.toml](example-config.toml)
3. 🪝 Set up [hooks](#hooks-system) for automation
4. 🔐 Learn about [secrets management](#milestone-1-secrets-encryption)
5. 🎯 Explore [selective apply](#milestone-2-selective-apply--diff)
6. 🧪 Test your setup on a VM before production use

## Support

- **Issues:** Report bugs via GitHub Issues
- **Documentation:** See [README.md](README.md) for detailed docs
- **Help:** Run `dotdipper --help` or `dotdipper <command> --help`

---

**Version:** 0.2.0 (Milestones 1-2 Complete)  
**Status:** Production-ready  
**Happy dotfile management! 🚀**

---

**[← Back to Main README](README.md)**
