# AGENTS.md

This file is the **single source of truth** for AI coding assistants (Claude
Code, Codex) and human contributors working in this repository. `CLAUDE.md`
redirects here; do not duplicate content between them (a pre-commit check
enforces it). Keep this file updated as the app evolves.

Feature and subsystem documentation lives in [`docs/README.md`](docs/README.md).
Read the relevant page before changing behavior; update that page in the same
change when interfaces, platform support, storage, configuration, or test
procedures change.

## Project

Commandeer is a Raycast-style command palette built with Tauri 2 (React/TypeScript
frontend, Rust backend). It is **cross-platform: Windows, Linux (Wayland/COSMIC),
and macOS** — originally Windows-only, then ported to Linux, then to macOS. It is
**still in active development** with new features added regularly.

**Scope:** a desktop launcher / command palette + screen capture + window
management utility. Desktop only — there is no mobile target, and the `android/`
and `ios/` entries under `src-tauri/icons/` are leftover `tauri icon` output, not
built. Platform parity is the organizing principle: every feature lands on all
three OSes where the platform allows, and is explicitly gated (and documented as
unsupported) where it doesn't.

Checks: `npm run build` runs `tsc` (strict) and is the frontend type-check; `npm test` runs the Vitest frontend regression suite; `npm run lint` runs ESLint (react-hooks rules only); `cargo test` in `src-tauri/`
runs the Rust unit tests; `cargo clippy --all-targets -- -D warnings` must stay
clean. The release workflow builds signed packages after every push to `main`;
see `RELEASING.md`. Clippy lints are platform-gated, so a clean local run only
proves the current OS: treat cross-OS clippy as unverified until the code is
pulled on the other machines.

## Development commands

```bash
bun install                          # install JS deps — bun.lock is the source of truth
                                     # (run this after pulling or tsc fails; also
                                     # installs the git hooks via the prepare script)
npm run tauri dev                    # run the app in dev mode (vite + cargo)
npm run tauri build -- --no-bundle   # release build (on Linux: source ~/.cargo/env first)
                                     # NEVER `cargo build --release` directly: without the tauri
                                     # CLI the binary is dev-mode and loads localhost:5173
npm run build                        # tsc + vite build (frontend only; use as the type-check)
npm test                             # Vitest frontend regression suite
npm run release                      # cross-platform release build + copy artifact to bin/
                                     #   Windows: commandeer.exe
                                     #   Linux:   commandeer binary
                                     #   macOS:   commandeer.app bundle
npm run format                       # prettier --write . + cargo fmt (whole repo)
npm run format:check                 # prettier --check . + cargo fmt --check (CI-equivalent)
```

