// Snap-aware hover indicator for the Windows arm: a click-through layered
// window on the hook thread, repainted at ~60 Hz via UpdateLayeredWindow.
use super::*;

use std::sync::atomic::{AtomicBool, AtomicI32, AtomicIsize, Ordering};

use windows::core::w;
use windows::Win32::Foundation::{COLORREF, SIZE};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC, SelectObject,
    BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS, HGDIOBJ,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetCursorPos, RegisterClassW, SetTimer,
    ShowWindow, UpdateLayeredWindow, SW_HIDE, SW_SHOWNA, ULW_ALPHA, WM_TIMER, WNDCLASSW,
    WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};

// ---- Snap-aware hover indicator ---------------------------------------
//
// While Alt is held over a draggable window (and no drag is in progress),
// the window is dimmed with a translucent black overlay; the region a resize
// would grab is dimmed *less* so it stands out. A normal window shows four
// quadrants (the hovered corner is lightest); a snapped window shows two
// halves (the free-edge side is lightest), since it only resizes along one
// axis.
//
// It's a single layered, click-through Win32 window owned by the hook thread
// (which already pumps messages). A window timer repaints it at ~60 Hz via
// UpdateLayeredWindow with a per-pixel-alpha bitmap, so it can (a) match the
// window's DWM visible frame exactly instead of GetWindowRect's oversized
// bounds and (b) round its corners to sit flush. It never touches the drag
// hot path: hidden the instant a grab starts and while ACTIVE.
static IND_HWND: AtomicIsize = AtomicIsize::new(0);
static IND_VISIBLE: AtomicBool = AtomicBool::new(false);
// Last window the indicator locked onto — reused when the cursor sits over
// the overlay itself (WindowFromPoint would otherwise return our own
// click-through window instead of the app beneath it).
static IND_TARGET: AtomicIsize = AtomicIsize::new(0);
// Last painted frame (visible-bounds rect + active-region code), so we only
// rebuild the bitmap when something actually changes.
static IND_LL: AtomicI32 = AtomicI32::new(i32::MIN);
static IND_LT: AtomicI32 = AtomicI32::new(0);
static IND_LR: AtomicI32 = AtomicI32::new(0);
static IND_LB: AtomicI32 = AtomicI32::new(0);
static IND_LACT: AtomicI32 = AtomicI32::new(i32::MIN);

const IND_TIMER: usize = 1;
// Black-overlay opacity: the active region is barely dimmed, the rest are
// dimmed enough to make it pop without hiding the window's contents.
const DIM_ACTIVE: u8 = 26; // ~10%
const DIM_INACTIVE: u8 = 84; // ~33%
const PREVIEW_ALPHA: u8 = 110; // move-snap preview: uniform, clearly visible
const CORNER_RADIUS: i32 = 8; // ~matches the Win11 rounded-corner radius

