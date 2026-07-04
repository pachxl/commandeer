# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

Commandeer is a Raycast-style command palette built with Tauri 2 (React/TypeScript frontend, Rust backend). It is **cross-platform: Windows and Linux (Wayland/COSMIC)** — originally Windows-only, later ported. It is **still in active development** with new features being added regularly; keep this file updated as the app evolves.

There is no test suite or linter configured. `npm run build` runs `tsc` and is the type-check.

## Commands

```bash
bun install                          # install JS deps — bun.lock is the source of truth
                                     # (package-lock.json is stale; run this after pulling or tsc fails)
npm run tauri dev                    # run the app in dev mode (vite + cargo)
npm run tauri build -- --no-bundle   # release build (on Linux: source ~/.cargo/env first)
                                     # NEVER `cargo build --release` directly: without the tauri
                                     # CLI the binary is dev-mode and loads localhost:5173
npm run build                        # tsc + vite build (frontend only; use as the type-check)
npm run release                      # Windows-only: build + copy commandeer.exe to bin/
```

Linux dev/test notes:
- Kill a running instance with `pkill -x commandeer` — **not** `pkill -f`, which matches and kills the invoking shell.
- Re-launching the binary toggles the palette (single-instance plugin) — this is the reliable trigger under Wayland, where global shortcuts (X11 grabs) don't work. The app also manages a COSMIC custom shortcut (Ctrl+Space; Alt+Space in game mode).
- `COMMANDEER_NO_AUTOHIDE=1` disables the focus-loss auto-hide (useful when inspecting the window).
- Screenshots: `cosmic-screenshot --interactive=false --notify=false --save-dir DIR`.
- Icons in `src-tauri/icons/*.png` must be RGBA — RGB fails the `generate_context!` macro on Linux.

## Architecture

Two always-running Tauri windows that hide/show rather than launching per use: the palette (label `palette`, transparent, undecorated) and the screenshot overlay (label `screenshot`, opaque fullscreen). A tray icon (Windows-only) and the global hotkey / single-instance toggle are the entry points.

### Screenshot tool

Lightshot-style region capture: trigger → Rust freezes the screen to `<app-cache>/frame.png` (`cosmic-screenshot` CLI on Linux, GDI BitBlt of the full virtual screen — all monitors — on Windows, with the overlay spanning the same bounds) → the `screenshot` window (same JS bundle; `main.tsx` branches on window label to `ScreenshotOverlay.tsx`) shows the frame under a dim veil → drag a region → Rust crops, saves to `~/Pictures/Screenshots`, and copies PNG to the clipboard (`wl-copy` on Linux, arboard on Windows). Esc cancels. Backend in `commands/screenshot.rs`. Triggers: `commandeer://screenshot` deep link (bound to PrtScn via a second managed COSMIC shortcut line on Linux), a configurable global shortcut on Windows (`screenshot_hotkey` config, **default `Insert`**, editable via Settings → Screenshot Hotkey), and a Tools → Take Screenshot palette command. **Do not default the Windows shortcut to PrintScreen**: `RegisterHotKey(VK_SNAPSHOT)` returns success but never fires `WM_HOTKEY` because PrtScn emits no `WM_KEYDOWN` — so it silently does nothing. Any ordinary key (Insert, Fn keys, letters+modifiers) works. The frame is encoded as fast/unfiltered PNG (transient file, reloaded once then deleted) — ~50 ms capture on a 2560×1440 release build; the unoptimized dev build is ~15× slower, so judge screenshot latency only from a release build. On Windows the overlay appears via a show-then-reveal handshake: `show_screenshot_overlay` shows the window **DWM-cloaked** (composited but not displayed), the frontend waits for a real paint of the frame (double-rAF after `<img>` onload), then `reveal_screenshot_overlay` uncloaks — atomic in DWM, so no stale-frame or black flash ever hits the screen (a 1500 ms Rust-side fallback force-shows *and* uncloaks; both commands are idempotent). The window is also positioned/sized at capture time, while still hidden — resizing at show time made WebView2 clear to black. On Linux the overlay is a 4-edge-anchored, exclusive-keyboard layer-shell surface and none of the cloak machinery applies. On Windows, both windows set `additionalBrowserArgs` with `CalculateNativeWinOcclusion` disabled (WebView2 browser args are process-wide — keep the two windows' args identical): without it, Chromium suspends rendering of hidden windows, the new frame never paints before `show()`, and the window flashes its stale surface (the previous capture) for a frame.

### Frontend (`src/`)

Everything hangs off three types in `src/types.ts`:

- **`Command`** — one entry in the root list. Either runs directly (`action`) or pushes a **`Step`**.
- **`Step`** — one level of the palette's navigation stack (list, grid, slider, form, or free-text input step). `onSelect`/`onCommitQuery` return a `StepResult` (`done` / `push` / `replace` / `pop` / `stay`) that drives navigation.
- **`CommandProvider`** (`src/providers/`) — contributes static root commands (`getCommands`) and/or per-query inline results (`search`). Registered in `src/providers/index.ts`. Newer feature families live here; the older script, snippet, and settings sources are assembled directly in `App.tsx`'s `refresh()`.

`App.tsx` builds the command list (grouping `folderName`-tagged commands under virtual folders) and hands it to `components/Palette.tsx` (~1500 lines), which owns the step stack, query state, fuzzy ranking (fzf + frecency in `src/lib/`), keyboard handling, and the Ctrl+K action panel. `src/lib/tauri.ts` is the single wrapper around all Rust `invoke` calls. `src/lib/appEvents.ts` is a mutable bridge so settings commands can flip App-level state without prop drilling.

User-facing "commands" also come from a scripts directory on disk (configurable `scripts_dir`; `.ps1`/`.lnk` on Windows, `.sh`/`.desktop`/`.AppImage`/executables on Linux), scanned by the Rust side.

### Backend (`src-tauri/src/`)

`lib.rs` holds setup (window creation, global shortcut, tray, single-instance, deep links) plus window show/hide/positioning. One module per feature in `commands/` (audio, clipboard, file_index, launcher, process, stats, …), all registered in the `invoke_handler` in `lib.rs`. The file index is self-hosted: SQLite + FTS5 (trigram) fed by a `notify` filesystem watcher.

### Platform split

All OS-specific code is behind `#[cfg(target_os = "windows")]` / `#[cfg(not(windows))]` in Rust and an `IS_LINUX` (user-agent) check in the frontend. The two platforms differ most in:

- **Window sizing/positioning.** Windows: frontend `setSize` + min/max + cursor-monitor positioning. Linux/Wayland: cosmic-comp ignores client resizes/moves of mapped toplevels, so the palette is rendered as a **wlr-layer-shell overlay** (gtk-layer-shell, set up in `lib.rs`), anchored to the top edge with a fixed margin; the frontend measures content height and calls the `resize_palette` Rust command, which changes the GTK size request to resize in place without flicker. `html,body,#root` are content-height on purpose so this measurement works.
- **Launching & icons.** Windows uses PowerShell/shell32 (`.lnk` icon extraction); Linux parses `.desktop` files and launches via direct exec / `sh` / `gio launch` / `xdg-open`.
- **Global hotkey.** See Linux notes above; `set_game_mode` in `lib.rs` rewrites the COSMIC custom-shortcut config on Linux.

Config is JSON read/written by the Rust side (`commands/config.rs`; `scripts_dir` defaults per-platform). Lightweight UI prefs (game mode, widget visibility, script cache) live in webview `localStorage`.
