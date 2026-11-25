pub mod discovery;
pub mod session;
mod win32_util;

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use maa_framework::MaaError;
use maa_framework::toolkit::Toolkit;
use thiserror::Error;

pub use discovery::{DiscoveredAdb, DiscoveredWin32, discover_adb_devices, discover_win32_windows};
pub use session::{MaaSession, SessionProgressEvent, load_bundles, run_task_entry};
pub use win32_util::is_current_process_elevated;

#[derive(Debug, Error)]
pub enum MaaAdapterError {
    #[error("MaaFramework DLL not found or failed to load: {0}")]
    LibraryLoad(String),
    #[error("MaaFramework error: {0}")]
    Framework(#[from] MaaError),
    #[error("Controller operation `{op}` failed with status {status}")]
    ControllerFailed { op: &'static str, status: String },
    #[error("Resource operation `{op}` failed with status {status}")]
    ResourceFailed { op: &'static str, status: String },
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

static MAA_INIT: OnceLock<Result<String, String>> = OnceLock::new();

/// Initialize MaaFramework dynamic library and toolkit logging once per process.
///
/// Returns the loaded MaaFramework version string on success.
pub fn init_maa(custom_dll_path: Option<&Path>) -> Result<&'static str, MaaAdapterError> {
    let res = MAA_INIT.get_or_init(|| init_maa_inner(custom_dll_path));
    match res {
        Ok(ver) => Ok(ver.as_str()),
        Err(msg) => Err(MaaAdapterError::LibraryLoad(msg.clone())),
    }
}

fn init_maa_inner(custom_dll_path: Option<&Path>) -> Result<String, String> {
    let lib_name = if cfg!(target_os = "windows") {
        "MaaFramework.dll"
    } else if cfg!(target_os = "macos") {
        "libMaaFramework.dylib"
    } else {
        "libMaaFramework.so"
    };

    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(p) = custom_dll_path {
        candidates.push(p.to_path_buf());
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        candidates.push(dir.join(lib_name));
    }
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("vendor").join("maa-sdk").join("bin").join(lib_name));
        candidates.push(cwd.join("bin").join(lib_name));
    }

    if let Some(found) = candidates.into_iter().find(|p| p.exists()) {
        tracing::info!("Loading MaaFramework library from {}", found.display());
        maa_framework::load_library(&found)?;
    } else {
        maa_framework::ensure_library_loaded()?;
    }

    let log_dir = Path::new("logs");
    let _ = std::fs::create_dir_all(log_dir);
    Toolkit::init_option("./logs", "{}").map_err(|e| format!("Toolkit::init_option failed: {e}"))?;

    Ok(maa_framework::maa_version().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_maa_sdk_and_azur_promilia_bundle() {
        let sdk_dll = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../vendor/maa-sdk/bin/MaaFramework.dll");
        if sdk_dll.exists() {
            let ver = init_maa(Some(&sdk_dll)).expect("MaaFramework DLL should load");
            assert!(!ver.is_empty());

            let ap_res_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../games/azur-promilia/resource");
            if ap_res_dir.is_dir() {
                let res = maa_framework::resource::Resource::new().expect("resource create");
                load_bundles(&res, &[ap_res_dir]).expect("azur-promilia bundle should load cleanly");
                let nodes = res.node_list().expect("node_list");
                assert!(nodes.iter().any(|n| n == "AP_DailyLoginEntry"));
                assert!(nodes.iter().any(|n| n == "AP_StaminaFarmEntry"));
            }
        }
    }
}
