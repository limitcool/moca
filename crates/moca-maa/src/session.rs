use std::ffi::c_void;
use std::path::{Path, PathBuf};

use image::RgbaImage;
use maa_framework::common::{Win32InputMethod, Win32ScreencapMethod};
use maa_framework::controller::Controller;
use maa_framework::event_sink;
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
    ActionStarting(String),
}

/// Isolated MaaFramework session bound to one device/window target.
pub struct MaaSession {
    controller: Controller,
    resource: Resource,
    tasker: Tasker,
}

impl MaaSession {
    /// Connect to an ADB device or Win32 window and wait until ready.
    pub fn connect(target: &DeviceTarget, mode: ExecutionMode) -> Result<Self, MaaAdapterError> {
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
                    match mode {
                        ExecutionMode::Background => {
                            (Win32ScreencapMethod::BACKGROUND | Win32ScreencapMethod::GDI).bits()
                        }
                        ExecutionMode::ForegroundExclusive => Win32ScreencapMethod::ALL.bits(),
                    }
                };
                let input = if *input_method != 0 {
                    *input_method
                } else {
                    match mode {
                        ExecutionMode::Background => Win32InputMethod::SEND_MESSAGE.bits(),
                        ExecutionMode::ForegroundExclusive => Win32InputMethod::SEIZE.bits(),
                    }
                };
                Controller::new_win32(*hwnd as *mut c_void, screencap, input, input)?
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

        let resource = Resource::new()?;
        let tasker = Tasker::new()?;
        tasker.bind_controller(&controller)?;
        tasker.bind_resource(&resource)?;

        Ok(Self {
            controller,
            resource,
            tasker,
        })
    }

    pub fn is_connected(&self) -> bool {
        self.controller.connected()
    }

    pub fn resolution(&self) -> Result<(i32, i32), MaaAdapterError> {
        Ok(self.controller.resolution()?)
    }

    /// Capture one frame from the connected device and convert to RGBA8.
    pub fn capture_rgba(&self) -> Result<RgbaImage, MaaAdapterError> {
        let cap_id = self.controller.post_screencap()?;
        let status = self.controller.wait(cap_id);
        if !status.succeeded() {
            return Err(MaaAdapterError::ControllerFailed {
                op: "post_screencap",
                status: status.to_string(),
            });
        }

        let img_buf = self.controller.cached_image()?;
        let dyn_img = img_buf.to_dynamic_image()?;
        Ok(dyn_img.to_rgba8())
    }

    /// Register a progress sink on the underlying Tasker.
    pub fn attach_progress_sink<F>(&self, cb: F) -> Result<(), MaaAdapterError>
    where
        F: Fn(SessionProgressEvent) + Send + Sync + 'static,
    {
        self.tasker.clear_sinks();
        let sink = event_sink::from_closure(move |_handle, event| {
            let mapped = match event {
                MaaEvent::TaskerTaskStarting(d) => Some(SessionProgressEvent::TaskStarting(d.entry.clone())),
                MaaEvent::TaskerTaskSucceeded(d) => Some(SessionProgressEvent::TaskSucceeded(d.entry.clone())),
                MaaEvent::TaskerTaskFailed(d) => Some(SessionProgressEvent::TaskFailed(d.entry.clone())),
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

    pub fn resource(&self) -> &Resource {
        &self.resource
    }

    pub fn tasker(&self) -> &Tasker {
        &self.tasker
    }
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
    let override_str = pipeline_override
        .map(|v| v.to_string())
        .unwrap_or_else(|| "{}".to_string());
    let job = tasker.post_task(entry, &override_str)?;
    let status = job.wait();
    Ok(status.succeeded())
}
