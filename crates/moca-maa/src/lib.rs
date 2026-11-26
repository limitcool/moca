pub mod discovery;
pub mod hotkey;
pub mod session;
mod win32_util;

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use maa_framework::MaaError;
use maa_framework::toolkit::Toolkit;
use thiserror::Error;

pub use discovery::{DiscoveredAdb, DiscoveredWin32, discover_adb_devices, discover_win32_windows};
pub use hotkey::{GlobalHotkeyAction, GlobalHotkeyListener};
pub use session::{
    MaaSession, OcrReader, SessionProgressEvent, SessionRunner, load_bundles, ocr_image_buffer_roi,
    run_task_entry,
};
pub use win32_util::{
    apply_window_icon_from_exe, foreground_window_hwnd, is_current_process_elevated,
    is_current_process_foreground, open_external_url, relaunch_as_admin, set_app_user_model_id,
    toggle_window_foreground,
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
        for _ in 0..320 {
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

    #[test]
    fn verifies_maa_native_ocr_on_redeem_templates() {
        use maa_framework::buffer::MaaImageBuffer;
        use maa_framework::controller::Controller;
        use maa_framework::custom_controller::CustomControllerCallback;
        use maa_framework::resource::Resource;
        use maa_framework::tasker::Tasker;

        let bundle_dir =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../games/azur-promilia/resource");
        init_maa(None).expect("init maa");

        struct DummyOcrCtrl {
            frame_png: Vec<u8>,
        }
        impl CustomControllerCallback for DummyOcrCtrl {
            fn connect(&self) -> bool {
                true
            }
            fn request_uuid(&self) -> Option<String> {
                Some("mock-ocr-ctrl".into())
            }
            fn screencap(&self) -> Option<Vec<u8>> {
                Some(self.frame_png.clone())
            }
        }

        // Compose a 1280x720 canvas placing both templates inside clear regions so Maa PP-OCR
        // loads games/azur-promilia/resource/model/ocr/ and recognizes them at realistic scale.
        let img_dir = bundle_dir.join("image");
        let mut screen = image::RgbaImage::from_pixel(1280, 720, image::Rgba([242, 242, 242, 255]));
        let entry_img = image::open(img_dir.join("redeem_entry_btn.png"))
            .expect("open redeem_entry_btn")
            .to_rgba8();
        image::imageops::overlay(&mut screen, &entry_img, 1000, 60);
        let title_img = image::open(img_dir.join("redeem_popup_title.png"))
            .expect("open redeem_popup_title")
            .to_rgba8();
        image::imageops::overlay(&mut screen, &title_img, 64, 150);

        let mut frame_png = Vec::new();
        image::DynamicImage::ImageRgba8(screen)
            .write_to(
                &mut std::io::Cursor::new(&mut frame_png),
                image::ImageFormat::Png,
            )
            .expect("encode ocr test canvas");

        let ctrl = Controller::new_custom(DummyOcrCtrl {
            frame_png: frame_png.clone(),
        })
        .expect("custom ocr ctrl");
        assert!(ctrl.wait(ctrl.post_connection().expect("conn")).succeeded());

        let res = Resource::new().expect("res");
        load_bundles(&res, &[bundle_dir]).expect("load ap bundle with model/ocr");
        let tasker = Tasker::new().expect("tasker");
        tasker.bind_controller(&ctrl).expect("bind ctrl");
        tasker.bind_resource(&res).expect("bind res");

        let mut buf = MaaImageBuffer::new().expect("img buffer");
        buf.set_encoded(&frame_png).expect("set encoded png");

        let entry_txt = ocr_image_buffer_roi(&tasker, &buf, Some([980, 40, 220, 90]))
            .expect("maa ocr entry btn");
        assert!(
            entry_txt.contains("点击兑换"),
            "expected Maa PP-OCR to read 点击兑换, got: {entry_txt:?}"
        );

        let title_txt = ocr_image_buffer_roi(&tasker, &buf, Some([40, 130, 260, 90]))
            .expect("maa ocr popup title");
        assert!(
            title_txt.contains("兑换码"),
            "expected Maa PP-OCR to read 兑换码, got: {title_txt:?}"
        );
    }

    #[test]
    fn verifies_redeem_pipeline_and_batch_override() {
        use maa_framework::controller::Controller;
        use maa_framework::custom_controller::CustomControllerCallback;
        use maa_framework::resource::Resource;
        use maa_framework::tasker::Tasker;
        use std::sync::{Arc, Mutex};

        let iface_path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../games/azur-promilia/interface.json");
        let manifest =
            moca_protocol::GameInterface::load_from_file(&iface_path).expect("load ap interface");
        let task = manifest
            .task
            .get("RedeemCodes")
            .expect("RedeemCodes task declared");
        assert_eq!(
            task.kind,
            moca_protocol::TaskKind::Action,
            "RedeemCodes should be a one-shot Action task without toggle switch"
        );
        assert_eq!(
            task.post_entry.as_deref(),
            Some("AP_RedeemFinishCleanup"),
            "RedeemCodes should clean up open redeem popups on completion"
        );
        let items = manifest.resolve_batch_items(task, &std::collections::HashMap::new());
        assert_eq!(items.len(), 13, "expected 13 built-in redeem codes");
        assert_eq!(items[0].item_label.as_deref(), Some("LSXY520"));
        assert_eq!(items[12].item_label.as_deref(), Some("LSXYLIANXIANG1126"));
        let ov_text = items[0]
            .pipeline_override
            .as_ref()
            .expect("override present")["AP_RedeemTypeCode"]["input_text"]
            .as_str()
            .expect("input_text string");
        assert_eq!(ov_text, "LSXY520");

        let bundle_dir =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../games/azur-promilia/resource");
        init_maa(None).expect("init maa");

        // Build a synthetic 1280x720 redeem popup screen placing the real cancel & confirm templates
        // inside their OCR ROIs and verify OCR-anchored input click, InputText("LSXY520"), and Confirm click.
        let img_dir = bundle_dir.join("image");
        let mut screen = image::RgbaImage::from_pixel(1280, 720, image::Rgba([235, 235, 235, 255]));
        let cancel = image::open(img_dir.join("redeem_cancel_btn.png"))
            .expect("open cancel")
            .to_rgba8();
        image::imageops::overlay(&mut screen, &cancel, 380, 440);
        let confirm = image::open(img_dir.join("redeem_confirm_btn.png"))
            .expect("open confirm")
            .to_rgba8();
        image::imageops::overlay(&mut screen, &confirm, 740, 440);

        let mut popup_png = Vec::new();
        image::DynamicImage::ImageRgba8(screen)
            .write_to(
                &mut std::io::Cursor::new(&mut popup_png),
                image::ImageFormat::Png,
            )
            .expect("encode popup png");

        #[derive(Default)]
        struct RedeemLog {
            clicks: Vec<(i32, i32)>,
            texts: Vec<String>,
        }
        struct MockRedeemCtrl {
            frames: Vec<Vec<u8>>,
            log: Arc<Mutex<RedeemLog>>,
        }
        impl CustomControllerCallback for MockRedeemCtrl {
            fn connect(&self) -> bool {
                true
            }
            fn request_uuid(&self) -> Option<String> {
                Some("mock-redeem-ctrl".into())
            }
            fn screencap(&self) -> Option<Vec<u8>> {
                let idx = self
                    .log
                    .lock()
                    .unwrap()
                    .clicks
                    .len()
                    .min(self.frames.len().saturating_sub(1));
                Some(self.frames[idx].clone())
            }
            fn click(&self, x: i32, y: i32) -> bool {
                self.log.lock().unwrap().clicks.push((x, y));
                true
            }
            fn key_down(&self, _keycode: i32) -> bool {
                true
            }
            fn key_up(&self, _keycode: i32) -> bool {
                true
            }
            fn click_key(&self, _keycode: i32) -> bool {
                true
            }
            fn input_text(&self, text: &str) -> bool {
                self.log.lock().unwrap().texts.push(text.to_string());
                true
            }
        }

        let log = Arc::new(Mutex::new(RedeemLog::default()));
        let ctrl = Controller::new_custom(MockRedeemCtrl {
            frames: vec![popup_png.clone()],
            log: Arc::clone(&log),
        })
        .expect("custom redeem ctrl");
        assert!(ctrl.wait(ctrl.post_connection().expect("conn")).succeeded());

        let res = Resource::new().expect("res");
        load_bundles(&res, std::slice::from_ref(&bundle_dir)).expect("load ap bundle");
        let tasker = Tasker::new().expect("tasker");
        tasker.bind_controller(&ctrl).expect("bind ctrl");
        tasker.bind_resource(&res).expect("bind res");

        let ok = run_task_entry(
            &tasker,
            "AP_RedeemSubmitOne",
            items[0].pipeline_override.as_ref(),
        )
        .expect("run redeem single");
        assert!(ok, "AP_RedeemSubmitOne should succeed");

        let recorded = log.lock().unwrap();
        assert_eq!(recorded.texts, vec!["LSXY520".to_string()]);
        assert_eq!(
            recorded.clicks.len(),
            2,
            "expected input click + confirm click"
        );
        // First click should anchor on 「取消」 and offset right-up into the input box
        assert!(
            (540..=760).contains(&recorded.clicks[0].0)
                && (280..=420).contains(&recorded.clicks[0].1),
            "unexpected input focus click: {:?}",
            recorded.clicks[0]
        );
        // Second click should be inside 确认 button (x=740..1095, y=440..511)
        assert!(
            (740..=1095).contains(&recorded.clicks[1].0)
                && (440..=511).contains(&recorded.clicks[1].1),
            "unexpected confirm click: {:?}",
            recorded.clicks[1]
        );

        // Regression test on the real 1280x720 account page capture if present on disk.
        let real_account_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../logs/debug/on_error/2026.11.26-17.29.02.657_AP_RedeemClickAccountTab.png");
        if real_account_path.exists() {
            let account_png = std::fs::read(&real_account_path).expect("read real account png");
            let entry_log = Arc::new(Mutex::new(RedeemLog::default()));
            let entry_ctrl = Controller::new_custom(MockRedeemCtrl {
                frames: vec![account_png, popup_png.clone()],
                log: Arc::clone(&entry_log),
            })
            .expect("custom entry ctrl");
            assert!(
                entry_ctrl
                    .wait(entry_ctrl.post_connection().expect("conn"))
                    .succeeded()
            );

            let entry_tasker = Tasker::new().expect("entry tasker");
            entry_tasker
                .bind_controller(&entry_ctrl)
                .expect("bind entry ctrl");
            entry_tasker.bind_resource(&res).expect("bind entry res");

            let entry_ok = run_task_entry(&entry_tasker, "AP_RedeemClickEntry", None)
                .expect("run entry click");
            assert!(entry_ok, "AP_RedeemClickEntry should match real capture");
            let clicks = &entry_log.lock().unwrap().clicks;
            assert_eq!(clicks.len(), 1, "expected exactly 1 click on 点击兑换");
            assert!(
                (960..=1100).contains(&clicks[0].0) && (135..=180).contains(&clicks[0].1),
                "expected click on 点击兑换 (960..1100, 135..180), got {:?}",
                clicks[0]
            );
        }

        // Verify post_entry cleanup clicks 「取消」 to exit the open redeem popup back to account page.
        let cleanup_log = Arc::new(Mutex::new(RedeemLog::default()));
        let cleanup_ctrl = Controller::new_custom(MockRedeemCtrl {
            frames: vec![popup_png],
            log: Arc::clone(&cleanup_log),
        })
        .expect("custom cleanup ctrl");
        assert!(
            cleanup_ctrl
                .wait(cleanup_ctrl.post_connection().expect("conn"))
                .succeeded()
        );
        let cleanup_tasker = Tasker::new().expect("cleanup tasker");
        cleanup_tasker
            .bind_controller(&cleanup_ctrl)
            .expect("bind cleanup ctrl");
        cleanup_tasker
            .bind_resource(&res)
            .expect("bind cleanup res");
        let cleanup_ok = run_task_entry(&cleanup_tasker, "AP_RedeemFinishCleanup", None)
            .expect("run redeem cleanup");
        assert!(cleanup_ok, "AP_RedeemFinishCleanup should succeed");
        let cleanup_clicks = &cleanup_log.lock().unwrap().clicks;
        assert_eq!(
            cleanup_clicks.len(),
            1,
            "expected exactly 1 click on 「取消」"
        );
        assert!(
            (380..=676).contains(&cleanup_clicks[0].0)
                && (440..=511).contains(&cleanup_clicks[0].1),
            "unexpected cancel button click: {:?}",
            cleanup_clicks[0]
        );
    }

    #[test]
    fn verifies_story_skip_ignores_redeem_popup() {
        use maa_framework::controller::Controller;
        use maa_framework::custom_controller::CustomControllerCallback;
        use maa_framework::resource::Resource;
        use maa_framework::tasker::Tasker;
        use std::sync::{Arc, Mutex};

        let redeem_popup_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../logs/debug/on_error/2026.11.26-17.42.18.113_AP_StoryDialogConfirm.png");
        let sdk_dll =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor/maa-sdk/bin/MaaFramework.dll");
        if !redeem_popup_path.exists() || !sdk_dll.exists() {
            return;
        }

        init_maa(Some(&sdk_dll)).expect("init maa");
        let redeem_png =
            std::fs::read(&redeem_popup_path).expect("read blank redeem popup capture");

        struct MockRedeemGuardCtrl {
            png: Vec<u8>,
            clicks: Arc<Mutex<Vec<(i32, i32)>>>,
        }
        impl CustomControllerCallback for MockRedeemGuardCtrl {
            fn connect(&self) -> bool {
                true
            }
            fn request_uuid(&self) -> Option<String> {
                Some("mock-story-redeem-guard-uuid".to_string())
            }
            fn screencap(&self) -> Option<Vec<u8>> {
                Some(self.png.clone())
            }
            fn click(&self, x: i32, y: i32) -> bool {
                self.clicks.lock().unwrap().push((x, y));
                true
            }
        }

        let clicks = Arc::new(Mutex::new(Vec::new()));
        let ctrl = Controller::new_custom(MockRedeemGuardCtrl {
            png: redeem_png,
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
            .expect("post story skip task");
        std::thread::sleep(std::time::Duration::from_millis(1600));
        if let Ok(stop_id) = tasker.post_stop()
            && stop_id != 0
        {
            let _ = maa_framework::job::Job::for_tasker(&tasker, stop_id).wait();
        }
        let _ = job.wait();

        let recorded = clicks.lock().unwrap();
        assert!(
            recorded.is_empty(),
            "story skip must ignore the redeem dialog confirm button, got clicks: {recorded:?}"
        );
    }
}
