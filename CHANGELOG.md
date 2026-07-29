# Changelog

All notable changes to dotdipper are documented here.

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
