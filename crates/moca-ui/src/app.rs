use std::collections::HashMap;
use std::path::PathBuf;

use egui::{
    Color32, ColorImage, CornerRadius, Margin, Sense, Stroke, TextureHandle, TextureOptions,
    ViewportCommand,
};
use image::RgbaImage;
use moca_maa::{
    DiscoveredAdb, DiscoveredWin32, GlobalHotkeyAction, GlobalHotkeyListener,
    is_current_process_elevated, toggle_window_foreground,
};
use moca_protocol::{
    ControllerKind, DeviceTarget, ExecutionMode, ResolvedTask, SavedGameProfile, TaskDecl, TaskKind,
};
use moca_runtime::worker::LogLevel;
use moca_runtime::{
    LoadedGamePackage, RuntimeCommand, RuntimeEvent, RuntimeHandle, resolve_workspace_subdir,
};

use crate::fonts::configure_cjk_fonts;
use crate::icons::{UiIcon, allocate_and_paint_icon};
use crate::roi_tool::{RoiToolState, save_cropped_template};
use crate::schema_ui::{SchemaAction, render_task_inspector, render_task_list};
use crate::theme::{
    IconButtonVariant, SegmentedOption, ThemeTokens, WindowCaptionKind, format_local_hms,
    handle_borderless_resize, pill_badge, pill_badge_with_icon,
};

const GITHUB_REPO_URL: &str = "https://github.com/limitcool/moca";

