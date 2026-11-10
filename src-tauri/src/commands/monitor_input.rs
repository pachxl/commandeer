//! Monitor input-source switching over DDC/CI (MCCS VCP code 0x60).
//!
//! This is the same channel ASUS DisplayWidget Center (and most vendor monitor
//! utilities) use to change a monitor's input from the PC, so it works without
//! talking to the vendor app. Windows-only: it goes through the dxva2 Monitor
//! Configuration API. The feature is hardware-gated: a monitor only appears
//! when its DDC/CI capabilities string advertises VCP 0x60 with a list of
//! inputs, so users without such a monitor never see the commands.
//!
//! DDC/CI is slow (a capabilities request takes ~1 s per monitor) and flaky
//! (other tools, e.g. DisplayWidget Center itself, share the I2C bus), so:
//! - capabilities are cached per physical monitor (keyed by its device
//!   interface path, which is stable across reboots and display rearranges),
//!   and warmed in the background at startup;
//! - every DDC call is serialized behind one lock and retried a few times.

use serde::Serialize;

/// The MCCS "Input Select" VCP code.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
const VCP_INPUT_SOURCE: u8 = 0x60;

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
#[derive(Debug, Clone, Serialize)]
pub struct MonitorInput {
    /// Raw VCP 0x60 value
    pub value: u8,
    pub label: String,
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
#[derive(Debug, Clone, Serialize)]
pub struct MonitorInputs {
    /// Stable id (device interface path); pass back to `set_monitor_input`
    pub id: String,
    /// Model name from the capabilities string, e.g. "XG27AQDMG"
    pub name: String,
    /// Windows display number ("Display 1"), to tell identical models apart
    pub display: String,
    /// Currently selected input, when it could be read (only filled by
    /// `list_monitor_inputs`; the cached listing leaves it empty)
    pub current: Option<u8>,
    pub inputs: Vec<MonitorInput>,
}

/// Human label for an MCCS 2.2 input-source value.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn input_label(value: u8) -> String {
    let label = match value {
        0x01 => "VGA 1",
        0x02 => "VGA 2",
        0x03 => "DVI 1",
        0x04 => "DVI 2",
        0x05 => "Composite 1",
        0x06 => "Composite 2",
        0x07 => "S-Video 1",
        0x08 => "S-Video 2",
        0x09 => "Tuner 1",
        0x0A => "Tuner 2",
        0x0B => "Tuner 3",
        0x0C => "Component 1",
        0x0D => "Component 2",
        0x0E => "Component 3",
        0x0F => "DisplayPort 1",
        0x10 => "DisplayPort 2",
        0x11 => "HDMI 1",
        0x12 => "HDMI 2",
        _ => return format!("Input 0x{value:02X}"),
    };
    label.to_string()
}

/// Pull `model(...)` and the VCP 0x60 value list out of an MCCS capabilities
/// string, e.g. `(prot(monitor)type(lcd)model(XG27AQDMG)...vcp(02 10 60(11 12 0F) 62)...)`.
/// Returns `None` when the monitor doesn't advertise input select with values.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn parse_capabilities(caps: &str) -> Option<(Option<String>, Vec<u8>)> {
    let lower = caps.to_ascii_lowercase();
    let model = top_level_group(caps, &lower, "model")
        .map(|m| m.trim().to_string())
        .filter(|m| !m.is_empty());
    let vcp = top_level_group(caps, &lower, "vcp")?;
    let inputs = vcp_values(vcp, VCP_INPUT_SOURCE)?;
    (!inputs.is_empty()).then_some((model, inputs))
}

/// Contents of the first `name(...)` group in the capabilities string (balanced parens), where `name` is
/// a whole word (so `vcp` doesn't match `vcpname`).
fn top_level_group<'a>(caps: &'a str, lower: &str, name: &str) -> Option<&'a str> {
    let needle = format!("{name}(");
    let mut from = 0;
    while let Some(rel) = lower[from..].find(&needle) {
        let start = from + rel;
        let prev = lower[..start].chars().next_back();
        if prev.is_none_or(|c| !c.is_ascii_alphanumeric() && c != '_') {
            let body = start + needle.len();
            let mut depth = 1usize;
            for (i, c) in caps[body..].char_indices() {
                match c {
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            return Some(&caps[body..body + i]);
                        }
                    }
                    _ => {}
                }
            }
            return Some(&caps[body..]); // unterminated: take the rest
        }
        from = start + needle.len();
    }
    None
}

