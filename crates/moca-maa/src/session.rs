use std::ffi::c_void;
use std::path::{Path, PathBuf};

use image::RgbaImage;
use maa_framework::buffer::MaaImageBuffer;
use maa_framework::common::{OCRResult, Win32InputMethod, Win32ScreencapMethod};
use maa_framework::controller::Controller;
use maa_framework::event_sink;
use maa_framework::job::Job;
use maa_framework::notification::MaaEvent;
use maa_framework::resource::Resource;
use maa_framework::tasker::Tasker;
use moca_protocol::{DeviceTarget, ExecutionMode};

use crate::MaaAdapterError;

/// Lightweight progress event emitted by MaaFramework Tasker during pipeline runs.
#[derive(Debug, Clone)]
pub enum SessionProgressEvent {
    TaskStarting(String),
    TaskSucceeded(String),
    TaskFailed(String),
    NodeStarting(String),
    NodeSucceeded(String),
    NodeFailed(String),
    RecognitionHit(String),
    RecognitionMissed { name: String, reco_id: i64 },
    ActionStarting(String),
}

/// Isolated MaaFramework session bound to one device/window target.
pub struct MaaSession {
    controller: Controller,
    resource: Resource,
    tasker: Tasker,
    win32_hwnd: Option<isize>,
}

impl MaaSession {
    /// Connect to an ADB device or Win32 window and wait until ready.
    pub fn connect(target: &DeviceTarget, mode: ExecutionMode) -> Result<Self, MaaAdapterError> {
        let win32_hwnd = match target {
            DeviceTarget::Win32 { hwnd, .. } => Some(*hwnd),
            DeviceTarget::Adb { .. } => None,
        };
        let controller = match target {
            DeviceTarget::Adb {
                adb_path,
                address,
                config_json,
                ..
            } => {
                let adb_str = adb_path.to_string_lossy();
                let cfg = if config_json.trim().is_empty() {
                    "{}"
                } else {
                    config_json.as_str()
                };
                Controller::new_adb(&adb_str, address, cfg, "")?
            }
            DeviceTarget::Win32 {
                hwnd,
                screencap_method,
                input_method,
                ..
            } => {
                let screencap = if *screencap_method != 0 {
                    *screencap_method
                } else {
                    // Never include GDI here: GDI captures desktop pixels on top of occluded windows.
                    match mode {
                        ExecutionMode::Background => Win32ScreencapMethod::BACKGROUND.bits(),
                        ExecutionMode::ForegroundExclusive => (Win32ScreencapMethod::FRAME_POOL
                            | Win32ScreencapMethod::DXGI_DESKTOP_DUP_WINDOW)
                            .bits(),
                    }
                };
                let (mouse, keyboard) = if *input_method != 0 {
                    (*input_method, *input_method)
                } else {
                    match mode {
                        ExecutionMode::Background => (
                            Win32InputMethod::ANCHORED_TOUCH.bits(),
                            Win32InputMethod::SEND_MESSAGE.bits(),
                        ),
                        ExecutionMode::ForegroundExclusive => (
                            Win32InputMethod::SEIZE.bits(),
                            Win32InputMethod::SEIZE.bits(),
                        ),
                    }
                };
                Controller::new_win32(*hwnd as *mut c_void, screencap, mouse, keyboard)?
            }
        };

        let conn_id = controller.post_connection()?;
        let status = controller.wait(conn_id);
        if !status.succeeded() {
            return Err(MaaAdapterError::ControllerFailed {
                op: "post_connection",
                status: status.to_string(),
            });
        }

        // Normalize all captures to 720p short-side so 16:9 templates work across 1080p/1440p/4K.
        let _ = controller.set_screenshot_target_short_side(720);

        let resource = Resource::new()?;
        let tasker = Tasker::new()?;
        tasker.bind_controller(&controller)?;
        tasker.bind_resource(&resource)?;

        Ok(Self {
            controller,
            resource,
            tasker,
            win32_hwnd,
        })
    }

    pub fn win32_hwnd(&self) -> Option<isize> {
        self.win32_hwnd
    }

    pub fn is_connected(&self) -> bool {
        self.controller.connected()
    }

    pub fn resolution(&self) -> Result<(i32, i32), MaaAdapterError> {
        Ok(self.controller.resolution()?)
    }

    /// Capture one frame from the connected device and convert to RGBA8.
    pub fn capture_rgba(&self) -> Result<RgbaImage, MaaAdapterError> {
        capture_controller_rgba(&self.controller)
    }

