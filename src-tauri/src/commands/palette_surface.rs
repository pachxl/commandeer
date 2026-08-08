//! Native backing material and geometry for the palette window.
//!
//! Onix uses the strongest material each platform can honestly provide:
//! macOS 26's `NSGlassEffectView`, Windows Acrylic (configured by Tauri) clipped
//! to the visual shell, and the transparent web surface on Linux. Older macOS
//! releases retain the existing `NSVisualEffectView` vibrancy fallback.

use std::sync::Mutex;

#[cfg(target_os = "macos")]
const DEFAULT_RADIUS_POINTS: f64 = 12.0;
// Keep these in lockstep with the built-in Onix presentation tokens
// (`--onix-capsule-radius` / `--onix-panel-radius`).
#[cfg(any(target_os = "macos", target_os = "windows", test))]
const ONIX_COMPACT_RADIUS_POINTS: f64 = 33.0;
#[cfg(any(target_os = "macos", target_os = "windows", test))]
const ONIX_EXPANDED_RADIUS_POINTS: f64 = 25.0;

#[derive(Clone, Copy, Debug, PartialEq)]
struct SurfaceConfig {
    onix: bool,
    expanded: bool,
    scale: f64,
}

static SURFACE_CONFIG: Mutex<SurfaceConfig> = Mutex::new(SurfaceConfig {
    onix: false,
    expanded: true,
    scale: 1.0,
});

#[cfg(target_os = "macos")]
#[derive(Clone, Copy, Debug)]
struct MorphBounds {
    start_height: f64,
    target_height: f64,
}

#[cfg(target_os = "macos")]
static MORPH_BOUNDS: Mutex<Option<MorphBounds>> = Mutex::new(None);

fn is_onix(style: &str) -> bool {
    style.trim().eq_ignore_ascii_case("onix")
}

fn sanitize_scale(scale: f64) -> f64 {
    if scale.is_finite() {
        scale.clamp(0.5, 1.5)
    } else {
        1.0
    }
}

fn surface_config(style: &str, expanded: bool, scale: f64) -> SurfaceConfig {
    SurfaceConfig {
        onix: is_onix(style),
        expanded,
        scale: sanitize_scale(scale),
    }
}

#[cfg(any(target_os = "macos", target_os = "windows", test))]
fn onix_radius_points(config: SurfaceConfig) -> f64 {
    let radius = if config.expanded {
        ONIX_EXPANDED_RADIUS_POINTS
    } else {
        ONIX_COMPACT_RADIUS_POINTS
    };
    radius * config.scale
}

#[cfg(any(target_os = "macos", test))]
fn onix_radius_during_morph(
    config: SurfaceConfig,
    current_height: f64,
    start_height: f64,
    target_height: f64,
) -> f64 {
    if !config.expanded || target_height <= start_height {
        return onix_radius_points(config);
    }

    let progress =
        ((current_height - start_height) / (target_height - start_height)).clamp(0.0, 1.0);
    let compact_radius = ONIX_COMPACT_RADIUS_POINTS * config.scale;
    let expanded_radius = ONIX_EXPANDED_RADIUS_POINTS * config.scale;
    compact_radius + (expanded_radius - compact_radius) * progress
}

#[cfg(any(target_os = "macos", test))]
fn should_morph_onix_radius(config: SurfaceConfig, start_height: f64, target_height: f64) -> bool {
    if !config.onix
        || !config.expanded
        || !start_height.is_finite()
        || !target_height.is_finite()
        || target_height <= start_height
    {
        return false;
    }

    let compact_height = 2.0 * ONIX_COMPACT_RADIUS_POINTS * config.scale;
    start_height <= compact_height + 1.0
}

#[cfg(any(target_os = "windows", test))]
fn radius_pixels(config: SurfaceConfig, dpi: u32, height: i32) -> i32 {
    let dpi_scale = f64::from(dpi.max(1)) / 96.0;
    let desired = (onix_radius_points(config) * dpi_scale).round() as i32;
    desired.max(1).min((height.max(2) + 1) / 2)
}

