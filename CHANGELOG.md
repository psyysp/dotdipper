# Changelog

All notable changes to dotdipper are documented here.

## [Unreleased]

### Fixed

- Snapshot records the compiled file hash after copy, so skipping an empty home file over a non-empty store copy cannot poison `manifest.lock`.
- `pull` refuses to check out remote 0-byte blobs over non-empty compiled files (symlink restore would empty `$HOME` immediately). Help text no longer claims `$HOME` is untouched until `--apply`.

## [0.7.5] - 2026-08-26

### Added

- **App capture promotion:** `apps capture` skips stock Apple and helper apps, treats already-installed Homebrew casks as managed (including iTerm/iterm2, zoom.us/zoom, and version suffixes), and promotes remaining apps that map to casks or known MAS ids into the Brewfile. Restore stays on Homebrew and the Mac App Store. True leftovers stay `[[unmanaged]]` with an optional `homepage`. Toggle with `[apps] promote_unmanaged` (default true).
- **macOS app capture & restore (`dotdipper apps`, macOS builds only):** `apps capture` dumps Homebrew state via `brew bundle dump` into a `Brewfile`, records Mac App Store apps (`mas list`) and scans `/Applications` + `~/Applications` into `apps_manifest.toml` (unmanaged apps flagged for manual install). Both files live in the compiled store and sync with `push`/`pull`. `apps install [--dry-run]` restores via `brew bundle`; capture runs automatically on `push` (`[apps] capture_on_push`, default true). Linux builds do not include the command.
- **Bootstrap overhaul for macOS:** generated `install_macos.sh` now installs Xcode Command Line Tools and Homebrew when missing, restores packages with `brew bundle` from the synced Brewfile (with a `mas` guard for App Store apps), and prints unmanaged apps to install manually. Legacy package-list install remains the fallback when no Brewfile exists.
- **Container e2e suite:** `scripts/container-test.sh` runs a real fresh-machine test in a Linux (Docker) container — build, push to a bare repo, pull `--apply` on a clean HOME, byte-identical restore, status clean, round-trip update, and Linux script generation.

### Fixed

- **Critical — snapshot/push no longer empties symlink-restored dotfiles.** After `apply` in symlink mode, home paths like `~/.zshrc` point at `compiled/.zshrc`. Snapshot then copied that path onto itself; `fs::copy` of a file onto the same inode truncates it to 0 bytes. Copy now skips when source and dest are the same file, and refuses to replace a non-empty file with an empty source.
- `dotdipper doctor` resolves home-relative manifest paths against `$HOME` (it previously treated `.zshrc` as relative to the current directory and failed every file).
- Generated `install.sh` no longer crashes on an unbound `$target_os` shell variable.
- `discover`/`snapshot` skip non-regular files (sockets, fifos) instead of failing.
- `push` retargets the `origin` remote when `github.repo_name` changes, and reports an error instead of claiming success when the GitHub repo/remote could not be prepared.
- Release tarballs removed from the repository (`release-v*/` now gitignored); test suite is tracked in git again.

### Previously unreleased (0.7.4 branch work, first shipped in this release)

### Added

- **SOPS secrets provider:** encrypt / decrypt / edit via the `sops` CLI with an age backend; apply decrypts `.sops.*` (and common `.enc.*`) names in-memory like `.age`.
- **Profile selection:** active profile drives `compiled/`, `manifest.lock`, and `snapshots/` under `profiles/<name>/`; `DOTDIPPER_PROFILE` overrides config; legacy top-level stores migrate into `profiles/default/`; compatibility symlinks keep `~/.config/dotdipper/compiled` working.
- **Per-profile config overlay:** `profiles/<name>/config.toml` is merged on top of the global config (overlay keys win). New profiles get a comments-only overlay so they inherit the global file.
- **Per-profile GitHub target:** push/pull/undo/clone use branch `main` for `default` and `dotdipper/<name>` otherwise. Overlay or global `[github].repo_name` can select a dedicated repository; `[github].branch` overrides the default. Branch and repo are independent.
- **`[secrets].recipients`:** multi-machine SOPS age recipients; encrypt also honors `SOPS_AGE_RECIPIENTS` / `.sops.yaml` without forcing a single local `--age`.
- **`dotdipper install script`:** print or export (`--out PATH`) `setup_dotfiles.sh` without running the full install. The generated script uses the compiled manifest (falling back to `tracked_files`, then a runtime `find`) and honors per-file `[files]` symlink/copy / exclude / `local_only` overrides.

