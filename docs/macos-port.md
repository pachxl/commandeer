# Commandeer → macOS port plan

Status: **planning** (no code changes yet). Commandeer currently targets
**Windows** and **Linux (Wayland/COSMIC)**; this document is the plan for adding
**macOS** as a third supported target from the same codebase.

## The core idea (how it stays "in-line" across platforms)

**One repo, one branch, no fork.** Clone on the Mac, work there, commit + push;
pull on the Windows box. Cross-platform behavior lives entirely in
`#[cfg(target_os = "...")]` gates, exactly as Windows/Linux already coexist.
macOS becomes a third arm.

**The one structural gotcha:** the codebase treats "not Windows" as "Linux."
`#[cfg(not(target_os = "windows"))]` is *true on macOS*, so macOS currently
inherits Linux's GTK/Wayland code — which won't compile. The mechanical heart of
the port is **splitting `not(windows)` into `linux` vs `macos`** wherever it
touches GTK, and adding Mac arms elsewhere.

**Rule to keep all platforms healthy:** never add a bare `not(windows)` branch
again — always gate `linux` and `macos` explicitly (or `unix` only when the code
is genuinely identical on both, like the `PermissionsExt` use in `fs.rs`).

## What's already portable (zero work)

- **Entire frontend** (`src/`, React/TS/Vite) — verified building on macOS.
- Cross-platform Rust: file index (SQLite+FTS5), `notify` watcher, `walkdir`,
  `reqwest`/rates, `claude` usage, `config`/`store`/snippets/quicklinks/themes,
  `fzf`, `arboard`.
- `fs.rs` — uses `std::os::unix::fs::PermissionsExt`, already correct on macOS.
- `stats.rs` — already has a `not(any(windows, linux))` fallback arm.
- Plugins: `autostart` (already `MacosLauncher::LaunchAgent`), `single-instance`,
  `deep-link`, `opener`, `global-shortcut` all support macOS.
- Many `not(windows)` command bodies are already stubs (`list_apps` → `vec![]`,
  audio/system/process/paste → errors), so they compile on macOS as-is with the
  same feature gaps Linux has.

## Phase 0 — Toolchain (prerequisite)

- [ ] Install Rust: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
      (Xcode Command Line Tools already present)
- [ ] `source ~/.cargo/env`; confirm `cargo --version`
- [ ] `bun install` (done)
- [ ] Bun/Node already installed

## Phase 1 — Make it compile & launch (structural)

Goal: an app that *runs* on macOS with the same feature gaps Linux has. This is
the bulk of the feasibility risk, and it's small.

- [ ] **`src-tauri/Cargo.toml`** — regate the GTK block:
      `[target.'cfg(not(windows))'.dependencies]` (gtk, gtk-layer-shell) →
      `[target.'cfg(target_os = "linux")'.dependencies]`. Stops macOS from
      building GTK.
- [ ] **`src-tauri/src/lib.rs`** — every `#[cfg(not(target_os = "windows"))]`
      block that uses `gtk`/`gtk_layer_shell` becomes `#[cfg(target_os = "linux")]`:
  - `PALETTE_TOP_MARGIN` const
  - `resize_palette` body (Linux-only; macOS/Windows resize via the frontend)
  - the layer-shell setup in `.setup()`
  - `update_cosmic_shortcut` and its calls in `set_game_mode`/`setup`
- [ ] **`src-tauri/src/lib.rs`** — add a `#[cfg(target_os = "macos")]` setup arm.
      Minimum to launch: nothing special (the palette window is already
      `transparent`, `decorations:false`, `alwaysOnTop`, `center`). Rounded
      corners / vibrancy come in Phase 2.
- [ ] **`src-tauri/tauri.conf.json`** — palette `windowEffects` uses `"acrylic"`
      (Windows-only, ignored on mac). Later swap for a macOS material
      (`"hudWindow"` / `"popover"`).
- [ ] **`src/lib/tauri.ts`** (wherever `IS_LINUX` lives) — add `IS_MAC`
      (`navigator.userAgent.includes('Mac')`). macOS follows the **Windows**
      sizing path (frontend `setSize`), not the Linux layer-shell path.
- [ ] `npm run tauri dev` → iterate until it launches. Expected: palette opens
      via global shortcut; launcher/audio/screenshot/etc. dead (same as Linux).
      This confirms macOS is a supported target.

## Phase 2 — Core UX parity