fn remember_config(config: SurfaceConfig) {
    *SURFACE_CONFIG.lock().unwrap_or_else(|e| e.into_inner()) = config;
    #[cfg(target_os = "macos")]
    if !config.onix || !config.expanded {
        *MORPH_BOUNDS.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}

fn remembered_config() -> SurfaceConfig {
    *SURFACE_CONFIG.lock().unwrap_or_else(|e| e.into_inner())
}

/// Record the native frame interval that is about to animate. Resize events
/// use these bounds to keep the public glass corner radius on the exact same
/// interpolation as the window, avoiding a second edge during the bloom.
#[cfg(target_os = "macos")]
pub(crate) fn begin_palette_surface_resize(start_height: f64, target_height: f64) {
    let config = remembered_config();
    let bounds =
        should_morph_onix_radius(config, start_height, target_height).then_some(MorphBounds {
            start_height,
            target_height,
        });
    *MORPH_BOUNDS.lock().unwrap_or_else(|e| e.into_inner()) = bounds;
}

#[cfg(target_os = "macos")]
fn mac_onix_radius_points(window: &tauri::WebviewWindow, config: SurfaceConfig) -> f64 {
    if !config.expanded {
        return onix_radius_points(config);
    }

    let logical_height = window
        .inner_size()
        .ok()
        .and_then(|size| {
            window
                .scale_factor()
                .ok()
                .filter(|factor| factor.is_finite() && *factor > 0.0)
                .map(|factor| f64::from(size.height) / factor)
        })
        .unwrap_or(f64::INFINITY);

    let mut morph = MORPH_BOUNDS.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(bounds) = *morph {
        let radius = onix_radius_during_morph(
            config,
            logical_height,
            bounds.start_height,
            bounds.target_height,
        );
        if logical_height >= bounds.target_height - 0.5 {
            *morph = None;
        }
        return radius;
    }

    // `set_palette_surface(expanded=true)` arrives one render before the
    // ResizeObserver knows the panel height. Retain the capsule curve for that
    // short interval instead of snapping the native material to 25 points.
    let compact_height = 2.0 * ONIX_COMPACT_RADIUS_POINTS * config.scale;
    if logical_height <= compact_height + 1.0 {
        ONIX_COMPACT_RADIUS_POINTS * config.scale
    } else {
        ONIX_EXPANDED_RADIUS_POINTS * config.scale
    }
}

/// Apply a style/state change immediately and remember it for native resize
/// events. This is also the setup-time entry point, before the palette is first
/// shown, so the initial frame never exposes the wrong backing material.
pub fn configure_palette_surface(
    window: &tauri::WebviewWindow,
    style: &str,
    expanded: bool,
    scale: f64,
) -> Result<(), String> {
    let config = surface_config(style, expanded, scale);
    remember_config(config);
    apply_surface(window, config, false)
}

/// Recompute native clipping/radius after the Tauri window changes size. The
/// remembered style keeps this path independent from the frontend resize race.
pub fn refresh_palette_surface(window: &tauri::WebviewWindow) -> Result<(), String> {
    apply_surface(window, remembered_config(), true)
}

#[tauri::command]
pub fn set_palette_surface(
    style: String,
    expanded: bool,
    scale: f64,
    window: tauri::WebviewWindow,
) -> Result<(), String> {
    configure_palette_surface(&window, &style, expanded, scale)
}

#[cfg(target_os = "macos")]
fn apply_surface(
    window: &tauri::WebviewWindow,
    config: SurfaceConfig,
    geometry_only: bool,
) -> Result<(), String> {
    use objc2::runtime::AnyClass;

    // Runtime lookup is intentional. It weakly gates the macOS 26-only class
    // without raising the app's deployment target or hard-linking a symbol that
    // older systems cannot load.
    let glass_class = AnyClass::get("NSGlassEffectView");

    if config.onix {
        if let Some(glass_class) = glass_class {
            return unsafe {
                apply_liquid_glass(window, glass_class, mac_onix_radius_points(window, config))
            };
        }

        if geometry_only {
            return Ok(());
        }
        return apply_vibrancy(window, onix_radius_points(config), false);
    }

    if geometry_only {
        return Ok(());
    }

    if let Some(glass_class) = glass_class {
        unsafe { unwrap_liquid_glass(window, glass_class)? };
    }
    apply_vibrancy(window, DEFAULT_RADIUS_POINTS, false)
}

#[cfg(target_os = "macos")]
fn apply_vibrancy(
    window: &tauri::WebviewWindow,
    radius: f64,
    has_shadow: bool,
) -> Result<(), String> {
    use objc2::runtime::AnyObject;
    use window_vibrancy::{
        apply_vibrancy, clear_vibrancy, NSVisualEffectMaterial, NSVisualEffectState,
    };

    clear_vibrancy(window).map_err(|e| e.to_string())?;
    apply_vibrancy(
        window,
        NSVisualEffectMaterial::HudWindow,
        Some(NSVisualEffectState::Active),
        Some(radius),
    )
    .map_err(|e| e.to_string())?;

    let ns_window = window.ns_window().map_err(|e| e.to_string())? as *mut AnyObject;
    unsafe {
        let _: () = objc2::msg_send![ns_window, setHasShadow: has_shadow];
        let _: () = objc2::msg_send![ns_window, invalidateShadow];
    }
    Ok(())
}

#[cfg(target_os = "macos")]
unsafe fn apply_liquid_glass(
    window: &tauri::WebviewWindow,
    glass_class: &objc2::runtime::AnyClass,
    radius: f64,
) -> Result<(), String> {
    use objc2::runtime::AnyObject;
    use window_vibrancy::clear_vibrancy;

    let ns_window = window.ns_window().map_err(|e| e.to_string())? as *mut AnyObject;
    if ns_window.is_null() {
        return Err("macOS palette NSWindow is null".to_string());
    }

    // Tauri creates a transparent window, but AppKit can retain the default
    // window background behind a replaced contentView. Reassert a truly clear,
    // non-opaque host every time so no rectangular compositor field can show
    // through beyond NSGlassEffectView's rounded public geometry.
    configure_transparent_window(ns_window);

    let root: *mut AnyObject = objc2::msg_send![ns_window, contentView];
    if root.is_null() {
        return Err("macOS palette content view is null".to_string());
    }

    let clip_class = liquid_glass_clip_class()?;
    let root_is_clip: bool = objc2::msg_send![root, isKindOfClass: clip_class];
    if root_is_clip {
        let subviews: *mut AnyObject = objc2::msg_send![root, subviews];
        let glass: *mut AnyObject = objc2::msg_send![subviews, firstObject];
        if glass.is_null() {
            return Err("Onix clip container lost its glass view".to_string());
        }
        let is_glass: bool = objc2::msg_send![glass, isKindOfClass: glass_class];
        if !is_glass {
            return Err("Onix clip container contains an invalid glass view".to_string());
        }
        configure_liquid_glass_clip(root, radius);
        configure_liquid_glass_view(glass, radius);
        let _: () = objc2::msg_send![ns_window, setHasShadow: false];
        let _: () = objc2::msg_send![ns_window, invalidateShadow];
        return Ok(());
    }

    let root_is_glass: bool = objc2::msg_send![root, isKindOfClass: glass_class];
    if root_is_glass {
        configure_liquid_glass_view(root, radius);
        let _: () = objc2::msg_send![ns_window, setHasShadow: false];
        let _: () = objc2::msg_send![ns_window, invalidateShadow];
        return Ok(());
    }

    // `clear_vibrancy` must run while Wry's parent is still the native root;
    // after wrapping, `ns_view()` resolves to the clipping container instead.
    clear_vibrancy(window).map_err(|e| e.to_string())?;

    let first_responder: *mut AnyObject = objc2::msg_send![ns_window, firstResponder];
    let first_responder = retain(first_responder);
    let root = retain(root);

    let bounds: objc2_foundation::NSRect = objc2::msg_send![root, bounds];
    let allocated_clip: *mut AnyObject = objc2::msg_send![clip_class, alloc];
    let clip: *mut AnyObject = objc2::msg_send![allocated_clip, initWithFrame: bounds];
    if clip.is_null() {
        release(root);
        release(first_responder);
        return Err("failed to create Onix clipping container".to_string());
    }
    configure_liquid_glass_clip(clip, radius);

    let allocated: *mut AnyObject = objc2::msg_send![glass_class, alloc];
    let glass: *mut AnyObject = objc2::msg_send![allocated, initWithFrame: bounds];
    if glass.is_null() {
        release(clip);
        release(root);
        release(first_responder);
        return Err("failed to create NSGlassEffectView".to_string());
    }

    // Apple requires the actual content to be assigned through `contentView`;
    // placing glass behind WKWebView as a sibling loses adaptive vibrancy and
    // produces an ordinary backdrop effect instead of Liquid Glass.
    configure_liquid_glass_view(glass, radius);
    let _: () = objc2::msg_send![glass, setAutoresizingMask: 18usize];
    let _: () = objc2::msg_send![glass, setContentView: root];
    let _: () = objc2::msg_send![clip, addSubview: glass];
    let _: () = objc2::msg_send![ns_window, setContentView: clip];

    if !first_responder.is_null() {
        let _: bool = objc2::msg_send![ns_window, makeFirstResponder: first_responder];
    }
    let _: () = objc2::msg_send![ns_window, setHasShadow: false];
    let _: () = objc2::msg_send![ns_window, invalidateShadow];

    // The window/container/glass hierarchy retained all three objects.
    release(glass);
    release(clip);
    release(root);
    release(first_responder);
    Ok(())
}

#[cfg(target_os = "macos")]
fn liquid_glass_clip_class() -> Result<&'static objc2::runtime::AnyClass, String> {
    use objc2::runtime::{AnyClass, ClassBuilder};

    const CLASS_NAME: &str = "CommandeerOnixGlassClipView";
    if let Some(class) = AnyClass::get(CLASS_NAME) {
        return Ok(class);
    }
    let superclass = AnyClass::get("NSView").ok_or("macOS NSView class is unavailable")?;
    ClassBuilder::new(CLASS_NAME, superclass)
        .map(ClassBuilder::register)
        .ok_or_else(|| "failed to create the Onix clipping view class".to_string())
}

