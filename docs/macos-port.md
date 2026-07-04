# Commandeer → macOS port plan

Status: **Phase 1 complete** — Commandeer compiles, links, and launches on macOS
with a transparent palette window. Phases 2–4 (UX parity, feature backends,
packaging) remain. Commandeer targets **Windows**, **Linux (Wayland/COSMIC)**,
and now **macOS** from the same codebase.

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

## Phase 0 — Toolchain (prerequisite) — DONE

- [x] Install Rust via rustup (1.96.1). Xcode Command Line Tools already present.
      Installed with `--no-modify-path`; `. "$HOME/.cargo/env"` added to `~/.zshenv`.
- [x] `cargo --version` confirmed
- [x] `bun install`
- [x] Bun/Node already installed

## Phase 1 — Make it compile & launch (structural) — DONE

Goal: an app that *runs* on macOS with the same feature gaps Linux has. Achieved
in a single session; the feasibility risk was as small as expected.

- [x] **`src-tauri/Cargo.toml`** — regated the GTK block:
      `[target.'cfg(not(windows))'.dependencies]` (gtk, gtk-layer-shell) →
      `[target.'cfg(target_os = "linux")'.dependencies]`. Stops macOS from
      building GTK. (`tauri-build` also auto-added the `macos-private-api` cargo
      feature once the config below was set.)
- [x] **`src-tauri/src/lib.rs`** — every `#[cfg(not(target_os = "windows"))]`
      block that used `gtk`/`gtk_layer_shell` is now `#[cfg(target_os = "linux")]`:
  - `PALETTE_TOP_MARGIN` const
  - `resize_palette` body (Linux-only; the no-op `let _` arm is now
    `cfg(not(target_os = "linux"))` so both Windows and macOS keep params used)
  - the layer-shell + WAYLAND setup in `.setup()`
  - `update_cosmic_shortcut` and its calls in `set_game_mode`/`setup`
- [x] **macOS setup arm** — none needed to launch: the palette is already
      `transparent`, `decorations:false`, `alwaysOnTop`, `center`. Rounded
      corners / vibrancy come in Phase 2.
- [x] **`src-tauri/tauri.conf.json`** — added `"macOSPrivateApi": true` under
      `app`. **Required**: without it macOS logs "window is set to be transparent
      but macos-private-api is not enabled" and the palette isn't transparent.
      (Note: this uses private Apple APIs → not Mac App Store distributable, which
      is fine here.) The Windows-only `"acrylic"` windowEffect is ignored on mac;
      swap for a macOS material (`"hudWindow"`/`"popover"`) in Phase 2.
- [x] **Frontend — no change needed.** `IS_LINUX` keys off
      `userAgent.includes('Linux')`, which is *false* on macOS, so all three
      `IS_LINUX` branches (palette sizing, window transparency, screenshot-hotkey
      setting) already resolve to the Windows-style path on Mac. macOS uses native
      `setSize` for palette resize automatically.
- [x] `npm run tauri dev` launches: `cargo build` is clean (exit 0, 3 harmless
      dead-code warnings), app runs without panic, file index scans, transparent
      window confirmed. macOS is a supported target.

### Known non-blocking gaps after Phase 1 (expected; addressed in later phases)

- Global hotkey default is Ctrl+Space — on macOS this often collides with input-
  source switching / Spotlight; verify and/or change default in Phase 2.
- `set_window_transparency` command returns an error on macOS (Windows-only
  native path); the transparency *setting* is a no-op. Give macOS the Linux-style
  CSS-opacity fallback or a native impl in Phase 2/3.
- Screenshot / launcher / audio / system / process / paste are stubs or Linux-only
  shell-outs → Phase 3.
- No tray icon on macOS (tray is Windows-only) → Phase 2.

## Phase 2 — Core UX parity — IN PROGRESS

Done in this pass (compiles clean; app launches, toggles the palette via the
single-instance path, and survives show/hide with no panic — visual confirmation
of vibrancy/tray is left to on-device testing since screen capture needs the
Screen Recording permission):

- [x] **Global hotkey default** — macOS now defaults to `Cmd+Shift+Space`
      (Ctrl+Space = input-source switch, Cmd+Space = Spotlight). Per-platform
      const in `shortcuts.rs`; still user-configurable.
- [x] **Vibrancy + rounded corners** — added the `window-vibrancy` crate
      (macOS-target dep) and apply `NSVisualEffectMaterial::HudWindow` +
      12px radius to the palette in a `#[cfg(target_os = "macos")]` setup arm.
      Best-effort (`let _`), so failure just falls back to the plain transparent
      window. Material is easy to tweak if HudWindow reads wrong under the Light
      theme.
- [x] **Background agent** — `set_activation_policy(Accessory)` on macOS: no Dock
      icon, no Cmd-Tab entry (matches Raycast/Spotlight). The tray + hotkey are
      the entry points.
- [x] **Tray icon** — `setup_tray` gate widened to
      `any(target_os = "windows", target_os = "macos")` (Show / Start at Login /
      Quit). Autostart already uses `MacosLauncher::LaunchAgent`. NOTE: it reuses
      the colored app icon; a monochrome **template** image would look more native
      in the macOS menu bar (follow-up).

### Phase 2b — DONE (except nspanel, deferred with rationale)

- [x] **Window positioning** — added a macOS `position_on_cursor_monitor` in
      `lib.rs` on Tauri's cross-platform `cursor_position` / `monitor_from_point`
      / `work_area` APIs. The palette opens centered on the display under the
      cursor, top at ~20% of the work area. The Win32 path is left untouched (no
      Windows regression); both show paths call it on `windows|macos`.
- [x] **`set_window_transparency`** — macOS now routes through the Linux-style
      CSS-opacity fallback (added `IS_MAC` in `src/lib/tauri.ts`) instead of the
      Windows-only native invoke that errored. The transparency setting works.
- [ ] **Non-activating panel (`tauri-nspanel`) — DEFERRED to Phase 3.** Rationale:
      1. The plugin is **git-only** (branch-pinned, not on crates.io) — a
         dependency-stability decision worth making deliberately, not by default.
      2. Its main payoff — *not* stealing the previously-focused app's active
         state — only matters for **paste-to-previous**, which is a Phase 3
         feature. Bundling nspanel with that work lets it be verified end-to-end.
      3. Its other benefit (show over fullscreen / on all spaces) is a
         `collectionBehavior` tweak; both benefits are inherently interactive and
         can't be verified in a headless/CI build, so they belong with hands-on
         paste testing.
      Until then the palette is a normal always-on-top window: it works, it just
      activates the app on show and won't float over another app's fullscreen
      space.

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
