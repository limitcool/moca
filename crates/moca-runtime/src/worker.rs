use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};

use image::RgbaImage;
use moca_cloud::{CloudClient, StubCloudClient};
use moca_maa::{
    DiscoveredAdb, DiscoveredWin32, MaaSession, SessionProgressEvent, discover_adb_devices,
    discover_win32_windows, init_maa, load_bundles, run_task_entry,
};
use moca_protocol::{DeviceTarget, ExecutionMode, TaskDecl};
use tokio::sync::{Mutex, mpsc as tokio_mpsc};

use crate::{LoadedGamePackage, scan_game_packages};

#[derive(Debug, Clone)]
pub enum RuntimeCommand {
    Initialize,
    ReloadGamePackages,
    ScanDevices,
    Connect {
        instance_id: String,
        target: DeviceTarget,
        mode: ExecutionMode,
    },
    Disconnect {
        instance_id: String,
    },
    CaptureFrame {
        instance_id: String,
    },
    RunTasks {
        instance_id: String,
        game_name: String,
        bundle_dirs: Vec<PathBuf>,
        tasks: Vec<TaskDecl>,
        mode: ExecutionMode,
    },
    StopTasks {
        instance_id: String,
    },
}

#[derive(Debug, Clone)]
pub enum RuntimeEvent {
    MaaReady {
        version: String,
    },
    MaaInitFailed {
        error: String,
    },
    GamePackagesLoaded(Vec<LoadedGamePackage>),
    DevicesDiscovered {
        adb: Vec<DiscoveredAdb>,
        win32: Vec<DiscoveredWin32>,
    },
    Connected {
        instance_id: String,
        resolution: Option<(i32, i32)>,
    },
    Disconnected {
        instance_id: String,
    },
    FrameCaptured {
        instance_id: String,
        image: RgbaImage,
    },
    TaskBatchStarted {
        instance_id: String,
    },
    TaskBatchFinished {
        instance_id: String,
        succeeded: bool,
    },
    Log {
        level: LogLevel,
        message: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Info,
    Warn,
    Error,
}

type RepaintCb = Option<Arc<dyn Fn() + Send + Sync>>;

/// Background worker owning the Tokio runtime, MaaFramework sessions, and foreground lock.
pub struct RuntimeHandle {
    cmd_tx: tokio_mpsc::UnboundedSender<RuntimeCommand>,
    event_rx: mpsc::Receiver<RuntimeEvent>,
    cloud: Arc<StubCloudClient>,
}

impl RuntimeHandle {
    pub fn spawn(repaint_cb: Option<Box<dyn Fn() + Send + Sync>>) -> Self {
        let (cmd_tx, mut cmd_rx) = tokio_mpsc::unbounded_channel::<RuntimeCommand>();
        let (event_tx, event_rx) = mpsc::channel::<RuntimeEvent>();
        let cloud = Arc::new(StubCloudClient::new());
        let cloud_worker = Arc::clone(&cloud);
        let repaint: RepaintCb = repaint_cb.map(Arc::from);

        std::thread::Builder::new()
            .name("moca-runtime".into())
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .expect("failed to build tokio runtime");

                rt.block_on(async move {
                    let state = Arc::new(WorkerState {
                        sessions: Mutex::new(HashMap::new()),
                        cancel_flags: Mutex::new(HashMap::new()),
                        foreground_lock: Mutex::new(()),
                        cloud: cloud_worker,
                    });

                    while let Some(cmd) = cmd_rx.recv().await {
                        let st = Arc::clone(&state);
                        let tx = event_tx.clone();
                        let rp = repaint.clone();
                        match cmd {
                            RuntimeCommand::Initialize => {
                                let res = tokio::task::spawn_blocking(|| init_maa(None)).await;
                                match res {
                                    Ok(Ok(ver)) => {
                                        emit(
                                            &tx,
                                            rp.as_deref(),
                                            RuntimeEvent::MaaReady {
                                                version: ver.to_string(),
                                            },
                                        );
                                        emit_log(
                                            &tx,
                                            rp.as_deref(),
                                            LogLevel::Info,
                                            format!("MaaFramework 加载成功 (版本: {ver})"),
                                        );
                                    }
                                    Ok(Err(e)) => {
                                        let msg = e.to_string();
                                        emit(
                                            &tx,
                                            rp.as_deref(),
                                            RuntimeEvent::MaaInitFailed { error: msg.clone() },
                                        );
                                        emit_log(
                                            &tx,
                                            rp.as_deref(),
                                            LogLevel::Error,
                                            format!("MaaFramework 初始化失败: {msg}"),
                                        );
                                    }
                                    Err(e) => emit_log(
                                        &tx,
                                        rp.as_deref(),
                                        LogLevel::Error,
                                        format!("初始化线程异常: {e}"),
                                    ),
                                }
                            }
                            RuntimeCommand::ReloadGamePackages => {
                                match scan_game_packages("games") {
                                    Ok(pkgs) => {
                                        let count = pkgs.len();
                                        emit(
                                            &tx,
                                            rp.as_deref(),
                                            RuntimeEvent::GamePackagesLoaded(pkgs),
                                        );
                                        emit_log(
                                            &tx,
                                            rp.as_deref(),
                                            LogLevel::Info,
                                            format!("已加载 {count} 个游戏包"),
                                        );
                                    }
                                    Err(e) => emit_log(
                                        &tx,
                                        rp.as_deref(),
                                        LogLevel::Error,
                                        format!("扫描游戏包失败: {e}"),
                                    ),
                                }
                            }
                            RuntimeCommand::ScanDevices => {
                                let res = tokio::task::spawn_blocking(|| {
                                    let adb = discover_adb_devices()?;
                                    let win32 = discover_win32_windows()?;
                                    Ok::<_, moca_maa::MaaAdapterError>((adb, win32))
                                })
                                .await;

                                match res {
                                    Ok(Ok((adb, win32))) => {
                                        emit_log(
                                            &tx,
                                            rp.as_deref(),
                                            LogLevel::Info,
                                            format!(
                                                "扫描完成：发现 {} 个 ADB 设备，{} 个 Win32 窗口",
                                                adb.len(),
                                                win32.len()
                                            ),
                                        );
                                        emit(
                                            &tx,
                                            rp.as_deref(),
                                            RuntimeEvent::DevicesDiscovered { adb, win32 },
                                        );
                                    }
                                    Ok(Err(e)) => emit_log(
                                        &tx,
                                        rp.as_deref(),
                                        LogLevel::Error,
                                        format!("设备扫描失败: {e}"),
                                    ),
                                    Err(e) => emit_log(
                                        &tx,
                                        rp.as_deref(),
                                        LogLevel::Error,
                                        format!("扫描任务异常: {e}"),
                                    ),
                                }
                            }
                            RuntimeCommand::Connect {
                                instance_id,
                                target,
                                mode,
                            } => {
                                let _fg_guard = if mode == ExecutionMode::ForegroundExclusive {
                                    Some(st.foreground_lock.lock().await)
                                } else {
                                    None
                                };
                                let res = tokio::task::spawn_blocking(move || {
                                    let session = MaaSession::connect(&target, mode)?;
                                    let resolution = session.resolution().ok();
                                    Ok::<_, moca_maa::MaaAdapterError>((session, resolution))
                                })
                                .await;

                                match res {
                                    Ok(Ok((session, resolution))) => {
                                        let res_text = resolution
                                            .map(|(w, h)| format!("{w}x{h}"))
                                            .unwrap_or_else(|| "未知".into());
                                        st.sessions.lock().await.insert(instance_id.clone(), session);
                                        emit_log(
                                            &tx,
                                            rp.as_deref(),
                                            LogLevel::Info,
                                            format!(
                                                "实例 [{instance_id}] 已连接 (分辨率: {res_text})"
                                            ),
                                        );
                                        emit(
                                            &tx,
                                            rp.as_deref(),
                                            RuntimeEvent::Connected {
                                                instance_id,
                                                resolution,
                                            },
                                        );
                                    }
                                    Ok(Err(e)) => emit_log(
                                        &tx,
                                        rp.as_deref(),
                                        LogLevel::Error,
                                        format!("连接失败 [{instance_id}]: {e}"),
                                    ),
                                    Err(e) => emit_log(
                                        &tx,
                                        rp.as_deref(),
                                        LogLevel::Error,
                                        format!("连接线程异常: {e}"),
                                    ),
                                }
                            }
                            RuntimeCommand::Disconnect { instance_id } => {
                                st.sessions.lock().await.remove(&instance_id);
                                emit_log(
                                    &tx,
                                    rp.as_deref(),
                                    LogLevel::Info,
                                    format!("实例 [{instance_id}] 已断开"),
                                );
                                emit(
                                    &tx,
                                    rp.as_deref(),
                                    RuntimeEvent::Disconnected { instance_id },
                                );
                            }
                            RuntimeCommand::CaptureFrame { instance_id } => {
                                let guard = st.sessions.lock().await;
                                if let Some(session) = guard.get(&instance_id) {
                                    match tokio::task::block_in_place(|| session.capture_rgba()) {
                                        Ok(img) => {
                                            let w = img.width();
                                            let h = img.height();
                                            emit(
                                                &tx,
                                                rp.as_deref(),
                                                RuntimeEvent::FrameCaptured {
                                                    instance_id: instance_id.clone(),
                                                    image: img,
                                                },
                                            );
                                            emit_log(
                                                &tx,
                                                rp.as_deref(),
                                                LogLevel::Info,
                                                format!("截图成功 [{instance_id}] ({w}x{h})"),
                                            );
                                        }
                                        Err(e) => emit_log(
                                            &tx,
                                            rp.as_deref(),
                                            LogLevel::Error,
                                            format!("截图失败 [{instance_id}]: {e}"),
                                        ),
                                    }
                                } else {
                                    emit_log(
                                        &tx,
                                        rp.as_deref(),
                                        LogLevel::Warn,
                                        format!("实例 [{instance_id}] 未连接，无法截图"),
                                    );
                                }
                            }
                            RuntimeCommand::RunTasks {
                                instance_id,
                                game_name,
                                bundle_dirs,
                                tasks,
                                mode,
                            } => {
                                tokio::spawn(execute_task_batch(
                                    st,
                                    tx,
                                    rp,
                                    instance_id,
                                    game_name,
                                    bundle_dirs,
                                    tasks,
                                    mode,
                                ));
                            }
                            RuntimeCommand::StopTasks { instance_id } => {
                                if let Some(flag) = st.cancel_flags.lock().await.get(&instance_id) {
                                    flag.store(true, Ordering::Relaxed);
                                }
                                let tasker = st
                                    .sessions
                                    .lock()
                                    .await
                                    .get(&instance_id)
                                    .map(|s| s.clone_tasker());
                                if let Some(tasker) = tasker {
                                    let _ = tasker.post_stop();
                                    emit_log(
                                        &tx,
                                        rp.as_deref(),
                                        LogLevel::Warn,
                                        format!("已发送停止信号给实例 [{instance_id}]"),
                                    );
                                }
                            }
                        }
                    }
                });
            })
            .expect("failed to spawn moca-runtime thread");