#[cfg(target_os = "macos")]
unsafe fn configure_liquid_glass_clip(clip: *mut objc2::runtime::AnyObject, radius: f64) {
    use objc2::runtime::{AnyClass, AnyObject};

    let _: () = objc2::msg_send![clip, setClipsToBounds: true];
    let _: () = objc2::msg_send![clip, setWantsLayer: true];
    let layer: *mut AnyObject = objc2::msg_send![clip, layer];
    if !layer.is_null() {
        // Window resize notifications already provide the interpolation.
        // Suppress CALayer's separate implicit animation so the mask never
        // trails the live glass curve or makes the anchored top corners pulse.
        let transaction = AnyClass::get("CATransaction");
        if let Some(transaction) = transaction {
            let _: () = objc2::msg_send![transaction, begin];
            let _: () = objc2::msg_send![transaction, setDisableActions: true];
        }
        let _: () = objc2::msg_send![layer, setCornerRadius: radius];
        let _: () = objc2::msg_send![layer, setMasksToBounds: true];
        if let Some(transaction) = transaction {
            let _: () = objc2::msg_send![transaction, commit];
        }
    }
}

#[cfg(target_os = "macos")]
unsafe fn configure_liquid_glass_view(glass: *mut objc2::runtime::AnyObject, radius: f64) {
    use objc2::runtime::AnyObject;

    let _: () = objc2::msg_send![glass, setCornerRadius: radius];
    // Regular retains the real dynamic lensing while adapting its contrast to
    // both the small search capsule and the larger text-dense panel. Clear
    // over-magnifies 1x desktop samples and takes on bright backdrop colours.
    let _: () = objc2::msg_send![glass, setStyle: 0isize]; // Regular glass

    // Newer macOS 26 builds expose interactive Liquid Glass response. Keep the
    // call runtime-gated so the same binary remains valid on the original 26.0
    // API and on the pre-26 vibrancy fallback.
    let interactive_selector = objc2::sel!(setEffectIsInteractive:);
    let supports_interactive: bool =
        objc2::msg_send![glass, respondsToSelector: interactive_selector];
    if supports_interactive {
        let _: () = objc2::msg_send![glass, setEffectIsInteractive: true];
    }

    // Keep the native material itself untinted. On some wallpapers even a very
    // low-alpha tint reveals the rectangular sampling host outside the public
    // rounded glass. The clipped WebGL field supplies the smoked absorption.
    let nil: *mut AnyObject = std::ptr::null_mut();
    let _: () = objc2::msg_send![glass, setTintColor: nil];
}