/// Split a run of hex digits into byte values. Some monitors omit the spaces
/// between codes (`vcp(021012)`), so long runs are read two digits at a time.
fn hex_bytes(token: &str) -> Vec<u8> {
    let digits: Vec<char> = token.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if digits.len() <= 2 {
        let s: String = digits.iter().collect();
        return u8::from_str_radix(&s, 16).into_iter().collect();
    }
    digits
        .chunks(2)
        .filter_map(|pair| u8::from_str_radix(&pair.iter().collect::<String>(), 16).ok())
        .collect()
}

/// Find `code(...)` inside a `vcp(...)` body and return its listed values.
fn vcp_values(vcp: &str, code: u8) -> Option<Vec<u8>> {
    let bytes = vcp.as_bytes();
    let mut i = 0;
    let mut last_code: Option<u8> = None;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if c == '(' {
            // Value list belonging to the last code read.
            let mut depth = 1usize;
            let start = i + 1;
            let mut j = start;
            while j < bytes.len() && depth > 0 {
                match bytes[j] {
                    b'(' => depth += 1,
                    b')' => depth -= 1,
                    _ => {}
                }
                j += 1;
            }
            let end = if depth == 0 { j - 1 } else { j };
            if last_code == Some(code) {
                let values = vcp[start..end]
                    .split_whitespace()
                    .flat_map(hex_bytes)
                    .collect();
                return Some(values);
            }
            last_code = None;
            i = j;
        } else if c.is_ascii_hexdigit() {
            let start = i;
            while i < bytes.len() && (bytes[i] as char).is_ascii_hexdigit() {
                i += 1;
            }
            last_code = hex_bytes(&vcp[start..i]).last().copied();
        } else {
            i += 1;
        }
    }
    None
}

/// Cached listing (no DDC traffic, `current` left empty). Kicks off a
/// background re-probe when the cache is stale, so hot-plugged monitors show up
/// on a later palette open. Empty on non-Windows or without a capable monitor.
#[tauri::command]
pub fn cached_monitor_inputs() -> Vec<MonitorInputs> {
    #[cfg(target_os = "windows")]
    {
        win::cached()
    }
    #[cfg(target_os = "macos")]
    {
        Vec::new()
    }
    #[cfg(target_os = "linux")]
    {
        Vec::new()
    }
}

/// Fresh listing including each monitor's current input.
#[tauri::command]
pub async fn list_monitor_inputs() -> Result<Vec<MonitorInputs>, String> {
    #[cfg(target_os = "windows")]
    {
        tokio::task::spawn_blocking(|| win::list(true))
            .await
            .map_err(|e| e.to_string())
    }
    #[cfg(target_os = "macos")]
    {
        Ok(Vec::new())
    }
    #[cfg(target_os = "linux")]
    {
        Ok(Vec::new())
    }
}

/// Switch one monitor to the given VCP 0x60 input value.
#[tauri::command]
pub async fn set_monitor_input(id: String, value: u8) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        tokio::task::spawn_blocking(move || win::set_input(&id, value))
            .await
            .map_err(|e| e.to_string())?
    }
    #[cfg(target_os = "macos")]
    {
        let _ = (id, value);
        Err("Monitor input switching is only available on Windows".to_string())
    }
    #[cfg(target_os = "linux")]
    {
        let _ = (id, value);
        Err("Monitor input switching is only available on Windows".to_string())
    }
}

/// Probe monitor capabilities in the background so the commands are ready by
/// the first palette open.
#[cfg(target_os = "windows")]
pub fn warm() {
    win::spawn_probe();
}