fn profiles_dir() -> PathBuf {
    resolve_workspace_subdir("profiles")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TargetTab {
    Win32,
    Adb,
}

struct LogEntry {
    time: String,
    level: LogLevel,
    message: String,
}

pub struct MocaApp {
    runtime: RuntimeHandle,
    hotkeys: GlobalHotkeyListener,
    maa_version: Option<String>,
    maa_error: Option<String>,
    process_elevated: bool,
    win32_hwnd: Option<isize>,
    icon_sync_remaining: u8,

    packages: Vec<LoadedGamePackage>,
    moca_icon_texture: Option<TextureHandle>,
    game_icons: HashMap<String, TextureHandle>,
    banner_textures: HashMap<String, TextureHandle>,
    theme: ThemeTokens,
    selected_game_idx: usize,
    selected_tasks: Vec<String>,
    focused_task_id: Option<String>,
    option_values: HashMap<String, serde_json::Value>,

    target_tab: TargetTab,
    adb_devices: Vec<DiscoveredAdb>,
    selected_adb_idx: Option<usize>,
    win32_windows: Vec<DiscoveredWin32>,
    win32_icons: Vec<Option<TextureHandle>>,
    selected_win32_idx: Option<usize>,
    matched_win32_indices: Vec<usize>,
    win32_filter: String,
    execution_mode: ExecutionMode,

    active_instance_id: String,
    connected: bool,
    tasks_running: bool,
    active_triggers: HashMap<String, (bool, u32)>,
    resolution: Option<(i32, i32)>,

    last_frame: Option<RgbaImage>,
    preview_texture: Option<TextureHandle>,
    preview_size: Option<[usize; 2]>,
    roi_tool: RoiToolState,
    logs: Vec<LogEntry>,
}

impl MocaApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut win32_hwnd = None;
        #[cfg(target_os = "windows")]
        {
            use raw_window_handle::{HasWindowHandle, RawWindowHandle};
            if let Ok(handle) = cc.window_handle()
                && let RawWindowHandle::Win32(win32) = handle.as_raw()
            {
                let hwnd = win32.hwnd.get();
                win32_hwnd = Some(hwnd);
                moca_maa::apply_window_icon_from_exe(hwnd);
            }
        }

        configure_cjk_fonts(&cc.egui_ctx);
        let theme = ThemeTokens::obsidian();
        theme.apply(&cc.egui_ctx);

        let ctx_clone = cc.egui_ctx.clone();
        let runtime = RuntimeHandle::spawn(Some(Box::new(move || {
            ctx_clone.request_repaint();
        })));

        runtime.send(RuntimeCommand::Initialize);
        runtime.send(RuntimeCommand::ReloadGamePackages);

        let rt_sender = runtime.sender();
        let ctx_hk = cc.egui_ctx.clone();
        let hwnd_opt = win32_hwnd;
        let default_instance = "default-session".to_string();
        let hotkeys = GlobalHotkeyListener::start(move |action| {
            match action {
                GlobalHotkeyAction::EmergencyStop => {
                    rt_sender.send(RuntimeCommand::StopTasks {
                        instance_id: default_instance.clone(),
                    });
                }
                GlobalHotkeyAction::ToggleWindow => {
                    if let Some(hwnd) = hwnd_opt {
                        toggle_window_foreground(hwnd);
                    }
                }
                GlobalHotkeyAction::ToggleTriggers => {}
            }
            ctx_hk.request_repaint();
        });

        let mut logs = vec![LogEntry {
            time: format_local_hms(),
            level: LogLevel::Info,
            message: "Moca 底座已启动，正在初始化 MaaFramework 与游戏包...".into(),
        }];
        let failed = hotkeys.failed_keys();
        if failed.is_empty() {
            logs.push(LogEntry {
                time: format_local_hms(),
                level: LogLevel::Info,
                message:
                    "全局热键已就绪: F8 开关辅助 · F9 急停 · F10 显隐窗口（窗口内 F5 截图/刷新）"
                        .into(),
            });
        } else {
            logs.push(LogEntry {
                time: format_local_hms(),
                level: LogLevel::Warn,
                message: format!(
                    "部分全局热键已被其他软件占用 ({})，其余快捷键仍可用",
                    failed.join(", ")
                ),
            });
        }

        let moca_icon_texture = load_embedded_moca_icon(&cc.egui_ctx);

        Self {
            runtime,
            hotkeys,
            maa_version: None,
            maa_error: None,
            process_elevated: is_current_process_elevated(),
            win32_hwnd,
            icon_sync_remaining: if win32_hwnd.is_some() { 10 } else { 0 },
            packages: Vec::new(),
            moca_icon_texture,
            game_icons: HashMap::new(),
            banner_textures: HashMap::new(),
            theme,
            selected_game_idx: 0,
            selected_tasks: Vec::new(),
            focused_task_id: None,
            option_values: HashMap::new(),
            target_tab: TargetTab::Win32,
            adb_devices: Vec::new(),
            selected_adb_idx: None,
            win32_windows: Vec::new(),
            win32_icons: Vec::new(),
            selected_win32_idx: None,
            matched_win32_indices: Vec::new(),
            win32_filter: String::new(),
            execution_mode: ExecutionMode::ForegroundExclusive,
            active_instance_id: "default-session".to_string(),
            connected: false,
            tasks_running: false,
            active_triggers: HashMap::new(),
            resolution: None,
            last_frame: None,
            preview_texture: None,
            preview_size: None,
            roi_tool: RoiToolState::new(),
            logs,
        }
    }

    fn push_log(&mut self, level: LogLevel, message: impl Into<String>) {
        self.logs.push(LogEntry {
            time: format_local_hms(),
            level,
            message: message.into(),
        });
        if self.logs.len() > 300 {
            let excess = self.logs.len() - 300;
            self.logs.drain(0..excess);
        }
    }

    fn load_game_icons(&mut self, ctx: &egui::Context) {
        self.game_icons.clear();
        self.banner_textures.clear();
        for pkg in &self.packages {
            let game_id = pkg.manifest.effective_game_id().to_string();
            if let Some(icon_rel) = &pkg.manifest.icon
                && let Ok(bytes) = std::fs::read(pkg.root_dir.join(icon_rel))
                && let Ok(img) = image::load_from_memory(&bytes)
            {
                let rgba = img.to_rgba8();
                let size = [rgba.width() as usize, rgba.height() as usize];
                let color_image = ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
                let tex = ctx.load_texture(
                    format!("game_icon_{game_id}"),
                    color_image,
                    TextureOptions::LINEAR,
                );
                self.game_icons.insert(game_id.clone(), tex);
            }
            if let Some(banner_rel) = pkg
                .manifest
                .theme
                .as_ref()
                .and_then(|t| t.banner.as_deref())
                && let Ok(bytes) = std::fs::read(pkg.root_dir.join(banner_rel))
                && let Ok(img) = image::load_from_memory(&bytes)
            {
                let rgba = img.to_rgba8();
                let size = [rgba.width() as usize, rgba.height() as usize];
                let color_image = ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
                let tex = ctx.load_texture(
                    format!("game_banner_{game_id}"),
                    color_image,
                    TextureOptions::LINEAR,
                );
                self.banner_textures.insert(game_id, tex);
            }
        }
    }

    fn apply_profile_for_selected_game(&mut self, ctx: &egui::Context) {
        let Some(pkg) = self.packages.get(self.selected_game_idx) else {
            return;
        };
        let game_id = pkg.manifest.effective_game_id().to_string();
        self.theme = ThemeTokens::from_game_theme(pkg.manifest.theme.as_ref());
        self.theme.apply(ctx);
        self.focused_task_id = None;
        match SavedGameProfile::load(profiles_dir(), &game_id) {
            Ok(Some(profile)) => {
                self.selected_tasks = profile.selected_tasks;
                self.option_values = profile.option_values;
                self.execution_mode = profile.execution_mode;
            }
            Ok(None) => {
                // Default to enabling all free tasks (including core realtime triggers such as AutoSkipStory).
                let mut defaults: Vec<String> = pkg
                    .manifest
                    .task
                    .iter()
                    .filter(|(_, t)| t.tier == moca_protocol::TaskTier::Free)
                    .map(|(id, _)| id.clone())
                    .collect();
                defaults.sort();
                self.selected_tasks = defaults;
                self.option_values.clear();
                self.execution_mode = pkg
                    .manifest
                    .controller
                    .values()
                    .find(|c| c.kind == ControllerKind::Win32)
                    .map(|c| c.config.execution_mode)
                    .unwrap_or(ExecutionMode::Background);
                self.save_current_profile();
            }
            Err(err) => {
                self.push_log(LogLevel::Warn, format!("读取配置失败 ({game_id}): {err}"));
            }
        }
        self.recompute_win32_matches();
    }

    fn open_repo_url(&mut self, ctx: &egui::Context) {
        if let Err(err) = moca_maa::open_external_url(GITHUB_REPO_URL) {
            self.push_log(
                LogLevel::Warn,
                format!("打开 GitHub 链接失败 ({err})，尝试默认回退"),
            );
            ctx.open_url(egui::OpenUrl::new_tab(GITHUB_REPO_URL));
        }
    }

    fn save_current_profile(&mut self) {
        let Some(pkg) = self.packages.get(self.selected_game_idx) else {
            return;
        };
        let profile = SavedGameProfile {
            game_id: pkg.manifest.effective_game_id().to_string(),
            execution_mode: self.execution_mode,
            selected_tasks: self.selected_tasks.clone(),
            option_values: self.option_values.clone(),
        };
        if let Err(err) = profile.save(profiles_dir()) {
            self.push_log(LogLevel::Warn, format!("保存配置失败: {err}"));
        }
    }

    fn recompute_win32_matches(&mut self) {
        self.matched_win32_indices.clear();
        let Some(pkg) = self.packages.get(self.selected_game_idx) else {
            return;
        };
        let win32_cfg = pkg
            .manifest
            .controller
            .values()
            .find(|c| c.kind == ControllerKind::Win32)
            .map(|c| &c.config);

        let title_pat = win32_cfg
            .and_then(|c| c.title_regex.as_deref())
            .filter(|p| !p.trim().is_empty() && *p != ".*");
        let class_pat = win32_cfg
            .and_then(|c| c.class_regex.as_deref())
            .filter(|p| !p.trim().is_empty() && *p != ".*");

        if title_pat.is_some() || class_pat.is_some() {
            for (idx, win) in self.win32_windows.iter().enumerate() {
                if win.title.starts_with("Moca") {
                    continue;
                }
                let title_ok = title_pat
                    .map(|p| matches_title_pattern(p, &win.title))
                    .unwrap_or(true);
                let class_ok = class_pat
                    .map(|p| matches_title_pattern(p, &win.class_name))
                    .unwrap_or(true);
                if title_ok && class_ok {
                    self.matched_win32_indices.push(idx);
                }
            }
        }

        if let Some(&first_match) = self.matched_win32_indices.first() {
            self.selected_win32_idx = Some(first_match);
        } else if self.selected_win32_idx.is_none() && !self.win32_windows.is_empty() {
            self.selected_win32_idx = Some(0);
        }
    }

    fn resolve_bundle_dirs(pkg: &LoadedGamePackage) -> Vec<PathBuf> {
        let dirs: Vec<PathBuf> = pkg
            .manifest
            .resource
            .iter()
            .map(|r| pkg.root_dir.join(&r.path))
            .collect();
        if dirs.is_empty() {
            vec![pkg.root_dir.join("resource")]
        } else {
            dirs
        }
    }

    fn dispatch_tasks(&mut self, tasks: Vec<TaskDecl>) {
        let Some(pkg) = self.packages.get(self.selected_game_idx).cloned() else {
            self.push_log(LogLevel::Warn, "请先选择游戏包");
            return;
        };
        if !self.connected {
            self.push_log(LogLevel::Warn, "请先在左侧「连接目标」后再执行任务");
            return;
        }
        let bundle_dirs = Self::resolve_bundle_dirs(&pkg);
        let mut resolved: Vec<ResolvedTask> = Vec::new();
        for decl in tasks {
            let items = pkg.manifest.resolve_batch_items(&decl, &self.option_values);
            if items.is_empty() {
                self.push_log(
                    LogLevel::Warn,
                    format!("任务「{}」未填入有效条目，已跳过", decl.label),
                );
            } else {
                resolved.extend(items);
            }
        }
        if resolved.is_empty() {
            return;
        }
        self.push_log(
            LogLevel::Info,
            format!("已提交执行任务队列（共 {} 项）", resolved.len()),
        );
        self.runtime.send(RuntimeCommand::RunTasks {
            instance_id: self.active_instance_id.clone(),
            game_name: pkg.manifest.name.clone(),
            bundle_dirs,
            tasks: resolved,
            mode: self.execution_mode,
        });
    }

    fn drain_events(&mut self, ctx: &egui::Context) {
        while let Some(ev) = self.runtime.try_recv_event() {
            match ev {
                RuntimeEvent::MaaReady { version } => {
                    self.maa_version = Some(version);
                    self.maa_error = None;
                    self.runtime.send(RuntimeCommand::ScanDevices);
                }
                RuntimeEvent::MaaInitFailed { error } => {
                    self.maa_error = Some(error);
                }
                RuntimeEvent::GamePackagesLoaded(pkgs) => {
                    let was_empty = self.packages.is_empty();
                    self.packages = pkgs;
                    if was_empty
                        && let Some(ap_idx) = self
                            .packages
                            .iter()
                            .position(|p| p.manifest.effective_game_id() == "azur-promilia")
                    {
                        self.selected_game_idx = ap_idx;
                    } else if self.selected_game_idx >= self.packages.len() {
                        self.selected_game_idx = 0;
                    }
                    self.load_game_icons(ctx);
                    self.apply_profile_for_selected_game(ctx);
                }
                RuntimeEvent::DevicesDiscovered { adb, win32 } => {
                    self.adb_devices = adb;
                    self.win32_icons = win32
                        .iter()
                        .enumerate()
                        .map(|(i, w)| {
                            w.icon_rgba.as_ref().map(|(iw, ih, rgba)| {
                                let img = ColorImage::from_rgba_unmultiplied(
                                    [*iw as usize, *ih as usize],
                                    rgba,
                                );
                                ctx.load_texture(
                                    format!("win32_icon_{i}_{}", w.hwnd),
                                    img,
                                    TextureOptions::LINEAR,
                                )
                            })
                        })
                        .collect();
                    self.win32_windows = win32;
                    if self.selected_adb_idx.is_none() && !self.adb_devices.is_empty() {
                        self.selected_adb_idx = Some(0);
                    }
                    self.selected_win32_idx = None;
                    self.recompute_win32_matches();
                    if !self.matched_win32_indices.is_empty()
                        && let Some(pkg) = self.packages.get(self.selected_game_idx)
                    {
                        self.push_log(
                            LogLevel::Info,
                            format!(
                                "已自动匹配到「{}」对应游戏窗口 ({} 个)",
                                pkg.manifest.name,
                                self.matched_win32_indices.len()
                            ),
                        );
                    }
                }
                RuntimeEvent::Connected {
                    instance_id,
                    resolution,
                } => {
                    if instance_id == self.active_instance_id {
                        self.connected = true;
                        self.resolution = resolution;
                        self.runtime
                            .send(RuntimeCommand::CaptureFrame { instance_id });
                        self.start_enabled_triggers();
                    }
                }
                RuntimeEvent::Disconnected { instance_id } => {
                    if instance_id == self.active_instance_id {
                        self.connected = false;
                        self.tasks_running = false;
                        self.active_triggers.clear();
                        self.resolution = None;
                    }
                }
                RuntimeEvent::FrameCaptured { instance_id, image } => {
                    if instance_id == self.active_instance_id {
                        let size = [image.width() as usize, image.height() as usize];
                        let color_image = ColorImage::from_rgba_unmultiplied(size, image.as_raw());
                        self.preview_size = Some(size);
                        self.last_frame = Some(image);
                        match &mut self.preview_texture {
                            Some(tex) => tex.set(color_image, TextureOptions::LINEAR),
                            None => {
                                self.preview_texture = Some(ctx.load_texture(
                                    "moca_preview",
                                    color_image,
                                    TextureOptions::LINEAR,
                                ));
                            }
                        }
                    }
                }
                RuntimeEvent::TaskBatchStarted { instance_id } => {
                    if instance_id == self.active_instance_id {
                        self.tasks_running = true;
                    }
                }
                RuntimeEvent::TaskBatchFinished {
                    instance_id,
                    succeeded,
                } => {
                    if instance_id == self.active_instance_id {
                        self.tasks_running = false;
                        let msg = if succeeded {
                            "本轮选中任务执行完毕"
                        } else {
                            "本轮任务结束（含失败/跳过或已中止项）"
                        };
                        self.push_log(
                            if succeeded {
                                LogLevel::Info
                            } else {
                                LogLevel::Warn
                            },
                            msg,
                        );
                    }
                }
                RuntimeEvent::TriggerStateChanged {
                    instance_id,
                    task_id,
                    active,
                } => {
                    if instance_id == self.active_instance_id {
                        let entry = self.active_triggers.entry(task_id).or_insert((false, 0));
                        entry.0 = active;
                        if active {
                            entry.1 = 0;
                        }
                    }
                }
                RuntimeEvent::TriggerActionCount {
                    instance_id,
                    task_id,
                    count,
                } => {
                    if instance_id == self.active_instance_id {
                        let entry = self.active_triggers.entry(task_id).or_insert((true, 0));
                        entry.1 = count;
                    }
                }
                RuntimeEvent::Log { level, message } => {
                    self.push_log(level, message);
                }
            }
        }
    }

    fn selected_device_target(&self) -> Option<DeviceTarget> {
        match self.target_tab {
            TargetTab::Adb => {
                let d = self
                    .selected_adb_idx
                    .and_then(|i| self.adb_devices.get(i))?;
                Some(DeviceTarget::Adb {
                    adb_path: d.adb_path.clone(),
                    address: d.address.clone(),
                    config_json: d.config_json.clone(),
                    display_name: d.name.clone(),
                })
            }
            TargetTab::Win32 => {
                let w = self
                    .selected_win32_idx
                    .and_then(|i| self.win32_windows.get(i))?;
                Some(DeviceTarget::Win32 {
                    hwnd: w.hwnd,
                    window_title: w.title.clone(),
                    class_name: w.class_name.clone(),
                    screencap_method: 0,
                    input_method: 0,
                })
            }
        }
    }
}

