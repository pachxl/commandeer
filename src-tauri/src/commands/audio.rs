//! System volume control via the Windows Core Audio API (IAudioEndpointVolume,
//! per render endpoint). Backs the per-device volume sliders and mute toggle.
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
    #[cfg(not(target_os = "windows"))]
    {
        Err("volume control is only implemented on Windows".to_string())
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
    #[cfg(not(target_os = "windows"))]
    {
        let _ = device;
        Err("volume control is only implemented on Windows".to_string())
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
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (level, device);
        Err("volume control is only implemented on Windows".to_string())
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
    #[cfg(not(target_os = "windows"))]
    {
        let _ = device;
        Err("volume control is only implemented on Windows".to_string())
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
            assert!(!devices.is_empty(), "no active output devices");
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