#[cfg(target_os = "windows")]
mod win {
    use super::{input_label, parse_capabilities, MonitorInput, MonitorInputs, VCP_INPUT_SOURCE};
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Mutex, OnceLock};
    use std::time::{Duration, Instant};
    use windows::core::PCWSTR;
    use windows::Win32::Devices::Display::{
        CapabilitiesRequestAndCapabilitiesReply, DestroyPhysicalMonitors,
        GetCapabilitiesStringLength, GetNumberOfPhysicalMonitorsFromHMONITOR,
        GetPhysicalMonitorsFromHMONITOR, GetVCPFeatureAndVCPFeatureReply, SetVCPFeature,
        PHYSICAL_MONITOR,
    };
    use windows::Win32::Foundation::{BOOL, LPARAM, RECT, TRUE};
    use windows::Win32::Graphics::Gdi::{
        EnumDisplayDevicesW, EnumDisplayMonitors, GetMonitorInfoW, DISPLAY_DEVICEW, HDC, HMONITOR,
        MONITORINFO, MONITORINFOEXW,
    };
    use windows::Win32::UI::WindowsAndMessaging::EDD_GET_DEVICE_INTERFACE_NAME;

    /// Retry a monitor whose capabilities couldn't be read after this long.
    const NEGATIVE_TTL: Duration = Duration::from_secs(120);
    /// Re-enumerate monitors in the background after this long.
    const PROBE_TTL: Duration = Duration::from_secs(30);
    const ATTEMPTS: usize = 3;
    /// MCCS asks hosts to leave ~50 ms between DDC/CI commands.
    const RETRY_DELAY: Duration = Duration::from_millis(60);

    #[derive(Clone)]
    enum Caps {
        Supported {
            model: Option<String>,
            inputs: Vec<u8>,
        },
        Unsupported {
            at: Instant,
        },
    }

    struct State {
        caps: HashMap<String, Caps>,
        /// Last enumeration result (supported monitors only, `current` empty)
        listing: Vec<MonitorInputs>,
        last_probe: Option<Instant>,
    }

    fn state() -> &'static Mutex<State> {
        static STATE: OnceLock<Mutex<State>> = OnceLock::new();
        STATE.get_or_init(|| {
            Mutex::new(State {
                caps: HashMap::new(),
                listing: Vec::new(),
                last_probe: None,
            })
        })
    }

    /// Serializes all DDC/CI traffic (probe vs. switch vs. listing).
    fn ddc_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    static PROBING: AtomicBool = AtomicBool::new(false);

    pub fn spawn_probe() {
        if PROBING.swap(true, Ordering::SeqCst) {
            return;
        }
        std::thread::spawn(|| {
            let _ = list(false);
            PROBING.store(false, Ordering::SeqCst);
        });
    }

    pub fn cached() -> Vec<MonitorInputs> {
        let st = state().lock().unwrap_or_else(|e| e.into_inner());
        let stale = st.last_probe.is_none_or(|t| t.elapsed() > PROBE_TTL);
        let listing = st.listing.clone();
        drop(st);
        if stale {
            spawn_probe();
        }
        listing
    }

    fn wide_to_string(buf: &[u16]) -> String {
        let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        String::from_utf16_lossy(&buf[..end])
    }

    /// Physical monitor handles for one enumeration; destroyed on drop.
    struct Physical {
        key: String,
        display: String,
        handle: PHYSICAL_MONITOR,
    }

    struct Enumeration(Vec<Physical>);

    impl Drop for Enumeration {
        fn drop(&mut self) {
            let handles: Vec<PHYSICAL_MONITOR> = self.0.iter().map(|p| p.handle).collect();
            if !handles.is_empty() {
                unsafe {
                    let _ = DestroyPhysicalMonitors(&handles);
                }
            }
        }
    }

    unsafe extern "system" fn collect_monitor(
        hmon: HMONITOR,
        _hdc: HDC,
        _rect: *mut RECT,
        data: LPARAM,
    ) -> BOOL {
        let list = &mut *(data.0 as *mut Vec<HMONITOR>);
        list.push(hmon);
        TRUE
    }

    fn enumerate() -> Enumeration {
        let mut hmons: Vec<HMONITOR> = Vec::new();
        unsafe {
            let _ = EnumDisplayMonitors(
                HDC::default(),
                None,
                Some(collect_monitor),
                LPARAM(&mut hmons as *mut _ as isize),
            );
        }

        let mut out = Vec::new();
        for hmon in hmons {
            let mut info = MONITORINFOEXW::default();
            info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
            let device = unsafe {
                if GetMonitorInfoW(hmon, &mut info as *mut _ as *mut MONITORINFO).as_bool() {
                    wide_to_string(&info.szDevice)
                } else {
                    String::new()
                }
            };
            let display = device
                .trim_start_matches(r"\\.\")
                .strip_prefix("DISPLAY")
                .map(|n| format!("Display {n}"))
                .unwrap_or_else(|| device.clone());

            let mut count = 0u32;
            if unsafe { GetNumberOfPhysicalMonitorsFromHMONITOR(hmon, &mut count) }.is_err()
                || count == 0
            {
                continue;
            }
            let mut phys = vec![PHYSICAL_MONITOR::default(); count as usize];
            if unsafe { GetPhysicalMonitorsFromHMONITOR(hmon, &mut phys) }.is_err() {
                continue;
            }

            // Device interface paths of the monitors attached to this display
            // output, in the same order as the physical monitors.
            let device_w: Vec<u16> = device.encode_utf16().chain(Some(0)).collect();
            for (i, p) in phys.into_iter().enumerate() {
                let mut dd = DISPLAY_DEVICEW {
                    cb: std::mem::size_of::<DISPLAY_DEVICEW>() as u32,
                    ..Default::default()
                };
                let ok = unsafe {
                    EnumDisplayDevicesW(
                        PCWSTR(device_w.as_ptr()),
                        i as u32,
                        &mut dd,
                        EDD_GET_DEVICE_INTERFACE_NAME,
                    )
                }
                .as_bool();
                let interface = if ok {
                    wide_to_string(&dd.DeviceID)
                } else {
                    String::new()
                };
                let key = if interface.is_empty() {
                    format!("{device}#{i}")
                } else {
                    interface
                };
                out.push(Physical {
                    key,
                    display: display.clone(),
                    handle: p,
                });
            }
        }
        Enumeration(out)
    }

    fn retry<T>(mut f: impl FnMut() -> Option<T>) -> Option<T> {
        for attempt in 0..ATTEMPTS {
            if let Some(v) = f() {
                return Some(v);
            }
            if attempt + 1 < ATTEMPTS {
                std::thread::sleep(RETRY_DELAY);
            }
        }
        None
    }

    fn read_capabilities(p: &Physical) -> Option<String> {
        let h = p.handle.hPhysicalMonitor;
        retry(|| unsafe {
            let mut len = 0u32;
            if GetCapabilitiesStringLength(h, &mut len) == 0 || len == 0 {
                return None;
            }
            let mut buf = vec![0u8; len as usize];
            if CapabilitiesRequestAndCapabilitiesReply(h, &mut buf) == 0 {
                return None;
            }
            let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
            Some(String::from_utf8_lossy(&buf[..end]).into_owned())
        })
    }

    fn read_current(p: &Physical) -> Option<u8> {
        let h = p.handle.hPhysicalMonitor;
        retry(|| unsafe {
            let mut cur = 0u32;
            (GetVCPFeatureAndVCPFeatureReply(h, VCP_INPUT_SOURCE, None, &mut cur, None) != 0)
                // Some firmware leaves junk in the high byte.
                .then_some((cur & 0xFF) as u8)
        })
    }

    /// Cached capabilities for a monitor, fetching them when unknown (or when a
    /// previous failure has aged out).
    fn capabilities(p: &Physical) -> Option<(Option<String>, Vec<u8>)> {
        let cached = state()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .caps
            .get(&p.key)
            .cloned();
        match cached {
            Some(Caps::Supported { model, inputs }) => return Some((model, inputs)),
            Some(Caps::Unsupported { at }) if at.elapsed() < NEGATIVE_TTL => return None,
            _ => {}
        }
        let parsed = read_capabilities(p).as_deref().and_then(parse_capabilities);
        let entry = match &parsed {
            Some((model, inputs)) => Caps::Supported {
                model: model.clone(),
                inputs: inputs.clone(),
            },
            None => Caps::Unsupported { at: Instant::now() },
        };
        state()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .caps
            .insert(p.key.clone(), entry);
        parsed
    }

    pub fn list(with_current: bool) -> Vec<MonitorInputs> {
        let _ddc = ddc_lock().lock().unwrap_or_else(|e| e.into_inner());
        let monitors = enumerate();
        let mut out = Vec::new();
        for p in &monitors.0 {
            let Some((model, inputs)) = capabilities(p) else {
                continue;
            };
            out.push(MonitorInputs {
                id: p.key.clone(),
                name: model.unwrap_or_else(|| "Monitor".to_string()),
                display: p.display.clone(),
                current: if with_current { read_current(p) } else { None },
                inputs: inputs
                    .into_iter()
                    .map(|value| MonitorInput {
                        value,
                        label: input_label(value),
                    })
                    .collect(),
            });
        }
        let mut st = state().lock().unwrap_or_else(|e| e.into_inner());
        st.listing = out
            .iter()
            .cloned()
            .map(|mut m| {
                m.current = None;
                m
            })
            .collect();
        st.last_probe = Some(Instant::now());
        out
    }

    pub fn set_input(id: &str, value: u8) -> Result<(), String> {
        let _ddc = ddc_lock().lock().unwrap_or_else(|e| e.into_inner());
        let monitors = enumerate();
        let p = monitors
            .0
            .iter()
            .find(|p| p.key == id)
            .ok_or_else(|| "That monitor is no longer connected".to_string())?;
        let h = p.handle.hPhysicalMonitor;
        retry(|| (unsafe { SetVCPFeature(h, VCP_INPUT_SOURCE, value as u32) } != 0).then_some(()))
            .ok_or_else(|| "The monitor didn't accept the input change (DDC/CI)".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const XG27: &str = "(prot(monitor)type(lcd)model(XG27AQDMG)cmds(01 02 03 07 0C E3 F3)vcp(02 03 04 08 10 12 14(05 06 08 0B) 16 18 1A 52 60(11 12 0F) 62 72(50 64 78 8C 96 A0) 8A CC(01 02) EB(00 01 02 03 04 05 06 07 ) EC(00 01) FF )mswhql(1)mccs_ver(2.0)asset_eep(32)mpu_ver(01))";
    const VG279: &str = "(prot(monitor) type(LCD)model(VG279Q) cmds(01 02 03 07 0C F3) vcp(02 04 05 08 10 12 14(05 06 08 0B) 16 18 1A 52 60(03 0F 11) 62 86(01 02 0B 0C) DF)mccs_ver(2.2)asset_eep(32)mpu(01)mswhql(1))";

    #[test]
    fn parses_asus_capabilities() {
        assert_eq!(
            parse_capabilities(XG27),
            Some((Some("XG27AQDMG".to_string()), vec![0x11, 0x12, 0x0F]))
        );
        assert_eq!(
            parse_capabilities(VG279),
            Some((Some("VG279Q".to_string()), vec![0x03, 0x0F, 0x11]))
        );
    }

    #[test]
    fn rejects_monitors_without_input_select() {
        assert_eq!(
            parse_capabilities("(prot(monitor)model(X)vcp(02 10 12 62))"),
            None
        );
        // 0x60 listed without values: nothing to offer.
        assert_eq!(
            parse_capabilities("(prot(monitor)model(X)vcp(02 10 60 62))"),
            None
        );
        assert_eq!(parse_capabilities(""), None);
    }

    #[test]
    fn handles_unspaced_codes_and_vcpname() {
        let caps = "(model(Foo)vcp(021060(0F11)62)vcpname(60(Input)))";
        assert_eq!(
            parse_capabilities(caps),
            Some((Some("Foo".to_string()), vec![0x0F, 0x11]))
        );
    }

    #[test]
    fn labels_inputs() {
        assert_eq!(input_label(0x0F), "DisplayPort 1");
        assert_eq!(input_label(0x11), "HDMI 1");
        assert_eq!(input_label(0x1B), "Input 0x1B");
    }
}
