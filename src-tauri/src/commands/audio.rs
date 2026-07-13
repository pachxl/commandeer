//! System volume control. Windows: Core Audio API (IAudioEndpointVolume, per
//! render endpoint), backing per-device volume sliders and a mute toggle.
//! macOS: osascript `set volume`, default output only (see `mod mac`).
//!
//! Endpoints are re-activated per call rather than cached: devices can appear
//! or vanish at any time (headphones plugged in, Bluetooth connects),
//! activation is cheap (~sub-ms), and it keeps the COM interface confined to
//! the blocking-pool thread that uses it.

use serde::Serialize;

/// An active audio output (render) device.
#[derive(Debug, Clone, Serialize)]
pub struct AudioDevice {
    /// Opaque endpoint id, passable back as the `device` argument
    pub id: String,
    /// Friendly name, e.g. "Speakers (Realtek High Definition Audio)"
    pub name: String,
    pub is_default: bool,
}

/// List active output devices, default endpoint first.
#[tauri::command]
pub async fn list_audio_devices() -> Result<Vec<AudioDevice>, String> {
    #[cfg(target_os = "windows")]
    {
        tokio::task::spawn_blocking(win::list_devices)
            .await
            .map_err(|e| e.to_string())?
    }
    #[cfg(target_os = "macos")]
    {
        mac::list_devices()
    }
    #[cfg(target_os = "linux")]
    {
        tokio::task::spawn_blocking(linux::list_devices)
            .await
            .map_err(|e| e.to_string())?
    }
}

/// Read the master volume of a device (`None` = default output) as a scalar
/// in `0.0..=1.0`.
#[tauri::command]
pub async fn get_volume(device: Option<String>) -> Result<f32, String> {
    #[cfg(target_os = "windows")]
    {
        tokio::task::spawn_blocking(move || win::get_volume(device.as_deref()))
            .await
            .map_err(|e| e.to_string())?
    }
    #[cfg(target_os = "macos")]
    {
        let _ = device; // osascript only addresses the default output
        tokio::task::spawn_blocking(mac::get_volume)
            .await
            .map_err(|e| e.to_string())?
    }
    #[cfg(target_os = "linux")]
    {
        tokio::task::spawn_blocking(move || linux::get_volume(device.as_deref()))
            .await
            .map_err(|e| e.to_string())?
    }
}

/// Set the master volume of a device (`None` = default output). `level` is a
/// scalar in `0.0..=1.0`; out-of-range values are clamped.
#[tauri::command]
pub async fn set_volume(level: f32, device: Option<String>) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let level = level.clamp(0.0, 1.0);
        tokio::task::spawn_blocking(move || win::set_volume(level, device.as_deref()))
            .await
            .map_err(|e| e.to_string())?
    }
    #[cfg(target_os = "macos")]
    {
        let _ = device; // osascript only addresses the default output
        let level = level.clamp(0.0, 1.0);
        tokio::task::spawn_blocking(move || mac::set_volume(level))
            .await
            .map_err(|e| e.to_string())?
    }
    #[cfg(target_os = "linux")]
    {
        let level = level.clamp(0.0, 1.0);
        tokio::task::spawn_blocking(move || linux::set_volume(level, device.as_deref()))
            .await
            .map_err(|e| e.to_string())?
    }
}

/// Flip the mute state of a device (`None` = default output) in one
/// round-trip (read + write on the same endpoint, no get/set race) and return
/// the new state.
#[tauri::command]
pub async fn toggle_mute(device: Option<String>) -> Result<bool, String> {
    #[cfg(target_os = "windows")]
    {
        tokio::task::spawn_blocking(move || win::toggle_mute(device.as_deref()))
            .await
            .map_err(|e| e.to_string())?
    }
    #[cfg(target_os = "macos")]
    {
        let _ = device; // osascript only addresses the default output
        tokio::task::spawn_blocking(mac::toggle_mute)
            .await
            .map_err(|e| e.to_string())?
    }
    #[cfg(target_os = "linux")]
    {
        tokio::task::spawn_blocking(move || linux::toggle_mute(device.as_deref()))
            .await
            .map_err(|e| e.to_string())?
    }
}

