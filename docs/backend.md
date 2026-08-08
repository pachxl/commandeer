# Rust backend and Tauri IPC

The Rust side is a collection of focused command modules composed by
`src-tauri/src/lib.rs`. The module boundary is both an ownership boundary and a
platform-gating boundary.

## Registration is the API

`lib.rs` registers every callable function in `tauri::generate_handler!`. A
function in `commands/*.rs` is not available to the frontend until it appears
there, and a frontend wrapper is not complete until it calls the exact command
name and serialized shape. Treat the handler list and `src/lib/tauri.ts` as a
pair during changes.

The backend is also responsible for setup that is not request/response IPC:
tray and single-instance handling, global/deep-link shortcuts, window
positioning, file-index and clipboard monitors, icon cache warmup, updater
startup, and reapplying persisted native settings.

## Module ownership

| Area                         | Module(s)                                                                                       | Responsibilities                                                               |
| ---------------------------- | ----------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------ |
| Composition and windows      | `lib.rs`, `commands/window.rs`, `commands/palette_surface.rs`                                   | Setup, show/hide, positioning, transparency, native palette material/geometry  |
| Configuration                | `commands/config.rs`, `commands/shortcuts.rs`                                                   | JSON config, migrations, hotkey parsing/registration, overrides                |
| Launching and scripts        | `commands/launcher.rs`, `commands/fs.rs`                                                        | Installed apps, running paths, script discovery, metadata, process launch      |
| Search                       | `commands/file_index.rs`, `commands/search.rs`, `commands/explorer.rs`, `commands/bookmarks.rs` | FTS5 index, fallback search, active-folder traversal, browser bookmarks        |
| User data                    | `commands/store.rs`                                                                             | Notes, Quick Links, themes, per-command overrides                              |
| Clipboard                    | `commands/clipboard.rs`, `commands/clipboard/{db,crypto}.rs`, `commands/paste.rs`               | Monitor, encrypted history, copy, paste to previous foreground app             |
| Media and system             | `commands/audio.rs`, `system.rs`, `appearance.rs`, `process.rs`, `stats.rs`, `rates.rs`         | Audio, power/session actions, process list, resource panels, FX rates          |
| Visuals                      | `commands/icons.rs`, `commands/screenshot.rs`                                                   | Shell/app icons, persistent icon cache, capture/annotation pipeline            |
| Platform integrations        | `commands/window_drag.rs`, `alt_tab.rs`, `linux_shortcuts.rs`, `deeplink.rs`                    | Window management, Windows switcher, Wayland/compositor shortcuts, URI routing |
| Updates and assistant panels | `commands/updater.rs`, `codex.rs`, `claude.rs`                                                  | Signed updates and rate-limit usage APIs                                       |

The complete file-level ownership list is in [`source-map.md`](source-map.md).

## Platform code rules

Use explicit `#[cfg(target_os = "windows")]`, `linux`, and `macos` gates. Do
not use a bare `not(windows)` branch when Linux and macOS have different
behavior. Frontend platform visibility uses `IS_LINUX`, `IS_MAC`, and the
corresponding Windows fallback.

Native APIs that may block, enumerate, or shell out should run on a blocking
thread or a dedicated message-pump thread. Keep GUI-thread requirements in mind:
Windows power APIs need the GUI message-queue thread; macOS AppKit mutations
need the main thread; Windows low-level hooks must not do window positioning in
the hook callback. These constraints belong in module-level comments and in
the feature documentation.

## Native palette surface contract

`commands/palette_surface.rs` owns the palette's platform-native material and
outer geometry. The typed `set_palette_surface(style, expanded, scale, window)`
IPC applies a state change and remembers it. Setup calls
`configure_palette_surface` before the hidden palette is first mapped, and the
palette `WindowEvent::Resized` path calls `refresh_palette_surface` so the final
native radius matches the resized window. These operations must remain
idempotent because setup, frontend state, and resize notifications can converge
on the same presentation.

The platform implementations intentionally differ:

- On macOS 26, a runtime-gated `NSGlassEffectView` owns the existing Wry/WebKit
  content through its `contentView` property. A dedicated rounded `NSView`
  clipping container becomes the `NSWindow` content view and contains the
  glass, preventing its rectangular compositor host from leaking through the
  transparent window corners. Only that outer container is layer-backed; the
  glass itself remains under AppKit's native backing/sampling control. Onix
  uses adaptive Regular glass with no native tint, keeps the host `NSWindow`
  non-opaque/clear, and updates both public corner radii together while
  suppressing separate Core Animation actions on the clip and preserving the
  first responder. Removing Onix unwraps the hierarchy before
  applying the `NSVisualEffectMaterial::HudWindow` fallback used by Default and
  older macOS.
- On Windows, Tauri supplies Acrylic and the module owns only the DPI-aware
  `SetWindowRgn` shape: capsule for compact Onix, rounded panel for expanded
  Onix, and no custom region for Default.
- On Linux, the native function is deliberately a no-op. The transparent
  layer-shell host lets the frontend render the modeled optical fallback and
  alpha corners; Wayland exposes no portable live-backdrop/refraction API.

`commands/window.rs` exposes `resize_palette_window`. Its macOS branch runs on
the AppKit main thread and animates an Onix expansion for 150 ms while preserving
the window's top edge; ordinary resize notifications drive
`refresh_palette_surface`. Radius interpolation is armed only when the native
frame actually leaves the compact capsule. Later result-count/step resizes keep
the settled panel radius, so its top corners never pulse back toward the capsule
curve. The non-macOS and Reduced Motion paths resize directly. The
frontend keeps an expanded Onix surface expanded for the rest of the visible
session and returns to compact only through whole-session reset/dismiss.

## State and failure behavior

Managed state is registered during setup when it must outlive one IPC call:
`ScreenshotState`, `FileIndex`, and `ClipboardDb` are examples. Protect shared
mutable state with the narrowest synchronization needed, and make operations
idempotent when the frontend can issue a fallback call after a timeout.

Return `Result<T, String>` for user-visible failures. Preserve enough context for
the frontend toast to explain permission, missing-tool, path, or native-API
errors. A best-effort background service may log and continue, but a command that
the user explicitly invoked must not report success if its requested side effect
did not occur.

## Adding a backend command

1. Pick or create the owning module and add a module-level behavior summary.
2. Define serialized structs/enums with stable field names.
3. Add explicit platform implementations and an intentional unsupported error
   where required.
4. Register the command in `lib.rs`.
5. Add a typed wrapper in `src/lib/tauri.ts` and wire it to a provider/command.
6. Add unit tests for pure parsing/geometry/state logic and a manual checklist
   for OS APIs or permissions.
7. Update [`commands.md`](commands.md), [`platforms.md`](platforms.md), and
   [`source-map.md`](source-map.md) if the public surface changed.

## Keeping this document current

Update this page when module ownership, IPC registration, thread/GUI rules,
platform gating, managed state, or backend extension steps change. Verify the
command list against `src-tauri/src/lib.rs` and the implementation against the
module being described; update the command catalog whenever a registered command
is added, renamed, or removed.