Release builds use Tauri's signed updater. The private signing key lives only in
the `TAURI_SIGNING_PRIVATE_KEY` GitHub Actions secret; never commit or rotate it
without an explicit migration plan because installed copies embed its public key.
All Tauri builds run through `tauri.cjs`, which resolves the build version from
`RELEASE_VERSION` in CI, an exact numeric Git tag locally, then `package.json` as
a development fallback. Raw Cargo build-directory executables never auto-update;
only installed packages do.

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
- Settings → Permissions & Diagnostics reports Screen Recording and Accessibility status without prompting, opens the corresponding Privacy & Security panes, and exposes screenshot/Alt-drag verification aids.
- Shutdown/Restart/Logout/Empty Trash trigger a one-time **Automation** prompt on first use (System Events / Finder). `@search` over the focused Finder folder uses the same Finder Automation channel (and only queries Finder when the palette opened over it; otherwise it falls back to the home folder like Linux).
- Clipboard history is encrypted at rest on all three platforms: DPAPI on Windows; ChaCha20-Poly1305 on Linux (key in the Secret Service, 0600 key-file fallback) and macOS (0600 key file next to the db). **Do not move the macOS key to the Keychain** while the app ships ad-hoc-signed: Keychain ACLs bind to the code signature, so every rebuild re-prompts — and the prompt fires during setup and blocks launch (verified on-device).
- AI-assistant usage panels (`commands/codex.rs` for Codex, `commands/claude.rs` for Claude) read each assistant's OAuth token to show rate-limit usage. On **macOS** those tokens live in the login **Keychain** (generic passwords, services `Codex-credentials` / `Claude Code-credentials`), not the `~/.codex/.credentials.json` / `~/.claude/.credentials.json` files (which only exist on Linux/Windows) — so macOS reads them from the Keychain via the `security` CLI, with the file as a fallback. **Read external Keychain items via `/usr/bin/security`, not an in-process crate** (`keyring`/`security-framework`): the ACL is keyed on the _calling_ binary, so a stable Apple binary earns a one-time "Always Allow", whereas the ad-hoc-signed app would re-prompt every rebuild — the same signature/ACL gotcha as the clipboard key above, but the reason it's fine to read here.
- The palette window joins all Spaces (`canJoinAllSpaces | fullScreenAuxiliary`), so toggling it never switches Spaces and it appears over fullscreen apps.
- App icons: `.app` bundles are directories, so both icon caches (Rust `icons.rs`, frontend `ResultRow`) key them **per path**, never on the shared folder/extension slot — regressing this makes every app render as the first-resolved app's icon. Icons are downscaled to ≤128px before base64 (a raw `iconForFile:` TIFF is a 1024×1024, ~2 MB payload). `iconForFile:` costs ~175 ms/icon **cold**, so the macOS icon cache is **persisted to disk** (`<app-cache>/icon-cache-v1.json`, keyed by path + mtime; a background flusher thread writes it every 3 s when dirty) and a **gentle sequential background warm** at startup (`lib.rs` setup → `icons::warm_app_icons`) resolves every installed app once. After the first run every icon loads from disk, so the Apps folder paints real icons immediately. **Do not** eagerly resolve the whole app list from the frontend per-launch — that re-pays the cold cost every time and queues the visible rows behind the entire install list (the reason the disk cache exists).
- `npm run release` produces a signed/unsigned `bin/commandeer.app` bundle; right-click → Open the first time if unsigned.

## Code style & formatting

Consistency is enforced by tooling, not by hand. The repo is kept
prettier-clean and `cargo fmt`-clean; a pre-commit hook formats staged files so
every contributor's output is identical regardless of OS or editor.

**Frontend (TS/TSX/JS/JSON/CSS/Markdown)** — Prettier, configured in
`.prettierrc.json`: single quotes, no semicolons, trailing commas, `printWidth`
120, `arrowParens: avoid`, `endOfLine: lf`. (120 matches the codebase's existing
long-line style; JSX inline-style objects and SVG path strings are left wide on
purpose.) Run `npm run format:frontend` to format, `npm run format:check` to
verify.

**Rust** — `rustfmt` with stable options (`src-tauri/rustfmt.toml`: `edition =
"2021"`, `max_width = 100`). `cargo fmt` and the per-file `rustfmt --edition 2021`
used by the hook produce identical output. `cargo clippy --all-targets
-- -D warnings` must stay clean (platform-gated — see Checks above).

**Pre-commit hook** — Husky + lint-staged (`.husky/pre-commit`). On install,
the `prepare` script runs `husky`, which points `core.hooksPath` at `.husky/`.
The hook runs lint-staged (Prettier on staged TS/JSON/CSS/MD, `rustfmt` on staged
`.rs`) and then `.agents/hooks/check-agent-sync.mjs`. It only **formats** — it
does not run `tsc`/`clippy`/`cargo test` (those stay manual; the ship-change
rebuild catches build errors). If you skip hooks with `--no-verify`, run
`npm run format:check` before pushing.

**Line endings** — `.gitattributes` forces LF (`* text=auto eol=lf`); binary
assets (`*.png *.ico *.icns`) are marked binary. The stored blobs are already LF;
editors and Prettier/rustfmt all write LF, so line endings never show up as diffs.

**TypeScript** is strict (`tsconfig.json`: `strict`, `noUnusedLocals`,
`noUnusedParameters`, `noFallthroughCasesInSwitch`). Don't add `any` where a
type is knowable. Frontend platform branches use `IS_LINUX` / `IS_MAC` (user-agent
flags), never bare negations.