/// Linux volume control by shelling to `wpctl` (WirePlumber — PipeWire's CLI,
/// the Fedora default) with `pactl` (PulseAudio) as the fallback. The backend
/// is probed once and cached; device ids stay opaque strings for the frontend
/// (wpctl object ids / pactl sink names).
#[cfg(target_os = "linux")]
mod linux {
    use super::AudioDevice;
    use std::sync::Mutex;

    #[derive(Clone, Copy, PartialEq)]
    enum Backend {
        Wpctl,
        Pactl,
    }

    /// Outer Option: probed yet? Inner: which tool, if any.
    static BACKEND: Mutex<Option<Option<Backend>>> = Mutex::new(None);

    fn backend() -> Result<Backend, String> {
        let mut guard = BACKEND.lock().unwrap();
        let cached = guard.get_or_insert_with(|| {
            if run("wpctl", &["status"]).is_ok() {
                Some(Backend::Wpctl)
            } else if run("pactl", &["info"]).is_ok() {
                Some(Backend::Pactl)
            } else {
                None
            }
        });
        cached.ok_or_else(|| {
            "no PipeWire/PulseAudio control tool found (install wireplumber or pulseaudio-utils)"
                .to_string()
        })
    }

    fn run(program: &str, args: &[&str]) -> Result<String, String> {
        let out = std::process::Command::new(program)
            .args(args)
            .output()
            .map_err(|e| format!("{program} failed to run: {e}"))?;
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
            return Err(if err.is_empty() {
                format!("{program} exited with {}", out.status)
            } else {
                err
            });
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    /// The wpctl/pactl spelling of "the default output device".
    fn default_id(backend: Backend) -> &'static str {
        match backend {
            Backend::Wpctl => "@DEFAULT_AUDIO_SINK@",
            Backend::Pactl => "@DEFAULT_SINK@",
        }
    }

    pub fn list_devices() -> Result<Vec<AudioDevice>, String> {
        match backend()? {
            Backend::Wpctl => {
                // Sinks lines look like:  │  *   52. Built-in Audio [vol: 0.40]
                let status = run("wpctl", &["status"])?;
                let mut devices = Vec::new();
                let mut in_sinks = false;
                for line in status.lines() {
                    let body: String = line
                        .chars()
                        .filter(|c| !matches!(c, '│' | '├' | '└' | '─'))
                        .collect();
                    let body = body.trim();
                    if body.starts_with("Sinks:") {
                        in_sinks = true;
                        continue;
                    }
                    if in_sinks {
                        if body.ends_with(':') || body.is_empty() {
                            // next section ("Sources:") or the blank spacer line
                            if body.ends_with(':') {
                                break;
                            }
                            continue;
                        }
                        let is_default = body.starts_with('*');
                        let body = body.trim_start_matches('*').trim();
                        let Some((id, rest)) = body.split_once('.') else {
                            continue;
                        };
                        let id = id.trim();
                        if id.parse::<u32>().is_err() {
                            continue;
                        }
                        let name = rest
                            .split("[vol:")
                            .next()
                            .unwrap_or(rest)
                            .trim()
                            .to_string();
                        devices.push(AudioDevice {
                            id: id.to_string(),
                            name,
                            is_default,
                        });
                    }
                }
                devices.sort_by(|a, b| {
                    b.is_default
                        .cmp(&a.is_default)
                        .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                });
                Ok(devices)
            }
            Backend::Pactl => {
                let default = run("pactl", &["get-default-sink"])
                    .map(|s| s.trim().to_string())
                    .unwrap_or_default();
                let short = run("pactl", &["list", "short", "sinks"])?;
                // Friendly names come from the long listing's Description: lines,
                // in the same sink order as the short listing.
                let descriptions: Vec<String> = run("pactl", &["list", "sinks"])
                    .map(|long| {
                        long.lines()
                            .filter_map(|l| l.trim().strip_prefix("Description:"))
                            .map(|d| d.trim().to_string())
                            .collect()
                    })
                    .unwrap_or_default();
                let mut devices: Vec<AudioDevice> = short
                    .lines()
                    .enumerate()
                    .filter_map(|(i, line)| {
                        let name = line.split('\t').nth(1)?.to_string();
                        Some(AudioDevice {
                            is_default: name == default,
                            name: descriptions.get(i).cloned().unwrap_or_else(|| name.clone()),
                            id: name,
                        })
                    })
                    .collect();
                devices.sort_by(|a, b| {
                    b.is_default
                        .cmp(&a.is_default)
                        .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                });
                Ok(devices)
            }
        }
    }

