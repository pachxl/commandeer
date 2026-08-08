// Snap-zone and tiling geometry for the Windows arm: Aero-Snap zones,
// flush-neighbor detection, and the clamped coordinate plans for tiled resizes.
use super::*;

use windows::Win32::Graphics::Gdi::MonitorFromPoint;

/// Find a window snapped flush against the resized window's free edge, so a
/// resize can move both edges together (tiling). Probes just past the free
/// edge, at the mid-point of the shared side, and accepts the window there
/// if its facing edge sits on the boundary.
/// Every distinct window snapped flush along the target's free edge. The
/// free edge is sampled at many points just outside it (not one midpoint):
/// a full-height window beside a stack of two shorter ones borders *both*,
/// and both must tile. `WindowFromPoint` respects Z-order, so only the
/// windows actually visible against the edge are picked up. Deduped by HWND.
pub(super) unsafe fn find_neighbors(
    snap: SnapKind,
    rect: &RECT,
    self_hwnd: HWND,
) -> Vec<(HWND, RECT)> {
    const PROBE: i32 = 8;
    // GetWindowRect includes the ~7px invisible border on both windows, so
    // two flush windows' facing edges can differ by ~2 borders.
    const TOL: i32 = 20;
    // Evenly spaced samples along the edge; even a MIN_SIZE-wide cell on a
    // wide monitor lands on at least one. Inset a little from the corners so
    // a probe never straddles a perpendicular neighbor.
    const SAMPLES: i32 = 48;
    const INSET: i32 = 6;
    let mut out: Vec<(HWND, RECT)> = Vec::new();
    if snap == SnapKind::None {
        return out;
    }
    // (start, end) is the range swept along the edge; the axis is x for the
    // Top/Bottom free edges and y for Left/Right.
    let along_x = matches!(snap, SnapKind::Top | SnapKind::Bottom);
    let (start, end) = if along_x {
        (rect.left + INSET, rect.right - INSET)
    } else {
        (rect.top + INSET, rect.bottom - INSET)
    };
    if end <= start {
        return out;
    }
    for i in 0..=SAMPLES {
        let t = start + (end - start) * i / SAMPLES;
        let probe = match snap {
            SnapKind::Right => POINT {
                x: rect.left - PROBE,
                y: t,
            },
            SnapKind::Left => POINT {
                x: rect.right + PROBE,
                y: t,
            },
            SnapKind::Top => POINT {
                x: t,
                y: rect.bottom + PROBE,
            },
            SnapKind::Bottom => POINT {
                x: t,
                y: rect.top - PROBE,
            },
            SnapKind::None => continue,
        };
        let root = GetAncestor(WindowFromPoint(probe), GA_ROOT);
        if root == self_hwnd || !is_draggable_root(root) {
            continue;
        }
        if out.iter().any(|(h, _)| *h == root) {
            continue;
        }
        let mut nr = RECT::default();
        if GetWindowRect(root, &mut nr).is_err() {
            continue;
        }
        let flush = match snap {
            SnapKind::Right => (nr.right - rect.left).abs() <= TOL,
            SnapKind::Left => (nr.left - rect.right).abs() <= TOL,
            SnapKind::Top => (nr.top - rect.bottom).abs() <= TOL,
            SnapKind::Bottom => (nr.bottom - rect.top).abs() <= TOL,
            SnapKind::None => false,
        };
        if flush {
            out.push((root, nr));
        }
    }
    out
}

/// The (possibly clamped) target edges `(l, t, r, b)` plus each neighbor's
/// `(hwnd, rect)` to apply.
type NeighborPlan = ((i32, i32, i32, i32), Vec<(isize, RECT)>);

