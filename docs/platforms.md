# Platform behavior and permissions

Commandeer targets Windows, Linux/Wayland, and macOS. Shared commands should
have a deliberate implementation or a deliberate unsupported result on each
OS. Never infer support from “not Windows”; Linux and macOS often need different
code and different permissions.

## Capability matrix

| Capability           | Windows                                                | Linux                                                             | macOS                                                     |
| -------------------- | ------------------------------------------------------ | ----------------------------------------------------------------- | --------------------------------------------------------- |
| Palette window       | Transparent undecorated native window                  | GTK layer-shell overlay for Wayland sizing/positioning            | Always-on-top transparent Accessory window with vibrancy  |
| Palette shortcut     | Global shortcut plugin                                 | Managed COSMIC/GNOME shortcut plus plugin on X11 where applicable | Global shortcut plugin; default `Cmd+Shift+Space`         |
| Screenshot           | GDI virtual-screen capture; DWM cloak/reveal handshake | CLI fallback chain; transparent stale-frame defense               | `screencapture -R` on cursor monitor                      |
| Output volume        | Core Audio endpoint APIs                               | `wpctl`, then `pactl`                                             | `osascript` default output                                |
| Application mixer    | Supported                                              | Unsupported                                                       | Unsupported                                               |
| Alt-drag windows     | Full move/resize/snap implementation                   | Unsupported by design under Wayland; compositor owns it           | Move/resize/raise with Accessibility; snapping not ported |
| Per-monitor Alt+Tab  | Supported native switcher                              | Unsupported                                                       | Unsupported                                               |
| Installed apps       | Shell AppsFolder and shortcut/icon APIs                | Desktop entries, AppImages, executables                           | `.app` bundles and `open`                                 |
| Active-folder search | Focused Explorer folder                                | Home-folder fallback                                              | Focused Finder folder or home fallback                    |
| Clipboard history    | DPAPI encryption and native listener                   | ChaCha20-Poly1305; poll monitor                                   | ChaCha20-Poly1305; poll plus pasteboard change count      |

## Permission matrix

| Permission / dependency              | When needed                                                           | Failure behavior                                                       |
| ------------------------------------ | --------------------------------------------------------------------- | ---------------------------------------------------------------------- |
| macOS Screen Recording               | Screenshot capture                                                    | Command fails with instructions; no silent empty image                 |
| macOS Accessibility                  | Paste-to-previous and Alt-drag                                        | Action reports the permission requirement                              |
| macOS Automation                     | System Events power/session actions, Finder-aware search, Empty Trash | One-time prompt on first use; denial returns an error                  |
| Linux `gtk-layer-shell`              | Wayland palette                                                       | Required at build/runtime for the layer-shell window path              |
| Linux `wpctl`/`pactl`                | Volume control                                                        | Backend probes once and reports missing control tools                  |
| Linux `wl-copy` or clipboard backend | Screenshot image copy                                                 | Capture can still be saved, but copy errors are surfaced               |
| Windows foreground/input APIs        | Alt-drag and Alt+Tab                                                  | Native hook/activation errors are surfaced or feature remains disabled |

## Important platform rules

- Linux config overlays replace the whole `app.windows` array. Keep
  `tauri.linux.conf.json` synchronized with `tauri.conf.json` when adding a
  window or changing a shared label.
- Windows WebView2 browser arguments are process-wide. Keep the palette and
  screenshot window `additionalBrowserArgs` identical, including the disabled
  native-occlusion feature required for hidden screenshot rendering.
- macOS app icons are cached by full bundle path, not extension/folder, and are
  downscaled before IPC. Do not eagerly resolve every icon from the frontend.
- Linux Wayland cannot let a client move another app’s window. The hidden Alt-drag
  setting is an intentional product decision, not an incomplete branch.
- Windows PrintScreen does not reliably produce a `WM_HOTKEY` event even when
  registration succeeds. Keep the screenshot default on an ordinary key such
  as `Insert`.

## Cross-platform change checklist

For any OS-sensitive change, identify the native API/tool, thread or main-loop
requirement, permission, fallback, and user-visible unsupported behavior. Build
and test on the current machine, then record which other platforms are
unverified. Do not call cross-OS clippy or a local compile evidence of runtime
parity.

## Keeping this document current

Update this page when a feature gains or loses an OS implementation, default
shortcut, permission, external tool, native API, fallback, or verification
status. Verify each matrix row against the corresponding `cfg` module,
frontend `IS_*` branch, Tauri config, and [`TODO.md`](../TODO.md) before
changing the status label.