    pub fn get_volume(id: Option<&str>) -> Result<f32, String> {
        let backend = backend()?;
        let id = id.unwrap_or(default_id(backend));
        match backend {
            Backend::Wpctl => {
                // "Volume: 0.40" or "Volume: 0.40 [MUTED]"
                let out = run("wpctl", &["get-volume", id])?;
                out.split_whitespace()
                    .nth(1)
                    .and_then(|v| v.parse::<f32>().ok())
                    // PipeWire allows >1.0 (overdrive); the UI slider is 0..=1.
                    .map(|v| v.clamp(0.0, 1.0))
                    .ok_or_else(|| format!("unexpected wpctl output: {out}"))
            }
            Backend::Pactl => {
                // "Volume: front-left: 26214 /  40% / ..." — first percentage.
                let out = run("pactl", &["get-sink-volume", id])?;
                out.split('/')
                    .find_map(|part| {
                        part.trim().strip_suffix('%')?.trim().parse::<f32>().ok()
                    })
                    .map(|pct| (pct / 100.0).clamp(0.0, 1.0))
                    .ok_or_else(|| format!("unexpected pactl output: {out}"))
            }
        }
    }

    pub fn set_volume(level: f32, id: Option<&str>) -> Result<(), String> {
        let backend = backend()?;
        let id = id.unwrap_or(default_id(backend));
        match backend {
            Backend::Wpctl => {
                run("wpctl", &["set-volume", id, &format!("{level:.2}")]).map(|_| ())
            }
            Backend::Pactl => {
                let pct = (level * 100.0).round() as u32;
                run("pactl", &["set-sink-volume", id, &format!("{pct}%")]).map(|_| ())
            }
        }
    }

    pub fn toggle_mute(id: Option<&str>) -> Result<bool, String> {
        let backend = backend()?;
        let id = id.unwrap_or(default_id(backend));
        match backend {
            Backend::Wpctl => {
                run("wpctl", &["set-mute", id, "toggle"])?;
                let out = run("wpctl", &["get-volume", id])?;
                Ok(out.contains("[MUTED]"))
            }
            Backend::Pactl => {
                run("pactl", &["set-sink-mute", id, "toggle"])?;
                let out = run("pactl", &["get-sink-mute", id])?;
                Ok(out.to_lowercase().contains("yes"))
            }
        }
    }
}

/// CoreAudio-backed volume control for the current default output. A single
/// pseudo-device is exposed because Commandeer controls the system-selected
/// output, matching the macOS menu-bar volume control.
#[cfg(target_os = "macos")]
mod mac {
    use super::AudioDevice;
    use std::ffi::c_void;

    type AudioObjectId = u32;
    type OsStatus = i32;

    #[repr(C)]
    struct PropertyAddress {
        selector: u32,
        scope: u32,
        element: u32,
    }

