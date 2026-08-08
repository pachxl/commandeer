use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

// --- Framework types (opaque pointers) ---
type CFTypeRef = *const c_void;
type CFStringRef = *const c_void;
type CFRunLoopRef = *mut c_void;
type CFRunLoopSourceRef = *mut c_void;
type CFMachPortRef = *mut c_void;
type AXUIElementRef = *const c_void;
type AXValueRef = *const c_void;
type CGEventRef = *mut c_void;
type CGEventTapProxy = *mut c_void;

#[repr(C)]
#[derive(Clone, Copy)]
struct CGPoint {
    x: f64,
    y: f64,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct CGSize {
    width: f64,
    height: f64,
}

type CGEventTapCallBack = unsafe extern "C" fn(
    proxy: CGEventTapProxy,
    etype: u32,
    event: CGEventRef,
    user_info: *mut c_void,
) -> CGEventRef;

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventTapCreate(
        tap: u32,
        place: u32,
        options: u32,
        events_of_interest: u64,
        callback: CGEventTapCallBack,
        user_info: *mut c_void,
    ) -> CFMachPortRef;
    fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
    fn CGEventGetLocation(event: CGEventRef) -> CGPoint;
    fn CGEventGetType(event: CGEventRef) -> u32;
    fn CGEventGetFlags(event: CGEventRef) -> u64;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    static kCFAllocatorDefault: CFTypeRef;
    static kCFRunLoopCommonModes: CFStringRef;
    fn CFStringCreateWithCString(
        alloc: CFTypeRef,
        cstr: *const std::os::raw::c_char,
        encoding: u32,
    ) -> CFStringRef;
    fn CFMachPortCreateRunLoopSource(
        alloc: CFTypeRef,
        port: CFMachPortRef,
        order: isize,
    ) -> CFRunLoopSourceRef;
    fn CFRunLoopGetCurrent() -> CFRunLoopRef;
    fn CFRunLoopAddSource(rl: CFRunLoopRef, source: CFRunLoopSourceRef, mode: CFStringRef);
    fn CFRunLoopRun();
    fn CFRunLoopStop(rl: CFRunLoopRef);
    fn CFRelease(cf: CFTypeRef);
}

// The kAX* constants are CFSTR("...") macros in the SDK headers, not
// exported linkable symbols (verified: they're absent from
// HIServices.tbd on modern macOS). Build the CFStringRefs at runtime
// instead and cache them for the process lifetime.
#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXUIElementCreateSystemWide() -> AXUIElementRef;
    fn AXUIElementCopyElementAtPosition(
        app: AXUIElementRef,
        x: f32,
        y: f32,
        element: *mut AXUIElementRef,
    ) -> i32;
    fn AXUIElementCopyAttributeValue(
        element: AXUIElementRef,
        attr: CFStringRef,
        value: *mut CFTypeRef,
    ) -> i32;
    fn AXUIElementSetAttributeValue(
        element: AXUIElementRef,
        attr: CFStringRef,
        value: CFTypeRef,
    ) -> i32;
    fn AXUIElementPerformAction(element: AXUIElementRef, action: CFStringRef) -> i32;
    fn AXValueCreate(the_type: u32, value_ptr: *const c_void) -> AXValueRef;
    fn AXValueGetValue(value: AXValueRef, the_type: u32, value_ptr: *mut c_void) -> bool;
}

// kCFStringEncodingUTF8
const K_CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;

// CGEventType
const K_LEFT_DOWN: u32 = 1;
const K_LEFT_UP: u32 = 2;
const K_RIGHT_DOWN: u32 = 3;
const K_RIGHT_UP: u32 = 4;
const K_LEFT_DRAG: u32 = 6;
const K_RIGHT_DRAG: u32 = 7;
const K_TAP_DISABLED_TIMEOUT: u32 = 0xFFFF_FFFE;
const K_TAP_DISABLED_USERINPUT: u32 = 0xFFFF_FFFF;

// kCGEventFlagMaskAlternate (the Option / Alt modifier)
const FLAG_ALT: u64 = 0x0008_0000;

// AXValueType
const K_AXVALUE_CGPOINT: u32 = 1;
const K_AXVALUE_CGSIZE: u32 = 2;

const MIN_SIZE: f64 = 120.0;

