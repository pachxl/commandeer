# Commandeer → macOS port plan

Status: **Phases 1–4 complete** — Commandeer runs on macOS with UX parity
(vibrancy, tray, hotkey, positioning), native feature backends (launcher,
screenshot, paste, audio, system actions, processes), packaging (`.app`
bundle, `.icns`, `CFBundleURLTypes`), sync hygiene (cross-platform `release`
script), and file-search / system-stats parity. This document is the port plan; the macOS notes in
`CLAUDE.md` are the up-to-date reference for current platform behavior and
permission handling. Commandeer targets **Windows**, **Linux (Wayland/COSMIC)**,
and **macOS** from the same codebase.

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

## Phase 2 — Core UX parity — DONE

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
- [x] **`set_window_transparency`** — implemented natively on macOS: sets
      `NSWindow.alphaValue` (objc2 `msg_send` on the main thread), the analogue of
      the Windows `LWA_ALPHA` path, for genuine see-through. (An initial
      CSS-opacity fallback was wrong here — it faded the webview onto the opaque
      vibrancy layer, reading as a blurred-wallpaper patch rather than
      transparency.) Only Linux keeps the CSS-opacity path.
- [x] **On-device fixes** (found testing Phase 2): rounded palette corners
      (CSS `border-radius` on macOS/Linux; Windows rounds via DWM) and a
      long-list scrolling bug (WKWebView recenters on
      `scrollIntoView({block:'nearest'})` — replaced with deterministic
      `scrollTop` math that behaves the same on every engine).
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

## Phase 3 — Feature backends — DONE

All backends implemented in one pass. `cargo build` clean; `cargo test` green,
including new macOS smoke tests (launcher scan, osascript volume round-trip,
process enumeration); app relaunched and palette toggled with the new
capture-foreground path. What could not be scripted is the permission-gated
interactive verification (see below).

- [x] **App launcher** (`launcher.rs`) — scan `~/Applications`,
      `/Applications`, `/System/Applications` (depth 3, never descending into
      a bundle; user-installed apps shadow system ones by name). Launch via
      `open`, which goes through LaunchServices and activates an already-
      running instance instead of starting a second copy.