    const SYSTEM_OBJECT: AudioObjectId = 1;
    const DEFAULT_OUTPUT: u32 = u32::from_be_bytes(*b"dOut");
    const VIRTUAL_MAIN_VOLUME: u32 = u32::from_be_bytes(*b"vmvc");
    const VOLUME_SCALAR: u32 = u32::from_be_bytes(*b"volm");
    const MUTE: u32 = u32::from_be_bytes(*b"mute");
    const SCOPE_GLOBAL: u32 = u32::from_be_bytes(*b"glob");
    const SCOPE_OUTPUT: u32 = u32::from_be_bytes(*b"outp");
    const ELEMENT_MAIN: u32 = 0;

    #[link(name = "CoreAudio", kind = "framework")]
    extern "C" {
        fn AudioObjectGetPropertyData(
            object: AudioObjectId,
            address: *const PropertyAddress,
            qualifier_size: u32,
            qualifier_data: *const c_void,
            data_size: *mut u32,
            data: *mut c_void,
        ) -> OsStatus;
        fn AudioObjectSetPropertyData(
            object: AudioObjectId,
            address: *const PropertyAddress,
            qualifier_size: u32,
            qualifier_data: *const c_void,
            data_size: u32,
            data: *const c_void,
        ) -> OsStatus;
    }

    fn address(selector: u32, scope: u32, element: u32) -> PropertyAddress {
        PropertyAddress {
            selector,
            scope,
            element,
        }
    }

    fn status_error(action: &str, status: OsStatus) -> String {
        let bytes = (status as u32).to_be_bytes();
        let printable = bytes.iter().all(|byte| byte.is_ascii_graphic());
        if printable {
            format!("CoreAudio {action} failed: '{}' ({status})", String::from_utf8_lossy(&bytes))
        } else {
            format!("CoreAudio {action} failed: {status}")
        }
    }

    fn get_property<T: Copy + Default>(
        object: AudioObjectId,
        address: &PropertyAddress,
    ) -> Result<T, OsStatus> {
        let mut value = T::default();
        let mut size = std::mem::size_of::<T>() as u32;
        let status = unsafe {
            AudioObjectGetPropertyData(
                object,
                address,
                0,
                std::ptr::null(),
                &mut size,
                (&mut value as *mut T).cast(),
            )
        };
        if status == 0 { Ok(value) } else { Err(status) }
    }

    fn set_property<T>(
        object: AudioObjectId,
        address: &PropertyAddress,
        value: &T,
    ) -> Result<(), OsStatus> {
        let status = unsafe {
            AudioObjectSetPropertyData(
                object,
                address,
                0,
                std::ptr::null(),
                std::mem::size_of::<T>() as u32,
                (value as *const T).cast(),
            )
        };
        if status == 0 { Ok(()) } else { Err(status) }
    }

    fn default_output() -> Result<AudioObjectId, String> {
        let device = get_property(
            SYSTEM_OBJECT,
            &address(DEFAULT_OUTPUT, SCOPE_GLOBAL, ELEMENT_MAIN),
        )
        .map_err(|status| status_error("default-output lookup", status))?;
        if device == 0 {
            Err("CoreAudio has no default output device".to_string())
        } else {
            Ok(device)
        }
    }

    fn read_volume(device: AudioObjectId) -> Result<f32, String> {
        for selector in [VIRTUAL_MAIN_VOLUME, VOLUME_SCALAR] {
            if let Ok(value) = get_property::<f32>(
                device,
                &address(selector, SCOPE_OUTPUT, ELEMENT_MAIN),
            ) {
                return Ok(value.clamp(0.0, 1.0));
            }
        }

        // Some devices expose volume only on the left/right channels.
        let channels: Vec<f32> = [1, 2]
            .into_iter()
            .filter_map(|element| {
                get_property(device, &address(VOLUME_SCALAR, SCOPE_OUTPUT, element)).ok()
            })
            .collect();
        if channels.is_empty() {
            Err("CoreAudio output device has no volume control".to_string())
        } else {
            Ok((channels.iter().sum::<f32>() / channels.len() as f32).clamp(0.0, 1.0))
        }
    }

