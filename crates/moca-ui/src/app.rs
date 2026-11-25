use std::collections::HashMap;
use std::path::PathBuf;

use egui::{
    Color32, ColorImage, CornerRadius, Margin, Sense, Stroke, TextureHandle, TextureOptions,
    ViewportCommand,
};
use image::RgbaImage;
use moca_cloud::CloudClient;
use moca_maa::{DiscoveredAdb, DiscoveredWin32, is_current_process_elevated};
use moca_protocol::{
    ControllerKind, DeviceTarget, ExecutionMode, SavedGameProfile, TaskDecl, UserTier, VipSession,
};
use moca_runtime::worker::LogLevel;
use moca_runtime::{LoadedGamePackage, RuntimeCommand, RuntimeEvent, RuntimeHandle};

use crate::fonts::configure_cjk_fonts;
use crate::roi_tool::{RoiToolState, save_cropped_template};
use crate::schema_ui::{SchemaAction, render_game_schema};
use crate::theme::{
    Palette, WindowCaptionKind, apply_moca_theme, card_frame, format_local_hms,
    handle_borderless_resize, paint_moca_logo, pill_badge, primary_button, segment_button,
    window_caption_button,
};

const PROFILES_DIR: &str = "profiles";

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
    maa_version: Option<String>,
    maa_error: Option<String>,
    process_elevated: bool,

    packages: Vec<LoadedGamePackage>,
    game_icons: HashMap<String, TextureHandle>,
    selected_game_idx: usize,
    selected_tasks: Vec<String>,
    option_values: HashMap<String, serde_json::Value>,

    target_tab: TargetTab,
    adb_devices: Vec<DiscoveredAdb>,
    selected_adb_idx: Option<usize>,
    win32_windows: Vec<DiscoveredWin32>,
    selected_win32_idx: Option<usize>,
    matched_win32_indices: Vec<usize>,
    win32_filter: String,
    execution_mode: ExecutionMode,

    active_instance_id: String,
    connected: bool,
    tasks_running: bool,
    resolution: Option<(i32, i32)>,

    last_frame: Option<RgbaImage>,
    preview_texture: Option<TextureHandle>,
    preview_size: Option<[usize; 2]>,
    roi_tool: RoiToolState,
    logs: Vec<LogEntry>,
}