impl eframe::App for MocaApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_events(ctx);
        self.drain_hotkeys();
        if !ctx.egui_wants_keyboard_input() && ctx.input(|i| i.key_pressed(egui::Key::F5)) {
            if self.connected {
                self.runtime.send(RuntimeCommand::CaptureFrame {
                    instance_id: self.active_instance_id.clone(),
                });
            } else {
                self.runtime.send(RuntimeCommand::ScanDevices);
            }
        }
        handle_borderless_resize(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if self.icon_sync_remaining > 0 {
            self.icon_sync_remaining -= 1;
            if let Some(hwnd) = self.win32_hwnd {
                moca_maa::apply_window_icon_from_exe(hwnd);
            }
        }
        self.render_title_bar(ui);
        self.render_left_task_panel(ui);
        self.render_middle_inspector_panel(ui);
        self.render_bottom_logs(ui);
        self.render_preview_center(ui);

        let win_rect = ui.ctx().viewport_rect();
        ui.painter().rect_stroke(
            win_rect,
            CornerRadius::ZERO,
            Stroke::new(1.0, self.theme.border_strong),
            egui::StrokeKind::Inside,
        );
    }
}

impl MocaApp {
    fn drain_hotkeys(&mut self) {
        while let Some(action) = self.hotkeys.try_recv() {
            match action {
                GlobalHotkeyAction::EmergencyStop => {
                    self.push_log(LogLevel::Warn, "[热键 F9] 已触发全局紧急急停");
                }
                GlobalHotkeyAction::ToggleWindow => {}
                GlobalHotkeyAction::ToggleTriggers => {
                    self.toggle_all_triggers_via_hotkey();
                }
            }
        }
    }

    fn toggle_all_triggers_via_hotkey(&mut self) {
        let Some(pkg) = self.packages.get(self.selected_game_idx).cloned() else {
            return;
        };
        if !self.connected {
            self.push_log(LogLevel::Warn, "[热键 F8] 请先连接目标窗口后再切换实时辅助");
            return;
        }
        let bundle_dirs = Self::resolve_bundle_dirs(&pkg);
        let active_ids: Vec<String> = self
            .active_triggers
            .iter()
            .filter(|(_, (on, _))| *on)
            .map(|(id, _)| id.clone())
            .collect();

        if !active_ids.is_empty() {
            for tid in active_ids {
                if let Some(task) = pkg.manifest.task.get(&tid).cloned() {
                    let resolved = pkg.manifest.resolve_single_task(&task, &self.option_values);
                    self.runtime.send(RuntimeCommand::SetTriggerActive {
                        instance_id: self.active_instance_id.clone(),
                        task_id: tid,
                        active: false,
                        game_name: pkg.manifest.name.clone(),
                        bundle_dirs: bundle_dirs.clone(),
                        task: Box::new(resolved),
                        mode: self.execution_mode,
                    });
                }
            }
            self.push_log(LogLevel::Info, "[热键 F8] 已暂停实时辅助");
            return;
        }

        let mut target_triggers: Vec<(String, TaskDecl)> = self
            .selected_tasks
            .iter()
            .filter_map(|tid| {
                pkg.manifest
                    .task
                    .get(tid)
                    .filter(|t| {
                        t.kind == TaskKind::Trigger && t.tier == moca_protocol::TaskTier::Free
                    })
                    .map(|t| (tid.clone(), t.clone()))
            })
            .collect();
        if target_triggers.is_empty()
            && let Some((tid, task)) = pkg.manifest.task.iter().find(|(_, t)| {
                t.kind == TaskKind::Trigger && t.tier == moca_protocol::TaskTier::Free
            })
        {
            target_triggers.push((tid.clone(), task.clone()));
        }

        if target_triggers.is_empty() {
            self.push_log(LogLevel::Warn, "[热键 F8] 当前游戏包未配置实时辅助任务");
            return;
        }

        for (tid, task) in target_triggers {
            let label = task.label.clone();
            let resolved = pkg.manifest.resolve_single_task(&task, &self.option_values);
            self.runtime.send(RuntimeCommand::SetTriggerActive {
                instance_id: self.active_instance_id.clone(),
                task_id: tid,
                active: true,
                game_name: pkg.manifest.name.clone(),
                bundle_dirs: bundle_dirs.clone(),
                task: Box::new(resolved),
                mode: self.execution_mode,
            });
            self.push_log(
                LogLevel::Info,
                format!("[热键 F8] 已恢复实时辅助:「{label}」"),
            );
        }
    }

