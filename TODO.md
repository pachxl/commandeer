# TODO

## Alt-drag window management (`window_drag`)

Hold **Alt** and drag any window to move it; hold **Alt** and **right-drag** to
resize it — Hyprland-style binds applied to any window system-wide. Toggle in
**Settings → Alt-Drag Windows**; persisted as `window_drag` in
`<app-data>/config.json` and re-applied at startup.

Backend: `src-tauri/src/commands/window_drag.rs` (one `platform` module per OS).

### Windows — implemented & tested on-device

The full feature set lives in the Windows `platform` module:

- **Move / resize.** `WH_MOUSE_LL` hook (dedicated message-pump thread) records
  the grab and swallows only the button events; a separate mover thread polls
  `GetCursorPos` at ~200 Hz (`timeBeginPeriod(1)` while dragging) and repositions
  via `SetWindowPos`. The hook never swallows `WM_MOUSEMOVE` (that freezes the
  cursor) and never calls `SetWindowPos` (that can stall system input).
- **Resize corner** is picked from the cursor's 2×2 quadrant.
- **Hover indicator.** While Alt is held over a draggable window, a
  click-through per-pixel-alpha overlay (`UpdateLayeredWindow`, sized to the DWM
  extended-frame bounds, rounded corners) dims the window and highlights the
  region a resize would grab — four quadrants for a normal window, two halves
  for a snapped one. Hidden while dragging and when a snapped window is tiled
  against a neighbor (only one drag is possible then).
- **Snapped-window resize.** A window snapped to a screen edge resizes from its
  one free edge only, keeping its snapped dimension.
- **Tiling.** Two windows snapped flush share an edge: resizing one moves the
  neighbor's facing edge too (single `DeferWindowPos` batch so they stay locked
  together), clamped so neither drops below the min size.
- **Aero-Snap on move.** Dragging toward a screen edge previews and (on release)
  snaps to a half / quarter, or maximizes at the top edge — even splits of the
  work area, offset by the invisible border so visible edges line up. Edge
  trigger band is 80 px. (Screen-edge snapping is **move-only**; resize never
  border-snaps.)

### macOS — basic move/resize only, UNVERIFIED

The macOS `platform` module implements **move + resize only** (`CGEventTap` +
Accessibility `AXUIElement`); none of the snapping / indicator / tiling features
above are ported. It was written on a Windows box and has **never been compiled
or run on a Mac**. Someone with a Mac needs to:

1. **Compile it.** `npm run tauri build -- --no-bundle` (after `source ~/.cargo/env`).
   Raw FFI to CoreGraphics / CoreFoundation / ApplicationServices. Likely
   first-build friction: extern-static linkage of the `kAX*`/`kCFRunLoopCommonModes`
   constants (HIServices, via the `ApplicationServices` umbrella `paste.rs`
   already links); by-value `CGPoint` across FFI; `bool` vs `Boolean` (`u8`).
2. **Grant Accessibility.** System Settings → Privacy & Security → Accessibility.
   Without it `CGEventTapCreate` returns null and `enable()` errors (surfaced as
   a toast). Same permission paste uses.
3. **Behavioral test.** Alt + left-drag moves; Alt + right-drag resizes from the
   cursor's cell; confirm the Alt+click is swallowed.
4. **Coordinate check.** `CGEventGetLocation` and `kAXPositionAttribute` are both
   top-left-origin global coords. Watch multi-display and Retina point-vs-pixel.
5. **Optional:** raise the window on grab (`AXUIElementPerformAction` +
   `kAXRaiseAction`); port the snapping features from the Windows module.

### Linux — intentionally unsupported

Wayland isolates clients: an app cannot read or change another app's window
geometry. This is *by design* and is exactly why a compositor like Hyprland can
offer move/resize binds — it **is** the compositor. COSMIC already provides the
gesture natively (Super + drag to move, Super + right-drag to resize floating
windows), so the Settings entry is hidden on Linux. If X11 support is ever
wanted, the clean route is `_NET_WM_MOVERESIZE` client messages behind an
`env_info().wayland == false` check — not implemented here.