**Rust** platform code is behind `#[cfg(target_os = "windows")]` /
`#[cfg(target_os = "linux")]` / `#[cfg(target_os = "macos")]`. Never use a bare
`#[cfg(not(windows))]` branch for Linux/macOS-specific code — gate each OS
explicitly (or `unix` only when the code is genuinely identical, like
`PermissionsExt`).

## Editor setup

Shared editor configs are committed so format-on-save matches the hook exactly.
Install the recommended extensions and no further setup is needed.

- **VS Code** — `.vscode/extensions.json` recommends Prettier, rust-analyzer,
  the Tauri extension, and ESLint. `.vscode/settings.json` enables
  format-on-save, sets Prettier as the default formatter (rust-analyzer for
  Rust), `files.eol` to `\n`, and tab size 2 (4 for Rust/TOML).
- **Zed** — `.zed/settings.json` enables format-on-save, LF, 2-space indent
  (4 for Rust/TOML), Rust formatting via rust-analyzer, and Prettier for
  TS/TSX/JS (install the Prettier extension when prompted).
- **Other editors** — `.editorconfig` provides the same baseline (LF, UTF-8,
  final newline, no trailing whitespace, 2-space — 4 for `*.rs`/`*.toml`).

## Shipping changes

After **every** completed task, bug fix, or feature — once the work is done and
verified — ship it: **(1) commit and push, (2) rebuild the release binary,
(3) restart the running process** on the new binary. The running app should
always reflect committed code. Use the `ship-change` skill
(`.agents/skills/ship-change/SKILL.md`), which encodes the exact per-OS steps
(Windows/macOS/Linux):

- Commit with the repo's footer lines and `git push` (this repo ships from `main`).
- Rebuild with `npm run tauri build -- --no-bundle` (Linux/macOS: `source ~/.cargo/env` first; Windows: `npm run release`) — only a release build is representative.
- Restart: kill the old process, then relaunch — `pkill -x commandeer` + `./src-tauri/target/release/commandeer` on Linux/macOS, `Stop-Process -Name commandeer` + the built exe on Windows. Kill before launching, since launching alone just toggles the palette (single-instance plugin).

This is also enforced by a **Stop hook** (`.agents/hooks/ship-reminder.mjs`,
wired in both `.claude/settings.json` and `.codex/hooks.json`): when a turn ends
with uncommitted changes it blocks once and asks the model to decide whether the
work is a complete feature/fix and ship it — it never auto-commits, and it stays
silent on a clean tree. The hook is Node (shell-neutral) so it runs identically on
all three OSes. If any step fails (build error, rejected push), stop and surface it
rather than reporting the change as shipped.

## Agent integration (skills + hooks)

Agent config is **normalized to work across both Claude Code and Codex** from a
single canonical home — there are no divergent per-tool copies.

```
.agents/                          # canonical, shared across tools
  skills/ship-change/SKILL.md     # the ship-change skill (tool-neutral)
  hooks/ship-reminder.mjs         # the Stop hook (tool-neutral)
  hooks/check-agent-sync.mjs      # pre-commit integrity check
.claude/
  settings.json                   # Claude Code: Stop hook -> .agents/hooks/ship-reminder.mjs
  skills/ship-change/SKILL.md     # mirror of the canonical skill, for Claude Code discovery
.codex/
  hooks.json                      # Codex: Stop hook -> .agents/hooks/ship-reminder.mjs
```

- `.agents/` is the source of truth. **Edit skills/hooks there.**
- Claude Code discovers skills from `.claude/skills/`, so the ship-change
  `SKILL.md` is mirrored there byte-for-byte. The pre-commit `check-agent-sync`
  script fails the commit if the mirror drifts from the canonical copy, if
  `CLAUDE.md` stops redirecting to `AGENTS.md`, or if either tool's config stops
  pointing at the shared hook.
- `.codex/hooks.json` uses a **relative** path (never an absolute machine path).
- Only `.claude/settings.local.json` and `.codex/settings.local.json` are
  gitignored (personal overrides); everything else under `.agents/`,
  `.claude/`, and `.codex/` is committed and shared.