    fn start_enabled_triggers(&mut self) {
        let Some(pkg) = self.packages.get(self.selected_game_idx).cloned() else {
            return;
        };
        let bundle_dirs = Self::resolve_bundle_dirs(&pkg);
        let enabled_triggers: Vec<(String, TaskDecl)> = self
            .selected_tasks
            .iter()
            .filter_map(|tid| {
                pkg.manifest
                    .task
                    .get(tid)
                    .filter(|t| {
                        t.kind == TaskKind::Trigger && t.tier == moca_protocol::TaskTier::Free
                    })
                    .map(|t| (tid.clone(), t.clone()))
            })
            .collect();
        for (tid, task) in enabled_triggers {
            let resolved = pkg.manifest.resolve_single_task(&task, &self.option_values);
            self.push_log(
                LogLevel::Info,
                format!("已自动启用持久化实时辅助:「{}」", task.label),
            );
            self.runtime.send(RuntimeCommand::SetTriggerActive {
                instance_id: self.active_instance_id.clone(),
                task_id: tid,
                active: true,
                game_name: pkg.manifest.name.clone(),
                bundle_dirs: bundle_dirs.clone(),
                task: Box::new(resolved),
                mode: self.execution_mode,
            });
        }
    }

    fn handle_schema_action(&mut self, pkg: &LoadedGamePackage, action: Option<SchemaAction>) {
        let Some(action) = action else {
            return;
        };
        match action {
            SchemaAction::RunBatchTask(task) => {
                self.dispatch_tasks(vec![task]);
            }
            SchemaAction::ToggleTrigger {
                task_id,
                task,
                active,
            } => {
                if !self.connected {
                    if active {
                        self.push_log(
                            LogLevel::Info,
                            format!(
                                "已启用「{}」并保存至配置，连接游戏窗口后将自动运行",
                                task.label
                            ),
                        );
                    }
                } else {
                    let bundle_dirs = Self::resolve_bundle_dirs(pkg);
                    let resolved = pkg.manifest.resolve_single_task(&task, &self.option_values);
                    self.runtime.send(RuntimeCommand::SetTriggerActive {
                        instance_id: self.active_instance_id.clone(),
                        task_id,
                        active,
                        game_name: pkg.manifest.name.clone(),
                        bundle_dirs,
                        task: Box::new(resolved),
                        mode: self.execution_mode,
                    });
                }
            }
        }
    }

