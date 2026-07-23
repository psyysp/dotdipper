# Changelog

All notable changes to dotdipper are documented here.

## [Unreleased]

### Fixed

- **Critical — pull → apply on a new machine:** `manifest.lock` is now written into `compiled/` (the git store) on every snapshot/push, and restored after `pull`. Previously the manifest lived only outside the repo, so `pull --apply` / `install` could find your files in git but fail to place them.
- **Install no longer blindly links every file under `compiled/`:** `dotdipper install` runs the OS package script, then uses the Rust `apply` path (respects excludes, backups, and the manifest). The generated `setup_dotfiles.sh` is manifest-aware and skips `.git` / metadata.
- **Snapshot rollback:** creates a safety snapshot first, preserves `compiled/.git` history, skips copying `.git` objects into snapshots, and re-syncs `manifest.lock`.
- **Pull `--force`:** actually means discard uncommitted changes in the local compiled git store (stash first, then hard-reset). Live `$HOME` files are untouched unless you also pass `--apply`.
- **Snapshot prune:** `keep_age` alone now correctly selects old snapshots (OR semantics with `keep_count`).
- **Snapshot IDs:** include milliseconds (plus a uniqueness suffix) so rapid successive snapshots cannot overwrite each other.

### Safety

- Warn (and confirm) before apply when `general.backup = false`.
- After pull, refresh `tracked_files` from the manifest so package discovery / install work on fresh machines.
- Prefer `dotdipper apply` from generated `install.sh` when the binary is on `PATH`.

### Tests

- Added full end-to-end coverage in `tests/e2e_full_sync_test.rs` (push/pull/apply/install round-trip, legacy manifest rebuild, dirty/force pull, excludes, symlink mode, rollback+git preservation, undo, backups).
- `DOTDIPPER_TEST_REMOTE` lets tests exercise real `push`/`pull` against a local bare git repo without GitHub SSH/`gh`.

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
