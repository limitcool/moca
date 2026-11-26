use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, mpsc};

use image::RgbaImage;
use moca_cloud::{CloudClient, StubCloudClient};
use moca_maa::{
    DiscoveredAdb, DiscoveredWin32, MaaSession, SessionProgressEvent, discover_adb_devices,
    discover_win32_windows, init_maa, load_bundles, run_task_entry,
};
use moca_protocol::{DeviceTarget, ExecutionMode, ResolvedTask};
use tokio::sync::{Mutex, mpsc as tokio_mpsc};

use crate::{LoadedGamePackage, resolve_workspace_subdir, scan_game_packages};

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
        tasks: Vec<ResolvedTask>,
        mode: ExecutionMode,
    },
    SetTriggerActive {
        instance_id: String,
        task_id: String,
        active: bool,
        game_name: String,
        bundle_dirs: Vec<PathBuf>,
        task: Box<ResolvedTask>,
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
    TriggerStateChanged {
        instance_id: String,
        task_id: String,
        active: bool,
    },
    TriggerActionCount {
        instance_id: String,
        task_id: String,
        count: u32,
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
                                let games_dir = resolve_workspace_subdir("games");
                                match scan_game_packages(&games_dir) {
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
                                                "实例 [{instance_id}] 已连接 (分辨率: {res_text}, 模式: {mode:?})"
                                            ),
                                        );
                                        if mode == ExecutionMode::Background {
                                            emit_log(
                                                &tx,
                                                rp.as_deref(),
                                                LogLevel::Info,
                                                "提示：Win32 后台并行采用 AnchoredTouch，请保持游戏窗口未最小化且目标按钮区域不被置顶窗口或任务栏遮挡；如遇点击报错可切换「前台独占」".into(),
                                            );
                                        }
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
                                if let Some(flag) = st.cancel_flags.lock().await.get(&instance_id) {
                                    flag.store(true, Ordering::Relaxed);
                                }
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
                                    None,
                                ));
                            }
                            RuntimeCommand::SetTriggerActive {
                                instance_id,
                                task_id,
                                active,
                                game_name,
                                bundle_dirs,
                                task,
                                mode,
                            } => {
                                if active {
                                    // Stop any existing running trigger before applying updated options cleanly.
                                    if st.cancel_flags.lock().await.contains_key(&instance_id) {
                                        stop_instance(&st, &tx, rp.as_deref(), &instance_id).await;
                                        for _ in 0..30 {
                                            if !st.cancel_flags.lock().await.contains_key(&instance_id) {
                                                break;
                                            }
                                            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                                        }
                                    }
                                    tokio::spawn(execute_task_batch(
                                        st,
                                        tx,
                                        rp,
                                        instance_id,
                                        game_name,
                                        bundle_dirs,
                                        vec![*task],
                                        mode,
                                        Some(task_id),
                                    ));
                                } else {
                                    stop_instance(&st, &tx, rp.as_deref(), &instance_id).await;
                                    emit(
                                        &tx,
                                        rp.as_deref(),
                                        RuntimeEvent::TriggerStateChanged {
                                            instance_id,
                                            task_id,
                                            active: false,
                                        },
                                    );
                                }
                            }
                            RuntimeCommand::StopTasks { instance_id } => {
                                stop_instance(&st, &tx, rp.as_deref(), &instance_id).await;
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

async fn stop_instance(
    st: &Arc<WorkerState>,
    tx: &mpsc::Sender<RuntimeEvent>,
    rp: Option<&(dyn Fn() + Send + Sync)>,
    instance_id: &str,
) {
    if let Some(flag) = st.cancel_flags.lock().await.get(instance_id) {
        flag.store(true, Ordering::Relaxed);
    }
    let tasker = st
        .sessions
        .lock()
        .await
        .get(instance_id)
        .map(|s| s.clone_tasker());
    if let Some(tasker) = tasker {
        let _ = tasker.post_stop();
        emit_log(
            tx,
            rp,
            LogLevel::Warn,
            format!("已停止实例 [{instance_id}] 当前任务"),
        );
    }
}

#[allow(clippy::too_many_arguments)]
async fn execute_task_batch(
    st: Arc<WorkerState>,
    tx: mpsc::Sender<RuntimeEvent>,
    rp: RepaintCb,
    instance_id: String,
    game_name: String,
    bundle_dirs: Vec<PathBuf>,
    tasks: Vec<ResolvedTask>,
    mode: ExecutionMode,
    trigger_task_id: Option<String>,
) {
    if tasks.is_empty() {
        emit_log(
            &tx,
            rp.as_deref(),
            LogLevel::Warn,
            "未选择任何待执行任务".into(),
        );
        return;
    }

    let action_counter = Arc::new(AtomicU32::new(0));
    let miss_counter = Arc::new(AtomicU32::new(0));

    let handles = {
        let guard = st.sessions.lock().await;
        guard.get(&instance_id).map(|session| {
            let tasker_ref = session.clone_tasker();
            let tx_sink = tx.clone();
            let rp_sink = rp.clone();
            let id_sink = instance_id.clone();
            let trig_id = trigger_task_id.clone();
            let act_cnt = Arc::clone(&action_counter);
            let miss_cnt = Arc::clone(&miss_counter);

            let _ = session.attach_progress_sink(move |ev| match ev {
                SessionProgressEvent::TaskStarting(entry) => {
                    emit_log(
                        &tx_sink,
                        rp_sink.as_deref(),
                        LogLevel::Info,
                        format!("[{id_sink}] 开始监听/执行: {entry}"),
                    );
                }
                SessionProgressEvent::RecognitionHit(name) => {
                    miss_cnt.store(0, Ordering::Relaxed);
                    emit_log(
                        &tx_sink,
                        rp_sink.as_deref(),
                        LogLevel::Info,
                        format!("[{id_sink}] ✓ 识别命中: {name}"),
                    );
                }
                SessionProgressEvent::RecognitionMissed { name, reco_id } => {
                    let n = miss_cnt.fetch_add(1, Ordering::Relaxed) + 1;
                    // Throttle diagnostic output: log first miss and every 20 misses (~30s) thereafter.
                    if n == 1 || n.is_multiple_of(20) {
                        let score_hint = tasker_ref
                            .get_recognition_detail(reco_id)
                            .ok()
                            .flatten()
                            .and_then(|d| {
                                d.detail
                                    .get("best")
                                    .and_then(|b| b.get("score"))
                                    .and_then(|s| s.as_f64())
                                    .or_else(|| d.detail.get("score").and_then(|s| s.as_f64()))
                            })
                            .map(|s| format!(" (最佳相似度: {s:.3})"))
                            .unwrap_or_default();
                        emit_log(
                            &tx_sink,
                            rp_sink.as_deref(),
                            LogLevel::Info,
                            format!("[{id_sink}] 扫描第 {n} 次未命中「{name}」{score_hint}"),
                        );
                    }
                }
                SessionProgressEvent::ActionStarting(name) => {
                    let count = act_cnt.fetch_add(1, Ordering::Relaxed) + 1;
                    emit_log(
                        &tx_sink,
                        rp_sink.as_deref(),
                        LogLevel::Info,
                        format!("[{id_sink}] ⚡ 执行动作: {name}"),
                    );
                    if let Some(tid) = &trig_id {
                        emit(
                            &tx_sink,
                            rp_sink.as_deref(),
                            RuntimeEvent::TriggerActionCount {
                                instance_id: id_sink.clone(),
                                task_id: tid.clone(),
                                count,
                            },
                        );
                    }
                }
                SessionProgressEvent::TaskSucceeded(entry) => {
                    emit_log(
                        &tx_sink,
                        rp_sink.as_deref(),
                        LogLevel::Info,
                        format!("[{id_sink}] ✓ 流程完成: {entry}"),
                    );
                }
                SessionProgressEvent::TaskFailed(entry) => {
                    emit_log(
                        &tx_sink,
                        rp_sink.as_deref(),
                        LogLevel::Warn,
                        format!("[{id_sink}] 流程停止/未匹配: {entry}"),
                    );
                }
                _ => {}
            });
            (session.clone_resource(), session.clone_tasker())
        })
    };

    let Some((resource, tasker)) = handles else {
        emit_log(
            &tx,
            rp.as_deref(),
            LogLevel::Warn,
            format!("请先连接目标设备/窗口后再开启「{game_name}」任务"),
        );
        if let Some(tid) = trigger_task_id {
            emit(
                &tx,
                rp.as_deref(),
                RuntimeEvent::TriggerStateChanged {
                    instance_id,
                    task_id: tid,
                    active: false,
                },
            );
        }
        return;
    };

    let cancel_flag = Arc::new(AtomicBool::new(false));
    st.cancel_flags
        .lock()
        .await
        .insert(instance_id.clone(), Arc::clone(&cancel_flag));

    if let Some(tid) = &trigger_task_id {
        emit(
            &tx,
            rp.as_deref(),
            RuntimeEvent::TriggerStateChanged {
                instance_id: instance_id.clone(),
                task_id: tid.clone(),
                active: true,
            },
        );
    } else {
        emit(
            &tx,
            rp.as_deref(),
            RuntimeEvent::TaskBatchStarted {
                instance_id: instance_id.clone(),
            },
        );
    }

    let _fg_guard = if mode == ExecutionMode::ForegroundExclusive {
        Some(st.foreground_lock.lock().await)
    } else {
        None
    };

    let load_res = tokio::task::spawn_blocking({
        let res = resource.clone();
        let dirs = bundle_dirs.clone();
        move || load_bundles(&res, &dirs)
    })
    .await;

    if !matches!(load_res, Ok(Ok(()))) {
        emit_log(
            &tx,
            rp.as_deref(),
            LogLevel::Error,
            format!("加载「{game_name}」资源失败"),
        );
        st.cancel_flags.lock().await.remove(&instance_id);
        if let Some(tid) = trigger_task_id {
            emit(
                &tx,
                rp.as_deref(),
                RuntimeEvent::TriggerStateChanged {
                    instance_id,
                    task_id: tid,
                    active: false,
                },
            );
        } else {
            emit(
                &tx,
                rp.as_deref(),
                RuntimeEvent::TaskBatchFinished {
                    instance_id,
                    succeeded: false,
                },
            );
        }
        return;
    }

    let mut all_ok = true;
    for resolved in tasks {
        let task = resolved.decl;
        let pipeline_override = resolved.pipeline_override;
        if cancel_flag.load(Ordering::Relaxed) {
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

        let entry = task.entry.clone();
        let tasker_clone = tasker.clone();
        let run_res = tokio::task::spawn_blocking(move || {
            run_task_entry(&tasker_clone, &entry, pipeline_override.as_ref())
        })
        .await;

        match run_res {
            Ok(Ok(true)) => {}
            Ok(Ok(false)) => {
                all_ok = false;
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
    if let Some(tid) = trigger_task_id {
        emit(
            &tx,
            rp.as_deref(),
            RuntimeEvent::TriggerStateChanged {
                instance_id,
                task_id: tid,
                active: false,
            },
        );
    } else {
        emit(
            &tx,
            rp.as_deref(),
            RuntimeEvent::TaskBatchFinished {
                instance_id,
                succeeded: all_ok,
            },
        );
    }
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