#[cfg(target_os = "macos")]
unsafe fn configure_transparent_window(ns_window: *mut objc2::runtime::AnyObject) {
    use objc2::runtime::{AnyClass, AnyObject};

    let _: () = objc2::msg_send![ns_window, setOpaque: false];
    if let Some(color_class) = AnyClass::get("NSColor") {
        let clear: *mut AnyObject = objc2::msg_send![color_class, clearColor];
        if !clear.is_null() {
            let _: () = objc2::msg_send![ns_window, setBackgroundColor: clear];
        }
    }
}

#[cfg(target_os = "macos")]
unsafe fn unwrap_liquid_glass(
    window: &tauri::WebviewWindow,
    glass_class: &objc2::runtime::AnyClass,
) -> Result<(), String> {
    use objc2::runtime::AnyObject;

    let ns_window = window.ns_window().map_err(|e| e.to_string())? as *mut AnyObject;
    let glass: *mut AnyObject = objc2::msg_send![ns_window, contentView];
    if glass.is_null() {
        return Err("macOS palette content view is null".to_string());
    }

    let root = glass;
    let clip_class = liquid_glass_clip_class()?;
    let root_is_clip: bool = objc2::msg_send![root, isKindOfClass: clip_class];
    let glass = if root_is_clip {
        let subviews: *mut AnyObject = objc2::msg_send![root, subviews];
        let child: *mut AnyObject = objc2::msg_send![subviews, firstObject];
        if child.is_null() {
            return Err("Onix clip container lost its glass view".to_string());
        }
        child
    } else {
        root
    };

    let is_glass: bool = objc2::msg_send![glass, isKindOfClass: glass_class];
    if !is_glass {
        return Ok(());
    }

    let content: *mut AnyObject = objc2::msg_send![glass, contentView];
    if content.is_null() {
        return Err("NSGlassEffectView lost its palette content".to_string());
    }

    let first_responder: *mut AnyObject = objc2::msg_send![ns_window, firstResponder];
    let first_responder = retain(first_responder);
    let content = retain(content);

    let nil: *mut AnyObject = std::ptr::null_mut();
    let _: () = objc2::msg_send![glass, setContentView: nil];
    let _: () = objc2::msg_send![ns_window, setContentView: content];
    if !first_responder.is_null() {
        let _: bool = objc2::msg_send![ns_window, makeFirstResponder: first_responder];
    }

    release(content);
    release(first_responder);
    Ok(())
}