- [x] **Screenshot capture** (`screenshot.rs`) — `screencapture -x -t png
      -R<rect>` of the cursor monitor; rect computed from Tauri's monitor APIs
      (physical ÷ scale = points; verified the PNG comes back at native 2×
      Retina pixels, which is what the overlay's physical sizing expects).
      Overlay positioning now shared with Windows (`monitor_origin`); in
      `.setup()` the overlay NSWindow is raised to the screen-saver level
      (1000) so it can cover the menu-bar strip (normal-level frames get
      clamped below it by constrainFrameRect, which would misalign the
      region→pixel mapping) and joins all Spaces/fullscreen. Click-away
      cancel enabled on macOS (was Windows-only).
- [x] **Screenshot → clipboard** — arboard image offer, same arm as Windows;
      only Linux keeps the `wl-copy` shell-out.
- [x] **Paste to previous** (`paste.rs`) — frontmost app pid captured via
      NSWorkspace when the palette shows; paste = clipboard → reactivate that
      app (NSRunningApplication, main thread) → 150 ms beat → ⌘V posted as
      CGEvents at the HID tap. Checks `AXIsProcessTrusted` first and returns a
      pointed "grant Accessibility" error instead of silently doing nothing.
- [x] **Audio volume** (`audio.rs`) — osascript `set volume` / `get volume
      settings`. AppleScript only addresses the default output, so macOS
      exposes one pseudo-device ("System Output"); per-device control would
      need CoreAudio proper (follow-up if ever wanted).
- [x] **System actions** (`system.rs`) — Lock/Sleep via `pmset`
      (`displaysleepnow` / `sleepnow`); Shutdown/Restart/Logout via System
      Events AppleScript (graceful — apps get to save; one-time Automation
      prompt); Empty Trash via Finder AppleScript. Hibernate returns a "not a
      macOS concept" error.
- [x] **Process list/kill** (`process.rs`) — `sysinfo` (macOS-only dep) for
      the list, `libc::kill(SIGKILL)` to match TerminateProcess semantics.
      The Windows/Linux arms were deliberately left untouched rather than
      unified.
- [x] **File/app icons** (`icons.rs`) — deferred as planned; `path_icon`
      returns `None` on macOS like Linux. `NSWorkspace.icon(forFile:)` is the
      follow-up.
- [x] **Script types** (`fs.rs`) — `.command` files surfaced and launched via
      `open` (opens Terminal, like double-clicking; checked before the
      executable-bit test so they don't run invisibly). Generic fallback
      opener is `open` instead of `xdg-open` on macOS; `.desktop`/`gio` gated
      to Linux. `scripts_dir` default needed no change (exe walk-up +
      `~/commandeer/commands` are portable).
- [x] **Verified `clipboard/crypto.rs`** — the `not(windows)` arm is a plain
      passthrough placeholder, not Linux-keyring-specific; compiles and runs
      on macOS as-is. macOS Keychain encryption is a possible follow-up.
- [x] **nspanel decision (deferred from Phase 2b) — not adopted.**
      Paste-to-previous works without it: the previous app is explicitly
      reactivated before ⌘V, so not-stealing-focus isn't required. Skipping
      it avoids the git-only, branch-pinned dependency. Revisit only if
      focus-return proves flaky in practice or show-over-fullscreen-apps
      becomes a priority for the palette itself.

**On-device verification still owed (permissions can't be scripted):**
- Screenshot: needs **Screen Recording** for the frozen frame to include other
  apps' windows (without it macOS silently captures just the wallpaper). In
  dev the permission attaches to the invoking terminal/IDE; for a bundled
  build, to the app.
- Paste: needs **Accessibility**; until granted the command errors with
  instructions rather than no-opping.
- Shutdown/Restart/Logout/Empty Trash: one-time **Automation** prompts on
  first use (System Events / Finder).
- @search over a Finder folder: the same Finder **Automation** prompt on
  first use. Verified headlessly that the palette stays responsive while the
  prompt is pending (the osascript runs on a worker thread and @search falls
  back to the home folder until it resolves); the actual folder pick needs
  the grant.

## Phase 4 — Packaging & sync hygiene — DONE

- [x] **`package.json` `release` script** — replaced the Windows-only one-liner
      with `scripts/release.js`, which builds the right artifact per platform:
      `.exe` on Windows, raw binary on Linux, `.app` bundle on macOS.
- [x] **`tauri.conf.json` `bundle`** — added `icons/icon.icns` and
      `bundle.macOS.info.CFBundleURLTypes` for `commandeer://` deep links.
- [x] **Deep link** — `CFBundleURLTypes` is now in the generated Info.plist;
      runtime `register()` covers dev/unbundled runs.
- [x] **Update `CLAUDE.md`** — added macOS to the project description, commands,
      dev notes, architecture, screenshot, and platform-split sections.
- [x] **CI** — added `.github/workflows/ci.yml` with a matrix of
      `ubuntu-latest` / `windows-latest` / `macos-latest` running the frontend
      type-check and Rust build + tests.
- [x] **macOS file-search icons** — implemented `NSWorkspace.iconForFile:` in
      `commands/icons.rs`; `path_icon` and file-search results now resolve icons
      on macOS.
- [x] **macOS system stats** — implemented CPU + memory in `commands/stats.rs`
      using `sysinfo` (already a dependency for process enumeration); GPU is
      intentionally absent because no reliable unprivileged cross-vendor metric
      exists on macOS.
- [x] **macOS screenshot hotkey** — Tauri's global shortcut is now registered on
      macOS when the user configures one (no default, to avoid conflicts with
      system shortcuts); the settings UI is shown on macOS as well.
- [x] **Platform-gate cleanup** — `commands/fs.rs` now uses explicit
      `target_os = "linux"` / `target_os = "macos"` gates instead of bare
      `not(windows)` branches, matching the port rule.

## Post-port parity pass (2026-07-04)

Gaps found comparing the finished port against the Windows/Linux arms after
the Linux-parity merge landed:

- [x] **@search on macOS** — was broken (`capture_location` was Windows-only,
      the home-folder fallback Linux-only, so macOS threw "No File Explorer
      folder is focused"). Now: when the palette opens over Finder, the front
      Finder window's folder is resolved via AppleScript (same Automation
      channel as Empty Trash; only queried when Finder was frontmost so the
      one-time prompt can't fire over unrelated apps); otherwise @search falls
      back to the home folder like Linux. Wording is Finder-specific on mac.
- [x] **Clipboard history encryption at rest** — was plaintext on macOS while
      Windows (DPAPI) and Linux (ChaCha20 + Secret Service) encrypted. The
      Linux ChaCha20-Poly1305 arm now covers macOS, with the key in a 0600
      key file (`~/Library/Application Support/dev.commandeer/
      clipboard.key`) and the same one-time plaintext re-encryption migration.
      Round-trip + legacy-passthrough tests added. **Keychain deliberately not
      used**: its ACLs bind to the code signature, and an ad-hoc-signed binary
      gets a new signature every rebuild, so each rebuild re-prompted — with
      the prompt firing inside setup and blocking launch entirely (observed
      on-device via a process sample stuck in SecKeychainFindGenericPassword).
      Revisit only with a stable Developer ID signature.
- [x] **System folder fixes** — "Empty Recycle Bin" is now "Empty Trash" on
      macOS, and Hibernate (which the backend rejects as not a macOS concept)
      is no longer listed.
- [x] **Menu-bar template icon** — the tray now uses a monochrome `>_` glyph
      (`icons/tray-template.png`, 18pt @2x, alpha-only) with
      `icon_as_template`, so macOS tints it correctly for light/dark menu
      bars; Windows/Linux keep the colored app icon. (Closes the Phase 2
      follow-up note.)

## Polish pass 2 (2026-07-04, later the same day)

- [x] **App icons were all Activity Monitor** — `.app` bundles are
      *directories*, so `icons.rs`'s macOS cache keyed every app on the shared
      folder slot (first app resolved won), and the frontend `ResultRow` cache
      keyed them on the `app` "extension" — same collision one layer up. Both
      now key `.app` (and extensionless binaries) per path. Regression test
      (`distinct_app_icons_and_small_payloads`) pins two stock apps to two
      different icons.
- [x] **Icons appeared late** — `iconForFile:` images expose one huge
      asset-catalog rep; the TIFF round trip produced a 1024×1024 PNG (~2 MB
      of base64) per icon. Now the smallest bitmap rep ≥ 36 px is encoded
      directly when present, and anything still over 128 px is thumbnailed to
      64 px with the `image` crate (rows draw at 18 pt). Same test asserts the
      payload stays small. The frontend also keeps a synchronous
      resolved-icon cache so re-ranked rows paint the real icon immediately
      instead of flashing the generic glyph each keystroke.
- [x] **Palette joins all Spaces / fullscreen** — the palette NSWindow now
      gets `canJoinAllSpaces | fullScreenAuxiliary` like the screenshot
      overlay, so toggling it from another Space opens it in place instead of
      yanking the user to the Space it last lived on. (This was the
      documented lightweight alternative to the nspanel plugin.)
- [x] **Reveal in file manager (all three platforms)** — new `reveal_path`
      command (`open -R` / `explorer /select,` / FileManager1 D-Bus with
      xdg-open fallback) surfaced as a Ctrl+K "Reveal in Finder/File
      Explorer/File Manager" action on file rows.
- [x] **System Stats panel** — the GPU cell is hidden on macOS (the backend
      intentionally has no GPU metric there); the settings sublabel matches.
- [x] **Clipboard poll** — each 500 ms tick now checks
      `NSPasteboard.changeCount` first and skips the arboard read when
      nothing changed; Linux keeps the plain poll.

## Effort estimate

- **Phase 1 (compiles + launches):** small — a focused session. Answers "is it
  possible?" → yes.
- **Phases 2–3:** incremental, feature-by-feature; usable after Phase 2 even with
  backends stubbed.
- **Riskiest:** macOS permissions (paste/screenshot) and NSPanel behavior;
  everything else is conventional.
