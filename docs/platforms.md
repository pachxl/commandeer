# Platform behavior and permissions

Commandeer targets Windows, Linux/Wayland, and macOS. Shared commands should
have a deliberate implementation or a deliberate unsupported result on each
OS. Never infer support from “not Windows”; Linux and macOS often need different
code and different permissions.

## Capability matrix

| Capability           | Windows                                                | Linux                                                             | macOS                                                      |
| -------------------- | ------------------------------------------------------ | ----------------------------------------------------------------- | ---------------------------------------------------------- |
| Palette window       | Acrylic; Onix adds a DPI-aware rounded native region   | GTK layer-shell; Onix models its optical material                 | Accessory window; macOS 26 Liquid Glass, vibrancy fallback |
| Palette shortcut     | Global shortcut plugin                                 | Managed COSMIC/GNOME shortcut plus plugin on X11 where applicable | Global shortcut plugin; default `Cmd+Shift+Space`          |
| Screenshot           | GDI virtual-screen capture; DWM cloak/reveal handshake | CLI fallback chain; transparent stale-frame defense               | `screencapture -R` on cursor monitor                       |
| Output volume        | Core Audio endpoint APIs                               | `wpctl`, then `pactl`                                             | `osascript` default output                                 |
| Application mixer    | Supported                                              | Unsupported                                                       | Unsupported                                                |
| Alt-drag windows     | Full move/resize/snap implementation                   | Unsupported by design under Wayland; compositor owns it           | Move/resize/raise with Accessibility; snapping not ported  |
| Per-monitor Alt+Tab  | Supported native switcher                              | Unsupported                                                       | Unsupported                                                |
| Installed apps       | Shell AppsFolder and shortcut/icon APIs                | Desktop entries, AppImages, executables                           | `.app` bundles and `open`                                  |
| Active-folder search | Focused Explorer folder                                | Home-folder fallback                                              | Focused Finder folder or home fallback                     |
| Clipboard history    | DPAPI encryption and native listener                   | ChaCha20-Poly1305; poll monitor                                   | ChaCha20-Poly1305; poll plus pasteboard change count       |

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

On macOS, Settings → Permissions & Diagnostics reads Screen Recording and
Accessibility grants without prompting, opens the matching Privacy & Security
pane when remediation is needed, links to Automation settings, and provides a
real screenshot test plus an Alt-drag verification checklist.

## Onix palette substrate

Onix shares one dark “Black Water” optical design, but its honest native
substrate differs by platform:

- **macOS 26:** `commands/palette_surface.rs` finds `NSGlassEffectView` at
  runtime and assigns Wry's existing WebKit view as the glass view's
  `contentView`. The glass then fills a dedicated rounded clipping view, which
  becomes the `NSWindow` content view.
  Making the glass a sibling or merely adding blur would not produce the same
  system Liquid Glass behavior. Onix uses adaptive Regular glass for both the
  compact lens and expanded panel. The native tint is unset and the `NSWindow`
  is explicitly non-opaque with a clear background; the rounded frontend
  absorption field supplies the dark Black Water tone without revealing a faint
  rectangular host on bright wallpaper.
  `NSGlassEffectView.cornerRadius` owns the native optical curve while the
  outer clip masks the rectangular compositor host to the same radius and the
  transparent web root clips its content to the coincident CSS curve. The
  glass view itself is not forced through a rasterizing Core Animation mask. The
  operation is idempotent and preserves the first responder. Switching to
  Default unwraps the WebKit view; Default and Onix on older macOS then use
  `NSVisualEffectMaterial::HudWindow` through
  `window-vibrancy`.
- **Windows:** Tauri's Acrylic window effect remains the native backdrop.
  Onix applies a DPI-aware `CreateRoundRectRgn`/`SetWindowRgn` capsule or panel
  boundary; switching to Default clears that region.
- **Linux/Wayland:** there is no portable API for sampling and refracting the
  desktop behind a layer-shell client. The native surface therefore stays
  transparent and the frontend models the dark material, edge refraction,
  Fresnel rim, and moving highlight. This is an intentional optical fallback,
  not a claim of native backdrop glass.

The compact capsule and expanded panel remain two semantic states. On macOS,
`resize_palette_window` uses a 150 ms `NSAnimationContext` frame animation that
keeps the top edge fixed; ordinary `WindowEvent::Resized` callbacks interpolate
the clip and glass radii from 33 to 25 points only for that capsule-to-panel
bloom. Subsequent expanded-panel height changes retain the panel radius.
Reduced Motion, Windows, X11, and Wayland use their direct native resize paths,
while the short frontend shape/content transitions keep state changes coherent.

Compactness is scoped to a visible palette session: Onix may open compact only
at a clean root. Once query, navigation, loading, error, confirmation, action,
or feedback state expands it, it remains expanded until the whole-session
reset/dismiss path runs. This prevents resize churn and flashes during ordinary
query clearing or navigation.

Themes normally supply Commandeer's colors. Onix is the deliberate exception:
it fixes the neutral material and foreground palette while continuing to use
the selected theme's accent color for emphasis.

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