    fn write_volume(device: AudioObjectId, level: f32) -> Result<(), String> {
        let level = level.clamp(0.0, 1.0);
        let mut last_status = None;
        for selector in [VIRTUAL_MAIN_VOLUME, VOLUME_SCALAR] {
            match set_property(device, &address(selector, SCOPE_OUTPUT, ELEMENT_MAIN), &level) {
                Ok(()) => return Ok(()),
                Err(status) => last_status = Some(status),
            }
        }

        let mut wrote_channel = false;
        for element in [1, 2] {
            if set_property(device, &address(VOLUME_SCALAR, SCOPE_OUTPUT, element), &level).is_ok() {
                wrote_channel = true;
            }
        }
        if wrote_channel {
            Ok(())
        } else {
            Err(status_error("volume write", last_status.unwrap_or(-1)))
        }
    }

    fn mute_addresses(device: AudioObjectId) -> Vec<(PropertyAddress, u32)> {
        [ELEMENT_MAIN, 1, 2]
            .into_iter()
            .filter_map(|element| {
                let address = address(MUTE, SCOPE_OUTPUT, element);
                get_property::<u32>(device, &address)
                    .ok()
                    .map(|value| (address, value))
            })
            .collect()
    }

    pub fn list_devices() -> Result<Vec<AudioDevice>, String> {
        Ok(vec![AudioDevice {
            id: "default".to_string(),
            name: "System Output".to_string(),
            is_default: true,
        }])
    }

    pub fn get_volume() -> Result<f32, String> {
        read_volume(default_output()?)
    }

    pub fn set_volume(level: f32) -> Result<(), String> {
        write_volume(default_output()?, level)
    }

    pub fn toggle_mute() -> Result<bool, String> {
        let device = default_output()?;
        let addresses = mute_addresses(device);
        if addresses.is_empty() {
            return Err("CoreAudio output device has no mute control".to_string());
        }
        let muted = addresses.iter().all(|(_, value)| *value != 0);
        let next = u32::from(!muted);
        let mut wrote = false;
        let mut last_status = None;
        for (address, _) in addresses {
            match set_property(device, &address, &next) {
                Ok(()) => wrote = true,
                Err(status) => last_status = Some(status),
            }
        }
        if wrote {
            Ok(!muted)
        } else {
            Err(status_error("mute write", last_status.unwrap_or(-1)))
        }
    }

    #[cfg(test)]
    mod tests {
        // Mirrors the Windows smoke test: real CoreAudio round-trip; writing
        // back the level just read changes nothing audible.
        #[test]
        fn smoke_volume_roundtrip() {
            let level = match super::get_volume() {
                Ok(level) => level,
                Err(error)
                    if error.contains("no default output device")
                        || error.contains("has no volume control") =>
                {
                    return;
                }
                Err(error) => panic!("get_volume: {error}"),
            };
            assert!((0.0..=1.0).contains(&level), "level {level}");
            super::set_volume(level).expect("set_volume");
        }
    }
}

#[cfg(target_os = "windows")]
mod win {
    use super::AudioDevice;
    use windows::core::HSTRING;
    use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
    use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
    use windows::Win32::Media::Audio::{
        eMultimedia, eRender, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator,
        DEVICE_STATE_ACTIVE,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, COINIT_MULTITHREADED,
        STGM_READ,
    };

