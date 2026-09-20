# Dotdipper

> A safe, deterministic, and feature-rich dotfiles manager built in Rust with encryption, selective apply, snapshots, profiles, and cloud sync.

[![Built with Rust](https://img.shields.io/badge/built%20with-Rust-orange.svg)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

---

## 🎯 What is Dotdipper?

Dotdipper is a comprehensive dotfiles manager that helps you synchronize, manage, and deploy your configuration files across multiple machines. Built with safety and determinism as core principles, it provides powerful features for managing dotfiles at scale.

### Key Features

- 🔐 **Secrets Encryption** - Age encryption for sensitive files with in-memory decryption
- 🎯 **Selective Apply** - Interactive TUI to choose which files to apply
- 📸 **Snapshot Management** - Create, list, and rollback to previous snapshots
- 👤 **Multiple Profiles** - Separate configs for work, personal, servers, etc.
- ☁️ **Cloud Backups** - Push/pull to LocalFS, S3, or WebDAV remotes
- 🤖 **Auto-Sync Daemon** - Watch files and auto-snapshot on changes
- 🪝 **Hooks System** - Automate workflows with pre/post hooks
- 🔄 **GitHub Sync** - Push/pull dotfiles to/from GitHub
- 🪞 **Public Mirror** - Publish a sanitized copy to a separate public repo, driven by a generated allowlist and a fail-closed secret scan
- 📦 **Package Management** - Auto-discover and install system packages from dotfiles
- 🔍 **Smart Diff** - Git-style diffs before applying changes
- 🛡️ **Safety First** - Backups, confirmations, and HOME boundary enforcement

---

## 🚀 Quick Start

### Installation

#### Homebrew (macOS and Linux) - Recommended

```bash
brew install psyysp/dotdipper/dotdipper
```

One command — the three-part name taps and installs in a single step. This also installs `age`, required for secrets encryption.

#### Arch Linux (AUR)

```bash
# Using yay
yay -S dotdipper

# Or using paru
paru -S dotdipper

# Binary version (faster install)
yay -S dotdipper-bin
```

#### Nix / NixOS

The repo provides a flake at the repo root. The Nix package wraps the binary so `age` is on `PATH` (secrets encryption works without a separate `age` install).

```bash
# Install into your user profile (recommended)
nix profile install github:psyysp/dotdipper

# Or from a local clone
git clone https://github.com/psyysp/dotdipper && cd dotdipper
nix profile install .#dotdipper
```

**NixOS (flake-based config):** add to your flake inputs `dotdipper.url = "github:psyysp/dotdipper";`, then in `environment.systemPackages` (or Home Manager `home.packages`) add `inputs.dotdipper.packages.${pkgs.system}.default`.

**Development shell** (Rust + pkg-config, openssl, age):

```bash
nix develop
```

#### Cargo (Rust)

```bash
# Install from crates.io
cargo install dotdipper

# Or from source
cargo install --git https://github.com/psyysp/dotdipper
```

#### Manual Binary Download

```bash
# macOS Apple Silicon (M1/M2/M3)
curl -LO https://github.com/psyysp/dotdipper/releases/latest/download/dotdipper-aarch64-apple-darwin.tar.gz
tar -xzf dotdipper-aarch64-apple-darwin.tar.gz
sudo mv dotdipper /usr/local/bin/

# macOS Intel
curl -LO https://github.com/psyysp/dotdipper/releases/latest/download/dotdipper-x86_64-apple-darwin.tar.gz
tar -xzf dotdipper-x86_64-apple-darwin.tar.gz
sudo mv dotdipper /usr/local/bin/

# Linux x86_64
curl -LO https://github.com/psyysp/dotdipper/releases/latest/download/dotdipper-x86_64-unknown-linux-gnu.tar.gz
tar -xzf dotdipper-x86_64-unknown-linux-gnu.tar.gz
sudo mv dotdipper /usr/local/bin/

# Linux ARM64
curl -LO https://github.com/psyysp/dotdipper/releases/latest/download/dotdipper-aarch64-unknown-linux-gnu.tar.gz
tar -xzf dotdipper-aarch64-unknown-linux-gnu.tar.gz
sudo mv dotdipper /usr/local/bin/
```

#### Build from Source

```bash
# Prerequisites: Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Clone and build
git clone https://github.com/psyysp/dotdipper
cd dotdipper
cargo install --path .

# Verify
dotdipper --version
```

#### Install age (Required for Secrets)

When using the Nix flake, `age` is included on the binary’s `PATH`—no separate install needed. For other install methods:

```bash
# macOS
brew install age

# Ubuntu/Debian
sudo apt install age

# Arch Linux
sudo pacman -S age

# Fedora
sudo dnf install age

# Nix (if not using the dotdipper flake)
nix profile install nixpkgs#age
```

### First-Time Setup

```bash
# 1. Initialize
dotdipper init

# 2. Setup secrets (optional)
dotdipper secrets init

# 3. Discover dotfiles
dotdipper discover --write

# 4. Create initial snapshot
dotdipper snapshot create -m "Initial snapshot"

# 5. Push to GitHub (configure GitHub section in config first)
#    If the repo already exists with a README etc., push will fetch, rebase, and push for you
dotdipper push -m "Initial commit"

# 6. Undo the last pushed commit if needed
dotdipper undo
```

### New Machine Setup

```bash
# 1. Install dotdipper (see above)
# 2. Initialize
dotdipper init

# 3. Pull your dotfiles (restores compiled/ + manifest.lock from git)
dotdipper pull

# 4. Preview what would change in $HOME
dotdipper diff --detailed

# 5. Apply selectively (creates .bak backups by default)
dotdipper apply --interactive

# 6. Install discovered packages, then place dotfiles safely
dotdipper install

# Optional: inspect or share the generated setup script (does not run it)
dotdipper install script
dotdipper install script -o setup_dotfiles.sh
```

`pull` only updates the local compiled store. Your live `$HOME` files are unchanged until you `apply` (or `pull --apply` / `install`). Keep `general.backup = true` so existing files are copied to `.bak.<timestamp>` before overwrite.

`dotdipper install` writes `setup_dotfiles.sh` from the compiled manifest (or tracked file list), honoring per-file `[files]` symlink/copy overrides. Prefer `dotdipper apply` on machines that have the binary; use `dotdipper install script` to print or export the fallback script.

---

## 📚 Core Features

### 🔐 Secrets Management

Securely manage sensitive dotfiles with **age** or **SOPS** (age backend):

```bash
# Initialize encryption (reads [secrets].provider; default: age)
dotdipper secrets init

# Encrypt files
dotdipper secrets encrypt ~/.aws/credentials
# age  → ~/.aws/credentials.age
# sops → ~/.aws/credentials.sops (or secrets.sops.yaml for .yaml inputs)

# Edit encrypted files seamlessly
dotdipper secrets edit ~/.ssh/config.age

# Auto-decrypts during apply (in-memory only; .age and .sops.* names)
dotdipper apply
```

```toml
[secrets]
provider = "age"   # or "sops"
key_path = "~/.config/age/keys.txt"
```

For SOPS, install the `sops` CLI and reuse the same age identity file. Dotdipper passes `--age <recipient>` on encrypt and sets `SOPS_AGE_KEY_FILE` for decrypt/edit.

**Security Features:**

- Age encryption with public/private keys (native age or SOPS+age)
- In-memory decryption on apply (never writes plaintext into the git store)
- Seamless edit workflow (decrypt → edit → re-encrypt; native `sops` edit for SOPS)
- 0600 permissions on key files

### 🎯 Selective Apply & Diff

Review changes and selectively apply configurations:

```bash
# See what would change
dotdipper diff --detailed

# Interactive selection
dotdipper apply --interactive

# Apply specific files
dotdipper apply --only "~/.zshrc,~/.config/nvim"
```

**Features:**

- Pre-apply diffs with colored output
- Interactive TUI for file selection
- Path filtering (files or directories)
- Binary file detection

### 📸 Snapshot Management

Create point-in-time snapshots with efficient storage:

```bash
# Create snapshot
dotdipper snapshot create -m "Before major update"

# List snapshots
dotdipper snapshot list

# Rollback to snapshot
dotdipper snapshot rollback <id>

# Delete snapshot
dotdipper snapshot delete <id>
```

**Features:**

- Hardlink optimization for efficient storage
- ISO-8601 timestamp IDs
- Safety snapshots before rollback
- Metadata tracking (file count, size, message)

#### Auto-Pruning

Automatically prune old snapshots after creation to manage disk space:

```toml
[auto_prune]
enabled = true
keep_count = 10      # Keep 10 most recent snapshots
keep_age = "30d"     # Keep snapshots from last 30 days
keep_size = "1GB"    # Keep until total size exceeds 1GB
```

Any combination of criteria can be used. Snapshots are kept if they match ANY criterion. Auto-pruning runs automatically after each snapshot creation.

### 👤 Multiple Profiles

Manage different compiled stores for work / personal / CI contexts:

```bash
# Create profiles
dotdipper profile create work
dotdipper profile create personal

# Switch profiles (updates config + compat links)
dotdipper profile switch work

# List profiles
dotdipper profile list

# One-off override without editing config
DOTDIPPER_PROFILE=work dotdipper status

# Remove profile
dotdipper profile remove work
```

**Layout:**

```text
~/.config/dotdipper/
  config.toml                 # active_profile = "work"
  profiles/default/compiled/  # default store (git push/pull target)
  profiles/work/compiled/
  compiled -> profiles/work/compiled   # compatibility symlink
```

Legacy top-level `compiled/` is migrated into `profiles/default/` on first use. Snapshot, apply, push, pull, status, diff, install, and remote bundles all use the **active** profile store.

**Per-profile overlay:** `profiles/<name>/config.toml` is merged on top of the global config (overlay keys win). Leave it comments-only to inherit everything. `profile switch` only updates `active_profile` in the global file.

Because the overlay wins on load, a key defined in both files makes the global copy dead weight — edits there have no effect. dotdipper keeps that from happening silently:

- `config --set` writes to whichever file governs the key, and says so when that is the overlay.
- `config --edit` warns about every shadowed key before opening; `config --edit --overlay` opens the overlay instead.
- `config --show` and `doctor` both report shadowed keys.
- Discovery writes `tracked_files` and `packages.common` to the overlay and removes the stale global copy, so exactly one file owns each key.

**GitHub sync:** each profile pushes to its own branch by default (`main` for `default`, `dotdipper/<name>` otherwise). Set `[github].repo_name` in the overlay for a dedicated repository. Branch and repo are independent; set `[github].branch` to override the default.

**Features:**

- Per-profile `compiled/`, `manifest.lock`, `snapshots/`, and overlay `config.toml`
- `DOTDIPPER_PROFILE` env override
- Compatibility symlinks so older scripts still see `~/.config/dotdipper/compiled`
- Legacy store migration support

### ☁️ Cloud Backups

Push/pull dotfiles to remote storage:

```bash
# Configure LocalFS remote
dotdipper remote set localfs --endpoint ~/dotfiles-backup

# Configure S3 remote (included in official release binaries / default cargo features)
dotdipper remote set s3 --bucket my-dotfiles --region us-east-1
# Set credentials via environment:
export AWS_ACCESS_KEY_ID=your-key
export AWS_SECRET_ACCESS_KEY=your-secret
# Or use custom S3-compatible endpoint (MinIO, DigitalOcean Spaces):
export AWS_ENDPOINT_URL=https://nyc3.digitaloceanspaces.com

# Configure WebDAV remote (included in official release binaries / default cargo features)
dotdipper remote set webdav --endpoint https://cloud.example.com/remote.php/webdav
# Set credentials via environment:
export WEBDAV_USERNAME=your-username
export WEBDAV_PASSWORD=your-password

# Show configuration
dotdipper remote show

# Push to remote
dotdipper remote push

# Pull from remote
dotdipper remote pull
```

**Supported Backends:**

- ✅ LocalFS (fully implemented)
- ✅ S3 (fully implemented; shipped in default/release builds)
- ✅ WebDAV (fully implemented; shipped in default/release builds)

Official packages and `cargo build --release` include S3 + WebDAV by default. For a minimal binary: `cargo build --release --no-default-features`. If you use a minimal build and run `remote set s3` / `webdav`, Dotdipper prints a clear rebuild message (`--features s3,webdav`).

**Features:**

- Compressed bundles (tar.zst)
- Bundle metadata tracking
- Dry-run support
- Profile-aware backups
- S3-compatible storage support (MinIO, DigitalOcean Spaces)
- WebDAV servers (Nextcloud, ownCloud, etc.)

### 🔀 Git vs Remote Backends: When to Use Each

Dotdipper provides two ways to sync your dotfiles to the cloud:

| Feature | GitHub Sync (`push`/`pull`) | Remote Backends (`remote push`/`remote pull`) |
|---------|----------------------------|----------------------------------------------|
| **Storage** | Git repository (GitHub, GitLab, etc.) | S3, WebDAV, LocalFS |
| **Version Control** | Full git history | Bundle-based (latest only by default) |
| **Collaboration** | Pull requests, issues, forks | Not designed for collaboration |
| **Setup Complexity** | Requires git + GitHub token | Environment variables only |
| **File Size Limits** | GitHub's limits apply | No practical limits |
| **Privacy** | Public/private repos | Fully private (your storage) |
| **Offline Access** | Clone repo locally | Download bundle when needed |

**Use GitHub Sync when you want:**

- Version history of all changes
- Collaboration with others
- Public sharing of your dotfiles
- Integration with GitHub workflows
- Easy cloning on new machines (`git clone`)

```bash
# GitHub workflow
dotdipper push -m "Update vim config"
dotdipper undo                      # Revert the last pushed commit
dotdipper pull --apply
```

**Git repo location:** Push/pull use a git repository inside your dotdipper directory (e.g. `~/.config/dotdipper/compiled/`). The `manifest.lock` is stored inside that repo so a fresh `pull` can apply files correctly. Don’t run `git pull` or `git push` from `~/.config`; use `dotdipper pull` and `dotdipper push` from any directory. If the remote already has commits (e.g. a new repo with a README), `dotdipper push` will fetch, rebase your changes on top, and push automatically. `pull --force` discards uncommitted changes in `compiled/` only (after stashing); it does not touch `$HOME` unless you also pass `--apply`.

**Use Remote Backends when you want:**

- Simple backups without git complexity
- Private storage (your own S3/WebDAV)
- No GitHub account required
- Large files that exceed git limits
- Integration with existing cloud storage

```bash
# Remote backend workflow
dotdipper remote push
dotdipper remote pull
```

**Combined Approach:** You can use both! Use GitHub for version control and collaboration, while also pushing backups to S3/WebDAV for redundancy.

### 🤖 Auto-Sync Daemon (Opt-In)

The daemon watches your dotfiles and automatically creates snapshots when changes are detected. This is an **opt-in feature** that must be explicitly enabled in your configuration.

**When to use the daemon:**

- You want automatic backups without running `dotdipper snapshot` manually
- You make frequent changes to your dotfiles
- You want to catch every change without thinking about it

**When NOT to use the daemon:**

- You prefer manual control over when snapshots are created
- You're on a resource-constrained system
- You only occasionally modify dotfiles

**Setup:**

```bash
# 1. Enable the daemon (creates config if needed)
dotdipper daemon enable

# 2. Start the daemon
dotdipper daemon start

# 3. Check status
dotdipper daemon status

# 4. Stop the daemon
dotdipper daemon stop

# 5. Disable the daemon
dotdipper daemon disable
```

Or manually configure in `config.toml`:

```toml
[daemon]
enabled = true
mode = "ask"      # "ask" = prompt before snapshot, "auto" = auto-snapshot
debounce_ms = 1500  # Wait time after changes before processing
```

**Features:**

- File watching with configurable debouncing
- Two modes: "auto" (automatic snapshots) or "ask" (prompt before snapshot)
- PID file management for single-instance enforcement
- Graceful start/stop with cleanup
- CLI commands to enable/disable without editing config

### 🪝 Hooks System

Automate workflows with custom hooks:

```toml
[hooks]
pre_apply = ["echo 'Starting...'"]
post_apply = [
    "tmux source-file ~/.tmux.conf || true",
    "source ~/.zshrc"
]
post_snapshot = ["git add -A && git commit -m 'Snapshot' || true"]
```

**Use Cases:**

- Reload services after apply
- Auto-commit snapshots
- Validate configs before apply
- Custom backup strategies

---

### 🪞 Public Mirror

Your private backup has to contain the things a restore actually needs — `.ssh/config`, `.gitconfig`, the Brewfile, the app manifest. Those are the same things you cannot make public. `dotdipper publish` derives a **separate, sanitized repository** from the same store, so you keep one complete private backup and one safe public copy.

Visibility on GitHub is per-repository, so the public copy needs its own repo:

```bash
dotdipper config --set github.public_repo_name=dotfiles-public
```

**Review first, publish second.** Nothing is published until you have seen what would be:

```bash
dotdipper publish --review     # writes public-allowlist.toml — read it
dotdipper publish --dry-run    # show the plan
dotdipper publish              # push the sanitized mirror
```

`--review` writes an allowlist recording every approved path and what was scrubbed from it:

```toml
[[files]]
path = ".gitconfig"
redactions = ["gitconfig-identity x4"]

[[withheld]]
path = ".ssh/config"
redactions = ["public.exclude '.ssh/**'"]
```

Commit that file. It is a diffable record of exactly what is public, and if an entry ever loses its redactions in a diff, a rule stopped matching.

**Three layers, each covering the previous one's gap:**

| Layer | Gates | Why it exists |
|-------|-------|---------------|
| Allowlist | which **paths** publish | A dotfile captured after your last review is withheld and reported as pending, instead of publishing itself |
| Redactors | file **content** | Strips git identity (including commented-out lines), hostnames, SSH endpoints, Tailscale names, home paths (rewritten to `$HOME`), emails, and the literal username / device name read from your environment |
| Scanner | the **finished tree** | Runs last and aborts the publish on any secret or PII it still finds. Nothing is written or pushed unless it comes back clean |

The third layer is the important one. Redaction rules are a denylist and denylists have gaps; the scan turns a gap into a failed command rather than a leak. Binary files are withheld outright, since they can be neither redacted nor meaningfully scanned.

Each finding gets a stable id. After reviewing one you can accept it explicitly:

```bash
dotdipper publish --allow-finding a1b2c3d4
```

Add project-specific rules with `[[public.redact]]` — see `example-config.toml`.

**The app inventory publishes as a script, not as itself.** The point of the mirror is that a new machine can install the same tools. The captured `Brewfile` and `apps_manifest.toml` do that, but they also record the machine's name, when the capture ran, the exact version of everything installed, and the applications that were installed by hand — none of which helps install anything. A version list in particular is a vulnerability list, and a hand-installed app list describes its owner rather than their toolchain.

So `publish` withholds both files and ships a generated `install-apps.sh` instead:

```bash
./install-apps.sh          # taps, formulae, casks, App Store titles — idempotent
```

Packages under your own Homebrew tap are dropped from the public copy and the omission is stated in the script, since a tap names its owner. Your **private** repo still has the real `Brewfile` and manifest, so your own new machine loses nothing. Generate the same script from the private store at any time:

```bash
dotdipper install apps-script              # keeps your personal tap
dotdipper install apps-script --shareable  # drops it, as the mirror does
```

Turn the whole behaviour off with `[public] apps_script = false`, which publishes the two files as they are.

### 🖥️ macOS Preferences

`dotdipper macos capture` records your system preferences as a replayable script at `~/.config/macos/defaults.sh`:

```bash
dotdipper macos capture   # regenerate from the live system
dotdipper macos keys      # show what is eligible for capture
```

It covers the Dock and its hot corners, Finder, keyboard repeat and text substitution, the trackpad, Stage Manager, spaces, and accessibility. A `post_apply` hook runs it, so a new machine receives the settings rather than a file it ignores.

**It never copies a preference plist.** Finder's holds `FXRecentFolders` — the names of directories you have been working in — and `NewWindowTargetPath`, an absolute home path; the Dock's holds the pinned app lineup as file URLs. Instead an explicit allowlist of individual keys is read, scalar values only, and any value that looks like a path or URL is refused even when its key is allowlisted. The pinned Dock lineup is deliberately left out for the same reason.

---

## ⚙️ Configuration

Configuration is stored in `~/.config/dotdipper/config.toml` (or `$XDG_CONFIG_HOME/dotdipper/config.toml`). You can override the base directory by setting the `DOTDIPPER_HOME` environment variable.

```toml
[general]
default_mode = "symlink"  # or "copy"
backup = true
active_profile = "default"
tracked_files = [
    "~/.zshrc",
    "~/.config/nvim",
    "~/.tmux.conf"
]

[github]
username = "psyysp"
repo_name = "dotfiles"
# branch = "main"  # optional; default is "main" for the default profile, else "dotdipper/<name>"
private = true

[secrets]
provider = "age"  # or "sops" (requires sops CLI; uses age keys)
key_path = "~/.config/age/keys.txt"

[hooks]
post_apply = ["tmux source-file ~/.tmux.conf || true"]

# Daemon is opt-in - uncomment to enable
# [daemon]
# enabled = true
# mode = "ask"  # or "auto"
# debounce_ms = 1500

[remote]
kind = "localfs"
endpoint = "~/dotfiles-backup"

# Per-file overrides
[files."~/.config/nvim"]
mode = "copy"

[files."~/.ssh/config"]
exclude = true

# Discovery patterns
include_patterns = ["~/.config/**", "~/.zshrc"]
exclude_patterns = ["~/.ssh/**", "**/*.key"]

[packages]
common = ["git", "vim", "tmux"]
macos = ["neovim", "fzf", "bat"]
linux = ["neovim", "fzf", "bat"]
```

---

## 📖 Command Reference

### Core Commands

```bash
dotdipper init                    # Initialize dotdipper
dotdipper discover [--write]      # Find dotfiles
dotdipper discover --packages     # Discover required packages from dotfiles
dotdipper snapshot create [-m "msg"]  # Create snapshot
dotdipper status                  # List changed file paths
dotdipper publish --review        # Generate the public allowlist, then read it
dotdipper publish [--dry-run]     # Push the sanitized public mirror
dotdipper config --show | --edit  # View/edit config
dotdipper doctor [--fix]          # Health check
```

### Secrets Commands

```bash
dotdipper secrets init                # Setup encryption
dotdipper secrets encrypt <file>      # Encrypt file
dotdipper secrets decrypt <file>      # Decrypt file
dotdipper secrets edit <file>         # Edit encrypted file
```

### Diff & Apply

```bash
dotdipper diff [--detailed]                    # Show changes
dotdipper apply [--interactive]                # Apply changes
dotdipper apply --only "~/.zshrc"              # Apply specific files
dotdipper apply --force                        # No confirmations
```

### Snapshot Management

```bash
dotdipper snapshot create [-m "msg"]  # Create snapshot
dotdipper snapshot list               # List snapshots
dotdipper snapshot rollback <id>      # Rollback
dotdipper snapshot delete <id>        # Delete snapshot
dotdipper snapshot prune              # Prune old snapshots
```

**Pruning options:**

```bash
# Keep only the 10 most recent snapshots
dotdipper snapshot prune --keep-count 10

# Keep snapshots from the last 30 days
dotdipper snapshot prune --keep-age 30d

# Keep snapshots until total size exceeds 1GB
dotdipper snapshot prune --keep-size 1GB

# Combine criteria (keep if ANY criterion is met)
dotdipper snapshot prune --keep-count 5 --keep-age 7d

# Dry run - see what would be deleted without deleting
dotdipper snapshot prune --keep-count 5 --dry-run
```

### Profile Management

```bash
dotdipper profile list              # List profiles
dotdipper profile create <name>     # Create profile
dotdipper profile switch <name>     # Switch profile
dotdipper profile remove <name>     # Remove profile
```

### Remote Backups

```bash
dotdipper remote set <kind>         # Configure remote
dotdipper remote show               # Show config
dotdipper remote push               # Push to remote
dotdipper remote pull               # Pull from remote
```

### Daemon

```bash
dotdipper daemon enable             # Enable daemon in config
dotdipper daemon disable            # Disable daemon in config
dotdipper daemon start              # Start daemon
dotdipper daemon status             # Check status
dotdipper daemon stop               # Stop daemon
```

### GitHub Sync

```bash
dotdipper push [-m "msg"]           # Push to GitHub
dotdipper pull [--apply]            # Pull from GitHub
dotdipper undo [--force]            # Revert the last pushed commit
```

### Package Management

```bash
# Discover packages from your dotfiles
dotdipper discover --packages                     # Auto-detect required packages
dotdipper discover --packages --validate          # Check which are already installed
dotdipper discover --packages --write             # Add discovered packages to config
dotdipper discover --packages --include-low-confidence  # Include uncertain matches

# Install packages
dotdipper install [--dry-run]       # Install packages
dotdipper install --target-os ubuntu  # Target specific OS
dotdipper install script              # Print setup_dotfiles.sh (manifest / tracked files)
dotdipper install script -o setup.sh  # Export to a file (creates parent dirs; marks executable)
```

---

## 🎓 Common Workflows

### Daily Workflow

```bash
# Make changes
vim ~/.zshrc

# Create snapshot
dotdipper snapshot create -m "Updated aliases"

# Push to GitHub
dotdipper push -m "Update zsh config"

# If the last push was a mistake
dotdipper undo
```

### Managing Secrets

```bash
# Encrypt credential
dotdipper secrets encrypt ~/.aws/credentials

# Track encrypted version
# Add ~/.aws/credentials.age to tracked_files in config

# Snapshot and push
dotdipper snapshot create -m "Add AWS creds"
dotdipper push

# On new machine
dotdipper pull
dotdipper apply  # Auto-decrypts
```

### Selective Updates

```bash
# Pull latest
dotdipper pull

# Review changes
dotdipper diff --detailed

# Apply specific files
dotdipper apply --only "~/.zshrc,~/.bashrc"

# Or use interactive mode
dotdipper apply --interactive
```

### Multi-Profile Setup

```bash
# Create work profile
dotdipper profile create work

# Optional: dedicated repo and/or branch in the profile overlay
# ~/.config/dotdipper/profiles/work/config.toml
# [github]
# repo_name = "dotfiles-work"
# branch = "main"

# Switch to work
dotdipper profile switch work

# Work-specific snapshot
dotdipper snapshot create -m "Work dotfiles"

# Push uses branch dotdipper/work (or overlay github.branch / github.repo_name)
dotdipper push -m "Work dotfiles"

# Switch back to personal
dotdipper profile switch default
```

---

## 🏗️ Feature Status

### ✅ Fully Implemented

- **Milestone 1:** Secrets Encryption (age encryption)
- **Milestone 2:** Selective Apply & Diff (interactive TUI)
- **Milestone 3:** Snapshot Management (hardlink optimization)
- **Milestone 4:** Multi-Profile Support (overlay semantics)
- **Milestone 5:** Remote Backends (LocalFS, S3, WebDAV fully implemented)
- **Milestone 6:** Auto-Sync Daemon (file watching, debouncing)
- **Core Features:** Init, discover, status, push, pull
- **Hooks System:** Pre/post hooks for operations
- **Package Management:** OS-specific installation
- **GitHub Sync:** Full push/pull support

---

## 🛡️ Safety Features

Dotdipper is designed with safety as a core principle:

- **HOME Boundary Enforcement** - Refuses operations outside `$HOME`
- **Backup Creation** - Creates `.bak.<timestamp>` backups (warns if disabled)
- **Confirmation Prompts** - Interactive confirmations unless `--force`
- **Hash-Based Detection** - BLAKE3 hashing
- **Deterministic Behavior** - Sorted manifests synced with the git store
- **Safe Rollback** - Pre-rollback safety snapshot; preserves `compiled/.git`
- **Safe Install** - Package install + Rust apply (respects excludes/manifest)
- **No Plaintext Secrets** - In-memory decryption only

---

## 🔍 Troubleshooting

### Age not found

```bash
brew install age  # macOS
sudo apt install age  # Ubuntu
```

### Permission denied

```bash
chmod 600 ~/.config/age/keys.txt
dotdipper apply --force
```

### Diff fails

```bash
which git  # Ensure git is installed
dotdipper diff  # Without --detailed
```

### Hook fails

```bash
# Test manually
sh -c "your-hook-command"

# Make non-fatal
post_apply = ["command || true"]
```

---

## 📊 Platform Support

- **macOS:** Full support ✅
- **Linux:** Full support (Ubuntu, Arch, Fedora) ✅
- **Windows:** Not currently supported (potential future milestone)

---

## 🧪 Testing

```bash
# Run all tests
cargo test

# Run specific test suite
cargo test --test secrets_tests
cargo test --test snapshots_tests
cargo test --test profiles_tests

# Build and run
cargo build --release
./target/release/dotdipper --help
```

### Container E2E tests

Linux pull-on-a-fresh-machine coverage, run via Docker (`rust:1-bookworm`, linux/arm64):

```bash
./scripts/container-test.sh
```

The suite builds a debug binary inside the container (host `target/` is not used), then exercises `init` → `discover --write` → `push` on machine A against a local bare git remote, and `init` → `pull --apply --force` on a completely fresh machine B. It asserts byte-identical fixture files (including nested `nvim/lua` paths), a clean `status` on B, a round-trip update, that `apps` is absent on Linux, and that generated install scripts have valid bash syntax with no Brewfile/Homebrew/mas logic. Named volumes `dotdipper-e2e-build` and `dotdipper-e2e-cargo` cache the cargo target dir and registry so reruns are fast. Requires a running Docker daemon.

---

## 🤝 Contributing

Contributions are welcome! Please:

1. Fork the repository
2. Create a feature branch
3. Write tests for new features
4. Run `./scripts/check.sh` — formatting, clippy (with and without default features) and the tests, in the order CI runs them
5. Submit a pull request

---

## 📄 License

MIT License - See LICENSE file for details

---

## 🙏 Acknowledgments

- **Age:** Modern encryption by Filippo Valsorda
- **Rust Community:** Amazing crates ecosystem

---

## 📞 Support

- **Documentation:** This README and `dotdipper --help`
- **Issues:** Report bugs via GitHub Issues
- **Help:** Run `dotdipper <command> --help` for any command

---

**Version:** 0.7.3  
**Status:** Production-ready  
**Last Updated:** March 14, 2026  
**Installation:** `brew install psyysp/dotdipper/dotdipper`

**Happy dotfile management! 🚀**