## Architecture

Two always-running Tauri windows that hide/show rather than launching per use: the palette (label `palette`, transparent, undecorated) and the screenshot overlay (label `screenshot`, opaque fullscreen). A tray icon (cross-platform; non-fatal if it can't be created on Linux) and the global hotkey / single-instance toggle are the entry points.

### Screenshot tool

Lightshot-style region capture: trigger → Rust freezes the screen to `<app-cache>/frame.png` (on Linux a four-CLI fallback chain — `cosmic-screenshot` → `gnome-screenshot` → `spectacle` → `grim`, first one present wins; on Windows a GDI BitBlt of the full virtual screen — all monitors — with the overlay spanning the same bounds; on macOS `screencapture -R` of the cursor monitor) → the `screenshot` window (shared HTML entry, but `main.tsx` lazy-loads a separate `ScreenshotOverlay.tsx` chunk by window label) shows the frame under a dim veil → drag a region → an **annotate stage** (Lightshot-style toolbar): further drags paint freehand red marker strokes to circle things, Ctrl+Z/Backspace undoes a stroke, holding **Alt** shows a color-pick tooltip (hex of the raw frame pixel under the cursor, sampled via the `pick_frame_color` command, which lazily decodes and caches the frame in `ScreenshotState` — the veil/strokes never bleed in) and **Alt+click** copies that hex to the clipboard instead of the image and finishes (the annotated crop is still saved to disk), Enter or the ✓ button finishes → Rust crops, burns in the annotations (anti-aliased round-capped polylines via a max-coverage capsule-SDF buffer in `draw_stroke_annotation` — max, not per-segment blending, so overlapping joints don't seam), saves to `~/Pictures/Screenshots`, and copies PNG to the clipboard (`wl-copy` on Linux, arboard on Windows/macOS). Esc cancels. Backend in `commands/screenshot.rs`. Triggers: `commandeer://screenshot` deep link (bound to PrtScn via a second managed COSMIC shortcut line on Linux), a configurable global shortcut on Windows/macOS (`screenshot_hotkey` config, **default `Insert` on Windows**, editable via Settings → Screenshot Hotkey), and a Tools → Take Screenshot palette command. **Do not default the Windows shortcut to PrintScreen**: `RegisterHotKey(VK_SNAPSHOT)` returns success but never fires `WM_HOTKEY` because PrtScn emits no `WM_KEYDOWN` — so it silently does nothing. Any ordinary key (Insert, Fn keys, letters+modifiers) works. macOS has no default screenshot hotkey because Mac keyboards lack PrintScreen and the common system shortcuts are `Cmd+Shift+3/4/5`. The frame is encoded as fast/unfiltered PNG (transient file, reloaded once then deleted) — ~50 ms capture on a 2560×1440 release build; the unoptimized dev build is ~15× slower, so judge screenshot latency only from a release build. On Windows the overlay appears via a cloak-then-reveal handshake: at capture time the window is positioned/sized and shown **DWM-cloaked** (composited but not displayed — WebView2 only renders while visible), the frame `<img>` loads and rasterizes off-screen, and `reveal_screenshot_overlay` uncloaks only when the webview reports the image was actually **presented**, via an **Element Timing** observer (`elementtiming="shot-frame"`). onLoad/double-rAF are NOT sufficient signals — they race the GPU raster of the multi-monitor-sized texture and flashed black. Fallbacks: a 500 ms frontend timer post-onload and a 1500 ms Rust-side force-show+uncloak; all commands are idempotent. On Linux the overlay is a 4-edge-anchored, exclusive-keyboard layer-shell surface; the frontend's `show_screenshot_overlay` call on img onload is the show path there (and on macOS) and the cloak machinery is Windows-only. Linux has its own stale-frame defense (there is no DWM cloak, and GTK3 toplevel opacity is a no-op on Wayland): WebKitGTK replays its **last composite** as the first frame when a hidden window is re-mapped, so the overlay window is **transparent on Linux** (`tauri.linux.conf.json` — platform configs replace the whole `app.windows` array, keep it in sync with `tauri.conf.json`) and the frontend always paints a cleared, fully transparent state (no frame, no veil) and waits a double-rAF (`afterClearPaint`) **before** any hide — finish, Esc-cancel, and the re-trigger path (Rust emits `screenshot-clear`; the webview clears then calls `hide_screenshot_overlay`; Rust force-hides after its pre-capture delay as fallback). The replayed composite is then invisible, the live desktop underneath is pixel-identical to the frozen frame, and the overlay's appearance reads as a single smooth veil dim. On Windows, both windows set `additionalBrowserArgs` with `CalculateNativeWinOcclusion` disabled (WebView2 browser args are process-wide — keep the two windows' args identical): without it, Chromium suspends rendering of hidden windows, the new frame never paints before `show()`, and the window flashes its stale surface (the previous capture) for a frame.

### Windows volume mixer

Windows has a dedicated application-session mixer page, separate from the
existing per-output-device `Set Volume` slider. Open it from the root footer or
with `Ctrl+M` (it is also searchable as `Volume Mixer`). The page lists every
non-expired Core Audio session on the default render endpoint, refreshes while
open, and keeps paused/inactive sessions visible like the system mixer. Up/Down
selects an app, Left/Right changes its session volume (Shift changes by 10%), and
Space/Enter toggles mute; all apps stay visible on the same page. The backend is
in `commands/audio.rs` (`IAudioSessionManager2` / `ISimpleAudioVolume`) and is
Windows-gated; the frontend view is `components/VolumeMixer.tsx`.

### Per-monitor Alt+Tab

Windows can replace the system Alt+Tab UI with a native switcher scoped to the monitor under the cursor (`per_monitor_alt_tab`; Settings → Per-Monitor Alt+Tab). It lists every eligible window on that monitor plus only the topmost eligible maximized window from each other monitor, preserving global Z-order. Remote candidates must be genuinely maximized (`IsZoomed`) and not minimized (`!IsIconic`); snapped, manually stretched, minimized, and borderless-fullscreen windows do not qualify. The overlay itself stays on the cursor monitor, and choosing a remote candidate focuses it without moving the window or cursor.

While the feature is enabled, its existing `WH_KEYBOARD_LL` service also owns fixed top-row shortcuts `Ctrl+Alt+1` and `Ctrl+Alt+2`. They focus the first eligible maximized, non-minimized window in Z-order on Windows `DISPLAY1` or `DISPLAY2`, respectively; the chords disappear when the feature is disabled. Window enumeration and activation are posted to the dedicated overlay thread—never perform them inside the low-level hook callback.

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

Frontend lifecycle invariants:

- **Escape and dismissal have one state machine.** Palette handles Escape from most-specific to least-specific state: cancel a pending confirm, close the action panel, pop one step, then dismiss at root. Every external dismissal path (focus loss, toggle, tray, etc.) must settle a pending confirm with `false` and clear transient feedback before hiding. Never leave a confirm promise or destructive callback alive across palette sessions.
- **Async palette loads are sequenced.** `@find`, active-folder `@search`, normal step `load()`, and explicit step reloads may overlap. Only the newest request for the still-current mode/step may update items, errors, or loading state; abandoning a mode must clear its loading state. Preserve this guard when adding a load path.
- **Selection is always valid for the current items.** Item replacement/reload clamps the selected index, and pointer hover sets an absolute selection rather than applying a delta derived from stale state. Enter must never target a different row from the rendered highlight.
- **Promise-based listeners must clean up after late registration.** Tauri `listen()`/`onFocusChanged()` registration can resolve after React effect cleanup (including the intentional StrictMode mount cycle). Cleanup must unsubscribe even in that ordering, using a disposed flag or by chaining unlisten from the registration promise.
- **First-run onboarding waits for real focus.** Never push it while the hidden Accessory window mounts. Existing installations are inferred from legacy localStorage keys and must not be interrupted; the Commandeer Guide remains searchable for everyone.
- **Slider side effects and persistence are separate.** Apply visual/audio feedback on every tick, but serialize/debounce whole-config persistence and flush the trailing value when the step exits. Do not allow older `writeConfig` calls to finish after and overwrite newer values.
- **Screenshot completion is transactional.** A failed finish must reset the finishing guard and surface an error instead of looking successful. Cancel only while a frame/capture is active so a delayed Escape cannot cancel the next capture.
- **Onix compactness is session-scoped.** It may open as a compact capsule only at a clean root. Once any query, navigation, loading, error, confirmation, action, or feedback state expands it, keep it expanded until the whole-session reset/dismiss path. On macOS the expansion animates downward from a fixed top edge for 150 ms and resize events keep the native glass radius synchronized; Reduced Motion and the other platforms use direct native sizing.

User-facing "commands" also come from a scripts directory on disk (configurable `scripts_dir`; `.ps1`/`.lnk` on Windows, `.sh`/`.desktop`/`.AppImage`/executables on Linux, `.sh`/`.command`/executables on macOS), scanned by the Rust side.

### Backend (`src-tauri/src/`)

`lib.rs` holds setup (window creation, global shortcut, tray, single-instance, deep links) plus window show/hide/positioning. One module per feature in `commands/` (audio, clipboard, file_index, launcher, process, stats, …), all registered in the `invoke_handler` in `lib.rs`. The file index is self-hosted: SQLite + FTS5 (trigram) fed by a `notify` filesystem watcher.

### Platform split

All OS-specific code is behind `#[cfg(target_os = "windows")]` / `#[cfg(target_os = "linux")]` / `#[cfg(target_os = "macos")]` in Rust and `IS_LINUX` / `IS_MAC` (user-agent) checks in the frontend. Never use a bare `#[cfg(not(windows))]` branch for Linux/macOS-specific code — gate each OS explicitly (or `unix` only when the code is genuinely identical, like `PermissionsExt`). The three platforms differ most in:

- **Window sizing/positioning.** Windows: frontend `setSize` + min/max + cursor-monitor positioning; Onix clips the Acrylic host with a DPI-aware compact/panel region. Linux/Wayland: cosmic-comp ignores client resizes/moves of mapped toplevels, so the palette is rendered as a **wlr-layer-shell overlay** (gtk-layer-shell, set up in `lib.rs`), anchored to the top edge with a fixed margin; the frontend measures content height and calls the `resize_palette` Rust command, which changes the GTK size request to resize in place without flicker. `html,body,#root` are content-height on purpose so this measurement works, and Onix uses a modeled optical fallback because Wayland has no portable backdrop-sampling API. macOS: a normal always-on-top transparent Accessory window positioned via Tauri monitor APIs; macOS 26 Onix wraps the web content in a runtime-gated `NSGlassEffectView`, while Default and older systems use `HudWindow` vibrancy. Onix expansion uses a short top-fixed AppKit frame animation and refreshes its radius on each native resize event.
- **Launching & icons.** Windows uses PowerShell/shell32 (`.lnk` icon extraction); Linux parses `.desktop` files and launches via direct exec / `sh` / `gio launch` / `xdg-open`; macOS scans `.app` bundles and launches via `open`. File-search icons use the same shell APIs (`SHGetFileInfoW` on Windows, `NSWorkspace.iconForFile:` on macOS).
- **Global hotkey.** See Linux/macOS notes above; `set_game_mode` in `lib.rs` rewrites the COSMIC custom-shortcut config on Linux and switches the registered base hotkey everywhere.

Config is JSON read/written by the Rust side (`commands/config.rs`; `scripts_dir` defaults per-platform). Lightweight UI prefs (game mode, widget visibility, script cache) live in webview `localStorage`.

Themes normally own the color system. Onix is the intentional exception: its neutral “Black Water” material and foreground palette are style-owned, while the active theme still supplies the accent color.

## Keeping this document current

Update `AGENTS.md` when repository-wide rules, supported platforms, build
commands, release mechanics, lifecycle invariants, or agent integration change.
Keep feature-specific explanations in [`docs/`](docs/README.md), and update the
relevant docs page alongside the code change. After editing the canonical agent
skill or hook files, run `.agents/hooks/check-agent-sync.mjs` and keep mirrored
files byte-identical.