### Changed

- `dotdipper init` scaffolds `profiles/default` and sets `active_profile = "default"`.
- `doctor` checks for `sops` when `[secrets].provider = "sops"`.
- Discover always skips the dotdipper base dir; default ignore covers all of `profiles/**`.
- Pull→apply no longer puts encrypted store names into `tracked_files`; snapshot preserves encrypted compiled blobs so consumer machines can push.
- Profile names are validated (blocks path traversal); non-default profiles are not auto-created from env typos.
- Remote push honors `DOTDIPPER_PROFILE`; remote pull uses timestamped backups and clearer profile-switch hints.
- Remote bundles omit `.git` / `.gitignore` and honor `push_ignore` / `local_only`.
- Config writes are atomic (temp file + rename). Discover writes `tracked_files` / packages to the active profile overlay.
- `dotdipper config --set` / `--edit` and `profile switch` write the global config only (overlays are not flattened).
- Dependency bumps: `tar` 0.4.46, `rust-s3` 0.37, `rustls-webpki` 0.103.14, `anyhow` 1.0.104 (cargo-audit).

### Fixed

- Apply/diff tests no longer share the runner `XDG_CONFIG_HOME` store (CI false-success / false-failure).

## [0.7.4] - 2026-07-29

### Fixed

- **Critical — pull → apply on a new machine:** `manifest.lock` is now written into `compiled/` (the git store) on every snapshot/push, and restored after `pull`.
- **Install** uses Rust `apply` (excludes/backups/manifest) instead of blindly linking every file under `compiled/`.
- **Snapshot rollback:** safety snapshot first; preserves `compiled/.git`; unique snapshot IDs with milliseconds.
- **Pull `--force`:** stashes then hard-resets the compiled store only (`$HOME` untouched unless `--apply`).
- **Prune:** `keep_age` OR-semantics fixed; `keep_size`-only no longer deletes everything.
- **Apply path traversal** rejected; encrypted apply always copies (never symlinks deleted temps).
- **Tracked file hashing** fails loudly; config expands `~`.
- **`apply` / `diff` / `pull --apply` / `install` apply** return non-zero when the manifest is missing (scripts can detect failure).
- **doctor --fix** no longer pretends to auto-repair.

### Changed

- **Default features** now include `s3` and `webdav` so release/Homebrew/AUR/Nix binaries ship remotes. Minimal builds: `--no-default-features`.
- Clear errors when S3/WebDAV are unavailable, or when `github`/`gcs` remotes are requested.
- Packaging metadata bumped to **0.7.4** (AUR, Nix, Scoop, root flake). Source/binary checksums for published artifacts still need updating when the GitHub release is cut.

### Tests

- Full e2e coverage in `tests/e2e_full_sync_test.rs` + `tests/safety_sync_test.rs`.

## [0.7.3] - 2026-03-14

### Fixed

- **CI:** Formatted `src/vcs/mod.rs` so the formatting check passes in GitHub Actions.
- **Release workflow:** The Homebrew tap update job now only runs when `HOMEBREW_TAP_TOKEN` is configured, so releases no longer fail just because that secret is missing.

## [0.7.2] - 2026-03-14

### Changed

- **Git push:** When push is rejected because the remote has commits you don't have (e.g. repo created with a README), `dotdipper push` now automatically fetches, rebases your changes onto `origin/main`, and retries the push. No need to run `dotdipper pull` first in this case.
- **Docs:** README now explains that the git repo used for push/pull lives under `~/.config/dotdipper/compiled/` and that you should use `dotdipper pull` / `dotdipper push` rather than raw `git` from `~/.config`.

## [0.7.1] - (previous release)

See GitHub releases for earlier history.
