//! `commandeer://` deep-link routing. Lets a browser, script, or another app
//! navigate to a specific command from anywhere — mirrors vicinae's
//! `vicinae://` URIs without treating an external URI as a trusted shortcut.
//!
//! Supported forms:
//!   commandeer://command/<id>   → show the palette and navigate to command <id>
//!   commandeer://screenshot     → start the region-screenshot flow
//!   commandeer://open           → just show the palette
//!
//! The <id> may be percent-encoded (command ids contain ':', e.g.
//! `settings:open` → `settings%3Aopen`).

use tauri::{AppHandle, Emitter};

#[derive(Debug, PartialEq)]
enum DeepLinkRoute {
    NavigateCommand(String),
    Screenshot,
    OpenPalette,
}

fn parse_url(url: &str) -> Option<DeepLinkRoute> {
    let rest = url
        .strip_prefix("commandeer://")
        .or_else(|| url.strip_prefix("commandeer:"))?;

    // Trim a trailing slash and split "action/arg".
    let rest = rest.trim_end_matches('/');
    let (action, arg) = match rest.split_once('/') {
        Some((a, b)) => (a, b),
        None => (rest, ""),
    };

    Some(match action {
        "command" | "run" if !arg.is_empty() => DeepLinkRoute::NavigateCommand(percent_decode(arg)),
        "screenshot" => DeepLinkRoute::Screenshot,
        // "open", a bare URI, and unknown actions all surface the palette.
        _ => DeepLinkRoute::OpenPalette,
    })
}

/// Route a single `commandeer://…` URL. Unknown/empty actions fall back to
/// showing the palette so a bare `commandeer://` link still does something
/// useful. Returns true if the URL was recognized as one of ours.
pub fn handle_url(app: &AppHandle, url: &str) -> bool {
    let Some(route) = parse_url(url) else {
        return false;
    };

    match route {
        DeepLinkRoute::NavigateCommand(id) => {
            // External URIs are navigation requests, not trusted execution
            // triggers. Keep this event separate from command-hotkey so a web
            // page cannot inherit the direct-action shortcut path.
            crate::show_palette(app);
            let _ = app.emit("command-deep-link", id);
        }
        // Must not fall through to show_palette: the palette would end up in
        // the frozen frame (this is the COSMIC PrtScn binding's entry point).
        DeepLinkRoute::Screenshot => {
            super::screenshot::start_screenshot_bg(app);
        }
        DeepLinkRoute::OpenPalette => {
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

#[cfg(test)]
mod tests {
    use super::{parse_url, DeepLinkRoute};

    #[test]
    fn command_uri_is_a_navigation_request() {
        assert_eq!(
            parse_url("commandeer://command/settings%3Aopen"),
            Some(DeepLinkRoute::NavigateCommand("settings:open".to_string()))
        );
        assert_eq!(
            parse_url("commandeer://run/system%3Ashutdown/"),
            Some(DeepLinkRoute::NavigateCommand(
                "system:shutdown".to_string()
            ))
        );
    }

    #[test]
    fn screenshot_and_palette_routes_remain_distinct() {
        assert_eq!(
            parse_url("commandeer://screenshot"),
            Some(DeepLinkRoute::Screenshot)
        );
        assert_eq!(
            parse_url("commandeer://open"),
            Some(DeepLinkRoute::OpenPalette)
        );
        assert_eq!(
            parse_url("commandeer://command"),
            Some(DeepLinkRoute::OpenPalette)
        );
        assert_eq!(parse_url("https://example.com"), None);
    }
}
