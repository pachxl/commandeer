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
- **Raise on grab.** The grabbed window is brought to the front via
  `AttachThreadInput` + `SetForegroundWindow` + `BringWindowToTop` (a plain
  cross-process `SetWindowPos(HWND_TOP)` is ignored by the foreground lock).
- **Games left alone.** `is_fullscreen_window` rejects a window that is not
  `IsZoomed`, covers its whole monitor (incl. the taskbar), and is borderless —
  so an Alt-drag never moves/resizes a fullscreen or borderless game, and the
  click passes straight through.
- **Resize edge selection.**
  - A half-screen snapped window resizes from its one free edge.
  - An un-snapped window whose edge is **cleanly tiled** — shared with
    neighbor(s) that span it exactly, none overhanging (`clean_tile_edge`) —
    resizes only that edge (nearest the cursor) and locks the rest, so a
    quarter-tiled window keeps its width and position. Two windows stacked in a
    column resize only their shared divider.
  - Otherwise the corner is picked from the cursor's 2×2 quadrant (free resize).
- **Hover indicator.** While Alt is held over a draggable window, a
  click-through per-pixel-alpha overlay (`UpdateLayeredWindow`, sized to the DWM
  extended-frame bounds, rounded corners) dims the window and highlights the
  region a resize would grab — four quadrants for a free window, two halves for
  a snapped one. Hidden while dragging and whenever the resize is locked to a
  single shared divider (same `clean_tile_edge` test as the resize).
- **Tiling (N windows).** Resizing a shared edge moves **every** window flush
  along it — `find_neighbors` samples the whole edge (not one midpoint), so one
  tall window beside a stack of two resizes both — applied in a single
  `DeferWindowPos` batch, clamped so none drops below `MIN_SIZE`. Neighbor
  facing edges overlap 3/4 of the combined invisible border to shrink the gap.
- **Aero-Snap on move.** Dragging toward a screen edge previews and (on release)
  snaps to a half / quarter, or maximizes at the top edge, offset by the
  invisible border so visible edges line up. Edge trigger band is 160 px.
  Snapping to a side **fills the space** beside an already-snapped window
  (`snap_fill_x`) rather than a fixed half. (Screen-edge snapping is
  **move-only**; resize never border-snaps.)

### macOS — basic move/resize + raise, COMPILING (behaviorally UNVERIFIED)

The macOS `platform` module implements **move + resize + raise-on-grab**
(`CGEventTap` + Accessibility `AXUIElement`); none of the snapping /
indicator / tiling features below are ported. It now **compiles and links
cleanly on macOS** (cargo build + the 19-test suite pass on arm64), but
has **never been behaviorally run on a Mac**. Originally written blind on
a Windows box, it hit the predicted FFI friction on first compile: the
`kAX*Attribute` constants are `CFSTR("...")` macros in the SDK headers,
not exported linkable symbols (absent from `HIServices.tbd` on modern
macOS), so they're now built at runtime via `CFStringCreateWithCString`
and cached in `OnceLock`s. Someone with a Mac still needs to:

1. ~~Compile it.~~ **Done** — `cargo build` / `cargo test` green on arm64.
2. **Grant Accessibility.** System Settings → Privacy & Security →
   Accessibility. Without it `CGEventTapCreate` returns null and `enable()`
   errors (surfaced as a toast). Same permission paste uses.
3. **Behavioral test.** Alt + left-drag moves; Alt + right-drag resizes
   from the cursor's cell; confirm the Alt+click is swallowed; confirm the
   grabbed window raises to the front.
4. **Coordinate check.** `CGEventGetLocation` and `kAXPositionAttribute`
   are both top-left-origin global coords. Watch multi-display and Retina
   point-vs-pixel.
5. **Optional:** port the snapping features from the Windows module
   (hover indicator, edge selection, tiling, Aero-Snap). macOS Sequoia
   already does native drag-to-edge tiling, so the gap is smaller than on
   Windows.

### Linux — intentionally unsupported

Wayland isolates clients: an app cannot read or change another app's window
geometry. This is _by design_ and is exactly why a compositor like Hyprland can
offer move/resize binds — it **is** the compositor. COSMIC already provides the
gesture natively (Super + drag to move, Super + right-drag to resize floating
windows), so the Settings entry is hidden on Linux. If X11 support is ever
wanted, the clean route is `_NET_WM_MOVERESIZE` client messages behind an
`env_info().wayland == false` check — not implemented here.