/// Create a CFStringRef from a literal and cache it for the process
/// lifetime (CFStringRefs are immutable and never need releasing).
fn ax_cfstr(literal: &'static str) -> CFStringRef {
    use std::sync::OnceLock;
    // CFStringRef is *const c_void, which isn't Send/Sync by default.
    // CFStrings are immutable and thread-safe, so sharing the pointer is
    // sound — wrap it in a Send+Sync newtype to satisfy OnceLock.
    struct CFStr(CFStringRef);
    unsafe impl Send for CFStr {}
    unsafe impl Sync for CFStr {}
    static POS: OnceLock<CFStr> = OnceLock::new();
    static SIZE: OnceLock<CFStr> = OnceLock::new();
    static WIN: OnceLock<CFStr> = OnceLock::new();
    static TOP: OnceLock<CFStr> = OnceLock::new();
    static RAISE: OnceLock<CFStr> = OnceLock::new();
    macro_rules! get {
        ($cell:expr) => {{
            $cell
                .get_or_init(|| {
                    CFStr(unsafe {
                        CFStringCreateWithCString(
                            kCFAllocatorDefault,
                            literal.as_ptr() as *const std::os::raw::c_char,
                            K_CF_STRING_ENCODING_UTF8,
                        )
                    })
                })
                .0
        }};
    }
    match literal {
        "AXPosition" => get!(POS),
        "AXSize" => get!(SIZE),
        "AXWindow" => get!(WIN),
        "AXTopLevelUIElement" => get!(TOP),
        "AXRaise" => get!(RAISE),
        _ => std::ptr::null(),
    }
}
fn k_position() -> CFStringRef {
    ax_cfstr("AXPosition")
}
fn k_size() -> CFStringRef {
    ax_cfstr("AXSize")
}
fn k_window() -> CFStringRef {
    ax_cfstr("AXWindow")
}
fn k_toplevel() -> CFStringRef {
    ax_cfstr("AXTopLevelUIElement")
}
fn k_raise() -> CFStringRef {
    ax_cfstr("AXRaise")
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Move,
    Resize,
}
#[derive(Clone, Copy, Default)]
struct Edges {
    left: bool,
    right: bool,
    top: bool,
    bottom: bool,
}

struct DragState {
    active: bool,
    mode: Mode,
    window: usize, // retained AXUIElementRef; 0 = none
    start_cursor: CGPoint,
    origin: CGPoint,
    size: CGSize,
    edges: Edges,
}
impl Default for DragState {
    fn default() -> Self {
        Self {
            active: false,
            mode: Mode::Move,
            window: 0,
            start_cursor: CGPoint { x: 0.0, y: 0.0 },
            origin: CGPoint { x: 0.0, y: 0.0 },
            size: CGSize {
                width: 0.0,
                height: 0.0,
            },
            edges: Edges::default(),
        }
    }
}

static STATE: OnceLock<Mutex<DragState>> = OnceLock::new();
fn state() -> &'static Mutex<DragState> {
    STATE.get_or_init(|| Mutex::new(DragState::default()))
}

static ACTIVE: AtomicBool = AtomicBool::new(false);
static RUNNING: AtomicBool = AtomicBool::new(false);
static RUNLOOP: AtomicUsize = AtomicUsize::new(0);
static TAP: AtomicUsize = AtomicUsize::new(0);

