//! Shell icons for file-search results: SHGetFileInfoW → 32×32 PNG data URL.
//! Icons are resolved per extension (files) or once for all folders, so the
//! Mutex-guarded cache makes repeat lookups free. The PNG encoder writes
//! uncompressed deflate blocks — no image/zlib dependencies needed for these
//! tiny payloads, and entries are cached anyway.

#![cfg(target_os = "windows")]

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

/// Data-URL icon for a path, or None if the shell has nothing for it.
/// Uses file extension attributes for fast lookup; best for generic files.
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

/// Data-URL icon for a specific file path, resolving .lnk targets and app icons.
/// Cached per path; slower than icon_for_path but accurate for shortcuts.
pub fn icon_for_file(path: &str) -> Option<String> {
    let key = path.to_lowercase();
    if let Some(hit) = file_cache().lock().ok()?.get(&key) {
        return hit.clone();
    }
    // For Windows shortcuts, resolve the real icon source so we get the app
    // icon instead of the generic shortcut overlay.
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

/// Data-URL icon for a shell parsing name (e.g. "shell:AppsFolder\\<id>"),
/// resolved via IShellItemImageFactory — works for UWP/Store apps that have
/// no filesystem icon source. Cached per path.
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
        // COM may already be initialized on this thread; ignore that.
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let item: IShellItem = SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None).ok()?;
        let factory: IShellItemImageFactory = item.cast().ok()?;
        let hbm = factory
            .GetImage(SIZE { cx: 32, cy: 32 }, SIIGBF_ICONONLY)
            .ok()?;
        let png = hbitmap_to_png(hbm);
        let _ = DeleteObject(hbm);
        png.map(|bytes| format!("data:image/png;base64,{}", super::fs::base64_encode(&bytes)))
    }
}

/// Resolve a Windows .lnk shortcut to the path that should be used for its
/// icon. This prefers an explicit icon location stored in the shortcut, then
/// falls back to the shortcut target so we avoid the shortcut overlay.
#[cfg(target_os = "windows")]
fn resolve_lnk_icon_source(path: &str) -> Option<String> {
    use windows::core::{Interface, PCWSTR};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, STGM,
    };
    use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};

    unsafe {
        // COM may already be initialized on this thread; ignore that.
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

        // First, honor an explicit icon location set on the shortcut itself.
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

        // Otherwise resolve the shortcut target.
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

#[cfg(not(target_os = "windows"))]
fn resolve_lnk_icon_source(_path: &str) -> Option<String> {
    None
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
            // USEFILEATTRIBUTES: resolve from the extension alone — no disk access
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
        png.map(|bytes| format!("data:image/png;base64,{}", super::fs::base64_encode(&bytes)))
    }
}

unsafe fn hicon_to_png(hicon: windows::Win32::UI::WindowsAndMessaging::HICON) -> Option<Vec<u8>> {
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
    // Monochrome (mask-only) icons are skipped: has_color is false and the
    // block below only reads hbmColor.
    if got != 0 && has_color && width > 0 && height > 0 && width <= 256 && height <= 256 {
        let hdc = GetDC(None);
        let header = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height, // top-down
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        };
        let count = (width * height) as usize;
        let mut bgra = vec![0u8; count * 4];

        let mut bmi = BITMAPINFO { bmiHeader: header, ..Default::default() };
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
            // Mask-based icons report all-zero alpha; recover it from hbmMask
            if bgra.chunks_exact(4).all(|px| px[3] == 0) {
                let mut mask = vec![0u8; count * 4];
                let mut mask_bmi = BITMAPINFO { bmiHeader: header, ..Default::default() };
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
                    // Mask white = transparent, black = opaque
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

/// 32bpp HBITMAP (e.g. from IShellItemImageFactory) → PNG bytes. Shell images
/// carry a real alpha channel; all-zero alpha (some legacy sources) is
/// recovered as fully opaque.
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
        biHeight: -height, // top-down
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB.0,
        ..Default::default()
    };
    let count = (width * height) as usize;
    let mut bgra = vec![0u8; count * 4];
    let mut bmi = BITMAPINFO { bmiHeader: header, ..Default::default() };
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

// ── Minimal PNG writer (RGBA8, uncompressed deflate) ─────────────────────────

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

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in data {
        a = (a + byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

fn png_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let crc = crc32(&out[start..]);
    out.extend_from_slice(&crc.to_be_bytes());
}

fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    // Raw scanlines, filter type 0 per row
    let stride = width as usize * 4;
    let mut raw = Vec::with_capacity((stride + 1) * height as usize);
    for row in rgba.chunks(stride) {
        raw.push(0);
        raw.extend_from_slice(row);
    }

    // zlib stream with stored (uncompressed) deflate blocks
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