        Self {
            cmd_tx,
            event_rx,
            cloud,
        }
    }

    pub fn send(&self, cmd: RuntimeCommand) {
        let _ = self.cmd_tx.send(cmd);
    }

    pub fn try_recv_event(&self) -> Option<RuntimeEvent> {
        self.event_rx.try_recv().ok()
    }

    pub fn cloud(&self) -> &StubCloudClient {
        &self.cloud
    }
}

struct WorkerState {
    sessions: Mutex<HashMap<String, MaaSession>>,
    cancel_flags: Mutex<HashMap<String, Arc<AtomicBool>>>,
    /// Ensures ForegroundExclusive Win32 instances do not fight over window focus/mouse.
    foreground_lock: Mutex<()>,
    cloud: Arc<StubCloudClient>,
}

#[allow(clippy::too_many_arguments)]
async fn execute_task_batch(
    st: Arc<WorkerState>,
    tx: mpsc::Sender<RuntimeEvent>,
    rp: RepaintCb,
    instance_id: String,
    game_name: String,
    bundle_dirs: Vec<PathBuf>,
    tasks: Vec<TaskDecl>,
    mode: ExecutionMode,
) {
    if tasks.is_empty() {
        emit_log(&tx, rp.as_deref(), LogLevel::Warn, "未选择任何待执行任务".into());
        return;
    }

    let handles = {
        let guard = st.sessions.lock().await;
        guard.get(&instance_id).map(|session| {
            let tx_sink = tx.clone();
            let rp_sink = rp.clone();
            let id_sink = instance_id.clone();
            let _ = session.attach_progress_sink(move |ev| {
                let msg = match ev {
                    SessionProgressEvent::TaskStarting(entry) => {
                        format!("[{id_sink}] 开始节点入口: {entry}")
                    }
                    SessionProgressEvent::NodeStarting(name) => {
                        format!("[{id_sink}] -> 执行节点: {name}")
                    }
                    SessionProgressEvent::RecognitionHit(name) => {
                        format!("[{id_sink}] ✓ 识别命中: {name}")
                    }
                    SessionProgressEvent::ActionStarting(name) => {
                        format!("[{id_sink}] ⚡ 执行动作: {name}")
                    }
                    SessionProgressEvent::TaskSucceeded(entry) => {
                        format!("[{id_sink}] ✓ 流程完成: {entry}")
                    }
                    SessionProgressEvent::TaskFailed(entry) => {
                        format!("[{id_sink}] ✗ 流程失败/中止: {entry}")
                    }
                    _ => return,
                };
                emit_log(&tx_sink, rp_sink.as_deref(), LogLevel::Info, msg);
            });
            (session.clone_resource(), session.clone_tasker())
        })
    };

    let Some((resource, tasker)) = handles else {
        emit_log(
            &tx,
            rp.as_deref(),
            LogLevel::Warn,
            format!("请先连接目标设备/窗口后再运行「{game_name}」任务"),
        );
        return;
    };

    let cancel_flag = Arc::new(AtomicBool::new(false));
    st.cancel_flags
        .lock()
        .await
        .insert(instance_id.clone(), Arc::clone(&cancel_flag));

    emit(
        &tx,
        rp.as_deref(),
        RuntimeEvent::TaskBatchStarted {
            instance_id: instance_id.clone(),
        },
    );

    let _fg_guard = if mode == ExecutionMode::ForegroundExclusive {
        Some(st.foreground_lock.lock().await)
    } else {
        None
    };

    emit_log(
        &tx,
        rp.as_deref(),
        LogLevel::Info,
        format!("正在加载「{game_name}」资源包 ({} 个目录)...", bundle_dirs.len()),
    );

    let load_res = tokio::task::spawn_blocking({
        let res = resource.clone();
        let dirs = bundle_dirs.clone();
        move || load_bundles(&res, &dirs)
    })
    .await;

    match load_res {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            emit_log(
                &tx,
                rp.as_deref(),
                LogLevel::Error,
                format!("加载游戏资源失败: {e}"),
            );
            st.cancel_flags.lock().await.remove(&instance_id);
            emit(
                &tx,
                rp.as_deref(),
                RuntimeEvent::TaskBatchFinished {
                    instance_id,
                    succeeded: false,
                },
            );
            return;
        }
        Err(e) => {
            emit_log(
                &tx,
                rp.as_deref(),
                LogLevel::Error,
                format!("加载线程异常: {e}"),
            );
            st.cancel_flags.lock().await.remove(&instance_id);
            emit(
                &tx,
                rp.as_deref(),
                RuntimeEvent::TaskBatchFinished {
                    instance_id,
                    succeeded: false,
                },
            );
            return;
        }
    }

    let mut all_ok = true;
    for task in tasks {
        if cancel_flag.load(Ordering::Relaxed) {
            emit_log(&tx, rp.as_deref(), LogLevel::Warn, "任务队列已被用户中止".into());
            all_ok = false;
            break;
        }

        if let Err(e) = st.cloud.verify_task_access(&task) {
            emit_log(
                &tx,
                rp.as_deref(),
                LogLevel::Warn,
                format!("跳过 VIP 任务「{}」: {e}", task.label),
            );
            all_ok = false;
            continue;
        }

        emit_log(
            &tx,
            rp.as_deref(),
            LogLevel::Info,
            format!("执行任务「{}」(入口: {})", task.label, task.entry),
        );

        let entry = task.entry.clone();
        let tasker_clone = tasker.clone();
        let run_res =
            tokio::task::spawn_blocking(move || run_task_entry(&tasker_clone, &entry, None)).await;

        match run_res {
            Ok(Ok(true)) => {}
            Ok(Ok(false)) => {
                all_ok = false;
                emit_log(
                    &tx,
                    rp.as_deref(),
                    LogLevel::Warn,
                    format!("任务「{}」未成功结束", task.label),
                );
            }
            Ok(Err(e)) => {
                all_ok = false;
                emit_log(
                    &tx,
                    rp.as_deref(),
                    LogLevel::Error,
                    format!("任务「{}」执行出错: {e}", task.label),
                );
            }
            Err(e) => {
                all_ok = false;
                emit_log(
                    &tx,
                    rp.as_deref(),
                    LogLevel::Error,
                    format!("任务线程异常: {e}"),
                );
            }
        }
    }

    st.cancel_flags.lock().await.remove(&instance_id);
    emit(
        &tx,
        rp.as_deref(),
        RuntimeEvent::TaskBatchFinished {
            instance_id,
            succeeded: all_ok,
        },
    );
}

fn emit(
    tx: &mpsc::Sender<RuntimeEvent>,
    repaint: Option<&(dyn Fn() + Send + Sync)>,
    ev: RuntimeEvent,
) {
    let _ = tx.send(ev);
    if let Some(cb) = repaint {
        cb();
    }
}

fn emit_log(
    tx: &mpsc::Sender<RuntimeEvent>,
    repaint: Option<&(dyn Fn() + Send + Sync)>,
    level: LogLevel,
    message: String,
) {
    emit(tx, repaint, RuntimeEvent::Log { level, message });
}