/// Given the target's resized edges `(tl, tt, tr, tb)`, move every neighbor's
/// facing edge to the shared boundary (keeping their other three edges), and
/// clamp the boundary so neither the target nor *any* neighbor drops below
/// `MIN_SIZE`. Returns the (possibly clamped) target edges plus each
/// neighbor's `(hwnd, rect)` to apply.
pub(super) fn coordinate_neighbors(
    snap: SnapKind,
    neighbors: &[Neighbor],
    tl: i32,
    tt: i32,
    tr: i32,
    tb: i32,
) -> NeighborPlan {
    // The target's free edge sits at the shared boundary `b`; each neighbor's
    // facing edge is pushed its own `overlap` past it (into the invisible
    // border) so the visible gap is halved. Use `.max(lo).min(hi)` rather
    // than `.clamp(lo, hi)`: if the combined span is too small the bounds
    // cross and `clamp` would panic — this degrades gracefully instead. The
    // boundary is clamped against the *tightest* neighbor so none collapses.
    match snap {
        // Target free edge = left; neighbors are the windows on the left
        // (facing edge = their right). Boundary = shared vertical line.
        SnapKind::Right => {
            let lo = neighbors
                .iter()
                .map(|n| n.rect.left + MIN_SIZE)
                .max()
                .unwrap_or(i32::MIN);
            let b = tl.max(lo).min(tr - MIN_SIZE);
            let nrects = neighbors
                .iter()
                .map(|n| {
                    (
                        n.hwnd,
                        RECT {
                            left: n.rect.left,
                            top: n.rect.top,
                            right: b + n.overlap,
                            bottom: n.rect.bottom,
                        },
                    )
                })
                .collect();
            ((b, tt, tr, tb), nrects)
        }
        // Free edge = right; neighbors on the right (facing = their left).
        SnapKind::Left => {
            let hi = neighbors
                .iter()
                .map(|n| n.rect.right - MIN_SIZE)
                .min()
                .unwrap_or(i32::MAX);
            let b = tr.max(tl + MIN_SIZE).min(hi);
            let nrects = neighbors
                .iter()
                .map(|n| {
                    (
                        n.hwnd,
                        RECT {
                            left: b - n.overlap,
                            top: n.rect.top,
                            right: n.rect.right,
                            bottom: n.rect.bottom,
                        },
                    )
                })
                .collect();
            ((tl, tt, b, tb), nrects)
        }
        // Free edge = bottom; neighbors below (facing = their top).
        SnapKind::Top => {
            let hi = neighbors
                .iter()
                .map(|n| n.rect.bottom - MIN_SIZE)
                .min()
                .unwrap_or(i32::MAX);
            let b = tb.max(tt + MIN_SIZE).min(hi);
            let nrects = neighbors
                .iter()
                .map(|n| {
                    (
                        n.hwnd,
                        RECT {
                            left: n.rect.left,
                            top: b - n.overlap,
                            right: n.rect.right,
                            bottom: n.rect.bottom,
                        },
                    )
                })
                .collect();
            ((tl, tt, tr, b), nrects)
        }
        // Free edge = top; neighbors above (facing = their bottom).
        SnapKind::Bottom => {
            let lo = neighbors
                .iter()
                .map(|n| n.rect.top + MIN_SIZE)
                .max()
                .unwrap_or(i32::MIN);
            let b = tt.max(lo).min(tb - MIN_SIZE);
            let nrects = neighbors
                .iter()
                .map(|n| {
                    (
                        n.hwnd,
                        RECT {
                            left: n.rect.left,
                            top: n.rect.top,
                            right: n.rect.right,
                            bottom: b + n.overlap,
                        },
                    )
                })
                .collect();
            ((tl, b, tr, tb), nrects)
        }
        SnapKind::None => ((tl, tt, tr, tb), Vec::new()),
    }
}

const MIN_SIZE: i32 = 120;

