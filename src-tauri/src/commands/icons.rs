//! Shell icons for file-search results. Icons are resolved lazily and cached
//! per extension/path so repeat lookups are free.
//!
//! Backends:
//! - Windows: SHGetFileInfoW / IShellItemImageFactory → 32×32 PNG data URL.
//! - macOS: NSWorkspace.iconForFile: → PNG data URL (via TIFF → bitmap rep).
//! - Linux: freedesktop theme and `.desktop` resolution lives in desktop.rs.

/// Data-URL icon for a path, or None if the shell has nothing for it.
/// Uses file extension attributes for fast lookup; best for generic files.
#[cfg(target_os = "windows")]
pub fn icon_for_path(path: &str) -> Option<String> {
    win::icon_for_path(path)
}

#[cfg(target_os = "macos")]
pub fn icon_for_path(path: &str) -> Option<String> {
    mac::icon_for_path(path)
}

/// Point the macOS icon cache at its on-disk backing file (the app cache dir).
/// Call once at startup, before the first lookup, so resolved icons survive
/// restarts instead of being re-resolved (NSWorkspace is ~175 ms/icon cold).
#[cfg(target_os = "macos")]
pub fn set_cache_dir(dir: std::path::PathBuf) {
    mac::set_cache_dir(dir);
}

/// Resolve every installed app's icon into the (disk-persisted) cache once, so
/// the first open of the Apps folder paints real icons. Run on a background
/// thread; near-instant after the first run (served from disk).
#[cfg(target_os = "macos")]
pub fn warm_app_icons(paths: Vec<String>) {
    mac::warm_app_icons(paths);
}

// Linux has no icon_for_path: file-search icons resolve through the
// .desktop-theme lookup in search.rs's linux_icons instead.

/// Data-URL icon for a specific file path, resolving shortcuts and app icons.
/// Cached per path; slower than icon_for_path but accurate for shortcuts.
#[cfg(target_os = "windows")]
pub fn icon_for_file(path: &str) -> Option<String> {
    win::icon_for_file(path)
}

/// Data-URL icon for a shell parsing name (e.g. "shell:AppsFolder\\<id>").
#[cfg(target_os = "windows")]
pub fn icon_for_shell_item(parse_path: &str) -> Option<String> {
    win::icon_for_shell_item(parse_path)
}

// ── Windows backend ──────────────────────────────────────────────────────────

#[cfg(target_os = "windows")]
mod win {
    use super::encode_png;
    use std::collections::HashMap;
    use std::path::Path;
    use std::sync::{OnceLock, RwLock};

