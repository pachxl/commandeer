# Testing and verification

Commandeer has pure frontend tests, Rust unit tests, static checks, and
platform-specific manual verification. Passing one category does not prove
another: in particular, a Linux build does not prove Windows hook behavior or
macOS permissions.

## Local checks

Run from the repository root unless noted:

| Check                     | Command                                                     | What it proves                                    |
| ------------------------- | ----------------------------------------------------------- | ------------------------------------------------- |
| Install/preparation       | `bun install`                                               | Dependencies and Husky hook are present           |
| Frontend typecheck/build  | `npm run build`                                             | Strict `tsc` plus Vite production bundle          |
| Frontend regression suite | `npm test`                                                  | Vitest tests under `src/`                         |
| ESLint                    | `npm run lint`                                              | Configured React hooks/lint rules                 |
| Frontend formatting       | `npm run format:check`                                      | Prettier-clean frontend/docs/config files         |
| Rust unit tests           | `cargo test` in `src-tauri/`                                | Current OS Rust tests and pure native logic       |
| Rust formatting           | `cargo fmt --manifest-path src-tauri/Cargo.toml --check`    | rustfmt-clean Rust                                |
| Rust lint                 | `cargo clippy --all-targets -- -D warnings` in `src-tauri/` | Current OS warning-free clippy                    |
| Agent sync                | `node .agents/hooks/check-agent-sync.mjs`                   | Canonical/mirrored agent wiring                   |
| Release binary            | `npm run tauri build -- --no-bundle`                        | Tauri release build, not a Vite-only dev artifact |

The Tauri package script injects the exact numeric Git tag as the local build
version (or `RELEASE_VERSION` in CI). This keeps a locally built release from
mistaking the current published tag for an update.

The repository’s `npm run format:check` also checks Rust formatting through its
package script. Use `npm run format` only when formatting changes are intended.

## Frontend tests

Current regression coverage includes Palette reducer transitions, fuzzy
ranking, focus-aware polling, feedback cleanup, onboarding and guide behavior,
destructive confirmations, configured-path parsing and Settings persistence,
multiline forms, and Windows Volume Mixer rendering/navigation. Put pure
ranking, parsing, reducer, geometry, and serialization tests beside the
implementation. Mock Tauri calls at the wrapper boundary rather than importing
native modules into jsdom tests.

## Rust tests

Rust tests cover command parsing and pure platform calculations such as shortcut
parsing, file-index schema/search behavior, clipboard crypto round trips,
screenshot stroke clipping, process snapshots, and window-drag geometry where
the target OS exposes those modules. Keep OS-gated tests explicit so a green
Linux run does not imply Windows/macOS coverage.

## Manual platform checks

### All platforms

- Open/dismiss the palette repeatedly; verify pending confirms and toasts do not
  survive dismissal.
- With empty localStorage, verify the welcome guide opens on first focus only;
  with an existing-install key, verify it stays searchable but does not auto-open.
- Verify loading, empty, and error panels are legible in both UI styles.
- Navigate list, grid, input, form, slider, and folder steps with keyboard and
  pointer; ensure Enter activates the highlighted row.
- Change the Scripts Directory and verify commands reload immediately. Save and
  reset File Search Roots, restart, then verify `@find` uses only the expected
  roots.
- Run a script, a confirmation-gated script, `@find`, `@search`, Calculator,
  Clipboard History, Notes, Quick Links, and a system action.
- Trigger a screenshot from the command and cancel/finish/retrigger it.
- Try assigning an OS-owned or duplicate shortcut. Verify the error reaches the
  palette, the previous binding still fires, and the rejected value is not
  written to `config.json` or `overrides.json`.

### Windows

- Test global palette/screenshot shortcuts and per-command shortcut dispatch.
- Test screenshot overlay across all monitors for stale/black-frame flashes.
- Test Alt-drag move/resize, clean tiled dividers, games/fullscreen exclusion,
  indicator, Aero-Snap, Alt+Tab, `Ctrl+Alt+1`, `Ctrl+Alt+2`, and Volume Mixer.
- Test shell icons for applications, `.lnk`, folders, executables, and ordinary files.

### Linux/Wayland

- Verify COSMIC/GNOME shortcut installation and removal, including screenshot
  deep link; re-launch the binary to test the single-instance toggle.
- Verify the layer-shell palette resizes without stale composite flashes.
- Test `wpctl` and `pactl` fallback behavior, `wl-copy`, and screenshot capture
  fallback tools in the documented order.
- Confirm Alt-drag is hidden and the compositor’s native gesture remains the
  documented alternative.

### macOS

- Verify Accessory app/tray behavior and `Cmd+Shift+Space`.
- Grant Screen Recording and Accessibility; test screenshot, paste-to-previous,
  and Alt-drag. Deny each permission once and verify an explanatory failure.
- In Settings → Permissions & Diagnostics, verify live grant state, each System
  Settings link, status refresh, screenshot test, and Alt-drag instructions.
- Test Automation prompts for Finder-aware search, Empty Trash, and system
  actions; test the Keychain OAuth reads through `/usr/bin/security`.
- Test `.app` icon identity by path and a Retina/multi-monitor screenshot.

## Performance-sensitive checks

Judge screenshot latency, icon warmup, and large file search from a release
build. Dev builds are intentionally much slower and may load from Vite. Do not
eagerly resolve all installed application icons as a test shortcut; that would
hide the production caching behavior.

## Keeping this document current

Update this page when test commands, test files, supported OS targets, manual
checklists, permissions, release-build assumptions, or coverage claims change.
Add a regression test or a manual checklist entry in the same change that fixes
the underlying bug; label platform verification honestly.
