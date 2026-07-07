# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

Commandeer is a Raycast-style command palette built with Tauri 2 (React/TypeScript frontend, Rust backend). It is **cross-platform: Windows, Linux (Wayland/COSMIC), and macOS** — originally Windows-only, then ported to Linux, then to macOS. It is **still in active development** with new features being added regularly; keep this file updated as the app evolves.

Checks: `npm run build` runs `tsc` (strict) and is the frontend type-check; `npm run lint` runs ESLint (react-hooks rules only); `cargo test` in `src-tauri/` runs the Rust unit tests; `cargo clippy --all-targets -- -D warnings` must stay clean. There is deliberately **no CI** — it was added and removed twice; don't re-add it. Clippy lints are platform-gated, so a clean local run only proves the current OS: treat cross-OS clippy as unverified until the code is pulled on the other machines.

## Commands

```bash
bun install                          # install JS deps — bun.lock is the source of truth
                                     # (run this after pulling or tsc fails)
npm run tauri dev                    # run the app in dev mode (vite + cargo)
npm run tauri build -- --no-bundle   # release build (on Linux: source ~/.cargo/env first)
                                     # NEVER `cargo build --release` directly: without the tauri
                                     # CLI the binary is dev-mode and loads localhost:5173
npm run build                        # tsc + vite build (frontend only; use as the type-check)
npm run release                      # cross-platform release build + copy artifact to bin/
                                     #   Windows: commandeer.exe
                                     #   Linux:   commandeer binary
                                     #   macOS:   commandeer.app bundle
```

Linux dev/test notes:
- Kill a running instance with `pkill -x commandeer` — **not** `pkill -f`, which matches and kills the invoking shell.
- Re-launching the binary toggles the palette (single-instance plugin) — this is the reliable trigger under Wayland, where global shortcuts (X11 grabs) don't work. The app also manages a COSMIC custom shortcut (Ctrl+Space; Alt+Space in game mode).
- `COMMANDEER_NO_AUTOHIDE=1` disables the focus-loss auto-hide (useful when inspecting the window).
- Screenshots: `cosmic-screenshot --interactive=false --notify=false --save-dir DIR`.
- Icons in `src-tauri/icons/*.png` must be RGBA — RGB fails the `generate_context!` macro on Linux.

macOS dev/test notes:
- The app is an Accessory (no Dock icon / Cmd-Tab entry). Use the tray icon or the global hotkey to surface it.
- Default toggle hotkey is `Cmd+Shift+Space` (Spotlight owns `Cmd+Space`, input-source switching owns `Ctrl+Space`).
- Screenshot capture and paste-to-previous require permission grants: **Screen Recording** for screenshots, **Accessibility** for paste. Until granted the commands fail with instructions rather than silently no-oping.
- Shutdown/Restart/Logout/Empty Trash trigger a one-time **Automation** prompt on first use (System Events / Finder). `@search` over the focused Finder folder uses the same Finder Automation channel (and only queries Finder when the palette opened over it; otherwise it falls back to the home folder like Linux).
- Clipboard history is encrypted at rest on all three platforms: DPAPI on Windows; ChaCha20-Poly1305 on Linux (key in the Secret Service, 0600 key-file fallback) and macOS (0600 key file next to the db). **Do not move the macOS key to the Keychain** while the app ships ad-hoc-signed: Keychain ACLs bind to the code signature, so every rebuild re-prompts — and the prompt fires during setup and blocks launch (verified on-device).
- The palette window joins all Spaces (`canJoinAllSpaces | fullScreenAuxiliary`), so toggling it never switches Spaces and it appears over fullscreen apps.
- App icons: `.app` bundles are directories, so both icon caches (Rust `icons.rs`, frontend `ResultRow`) key them **per path**, never on the shared folder/extension slot — regressing this makes every app render as the first-resolved app's icon. Icons are downscaled to ≤128px before base64 (a raw `iconForFile:` TIFF is a 1024×1024, ~2 MB payload). `iconForFile:` costs ~175 ms/icon **cold**, so the macOS icon cache is **persisted to disk** (`<app-cache>/icon-cache-v1.json`, keyed by path + mtime; a background flusher thread writes it every 3 s when dirty) and a **gentle sequential background warm** at startup (`lib.rs` setup → `icons::warm_app_icons`) resolves every installed app once. After the first run every icon loads from disk, so the Apps folder paints real icons immediately. **Do not** eagerly resolve the whole app list from the frontend per-launch — that re-pays the cold cost every time and queues the visible rows behind the entire install list (the reason the disk cache exists).
- `npm run release` produces a signed/unsigned `bin/commandeer.app` bundle; right-click → Open the first time if unsigned.