#[derive(Clone, Copy, PartialEq)]
pub(super) enum SnapZone {
    Left,
    Right,
    Maximize,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

/// The work area of the monitor under a screen point (the cursor's monitor,
/// so snapping follows the pointer across displays).
pub(super) unsafe fn work_area_at(p: POINT) -> Option<RECT> {
    let hmon = MonitorFromPoint(p, MONITOR_DEFAULTTONEAREST);
    let mut mi = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if GetMonitorInfoW(hmon, &mut mi).as_bool() {
        Some(mi.rcWork)
    } else {
        None
    }
}

/// Which snap zone the cursor is in while moving a window, or None — same
/// regions as dragging a title bar in Windows: side edges give halves,
/// corners give quarters, the top edge maximizes.
pub(super) fn snap_zone(p: POINT, work: RECT) -> Option<SnapZone> {
    // Trigger band along each screen edge. Generous so a fast drag lands in
    // it well before the cursor reaches the edge — on multi-monitor setups
    // that means you can snap without slowing down to avoid overshooting
    // onto the next display.
    const EDGE: i32 = 160;
    const CORNER: i32 = 100; // how far along an edge still counts as a corner
    let near_left = p.x <= work.left + EDGE;
    let near_right = p.x >= work.right - EDGE;
    let near_top = p.y <= work.top + EDGE;
    if near_left {
        if p.y <= work.top + CORNER {
            Some(SnapZone::TopLeft)
        } else if p.y >= work.bottom - CORNER {
            Some(SnapZone::BottomLeft)
        } else {
            Some(SnapZone::Left)
        }
    } else if near_right {
        if p.y <= work.top + CORNER {
            Some(SnapZone::TopRight)
        } else if p.y >= work.bottom - CORNER {
            Some(SnapZone::BottomRight)
        } else {
            Some(SnapZone::Right)
        }
    } else if near_top {
        Some(SnapZone::Maximize)
    } else {
        None
    }
}

/// The interior vertical boundary a half-snap should fill up to, if a window
/// is already snapped against the opposite wall — Windows-style: snapping
/// left when the right half is occupied fills the *remaining* space (up to
/// that window's visible edge) instead of a fixed 50%. `None` (no opposite
/// snapped window) falls back to the work-area midpoint. Probes 3/4 of the
/// way across, so a half-height quarter won't be mistaken for a full column.
pub(super) unsafe fn snap_fill_x(zone: SnapZone, work: RECT, dragged: HWND) -> Option<i32> {
    let w = work.right - work.left;
    let (probe_x, want) = match zone {
        SnapZone::Left => (work.left + w * 3 / 4, SnapKind::Right),
        SnapZone::Right => (work.left + w / 4, SnapKind::Left),
        _ => return None,
    };
    let probe = POINT {
        x: probe_x,
        y: (work.top + work.bottom) / 2,
    };
    let root = GetAncestor(WindowFromPoint(probe), GA_ROOT);
    if root == dragged || !is_draggable_root(root) {
        return None;
    }
    let mut wr = RECT::default();
    if GetWindowRect(root, &mut wr).is_err() || snap_kind(root, &wr) != want {
        return None;
    }
    // Fill up to the occupant's *visible* facing edge so the two touch flush.
    let vb = visible_bounds(root).unwrap_or(wr);
    Some(match want {
        SnapKind::Right => vb.left, // occupant on the right; fill our right edge to its left
        _ => vb.right,              // occupant on the left; fill our left edge to its right
    })
}

/// The target window rect for a snap zone — a half / quarter / full split of
/// the work area, offset by the window's invisible border so the visible
/// edges line up. `fill_x` overrides the interior vertical boundary of a
/// half-snap so it fills the space beside an already-snapped window. The
/// bool marks a full-screen (maximize) target (drawn with square corners);
/// `Maximize` uses the OS maximize on commit, this rect is only for preview.
pub(super) fn zone_rect(
    zone: SnapZone,
    work: RECT,
    border: RECT,
    fill_x: Option<i32>,
) -> (RECT, bool) {
    let midx = (work.left + work.right) / 2;
    let midy = (work.top + work.bottom) / 2;
    // For a left/right half, the interior edge fills up to an existing
    // neighbor when there is one; otherwise it's the midpoint.
    let bound = fill_x.unwrap_or(midx);
    let (bl, bt, br, bb) = (border.left, border.top, border.right, border.bottom);
    let mk = |l: i32, t: i32, r: i32, b: i32| RECT {
        left: l,
        top: t,
        right: r,
        bottom: b,
    };
    match zone {
        SnapZone::Left => (
            mk(work.left - bl, work.top - bt, bound + br, work.bottom + bb),
            false,
        ),
        SnapZone::Right => (
            mk(bound - bl, work.top - bt, work.right + br, work.bottom + bb),
            false,
        ),
        SnapZone::Maximize => (mk(work.left, work.top, work.right, work.bottom), true),
        SnapZone::TopLeft => (
            mk(work.left - bl, work.top - bt, midx + br, midy + bb),
            false,
        ),
        SnapZone::TopRight => (
            mk(midx - bl, work.top - bt, work.right + br, midy + bb),
            false,
        ),
        SnapZone::BottomLeft => (
            mk(work.left - bl, midy - bt, midx + br, work.bottom + bb),
            false,
        ),
        SnapZone::BottomRight => (
            mk(midx - bl, midy - bt, work.right + br, work.bottom + bb),
            false,
        ),
    }
}

pub(super) fn pick_edges(px: i32, py: i32, r: &RECT) -> Edges {
    let w = (r.right - r.left).max(1);
    let h = (r.bottom - r.top).max(1);
    let rx = px - r.left;
    let ry = py - r.top;
    // 2x2 quadrants: the grabbed corner follows the cursor. Left half drives
    // the left edge, right half the right edge; top half the top edge,
    // bottom half the bottom edge. Every grab resolves to exactly one
    // horizontal and one vertical edge, so there's no dead center.
    let mut e = Edges::default();
    if rx < w / 2 {
        e.left = true;
    } else {
        e.right = true;
    }
    if ry < h / 2 {
        e.top = true;
    } else {
        e.bottom = true;
    }
    e
}

/// Distance from the cursor to the edge whose free edge is `e` — used to
/// pick which of a window's tileable edges a resize grabs.
pub(super) fn edge_dist(e: SnapKind, px: i32, py: i32, r: &RECT) -> i32 {
    match e {
        SnapKind::Right => px - r.left, // free edge = left
        SnapKind::Left => r.right - px, // free edge = right
        SnapKind::Bottom => py - r.top, // free edge = top
        SnapKind::Top => r.bottom - py, // free edge = bottom
        SnapKind::None => i32::MAX,
    }
}

/// If the window's edge (named by the virtual `SnapKind` whose free edge is
/// that edge) is shared with flush neighbors that *cleanly* tile it — they
/// span the whole edge and none overhangs past it — return those neighbors.
///
/// This is what keeps a quarter-tiled window's width fixed: window 2's bottom
/// edge is cleanly tiled by window 3 (same width, aligned), so it resizes;
/// but its left edge is shared with the full-height window 1, which overhangs
/// below window 2, so that edge stays locked (moving it would misalign
/// window 3). Screen-border edges have no neighbor and are never resizable.
pub(super) unsafe fn clean_tile_edge(
    e: SnapKind,
    rect: &RECT,
    self_hwnd: HWND,
) -> Option<Vec<(HWND, RECT)>> {
    const TOL: i32 = 24;
    let nb = find_neighbors(e, rect, self_hwnd);
    if nb.is_empty() {
        return None;
    }
    // The edge runs along the perpendicular axis: x for a Top/Bottom free
    // edge, y for Left/Right.
    let horizontal = matches!(e, SnapKind::Top | SnapKind::Bottom);
    let (w_start, w_end) = if horizontal {
        (rect.left, rect.right)
    } else {
        (rect.top, rect.bottom)
    };
    let mut cover_start = i32::MAX;
    let mut cover_end = i32::MIN;
    for (_, nr) in &nb {
        let (n_start, n_end) = if horizontal {
            (nr.left, nr.right)
        } else {
            (nr.top, nr.bottom)
        };
        // A neighbor that spills past our edge is shared with other windows
        // too (a full-height window beside a half-height one) — moving it
        // would misalign them, so this edge isn't cleanly tileable.
        if n_start < w_start - TOL || n_end > w_end + TOL {
            return None;
        }
        cover_start = cover_start.min(n_start);
        cover_end = cover_end.max(n_end);
    }
    // The neighbors must span our whole edge, or resizing would leave a gap.
    if cover_start <= w_start + TOL && cover_end >= w_end - TOL {
        Some(nb)
    } else {
        None
    }
}

/// Resolve the target frame (x, y, w, h) for the current cursor position.
pub(super) fn compute_target(
    mode: Mode,
    sx: i32,
    sy: i32,
    r: RECT,
    edges: Edges,
    cx: i32,
    cy: i32,
) -> (i32, i32, i32, i32) {
    let dx = cx - sx;
    let dy = cy - sy;
    match mode {
        Mode::Move => (r.left + dx, r.top + dy, r.right - r.left, r.bottom - r.top),
        Mode::Resize => {
            let mut left = r.left;
            let mut top = r.top;
            let mut right = r.right;
            let mut bottom = r.bottom;
            // Only the selected edges move. For a snapped window the grab
            // sets `edges` to just the single free edge, so the snapped
            // dimension is left untouched automatically.
            if edges.left {
                left = r.left + dx;
            }
            if edges.right {
                right = r.right + dx;
            }
            if edges.top {
                top = r.top + dy;
            }
            if edges.bottom {
                bottom = r.bottom + dy;
            }
            if right - left < MIN_SIZE {
                if edges.left {
                    left = right - MIN_SIZE;
                } else {
                    right = left + MIN_SIZE;
                }
            }
            if bottom - top < MIN_SIZE {
                if edges.top {
                    top = bottom - MIN_SIZE;
                } else {
                    bottom = top + MIN_SIZE;
                }
            }
            (left, top, right - left, bottom - top)
        }
    }
}