- [ ] **Window positioning** (`position_on_cursor_monitor`): add a macOS arm.
      Cleanest option — replace both with Tauri's cross-platform
      `app.cursor_position()` + `available_monitors()` and pick the containing
      monitor (could unify Windows too).
- [ ] **Non-activating panel (Raycast feel):** add the `tauri-nspanel` plugin
      (community, Tauri v2) so the palette is an `NSPanel` that shows over
      fullscreen apps and doesn't steal focus/activate the app. Optional but this
      is what makes it feel native.
- [ ] **Vibrancy + rounded corners:** `NSVisualEffectView` material via
      `windowEffects`; corner radius via the panel.
- [ ] **Global hotkey:** `global-shortcut` works natively on macOS — verify
      registration. Note Cmd+Space collides with Spotlight; pick a default or
      keep it configurable (plumbing already exists).
- [ ] **Tray icon** (`setup_tray`, currently Windows-only): enable for macOS;
      Tauri tray works. Icons must be RGBA.

## Phase 3 — Feature backends (each independent; fill in over time)

| Feature | File | macOS approach |
|---|---|---|
| App launcher list | `launcher.rs` | Scan `/Applications`, `/System/Applications`, `~/Applications` for `.app` bundles |
| Launch app | `launcher.rs` | `open <path>` / `open -a` |
| Screenshot capture | `screenshot.rs` | Built-in `screencapture -x -t png <file>` (replaces `cosmic-screenshot`) |
| Screenshot → clipboard | `screenshot.rs` | `arboard` image set (replaces `wl-copy`), or `osascript` |
| Paste to previous | `paste.rs` | Simulate ⌘V via `CGEvent` — **needs Accessibility permission** |
| Audio volume | `audio.rs` | `osascript -e 'set volume output volume ...'` (quick) or CoreAudio (proper) |
| System actions | `system.rs` | `osascript` / `pmset sleepnow` / lock via `pmset displaysleepnow`; empty trash via Finder AppleScript |
| Process list/kill | `process.rs` | Add cross-platform `sysinfo` crate + `libc::kill` (could unify all 3 OSes) |
| File/app icons | `icons.rs` | `NSWorkspace.icon(forFile:)` → PNG; defer (return `None` like Linux initially) |
| Everything search | `search.rs` | N/A — Windows-only indexer; macOS uses the built-in `file_index` (cross-platform) |
| Script types / `scripts_dir` | `fs.rs` / `config.rs` | Add macOS default dir + `.sh` / `.command` / executables |

**macOS's genuinely hard part — permissions.** Paste (keystroke synthesis) needs
**Accessibility**; `screencapture` of other windows may need **Screen
Recording**; global hotkeys can need **Input Monitoring**. These require user
approval in System Settings and behave differently for unsigned dev builds.
Budget time here — it's the one area with no Windows/Linux analog.

- [ ] **Verify `clipboard/crypto.rs`** — the clipboard-history encryption has cfg
      gates; confirm the `not(windows)` key path isn't Linux-keyring-specific
      (may need a macOS Keychain or portable arm).

## Phase 4 — Packaging & sync hygiene

- [ ] **`package.json` `release` script** — hardcodes `commandeer.exe`. Add a
      macOS branch producing `.app`/`.dmg` (`tauri build` emits these;
      `bundle.targets: "all"` is set).
- [ ] **`tauri.conf.json` `bundle`** — add a macOS icon (`.icns`) alongside `.ico`.
- [ ] **Deep link** — `commandeer://` on macOS needs `CFBundleURLTypes` in the
      bundle Info.plist (deep-link plugin config); runtime `register()` may not
      persist unbundled.
- [ ] **Update `CLAUDE.md`** — its "cross-platform: Windows and Linux" line and
      Platform-split section need a macOS column so future work stays consistent.
- [ ] **Consider CI** — a GitHub Actions matrix (`windows-latest` +
      `macos-latest`, later `ubuntu`) running `npm run build` + `cargo build` per
      push is the real guardrail that keeps platforms in-line automatically.

## Effort estimate

- **Phase 1 (compiles + launches):** small — a focused session. Answers "is it
  possible?" → yes.
- **Phases 2–3:** incremental, feature-by-feature; usable after Phase 2 even with
  backends stubbed.
- **Riskiest:** macOS permissions (paste/screenshot) and NSPanel behavior;
  everything else is conventional.