## Shipping changes

After **every** completed task, bug fix, or feature — once the work is done and verified — ship it: **(1) commit and push, (2) rebuild the release binary, (3) restart the running process** on the new binary. The running app should always reflect committed code. Use the `ship-change` skill (`.claude/skills/ship-change/SKILL.md`), which encodes the exact per-OS steps (Windows/macOS/Linux):

- Commit with the repo's footer lines and `git push` (this repo ships from `main`).
- Rebuild with `npm run tauri build -- --no-bundle` (Linux/macOS: `source ~/.cargo/env` first; Windows: `npm run release`) — only a release build is representative.
- Restart: kill the old process, then relaunch — `pkill -x commandeer` + `./src-tauri/target/release/commandeer` on Linux/macOS, `Stop-Process -Name commandeer` + the built exe on Windows. Kill before launching, since launching alone just toggles the palette (single-instance plugin).

This is also enforced by a **Stop hook** (`.claude/hooks/ship-reminder.mjs`, wired in `.claude/settings.json`): when a turn ends with uncommitted changes it blocks once and asks the model to decide whether the work is a complete feature/fix and ship it — it never auto-commits, and it stays silent on a clean tree. The hook is Node (shell-neutral) so it runs identically on all three OSes. If any step fails (build error, rejected push), stop and surface it rather than reporting the change as shipped.

Everything under `.claude/` (this skill, the hook, project settings) is committed and shared across systems; only `.claude/settings.local.json` is gitignored for personal overrides.

## Architecture

