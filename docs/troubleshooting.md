# Troubleshooting guide

Start by reproducing with a release binary when the issue involves latency,
window composition, icons, or screenshots. Check the platform notes in
[`platforms.md`](platforms.md) before treating an unsupported capability as a
bug.

## The palette does not appear

- Confirm no stale process is holding the single-instance slot. On Linux/macOS,
  use `pkill -x commandeer`, then launch the release binary; do not use `pkill -f`.
- On Linux/Wayland, verify the COSMIC/GNOME managed shortcut and remember that
  re-launching the binary is the reliable toggle path.
- On macOS, check that the app is running as an Accessory and use the tray or
  `Cmd+Shift+Space`; Spotlight owns `Cmd+Space`.
- Check `config.json` hotkey values and shortcut registration errors in stderr.

## Screenshot is blank, stale, or does not copy

- Use a release build. Dev WebView rendering is not representative.
- Windows depends on the cloak/reveal handshake and disabled native occlusion;
  keep the two window browser args identical.
- Linux depends on clearing the transparent overlay before hiding; do not add a
  direct hide/show that skips the clear paint.
- macOS needs Screen Recording permission. Linux needs an available capture CLI
  and clipboard backend; inspect the fallback error.
- If saving succeeds but copying fails, check the saved path under
  `Pictures/Screenshots` and the platform clipboard tool/permission.

## Clipboard history is empty or old entries are missing

- The monitor records text only, skips empty values, deduplicates the latest
  entry, and retains at most 100 rows.
- Verify the app-data directory contains `clipboard.db`; do not delete the key
  file or Linux Secret Service item while diagnosing encryption.
- If Linux/macOS rows are undecryptable, the key source changed or was lost;
  the backend skips those rows rather than exposing plaintext.
- Paste-to-previous requires Accessibility on macOS and a captured foreground
  window before the palette was shown.

## Global search misses a file

- `@find` indexes the configured roots, defaults to Desktop/Documents/Downloads,
  prunes hidden, dependency, VCS, cache, and generated-output directories, caps
  depth, and may still be scanning.
- Configure roots in Settings → File Search Roots using one absolute directory
  per line. Restart after saving or resetting them; the background manager reads
  its root set once at startup.
- The index is a cache; if its schema is stale, stop the app and remove only
  `<app-data>/file_index.db` so it can rebuild. Preserve user data and config.
- Windows search falls back to Everything and then walkdir; Linux/macOS use the
  self-hosted index and walk paths rather than Everything.

## Scripts do not appear or run

- Check Settings → Scripts Directory and the platform extension/executable-bit
  rules in [`scripts.md`](scripts.md).
- Metadata is read only from the first 8192 bytes and must use an accepted
  comment marker. A malformed JSON argument or keyword array is ignored.
- Confirm the file is not a directory child that was expected to be root-level,
  and check whether a sibling PNG/name collision changed its display.
- Run errors are surfaced by the backend; do not “fix” a script problem by
  suppressing the error toast.

## Icons are wrong or slow

- Icons are resolved lazily for visible rows. Wait for the background request;
  do not turn the whole list into eager icon IPC.
- macOS bundles must be keyed by full path, not only extension or directory.
- If a cache is corrupt, remove only `<app-cache>/icon-cache-v1.json`; it is
  regenerated. Verify the current process path is not being mistaken for the
  app icon.

## Build or commit failures

- Run `bun install` if TypeScript packages or Husky are missing.
- Run `npm run format:check` before pushing. Pre-commit formats staged files but
  does not run typecheck, tests, or clippy.
- If the agent sync hook fails, copy the canonical skill from
  `.agents/skills/ship-change/SKILL.md` to the Claude mirror and preserve the
  `CLAUDE.md` redirect.
- If a push is rejected because `main` moved, rebase local work on the remote
  branch; never overwrite remote history.

## Keeping this document current

Add a troubleshooting entry when a recurring failure has a non-obvious cause,
platform-specific recovery, data-safety warning, or verification command. Keep
the remedy aligned with the owning module and update the feature/platform page
when the underlying behavior changes; remove advice that no longer applies.