    /// Register a progress sink on the underlying Tasker.
    pub fn attach_progress_sink<F>(&self, cb: F) -> Result<(), MaaAdapterError>
    where
        F: Fn(SessionProgressEvent) + Send + Sync + 'static,
    {
        self.tasker.clear_sinks();
        let sink = event_sink::from_closure(move |_handle, event| {
            let mapped = match event {
                MaaEvent::TaskerTaskStarting(d) => {
                    Some(SessionProgressEvent::TaskStarting(d.entry.clone()))
                }
                MaaEvent::TaskerTaskSucceeded(d) => {
                    Some(SessionProgressEvent::TaskSucceeded(d.entry.clone()))
                }
                MaaEvent::TaskerTaskFailed(d) => {
                    Some(SessionProgressEvent::TaskFailed(d.entry.clone()))
                }
                MaaEvent::NodePipelineNodeStarting(d) => {
                    Some(SessionProgressEvent::NodeStarting(d.name.clone()))
                }
                MaaEvent::NodePipelineNodeSucceeded(d) => {
                    Some(SessionProgressEvent::NodeSucceeded(d.name.clone()))
                }
                MaaEvent::NodePipelineNodeFailed(d) => {
                    Some(SessionProgressEvent::NodeFailed(d.name.clone()))
                }
                MaaEvent::NodeRecognitionSucceeded(d) => {
                    Some(SessionProgressEvent::RecognitionHit(d.name.clone()))
                }
                MaaEvent::NodeRecognitionFailed(d) => {
                    Some(SessionProgressEvent::RecognitionMissed {
                        name: d.name.clone(),
                        reco_id: d.reco_id,
                    })
                }
                MaaEvent::NodeActionStarting(d) => {
                    Some(SessionProgressEvent::ActionStarting(d.name.clone()))
                }
                _ => None,
            };
            if let Some(ev) = mapped {
                cb(ev);
            }
        });
        self.tasker.add_event_sink(Box::new(sink))?;
        Ok(())
    }

    /// Load a game resource bundle directory into this session.
    pub fn load_bundle(&self, resource_dir: &Path) -> Result<(), MaaAdapterError> {
        load_bundles(&self.resource, &[resource_dir.to_path_buf()])
    }

    /// Dynamically override pipeline definitions in memory (used by cloud-delivered strategies).
    pub fn override_pipeline_json(
        &self,
        pipeline_override: &serde_json::Value,
    ) -> Result<(), MaaAdapterError> {
        self.resource.override_pipeline_json(pipeline_override)?;
        Ok(())
    }

    pub fn clone_resource(&self) -> Resource {
        self.resource.clone()
    }

    pub fn clone_tasker(&self) -> Tasker {
        self.tasker.clone()
    }

    pub fn clone_ocr_reader(&self) -> OcrReader {
        OcrReader {
            controller: self.controller.clone(),
            tasker: self.tasker.clone(),
        }
    }

    pub fn clone_runner(&self) -> SessionRunner {
        SessionRunner {
            resource: self.resource.clone(),
            tasker: self.tasker.clone(),
            ocr: self.clone_ocr_reader(),
            win32_hwnd: self.win32_hwnd,
        }
    }

    pub fn post_stop(&self) {
        let _ = self.tasker.post_stop();
    }

    /// Capture one live frame and run MaaFramework native PP-OCR within optional 720p ROI.
    pub fn ocr_roi(&self, roi_720p: Option<[u32; 4]>) -> Result<String, MaaAdapterError> {
        self.clone_ocr_reader().recognize_roi(roi_720p)
    }

    pub fn resource(&self) -> &Resource {
        &self.resource
    }

    pub fn tasker(&self) -> &Tasker {
        &self.tasker
    }
}

/// Lightweight handle for capturing live frames and running MaaFramework native PP-OCR.
#[derive(Clone)]
pub struct OcrReader {
    controller: Controller,
    tasker: Tasker,
}

impl OcrReader {
    /// Capture one frame from the bound controller and extract text inside `roi_720p`.
    pub fn recognize_roi(&self, roi_720p: Option<[u32; 4]>) -> Result<String, MaaAdapterError> {
        let cap_id = self.controller.post_screencap()?;
        let status = self.controller.wait(cap_id);
        if !status.succeeded() {
            return Err(MaaAdapterError::ControllerFailed {
                op: "post_screencap",
                status: status.to_string(),
            });
        }
        let img_buf = self.controller.cached_image()?;
        ocr_image_buffer_roi(&self.tasker, &img_buf, roi_720p)
    }
}

/// Cloneable execution handle isolating `moca-runtime` from raw `maa-framework` types.
#[derive(Clone)]
pub struct SessionRunner {
    resource: Resource,
    tasker: Tasker,
    ocr: OcrReader,
    win32_hwnd: Option<isize>,
}

impl SessionRunner {
    pub fn win32_hwnd(&self) -> Option<isize> {
        self.win32_hwnd
    }

    pub fn ocr_reader(&self) -> OcrReader {
        self.ocr.clone()
    }

    pub fn load_bundles(&self, bundle_dirs: &[PathBuf]) -> Result<(), MaaAdapterError> {
        load_bundles(&self.resource, bundle_dirs)
    }

