use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, mpsc};

use image::RgbaImage;
use moca_cloud::{CloudClient, StubCloudClient};
use moca_maa::{
    DiscoveredAdb, DiscoveredWin32, MaaSession, SessionProgressEvent, SessionRunner,
    discover_adb_devices, discover_win32_windows, foreground_window_hwnd, init_maa,
    is_current_process_foreground,
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

/// Cloneable command sender that allows out-of-band callers (e.g. hotkey thread) to dispatch commands.
#[derive(Clone)]
pub struct RuntimeSender {
    cmd_tx: tokio_mpsc::UnboundedSender<RuntimeCommand>,
}

impl RuntimeSender {
    pub fn send(&self, cmd: RuntimeCommand) {
        let _ = self.cmd_tx.send(cmd);
    }
}

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
                        running_tasks: Mutex::new(HashMap::new()),
                        desired_triggers: Mutex::new(HashMap::new()),
                        instance_epochs: Mutex::new(HashMap::new()),
                        run_seq: AtomicU64::new(1),
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
                                st.desired_triggers.lock().await.remove(&instance_id);
                                let _ = bump_epoch_and_stop(&st, &instance_id).await;
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
                                let (epoch, was_trigger) =
                                    bump_epoch_and_stop(&st, &instance_id).await;
                                if was_trigger == Some(true) {
                                    emit_log(
                                        &tx,
                                        rp.as_deref(),
                                        LogLevel::Info,
                                        format!(
                                            "[{instance_id}] 已暂停实时辅助让出控制权，准备执行手动任务..."
                                        ),
                                    );
                                }
                                tokio::spawn(run_manual_batch_task(
                                    st,
                                    tx,
                                    rp,
                                    epoch,
                                    instance_id,
                                    game_name,
                                    bundle_dirs,
                                    tasks,
                                    mode,
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
                                    let spec = ActiveTriggerSpec {
                                        task_id: task_id.clone(),
                                        game_name: game_name.clone(),
                                        bundle_dirs: bundle_dirs.clone(),
                                        task: *task,
                                        mode,
                                    };
                                    st.desired_triggers
                                        .lock()
                                        .await
                                        .insert(instance_id.clone(), spec.clone());
                                    let batch_busy = st
                                        .running_tasks
                                        .lock()
                                        .await
                                        .get(&instance_id)
                                        .is_some_and(|h| !h.is_trigger);
                                    if batch_busy {
                                        emit_log(
                                            &tx,
                                            rp.as_deref(),
                                            LogLevel::Info,
                                            format!(
                                                "已保存「{}」规则配置，将在当前批量任务完成后自动恢复",
                                                spec.task.decl.label
                                            ),
                                        );
                                    } else {
                                        let (epoch, _) =
                                            bump_epoch_and_stop(&st, &instance_id).await;
                                        tokio::spawn(run_trigger_task(
                                            st,
                                            tx,
                                            rp,
                                            epoch,
                                            instance_id,
                                            spec,
                                        ));
                                    }
                                } else {
                                    st.desired_triggers.lock().await.remove(&instance_id);
                                    let is_running_trigger = st
                                        .running_tasks
                                        .lock()
                                        .await
                                        .get(&instance_id)
                                        .is_some_and(|h| h.is_trigger);
                                    if is_running_trigger {
                                        let _ = bump_epoch_and_stop(&st, &instance_id).await;
                                        emit_log(
                                            &tx,
                                            rp.as_deref(),
                                            LogLevel::Info,
                                            format!("已关闭实例 [{instance_id}] 实时辅助"),
                                        );
                                    }
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
                                let removed_trigger =
                                    st.desired_triggers.lock().await.remove(&instance_id);
                                let (_, was_running) = bump_epoch_and_stop(&st, &instance_id).await;
                                if let Some(spec) = removed_trigger {
                                    emit(
                                        &tx,
                                        rp.as_deref(),
                                        RuntimeEvent::TriggerStateChanged {
                                            instance_id: instance_id.clone(),
                                            task_id: spec.task_id,
                                            active: false,
                                        },
                                    );
                                }
                                if was_running.is_some() {
                                    emit_log(
                                        &tx,
                                        rp.as_deref(),
                                        LogLevel::Warn,
                                        format!("已停止实例 [{instance_id}] 当前任务"),
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

    pub fn sender(&self) -> RuntimeSender {
        RuntimeSender {
            cmd_tx: self.cmd_tx.clone(),
        }
    }

    pub fn try_recv_event(&self) -> Option<RuntimeEvent> {
        self.event_rx.try_recv().ok()
    }

    pub fn cloud(&self) -> &StubCloudClient {
        &self.cloud
    }
}

#[derive(Clone)]
struct ActiveTriggerSpec {
    task_id: String,
    game_name: String,
    bundle_dirs: Vec<PathBuf>,
    task: ResolvedTask,
    mode: ExecutionMode,
}

struct RunningTaskHandle {
    run_id: u64,
    cancel_flag: Arc<AtomicBool>,
    is_trigger: bool,
}

struct WorkerState {
    sessions: Mutex<HashMap<String, MaaSession>>,
    running_tasks: Mutex<HashMap<String, RunningTaskHandle>>,
    desired_triggers: Mutex<HashMap<String, ActiveTriggerSpec>>,
    instance_epochs: Mutex<HashMap<String, u64>>,
    run_seq: AtomicU64,
    /// Ensures ForegroundExclusive Win32 instances do not fight over window focus/mouse.
    foreground_lock: Mutex<()>,
    cloud: Arc<StubCloudClient>,
}

async fn bump_epoch_and_stop(st: &Arc<WorkerState>, instance_id: &str) -> (u64, Option<bool>) {
    let epoch = st.run_seq.fetch_add(1, Ordering::Relaxed) + 1;
    st.instance_epochs
        .lock()
        .await
        .insert(instance_id.to_string(), epoch);

    let was_trigger = {
        let guard = st.running_tasks.lock().await;
        guard.get(instance_id).map(|h| {
            h.cancel_flag.store(true, Ordering::Relaxed);
            h.is_trigger
        })
    };

    if was_trigger.is_some()
        && let Some(runner) = st
            .sessions
            .lock()
            .await
            .get(instance_id)
            .map(|s| s.clone_runner())
    {
        runner.stop();
    }

    (epoch, was_trigger)
}

async fn wait_instance_slot(st: &Arc<WorkerState>, instance_id: &str, my_epoch: u64) -> bool {
    for _ in 0..160 {
        if st.instance_epochs.lock().await.get(instance_id).copied() != Some(my_epoch) {
            return false;
        }
        if !st.running_tasks.lock().await.contains_key(instance_id) {
            let runner = st
                .sessions
                .lock()
                .await
                .get(instance_id)
                .map(|s| s.clone_runner());
            if let Some(runner) = runner {
                for _ in 0..75 {
                    if st.instance_epochs.lock().await.get(instance_id).copied() != Some(my_epoch) {
                        return false;
                    }
                    if !runner.is_stopping() {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                }
            }
            return true;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    false
}

async fn unregister_run_slot(st: &Arc<WorkerState>, instance_id: &str, my_epoch: u64) {
    let mut guard = st.running_tasks.lock().await;
    if guard.get(instance_id).is_some_and(|h| h.run_id == my_epoch) {
        guard.remove(instance_id);
    }
}

async fn prepare_session_runner(
    st: &Arc<WorkerState>,
    tx: &mpsc::Sender<RuntimeEvent>,
    rp: &RepaintCb,
    instance_id: &str,
    trigger_task_id: Option<&str>,
    action_counter: &Arc<AtomicU32>,
    miss_counter: &Arc<AtomicU32>,
) -> Option<SessionRunner> {
    let guard = st.sessions.lock().await;
    guard.get(instance_id).map(|session| {
        let tasker_ref = session.clone_tasker();
        let tx_sink = tx.clone();
        let rp_sink = rp.clone();
        let id_sink = instance_id.to_string();
        let trig_id = trigger_task_id.map(ToString::to_string);
        let act_cnt = Arc::clone(action_counter);
        let miss_cnt = Arc::clone(miss_counter);

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
        session.clone_runner()
    })
}

#[allow(clippy::too_many_arguments)]
async fn run_manual_batch_task(
    st: Arc<WorkerState>,
    tx: mpsc::Sender<RuntimeEvent>,
    rp: RepaintCb,
    epoch: u64,
    instance_id: String,
    game_name: String,
    bundle_dirs: Vec<PathBuf>,
    tasks: Vec<ResolvedTask>,
    mode: ExecutionMode,
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

    if !wait_instance_slot(&st, &instance_id, epoch).await {
        return;
    }

    let action_counter = Arc::new(AtomicU32::new(0));
    let miss_counter = Arc::new(AtomicU32::new(0));
    let Some(runner) = prepare_session_runner(
        &st,
        &tx,
        &rp,
        &instance_id,
        None,
        &action_counter,
        &miss_counter,
    )
    .await
    else {
        emit_log(
            &tx,
            rp.as_deref(),
            LogLevel::Warn,
            format!("请先连接目标设备/窗口后再开启「{game_name}」任务"),
        );
        return;
    };

    let cancel_flag = Arc::new(AtomicBool::new(false));
    st.running_tasks.lock().await.insert(
        instance_id.clone(),
        RunningTaskHandle {
            run_id: epoch,
            cancel_flag: Arc::clone(&cancel_flag),
            is_trigger: false,
        },
    );

    emit(
        &tx,
        rp.as_deref(),
        RuntimeEvent::TaskBatchStarted {
            instance_id: instance_id.clone(),
        },
    );

    let mut fg_guard = if mode == ExecutionMode::ForegroundExclusive {
        Some(st.foreground_lock.lock().await)
    } else {
        None
    };

    let load_res = tokio::task::spawn_blocking({
        let runner_load = runner.clone();
        let dirs = bundle_dirs.clone();
        move || runner_load.load_bundles(&dirs)
    })
    .await;

    if !matches!(load_res, Ok(Ok(()))) {
        emit_log(
            &tx,
            rp.as_deref(),
            LogLevel::Error,
            format!("加载「{game_name}」资源失败"),
        );
        drop(fg_guard);
        unregister_run_slot(&st, &instance_id, epoch).await;
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

    let total_items = tasks.len();
    let mut executed_pre_entries = std::collections::HashSet::new();
    let mut pending_post_entries = Vec::new();
    let mut batch_results: Vec<(String, String)> = Vec::new();
    let mut all_ok = true;
    let saw_target_fg = Arc::new(AtomicBool::new(false));
    let target_hwnd = if mode == ExecutionMode::ForegroundExclusive {
        runner.win32_hwnd()
    } else {
        None
    };

    for (idx, resolved) in tasks.into_iter().enumerate() {
        if cancel_flag.load(Ordering::Relaxed) {
            all_ok = false;
            break;
        }
        let task = resolved.decl;
        let pipeline_override = resolved.pipeline_override;
        let item_label = resolved.item_label;
        let pre_entry = resolved.pre_entry;
        if let Some(post) = resolved.post_entry
            && !pending_post_entries.contains(&post)
        {
            pending_post_entries.push(post);
        }
        let result_ocr_roi = resolved.result_ocr_roi;
        let result_ocr_ignore = resolved.result_ocr_ignore;

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

        // If user switched back to Moca mid-batch under ForegroundExclusive, pause cleanly
        // without stealing focus back to the game window until user leaves Moca or stops.
        if target_hwnd.is_some()
            && saw_target_fg.load(Ordering::Relaxed)
            && is_current_process_foreground()
        {
            fg_guard = None;
            emit_log(
                &tx,
                rp.as_deref(),
                LogLevel::Info,
                format!(
                    "[{instance_id}] ⏸ 检测到已切回 Moca 窗口，暂停抢占前台；切回游戏后自动继续（或点「停止」取消）"
                ),
            );
            while !cancel_flag.load(Ordering::Relaxed) && is_current_process_foreground() {
                tokio::time::sleep(std::time::Duration::from_millis(150)).await;
            }
            if cancel_flag.load(Ordering::Relaxed) {
                all_ok = false;
                break;
            }
            if mode == ExecutionMode::ForegroundExclusive {
                fg_guard = Some(st.foreground_lock.lock().await);
            }
            emit_log(
                &tx,
                rp.as_deref(),
                LogLevel::Info,
                format!("[{instance_id}] ▶ 已继续执行剩余任务"),
            );
        }

        if let Some(pre) = pre_entry
            && executed_pre_entries.insert(pre.clone())
        {
            emit_log(
                &tx,
                rp.as_deref(),
                LogLevel::Info,
                format!("[{instance_id}] 执行前置导航: {pre}"),
            );
            let pre_res = run_entry_tracking_focus(
                &runner,
                pre,
                pipeline_override.clone(),
                target_hwnd,
                &saw_target_fg,
            )
            .await;
            if matches!(pre_res, Ok(Ok(false))) {
                emit_log(
                    &tx,
                    rp.as_deref(),
                    LogLevel::Warn,
                    format!("前置导航未完全匹配，尝试直接执行「{}」", task.label),
                );
            }
        }

        if cancel_flag.load(Ordering::Relaxed) {
            all_ok = false;
            break;
        }

        // Also yield before submitting next code if user switched to Moca during pre_entry.
        if target_hwnd.is_some()
            && saw_target_fg.load(Ordering::Relaxed)
            && is_current_process_foreground()
        {
            fg_guard = None;
            emit_log(
                &tx,
                rp.as_deref(),
                LogLevel::Info,
                format!(
                    "[{instance_id}] ⏸ 检测到已切回 Moca 窗口，暂停提交下一项；切回游戏后自动继续"
                ),
            );
            while !cancel_flag.load(Ordering::Relaxed) && is_current_process_foreground() {
                tokio::time::sleep(std::time::Duration::from_millis(150)).await;
            }
            if cancel_flag.load(Ordering::Relaxed) {
                all_ok = false;
                break;
            }
            if mode == ExecutionMode::ForegroundExclusive {
                fg_guard = Some(st.foreground_lock.lock().await);
            }
        }

        if let Some(label) = &item_label {
            emit_log(
                &tx,
                rp.as_deref(),
                LogLevel::Info,
                format!("[{}/{}] 正在提交: {label}", idx + 1, total_items),
            );
        }

        let run_res = run_entry_tracking_focus(
            &runner,
            task.entry.clone(),
            pipeline_override.clone(),
            target_hwnd,
            &saw_target_fg,
        )
        .await;

        let step_ok = match run_res {
            Ok(Ok(true)) => true,
            Ok(Ok(false)) => {
                all_ok = false;
                false
            }
            Ok(Err(e)) => {
                all_ok = false;
                emit_log(
                    &tx,
                    rp.as_deref(),
                    LogLevel::Error,
                    format!("任务「{}」执行出错: {e}", task.label),
                );
                false
            }
            Err(e) => {
                all_ok = false;
                emit_log(
                    &tx,
                    rp.as_deref(),
                    LogLevel::Error,
                    format!("任务线程异常: {e}"),
                );
                false
            }
        };

        if let Some(roi) = result_ocr_roi {
            let key = item_label.clone().unwrap_or_else(|| task.label.clone());
            let feedback = if step_ok {
                let ctrl_ocr = runner.ocr_reader();
                let key_filter = key.clone();
                let ignores = result_ocr_ignore.clone();
                tokio::task::spawn_blocking(move || {
                    poll_ocr_feedback(&ctrl_ocr, roi, Some(&key_filter), &ignores)
                })
                .await
                .unwrap_or_else(|e| format!("OCR 线程异常: {e}"))
            } else {
                "步骤执行未成功（未完成确认）".to_string()
            };
            emit_log(
                &tx,
                rp.as_deref(),
                if feedback.contains("成功")
                    || feedback.contains("已发")
                    || feedback.contains("邮件")
                    || feedback.contains("奖励")
                {
                    LogLevel::Info
                } else {
                    LogLevel::Warn
                },
                format!("[结果] {key} -> {feedback}"),
            );
            batch_results.push((key, feedback));
        }
    }

    if batch_results.len() > 1 {
        let summary = batch_results
            .iter()
            .map(|(k, v)| format!("{k}: {v}"))
            .collect::<Vec<_>>()
            .join(" | ");
        emit_log(
            &tx,
            rp.as_deref(),
            LogLevel::Info,
            format!("批量结果汇总 ({} 条): {summary}", batch_results.len()),
        );
    }

    if !cancel_flag.load(Ordering::Relaxed) {
        for post in pending_post_entries {
            if cancel_flag.load(Ordering::Relaxed) {
                break;
            }
            emit_log(
                &tx,
                rp.as_deref(),
                LogLevel::Info,
                format!("[{instance_id}] 执行收尾清理: {post}"),
            );
            let _ =
                run_entry_tracking_focus(&runner, post, None, target_hwnd, &saw_target_fg).await;
        }
    }

    drop(fg_guard);
    unregister_run_slot(&st, &instance_id, epoch).await;
    emit(
        &tx,
        rp.as_deref(),
        RuntimeEvent::TaskBatchFinished {
            instance_id: instance_id.clone(),
            succeeded: all_ok,
        },
    );

    let was_cancelled = cancel_flag.load(Ordering::Relaxed);
    let still_latest = st.instance_epochs.lock().await.get(&instance_id).copied() == Some(epoch);
    let desired_spec = if !was_cancelled && still_latest {
        st.desired_triggers.lock().await.get(&instance_id).cloned()
    } else {
        None
    };
    if let Some(spec) = desired_spec {
        let next_epoch = st.run_seq.fetch_add(1, Ordering::Relaxed) + 1;
        st.instance_epochs
            .lock()
            .await
            .insert(instance_id.clone(), next_epoch);
        emit_log(
            &tx,
            rp.as_deref(),
            LogLevel::Info,
            format!(
                "[{instance_id}] ▶ 手动任务已完成，自动恢复实时辅助:「{}」",
                spec.task.decl.label
            ),
        );
        tokio::spawn(run_trigger_task(st, tx, rp, next_epoch, instance_id, spec));
    }
}

async fn run_trigger_task(
    st: Arc<WorkerState>,
    tx: mpsc::Sender<RuntimeEvent>,
    rp: RepaintCb,
    epoch: u64,
    instance_id: String,
    spec: ActiveTriggerSpec,
) {
    if !wait_instance_slot(&st, &instance_id, epoch).await {
        return;
    }

    if let Err(e) = st.cloud.verify_task_access(&spec.task.decl) {
        emit_log(
            &tx,
            rp.as_deref(),
            LogLevel::Warn,
            format!("跳过 VIP 实时辅助「{}」: {e}", spec.task.decl.label),
        );
        emit(
            &tx,
            rp.as_deref(),
            RuntimeEvent::TriggerStateChanged {
                instance_id,
                task_id: spec.task_id,
                active: false,
            },
        );
        return;
    }

    let action_counter = Arc::new(AtomicU32::new(0));
    let miss_counter = Arc::new(AtomicU32::new(0));
    let Some(runner) = prepare_session_runner(
        &st,
        &tx,
        &rp,
        &instance_id,
        Some(&spec.task_id),
        &action_counter,
        &miss_counter,
    )
    .await
    else {
        emit_log(
            &tx,
            rp.as_deref(),
            LogLevel::Warn,
            format!("请先连接目标设备/窗口后再开启「{}」", spec.game_name),
        );
        emit(
            &tx,
            rp.as_deref(),
            RuntimeEvent::TriggerStateChanged {
                instance_id,
                task_id: spec.task_id,
                active: false,
            },
        );
        return;
    };

    let cancel_flag = Arc::new(AtomicBool::new(false));
    st.running_tasks.lock().await.insert(
        instance_id.clone(),
        RunningTaskHandle {
            run_id: epoch,
            cancel_flag: Arc::clone(&cancel_flag),
            is_trigger: true,
        },
    );

    emit(
        &tx,
        rp.as_deref(),
        RuntimeEvent::TriggerStateChanged {
            instance_id: instance_id.clone(),
            task_id: spec.task_id.clone(),
            active: true,
        },
    );

    let load_res = tokio::task::spawn_blocking({
        let runner_load = runner.clone();
        let dirs = spec.bundle_dirs.clone();
        move || runner_load.load_bundles(&dirs)
    })
    .await;

    if !matches!(load_res, Ok(Ok(()))) {
        emit_log(
            &tx,
            rp.as_deref(),
            LogLevel::Error,
            format!("加载「{}」资源失败", spec.game_name),
        );
        unregister_run_slot(&st, &instance_id, epoch).await;
        emit(
            &tx,
            rp.as_deref(),
            RuntimeEvent::TriggerStateChanged {
                instance_id,
                task_id: spec.task_id,
                active: false,
            },
        );
        return;
    }

    let watch_hwnd = if spec.mode == ExecutionMode::ForegroundExclusive {
        runner.win32_hwnd()
    } else {
        None
    };
    let mut logged_focus_yield = false;

    while !cancel_flag.load(Ordering::Relaxed) {
        if let Some(target_hwnd) = watch_hwnd
            && foreground_window_hwnd() != target_hwnd
        {
            if !logged_focus_yield {
                logged_focus_yield = true;
                emit_log(
                    &tx,
                    rp.as_deref(),
                    LogLevel::Info,
                    format!(
                        "[{instance_id}] Moca/其他窗口在前台中，「{}」已静默避让焦点；切回游戏后自动恢复",
                        spec.task.decl.label
                    ),
                );
            }
            tokio::time::sleep(std::time::Duration::from_millis(220)).await;
            continue;
        }
        if logged_focus_yield {
            logged_focus_yield = false;
            emit_log(
                &tx,
                rp.as_deref(),
                LogLevel::Info,
                format!(
                    "[{instance_id}] 检测到已切回游戏窗口，恢复实时辅助「{}」",
                    spec.task.decl.label
                ),
            );
        }

        let _fg_guard = if spec.mode == ExecutionMode::ForegroundExclusive {
            Some(st.foreground_lock.lock().await)
        } else {
            None
        };
        if cancel_flag.load(Ordering::Relaxed) {
            break;
        }

        let yielded_focus = Arc::new(AtomicBool::new(false));
        let entry = spec.task.decl.entry.clone();
        let ov = spec.task.pipeline_override.clone();
        let runner_exec = runner.clone();
        let mut handle =
            tokio::task::spawn_blocking(move || runner_exec.run_task_entry(&entry, ov.as_ref()));

        let run_res = if let Some(target_hwnd) = watch_hwnd {
            let runner_watch = runner.clone();
            let cancel_watch = Arc::clone(&cancel_flag);
            let yield_flag = Arc::clone(&yielded_focus);
            tokio::select! {
                res = &mut handle => res,
                _ = async move {
                    loop {
                        tokio::time::sleep(std::time::Duration::from_millis(120)).await;
                        if cancel_watch.load(Ordering::Relaxed) {
                            break;
                        }
                        if foreground_window_hwnd() != target_hwnd {
                            yield_flag.store(true, Ordering::Relaxed);
                            runner_watch.stop();
                            break;
                        }
                    }
                } => handle.await,
            }
        } else {
            handle.await
        };

        if yielded_focus.load(Ordering::Relaxed) {
            let runner_wait = runner.clone();
            let _ = tokio::task::spawn_blocking(move || runner_wait.stop_and_wait()).await;
            if !cancel_flag.load(Ordering::Relaxed) {
                tokio::time::sleep(std::time::Duration::from_millis(180)).await;
                continue;
            }
        }

        if let Ok(Err(e)) = run_res {
            emit_log(
                &tx,
                rp.as_deref(),
                LogLevel::Error,
                format!("实时辅助「{}」运行出错: {e}", spec.task.decl.label),
            );
        }
        break;
    }

    unregister_run_slot(&st, &instance_id, epoch).await;
    // Keep UI trigger indicator off only when stopped or exited without pending batch takeover.
    let still_desired = st
        .desired_triggers
        .lock()
        .await
        .get(&instance_id)
        .is_some_and(|cur| cur.task_id == spec.task_id);
    if !still_desired {
        emit(
            &tx,
            rp.as_deref(),
            RuntimeEvent::TriggerStateChanged {
                instance_id,
                task_id: spec.task_id,
                active: false,
            },
        );
    }
}

async fn run_entry_tracking_focus(
    runner: &SessionRunner,
    entry: String,
    pipeline_override: Option<serde_json::Value>,
    target_hwnd: Option<isize>,
    saw_target_fg: &Arc<AtomicBool>,
) -> Result<Result<bool, moca_maa::MaaAdapterError>, tokio::task::JoinError> {
    let runner_run = runner.clone();
    let mut handle = tokio::task::spawn_blocking(move || {
        runner_run.run_task_entry(&entry, pipeline_override.as_ref())
    });

    let Some(hwnd) = target_hwnd else {
        return handle.await;
    };

    let saw = Arc::clone(saw_target_fg);
    let done = Arc::new(AtomicBool::new(false));
    let done_watch = Arc::clone(&done);
    let watcher = tokio::spawn(async move {
        while !done_watch.load(Ordering::Relaxed) {
            if foreground_window_hwnd() == hwnd {
                saw.store(true, Ordering::Relaxed);
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(80)).await;
        }
    });
    let res = (&mut handle).await;
    done.store(true, Ordering::Relaxed);
    watcher.abort();
    res
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

fn poll_ocr_feedback(
    ocr: &moca_maa::OcrReader,
    roi: [u32; 4],
    item_key: Option<&str>,
    ignores: &[String],
) -> String {
    let mut best_text = String::new();
    for attempt in 0..8 {
        if attempt > 0 {
            std::thread::sleep(std::time::Duration::from_millis(120));
        } else {
            std::thread::sleep(std::time::Duration::from_millis(220));
        }
        let Ok(raw) = ocr.recognize_roi(Some(roi)) else {
            continue;
        };
        let cleaned = extract_feedback_text(&raw, item_key, ignores);
        if !cleaned.is_empty() {
            best_text = cleaned;
            if looks_like_terminal_feedback(&best_text) {
                return best_text;
            }
        }
    }
    if best_text.is_empty() {
        "已提交（未捕捉到弹窗反馈文字）".to_string()
    } else {
        best_text
    }
}

fn extract_feedback_text(raw: &str, item_key: Option<&str>, ignores: &[String]) -> String {
    let item_upper = item_key.map(|s| s.to_ascii_uppercase());
    let mut lines = Vec::new();
    for part in raw.lines() {
        let trimmed = part.trim();
        if trimmed.chars().count() < 2 {
            continue;
        }
        let upper = trimmed.to_ascii_uppercase();
        if item_upper.as_ref().is_some_and(|k| upper.contains(k)) {
            continue;
        }
        if ignores.iter().any(|ig| {
            let ig_t = ig.trim();
            !ig_t.is_empty() && (trimmed == ig_t || trimmed.contains(ig_t))
        }) {
            continue;
        }
        if !lines.iter().any(|l: &String| l == trimmed) {
            lines.push(trimmed.to_string());
        }
    }
    lines.join(" · ")
}

fn looks_like_terminal_feedback(text: &str) -> bool {
    const KEYWORDS: &[&str] = &[
        "成功",
        "领取",
        "奖励",
        "邮件",
        "已兑换",
        "已使用",
        "已被使用",
        "重复",
        "过期",
        "失效",
        "无效",
        "不存在",
        "错误",
        "上限",
        "次数",
        "稍后",
    ];
    KEYWORDS.iter().any(|k| text.contains(k))
}