impl MocaApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        configure_cjk_fonts(&cc.egui_ctx);
        apply_moca_theme(&cc.egui_ctx);

        let ctx_clone = cc.egui_ctx.clone();
        let runtime = RuntimeHandle::spawn(Some(Box::new(move || {
            ctx_clone.request_repaint();
        })));

        runtime.send(RuntimeCommand::Initialize);
        runtime.send(RuntimeCommand::ReloadGamePackages);

        Self {
            runtime,
            maa_version: None,
            maa_error: None,
            process_elevated: is_current_process_elevated(),
            packages: Vec::new(),
            game_icons: HashMap::new(),
            selected_game_idx: 0,
            selected_tasks: Vec::new(),
            option_values: HashMap::new(),
            target_tab: TargetTab::Win32,
            adb_devices: Vec::new(),
            selected_adb_idx: None,
            win32_windows: Vec::new(),
            selected_win32_idx: None,
            matched_win32_indices: Vec::new(),
            win32_filter: String::new(),
            execution_mode: ExecutionMode::ForegroundExclusive,
            active_instance_id: "default-session".to_string(),
            connected: false,
            tasks_running: false,
            resolution: None,
            last_frame: None,
            preview_texture: None,
            preview_size: None,
            roi_tool: RoiToolState::new(),
            logs: vec![LogEntry {
                time: format_local_hms(),
                level: LogLevel::Info,
                message: "Moca 底座已启动，正在初始化 MaaFramework 与游戏包...".into(),
            }],
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
        for pkg in &self.packages {
            let Some(icon_rel) = &pkg.manifest.icon else {
                continue;
            };
            let icon_path = pkg.root_dir.join(icon_rel);
            let Ok(bytes) = std::fs::read(&icon_path) else {
                continue;
            };
            let Ok(img) = image::load_from_memory(&bytes) else {
                continue;
            };
            let rgba = img.to_rgba8();
            let size = [rgba.width() as usize, rgba.height() as usize];
            let color_image = ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
            let game_id = pkg.manifest.effective_game_id().to_string();
            let tex = ctx.load_texture(
                format!("game_icon_{game_id}"),
                color_image,
                TextureOptions::LINEAR,
            );
            self.game_icons.insert(game_id, tex);
        }
    }

    fn apply_profile_for_selected_game(&mut self) {
        let Some(pkg) = self.packages.get(self.selected_game_idx) else {
            return;
        };
        let game_id = pkg.manifest.effective_game_id().to_string();
        match SavedGameProfile::load(PROFILES_DIR, &game_id) {
            Ok(Some(profile)) => {
                self.selected_tasks = profile.selected_tasks;
                self.option_values = profile.option_values;
                self.execution_mode = profile.execution_mode;
            }
            Ok(None) => {
                // Default to selecting free routine tasks on first load.
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
            }
            Err(err) => {
                self.push_log(LogLevel::Warn, format!("读取配置失败 ({game_id}): {err}"));
            }
        }
        self.recompute_win32_matches();
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
        if let Err(err) = profile.save(PROFILES_DIR) {
            self.push_log(LogLevel::Warn, format!("保存配置失败: {err}"));
        }
    }

    fn recompute_win32_matches(&mut self) {
        self.matched_win32_indices.clear();
        let Some(pkg) = self.packages.get(self.selected_game_idx) else {
            return;
        };
        let pattern = pkg
            .manifest
            .controller
            .values()
            .find(|c| c.kind == ControllerKind::Win32)
            .and_then(|c| c.config.title_regex.as_deref())
            .filter(|p| !p.trim().is_empty() && *p != ".*");

        if let Some(pat) = pattern {
            for (idx, win) in self.win32_windows.iter().enumerate() {
                if win.title.starts_with("Moca") {
                    continue;
                }
                if matches_title_pattern(pat, &win.title) {
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
        self.runtime.send(RuntimeCommand::RunTasks {
            instance_id: self.active_instance_id.clone(),
            game_name: pkg.manifest.name.clone(),
            bundle_dirs,
            tasks,
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
                    self.apply_profile_for_selected_game();
                }
                RuntimeEvent::DevicesDiscovered { adb, win32 } => {
                    self.adb_devices = adb;
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
                        self.runtime.send(RuntimeCommand::CaptureFrame { instance_id });
                    }
                }
                RuntimeEvent::Disconnected { instance_id } => {
                    if instance_id == self.active_instance_id {
                        self.connected = false;
                        self.tasks_running = false;
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
                RuntimeEvent::Log { level, message } => {
                    self.push_log(level, message);
                }
            }
        }
    }

    fn selected_device_target(&self) -> Option<DeviceTarget> {
        match self.target_tab {
            TargetTab::Adb => {
                let d = self.selected_adb_idx.and_then(|i| self.adb_devices.get(i))?;
                Some(DeviceTarget::Adb {
                    adb_path: d.adb_path.clone(),
                    address: d.address.clone(),
                    config_json: d.config_json.clone(),
                    display_name: d.name.clone(),
                })
            }
            TargetTab::Win32 => {
                let w = self.selected_win32_idx.and_then(|i| self.win32_windows.get(i))?;
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
        handle_borderless_resize(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.render_title_bar(ui);
        self.render_left_sidebar(ui);
        self.render_bottom_logs(ui);
        self.render_preview_center(ui);

        let win_rect = ui.ctx().viewport_rect();
        ui.painter().rect_stroke(
            win_rect,
            CornerRadius::ZERO,
            Stroke::new(1.0, Palette::BORDER_STRONG),
            egui::StrokeKind::Inside,
        );
    }
}

impl MocaApp {
    fn render_title_bar(&mut self, ui: &mut egui::Ui) {
        let header_frame = egui::Frame::new()
            .fill(Palette::BG_HEADER)
            .stroke(Stroke::new(1.0, Palette::BORDER))
            .inner_margin(Margin::symmetric(14, 9));

        let is_maximized = ui.ctx().input(|i| i.viewport().maximized.unwrap_or(false));

        egui::Panel::top("top_bar")
            .frame(header_frame)
            .show_separator_line(false)
            .show(ui, |ui| {
                let bar_resp = ui.interact(
                    ui.max_rect(),
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

                ui.horizontal(|ui| {
                    paint_moca_logo(ui, 26.0);
                    ui.label(
                        egui::RichText::new("Moca")
                            .size(16.5)
                            .strong()
                            .color(Palette::TEXT_PRIMARY),
                    );
                    pill_badge(
                        ui,
                        "v0.1",
                        Palette::TEXT_SECONDARY,
                        Palette::BG_CARD,
                        Palette::BORDER,
                    );

                    if let Some(ver) = &self.maa_version {
                        pill_badge(
                            ui,
                            &format!("● MaaFramework {ver}"),
                            Palette::SUCCESS,
                            Palette::SUCCESS_BG,
                            Palette::SUCCESS.gamma_multiply(0.35),
                        );
                    } else if let Some(err) = &self.maa_error {
                        pill_badge(
                            ui,
                            &format!("▲ MAA 异常: {err}"),
                            Palette::ERROR,
                            Palette::ERROR_BG,
                            Palette::ERROR.gamma_multiply(0.35),
                        );
                    } else {
                        pill_badge(
                            ui,
                            "MAA 初始化中...",
                            Palette::TEXT_SECONDARY,
                            Palette::BG_CARD,
                            Palette::BORDER,
                        );
                    }

                    if self.process_elevated {
                        pill_badge(
                            ui,
                            "管理员",
                            Palette::ACCENT_HOVER,
                            Palette::ACCENT_SOFT_BG,
                            Palette::ACCENT.gamma_multiply(0.5),
                        );
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if window_caption_button(ui, WindowCaptionKind::Close).clicked() {
                            ui.ctx().send_viewport_cmd(ViewportCommand::Close);
                        }
                        let max_kind = if is_maximized {
                            WindowCaptionKind::Restore
                        } else {
                            WindowCaptionKind::Maximize
                        };
                        if window_caption_button(ui, max_kind).clicked() {
                            ui.ctx()
                                .send_viewport_cmd(ViewportCommand::Maximized(!is_maximized));
                        }
                        if window_caption_button(ui, WindowCaptionKind::Minimize).clicked() {
                            ui.ctx().send_viewport_cmd(ViewportCommand::Minimized(true));
                        }

                        ui.add_space(8.0);

                        let is_vip = matches!(
                            self.runtime.cloud().session().map(|s| s.tier),
                            Some(UserTier::Vip { .. })
                        );
                        if is_vip {
                            let btn = egui::Button::new(
                                egui::RichText::new("Moca Cloud VIP · 已激活")
                                    .size(12.0)
                                    .strong()
                                    .color(Palette::VIP_GOLD),
                            )
                            .fill(Palette::VIP_BG)
                            .stroke(Stroke::new(1.0, Palette::VIP_BORDER))
                            .corner_radius(CornerRadius::same(99));
                            if ui.add(btn).clicked() {
                                self.runtime.cloud().set_session(None);
                                self.push_log(LogLevel::Info, "已切换为免费开源状态");
                            }
                        } else {
                            let btn = egui::Button::new(
                                egui::RichText::new("Moca Cloud VIP")
                                    .size(12.0)
                                    .color(Palette::VIP_GOLD),
                            )
                            .fill(Palette::BG_CARD)
                            .stroke(Stroke::new(1.0, Palette::VIP_BORDER))
                            .corner_radius(CornerRadius::same(99));
                            if ui.add(btn).clicked() {
                                self.runtime.cloud().set_session(Some(VipSession {
                                    account_id: "demo-vip".into(),
                                    access_token: "demo-token".into(),
                                    tier: UserTier::Vip {
                                        expires_at_unix: 1_999_999_999,
                                    },
                                }));
                                self.push_log(LogLevel::Info, "已模拟登录 Moca Cloud VIP 会话");
                            }
                        }
                    });
                });
            });
    }

    fn render_left_sidebar(&mut self, ui: &mut egui::Ui) {
        let sidebar_frame = egui::Frame::new()
            .fill(Palette::BG_SIDEBAR)
            .stroke(Stroke::new(1.0, Palette::BORDER))
            .inner_margin(Margin::same(12));

        egui::Panel::left("left_sidebar")
            .frame(sidebar_frame)
            .show_separator_line(false)
            .default_size(400.0)
            .size_range(340.0..=520.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        self.render_game_selector_card(ui);
                        ui.add_space(10.0);
                        self.render_connection_card(ui);
                        ui.add_space(10.0);
                        self.render_tasks_card(ui);
                    });
            });
    }

    fn render_game_selector_card(&mut self, ui: &mut egui::Ui) {
        card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("游戏包选择")
                        .size(14.0)
                        .strong()
                        .color(Palette::TEXT_PRIMARY),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("刷新包").clicked() {
                        self.runtime.send(RuntimeCommand::ReloadGamePackages);
                    }
                });
            });
            ui.add_space(4.0);

            if self.packages.is_empty() {
                ui.label(
                    egui::RichText::new("未发现游戏包 (games/*/interface.json)")
                        .color(Palette::WARN),
                );
                return;
            }

            let mut clicked_idx = None;
            ui.horizontal_wrapped(|ui| {
                for (idx, pkg) in self.packages.iter().enumerate() {
                    let selected = idx == self.selected_game_idx;
                    let game_id = pkg.manifest.effective_game_id();
                    let tile_size = egui::vec2(64.0, 64.0);
                    let (rect, resp) = ui.allocate_exact_size(tile_size, Sense::click());
                    if resp.clicked() {
                        clicked_idx = Some(idx);
                    }

                    let bg = if selected {
                        Palette::ACCENT_SOFT_BG
                    } else {
                        Palette::BG_SUBCARD
                    };
                    let border = if selected {
                        Stroke::new(2.0, Palette::ACCENT)
                    } else if resp.hovered() {
                        Stroke::new(1.0, Palette::BORDER_STRONG)
                    } else {
                        Stroke::new(1.0, Palette::BORDER)
                    };

                    ui.painter().rect_filled(rect, CornerRadius::same(10), bg);
                    if let Some(tex) = self.game_icons.get(game_id) {
                        let inner = rect.shrink(4.0);
                        ui.painter().image(
                            tex.id(),
                            inner,
                            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                            Color32::WHITE,
                        );
                    } else {
                        let monogram: String = pkg.manifest.name.chars().take(2).collect();
                        ui.painter().text(
                            rect.center() - egui::vec2(0.0, 6.0),
                            egui::Align2::CENTER_CENTER,
                            monogram,
                            egui::FontId::proportional(18.0),
                            if selected {
                                Color32::WHITE
                            } else {
                                Palette::TEXT_PRIMARY
                            },
                        );
                        ui.painter().text(
                            rect.center() + egui::vec2(0.0, 16.0),
                            egui::Align2::CENTER_CENTER,
                            game_id,
                            egui::FontId::proportional(9.5),
                            Palette::TEXT_MUTED,
                        );
                    }
                    ui.painter().rect_stroke(
                        rect,
                        CornerRadius::same(10),
                        border,
                        egui::StrokeKind::Inside,
                    );
                }

                let (add_rect, add_resp) =
                    ui.allocate_exact_size(egui::vec2(64.0, 64.0), Sense::click());
                let stroke = if add_resp.hovered() {
                    Stroke::new(1.0, Palette::BORDER_STRONG)
                } else {
                    Stroke::new(1.0, Palette::BORDER)
                };
                ui.painter()
                    .rect_filled(add_rect, CornerRadius::same(10), Palette::BG_INPUT);
                ui.painter().rect_stroke(
                    add_rect,
                    CornerRadius::same(10),
                    stroke,
                    egui::StrokeKind::Inside,
                );
                ui.painter().text(
                    add_rect.center() - egui::vec2(0.0, 6.0),
                    egui::Align2::CENTER_CENTER,
                    "+",
                    egui::FontId::proportional(20.0),
                    Palette::TEXT_MUTED,
                );
                ui.painter().text(
                    add_rect.center() + egui::vec2(0.0, 15.0),
                    egui::Align2::CENTER_CENTER,
                    "扩展包",
                    egui::FontId::proportional(10.0),
                    Palette::TEXT_MUTED,
                );
                if add_resp.clicked() {
                    self.push_log(
                        LogLevel::Info,
                        "将符合 interface.json 规范的游戏包放入 games/<id>/ 目录后点击「刷新包」即可接入",
                    );
                }
            });

            if let Some(idx) = clicked_idx
                && idx != self.selected_game_idx
            {
                self.selected_game_idx = idx;
                self.apply_profile_for_selected_game();
            }

            if let Some(pkg) = self.packages.get(self.selected_game_idx) {
                ui.add_space(2.0);
                ui.label(
                    egui::RichText::new(format!(
                        "当前: {} ({})",
                        pkg.manifest.name,
                        pkg.manifest.effective_game_id()
                    ))
                    .size(12.5)
                    .color(Palette::TEXT_SECONDARY),
                );
            }
        });
    }

    fn render_connection_card(&mut self, ui: &mut egui::Ui) {
        card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("设备与窗口接入")
                        .size(14.0)
                        .strong()
                        .color(Palette::TEXT_PRIMARY),
                );
                if !self.matched_win32_indices.is_empty() && self.target_tab == TargetTab::Win32 {
                    pill_badge(
                        ui,
                        "已智能匹配",
                        Palette::SUCCESS,
                        Palette::SUCCESS_BG,
                        Palette::SUCCESS.gamma_multiply(0.35),
                    );
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("扫描设备").clicked() {
                        self.runtime.send(RuntimeCommand::ScanDevices);
                    }
                });
            });

            ui.add_space(4.0);
            ui.columns(2, |cols| {
                cols[0].vertical_centered_justified(|ui| {
                    if segment_button(ui, self.target_tab == TargetTab::Win32, "Win32 端游")
                        .clicked()
                    {
                        self.target_tab = TargetTab::Win32;
                    }
                });
                cols[1].vertical_centered_justified(|ui| {
                    if segment_button(ui, self.target_tab == TargetTab::Adb, "ADB 模拟器")
                        .clicked()
                    {
                        self.target_tab = TargetTab::Adb;
                    }
                });
            });

            ui.add_space(4.0);
            match self.target_tab {
                TargetTab::Adb => {
                    if self.adb_devices.is_empty() {
                        ui.label(
                            egui::RichText::new("暂未检测到运行中的 ADB 模拟器")
                                .size(12.5)
                                .color(Palette::TEXT_MUTED),
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
                    let selected_label = self
                        .selected_win32_idx
                        .and_then(|i| self.win32_windows.get(i))
                        .map(|w| w.title.clone())
                        .unwrap_or_else(|| "选择目标游戏窗口".into());
                    egui::ComboBox::from_id_salt("win32_select")
                        .width(ui.available_width())
                        .selected_text(selected_label)
                        .show_ui(ui, |ui| {
                            // Show matched windows first.
                            let mut ordered: Vec<usize> = self.matched_win32_indices.clone();
                            for i in 0..self.win32_windows.len() {
                                if !ordered.contains(&i) {
                                    ordered.push(i);
                                }
                            }
                            for idx in ordered {
                                let win = &self.win32_windows[idx];
                                if !filter_lower.is_empty()
                                    && !win.title.to_lowercase().contains(&filter_lower)
                                {
                                    continue;
                                }
                                let size_str = win
                                    .client_size
                                    .map(|(w, h)| format!("{w}x{h}"))
                                    .unwrap_or_default();
                                let match_tag = if self.matched_win32_indices.contains(&idx) {
                                    "[匹配] "
                                } else {
                                    ""
                                };
                                let priv_tag = if win.is_elevated == Some(true) {
                                    " [管理员]"
                                } else {
                                    ""
                                };
                                ui.selectable_value(
                                    &mut self.selected_win32_idx,
                                    Some(idx),
                                    format!("{match_tag}{} ({size_str}){priv_tag}", win.title),
                                );
                            }
                        });

                    if let Some(w) = self
                        .selected_win32_idx
                        .and_then(|i| self.win32_windows.get(i))
                        && w.is_elevated == Some(true)
                        && !self.process_elevated
                    {
                        egui::Frame::new()
                            .fill(Palette::WARN_BG)
                            .stroke(Stroke::new(1.0, Palette::WARN.gamma_multiply(0.5)))
                            .corner_radius(CornerRadius::same(6))
                            .inner_margin(Margin::symmetric(10, 6))
                            .show(ui, |ui| {
                                ui.label(
                                    egui::RichText::new(
                                        "⚠ 目标窗口以管理员运行，请以管理员身份重启 Moca",
                                    )
                                    .size(12.0)
                                    .color(Palette::WARN),
                                );
                            });
                    }
                }
            }

            ui.add_space(4.0);
            let mut mode_changed = false;
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("执行模式:")
                        .size(12.5)
                        .color(Palette::TEXT_SECONDARY),
                );
                if segment_button(
                    ui,
                    self.execution_mode == ExecutionMode::Background,
                    "后台并行",
                )
                .clicked()
                    && self.execution_mode != ExecutionMode::Background
                {
                    self.execution_mode = ExecutionMode::Background;
                    mode_changed = true;
                }
                if segment_button(
                    ui,
                    self.execution_mode == ExecutionMode::ForegroundExclusive,
                    "前台独占",
                )
                .clicked()
                    && self.execution_mode != ExecutionMode::ForegroundExclusive
                {
                    self.execution_mode = ExecutionMode::ForegroundExclusive;
                    mode_changed = true;
                }
            });
            if mode_changed {
                self.save_current_profile();
            }

            ui.add_space(6.0);
            if !self.connected {
                if primary_button(ui, "连接并抓取画面", true).clicked()
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
                        if primary_button(ui, "截取实时画面", true).clicked() {
                            self.runtime.send(RuntimeCommand::CaptureFrame {
                                instance_id: self.active_instance_id.clone(),
                            });
                        }
                    });
                    cols[1].vertical_centered_justified(|ui| {
                        let disconnect = egui::Button::new(
                            egui::RichText::new("断开连接").color(Palette::ERROR),
                        )
                        .fill(Palette::ERROR_BG)
                        .stroke(Stroke::new(1.0, Palette::ERROR.gamma_multiply(0.5)))
                        .corner_radius(CornerRadius::same(7))
                        .min_size(egui::vec2(ui.available_width(), 34.0));
                        if ui.add(disconnect).clicked() {
                            self.runtime.send(RuntimeCommand::Disconnect {
                                instance_id: self.active_instance_id.clone(),
                            });
                        }
                    });
                });
            }
        });
    }

    fn render_tasks_card(&mut self, ui: &mut egui::Ui) {
        card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("任务与动态表单")
                        .size(14.0)
                        .strong()
                        .color(Palette::TEXT_PRIMARY),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if self.tasks_running {
                        let stop_btn = egui::Button::new(
                            egui::RichText::new("■ 停止任务")
                                .size(12.0)
                                .strong()
                                .color(Palette::ERROR),
                        )
                        .fill(Palette::ERROR_BG)
                        .stroke(Stroke::new(1.0, Palette::ERROR.gamma_multiply(0.6)))
                        .corner_radius(CornerRadius::same(6));
                        if ui.add(stop_btn).clicked() {
                            self.runtime.send(RuntimeCommand::StopTasks {
                                instance_id: self.active_instance_id.clone(),
                            });
                        }
                    } else {
                        let run_btn = egui::Button::new(
                            egui::RichText::new("▶ 运行选中任务")
                                .size(12.0)
                                .strong()
                                .color(Color32::WHITE),
                        )
                        .fill(Palette::ACCENT)
                        .corner_radius(CornerRadius::same(6));
                        if ui.add(run_btn).clicked()
                            && let Some(pkg) = self.packages.get(self.selected_game_idx).cloned()
                        {
                            let mut batch = Vec::new();
                            for tid in &self.selected_tasks {
                                if let Some(t) = pkg.manifest.task.get(tid) {
                                    batch.push(t.clone());
                                }
                            }
                            self.dispatch_tasks(batch);
                        }
                    }
                });
            });
            ui.add_space(4.0);

            if let Some(pkg) = self.packages.get(self.selected_game_idx).cloned() {
                let prev_tasks = self.selected_tasks.clone();
                let prev_opts = self.option_values.clone();

                if let Some(SchemaAction::RunOrCheckTask(task)) = render_game_schema(
                    ui,
                    &pkg.manifest,
                    &mut self.selected_tasks,
                    &mut self.option_values,
                ) {
                    self.dispatch_tasks(vec![task]);
                }

                if self.selected_tasks != prev_tasks || self.option_values != prev_opts {
                    self.save_current_profile();
                }
            } else {
                ui.label(egui::RichText::new("请先加载一个游戏包").color(Palette::TEXT_MUTED));
            }
        });
    }

    fn render_bottom_logs(&mut self, ui: &mut egui::Ui) {
        let log_frame = egui::Frame::new()
            .fill(Palette::BG_CONSOLE)
            .stroke(Stroke::new(1.0, Palette::BORDER))
            .inner_margin(Margin::symmetric(14, 10));

        egui::Panel::bottom("log_panel")
            .frame(log_frame)
            .show_separator_line(false)
            .default_size(155.0)
            .size_range(95.0..=320.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("运行日志控制台")
                            .size(13.0)
                            .strong()
                            .color(Palette::TEXT_PRIMARY),
                    );
                    pill_badge(
                        ui,
                        &format!("{} 条", self.logs.len()),
                        Palette::TEXT_MUTED,
                        Palette::BG_CARD,
                        Palette::BORDER,
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("清空").clicked() {
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
                                    ("[INFO]", Palette::ACCENT_HOVER, Palette::TEXT_PRIMARY)
                                }
                                LogLevel::Warn => ("[WARN]", Palette::WARN, Palette::WARN),
                                LogLevel::Error => ("[ERR ]", Palette::ERROR, Palette::ERROR),
                            };
                            ui.horizontal_wrapped(|ui| {
                                ui.label(
                                    egui::RichText::new(&entry.time)
                                        .monospace()
                                        .size(12.0)
                                        .color(Palette::TEXT_MUTED),
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
            .fill(Palette::BG_APP)
            .inner_margin(Margin::same(14));

        egui::CentralPanel::default()
            .frame(center_frame)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("实时画面 · CV 识别调试画布")
                            .size(15.0)
                            .strong()
                            .color(Palette::TEXT_PRIMARY),
                    );
                    if self.connected {
                        pill_badge(
                            ui,
                            "● 已连接",
                            Palette::SUCCESS,
                            Palette::SUCCESS_BG,
                            Palette::SUCCESS.gamma_multiply(0.35),
                        );
                    } else {
                        pill_badge(
                            ui,
                            "○ 未连接",
                            Palette::TEXT_MUTED,
                            Palette::BG_CARD,
                            Palette::BORDER,
                        );
                    }
                    if let Some((w, h)) = self.resolution {
                        pill_badge(
                            ui,
                            &format!("控制器 {w}×{h}"),
                            Palette::TEXT_SECONDARY,
                            Palette::BG_CARD,
                            Palette::BORDER,
                        );
                    }
                    if let Some([pw, ph]) = self.preview_size {
                        pill_badge(
                            ui,
                            &format!("帧 {pw}×{ph}"),
                            Palette::TEXT_SECONDARY,
                            Palette::BG_CARD,
                            Palette::BORDER,
                        );
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if segment_button(ui, self.roi_tool.enabled, "ROI 采图框选").clicked() {
                            self.roi_tool.enabled = !self.roi_tool.enabled;
                        }
                    });
                });

                if self.roi_tool.enabled {
                    ui.add_space(6.0);
                    egui::Frame::new()
                        .fill(Palette::BG_CARD)
                        .stroke(Stroke::new(1.0, Palette::BORDER))
                        .corner_radius(CornerRadius::same(8))
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
                                                .color(Palette::TEXT_SECONDARY),
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
                    .fill(Palette::BG_CONSOLE)
                    .stroke(Stroke::new(1.0, Palette::BORDER))
                    .corner_radius(CornerRadius::same(12))
                    .inner_margin(Margin::same(12));

                canvas_frame.show(ui, |ui| {
                    ui.set_min_size(ui.available_size());
                    if let Some(tex) = &self.preview_texture {
                        let avail = ui.available_size();
                        let tex_size = tex.size_vec2();
                        let scale = (avail.x / tex_size.x)
                            .min(avail.y / tex_size.y)
                            .min(1.0)
                            .max(0.1);
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
                        let avail = ui.available_size();
                        let card_w = 400.0_f32.min(avail.x - 24.0).max(240.0);
                        let top_pad = ((avail.y - 190.0) * 0.5).max(16.0);
                        ui.add_space(top_pad);
                        ui.horizontal(|ui| {
                            let side_pad = ((avail.x - card_w) * 0.5).max(8.0);
                            ui.add_space(side_pad);
                            ui.allocate_ui(egui::vec2(card_w, 190.0), |ui| {
                                card_frame().show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.vertical_centered(|ui| {
                                        ui.label(
                                            egui::RichText::new("等待画面接入")
                                                .size(18.0)
                                                .strong()
                                                .color(Palette::TEXT_PRIMARY),
                                        );
                                        ui.add_space(8.0);
                                        ui.label(
                                            egui::RichText::new(
                                                "1. 在左侧选择游戏包（已预置《蓝色星原：旅谣》）\n2. 选择 Win32 端游窗口或 ADB 模拟器\n3. 点击「连接并抓取画面」即可运行任务或 ROI 采图",
                                            )
                                            .size(13.0)
                                            .color(Palette::TEXT_SECONDARY),
                                        );
                                        ui.add_space(14.0);
                                        if primary_button(ui, "重新扫描设备与窗口", false).clicked()
                                        {
                                            self.runtime.send(RuntimeCommand::ScanDevices);
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
