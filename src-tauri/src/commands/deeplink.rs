//! `commandeer://` deep-link routing. Lets a browser, script, or another app
//! open a specific command from anywhere — mirrors vicinae's `vicinae://` URIs.
//!
//! Supported forms:
//!   commandeer://command/<id>   → show the palette and run/open command <id>
//!                                 (reuses the per-command-hotkey path)
//!   commandeer://screenshot     → start the region-screenshot flow
//!   commandeer://open           → just show the palette
//!
//! The <id> may be percent-encoded (command ids contain ':', e.g.
//! `settings:open` → `settings%3Aopen`).

use tauri::{AppHandle, Emitter};

/// Route a single `commandeer://…` URL. Unknown/empty actions fall back to
/// showing the palette so a bare `commandeer://` link still does something
/// useful. Returns true if the URL was recognized as one of ours.
pub fn handle_url(app: &AppHandle, url: &str) -> bool {
    let Some(rest) = url
        .strip_prefix("commandeer://")
        .or_else(|| url.strip_prefix("commandeer:"))
    else {
        return false;
    };

    // Trim a trailing slash and split "action/arg".
    let rest = rest.trim_end_matches('/');
    let (action, arg) = match rest.split_once('/') {
        Some((a, b)) => (a, b),
        None => (rest, ""),
    };

    match action {
        "command" | "run" if !arg.is_empty() => {
            let id = percent_decode(arg);
            let _ = app.emit("command-hotkey", id);
        }
        // Must not fall through to show_palette: the palette would end up in
        // the frozen frame (this is the COSMIC PrtScn binding's entry point).
        "screenshot" => {
            super::screenshot::start_screenshot_bg(app);
        }
        _ => {
            // "open" or anything unrecognized: surface the palette.
            crate::show_palette(app);
        }
    }
    true
}

/// Scan process arguments for a `commandeer://` URL and route the first one.
/// Windows delivers deep links as a launch argument, forwarded to the running
/// instance by the single-instance plugin.
pub fn handle_args<I: IntoIterator<Item = String>>(app: &AppHandle, args: I) -> bool {
    for arg in args {
        if arg.starts_with("commandeer:") && handle_url(app, &arg) {
            return true;
        }
    }
    false
}

/// Minimal percent-decoder (RFC 3986). Enough for command ids; unrecognized
/// escapes are left verbatim.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(hi), Some(lo)) = (hi, lo) {
                out.push((hi * 16 + lo) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
