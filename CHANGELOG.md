# Changelog

All notable changes to dotdipper are documented here.

## [Unreleased]

### Added

- **SOPS secrets provider:** encrypt / decrypt / edit via the `sops` CLI with an age backend; apply decrypts `.sops.*` (and common `.enc.*`) names in-memory like `.age`.
- **Profile selection:** active profile drives `compiled/`, `manifest.lock`, and `snapshots/` under `profiles/<name>/`; `DOTDIPPER_PROFILE` overrides config; legacy top-level stores migrate into `profiles/default/`; compatibility symlinks keep `~/.config/dotdipper/compiled` working.
- **`[secrets].recipients`:** multi-machine SOPS age recipients; encrypt also honors `SOPS_AGE_RECIPIENTS` / `.sops.yaml` without forcing a single local `--age`.

### Changed

- `dotdipper init` scaffolds `profiles/default` and sets `active_profile = "default"`.
- `doctor` checks for `sops` when `[secrets].provider = "sops"`.
- Discover always skips the dotdipper base dir; default ignore covers all of `profiles/**`.
- Pull→apply no longer puts encrypted store names into `tracked_files`; snapshot preserves encrypted compiled blobs so consumer machines can push.
- Profile names are validated (blocks path traversal); non-default profiles are not auto-created from env typos.
- Remote push honors `DOTDIPPER_PROFILE`; remote pull uses timestamped backups and clearer profile-switch hints.
- Remote bundles omit `.git` / `.gitignore` and honor `push_ignore` / `local_only`.
- GitHub push warns when the active profile is not `default` (shared `main` branch).
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