    fn hot_reload_active_triggers(
        &mut self,
        pkg: &LoadedGamePackage,
        prev_opts: &HashMap<String, serde_json::Value>,
    ) {
        if !self.connected || &self.option_values == prev_opts {
            return;
        }
        let active_ids: Vec<String> = self
            .active_triggers
            .iter()
            .filter_map(|(id, (on, _))| if *on { Some(id.clone()) } else { None })
            .collect();
        for tid in active_ids {
            if let Some(t) = pkg.manifest.task.get(&tid).cloned()
                && t.option
                    .iter()
                    .any(|opt| self.option_values.get(opt) != prev_opts.get(opt))
            {
                self.push_log(
                    LogLevel::Info,
                    format!("策略选项已更新，正在热重载「{}」规则...", t.label),
                );
                let bundle_dirs = Self::resolve_bundle_dirs(pkg);
                let resolved = pkg.manifest.resolve_single_task(&t, &self.option_values);
                self.runtime.send(RuntimeCommand::SetTriggerActive {
                    instance_id: self.active_instance_id.clone(),
                    task_id: tid,
                    active: true,
                    game_name: pkg.manifest.name.clone(),
                    bundle_dirs,
                    task: Box::new(resolved),
                    mode: self.execution_mode,
                });
            }
        }
    }
    fn render_title_bar(&mut self, ui: &mut egui::Ui) {
        let header_frame = egui::Frame::new()
            .fill(self.theme.bg_header)
            .stroke(Stroke::new(1.0, self.theme.border))
            .inner_margin(Margin::symmetric(14, 9));

        let is_maximized = ui.ctx().input(|i| i.viewport().maximized.unwrap_or(false));

        egui::Panel::top("top_bar")
            .frame(header_frame)
            .show_separator_line(false)
            .show(ui, |ui| {
                let bar_rect = ui.max_rect();
                if self.theme.starlight_decor {
                    ui.painter().line_segment(
                        [
                            egui::pos2(bar_rect.left() + 12.0, bar_rect.bottom() + 8.0),
                            egui::pos2(bar_rect.right() - 12.0, bar_rect.bottom() + 8.0),
                        ],
                        Stroke::new(1.0, self.theme.accent.gamma_multiply(0.28)),
                    );
                }

                let mut interactive_right_x = bar_rect.right();
                let mut open_repo_clicked = false;

                ui.horizontal(|ui| {
                    if let Some(tex) = &self.moca_icon_texture {
                        ui.image((tex.id(), egui::vec2(26.0, 26.0)));
                    }
                    ui.label(
                        egui::RichText::new("Moca")
                            .size(16.5)
                            .strong()
                            .color(self.theme.text_primary),
                    );
                    pill_badge(
                        ui,
                        concat!("v", env!("CARGO_PKG_VERSION")),
                        self.theme.text_secondary,
                        self.theme.bg_card,
                        self.theme.border,
                    );

                    if let Some(ver) = &self.maa_version {
                        pill_badge(
                            ui,
                            &format!("● MaaFramework {ver}"),
                            self.theme.success,
                            self.theme.success_bg,
                            self.theme.success.gamma_multiply(0.35),
                        );
                    } else if let Some(err) = &self.maa_error {
                        pill_badge(
                            ui,
                            &format!("▲ MAA 异常: {err}"),
                            self.theme.error,
                            self.theme.error_bg,
                            self.theme.error.gamma_multiply(0.35),
                        );
                    } else {
                        pill_badge(
                            ui,
                            "MAA 初始化中...",
                            self.theme.text_secondary,
                            self.theme.bg_card,
                            self.theme.border,
                        );
                    }

                    if self.process_elevated {
                        pill_badge_with_icon(
                            ui,
                            Some(UiIcon::Shield),
                            "管理员",
                            self.theme.accent_hover,
                            self.theme.accent_soft_bg,
                            self.theme.accent.gamma_multiply(0.5),
                        );
                    }

                    ui.scope(|ui| {
                        pill_badge(
                            ui,
                            "F8/F9/F10",
                            self.theme.text_muted,
                            self.theme.bg_card,
                            self.theme.border,
                        );
                    })
                    .response
                    .on_hover_text(
                        "全局热键：\n• F8：暂停 / 恢复实时辅助\n• F9：紧急急停全部任务\n• F10：显示 / 最小化 Moca\n窗口内：\n• F5：截取画面 / 刷新窗口",
                    );

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let close_resp = self
                            .theme
                            .window_caption_button(ui, WindowCaptionKind::Close);
                        interactive_right_x = interactive_right_x.min(close_resp.rect.left() - 4.0);
                        if close_resp.clicked() {
                            ui.ctx().send_viewport_cmd(ViewportCommand::Close);
                        }
                        let max_kind = if is_maximized {
                            WindowCaptionKind::Restore
                        } else {
                            WindowCaptionKind::Maximize
                        };
                        let max_resp = self.theme.window_caption_button(ui, max_kind);
                        interactive_right_x = interactive_right_x.min(max_resp.rect.left() - 4.0);
                        if max_resp.clicked() {
                            ui.ctx()
                                .send_viewport_cmd(ViewportCommand::Maximized(!is_maximized));
                        }
                        let min_resp = self
                            .theme
                            .window_caption_button(ui, WindowCaptionKind::Minimize);
                        interactive_right_x = interactive_right_x.min(min_resp.rect.left() - 4.0);
                        if min_resp.clicked() {
                            ui.ctx().send_viewport_cmd(ViewportCommand::Minimized(true));
                        }

                        ui.add_space(10.0);

                        let gh_resp = self
                            .theme
                            .icon_button(
                                ui,
                                Some(UiIcon::GitHub),
                                "GitHub · limitcool/moca",
                                Some(UiIcon::ExternalLink),
                                IconButtonVariant::SoftAccent,
                                26.0,
                                false,
                            )
                            .on_hover_text("在浏览器中打开 Moca GitHub 开源仓库");
                        interactive_right_x = interactive_right_x.min(gh_resp.rect.left() - 6.0);
                        if gh_resp.clicked() {
                            open_repo_clicked = true;
                        }
                    });
                });

                if open_repo_clicked {
                    self.open_repo_url(ui.ctx());
                }

                // Only register window-drag sensing on the non-interactive left/spacer region so
                // it never intercepts clicks on the GitHub link or caption buttons.
                if interactive_right_x > bar_rect.left() + 8.0 {
                    let drag_rect = egui::Rect::from_min_max(
                        bar_rect.min,
                        egui::pos2(interactive_right_x, bar_rect.max.y),
                    );
                    let bar_resp = ui.interact(
                        drag_rect,
                        ui.make_persistent_id("moca_titlebar_drag"),
                        Sense::click_and_drag(),
                    );
                    if bar_resp.drag_started_by(egui::PointerButton::Primary) {
                        ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
                    }
                    if bar_resp.double_clicked() {
                        ui.ctx()
                            .send_viewport_cmd(ViewportCommand::Maximized(!is_maximized));
                    }
                }
            });
    }

    fn render_left_task_panel(&mut self, ui: &mut egui::Ui) {
        let sidebar_frame = egui::Frame::new()
            .fill(self.theme.bg_sidebar)
            .stroke(Stroke::new(1.0, self.theme.border))
            .inner_margin(Margin::same(10));

        egui::Panel::left("left_task_panel")
            .frame(sidebar_frame)
            .show_separator_line(false)
            .default_size(255.0)
            .size_range(235.0..=320.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("left_tasks_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        self.render_game_selector_card(ui);
                        ui.add_space(7.0);
                        self.render_tasks_list_card(ui);
                    });
            });
    }

    fn render_middle_inspector_panel(&mut self, ui: &mut egui::Ui) {
        let mid_frame = egui::Frame::new()
            .fill(self.theme.bg_sidebar)
            .stroke(Stroke::new(1.0, self.theme.border))
            .inner_margin(Margin::same(10));

        egui::Panel::left("middle_inspector_panel")
            .frame(mid_frame)
            .show_separator_line(false)
            .default_size(340.0)
            .size_range(305.0..=440.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("mid_inspector_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        self.render_connection_card(ui);
                        ui.add_space(7.0);
                        self.render_task_inspector_card(ui);
                    });
            });
    }

    fn render_game_selector_card(&mut self, ui: &mut egui::Ui) {
        self.theme.card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                allocate_and_paint_icon(ui, UiIcon::Gamepad, 15.0, self.theme.accent_hover);
                ui.label(
                    egui::RichText::new("游戏包")
                        .size(13.5)
                        .strong()
                        .color(self.theme.text_primary),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if self
                        .theme
                        .icon_button(
                            ui,
                            Some(UiIcon::Refresh),
                            "刷新",
                            None,
                            IconButtonVariant::Secondary,
                            24.0,
                            false,
                        )
                        .clicked()
                    {
                        self.runtime.send(RuntimeCommand::ReloadGamePackages);
                    }
                    if self
                        .theme
                        .icon_button(
                            ui,
                            Some(UiIcon::Plus),
                            "扩展",
                            None,
                            IconButtonVariant::Secondary,
                            24.0,
                            false,
                        )
                        .clicked()
                    {
                        self.push_log(
                            LogLevel::Info,
                            "将符合 interface.json 规范的游戏包放入 games/<id>/ 目录后点击「刷新」即可接入",
                        );
                    }
                });
            });
            ui.add_space(3.0);

            if self.packages.is_empty() {
                ui.label(
                    egui::RichText::new("未发现游戏包 (games/*/interface.json)")
                        .color(self.theme.warn),
                );
                return;
            }

            let mut clicked_idx = None;
            ui.horizontal_wrapped(|ui| {
                let tile_radius = CornerRadius::same(self.theme.subcard_radius.max(9));
                for (idx, pkg) in self.packages.iter().enumerate() {
                    let selected = idx == self.selected_game_idx;
                    let game_id = pkg.manifest.effective_game_id();
                    let tile_size = egui::vec2(46.0, 46.0);
                    let (rect, resp) = ui.allocate_exact_size(tile_size, Sense::click());
                    if resp.clicked() {
                        clicked_idx = Some(idx);
                    }

                    let bg = if selected {
                        self.theme.accent_soft_bg
                    } else {
                        self.theme.bg_subcard
                    };
                    let border = if selected {
                        Stroke::new(2.0, self.theme.accent)
                    } else if resp.hovered() {
                        Stroke::new(1.0, self.theme.border_strong)
                    } else {
                        Stroke::new(1.0, self.theme.border)
                    };

                    ui.painter().rect_filled(rect, tile_radius, bg);
                    if let Some(tex) = self.game_icons.get(game_id) {
                        let inner = rect.shrink(3.0);
                        ui.painter().image(
                            tex.id(),
                            inner,
                            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                            Color32::WHITE,
                        );
                    } else {
                        let monogram: String = pkg.manifest.name.chars().take(2).collect();
                        ui.painter().text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            monogram,
                            egui::FontId::proportional(15.0),
                            if selected {
                                Color32::WHITE
                            } else {
                                self.theme.text_primary
                            },
                        );
                    }
                    ui.painter().rect_stroke(
                        rect,
                        tile_radius,
                        border,
                        egui::StrokeKind::Inside,
                    );
                    resp.on_hover_text(format!("{} ({game_id})", pkg.manifest.name));
                }

                if let Some(pkg) = self.packages.get(self.selected_game_idx) {
                    ui.vertical(|ui| {
                        ui.add_space(2.0);
                        ui.label(
                            egui::RichText::new(&pkg.manifest.name)
                                .size(13.5)
                                .strong()
                                .color(self.theme.text_primary),
                        );
                        if let Some(theme_name) = &self.theme.theme_name {
                            pill_badge_with_icon(
                                ui,
                                Some(UiIcon::Sparkle),
                                &format!("皮肤: {theme_name}"),
                                self.theme.accent_hover,
                                self.theme.accent_soft_bg,
                                self.theme.accent.gamma_multiply(0.45),
                            );
                        } else {
                            ui.label(
                                egui::RichText::new(pkg.manifest.effective_game_id())
                                    .size(11.5)
                                    .color(self.theme.text_muted),
                            );
                        }
                    });
                }
            });

            if let Some(idx) = clicked_idx
                && idx != self.selected_game_idx
            {
                self.selected_game_idx = idx;
                self.apply_profile_for_selected_game(ui.ctx());
            }
        });
    }

    fn render_connection_card(&mut self, ui: &mut egui::Ui) {
        self.theme.card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                allocate_and_paint_icon(ui, UiIcon::Plug, 15.0, self.theme.accent_hover);
                ui.label(
                    egui::RichText::new("设备与窗口接入")
                        .size(14.0)
                        .strong()
                        .color(self.theme.text_primary),
                );
                if !self.matched_win32_indices.is_empty() && self.target_tab == TargetTab::Win32 {
                    pill_badge(
                        ui,
                        "已智能匹配",
                        self.theme.success,
                        self.theme.success_bg,
                        self.theme.success.gamma_multiply(0.35),
                    );
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if self
                        .theme
                        .icon_button(
                            ui,
                            Some(UiIcon::Scan),
                            "扫描设备",
                            None,
                            IconButtonVariant::Secondary,
                            24.0,
                            false,
                        )
                        .clicked()
                    {
                        self.runtime.send(RuntimeCommand::ScanDevices);
                    }
                });
            });

            ui.add_space(4.0);
            if let Some(idx) = self.theme.segmented_row(
                ui,
                &[
                    SegmentedOption {
                        label: "Win32 端游",
                        icon: Some(UiIcon::Desktop),
                        selected: self.target_tab == TargetTab::Win32,
                    },
                    SegmentedOption {
                        label: "ADB 模拟器",
                        icon: Some(UiIcon::Smartphone),
                        selected: self.target_tab == TargetTab::Adb,
                    },
                ],
            ) {
                self.target_tab = if idx == 0 {
                    TargetTab::Win32
                } else {
                    TargetTab::Adb
                };
            }

            ui.add_space(4.0);
            match self.target_tab {
                TargetTab::Adb => {
                    if self.adb_devices.is_empty() {
                        ui.label(
                            egui::RichText::new("暂未检测到运行中的 ADB 模拟器")
                                .size(12.5)
                                .color(self.theme.text_muted),
                        );
                    } else {
                        let label = self
                            .selected_adb_idx
                            .and_then(|i| self.adb_devices.get(i))
                            .map(|d| format!("{} ({})", d.name, d.address))
                            .unwrap_or_else(|| "选择 ADB 设备".into());
                        egui::ComboBox::from_id_salt("adb_select")
                            .width(ui.available_width())
                            .selected_text(label)
                            .show_ui(ui, |ui| {
                                for (idx, dev) in self.adb_devices.iter().enumerate() {
                                    ui.selectable_value(
                                        &mut self.selected_adb_idx,
                                        Some(idx),
                                        format!("{} ({})", dev.name, dev.address),
                                    );
                                }
                            });
                    }
                }
                TargetTab::Win32 => {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.win32_filter)
                            .desired_width(ui.available_width())
                            .hint_text("搜索过滤窗口标题..."),
                    );
                    let filter_lower = self.win32_filter.to_lowercase();
                    render_win32_window_combo(
                        ui,
                        &self.win32_windows,
                        &self.win32_icons,
                        &self.matched_win32_indices,
                        &filter_lower,
                        &mut self.selected_win32_idx,
                    );

                    if let Some(w) = self
                        .selected_win32_idx
                        .and_then(|i| self.win32_windows.get(i))
                        && w.is_elevated == Some(true)
                        && !self.process_elevated
                    {
                        egui::Frame::new()
                            .fill(self.theme.warn_bg)
                            .stroke(Stroke::new(1.0, self.theme.warn.gamma_multiply(0.5)))
                            .corner_radius(CornerRadius::same(6))
                            .inner_margin(Margin::symmetric(10, 6))
                            .show(ui, |ui| {
                                ui.label(
                                    egui::RichText::new(
                                        "⚠ 目标窗口以管理员运行，请以管理员身份重启 Moca",
                                    )
                                    .size(12.0)
                                    .color(self.theme.warn),
                                );
                            });
                    }
                }
            }

            ui.add_space(4.0);
            let mut mode_changed = false;
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("执行模式")
                        .size(12.5)
                        .color(self.theme.text_secondary),
                );
                ui.add_space((ui.available_width() - 216.0).max(8.0));
                if let Some(idx) = self.theme.segmented_row(
                    ui,
                    &[
                        SegmentedOption {
                            label: "后台并行",
                            icon: Some(UiIcon::Layers),
                            selected: self.execution_mode == ExecutionMode::Background,
                        },
                        SegmentedOption {
                            label: "前台独占",
                            icon: Some(UiIcon::Bolt),
                            selected: self.execution_mode == ExecutionMode::ForegroundExclusive,
                        },
                    ],
                ) {
                    let next = if idx == 0 {
                        ExecutionMode::Background
                    } else {
                        ExecutionMode::ForegroundExclusive
                    };
                    if next != self.execution_mode {
                        self.execution_mode = next;
                        mode_changed = true;
                    }
                }
            });
            if mode_changed {
                self.save_current_profile();
                if self.connected
                    && let Some(target) = self.selected_device_target()
                {
                    let mode = self.execution_mode;
                    self.push_log(
                        LogLevel::Info,
                        format!("执行模式已切换为 {mode:?}，正在重新连接目标..."),
                    );
                    self.runtime.send(RuntimeCommand::Disconnect {
                        instance_id: self.active_instance_id.clone(),
                    });
                    self.runtime.send(RuntimeCommand::Connect {
                        instance_id: self.active_instance_id.clone(),
                        target,
                        mode,
                    });
                }
            }

            ui.add_space(6.0);
            if !self.connected {
                if self
                    .theme
                    .icon_button(
                        ui,
                        Some(UiIcon::Plug),
                        "连接并抓取画面",
                        None,
                        IconButtonVariant::Primary,
                        34.0,
                        true,
                    )
                    .clicked()
                    && let Some(target) = self.selected_device_target()
                {
                    self.runtime.send(RuntimeCommand::Connect {
                        instance_id: self.active_instance_id.clone(),
                        target,
                        mode: self.execution_mode,
                    });
                }
            } else {
                ui.columns(2, |cols| {
                    cols[0].vertical_centered_justified(|ui| {
                        if self
                            .theme
                            .icon_button(
                                ui,
                                Some(UiIcon::Monitor),
                                "截取实时画面",
                                None,
                                IconButtonVariant::Primary,
                                34.0,
                                true,
                            )
                            .on_hover_text("截取实时画面 (F5)")
                            .clicked()
                        {
                            self.runtime.send(RuntimeCommand::CaptureFrame {
                                instance_id: self.active_instance_id.clone(),
                            });
                        }
                    });
                    cols[1].vertical_centered_justified(|ui| {
                        if self
                            .theme
                            .icon_button(
                                ui,
                                Some(UiIcon::Unplug),
                                "断开连接",
                                None,
                                IconButtonVariant::Danger,
                                34.0,
                                true,
                            )
                            .clicked()
                        {
                            self.runtime.send(RuntimeCommand::Disconnect {
                                instance_id: self.active_instance_id.clone(),
                            });
                        }
                    });
                });
            }
        });
    }

    fn render_tasks_list_card(&mut self, ui: &mut egui::Ui) {
        self.theme.card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                allocate_and_paint_icon(ui, UiIcon::ListChecks, 15.0, self.theme.accent_hover);
                ui.label(
                    egui::RichText::new("任务队列")
                        .size(13.5)
                        .strong()
                        .color(self.theme.text_primary),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if self.tasks_running {
                        if self
                            .theme
                            .icon_button(
                                ui,
                                Some(UiIcon::Stop),
                                "停止 (F9)",
                                None,
                                IconButtonVariant::Danger,
                                25.0,
                                false,
                            )
                            .clicked()
                        {
                            self.runtime.send(RuntimeCommand::StopTasks {
                                instance_id: self.active_instance_id.clone(),
                            });
                        }
                    } else if self
                        .theme
                        .icon_button(
                            ui,
                            Some(UiIcon::Play),
                            "运行日常",
                            None,
                            IconButtonVariant::Primary,
                            25.0,
                            false,
                        )
                        .clicked()
                        && let Some(pkg) = self.packages.get(self.selected_game_idx).cloned()
                    {
                        let mut batch = Vec::new();
                        for tid in &self.selected_tasks {
                            if let Some(t) = pkg.manifest.task.get(tid)
                                && t.kind == TaskKind::Batch
                            {
                                batch.push(t.clone());
                            }
                        }
                        self.dispatch_tasks(batch);
                    }
                });
            });

            if let Some(pkg) = self.packages.get(self.selected_game_idx).cloned() {
                let prev_tasks = self.selected_tasks.clone();
                let action = render_task_list(
                    ui,
                    &self.theme,
                    &pkg.manifest,
                    &mut self.selected_tasks,
                    &mut self.focused_task_id,
                    &self.active_triggers,
                    self.connected,
                );
                self.handle_schema_action(&pkg, action);
                if self.selected_tasks != prev_tasks {
                    self.save_current_profile();
                }
            } else {
                ui.label(egui::RichText::new("请先加载一个游戏包").color(self.theme.text_muted));
            }
        });
    }

    fn render_task_inspector_card(&mut self, ui: &mut egui::Ui) {
        self.theme.card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                allocate_and_paint_icon(ui, UiIcon::Sliders, 15.0, self.theme.accent_hover);
                ui.label(
                    egui::RichText::new("任务配置详情")
                        .size(13.5)
                        .strong()
                        .color(self.theme.text_primary),
                );
            });
            ui.add_space(4.0);

            if let Some(pkg) = self.packages.get(self.selected_game_idx).cloned() {
                let prev_tasks = self.selected_tasks.clone();
                let prev_opts = self.option_values.clone();
                let action = render_task_inspector(
                    ui,
                    &self.theme,
                    &pkg.manifest,
                    &mut self.selected_tasks,
                    self.focused_task_id.as_deref(),
                    &mut self.option_values,
                    &self.active_triggers,
                    self.connected,
                );
                self.handle_schema_action(&pkg, action);
                self.hot_reload_active_triggers(&pkg, &prev_opts);
                if self.selected_tasks != prev_tasks || self.option_values != prev_opts {
                    self.save_current_profile();
                }
            } else {
                ui.label(egui::RichText::new("未选中游戏包").color(self.theme.text_muted));
            }
        });
    }

    fn render_bottom_logs(&mut self, ui: &mut egui::Ui) {
        let log_frame = egui::Frame::new()
            .fill(self.theme.bg_console)
            .stroke(Stroke::new(1.0, self.theme.border))
            .inner_margin(Margin::symmetric(14, 10));

        egui::Panel::bottom("log_panel")
            .frame(log_frame)
            .show_separator_line(false)
            .default_size(155.0)
            .size_range(95.0..=320.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    allocate_and_paint_icon(ui, UiIcon::Terminal, 14.5, self.theme.accent_hover);
                    ui.label(
                        egui::RichText::new("运行日志控制台")
                            .size(13.0)
                            .strong()
                            .color(self.theme.text_primary),
                    );
                    pill_badge(
                        ui,
                        &format!("{} 条", self.logs.len()),
                        self.theme.text_muted,
                        self.theme.bg_card,
                        self.theme.border,
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if self
                            .theme
                            .icon_button(
                                ui,
                                Some(UiIcon::Trash),
                                "清空",
                                None,
                                IconButtonVariant::Secondary,
                                24.0,
                                false,
                            )
                            .clicked()
                        {
                            self.logs.clear();
                        }
                    });
                });
                ui.add_space(4.0);
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        for entry in &self.logs {
                            let (tag, tag_color, msg_color) = match entry.level {
                                LogLevel::Info => {
                                    ("[INFO]", self.theme.accent_hover, self.theme.text_primary)
                                }
                                LogLevel::Warn => ("[WARN]", self.theme.warn, self.theme.warn),
                                LogLevel::Error => ("[ERR ]", self.theme.error, self.theme.error),
                            };
                            ui.horizontal_wrapped(|ui| {
                                ui.label(
                                    egui::RichText::new(&entry.time)
                                        .monospace()
                                        .size(12.0)
                                        .color(self.theme.text_muted),
                                );
                                ui.label(
                                    egui::RichText::new(tag)
                                        .monospace()
                                        .size(12.0)
                                        .color(tag_color),
                                );
                                ui.label(
                                    egui::RichText::new(&entry.message)
                                        .size(12.5)
                                        .color(msg_color),
                                );
                            });
                        }
                    });
            });
    }

    fn render_preview_center(&mut self, ui: &mut egui::Ui) {
        let center_frame = egui::Frame::new()
            .fill(self.theme.bg_app)
            .inner_margin(Margin::same(14));

        egui::CentralPanel::default()
            .frame(center_frame)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    allocate_and_paint_icon(ui, UiIcon::Monitor, 16.0, self.theme.accent_hover);
                    ui.label(
                        egui::RichText::new("实时画面 · CV 识别调试画布")
                            .size(15.0)
                            .strong()
                            .color(self.theme.text_primary),
                    );
                    if self.connected {
                        pill_badge(
                            ui,
                            "● 已连接",
                            self.theme.success,
                            self.theme.success_bg,
                            self.theme.success.gamma_multiply(0.35),
                        );
                    } else {
                        pill_badge(
                            ui,
                            "○ 未连接",
                            self.theme.text_muted,
                            self.theme.bg_card,
                            self.theme.border,
                        );
                    }
                    if let Some((w, h)) = self.resolution {
                        pill_badge(
                            ui,
                            &format!("控制器 {w}×{h}"),
                            self.theme.text_secondary,
                            self.theme.bg_card,
                            self.theme.border,
                        );
                    }
                    if let Some([pw, ph]) = self.preview_size {
                        pill_badge(
                            ui,
                            &format!("帧 {pw}×{ph}"),
                            self.theme.text_secondary,
                            self.theme.bg_card,
                            self.theme.border,
                        );
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if self
                            .theme
                            .chip_toggle_button(
                                ui,
                                Some(UiIcon::Crop),
                                self.roi_tool.enabled,
                                "ROI 采图框选",
                            )
                            .clicked()
                        {
                            self.roi_tool.enabled = !self.roi_tool.enabled;
                        }
                    });
                });

                if self.roi_tool.enabled {
                    ui.add_space(6.0);
                    egui::Frame::new()
                        .fill(self.theme.bg_card)
                        .stroke(Stroke::new(1.0, self.theme.border))
                        .corner_radius(CornerRadius::same(self.theme.subcard_radius))
                        .inner_margin(Margin::symmetric(10, 6))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let roi = self.preview_size.and_then(|[w, h]| {
                                    self.roi_tool.selected_pixel_rect(w as u32, h as u32)
                                });
                                match roi {
                                    Some([x, y, w, h]) => {
                                        ui.label(
                                            egui::RichText::new(format!("ROI: [{x}, {y}, {w}, {h}]"))
                                                .monospace()
                                                .color(Color32::from_rgb(56, 212, 248)),
                                        );
                                        if ui.small_button("复制坐标").clicked() {
                                            let text = format!("[{x}, {y}, {w}, {h}]");
                                            ui.ctx().copy_text(text.clone());
                                            self.push_log(
                                                LogLevel::Info,
                                                format!("已复制 ROI 坐标: {text}"),
                                            );
                                        }
                                    }
                                    None => {
                                        ui.label(
                                            egui::RichText::new("在下方画面中按住左键拖拽框选按钮区域")
                                                .size(12.5)
                                                .color(self.theme.text_secondary),
                                        );
                                    }
                                }

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui.small_button("保存模板 PNG").clicked()
                                            && let (Some(frame), Some(r), Some(pkg)) = (
                                                self.last_frame.as_ref(),
                                                roi,
                                                self.packages.get(self.selected_game_idx),
                                            )
                                        {
                                            match save_cropped_template(
                                                frame,
                                                r,
                                                &pkg.root_dir,
                                                &self.roi_tool.template_name,
                                            ) {
                                                Ok(path) => self.push_log(
                                                    LogLevel::Info,
                                                    format!("模板已保存: {}", path.display()),
                                                ),
                                                Err(err) => self.push_log(LogLevel::Error, err),
                                            }
                                        }
                                        ui.add(
                                            egui::TextEdit::singleline(
                                                &mut self.roi_tool.template_name,
                                            )
                                            .desired_width(130.0)
                                            .hint_text("模板名称"),
                                        );
                                    },
                                );
                            });
                        });
                }

                ui.add_space(8.0);

                let canvas_frame = egui::Frame::new()
                    .fill(self.theme.bg_console)
                    .stroke(Stroke::new(1.0, self.theme.border))
                    .corner_radius(CornerRadius::same(self.theme.card_radius))
                    .inner_margin(Margin::same(12));

                canvas_frame.show(ui, |ui| {
                    ui.set_min_size(ui.available_size());
                    if let Some(tex) = &self.preview_texture {
                        let avail = ui.available_size();
                        let tex_size = tex.size_vec2();
                        let scale = (avail.x / tex_size.x)
                            .min(avail.y / tex_size.y)
                            .clamp(0.1, 1.0);
                        let draw_size = tex_size * scale;
                        let img_resp = ui
                            .centered_and_justified(|ui| ui.image((tex.id(), draw_size)))
                            .inner;
                        self.roi_tool.interact_and_paint(
                            ui,
                            img_resp.rect,
                            tex_size.x as u32,
                            tex_size.y as u32,
                        );
                    } else {
                        let canvas_rect = ui.available_rect_before_wrap();
                        if let Some(pkg) = self.packages.get(self.selected_game_idx)
                            && let Some(banner_tex) =
                                self.banner_textures.get(pkg.manifest.effective_game_id())
                        {
                            ui.painter().image(
                                banner_tex.id(),
                                canvas_rect,
                                egui::Rect::from_min_max(
                                    egui::pos2(0.0, 0.0),
                                    egui::pos2(1.0, 1.0),
                                ),
                                Color32::from_white_alpha(38),
                            );
                        }
                        self.theme
                            .paint_starlight_canvas_decor(ui.painter(), canvas_rect);

                        let avail = ui.available_size();
                        let card_w = 420.0_f32.min(avail.x - 24.0).max(240.0);
                        let top_pad = ((avail.y - 220.0) * 0.5).max(16.0);
                        ui.add_space(top_pad);
                        ui.horizontal(|ui| {
                            let side_pad = ((avail.x - card_w) * 0.5).max(8.0);
                            ui.add_space(side_pad);
                            ui.allocate_ui(egui::vec2(card_w, 220.0), |ui| {
                                self.theme.card_frame().show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.vertical_centered(|ui| {
                                        ui.label(
                                            egui::RichText::new("等待画面接入")
                                                .size(18.0)
                                                .strong()
                                                .color(self.theme.text_primary),
                                        );
                                        ui.add_space(8.0);
                                        ui.label(
                                            egui::RichText::new(
                                                "1. 在左侧选择游戏包（已预置《蓝色星原：旅谣》）\n2. 选择 Win32 端游窗口或 ADB 模拟器\n3. 点击「连接并抓取画面」即可运行任务或 ROI 采图",
                                            )
                                            .size(13.0)
                                            .color(self.theme.text_secondary),
                                        );
                                        ui.add_space(12.0);
                                        if self
                                            .theme
                                            .icon_button(
                                                ui,
                                                Some(UiIcon::Scan),
                                                "重新扫描设备与窗口",
                                                None,
                                                IconButtonVariant::Primary,
                                                32.0,
                                                false,
                                            )
                                            .clicked()
                                        {
                                            self.runtime.send(RuntimeCommand::ScanDevices);
                                        }
                                        ui.add_space(10.0);
                                        if self
                                            .theme
                                            .icon_button(
                                                ui,
                                                Some(UiIcon::GitHub),
                                                "开源仓库 · github.com/limitcool/moca",
                                                Some(UiIcon::ExternalLink),
                                                IconButtonVariant::SoftAccent,
                                                28.0,
                                                false,
                                            )
                                            .on_hover_text("点击在浏览器中打开 Moca GitHub 仓库")
                                            .clicked()
                                        {
                                            self.open_repo_url(ui.ctx());
                                        }
                                    });
                                });
                            });
                        });
                    }
                });
            });
    }
}

