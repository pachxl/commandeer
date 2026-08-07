# Feature internals

This page records the behavior that is easiest to break because it crosses UI,
native APIs, background work, and platform-specific timing.

## Screenshot capture and annotation

The flow is transactional and is implemented jointly by
`src/components/ScreenshotOverlay.tsx`, `src/providers/screenshot.ts`, and
`src-tauri/src/commands/screenshot.rs`:

1. A palette command, global shortcut, deep link, or Linux desktop binding
   starts capture.
2. Rust clears/hides Commandeer, waits for the overlay to unmap, freezes the
   display, and stores a transient frame under the app cache.
3. The screenshot window is positioned over the captured monitor/virtual screen
   and receives the frame path. Windows shows it DWM-cloaked until Element
   Timing reports that the frame was presented; Linux/macOS show after load.
4. The overlay first selects a region, then enters annotation mode. Marker
   strokes are held in frontend state; `Ctrl+Z`/Backspace removes the last one.
5. Alt samples the raw frame through `pick_frame_color`; it never samples the
   dim veil or marker layer. Alt-click copies the hex color and still saves the
   crop.
6. Finish crops the frozen frame, burns in strokes, saves a timestamped PNG in
   `Pictures/Screenshots`, and copies either the image or selected color. The
   temporary frame is removed only after all fallible work succeeds.

The Windows cloak/reveal fallback and Linux transparent-clear/double-rAF reset
paths are intentional. Do not simplify them to `onLoad + show`, because hidden
WebView2/WebKit surfaces can present a stale or black frame.

## Scripts and command metadata

`commands/fs.rs` scans the configured scripts directory and parses the first
8 KiB for Raycast/Vicinae-style directives. `src/commands/index.ts` turns those
records into commands, folders, confirmations, icons, and live-output rows.
The complete format is documented in [`scripts.md`](scripts.md).

## Global and active-folder search

`@find` uses the self-hosted SQLite FTS5 trigram index, then the Windows
Everything IPC protocol when available, then a walkdir fallback. The frontend
adds fuzzy matching, filename substring boosts, and junk-path down-ranking.
`file_index.rs` scans configured roots at startup and watches them for changes;
it skips hidden/build-heavy paths and caps scan depth.

`@search` captures the previously focused Explorer/Finder location before the
palette appears, lists the folder recursively once, then filters the returned
entries in the frontend. It is intentionally home-folder based on Linux and
when Finder is not the frontmost app.

## Clipboard history and paste

The Rust clipboard monitor records up to 100 distinct non-empty text entries,
deduplicated at the top. Windows prefers a native clipboard listener; Linux and
macOS poll, with macOS checking pasteboard `changeCount` before reading text.

History is stored in SQLite and encrypted at rest: Windows uses DPAPI; Linux
uses ChaCha20-Poly1305 with Secret Service first and a 0600 file fallback;
macOS uses a 0600 file because ad-hoc signatures make Keychain ACLs re-prompt on
every rebuild. Legacy plaintext rows migrate in place. Paste-to-previous
captures the foreground app before showing the palette and synthesizes/uses the
platform paste path; macOS needs Accessibility.

## Audio and system utilities

The Set Volume flow lists output devices and opens a live slider. Windows uses
Core Audio, Linux probes `wpctl` then `pactl`, and macOS controls the default
output with `osascript`. The Windows-only Volume Mixer keeps non-expired app
sessions visible and controls exact session ids. It owns its keyboard handling:
arrows select, Left/Right change volume, Shift changes by 10%, and Space/Enter
toggles mute.

System actions are direct native calls or standard session tools, with confirm
steps for restart, shutdown, logout, and emptying trash. Process listing and
system stats use platform APIs/files and must not make the UI depend on a single
vendor-specific tool.

## Window management and Alt+Tab

Windows Alt-drag uses a low-level mouse hook only to observe/swallow button
events; a separate mover polls the real cursor and calls `SetWindowPos`. It
supports edge-aware resize, clean tiled-edge propagation, an indicator overlay,
and move-only Aero-Snap. The hook must not swallow mouse-move events or perform
window positioning itself.

macOS Alt-drag uses `CGEventTap` and Accessibility AX APIs for move/resize/raise.
Linux Wayland is intentionally unsupported because a client cannot move another
client’s window; COSMIC provides the native gesture.

Windows Per-monitor Alt+Tab uses a native overlay and a keyboard hook. Remote
monitor candidates must be genuinely maximized and not minimized; enumeration
and activation run on the overlay thread, never in the hook callback.

## Icons, applications, and assistant panels

Installed app lists are cached in webview storage and refreshed with separate
installed/running TTLs. Icons resolve lazily for visible rows. macOS app icons
are persisted in `<app-cache>/icon-cache-v1.json` and warmed sequentially in the
background; do not move the whole-list resolution back into the frontend.

Codex and Claude usage panels read each tool’s OAuth data through the Rust
backend, cache the last successful result in localStorage, and back off polling
after rate limits. On macOS the backend reads Keychain items through the stable
`/usr/bin/security` process, with a file fallback.

## Automatic updates

Installed release packages wait 30 seconds, then check every six hours. The updater obeys
`auto_update` on every cycle, verifies the Tauri signature, installs a newer
SemVer release, and requests a restart. Debug builds and optimized binaries run
directly from a Cargo build directory do not update themselves.
Signing and release artifact details belong in [`../RELEASING.md`](../RELEASING.md).

## Keeping this document current

Update this page when a cross-layer feature changes its state machine, fallback
chain, timing handshake, persistence model, native API, permissions, keyboard
contract, or platform status. For screenshot changes also update the screenshot
sections in `AGENTS.md` and `platforms.md`; for window-drag changes update
`TODO.md` if verification debt changes.