    fn cache() -> &'static RwLock<HashMap<String, Option<String>>> {
        static CACHE: OnceLock<RwLock<HashMap<String, Option<String>>>> = OnceLock::new();
        CACHE.get_or_init(|| RwLock::new(HashMap::new()))
    }

    fn file_cache() -> &'static RwLock<HashMap<String, Option<String>>> {
        static CACHE: OnceLock<RwLock<HashMap<String, Option<String>>>> = OnceLock::new();
        CACHE.get_or_init(|| RwLock::new(HashMap::new()))
    }

    pub fn icon_for_path(path: &str) -> Option<String> {
        let p = Path::new(path);
        let is_dir = p.is_dir();
        let key = if is_dir {
            "\u{0}folder".to_string()
        } else {
            p.extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_lowercase())
                .unwrap_or_default()
        };
        // Use read lock for cache lookup (fast path)
        if let Ok(guard) = cache().read() {
            if let Some(hit) = guard.get(&key) {
                return hit.clone();
            }
        }
        let icon = shell_icon(path, is_dir, true);
        // Use write lock for cache insert
        if let Ok(mut guard) = cache().write() {
            guard.insert(key, icon.clone());
        }
        icon
    }

    pub fn icon_for_file(path: &str) -> Option<String> {
        let key = path.to_lowercase();
        // Use read lock for cache lookup
        if let Ok(guard) = file_cache().read() {
            if let Some(hit) = guard.get(&key) {
                return hit.clone();
            }
        }
        let resolved = if path.to_lowercase().ends_with(".lnk") {
            resolve_lnk_icon_source(path).unwrap_or_else(|| path.to_string())
        } else {
            path.to_string()
        };
        let icon = shell_icon(&resolved, Path::new(&resolved).is_dir(), false);
        // Use write lock for cache insert
        if let Ok(mut guard) = file_cache().write() {
            guard.insert(key, icon.clone());
        }
        icon
    }

    pub fn icon_for_shell_item(parse_path: &str) -> Option<String> {
        let key = parse_path.to_lowercase();
        // Use read lock for cache lookup
        if let Ok(guard) = file_cache().read() {
            if let Some(hit) = guard.get(&key) {
                return hit.clone();
            }
        }
        let icon = shell_item_icon(parse_path);
        // Use write lock for cache insert
        if let Ok(mut guard) = file_cache().write() {
            guard.insert(key, icon.clone());
        }
        icon
    }

    fn shell_item_icon(parse_path: &str) -> Option<String> {
        use windows::core::{Interface, PCWSTR};
        use windows::Win32::Foundation::SIZE;
        use windows::Win32::Graphics::Gdi::DeleteObject;
        use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
        use windows::Win32::UI::Shell::{
            IShellItem, IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_ICONONLY,
        };

        let wide: Vec<u16> = parse_path
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let item: IShellItem = SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None).ok()?;
            let factory: IShellItemImageFactory = item.cast().ok()?;
            let hbm = factory
                .GetImage(SIZE { cx: 32, cy: 32 }, SIIGBF_ICONONLY)
                .ok()?;
            let png = hbitmap_to_png(hbm);
            let _ = DeleteObject(hbm);
            png.map(|bytes| {
                format!(
                    "data:image/png;base64,{}",
                    super::super::fs::base64_encode(&bytes)
                )
            })
        }
    }

    fn resolve_lnk_icon_source(path: &str) -> Option<String> {
        use windows::core::{Interface, PCWSTR};
        use windows::Win32::System::Com::{
            CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, STGM,
        };
        use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};

        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let shell_link: IShellLinkW =
                CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).ok()?;
            let persist_file: windows::Win32::System::Com::IPersistFile = shell_link.cast().ok()?;
            let wide: Vec<u16> = path
                .replace('/', "\\")
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
            persist_file.Load(PCWSTR(wide.as_ptr()), STGM(0)).ok()?;

            let mut icon_path = [0u16; 260];
            let mut icon_index = 0i32;
            if shell_link
                .GetIconLocation(&mut icon_path, &mut icon_index)
                .is_ok()
            {
                let len = icon_path
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(icon_path.len());
                let icon = String::from_utf16_lossy(&icon_path[..len]);
                if !icon.is_empty() {
                    return Some(icon);
                }
            }

            let mut target = [0u16; 260];
            shell_link
                .GetPath(&mut target, std::ptr::null_mut(), 0)
                .ok()?;
            let len = target.iter().position(|&c| c == 0).unwrap_or(target.len());
            let resolved = String::from_utf16_lossy(&target[..len]);
            if resolved.is_empty() {
                None
            } else {
                Some(resolved)
            }
        }
    }

    fn shell_icon(path: &str, is_dir: bool, use_file_attributes: bool) -> Option<String> {
        use windows::core::PCWSTR;
        use windows::Win32::Storage::FileSystem::{
            FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL,
        };
        use windows::Win32::UI::Shell::{
            SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON, SHGFI_USEFILEATTRIBUTES,
        };
        use windows::Win32::UI::WindowsAndMessaging::DestroyIcon;

        let wide: Vec<u16> = path
            .replace('/', "\\")
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let attrs = if is_dir {
            FILE_ATTRIBUTE_DIRECTORY
        } else {
            FILE_ATTRIBUTE_NORMAL
        };

        unsafe {
            let mut info = SHFILEINFOW::default();
            let mut flags = SHGFI_ICON | SHGFI_LARGEICON;
            if use_file_attributes {
                flags |= SHGFI_USEFILEATTRIBUTES;
            }
            let ok = SHGetFileInfoW(
                PCWSTR(wide.as_ptr()),
                attrs,
                Some(&mut info),
                std::mem::size_of::<SHFILEINFOW>() as u32,
                flags,
            );
            if ok == 0 || info.hIcon.is_invalid() {
                return None;
            }
            let png = hicon_to_png(info.hIcon);
            let _ = DestroyIcon(info.hIcon);
            png.map(|bytes| {
                format!(
                    "data:image/png;base64,{}",
                    super::super::fs::base64_encode(&bytes)
                )
            })
        }
    }

    unsafe fn hicon_to_png(
        hicon: windows::Win32::UI::WindowsAndMessaging::HICON,
    ) -> Option<Vec<u8>> {
        use windows::Win32::Graphics::Gdi::{
            DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO,
            BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
        };
        use windows::Win32::UI::WindowsAndMessaging::{GetIconInfo, ICONINFO};

        let mut info = ICONINFO::default();
        GetIconInfo(hicon, &mut info).ok()?;

        let mut bm = BITMAP::default();
        let has_color = !info.hbmColor.is_invalid();
        let src = if has_color {
            info.hbmColor
        } else {
            info.hbmMask
        };
        let got = GetObjectW(
            src,
            std::mem::size_of::<BITMAP>() as i32,
            Some(&mut bm as *mut _ as *mut _),
        );
        let (width, height) = (bm.bmWidth, bm.bmHeight);

        let mut result = None;
        if got != 0 && has_color && width > 0 && height > 0 && width <= 256 && height <= 256 {
            let hdc = GetDC(None);
            let header = BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            };
            let count = (width * height) as usize;
            let mut bgra = vec![0u8; count * 4];

            let mut bmi = BITMAPINFO {
                bmiHeader: header,
                ..Default::default()
            };
            let lines = GetDIBits(
                hdc,
                info.hbmColor,
                0,
                height as u32,
                Some(bgra.as_mut_ptr() as *mut _),
                &mut bmi,
                DIB_RGB_COLORS,
            );
            if lines != 0 {
                if bgra.chunks_exact(4).all(|px| px[3] == 0) {
                    let mut mask = vec![0u8; count * 4];
                    let mut mask_bmi = BITMAPINFO {
                        bmiHeader: header,
                        ..Default::default()
                    };
                    let mask_ok = !info.hbmMask.is_invalid()
                        && GetDIBits(
                            hdc,
                            info.hbmMask,
                            0,
                            height as u32,
                            Some(mask.as_mut_ptr() as *mut _),
                            &mut mask_bmi,
                            DIB_RGB_COLORS,
                        ) != 0;
                    for (i, px) in bgra.chunks_exact_mut(4).enumerate() {
                        px[3] = if mask_ok && mask[i * 4] != 0 { 0 } else { 255 };
                    }
                }
                let mut rgba = Vec::with_capacity(count * 4);
                for px in bgra.chunks_exact(4) {
                    rgba.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
                }
                result = Some(encode_png(width as u32, height as u32, &rgba));
            }
            ReleaseDC(None, hdc);
        }

        if !info.hbmColor.is_invalid() {
            let _ = DeleteObject(info.hbmColor);
        }
        if !info.hbmMask.is_invalid() {
            let _ = DeleteObject(info.hbmMask);
        }
        result
    }

    unsafe fn hbitmap_to_png(hbm: windows::Win32::Graphics::Gdi::HBITMAP) -> Option<Vec<u8>> {
        use windows::Win32::Graphics::Gdi::{
            GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
            DIB_RGB_COLORS,
        };

        let mut bm = BITMAP::default();
        let got = GetObjectW(
            hbm,
            std::mem::size_of::<BITMAP>() as i32,
            Some(&mut bm as *mut _ as *mut _),
        );
        let (width, height) = (bm.bmWidth, bm.bmHeight);
        if got == 0 || width <= 0 || height <= 0 || width > 256 || height > 256 {
            return None;
        }

        let hdc = GetDC(None);
        let header = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        };
        let count = (width * height) as usize;
        let mut bgra = vec![0u8; count * 4];
        let mut bmi = BITMAPINFO {
            bmiHeader: header,
            ..Default::default()
        };
        let lines = GetDIBits(
            hdc,
            hbm,
            0,
            height as u32,
            Some(bgra.as_mut_ptr() as *mut _),
            &mut bmi,
            DIB_RGB_COLORS,
        );
        ReleaseDC(None, hdc);
        if lines == 0 {
            return None;
        }

        if bgra.chunks_exact(4).all(|px| px[3] == 0) {
            for px in bgra.chunks_exact_mut(4) {
                px[3] = 255;
            }
        }
        let mut rgba = Vec::with_capacity(count * 4);
        for px in bgra.chunks_exact(4) {
            rgba.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
        }
        Some(encode_png(width as u32, height as u32, &rgba))
    }
}