unsafe extern "system" fn indicator_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_TIMER {
        update_indicator();
        return LRESULT(0);
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

pub(super) unsafe fn create_indicator(hinst: HINSTANCE) {
    let class = w!("CommandeerDragQuadrant");
    let wc = WNDCLASSW {
        lpfnWndProc: Some(indicator_proc),
        hInstance: hinst,
        lpszClassName: class,
        ..Default::default()
    };
    // Ignore the result: re-enabling registers the same class again, which
    // fails with ERROR_CLASS_ALREADY_EXISTS but leaves it usable.
    RegisterClassW(&wc);
    let hwnd = CreateWindowExW(
        WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
        class,
        w!(""),
        WS_POPUP,
        0,
        0,
        0,
        0,
        None,
        None,
        hinst,
        None,
    );
    let Ok(hwnd) = hwnd else { return };
    IND_HWND.store(hwnd.0 as isize, Ordering::Relaxed);
    IND_LL.store(i32::MIN, Ordering::Relaxed);
    IND_LACT.store(i32::MIN, Ordering::Relaxed);
    SetTimer(hwnd, IND_TIMER, 16, None);
}

pub(super) unsafe fn destroy_indicator() {
    IND_VISIBLE.store(false, Ordering::Relaxed);
    IND_TARGET.store(0, Ordering::Relaxed);
    let hwnd = HWND(IND_HWND.swap(0, Ordering::Relaxed) as *mut _);
    if !hwnd.0.is_null() {
        let _ = DestroyWindow(hwnd);
    }
}

pub(super) unsafe fn hide_indicator() {
    if !IND_VISIBLE.swap(false, Ordering::Relaxed) {
        return;
    }
    let hwnd = HWND(IND_HWND.load(Ordering::Relaxed) as *mut _);
    if !hwnd.0.is_null() {
        let _ = ShowWindow(hwnd, SW_HIDE);
    }
}

/// The window's visible frame in screen coordinates. GetWindowRect includes
/// the invisible resize border on Win10/11 (so an overlay sized to it spills
/// past the window); the DWM extended-frame-bounds is the true visible rect.
unsafe fn update_indicator() {
    if IND_HWND.load(Ordering::Relaxed) == 0 {
        return;
    }
    if !alt_down() {
        hide_indicator();
        return;
    }
    // During a move drag, show the Aero-Snap-style preview instead; during a
    // resize drag, show nothing.
    if ACTIVE.load(Ordering::Relaxed) {
        move_snap_preview();
        return;
    }
    let mut p = POINT::default();
    if GetCursorPos(&mut p).is_err() {
        hide_indicator();
        return;
    }
    let root = GetAncestor(WindowFromPoint(p), GA_ROOT);
    let mut pid = 0u32;
    GetWindowThreadProcessId(root, Some(&mut pid));
    // If the cursor is over our own window, it's the overlay itself — fall
    // back to the window we last locked onto (still under it).
    let target = if pid == GetCurrentProcessId() {
        let cached = HWND(IND_TARGET.load(Ordering::Relaxed) as *mut _);
        if cached.0.is_null() || !IsWindowVisible(cached).as_bool() {
            hide_indicator();
            return;
        }
        cached
    } else if is_draggable_root(root) {
        IND_TARGET.store(root.0 as isize, Ordering::Relaxed);
        root
    } else {
        IND_TARGET.store(0, Ordering::Relaxed);
        hide_indicator();
        return;
    };

    // Edge selection uses GetWindowRect (what pick_edges/snap_kind see at
    // grab); the overlay itself is drawn against the DWM visible bounds.
    let mut wr = RECT::default();
    let Some(vb) = visible_bounds(target) else {
        hide_indicator();
        return;
    };
    if GetWindowRect(target, &mut wr).is_err() || wr.right - wr.left < 2 || wr.bottom - wr.top < 2 {
        hide_indicator();
        return;
    }

    // A window whose resize is locked to a single shared divider has exactly
    // one possible drag, so the overlay would be noise — don't show it. That
    // covers both a half-snapped window tiled against a neighbor and a
    // quarter-tiled window (e.g. windows 2/3) with one cleanly-tiled edge.
    let sk = snap_kind(target, &wr);
    let tiled = if sk != SnapKind::None {
        !snap::find_neighbors(sk, &wr, target).is_empty()
    } else {
        [
            SnapKind::Left,
            SnapKind::Right,
            SnapKind::Top,
            SnapKind::Bottom,
        ]
        .into_iter()
        .any(|e| snap::clean_tile_edge(e, &wr, target).is_some())
    };
    if tiled {
        hide_indicator();
        return;
    }

    // Active region as (horizontal, vertical) sides: 0 = min-edge (left /
    // top) half, 1 = max-edge (right / bottom) half, 2 = the whole span. A
    // normal window keys off the cursor's quadrant; a snapped window
    // highlights the half on its free-edge side.
    let (ah, av) = match sk {
        SnapKind::None => {
            let midx = wr.left + (wr.right - wr.left) / 2;
            let midy = wr.top + (wr.bottom - wr.top) / 2;
            (
                if p.x < midx { 0 } else { 1 },
                if p.y < midy { 0 } else { 1 },
            )
        }
        SnapKind::Left => (1, 2),   // free edge = right
        SnapKind::Right => (0, 2),  // free edge = left
        SnapKind::Top => (2, 1),    // free edge = bottom
        SnapKind::Bottom => (2, 0), // free edge = top
    };
    paint_indicator(
        vb,
        ah,
        av,
        IsZoomed(target).as_bool(),
        DIM_ACTIVE,
        DIM_INACTIVE,
    );
}

/// While moving a window, preview the Aero-Snap zone under the cursor as a
/// uniform translucent fill of the target half / quarter / full-screen, or
/// hide it when the cursor isn't in a zone (or the drag is a resize).
unsafe fn move_snap_preview() {
    let (mode, border, dragged) = match state().lock() {
        Ok(st) => (st.mode, st.border, HWND(st.hwnd as *mut _)),
        Err(_) => {
            hide_indicator();
            return;
        }
    };
    if mode != Mode::Move {
        hide_indicator();
        return;
    }
    let mut p = POINT::default();
    if GetCursorPos(&mut p).is_err() {
        hide_indicator();
        return;
    }
    let Some(work) = snap::work_area_at(p) else {
        hide_indicator();
        return;
    };
    let Some(zone) = snap::snap_zone(p, work) else {
        hide_indicator();
        return;
    };
    let fill_x = snap::snap_fill_x(zone, work, dragged);
    let (rect, maximized) = snap::zone_rect(zone, work, border, fill_x);
    paint_indicator(rect, 2, 2, maximized, PREVIEW_ALPHA, PREVIEW_ALPHA);
}

/// Rebuild the overlay bitmap for the visible bounds `vb`, filling the four
/// quadrants — the active region (matching `ah`/`av`, where 2 means "either
/// side") gets `active_alpha`, the rest `inactive_alpha` — with rounded
/// outer corners. Only repaints when the bounds or active region changed.
unsafe fn paint_indicator(
    vb: RECT,
    ah: i32,
    av: i32,
    maximized: bool,
    active_alpha: u8,
    inactive_alpha: u8,
) {
    let hwnd = HWND(IND_HWND.load(Ordering::Relaxed) as *mut _);
    if hwnd.0.is_null() {
        return;
    }
    let w = vb.right - vb.left;
    let h = vb.bottom - vb.top;
    if w < 2 || h < 2 {
        hide_indicator();
        return;
    }
    let act_code = ah * 4 + av;
    let changed = IND_LL.load(Ordering::Relaxed) != vb.left
        || IND_LT.load(Ordering::Relaxed) != vb.top
        || IND_LR.load(Ordering::Relaxed) != vb.right
        || IND_LB.load(Ordering::Relaxed) != vb.bottom
        || IND_LACT.load(Ordering::Relaxed) != act_code;

    if changed {
        // Top-down 32bpp premultiplied BGRA. The dim is pure black, so
        // premultiplied color channels are 0 and every pixel is just
        // (alpha << 24); transparent outside the rounded corners.
        let radius = if maximized { 0 } else { CORNER_RADIUS };
        let midx = w / 2;
        let midy = h / 2;
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h, // negative = top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let screen_dc = GetDC(None);
        let mem_dc = CreateCompatibleDC(screen_dc);
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        if let Ok(dib) = CreateDIBSection(mem_dc, &bmi, DIB_RGB_COLORS, &mut bits, None, 0) {
            if !bits.is_null() {
                let px = std::slice::from_raw_parts_mut(bits as *mut u32, (w * h) as usize);
                for y in 0..h {
                    let qv = if y < midy { 0 } else { 1 };
                    let v_active = av == 2 || av == qv;
                    for x in 0..w {
                        let alpha = if rounded_out(x, y, w, h, radius) {
                            0u32
                        } else {
                            let qh = if x < midx { 0 } else { 1 };
                            if v_active && (ah == 2 || ah == qh) {
                                active_alpha as u32
                            } else {
                                inactive_alpha as u32
                            }
                        };
                        px[(y * w + x) as usize] = alpha << 24;
                    }
                }
                let old = SelectObject(mem_dc, HGDIOBJ(dib.0));
                let pt_dst = POINT {
                    x: vb.left,
                    y: vb.top,
                };
                let sz = SIZE { cx: w, cy: h };
                let pt_src = POINT { x: 0, y: 0 };
                let blend = BLENDFUNCTION {
                    BlendOp: 0, // AC_SRC_OVER
                    BlendFlags: 0,
                    SourceConstantAlpha: 255,
                    AlphaFormat: 1, // AC_SRC_ALPHA (premultiplied)
                };
                let _ = UpdateLayeredWindow(
                    hwnd,
                    screen_dc,
                    Some(&pt_dst),
                    Some(&sz),
                    mem_dc,
                    Some(&pt_src),
                    COLORREF(0),
                    Some(&blend),
                    ULW_ALPHA,
                );
                SelectObject(mem_dc, old);
            }
            let _ = DeleteObject(HGDIOBJ(dib.0));
        }
        let _ = DeleteDC(mem_dc);
        ReleaseDC(None, screen_dc);

        IND_LL.store(vb.left, Ordering::Relaxed);
        IND_LT.store(vb.top, Ordering::Relaxed);
        IND_LR.store(vb.right, Ordering::Relaxed);
        IND_LB.store(vb.bottom, Ordering::Relaxed);
        IND_LACT.store(act_code, Ordering::Relaxed);
    }

    if !IND_VISIBLE.swap(true, Ordering::Relaxed) {
        let _ = ShowWindow(hwnd, SW_SHOWNA);
    }
}

/// True if pixel (x, y) falls outside the window's rounded corner of radius
/// `r` — so it's painted fully transparent and the overlay's corners match
/// the window's.
fn rounded_out(x: i32, y: i32, w: i32, h: i32, r: i32) -> bool {
    if r <= 0 {
        return false;
    }
    let cx = if x < r {
        r
    } else if x >= w - r {
        w - 1 - r
    } else {
        return false;
    };
    let cy = if y < r {
        r
    } else if y >= h - r {
        h - 1 - r
    } else {
        return false;
    };
    let dx = x - cx;
    let dy = y - cy;
    dx * dx + dy * dy > r * r
}
