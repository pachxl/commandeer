# Storage, caches, and migrations

Commandeer uses two persistence layers:

- Rust-owned files under the Tauri app-data/app-cache directories for user data,
  configuration, databases, encryption keys, and native caches.
- Webview `localStorage` for small UI preferences, ranking data, and stale-safe
  read caches.

Use the `data_dir` command or Settings’ data-directory action to locate the
current app-data directory. Never embed a platform path in a feature.

## Rust-owned data

| Location                              | Owner                               | Contents / migration note                                                                    |
| ------------------------------------- | ----------------------------------- | -------------------------------------------------------------------------------------------- |
| `<app-data>/config.json`              | `commands/config.rs`                | `AppConfig`; missing/invalid config falls back to defaults                                   |
| `<app-data>/quicklinks.json`          | `commands/store.rs`                 | Quick Links; first read seeds stable example ids                                             |
| `<app-data>/notes.json`               | `commands/store.rs`                 | User notes                                                                                   |
| `<app-data>/overrides.json`           | `commands/store.rs`, `shortcuts.rs` | Alias, pinned, root visibility, per-command hotkeys                                          |
| `<app-data>/themes/*.json`            | `commands/store.rs`                 | User themes with CSS variable maps                                                           |
| `<app-data>/clipboard.db`             | `commands/clipboard/db.rs`          | Up to 100 clipboard entries; text blobs are encrypted                                        |
| `<app-data>/file_index.db`            | `commands/file_index.rs`            | Regenerable SQLite/FTS5 global file index                                                    |
| `<app-data>/.identifier-migrated-v1`  | `commands/config.rs`                | Marker for copy-only migration from `dev.commandeer.app`                                     |
| `<app-data>/clipboard.key` where used | `clipboard/crypto.rs`               | 0600 ChaCha key fallback/primary on macOS; Linux fallback when Secret Service is unavailable |
| Secret Service item where available   | `clipboard/crypto.rs`               | Linux ChaCha key; do not silently change lookup identity                                     |
| `<app-cache>/frame.png`               | `commands/screenshot.rs`            | Transient frozen screenshot frame; remove after finish/cancel                                |
| `<app-cache>/icon-cache-v1.json`      | `commands/icons.rs`                 | Persisted app icon cache keyed by path and mtime where needed                                |

The file index is a cache and can be rebuilt. Clipboard keys are not caches:
losing a key makes encrypted Linux/macOS rows unreadable. Preserve migration
and key behavior when changing app identifiers or storage formats.

## Webview localStorage keys

| Key family                                  | Owner                       | Purpose                                           |
| ------------------------------------------- | --------------------------- | ------------------------------------------------- |
| `commandeer:gamemode`                       | `App.tsx`                   | Game Mode toggle                                  |
| `commandeer:*-visible`                      | `App.tsx`                   | Usage panels, system stats, web-search visibility |
| `commandeer:scripts`, `commandeer:apps`     | `App.tsx`, `appLauncher.ts` | Cold-start command caches                         |
| `commandeer:frecency`                       | `lib/frecency.ts`           | Stable command ranking history                    |
| `commandeer:confirm-suppressed`             | `lib/confirm.ts`            | “Don’t ask again” confirmation keys               |
| `commandeer:last`                           | `Palette.tsx`               | Last selected command                             |
| `commandeer:onboarding-version`             | `lib/onboarding.ts`         | Completed first-run guide version                 |
| `commandeer:codex-*`, `commandeer:claude-*` | usage components            | Last usage result and polling/backoff state       |

Local caches must tolerate invalid JSON, quota/private-mode failures, and stale
command ids. Debounce frequent writes and cap unbounded collections.

## Migration rules

- Config fields must be optional/defaulted so older files load.
- App identifier migration is copy-only and non-overwriting; the old directory
  remains for rollback.
- Legacy seeded scripts are removed only when the marker and exact pristine
  fingerprints match. Never delete edited or unrelated user scripts.
- Legacy plaintext clipboard JSON and rows migrate idempotently to the encrypted
  SQLite format.
- Regenerable caches may be invalidated with a versioned filename; user data and
  encryption keys require an explicit migration plan.

## Keeping this document current

Update this page whenever a file, database, localStorage key, cache, encryption
key, migration marker, or retention limit changes. Verify paths against the
Rust module that writes them and search for the localStorage key before editing;
update [`configuration.md`](configuration.md) for fields and
[`features.md`](features.md) for behavior that depends on the stored data.