// ── macOS backend ────────────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
mod mac {
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Mutex, OnceLock};
    use std::time::{Duration, UNIX_EPOCH};

    use serde::{Deserialize, Serialize};

    // One cache slot. Path-keyed entries store the target's mtime so a bundle
    // update (new mtime) re-resolves; generic folder/ext entries carry mtime 0
    // and never expire.
    #[derive(Clone, Serialize, Deserialize)]
    struct Entry {
        mtime: u64,
        icon: Option<String>,
    }

    static CACHE_DIR: OnceLock<PathBuf> = OnceLock::new();
    // Set whenever an entry is inserted; the flusher thread writes the map to
    // disk on the next tick and clears it. Coalesces the warm's ~200 inserts
    // into a handful of writes instead of one per icon.
    static DIRTY: AtomicBool = AtomicBool::new(false);

    pub fn set_cache_dir(dir: PathBuf) {
        let _ = CACHE_DIR.set(dir);
    }

    fn cache_file() -> Option<PathBuf> {
        CACHE_DIR.get().map(|d| d.join("icon-cache-v1.json"))
    }

    fn cache() -> &'static Mutex<HashMap<String, Entry>> {
        static CACHE: OnceLock<Mutex<HashMap<String, Entry>>> = OnceLock::new();
        CACHE.get_or_init(|| {
            let map = load_from_disk().unwrap_or_default();
            // Persist dirty state off the hot path (the app runs continuously).
            std::thread::spawn(flush_loop);
            Mutex::new(map)
        })
    }

    fn load_from_disk() -> Option<HashMap<String, Entry>> {
        let bytes = std::fs::read(cache_file()?).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    fn flush_loop() {
        loop {
            std::thread::sleep(Duration::from_secs(3));
            if DIRTY.swap(false, Ordering::AcqRel) {
                flush();
            }
        }
    }

    fn flush() {
        let Some(path) = cache_file() else { return };
        let json = {
            let Ok(guard) = cache().lock() else { return };
            match serde_json::to_vec(&*guard) {
                Ok(j) => j,
                Err(_) => return,
            }
        };
        // Temp-then-rename so a crash mid-write can't corrupt the cache file.
        let tmp = path.with_extension("json.tmp");
        if std::fs::write(&tmp, &json).is_ok() {
            let _ = std::fs::rename(&tmp, &path);
        }
    }

    fn path_mtime(path: &str) -> u64 {
        std::fs::metadata(path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    pub fn warm_app_icons(paths: Vec<String>) {
        // One at a time: NSWorkspace serializes icon lookups internally, so a
        // burst neither helps nor is kinder to on-demand lookups for the rows
        // the user is actually looking at.
        for p in paths {
            let _ = icon_for_path(&p);
        }
    }

    /// Process enumeration returns the executable inside an application bundle
    /// (`Foo.app/Contents/MacOS/Foo`). NSWorkspace needs the owning `.app` path
    /// to return the application artwork rather than a generic Mach-O icon.
    fn app_bundle_ancestor(path: &std::path::Path) -> Option<&std::path::Path> {
        path.ancestors().find(|ancestor| {
            ancestor
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("app"))
        })
    }

    pub fn icon_for_path(path: &str) -> Option<String> {
        let requested = std::path::Path::new(path);
        let p = app_bundle_ancestor(requested).unwrap_or(requested);
        let lookup_path = p.to_string_lossy();
        let ext = p
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())
            .unwrap_or_default();
        // App bundles are directories, but each carries its own icon — they
        // must never share a cache slot (keyed on the folder slot, every app
        // rendered as whichever app resolved first). Extensionless files
        // (mach-O binaries in scripts dirs) also get per-path slots. Plain
        // folders and ordinary files share per-kind/per-extension slots.
        let is_path_key = ext == "app" || (ext.is_empty() && !p.is_dir());
        let key = if is_path_key {
            lookup_path.to_string()
        } else if p.is_dir() {
            "\u{0}folder".to_string()
        } else {
            ext
        };
        // Only path-keyed entries carry a meaningful mtime to invalidate on.
        let mtime = if is_path_key {
            path_mtime(&lookup_path)
        } else {
            0
        };

        if let Ok(c) = cache().lock() {
            if let Some(hit) = c.get(&key) {
                if !is_path_key || hit.mtime == mtime {
                    return hit.icon.clone();
                }
            }
        }
        let icon = nsimage_icon_for_path(&lookup_path);
        if let Ok(mut c) = cache().lock() {
            c.insert(
                key,
                Entry {
                    mtime,
                    icon: icon.clone(),
                },
            );
        }
        DIRTY.store(true, Ordering::Release);
        icon
    }

    fn nsstring(s: &str) -> Option<*mut objc2::runtime::AnyObject> {
        use objc2::msg_send;
        use objc2::runtime::AnyClass;
        use std::ffi::CString;

        let c = CString::new(s).ok()?;
        unsafe {
            let cls = AnyClass::get("NSString")?;
            let ns: *mut objc2::runtime::AnyObject =
                msg_send![cls, stringWithUTF8String: c.as_ptr()];
            if ns.is_null() {
                None
            } else {
                Some(ns)
            }
        }
    }

    fn nsimage_icon_for_path(path: &str) -> Option<String> {
        use objc2::msg_send;
        use objc2::runtime::{AnyClass, AnyObject};

        unsafe {
            let workspace_cls = AnyClass::get("NSWorkspace")?;
            let workspace: *mut AnyObject = msg_send![workspace_cls, sharedWorkspace];
            if workspace.is_null() {
                return None;
            }
            let ns_path = nsstring(path)?;
            let image: *mut AnyObject = msg_send![workspace, iconForFile: ns_path];
            if image.is_null() {
                return None;
            }

            // Icon NSImages carry bitmap reps at 16/32/128/256/512/1024 px.
            // The rows draw at 18pt (36 physical px @2x), so encode the
            // smallest rep that still covers that instead of the TIFF of the
            // whole image — initWithData: picks an arbitrary (usually the
            // biggest) rep, and a 1024×1024 PNG per icon made first paint
            // visibly late and ballooned the IPC payload.
            let rep_cls = AnyClass::get("NSBitmapImageRep")?;
            let reps: *mut AnyObject = msg_send![image, representations];
            let mut rep: *mut AnyObject = std::ptr::null_mut();
            if !reps.is_null() {
                let count: usize = msg_send![reps, count];
                let mut best_w = isize::MAX;
                for i in 0..count {
                    let r: *mut AnyObject = msg_send![reps, objectAtIndex: i];
                    if r.is_null() {
                        continue;
                    }
                    let is_bitmap: bool = msg_send![r, isKindOfClass: rep_cls];
                    if !is_bitmap {
                        continue;
                    }
                    let w: isize = msg_send![r, pixelsWide];
                    // Smallest rep >= 36px; failing that, the biggest one.
                    let better = if rep.is_null() {
                        true
                    } else if (w >= 36) != (best_w >= 36) {
                        w >= 36
                    } else if w >= 36 {
                        w < best_w
                    } else {
                        w > best_w
                    };
                    if better {
                        rep = r;
                        best_w = w;
                    }
                }
            }
            if rep.is_null() {
                // Non-bitmap reps only (rare): fall back to the TIFF round trip.
                let tiff: *mut AnyObject = msg_send![image, TIFFRepresentation];
                if tiff.is_null() {
                    return None;
                }
                let alloc: *mut AnyObject = msg_send![rep_cls, alloc];
                rep = msg_send![alloc, initWithData: tiff];
                if rep.is_null() {
                    return None;
                }
            }
            // NSBitmapImageFileTypePNG = 4
            let png_type: u64 = 4;
            let empty_dict: *mut AnyObject = msg_send![AnyClass::get("NSDictionary")?, dictionary];
            let data: *mut AnyObject =
                msg_send![rep, representationUsingType: png_type properties: empty_dict];
            if data.is_null() {
                return None;
            }
            let len: usize = msg_send![data, length];
            let bytes: *const std::ffi::c_void = msg_send![data, bytes];
            let bytes = std::slice::from_raw_parts(bytes as *const u8, len);
            let png = downscale_png_if_large(bytes.to_vec());
            let b64 = super::super::fs::base64_encode(&png);
            Some(format!("data:image/png;base64,{b64}"))
        }
    }

    /// Modern asset-catalog icons often expose a single huge rep (the
    /// smallest-rep selection above then never fires and the TIFF fallback
    /// yields a 1024×1024 PNG — ~2 MB of base64 per icon, which is what made
    /// icons appear late). The rows draw at 18pt (36px @2x), so anything past
    /// 128px is resized down to 64px here. Runs once per cache slot.
    fn downscale_png_if_large(png: Vec<u8>) -> Vec<u8> {
        let Ok(img) = image::load_from_memory_with_format(&png, image::ImageFormat::Png) else {
            return png;
        };
        if img.width() <= 128 && img.height() <= 128 {
            return png;
        }
        let small = img.thumbnail(64, 64);
        let mut out = std::io::Cursor::new(Vec::new());
        match small.write_to(&mut out, image::ImageFormat::Png) {
            Ok(()) => out.into_inner(),
            Err(_) => png,
        }
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn process_executable_resolves_to_app_bundle() {
            let executable =
                std::path::Path::new("/Applications/Commandeer.app/Contents/MacOS/commandeer");
            assert_eq!(
                super::app_bundle_ancestor(executable),
                Some(std::path::Path::new("/Applications/Commandeer.app"))
            );
            assert_eq!(
                super::app_bundle_ancestor(std::path::Path::new("/usr/bin/ssh")),
                None
            );
        }

        // NSWorkspace icon lookup for a stock app; verifies the Objective-C
        // bridge and TIFF→PNG conversion without relying on a specific icon.
        #[test]
        fn smoke_mac_app_icon() {
            let paths = [
                "/Applications/Calculator.app",
                "/System/Applications/Calculator.app",
            ];
            let mut found = false;
            for path in &paths {
                if std::path::Path::new(path).exists() {
                    let icon = super::icon_for_path(path);
                    assert!(
                        icon.as_deref()
                            .unwrap_or("")
                            .starts_with("data:image/png;base64,"),
                        "expected PNG data URL for {path}, got {icon:?}"
                    );
                    found = true;
                    break;
                }
            }
            assert!(found, "neither Calculator.app path exists");
        }

        // Regression: .app bundles are directories, and the cache once keyed
        // all directories on one shared folder slot — every app rendered as
        // whichever app resolved first (Activity Monitor, in practice). Two
        // different apps must occupy two path-keyed cache entries, and each
        // payload must be a small rep, not the full 1024×1024 TIFF re-encode.
        // A headless `cargo test` process can receive the same generic AppKit
        // icon for every bundle, so pixel inequality is a runtime integration
        // concern; this unit test guards the cache-key bug deterministically.
        #[test]
        fn distinct_app_cache_entries_and_small_payloads() {
            let a = [
                "/System/Applications/Calculator.app",
                "/Applications/Calculator.app",
            ]
            .iter()
            .find(|p| std::path::Path::new(p).exists())
            .copied();
            let b = [
                "/System/Applications/Utilities/Activity Monitor.app",
                "/System/Applications/Utilities/Terminal.app",
            ]
            .iter()
            .find(|p| std::path::Path::new(p).exists())
            .copied();
            let (Some(a), Some(b)) = (a, b) else {
                return; // stock apps missing; nothing to compare
            };
            let ia = super::icon_for_path(a).expect("icon for first app");
            let ib = super::icon_for_path(b).expect("icon for second app");
            let cache = super::cache().lock().expect("icon cache lock");
            assert_ne!(a, b);
            assert!(
                cache.contains_key(a),
                "missing path-keyed cache entry for {a}"
            );
            assert!(
                cache.contains_key(b),
                "missing path-keyed cache entry for {b}"
            );
            drop(cache);
            for (path, icon) in [(a, &ia), (b, &ib)] {
                assert!(
                    icon.len() < 300_000,
                    "icon for {path} is {} bytes — looks like a full-size rep, not a small one",
                    icon.len()
                );
            }
        }
    }
}