    pub fn run_task_entry(
        &self,
        entry: &str,
        pipeline_override: Option<&serde_json::Value>,
    ) -> Result<bool, MaaAdapterError> {
        run_task_entry(&self.tasker, entry, pipeline_override)
    }

    pub fn stop(&self) {
        let _ = self.tasker.post_stop();
    }

    pub fn is_stopping(&self) -> bool {
        self.tasker.stopping()
    }

    pub fn stop_and_wait(&self) {
        if let Ok(stop_id) = self.tasker.post_stop()
            && stop_id != 0
        {
            let _ = Job::for_tasker(&self.tasker, stop_id).wait();
        }
        for _ in 0..100 {
            if !self.tasker.stopping() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(15));
        }
    }
}

/// Run MaaFramework's built-in PP-OCR recognizer directly on an image buffer.
pub fn ocr_image_buffer_roi(
    tasker: &Tasker,
    img_buf: &MaaImageBuffer,
    roi_720p: Option<[u32; 4]>,
) -> Result<String, MaaAdapterError> {
    if !tasker.inited() {
        return Err(MaaAdapterError::ResourceFailed {
            op: "tasker.inited",
            status: "not initialized (missing resource or controller)".into(),
        });
    }
    let param = match roi_720p {
        Some(roi) => serde_json::json!({ "roi": roi }),
        None => serde_json::json!({}),
    };
    let job = tasker.post_recognition("OCR", &param.to_string(), img_buf)?;
    let _ = job.wait();
    // maa-framework 1.29.1's `post_recognition().get()` forwards the task_id to
    // `get_recognition_detail`; read the hydrated `NodeDetail.recognition` via `get_task_detail` instead.
    let Some(task_detail) = tasker.get_task_detail(job.id)? else {
        return Ok(String::new());
    };
    let Some(detail) = task_detail
        .nodes
        .into_iter()
        .flatten()
        .find_map(|node| node.recognition)
    else {
        return Ok(String::new());
    };
    Ok(extract_ocr_detail_text(&detail.detail))
}

fn extract_ocr_detail_text(detail: &serde_json::Value) -> String {
    if let Some(items) = detail.get("all").and_then(|v| v.as_array()) {
        let mut entries: Vec<OCRResult> = items
            .iter()
            .filter_map(|item| serde_json::from_value::<OCRResult>(item.clone()).ok())
            .filter(|entry| !entry.text.trim().is_empty())
            .collect();
        entries.sort_by_key(|entry| (entry.base.box_rect.1 / 14, entry.base.box_rect.0));
        let joined = entries
            .into_iter()
            .map(|entry| entry.text.trim().to_string())
            .collect::<Vec<_>>()
            .join("\n");
        if !joined.is_empty() {
            return joined;
        }
    }
    detail
        .get("best")
        .and_then(|best| best.get("text"))
        .and_then(|t| t.as_str())
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

fn capture_controller_rgba(controller: &Controller) -> Result<RgbaImage, MaaAdapterError> {
    let cap_id = controller.post_screencap()?;
    let status = controller.wait(cap_id);
    if !status.succeeded() {
        return Err(MaaAdapterError::ControllerFailed {
            op: "post_screencap",
            status: status.to_string(),
        });
    }
    let img_buf = controller.cached_image()?;
    let dyn_img = img_buf.to_dynamic_image()?;
    Ok(dyn_img.to_rgba8())
}

/// Reload one or more resource bundle directories onto a `Resource` handle.
pub fn load_bundles(resource: &Resource, bundle_dirs: &[PathBuf]) -> Result<(), MaaAdapterError> {
    resource.clear()?;
    for dir in bundle_dirs {
        let dir_str = dir.to_string_lossy();
        let job = resource.post_bundle(&dir_str)?;
        let status = job.wait();
        if !status.succeeded() {
            return Err(MaaAdapterError::ResourceFailed {
                op: "post_bundle",
                status: status.to_string(),
            });
        }
    }
    Ok(())
}

/// Run a single pipeline entry node synchronously on a bound `Tasker`, returning whether it succeeded.
pub fn run_task_entry(
    tasker: &Tasker,
    entry: &str,
    pipeline_override: Option<&serde_json::Value>,
) -> Result<bool, MaaAdapterError> {
    if !tasker.inited() {
        return Err(MaaAdapterError::ResourceFailed {
            op: "tasker.inited",
            status: "not initialized (missing resource or controller)".into(),
        });
    }
    for _ in 0..100 {
        if !tasker.stopping() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(15));
    }
    let override_str = pipeline_override
        .map(|v| v.to_string())
        .unwrap_or_else(|| "{}".to_string());
    let job = tasker.post_task(entry, &override_str)?;
    if job.id == 0 {
        return Ok(false);
    }
    let status = job.wait();
    Ok(status.succeeded())
}