/// Match a window title against a `title_regex`-style pattern (supports `|` alternation and `.*` wildcards).
fn load_embedded_moca_icon(ctx: &egui::Context) -> Option<TextureHandle> {
    let bytes = include_bytes!("../../../assets/moca_icon.png");
    let img = image::load_from_memory(bytes).ok()?.to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    let color_image = ColorImage::from_rgba_unmultiplied(size, img.as_raw());
    Some(ctx.load_texture("moca_brand_icon", color_image, TextureOptions::LINEAR))
}

fn render_win32_window_combo(
    ui: &mut egui::Ui,
    win32_windows: &[DiscoveredWin32],
    win32_icons: &[Option<TextureHandle>],
    matched_indices: &[usize],
    filter_lower: &str,
    selected_idx: &mut Option<usize>,
) {
    let selected_icon = selected_idx
        .and_then(|i| win32_icons.get(i))
        .and_then(|opt| opt.as_ref());
    let selected_label = selected_idx
        .and_then(|i| win32_windows.get(i))
        .map(|w| {
            let size_str = w
                .client_size
                .map(|(cw, ch)| format!(" ({cw}x{ch})"))
                .unwrap_or_default();
            format!("{} [{}]{size_str}", w.title, w.class_name)
        })
        .unwrap_or_else(|| "选择目标游戏窗口".into());

    ui.horizontal(|ui| {
        if let Some(tex) = selected_icon {
            ui.image((tex.id(), egui::vec2(18.0, 18.0)));
        }
        egui::ComboBox::from_id_salt("win32_select")
            .width(ui.available_width().max(140.0))
            .truncate()
            .selected_text(selected_label)
            .show_ui(ui, |ui| {
                let mut ordered: Vec<usize> = matched_indices.to_vec();
                for i in 0..win32_windows.len() {
                    if !ordered.contains(&i) {
                        ordered.push(i);
                    }
                }
                for idx in ordered {
                    let win = &win32_windows[idx];
                    if !filter_lower.is_empty()
                        && !win.title.to_lowercase().contains(filter_lower)
                        && !win.class_name.to_lowercase().contains(filter_lower)
                    {
                        continue;
                    }
                    let size_str = win
                        .client_size
                        .map(|(w, h)| format!("{w}x{h}"))
                        .unwrap_or_default();
                    let match_tag = if matched_indices.contains(&idx) {
                        "[匹配] "
                    } else {
                        ""
                    };
                    let priv_tag = if win.is_elevated == Some(true) {
                        " [管理员]"
                    } else {
                        ""
                    };
                    ui.horizontal(|ui| {
                        if let Some(Some(tex)) = win32_icons.get(idx) {
                            ui.image((tex.id(), egui::vec2(16.0, 16.0)));
                        } else {
                            ui.add_space(16.0);
                        }
                        if ui
                            .selectable_label(
                                *selected_idx == Some(idx),
                                format!(
                                    "{match_tag}{} [{}] ({size_str}){priv_tag}",
                                    win.title, win.class_name
                                ),
                            )
                            .clicked()
                        {
                            *selected_idx = Some(idx);
                            ui.close();
                        }
                    });
                }
            });
    });
}

fn matches_title_pattern(pattern: &str, title: &str) -> bool {
    let title_lower = title.to_lowercase();
    pattern.split('|').any(|alt| {
        let alt = alt.trim().to_lowercase();
        if alt.is_empty() {
            return false;
        }
        let mut rest = title_lower.as_str();
        for part in alt.split(".*") {
            if part.is_empty() {
                continue;
            }
            if let Some(pos) = rest.find(part) {
                rest = &rest[pos + part.len()..];
            } else {
                return false;
            }
        }
        true
    })
}

#[cfg(test)]
mod tests {
    use super::matches_title_pattern;

    #[test]
    fn matches_azur_promilia_title_variants() {
        let pat = "蓝色星原|Azur.*Promilia";
        assert!(matches_title_pattern(pat, "蓝色星原：旅谣"));
        assert!(matches_title_pattern(pat, "Azur Promilia Client"));
        assert!(!matches_title_pattern(pat, "Moca - 多游戏自动化通用底座"));
    }
}