// ── Minimal PNG writer (RGBA8, uncompressed deflate) ─────────────────────────
// Only the Windows backend needs this; macOS asks AppKit for PNG bytes directly.

#[cfg(target_os = "windows")]
fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

#[cfg(target_os = "windows")]
fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in data {
        a = (a + byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

#[cfg(target_os = "windows")]
fn png_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let crc = crc32(&out[start..]);
    out.extend_from_slice(&crc.to_be_bytes());
}

#[cfg(target_os = "windows")]
fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let stride = width as usize * 4;
    let mut raw = Vec::with_capacity((stride + 1) * height as usize);
    for row in rgba.chunks(stride) {
        raw.push(0);
        raw.extend_from_slice(row);
    }

    let mut z = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = raw.chunks(0xFFFF).collect();
    for (i, block) in blocks.iter().enumerate() {
        z.push(if i == blocks.len() - 1 { 1 } else { 0 });
        let len = block.len() as u16;
        z.extend_from_slice(&len.to_le_bytes());
        z.extend_from_slice(&(!len).to_le_bytes());
        z.extend_from_slice(block);
    }
    z.extend_from_slice(&adler32(&raw).to_be_bytes());

    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // 8-bit RGBA

    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    png_chunk(&mut out, b"IHDR", &ihdr);
    png_chunk(&mut out, b"IDAT", &z);
    png_chunk(&mut out, b"IEND", &[]);
    out
}
