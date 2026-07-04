//! Shell icons for file-search results. Icons are resolved lazily and cached
//! per extension/path so repeat lookups are free.
//!
//! Backends:
//! - Windows: SHGetFileInfoW / IShellItemImageFactory → 32×32 PNG data URL.
//! - macOS: NSWorkspace.iconForFile: → PNG data URL (via TIFF → bitmap rep).
//! - Linux: not implemented; returns None.

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

#[cfg(target_os = "linux")]
pub fn icon_for_path(path: &str) -> Option<String> {
    let _ = path;
    None
}

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
    use std::sync::{Mutex, OnceLock};

    fn cache() -> &'static Mutex<HashMap<String, Option<String>>> {
        static CACHE: OnceLock<Mutex<HashMap<String, Option<String>>>> = OnceLock::new();
        CACHE.get_or_init(|| Mutex::new(HashMap::new()))
    }

    fn file_cache() -> &'static Mutex<HashMap<String, Option<String>>> {
        static CACHE: OnceLock<Mutex<HashMap<String, Option<String>>>> = OnceLock::new();
        CACHE.get_or_init(|| Mutex::new(HashMap::new()))
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
        if let Some(hit) = cache().lock().ok()?.get(&key) {
            return hit.clone();
        }
        let icon = shell_icon(path, is_dir, true);
        if let Ok(mut c) = cache().lock() {
            c.insert(key, icon.clone());
        }
        icon
    }

    pub fn icon_for_file(path: &str) -> Option<String> {
        let key = path.to_lowercase();
        if let Some(hit) = file_cache().lock().ok()?.get(&key) {
            return hit.clone();
        }
        let resolved = if path.to_lowercase().ends_with(".lnk") {
            resolve_lnk_icon_source(path).unwrap_or_else(|| path.to_string())
        } else {
            path.to_string()
        };
        let icon = shell_icon(&resolved, Path::new(&resolved).is_dir(), false);
        if let Ok(mut c) = file_cache().lock() {
            c.insert(key, icon.clone());
        }
        icon
    }

    pub fn icon_for_shell_item(parse_path: &str) -> Option<String> {
        let key = parse_path.to_lowercase();
        if let Some(hit) = file_cache().lock().ok()?.get(&key) {
            return hit.clone();
        }
        let icon = shell_item_icon(parse_path);
        if let Ok(mut c) = file_cache().lock() {
            c.insert(key, icon.clone());
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
            png.map(|bytes| format!("data:image/png;base64,{}", super::super::fs::base64_encode(&bytes)))
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
            shell_link.GetPath(&mut target, std::ptr::null_mut(), 0).ok()?;
            let len = target
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(target.len());
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
            png.map(|bytes| format!("data:image/png;base64,{}", super::super::fs::base64_encode(&bytes)))
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
        let src = if has_color { info.hbmColor } else { info.hbmMask };
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
    use std::sync::{Mutex, OnceLock};

    fn cache() -> &'static Mutex<HashMap<String, Option<String>>> {
        static CACHE: OnceLock<Mutex<HashMap<String, Option<String>>>> = OnceLock::new();
        CACHE.get_or_init(|| Mutex::new(HashMap::new()))
    }

    pub fn icon_for_path(path: &str) -> Option<String> {
        let key = if std::path::Path::new(path).is_dir() {
            "\u{0}folder".to_string()
        } else {
            std::path::Path::new(path)
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_lowercase())
                .unwrap_or_default()
        };
        if let Some(hit) = cache().lock().ok()?.get(&key) {
            return hit.clone();
        }
        let icon = nsimage_icon_for_path(path);
        if let Ok(mut c) = cache().lock() {
            c.insert(key, icon.clone());
        }
        icon
    }

    fn nsstring(s: &str) -> Option<*mut objc2::runtime::AnyObject> {
        use objc2::msg_send;
        use objc2::runtime::AnyClass;
        use std::ffi::CString;

        let c = CString::new(s).ok()?;
        unsafe {
            let cls = AnyClass::get("NSString")?;
            let ns: *mut objc2::runtime::AnyObject = msg_send![cls, stringWithUTF8String: c.as_ptr()];
            if ns.is_null() { None } else { Some(ns) }
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

            // NSImage → TIFF → NSBitmapImageRep → PNG data.
            let tiff: *mut AnyObject = msg_send![image, TIFFRepresentation];
            if tiff.is_null() {
                return None;
            }
            let rep_cls = AnyClass::get("NSBitmapImageRep")?;
            let rep: *mut AnyObject = msg_send![rep_cls, alloc];
            let rep: *mut AnyObject = msg_send![rep, initWithData: tiff];
            if rep.is_null() {
                return None;
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
            let b64 = super::super::fs::base64_encode(bytes);
            Some(format!("data:image/png;base64,{b64}"))
        }
    }

    #[cfg(test)]
    mod tests {
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
                        icon.as_deref().unwrap_or("").starts_with("data:image/png;base64,"),
                        "expected PNG data URL for {path}, got {icon:?}"
                    );
                    found = true;
                    break;
                }
            }
            assert!(found, "neither Calculator.app path exists");
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
