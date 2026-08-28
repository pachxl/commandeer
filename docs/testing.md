# Testing and verification

Commandeer has pure frontend tests, Rust unit tests, static checks, and
platform-specific manual verification. Passing one category does not prove
another: in particular, a Linux build does not prove Windows hook behavior or
macOS permissions.

## Local checks

Run from the repository root unless noted:

| Check                    | Command                                                     | What it proves                                    |
| ------------------------ | ----------------------------------------------------------- | ------------------------------------------------- |
| Install/preparation      | `bun install`                                               | Dependencies and Husky hook are present           |
| Frontend typecheck/build | `npm run build`                                             | Strict `tsc` plus Vite production bundle          |
| Regression suite         | `npm test`                                                  | Vitest plus lightweight Node release/config tests |
| Atomic release helper    | `npm run test:release`                                      | Asset validation and nine-key updater manifest    |
| ESLint                   | `npm run lint`                                              | Configured React hooks/lint rules                 |
| Frontend formatting      | `npm run format:check`                                      | Prettier-clean frontend/docs/config files         |
| Rust unit tests          | `cargo test` in `src-tauri/`                                | Current OS Rust tests and pure native logic       |
| Rust formatting          | `cargo fmt --manifest-path src-tauri/Cargo.toml --check`    | rustfmt-clean Rust                                |
| Rust lint                | `cargo clippy --all-targets -- -D warnings` in `src-tauri/` | Current OS warning-free clippy                    |
| Agent sync               | `node .agents/hooks/check-agent-sync.mjs`                   | Canonical/mirrored agent wiring                   |
| Release binary           | `npm run tauri build -- --no-bundle`                        | Tauri release build, not a Vite-only dev artifact |

The Tauri package script injects the exact numeric Git tag as the local build
version (or `RELEASE_VERSION` in CI). This keeps a locally built release from
mistaking the current published tag for an update.

The repository’s `npm run format:check` also checks Rust formatting through its
package script. Use `npm run format` only when formatting changes are intended.

The focused atomic-release helper tests cover the complete nine-key updater
manifest and rejection of a missing updater payload. The live workflow adds the
GitHub-side invariant: packages stay on one draft until every expected asset and
the uploaded `latest.json` have been validated, then the draft is published once.

## Frontend tests

Current regression coverage includes Palette reducer transitions, fuzzy
ranking, focus-aware polling, feedback cleanup, onboarding and guide behavior,
destructive confirmations, configured-path parsing and Settings persistence,
multiline forms, Windows Volume Mixer rendering/navigation, Onix compact-session
transitions, serialized native sizing, 2× optical render metrics and pointer-ray
geometry, optical fallback/accessibility policy, bundled footer-logo fallback,
semantic resource-stat colours, and selection-lens geometry.
Put pure ranking, parsing, reducer, geometry, and serialization tests beside the
implementation. Mock Tauri calls at the wrapper boundary rather than importing
native modules into jsdom tests. jsdom cannot validate shader output or native
backdrop material, so WebGL tests prove fallback policy and DOM contracts while
the final appearance remains a release-build manual check.

## Rust tests

Rust tests cover command parsing and pure platform calculations such as shortcut
parsing, file-index schema/search behavior, clipboard crypto round trips,
screenshot stroke clipping, process snapshots, and window-drag geometry where
the target OS exposes those modules. Onix also tests native radius/DPI math,
macOS morph interpolation, palette dimension validation, and fixed-top frame
geometry. Keep OS-gated tests explicit so a green Linux run does not imply
Windows/macOS coverage.

## Manual platform checks

### All platforms

- Open/dismiss the palette repeatedly; verify pending confirms and toasts do not
  survive dismissal.
- With empty localStorage, verify the welcome guide opens on first focus only;
  with an existing-install key, verify it stays searchable but does not auto-open.
- Verify loading, empty, and error panels are legible in both UI styles.
- In Onix, verify a new root session opens as a compact search capsule. Typing,
  clicking the search surface, or pressing Up/Down/Enter/Tab must expand it;
  clearing the query and popping a step must keep it expanded until dismissal,
  and the next invocation must be compact again. The expansion should grow
  downward from a stationary top edge with no radius snap, dark-layer flash, or
  second outline; slow-motion capture must not expose a lighter native-glass
  band beneath the WebGL surface.
- Exercise list, grid, and Actions selection. Verify exactly one moving lens is
  visible on the active surface, it follows keyboard and real pointer movement,
  and opening Actions deactivates the result lens. Repeat while results load,
  shrink, rerank, and scroll so Enter still targets the rendered highlight.
- Test Onix at 50%, 100%, and 150% scale, at low and high window transparency,
  and with dark and light themes. The shell must remain dark/readable while the
  theme accent continues to tint highlights and semantic states.
- Disable WebGL2 or force a context loss and verify the CSS optical fallback is
  seamless and interactive. Enable reduced motion, reduced transparency, and
  forced-colors independently: motion must stop, reduced transparency must be
  opaque, and system colors must remain legible without decorative caustics.
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
- In Onix, verify Acrylic is clipped to the compact capsule and expanded panel
  at 100%, 150%, and a non-integer display scale. Expansion must not expose a
  rectangular native backing frame or briefly restore the older corner radius.
- Test screenshot overlay across all monitors for stale/black-frame flashes.
- Test Alt-drag move/resize, clean tiled dividers, games/fullscreen exclusion,
  indicator, Aero-Snap, Alt+Tab, `Ctrl+Alt+1`, `Ctrl+Alt+2`, and Volume Mixer.
- Test shell icons for applications, `.lnk`, folders, executables, and ordinary files.

### Linux/Wayland

- Verify COSMIC/GNOME shortcut installation and removal, including screenshot
  deep link; re-launch the binary to test the single-instance toggle.
- Verify the layer-shell palette resizes without stale composite flashes.
- Repeatedly expand Onix while results and widgets change height. Verify the
  serialized newest geometry wins and the transparent CSS optical fallback has
  clean capsule/panel corners without resize oscillation.
- Test `wpctl` and `pactl` fallback behavior, `wl-copy`, and screenshot capture
  fallback tools in the documented order.
- Confirm Alt-drag is hidden and the compositor’s native gesture remains the
  documented alternative.

### macOS

- Verify Accessory app/tray behavior and `Cmd+Shift+Space`.
- On macOS 26+, verify Onix uses the native Liquid Glass content wrapper inside
  its rounded clipping container at both compact and expanded radii. Test over
  bright, dark, and high-frequency backgrounds: no translucent rectangular
  corner may extend beyond the rounded
  vessel. Verify the 150 ms capsule-to-panel animation keeps its top edge fixed,
  continuously changes the glass radius, and becomes immediate with Reduce
  Motion. On older macOS, verify the vibrancy fallback has matching geometry,
  remains focusable, and does not acquire a rectangular shadow.
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

Judge Onix optics from a release build as well. Pointer movement should schedule
frames only until its light settles, the canvas backing store must remain capped
at 2× device pixel ratio, and repeated idle openings must not show continuous
GPU activity. Test the CSS fallback separately rather than treating a working
shader as proof of fallback quality.

## Keeping this document current

Update this page when test commands, test files, supported OS targets, manual
checklists, permissions, release-build assumptions, or coverage claims change.
Add a regression test or a manual checklist entry in the same change that fixes
the underlying bug; label platform verification honestly.