Two always-running Tauri windows that hide/show rather than launching per use: the palette (label `palette`, transparent, undecorated) and the screenshot overlay (label `screenshot`, opaque fullscreen). A tray icon (cross-platform; non-fatal if it can't be created on Linux) and the global hotkey / single-instance toggle are the entry points.

### Screenshot tool

Lightshot-style region capture: trigger → Rust freezes the screen to `<app-cache>/frame.png` (on Linux a four-CLI fallback chain — `cosmic-screenshot` → `gnome-screenshot` → `spectacle` → `grim`, first one present wins; on Windows a GDI BitBlt of the full virtual screen — all monitors — with the overlay spanning the same bounds; on macOS `screencapture -R` of the cursor monitor) → the `screenshot` window (same JS bundle; `main.tsx` branches on window label to `ScreenshotOverlay.tsx`) shows the frame under a dim veil → drag a region → Rust crops, saves to `~/Pictures/Screenshots`, and copies PNG to the clipboard (`wl-copy` on Linux, arboard on Windows/macOS). Esc cancels. Backend in `commands/screenshot.rs`. Triggers: `commandeer://screenshot` deep link (bound to PrtScn via a second managed COSMIC shortcut line on Linux), a configurable global shortcut on Windows/macOS (`screenshot_hotkey` config, **default `Insert` on Windows**, editable via Settings → Screenshot Hotkey), and a Tools → Take Screenshot palette command. **Do not default the Windows shortcut to PrintScreen**: `RegisterHotKey(VK_SNAPSHOT)` returns success but never fires `WM_HOTKEY` because PrtScn emits no `WM_KEYDOWN` — so it silently does nothing. Any ordinary key (Insert, Fn keys, letters+modifiers) works. macOS has no default screenshot hotkey because Mac keyboards lack PrintScreen and the common system shortcuts are `Cmd+Shift+3/4/5`. The frame is encoded as fast/unfiltered PNG (transient file, reloaded once then deleted) — ~50 ms capture on a 2560×1440 release build; the unoptimized dev build is ~15× slower, so judge screenshot latency only from a release build. On Windows the overlay appears via a cloak-then-reveal handshake: at capture time the window is positioned/sized and shown **DWM-cloaked** (composited but not displayed — WebView2 only renders while visible), the frame `<img>` loads and rasterizes off-screen, and `reveal_screenshot_overlay` uncloaks only when the webview reports the image was actually **presented**, via an **Element Timing** observer (`elementtiming="shot-frame"`). onLoad/double-rAF are NOT sufficient signals — they race the GPU raster of the multi-monitor-sized texture and flashed black. Fallbacks: a 500 ms frontend timer post-onload and a 1500 ms Rust-side force-show+uncloak; all commands are idempotent. On Linux the overlay is a 4-edge-anchored, exclusive-keyboard layer-shell surface; the frontend's `show_screenshot_overlay` call on img onload is the show path there (and on macOS) and the cloak machinery is Windows-only. Linux has its own stale-frame defense (there is no DWM cloak, and GTK3 toplevel opacity is a no-op on Wayland): WebKitGTK replays its **last composite** as the first frame when a hidden window is re-mapped, so the overlay window is **transparent on Linux** (`tauri.linux.conf.json` — platform configs replace the whole `app.windows` array, keep it in sync with `tauri.conf.json`) and the frontend always paints a cleared, fully transparent state (no frame, no veil) and waits a double-rAF (`afterClearPaint`) **before** any hide — finish, Esc-cancel, and the re-trigger path (Rust emits `screenshot-clear`; the webview clears then calls `hide_screenshot_overlay`; Rust force-hides after its pre-capture delay as fallback). The replayed composite is then invisible, the live desktop underneath is pixel-identical to the frozen frame, and the overlay's appearance reads as a single smooth veil dim. On Windows, both windows set `additionalBrowserArgs` with `CalculateNativeWinOcclusion` disabled (WebView2 browser args are process-wide — keep the two windows' args identical): without it, Chromium suspends rendering of hidden windows, the new frame never paints before `show()`, and the window flashes its stale surface (the previous capture) for a frame.

### Window management (Alt-drag, `window_drag`)

Hyprland-style: hold **Alt** and drag any window to move it, Alt + right-drag to resize. Toggle in **Settings → Alt-Drag Windows** (`window_drag` config, re-applied at startup). Backend `commands/window_drag.rs`, one `platform` module per OS. **Windows** is the full implementation (see `TODO.md`): a `WH_MOUSE_LL` hook on a dedicated pump thread records the grab and swallows only the button events — it must **never** swallow `WM_MOUSEMOVE` (freezes the cursor → the jitter/snap-back bug) and **never** call `SetWindowPos` (stalls system input); a separate mover thread polls `GetCursorPos` at ~200 Hz (`timeBeginPeriod(1)` while dragging) and repositions. Fullscreen/borderless games are left alone (`is_fullscreen_window`: not `IsZoomed`, covers the whole monitor incl. taskbar, and borderless). The grabbed window is raised on grab via `AttachThreadInput` + `SetForegroundWindow` (a bare `SetWindowPos(HWND_TOP)` is ignored cross-process). Snapping features are Windows-only:

- **Hover indicator** — a click-through per-pixel-alpha overlay (`UpdateLayeredWindow`, sized to DWM extended-frame bounds with rounded corners) dims the window and highlights the resize region (quadrants for a free window, halves for a snapped one). It's **hidden** whenever the resize is locked to a single shared divider (the same `clean_tile_edge` test the resize uses).
- **Resize edge selection** — a half-snapped window resizes from its one free edge. An un-snapped window whose edge is **cleanly tiled** — shared with neighbor(s) that span it without overhanging (`clean_tile_edge`) — resizes only that edge and locks the rest, so a quarter-tiled window keeps its width/position (grid-style: two stacked windows resize only their shared divider). Otherwise, a free quadrant-corner resize.
- **Tiling** — resizing a shared edge moves **every** window flush along it (`find_neighbors` samples the whole edge, not one midpoint), in a single `DeferWindowPos` batch, clamped so none drops below `MIN_SIZE`. Neighbor facing edges are overlapped 3/4 of the combined invisible border to shrink the visible gap.
- **Aero-Snap on move** — previews + commits half/quarter/maximize when a drag reaches a screen edge (160 px band). Snapping to a side **fills the space** beside an already-snapped window (`snap_fill_x`) instead of a fixed half.

**macOS** implements move/resize + raise-on-grab (`CGEventTap` + Accessibility, needs the Accessibility grant). It now **compiles and links on macOS** (the `kAX*` attribute constants are `CFSTR` macros, not linkable symbols, so they're built at runtime via `CFStringCreateWithCString`); behavioral testing on-device is still owed. **Linux** is unsupported by design (Wayland forbids a client from moving other apps' windows; COSMIC provides Super+drag natively), so the Settings entry is hidden there.

### Frontend (`src/`)

Everything hangs off three types in `src/types.ts`:

- **`Command`** — one entry in the root list. Either runs directly (`action`) or pushes a **`Step`**.
- **`Step`** — one level of the palette's navigation stack (list, grid, slider, form, or free-text input step). `onSelect`/`onCommitQuery` return a `StepResult` (`done` / `push` / `replace` / `pop` / `stay`) that drives navigation.
- **`CommandProvider`** (`src/providers/`) — contributes static root commands (`getCommands`) and/or per-query inline results (`search`). Registered in `src/providers/index.ts`. Newer feature families live here; the older script and settings sources are assembled directly in `App.tsx`'s `refresh()`. Quick Links, Notes and Bookmarks render as sub-folders inside the Tools virtual folder (wired in `refresh()`).

`App.tsx` builds the command list (grouping `folderName`-tagged commands under virtual folders) and hands it to `components/Palette.tsx` (~1900 lines), which owns the step stack, query state, fuzzy ranking (fzf + frecency in `src/lib/`), keyboard handling, and the Ctrl+K action panel. `src/lib/tauri.ts` is the single wrapper around all Rust `invoke` calls. `src/lib/appEvents.ts` is a mutable bridge so settings commands can flip App-level state without prop drilling.

User-facing "commands" also come from a scripts directory on disk (configurable `scripts_dir`; `.ps1`/`.lnk` on Windows, `.sh`/`.desktop`/`.AppImage`/executables on Linux, `.sh`/`.command`/executables on macOS), scanned by the Rust side.

### Backend (`src-tauri/src/`)

`lib.rs` holds setup (window creation, global shortcut, tray, single-instance, deep links) plus window show/hide/positioning. One module per feature in `commands/` (audio, clipboard, file_index, launcher, process, stats, …), all registered in the `invoke_handler` in `lib.rs`. The file index is self-hosted: SQLite + FTS5 (trigram) fed by a `notify` filesystem watcher.

### Platform split

All OS-specific code is behind `#[cfg(target_os = "windows")]` / `#[cfg(target_os = "linux")]` / `#[cfg(target_os = "macos")]` in Rust and `IS_LINUX` / `IS_MAC` (user-agent) checks in the frontend. Never use a bare `#[cfg(not(windows))]` branch for Linux/macOS-specific code — gate each OS explicitly (or `unix` only when the code is genuinely identical, like `PermissionsExt`). The three platforms differ most in:

- **Window sizing/positioning.** Windows: frontend `setSize` + min/max + cursor-monitor positioning. Linux/Wayland: cosmic-comp ignores client resizes/moves of mapped toplevels, so the palette is rendered as a **wlr-layer-shell overlay** (gtk-layer-shell, set up in `lib.rs`), anchored to the top edge with a fixed margin; the frontend measures content height and calls the `resize_palette` Rust command, which changes the GTK size request to resize in place without flicker. `html,body,#root` are content-height on purpose so this measurement works. macOS: a normal always-on-top transparent window positioned via Tauri monitor APIs; vibrancy and rounded corners are applied with `window-vibrancy`.
- **Launching & icons.** Windows uses PowerShell/shell32 (`.lnk` icon extraction); Linux parses `.desktop` files and launches via direct exec / `sh` / `gio launch` / `xdg-open`; macOS scans `.app` bundles and launches via `open`. File-search icons use the same shell APIs (`SHGetFileInfoW` on Windows, `NSWorkspace.iconForFile:` on macOS).
- **Global hotkey.** See Linux/macOS notes above; `set_game_mode` in `lib.rs` rewrites the COSMIC custom-shortcut config on Linux and switches the registered base hotkey everywhere.

Config is JSON read/written by the Rust side (`commands/config.rs`; `scripts_dir` defaults per-platform). Lightweight UI prefs (game mode, widget visibility, script cache) live in webview `localStorage`.