#[cfg(target_os = "macos")]
unsafe fn retain(object: *mut objc2::runtime::AnyObject) -> *mut objc2::runtime::AnyObject {
    if object.is_null() {
        object
    } else {
        objc2::msg_send![object, retain]
    }
}

#[cfg(target_os = "macos")]
unsafe fn release(object: *mut objc2::runtime::AnyObject) {
    if !object.is_null() {
        let _: () = objc2::msg_send![object, release];
    }
}

#[cfg(target_os = "windows")]
fn apply_surface(
    window: &tauri::WebviewWindow,
    config: SurfaceConfig,
    _geometry_only: bool,
) -> Result<(), String> {
    use windows::Win32::Foundation::{HWND, RECT};
    use windows::Win32::Graphics::Gdi::{
        CreateRoundRectRgn, DeleteObject, SetWindowRgn, HGDIOBJ, HRGN,
    };
    use windows::Win32::UI::HiDpi::GetDpiForWindow;
    use windows::Win32::UI::WindowsAndMessaging::GetClientRect;

    let raw = window.hwnd().map_err(|e| e.to_string())?;
    let hwnd = HWND(raw.0 as *mut _);

    unsafe {
        if !config.onix {
            if SetWindowRgn(hwnd, HRGN(std::ptr::null_mut()), true) == 0 {
                return Err("failed to clear the Windows palette region".to_string());
            }
            return Ok(());
        }

        let mut rect = RECT::default();
        GetClientRect(hwnd, &mut rect).map_err(|e| e.to_string())?;
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        if width <= 0 || height <= 0 {
            return Ok(());
        }

        let dpi = GetDpiForWindow(hwnd).max(96);
        let radius = radius_pixels(config, dpi, height);
        let diameter = radius.saturating_mul(2);
        // GDI excludes the lower/right edge; one extra pixel avoids a clipped
        // antialias fringe while still matching the physical client bounds.
        let region = CreateRoundRectRgn(0, 0, width + 1, height + 1, diameter, diameter);
        if region.0.is_null() {
            return Err("failed to create the Windows palette region".to_string());
        }

        if SetWindowRgn(hwnd, region, true) == 0 {
            // Ownership transfers to the system only after a successful call.
            let _ = DeleteObject(HGDIOBJ(region.0));
            return Err("failed to apply the Windows palette region".to_string());
        }
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn apply_surface(
    _window: &tauri::WebviewWindow,
    _config: SurfaceConfig,
    _geometry_only: bool,
) -> Result<(), String> {
    // Wayland does not expose portable backdrop sampling or native custom
    // corner geometry. The transparent layer-shell surface lets the frontend's
    // modeled optical shell and alpha corners render without a toplevel frame.
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_onix_selects_the_native_glass_surface() {
        assert!(is_onix("Onix"));
        assert!(is_onix("  onIX "));
        assert!(!is_onix("Default"));
        assert!(!is_onix(""));
    }

    #[test]
    fn scale_is_finite_and_bounded() {
        assert_eq!(sanitize_scale(f64::NAN), 1.0);
        assert_eq!(sanitize_scale(f64::INFINITY), 1.0);
        assert_eq!(sanitize_scale(0.1), 0.5);
        assert_eq!(sanitize_scale(2.0), 1.5);
        assert_eq!(sanitize_scale(1.25), 1.25);
    }

    #[test]
    fn compact_surface_is_rounder_than_expanded_panel() {
        let compact = surface_config("Onix", false, 1.0);
        let expanded = surface_config("Onix", true, 1.0);
        assert_eq!(onix_radius_points(compact), 33.0);
        assert_eq!(onix_radius_points(expanded), 25.0);
    }

    #[test]
    fn mac_radius_tracks_the_native_expansion_geometry() {
        let expanded = surface_config("Onix", true, 1.0);
        assert_eq!(onix_radius_during_morph(expanded, 66.0, 66.0, 426.0), 33.0);
        assert_eq!(onix_radius_during_morph(expanded, 246.0, 66.0, 426.0), 29.0);
        assert_eq!(onix_radius_during_morph(expanded, 426.0, 66.0, 426.0), 25.0);
    }

    #[test]
    fn radius_morph_only_arms_for_the_capsule_to_panel_transition() {
        let expanded = surface_config("Onix", true, 1.0);
        let compact = surface_config("Onix", false, 1.0);
        let default = surface_config("Default", true, 1.0);

        assert!(should_morph_onix_radius(expanded, 66.0, 426.0));
        assert!(!should_morph_onix_radius(expanded, 260.0, 426.0));
        assert!(!should_morph_onix_radius(expanded, 426.0, 260.0));
        assert!(!should_morph_onix_radius(compact, 66.0, 426.0));
        assert!(!should_morph_onix_radius(default, 66.0, 426.0));
    }

    #[test]
    fn windows_radius_tracks_dpi_and_never_exceeds_half_height() {
        let compact = surface_config("Onix", false, 1.0);
        let expanded = surface_config("Onix", true, 1.5);
        assert_eq!(radius_pixels(compact, 144, 100), 50);
        assert_eq!(radius_pixels(expanded, 192, 400), 75);
        assert_eq!(radius_pixels(compact, 96, 40), 20);
    }
}
