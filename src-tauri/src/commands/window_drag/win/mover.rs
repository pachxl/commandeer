// The window mover thread for the Windows arm: polls the real cursor at ~200 Hz
// and repositions the grabbed window (and tiled neighbors) with SetWindowPos.
use super::*;

use std::time::Duration;
use windows::Win32::Media::{timeBeginPeriod, timeEndPeriod};
use windows::Win32::UI::WindowsAndMessaging::{
    BeginDeferWindowPos, DeferWindowPos, EndDeferWindowPos, GetCursorPos, ShowWindow,
    SWP_ASYNCWINDOWPOS, SWP_NOOWNERZORDER, SWP_NOZORDER, SW_MAXIMIZE, SW_RESTORE,
};

// The mover thread applies window moves at ~200 Hz regardless of mouse
// polling rate. `thread::sleep` only honours a sub-16 ms interval when the
// system timer resolution is raised — the mover does that (timeBeginPeriod)
// for the duration of each drag, so this interval is real, not rounded up
// to the default ~15.6 ms tick (which would cap us near 64 Hz and look
// choppy next to a native title-bar drag).
const MOVE_INTERVAL: Duration = Duration::from_millis(5);

/// The window mover. Runs on its own thread so no window operation ever
/// executes inside the input-hook callback (SetWindowPos/ShowWindow send
/// synchronous messages to the target app's thread and can stall the OS
/// input pipeline past the LL-hook timeout). Wakes ~120x/sec, polls the
/// *real* cursor with GetCursorPos, and repositions the window. Because the
/// hook never blocks mouse-moves, the physical cursor travels freely,
/// relative input accumulates normally, and the window simply follows the
/// pointer at a fixed grab offset.
pub(super) fn mover_loop() {
    let mut was_active = false;
    let mut local_gen = u64::MAX;
    let mut last: Option<(i32, i32, i32, i32)> = None;
    // Raise the system timer resolution to 1 ms only while a drag is live
    // (AltSnap does the same), so MOVE_INTERVAL sleeps are accurate without
    // holding a 1 ms tick system-wide when idle.
    let mut hires = false;
    loop {
        std::thread::sleep(MOVE_INTERVAL);
        if !MOVER_RUN.load(Ordering::Relaxed) {
            break;
        }
        let active = ACTIVE.load(Ordering::Relaxed);
        let finishing = was_active && !active; // apply one exact final frame
        was_active = active;
        if active && !hires {
            unsafe { timeBeginPeriod(1) };
            hires = true;
        } else if !active && !finishing && hires {
            unsafe { timeEndPeriod(1) };
            hires = false;
        }
        if !active && !finishing {
            continue;
        }

        // Snapshot the per-grab params under a brief lock (never held during
        // SetWindowPos, so the hook's button handlers don't stall).
        let (mode, hwnd, sx, sy, mut rect, mut edges, restore_max, snap, neighbors, mut border) =
            match state().lock() {
                Ok(st) => (
                    st.mode,
                    st.hwnd,
                    st.start_x,
                    st.start_y,
                    st.rect,
                    st.edges,
                    st.restore_max,
                    st.snap,
                    st.neighbors.clone(),
                    st.border,
                ),
                Err(_) => continue,
            };
        let gen = GEN.load(Ordering::Relaxed);
        let new_grab = gen != local_gen;
        if new_grab {
            local_gen = gen;
            last = None;
        }
        let hw = HWND(hwnd as *mut _);

        // Bring the grabbed window to the front on the first frame of each
        // grab. We swallow the Alt+click, so the window never gets the click
        // that would normally raise it — replicate that here. A plain
        // SetWindowPos(HWND_TOP) is silently ignored across processes (a
        // background app can't reorder another app's window), so use the
        // AttachThreadInput + SetForegroundWindow recipe. Done on the mover
        // thread, never the hook thread.
        if new_grab {
            unsafe {
                raise_window(hw);
            }
        }

        // Deferred maximized-restore: a maximized window can't be usefully
        // moved/resized in place. Restore it here, on the grab's first
        // frame, then (for a move) recenter it under the grab point so the
        // drag continues naturally.
        if restore_max {
            unsafe {
                let _ = ShowWindow(hw, SW_RESTORE);
                let mut r = RECT::default();
                if GetWindowRect(hw, &mut r).is_ok() {
                    // Recompute the border insets from the *restored* frame.
                    // The insets captured at grab time were the maximized
                    // window's, whose rect overhangs the work area on all
                    // four sides (including a ~8px top inset that a normal
                    // window doesn't have). Reusing them for a move-snap
                    // offsets the snapped window that far above the work
                    // area — the title bar gets clipped and the window ends
                    // up a few px too tall. The restored frame has the true,
                    // ~0px top inset.
                    border = border_insets(hw, &r);
                    if mode == Mode::Move {
                        let w = r.right - r.left;
                        let h = r.bottom - r.top;
                        let nx = sx - w / 2;
                        let ny = sy - 15;
                        let _ = SetWindowPos(
                            hw,
                            HWND::default(),
                            nx,
                            ny,
                            0,
                            0,
                            SWP_NOZORDER | SWP_NOSIZE | SWP_NOACTIVATE,
                        );
                        rect = RECT {
                            left: nx,
                            top: ny,
                            right: nx + w,
                            bottom: ny + h,
                        };
                    } else {
                        rect = r;
                        edges = snap::pick_edges(sx, sy, &r);
                    }
                }
            }
            if let Ok(mut st) = state().lock() {
                // Persist the corrected grab rect/edges/border unless a newer
                // grab already replaced this one (GEN bumps under the same
                // lock).
                if GEN.load(Ordering::Relaxed) == gen {
                    st.rect = rect;
                    st.edges = edges;
                    st.border = border;
                    st.restore_max = false;
                }
            }
        }

        // Ground truth for the drag: the actual on-screen cursor. Never
        // fall back to (0,0) if the read fails (secure desktop etc.).
        let mut p = POINT::default();
        if unsafe { GetCursorPos(&mut p) }.is_err() {
            continue;
        }
        let (cx, cy) = (p.x, p.y);
        let (mut x, mut y, mut w, mut h) = snap::compute_target(mode, sx, sy, rect, edges, cx, cy);

        // Aero-Snap on move: committed on release (the finishing frame). If
        // the cursor is in an edge zone, snap to that half / quarter, or
        // maximize for the top edge. During the drag the window just follows
        // the cursor; the preview overlay shows where it will land.
        let mut maximize_apply = false;
        let mut snapped_move = false;
        if mode == Mode::Move && finishing {
            let cp = POINT { x: cx, y: cy };
            if let Some(work) = unsafe { snap::work_area_at(cp) } {
                if let Some(zone) = snap::snap_zone(cp, work) {
                    if zone == snap::SnapZone::Maximize {
                        maximize_apply = true;
                    } else {
                        let fill_x = unsafe { snap::snap_fill_x(zone, work, hw) };
                        let (rc, _) = snap::zone_rect(zone, work, border, fill_x);
                        x = rc.left;
                        y = rc.top;
                        w = rc.right - rc.left;
                        h = rc.bottom - rc.top;
                        snapped_move = true;
                    }
                }
            }
        }

        // Tiling: a snapped resize whose free edge is shared with neighbors
        // moves every neighbor's facing edge too (clamped so none drops
        // below MIN_SIZE), so shrinking the target grows all of them.
        let mut neighbor_apply: Vec<(isize, i32, i32, i32, i32)> = Vec::new();
        if mode == Mode::Resize && snap != SnapKind::None && !neighbors.is_empty() {
            let ((tl, tt, tr, tb), nrects) =
                snap::coordinate_neighbors(snap, &neighbors, x, y, x + w, y + h);
            x = tl;
            y = tt;
            w = tr - tl;
            h = tb - tt;
            neighbor_apply = nrects
                .into_iter()
                .map(|(nh, r)| (nh, r.left, r.top, r.right - r.left, r.bottom - r.top))
                .collect();
        }

        // Dead-zone: skip sub-2px changes to absorb high-polling-mouse
        // tremor; always apply the final frame so the landing is exact.
        let skip = !finishing
            && matches!(last, Some((ax, ay, aw, ah))
                    if (x - ax).abs() <= 1 && (y - ay).abs() <= 1
                        && (w - aw).abs() <= 1 && (h - ah).abs() <= 1);
        if skip {
            continue;
        }
        unsafe {
            if !neighbor_apply.is_empty() {
                // Tiling: move the target and ALL its neighbors in one
                // deferred batch so the window manager applies them in a
                // single screen-refresh cycle — the shared edges stay locked
                // instead of the neighbors trailing the target. Synchronous
                // (no SWP_ASYNCWINDOWPOS) so they land together this frame.
                let dflags = SWP_NOZORDER | SWP_NOOWNERZORDER | SWP_NOACTIVATE;
                if let Ok(mut hdwp) = BeginDeferWindowPos(1 + neighbor_apply.len() as i32) {
                    if let Ok(h) = DeferWindowPos(hdwp, hw, HWND::default(), x, y, w, h, dflags) {
                        hdwp = h;
                        for (nh, nx, ny, nw, nhh) in &neighbor_apply {
                            let nhw = HWND(*nh as *mut _);
                            match DeferWindowPos(
                                hdwp,
                                nhw,
                                HWND::default(),
                                *nx,
                                *ny,
                                *nw,
                                *nhh,
                                dflags,
                            ) {
                                Ok(h) => hdwp = h,
                                Err(_) => break,
                            }
                        }
                        let _ = EndDeferWindowPos(hdwp);
                    }
                }
            } else if maximize_apply {
                // Top-edge move-snap: use the real OS maximize.
                let _ = ShowWindow(hw, SW_MAXIMIZE);
            } else if mode == Mode::Move && !snapped_move {
                // Plain move: SWP_ASYNCWINDOWPOS (same as AltSnap) posts the
                // reposition to the target's thread instead of waiting on it,
                // so a busy or hung app never stalls this loop. A move is
                // cheap for the app (no relayout), so there's no flood risk.
                let _ = SetWindowPos(
                    hw,
                    HWND::default(),
                    x,
                    y,
                    w,
                    h,
                    SWP_NOZORDER
                        | SWP_NOOWNERZORDER
                        | SWP_NOSIZE
                        | SWP_NOACTIVATE
                        | SWP_ASYNCWINDOWPOS,
                );
            } else {
                // Resize: SYNCHRONOUS (no SWP_ASYNCWINDOWPOS) so the mover
                // self-paces to how fast the app can relayout. Firing async
                // resizes at ~200 Hz floods a slow app (File Explorer relays
                // out its view on every WM_SIZE): its queue backs up and the
                // window falls further and further behind the cursor.
                // Synchronous keeps latency bounded — the native experience.
                let _ = SetWindowPos(
                    hw,
                    HWND::default(),
                    x,
                    y,
                    w,
                    h,
                    SWP_NOZORDER | SWP_NOOWNERZORDER | SWP_NOACTIVATE,
                );
            }
        }
        last = Some((x, y, w, h));
    }
    if hires {
        unsafe { timeEndPeriod(1) };
    }
}
