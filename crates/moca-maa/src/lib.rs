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
pub use win32_util::{
    apply_window_icon_from_exe, is_current_process_elevated, open_external_url, relaunch_as_admin,
};

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
    if let Ok(exe) = std::env::current_exe() {
        for ancestor in exe.ancestors().take(5) {
            candidates.push(ancestor.join(lib_name));
            candidates.push(
                ancestor
                    .join("vendor")
                    .join("maa-sdk")
                    .join("bin")
                    .join(lib_name),
            );
            candidates.push(ancestor.join("bin").join(lib_name));
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(
            cwd.join("vendor")
                .join("maa-sdk")
                .join("bin")
                .join(lib_name),
        );
        candidates.push(cwd.join("bin").join(lib_name));
    }

    if let Some(found) = candidates.into_iter().find(|p| p.exists()) {
        tracing::info!("Loading MaaFramework library from {}", found.display());
        maa_framework::load_library(&found)?;
    } else {
        maa_framework::ensure_library_loaded()?;
    }

    let log_dir = std::env::current_exe()
        .ok()
        .and_then(|exe| {
            exe.ancestors()
                .take(5)
                .find(|dir| dir.join("games").is_dir())
                .map(|dir| dir.join("logs"))
        })
        .unwrap_or_else(|| PathBuf::from("logs"));
    let _ = std::fs::create_dir_all(&log_dir);
    Toolkit::init_option(log_dir.to_string_lossy().as_ref(), "{}")
        .map_err(|e| format!("Toolkit::init_option failed: {e}"))?;

    Ok(maa_framework::maa_version().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_maa_sdk_and_azur_promilia_bundle() {
        let sdk_dll =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor/maa-sdk/bin/MaaFramework.dll");
        if sdk_dll.exists() {
            let ver = init_maa(Some(&sdk_dll)).expect("MaaFramework DLL should load");
            assert!(!ver.is_empty());

            let ap_res_dir =
                Path::new(env!("CARGO_MANIFEST_DIR")).join("../../games/azur-promilia/resource");
            if ap_res_dir.is_dir() {
                let res = maa_framework::resource::Resource::new().expect("resource create");
                load_bundles(&res, std::slice::from_ref(&ap_res_dir))
                    .expect("azur-promilia bundle should load cleanly");
                let nodes = res.node_list().expect("node_list");
                assert!(nodes.iter().any(|n| n == "AP_DailyLoginEntry"));
                assert!(nodes.iter().any(|n| n == "AP_StaminaFarmEntry"));
                assert!(nodes.iter().any(|n| n == "AP_AutoSkipStoryEntry"));
                assert!(nodes.iter().any(|n| n == "AP_StoryClickSkipArrow"));
                assert!(nodes.iter().any(|n| n == "AP_StoryConfirmSkip"));
                assert!(nodes.iter().any(|n| n == "AP_StoryWaitConfirmGone"));
                assert!(nodes.iter().any(|n| n == "AP_StoryDialogConfirm"));
                assert!(nodes.iter().any(|n| n == "AP_StoryWaitDialogConfirmGone"));
                assert!(nodes.iter().any(|n| n == "AP_StoryFastNext"));

                // Enforce zero hard-coded sleep (pre_delay == 0 && post_delay == 0) across all pipeline nodes.
                let pipeline_dir = ap_res_dir.join("pipeline");
                for entry in std::fs::read_dir(pipeline_dir).expect("read pipeline dir") {
                    let path = entry.expect("dir entry").path();
                    if path.extension().and_then(|e| e.to_str()) != Some("json") {
                        continue;
                    }
                    let raw = std::fs::read_to_string(&path).expect("read pipeline json");
                    let obj: serde_json::Map<String, serde_json::Value> =
                        serde_json::from_str(&raw).expect("valid pipeline json");
                    for (node_name, node_val) in obj {
                        let pre = node_val
                            .get("pre_delay")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0);
                        let post = node_val
                            .get("post_delay")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0);
                        assert_eq!(
                            pre,
                            0,
                            "Node `{node_name}` in {} violates zero-sleep rule (pre_delay={pre})",
                            path.display()
                        );
                        assert_eq!(
                            post,
                            0,
                            "Node `{node_name}` in {} violates zero-sleep rule (post_delay={post})",
                            path.display()
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn verifies_auto_skip_story_pipeline_with_screenshots() {
        use maa_framework::controller::Controller;
        use maa_framework::custom_controller::CustomControllerCallback;
        use maa_framework::resource::Resource;
        use maa_framework::tasker::Tasker;
        use std::sync::{Arc, Mutex};

        let Ok(scratch_env) = std::env::var("MOCA_TEST_SCRATCH_DIR") else {
            return;
        };
        let scratch = Path::new(&scratch_env);
        let f1_path = scratch.join("frame_story_1.png");
        let f2_path = scratch.join("frame_story_2.png");
        let sdk_dll =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor/maa-sdk/bin/MaaFramework.dll");
        if !f1_path.exists() || !f2_path.exists() || !sdk_dll.exists() {
            return;
        }

        init_maa(Some(&sdk_dll)).expect("init maa");
        let f1 = std::fs::read(&f1_path).expect("read f1");
        let f2 = std::fs::read(&f2_path).expect("read f2");

        struct MockStoryCtrl {
            frame1: Vec<u8>,
            frame2: Vec<u8>,
            frame_empty: Vec<u8>,
            clicks: Arc<Mutex<Vec<(i32, i32)>>>,
        }
        impl CustomControllerCallback for MockStoryCtrl {
            fn connect(&self) -> bool {
                true
            }
            fn request_uuid(&self) -> Option<String> {
                Some("mock-story-uuid".to_string())
            }
            fn screencap(&self) -> Option<Vec<u8>> {
                let count = self.clicks.lock().unwrap().len();
                match count {
                    0 => Some(self.frame1.clone()),
                    1 => Some(self.frame2.clone()),
                    _ => Some(self.frame_empty.clone()),
                }
            }
            fn click(&self, x: i32, y: i32) -> bool {
                self.clicks.lock().unwrap().push((x, y));
                true
            }
        }

        let empty_img = image::DynamicImage::new_rgb8(1280, 720);
        let mut frame_empty = Vec::new();
        empty_img
            .write_to(
                &mut std::io::Cursor::new(&mut frame_empty),
                image::ImageFormat::Png,
            )
            .expect("encode empty png");

        let clicks = Arc::new(Mutex::new(Vec::new()));
        let ctrl = Controller::new_custom(MockStoryCtrl {
            frame1: f1,
            frame2: f2,
            frame_empty,
            clicks: Arc::clone(&clicks),
        })
        .expect("custom controller");
        let conn_status = ctrl.wait(ctrl.post_connection().expect("conn"));
        assert!(conn_status.succeeded());

        let res = Resource::new().expect("resource");
        let ap_res_dir =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../games/azur-promilia/resource");
        load_bundles(&res, &[ap_res_dir]).expect("load ap bundle");

        let tasker = Tasker::new().expect("tasker");
        tasker.bind_controller(&ctrl).expect("bind ctrl");
        tasker.bind_resource(&res).expect("bind res");

        let t0 = std::time::Instant::now();
        let job = tasker
            .post_task("AP_AutoSkipStoryEntry", "{}")
            .expect("post skip task");

        for _ in 0..50 {
            if clicks.lock().unwrap().len() >= 2 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let elapsed = t0.elapsed();
        std::thread::sleep(std::time::Duration::from_millis(200));
        if let Ok(stop_id) = tasker.post_stop() {
            let _ = maa_framework::job::Job::for_tasker(&tasker, stop_id).wait();
        }
        let _ = job.wait();
        drop(tasker);
        drop(ctrl);

        let recorded = clicks.lock().unwrap().clone();
        println!("Recorded AutoSkipStory clicks in {elapsed:?}: {recorded:?}");
        assert_eq!(
            recorded.len(),
            2,
            "expected exact 2 clicks without duplicate click on fade-out: {recorded:?}"
        );
        let (ax, ay) = recorded[0];
        assert!(
            (1100..=1280).contains(&ax) && (0..=90).contains(&ay),
            "arrow click out of ROI: {ax},{ay}"
        );
        let (cx, cy) = recorded[1];
        assert!(
            (640..=1280).contains(&cx) && (380..=720).contains(&cy),
            "confirm click out of ROI: {cx},{cy}"
        );
    }

    #[test]
    fn verifies_dialog_confirm_skip_template_on_real_capture() {
        use maa_framework::controller::Controller;
        use maa_framework::custom_controller::CustomControllerCallback;
        use maa_framework::resource::Resource;
        use maa_framework::tasker::Tasker;
        use std::sync::{Arc, Mutex};

        let capture_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../logs/debug/on_error/2026.11.26-10.23.15.428_AP_StoryClickSkipArrow.png");
        let sdk_dll =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor/maa-sdk/bin/MaaFramework.dll");
        if !capture_path.exists() || !sdk_dll.exists() {
            return;
        }

        init_maa(Some(&sdk_dll)).expect("init maa");
        let dialog_png = std::fs::read(&capture_path).expect("read dialog capture");
        let empty_img = image::DynamicImage::new_rgb8(1280, 720);
        let mut frame_empty = Vec::new();
        empty_img
            .write_to(
                &mut std::io::Cursor::new(&mut frame_empty),
                image::ImageFormat::Png,
            )
            .expect("encode empty png");

        struct MockDialogCtrl {
            dialog_png: Vec<u8>,
            frame_empty: Vec<u8>,
            clicks: Arc<Mutex<Vec<(i32, i32)>>>,
        }
        impl CustomControllerCallback for MockDialogCtrl {
            fn connect(&self) -> bool {
                true
            }
            fn request_uuid(&self) -> Option<String> {
                Some("mock-dialog-confirm-uuid".to_string())
            }
            fn screencap(&self) -> Option<Vec<u8>> {
                if self.clicks.lock().unwrap().is_empty() {
                    Some(self.dialog_png.clone())
                } else {
                    Some(self.frame_empty.clone())
                }
            }
            fn click(&self, x: i32, y: i32) -> bool {
                self.clicks.lock().unwrap().push((x, y));
                true
            }
        }

        let clicks = Arc::new(Mutex::new(Vec::new()));
        let ctrl = Controller::new_custom(MockDialogCtrl {
            dialog_png,
            frame_empty,
            clicks: Arc::clone(&clicks),
        })
        .expect("custom controller");
        assert!(ctrl.wait(ctrl.post_connection().expect("conn")).succeeded());

        let res = Resource::new().expect("resource");
        let ap_res_dir =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../games/azur-promilia/resource");
        load_bundles(&res, &[ap_res_dir]).expect("load ap bundle");

        let tasker = Tasker::new().expect("tasker");
        tasker.bind_controller(&ctrl).expect("bind ctrl");
        tasker.bind_resource(&res).expect("bind res");

        let job = tasker
            .post_task("AP_AutoSkipStoryEntry", "{}")
            .expect("post skip task");
        for _ in 0..40 {
            if !clicks.lock().unwrap().is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        std::thread::sleep(std::time::Duration::from_millis(120));
        if let Ok(stop_id) = tasker.post_stop() {
            let _ = maa_framework::job::Job::for_tasker(&tasker, stop_id).wait();
        }
        let _ = job.wait();
        drop(tasker);
        drop(ctrl);

        let recorded = clicks.lock().unwrap().clone();
        assert_eq!(
            recorded.len(),
            1,
            "expected exact 1 click on ✓ 确认: {recorded:?}"
        );
        let (dx, dy) = recorded[0];
        assert!(
            (720..=940).contains(&dx) && (420..=470).contains(&dy),
            "dialog confirm click out of button bounds: {dx},{dy}"
        );

        // Verify `summary_only` pipeline_override stops at ✓ 确认 without clicking it.
        let iface_path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../games/azur-promilia/interface.json");
        let manifest =
            moca_protocol::GameInterface::load_from_file(&iface_path).expect("load ap interface");
        let mut opts = std::collections::HashMap::new();
        opts.insert(
            "story_skip_confirm_mode".into(),
            serde_json::json!("summary_only"),
        );
        let override_json = manifest
            .resolve_pipeline_override(&manifest.task["AutoSkipStory"], &opts)
            .expect("summary_only override");

        clicks.lock().unwrap().clear();
        let dialog_png2 = std::fs::read(&capture_path).expect("read dialog capture");
        let ctrl2 = Controller::new_custom(MockDialogCtrl {
            dialog_png: dialog_png2,
            frame_empty: Vec::new(),
            clicks: Arc::clone(&clicks),
        })
        .expect("custom controller 2");
        assert!(
            ctrl2
                .wait(ctrl2.post_connection().expect("conn2"))
                .succeeded()
        );

        let tasker2 = Tasker::new().expect("tasker2");
        tasker2.bind_controller(&ctrl2).expect("bind ctrl2");
        tasker2.bind_resource(&res).expect("bind res2");

        let ov_str = override_json.to_string();
        let job2 = tasker2
            .post_task("AP_AutoSkipStoryEntry", &ov_str)
            .expect("post summary_only skip task");
        std::thread::sleep(std::time::Duration::from_millis(250));
        if let Ok(stop_id) = tasker2.post_stop() {
            let _ = maa_framework::job::Job::for_tasker(&tasker2, stop_id).wait();
        }
        let _ = job2.wait();
        drop(tasker2);
        drop(res);
        drop(ctrl2);
        assert!(
            clicks.lock().unwrap().is_empty(),
            "summary_only mode should NOT auto-click ✓ 确认"
        );
    }
}