    /// COM is initialized per-call on the (blocking-pool) thread; re-init is
    /// harmless.
    fn enumerator() -> Result<IMMDeviceEnumerator, String> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(|e| e.to_string())
        }
    }

    /// Resolve a device by endpoint id, or the default render endpoint.
    fn device(id: Option<&str>) -> Result<IMMDevice, String> {
        unsafe {
            let enumerator = enumerator()?;
            match id {
                Some(id) => enumerator
                    .GetDevice(&HSTRING::from(id))
                    .map_err(|e| e.to_string()),
                None => enumerator
                    .GetDefaultAudioEndpoint(eRender, eMultimedia)
                    .map_err(|e| e.to_string()),
            }
        }
    }

    fn endpoint(id: Option<&str>) -> Result<IAudioEndpointVolume, String> {
        unsafe {
            device(id)?
                .Activate(CLSCTX_ALL, None)
                .map_err(|e| e.to_string())
        }
    }

    /// A device's endpoint id (GetId allocates; copy out and free).
    fn device_id(device: &IMMDevice) -> Result<String, String> {
        unsafe {
            let pw = device.GetId().map_err(|e| e.to_string())?;
            let id = pw.to_string().map_err(|e| e.to_string());
            CoTaskMemFree(Some(pw.as_ptr() as *const _));
            id
        }
    }

    pub fn list_devices() -> Result<Vec<AudioDevice>, String> {
        unsafe {
            let enumerator = enumerator()?;
            let default_id = enumerator
                .GetDefaultAudioEndpoint(eRender, eMultimedia)
                .ok()
                .and_then(|d| device_id(&d).ok());

            let collection = enumerator
                .EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)
                .map_err(|e| e.to_string())?;
            let count = collection.GetCount().map_err(|e| e.to_string())?;

            let mut devices = Vec::with_capacity(count as usize);
            for i in 0..count {
                let device = match collection.Item(i) {
                    Ok(d) => d,
                    Err(_) => continue, // device vanished mid-enumeration
                };
                let Ok(id) = device_id(&device) else { continue };
                let name = device
                    .OpenPropertyStore(STGM_READ)
                    .and_then(|store| store.GetValue(&PKEY_Device_FriendlyName))
                    .map(|v| v.to_string())
                    .unwrap_or_else(|_| id.clone());
                devices.push(AudioDevice {
                    is_default: default_id.as_deref() == Some(id.as_str()),
                    id,
                    name,
                });
            }
            // Default first, then alphabetical — the common case is adjusting
            // whatever is currently playing.
            devices.sort_by(|a, b| {
                b.is_default
                    .cmp(&a.is_default)
                    .then_with(|| a.name.cmp(&b.name))
            });
            Ok(devices)
        }
    }

    pub fn get_volume(id: Option<&str>) -> Result<f32, String> {
        unsafe {
            endpoint(id)?
                .GetMasterVolumeLevelScalar()
                .map_err(|e| e.to_string())
        }
    }

    pub fn set_volume(level: f32, id: Option<&str>) -> Result<(), String> {
        unsafe {
            endpoint(id)?
                .SetMasterVolumeLevelScalar(level, std::ptr::null())
                .map_err(|e| e.to_string())
        }
    }

    pub fn toggle_mute(id: Option<&str>) -> Result<bool, String> {
        unsafe {
            let ep = endpoint(id)?;
            let muted = ep.GetMute().map_err(|e| e.to_string())?.as_bool();
            ep.SetMute(!muted, std::ptr::null())
                .map_err(|e| e.to_string())?;
            Ok(!muted)
        }
    }

    #[cfg(test)]
    mod tests {
        // Exercises the real Core Audio path (COM init, enumeration, endpoint
        // activation, get + set). Writing back the level just read changes
        // nothing audible.
        #[test]
        fn smoke_devices_and_volume_roundtrip() {
            let devices = super::list_devices().expect("list_devices");
            if devices.is_empty() {
                // Headless environments (CI runners) have no audio endpoints;
                // enumeration succeeding is all that can be exercised there.
                eprintln!("skipping volume roundtrip: no active output devices");
                return;
            }
            assert!(devices[0].is_default);

            for d in &devices {
                let level = super::get_volume(Some(&d.id)).expect("get_volume");
                assert!((0.0..=1.0).contains(&level), "{}: {level}", d.name);
                super::set_volume(level, Some(&d.id)).expect("set_volume");
            }

            let default_level = super::get_volume(None).expect("default get_volume");
            assert!((0.0..=1.0).contains(&default_level));
        }
    }
}