pub fn enable() -> Result<(), String> {
    if RUNNING.load(Ordering::Relaxed) {
        return Ok(());
    }
    unsafe {
        if !AXIsProcessTrusted() {
            return Err("Alt-drag needs the Accessibility permission: System Settings → Privacy & Security → Accessibility".into());
        }
    }
    RUNNING.store(true, Ordering::Relaxed);
    std::thread::Builder::new()
        .name("window-drag-tap".into())
        .spawn(|| unsafe {
            let mask: u64 = (1u64 << K_LEFT_DOWN)
                | (1u64 << K_LEFT_UP)
                | (1u64 << K_LEFT_DRAG)
                | (1u64 << K_RIGHT_DOWN)
                | (1u64 << K_RIGHT_UP)
                | (1u64 << K_RIGHT_DRAG);
            // kCGHIDEventTap=0, kCGHeadInsertEventTap=0, kCGEventTapOptionDefault=0
            let tap = CGEventTapCreate(0, 0, 0, mask, tap_callback, std::ptr::null_mut());
            if tap.is_null() {
                RUNNING.store(false, Ordering::Relaxed);
                eprintln!("window_drag: CGEventTapCreate returned null (permission?)");
                return;
            }
            TAP.store(tap as usize, Ordering::Relaxed);
            let source = CFMachPortCreateRunLoopSource(std::ptr::null(), tap, 0);
            let rl = CFRunLoopGetCurrent();
            RUNLOOP.store(rl as usize, Ordering::Relaxed);
            CFRunLoopAddSource(rl, source, kCFRunLoopCommonModes);
            CGEventTapEnable(tap, true);
            CFRunLoopRun(); // returns when disable() calls CFRunLoopStop

            CGEventTapEnable(tap, false);
            CFRelease(source as CFTypeRef);
            CFRelease(tap as CFTypeRef);
            TAP.store(0, Ordering::Relaxed);
            RUNLOOP.store(0, Ordering::Relaxed);
            RUNNING.store(false, Ordering::Relaxed);
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn disable() -> Result<(), String> {
    if let Ok(mut st) = state().lock() {
        if st.window != 0 {
            unsafe { CFRelease(st.window as CFTypeRef) };
            st.window = 0;
        }
        st.active = false;
    }
    ACTIVE.store(false, Ordering::Relaxed);
    let rl = RUNLOOP.load(Ordering::Relaxed);
    if rl != 0 {
        unsafe { CFRunLoopStop(rl as CFRunLoopRef) };
    }
    Ok(())
}

unsafe extern "C" fn tap_callback(
    _proxy: CGEventTapProxy,
    etype: u32,
    event: CGEventRef,
    _info: *mut c_void,
) -> CGEventRef {
    // The OS disables the tap if our callback is slow or on some input;
    // re-enable and pass the event through.
    if etype == K_TAP_DISABLED_TIMEOUT || etype == K_TAP_DISABLED_USERINPUT {
        let tap = TAP.load(Ordering::Relaxed);
        if tap != 0 {
            CGEventTapEnable(tap as CFMachPortRef, true);
        }
        return event;
    }

    let loc = CGEventGetLocation(event);
    let alt = (CGEventGetFlags(event) & FLAG_ALT) != 0;
    let _ = CGEventGetType; // (type comes in via `etype`)

    let mut st = match state().lock() {
        Ok(g) => g,
        Err(_) => return event,
    };

    match etype {
        K_LEFT_DOWN | K_RIGHT_DOWN => {
            if st.active {
                return std::ptr::null_mut();
            }
            if !alt {
                return event;
            }
            let mode = if etype == K_LEFT_DOWN {
                Mode::Move
            } else {
                Mode::Resize
            };
            if let Some(win) = window_at(loc.x, loc.y) {
                if let (Some(origin), Some(size)) =
                    (read_point(win, k_position()), read_size(win, k_size()))
                {
                    st.active = true;
                    st.mode = mode;
                    st.window = win as usize;
                    st.start_cursor = loc;
                    st.origin = origin;
                    st.size = size;
                    if mode == Mode::Resize {
                        st.edges = pick_edges(loc, origin, size);
                    }
                    // Raise the grabbed window to the front, matching the
                    // Windows arm's raise-on-grab. Best-effort: a failure
                    // (e.g. the element doesn't implement AXRaise) doesn't
                    // abort the drag.
                    let _ = AXUIElementPerformAction(win, k_raise());
                    ACTIVE.store(true, Ordering::Relaxed);
                    return std::ptr::null_mut(); // consume the click
                }
                CFRelease(win as CFTypeRef);
            }
            event
        }
        K_LEFT_DRAG | K_RIGHT_DRAG => {
            if !st.active {
                return event;
            }
            apply(&st, loc);
            std::ptr::null_mut()
        }
        K_LEFT_UP => end(&mut st, Mode::Move, event),
        K_RIGHT_UP => end(&mut st, Mode::Resize, event),
        _ => event,
    }
}

unsafe fn end(st: &mut DragState, mode: Mode, event: CGEventRef) -> CGEventRef {
    if st.active && st.mode == mode {
        if st.window != 0 {
            CFRelease(st.window as CFTypeRef);
            st.window = 0;
        }
        st.active = false;
        ACTIVE.store(false, Ordering::Relaxed);
        std::ptr::null_mut()
    } else {
        event
    }
}

unsafe fn copy_attr(el: AXUIElementRef, attr: CFStringRef) -> CFTypeRef {
    let mut val: CFTypeRef = std::ptr::null();
    if AXUIElementCopyAttributeValue(el, attr, &mut val) == 0 {
        val
    } else {
        std::ptr::null()
    }
}

/// The window under the cursor, retained (+1). Caller must CFRelease.
unsafe fn window_at(px: f64, py: f64) -> Option<AXUIElementRef> {
    let sys = AXUIElementCreateSystemWide();
    if sys.is_null() {
        return None;
    }
    let mut el: AXUIElementRef = std::ptr::null();
    let err = AXUIElementCopyElementAtPosition(sys, px as f32, py as f32, &mut el);
    CFRelease(sys as CFTypeRef);
    if err != 0 || el.is_null() {
        return None;
    }
    // The hit element is usually a control; climb to its window.
    let mut win = copy_attr(el, k_window());
    if win.is_null() {
        win = copy_attr(el, k_toplevel());
    }
    CFRelease(el as CFTypeRef);
    if win.is_null() {
        None
    } else {
        Some(win as AXUIElementRef)
    }
}

unsafe fn read_point(el: AXUIElementRef, attr: CFStringRef) -> Option<CGPoint> {
    let v = copy_attr(el, attr);
    if v.is_null() {
        return None;
    }
    let mut p = CGPoint { x: 0.0, y: 0.0 };
    let ok = AXValueGetValue(
        v as AXValueRef,
        K_AXVALUE_CGPOINT,
        &mut p as *mut _ as *mut c_void,
    );
    CFRelease(v);
    if ok {
        Some(p)
    } else {
        None
    }
}

unsafe fn read_size(el: AXUIElementRef, attr: CFStringRef) -> Option<CGSize> {
    let v = copy_attr(el, attr);
    if v.is_null() {
        return None;
    }
    let mut s = CGSize {
        width: 0.0,
        height: 0.0,
    };
    let ok = AXValueGetValue(
        v as AXValueRef,
        K_AXVALUE_CGSIZE,
        &mut s as *mut _ as *mut c_void,
    );
    CFRelease(v);
    if ok {
        Some(s)
    } else {
        None
    }
}

unsafe fn set_point(el: AXUIElementRef, attr: CFStringRef, p: CGPoint) {
    let v = AXValueCreate(K_AXVALUE_CGPOINT, &p as *const _ as *const c_void);
    if !v.is_null() {
        AXUIElementSetAttributeValue(el, attr, v as CFTypeRef);
        CFRelease(v as CFTypeRef);
    }
}

unsafe fn set_size(el: AXUIElementRef, attr: CFStringRef, s: CGSize) {
    let v = AXValueCreate(K_AXVALUE_CGSIZE, &s as *const _ as *const c_void);
    if !v.is_null() {
        AXUIElementSetAttributeValue(el, attr, v as CFTypeRef);
        CFRelease(v as CFTypeRef);
    }
}

fn pick_edges(cursor: CGPoint, origin: CGPoint, size: CGSize) -> Edges {
    let w = size.width.max(1.0);
    let h = size.height.max(1.0);
    let rx = cursor.x - origin.x;
    let ry = cursor.y - origin.y;
    // 2x2 quadrants: the grabbed corner follows the cursor (see the Windows
    // pick_edges). Every grab resolves to one horizontal + one vertical edge.
    let mut e = Edges::default();
    if rx < w / 2.0 {
        e.left = true;
    } else {
        e.right = true;
    }
    if ry < h / 2.0 {
        e.top = true;
    } else {
        e.bottom = true;
    }
    e
}

unsafe fn apply(st: &DragState, cursor: CGPoint) {
    let win = st.window as AXUIElementRef;
    if win.is_null() {
        return;
    }
    let dx = cursor.x - st.start_cursor.x;
    let dy = cursor.y - st.start_cursor.y;
    match st.mode {
        Mode::Move => {
            set_point(
                win,
                k_position(),
                CGPoint {
                    x: st.origin.x + dx,
                    y: st.origin.y + dy,
                },
            );
        }
        Mode::Resize => {
            let mut left = st.origin.x;
            let mut top = st.origin.y;
            let mut right = st.origin.x + st.size.width;
            let mut bottom = st.origin.y + st.size.height;
            if st.edges.left {
                left = st.origin.x + dx;
            }
            if st.edges.right {
                right = st.origin.x + st.size.width + dx;
            }
            if st.edges.top {
                top = st.origin.y + dy;
            }
            if st.edges.bottom {
                bottom = st.origin.y + st.size.height + dy;
            }
            if right - left < MIN_SIZE {
                if st.edges.left {
                    left = right - MIN_SIZE;
                } else {
                    right = left + MIN_SIZE;
                }
            }
            if bottom - top < MIN_SIZE {
                if st.edges.top {
                    top = bottom - MIN_SIZE;
                } else {
                    bottom = top + MIN_SIZE;
                }
            }
            // Move the origin first so AX doesn't clamp the new size against
            // the old frame when a top/left edge is being dragged.
            set_point(win, k_position(), CGPoint { x: left, y: top });
            set_size(
                win,
                k_size(),
                CGSize {
                    width: right - left,
                    height: bottom - top,
                },
            );
        }
    }
}
