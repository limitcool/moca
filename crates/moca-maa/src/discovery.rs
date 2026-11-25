use std::path::PathBuf;

use maa_framework::toolkit::Toolkit;

use crate::{MaaAdapterError, win32_util};

#[derive(Debug, Clone)]
pub struct DiscoveredAdb {
    pub name: String,
    pub adb_path: PathBuf,
    pub address: String,
    pub config_json: String,
}

#[derive(Debug, Clone)]
pub struct DiscoveredWin32 {
    pub hwnd: isize,
    pub title: String,
    pub class_name: String,
    pub client_size: Option<(i32, i32)>,
    pub is_elevated: Option<bool>,
}

/// Discover connected ADB devices and supported Android emulators.
pub fn discover_adb_devices() -> Result<Vec<DiscoveredAdb>, MaaAdapterError> {
    let devices = Toolkit::find_adb_devices()?;
    Ok(devices
        .into_iter()
        .map(|d| DiscoveredAdb {
            name: d.name,
            adb_path: d.adb_path,
            address: d.address,
            config_json: d.config.to_string(),
        })
        .collect())
}

/// Discover visible desktop windows suitable for Win32 automation.
pub fn discover_win32_windows() -> Result<Vec<DiscoveredWin32>, MaaAdapterError> {
    let windows = Toolkit::find_desktop_windows()?;
    let mut results = Vec::new();
    for w in windows {
        if w.window_name.trim().is_empty() {
            continue;
        }
        let hwnd = w.hwnd as isize;
        let client_size = win32_util::get_client_size(hwnd);
        if let Some((cw, ch)) = client_size
            && (cw < 200 || ch < 150)
        {
            continue;
        }
        let is_elevated = win32_util::is_window_process_elevated(hwnd);
        results.push(DiscoveredWin32 {
            hwnd,
            title: w.window_name,
            class_name: w.class_name,
            client_size,
            is_elevated,
        });
    }
    Ok(results)
}
